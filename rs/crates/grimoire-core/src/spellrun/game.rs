//! Spellrun: a little apprentice auto-running through an endless generated pixel world, played by the music
//! (port of `SpellrunGame.swift`). World units are pixels; y points up from the bottom of the world.

use std::collections::VecDeque;
use std::f64::consts::PI;

use super::art::BIOMES;
use super::canvas::{mix, rgb, SplitMix64};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyKind {
    Slime,
    Bat,
}

/// One 16 px wide slice of the world. Rows count up from the bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    /// Solid ground tiles from the bottom; 0 is a pit.
    pub ground: i64,
    /// Row of a one-way floating platform, or -1.
    pub platform: i64,
    /// Row of a solid rune block, or -1.
    pub block: i64,
    /// Row of a collectible mote, or -1.
    pub mote: i64,
    pub enemy: Option<EnemyKind>,
    pub enemy_row: i64,
    /// A beat lantern post that flashes with the bass.
    pub lantern: bool,
    /// 0 none, 1 grass tuft, 2 flower, 3 crystal or rock.
    pub decor: u8,
    pub biome: usize,
}

impl Column {
    pub fn new(ground: i64) -> Column {
        Column { ground, platform: -1, block: -1, mote: -1, enemy: None, enemy_row: 0, lantern: false, decor: 0, biome: 0 }
    }
}

/// Endless level generator. Every gap and step it produces is one the apprentice can always clear at minimum
/// speed. `energy` (0...1, driven by the mids) shifts it from long calm runs toward gaps, hills, platforms and blocks.
#[derive(Clone, Debug)]
pub struct LevelGenerator {
    pub energy: f64,
    pub biome: usize,
    rng: SplitMix64,
    height: i64,
    queue: VecDeque<Column>,
    since_lantern: i64,
}

impl LevelGenerator {
    pub const MAX_GAP: i64 = 3;
    pub const MAX_STEP_UP: i64 = 2;
    pub const MIN_HEIGHT: i64 = 2;
    pub const MAX_HEIGHT: i64 = 6;

    pub fn new(seed: u64) -> LevelGenerator {
        let mut generator = LevelGenerator {
            energy: 0.3,
            biome: 0,
            rng: SplitMix64::new(seed),
            height: 3,
            queue: VecDeque::new(),
            since_lantern: 0,
        };
        generator.emit_flat(10); // a safe runway to start on
        generator
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Column {
        while self.queue.is_empty() {
            self.generate_segment();
        }
        self.queue.pop_front().unwrap()
    }

    fn generate_segment(&mut self) {
        let e = self.energy;
        let weights = [3.0 - 2.0 * e, 1.5 + 0.5 * e, 0.8 + 1.6 * e, 0.8 + 1.0 * e, 0.8 + 0.7 * e];
        let mut roll = self.rng.unit() * weights.iter().sum::<f64>();
        let mut choice = 0;
        while choice < weights.len() - 1 && roll >= weights[choice] {
            roll -= weights[choice];
            choice += 1;
        }
        match choice {
            0 => self.flat_run(),
            1 => self.hill(),
            2 => self.gap(),
            3 => self.platforms(),
            _ => self.blocks(),
        }
        if choice != 2 && self.rng.chance(0.10 + 0.2 * e) {
            self.add_bat();
        }
    }

    fn emit_flat(&mut self, count: i64) -> std::ops::Range<usize> {
        let start = self.queue.len();
        for _ in 0..count {
            let mut column = Column::new(self.height);
            column.biome = self.biome;
            self.since_lantern += 1;
            if self.since_lantern >= 11 && self.rng.chance(0.4) {
                column.lantern = true;
                self.since_lantern = 0;
            } else if self.rng.chance(0.35) {
                column.decor = self.rng.int(1, 3) as u8;
            }
            self.queue.push_back(column);
        }
        start..self.queue.len()
    }

    fn flat_run(&mut self) {
        let length = self.rng.int(4, 9);
        let run = self.emit_flat(length);
        if run.len() >= 5 && self.rng.chance(0.25 + 0.3 * self.energy) {
            self.queue[run.start + run.len() / 2].enemy = Some(EnemyKind::Slime);
        }
        if self.rng.chance(0.4) {
            let start = run.start + 1;
            for (offset, lift) in [1, 2, 2, 1].into_iter().enumerate() {
                if start + offset < run.end {
                    self.queue[start + offset].mote = self.height + lift;
                }
            }
        }
    }

    fn hill(&mut self) {
        let up = self.height <= Self::MIN_HEIGHT || (self.height < Self::MAX_HEIGHT && self.rng.chance(0.5));
        let step = if up {
            self.rng.int(1, if self.energy > 0.5 { Self::MAX_STEP_UP } else { 1 })
        } else {
            self.rng.int(1, 2)
        };
        self.height = (self.height + if up { step } else { -step }).clamp(Self::MIN_HEIGHT, Self::MAX_HEIGHT);
        let length = self.rng.int(3, 6);
        self.emit_flat(length);
    }

    fn gap(&mut self) {
        let width = self.rng.int(1, if self.energy > 0.55 { Self::MAX_GAP } else { 2 });
        let start = self.queue.len();
        for _ in 0..width {
            let mut pit = Column::new(0);
            pit.biome = self.biome;
            self.queue.push_back(pit);
        }
        // A reward arcing over the gap.
        if self.rng.chance(0.5) {
            self.queue[start + width as usize / 2].mote = self.height + 3;
        }
        // A wide gap only lands level or lower; a narrow one may land one step up.
        let change = if width >= Self::MAX_GAP { self.rng.int(-1, 0) } else { self.rng.int(-1, 1) };
        self.height = (self.height + change).clamp(Self::MIN_HEIGHT, Self::MAX_HEIGHT);
        let length = self.rng.int(3, 5);
        self.emit_flat(length);
    }

    fn platforms(&mut self) {
        let count = self.rng.int(7, 10);
        let run = self.emit_flat(count);
        let length = self.rng.int(3, 5.min(run.len() as i64 - 4)) as usize;
        let first = run.start + 2;
        for index in first..first + length {
            self.queue[index].platform = self.height + 2;
            if (index - first) % 2 == 0 {
                self.queue[index].mote = self.height + 3;
            }
        }
    }

    fn blocks(&mut self) {
        let length = self.rng.int(6, 9);
        let run = self.emit_flat(length);
        let count = self.rng.int(1, 3) as usize;
        for i in 0..count {
            let index = run.start + 2 + i * 2;
            if index >= run.end - 1 {
                break;
            }
            self.queue[index].block = self.height + 3;
            self.queue[index].mote = self.height + 4;
        }
    }

    fn add_bat(&mut self) {
        let Some(index) = (0..self.queue.len())
            .rev()
            .find(|&i| self.queue[i].ground > 0 && self.queue[i].platform < 0 && self.queue[i].block < 0)
        else {
            return;
        };
        self.queue[index].enemy = Some(EnemyKind::Bat);
        self.queue[index].enemy_row = self.queue[index].ground + 3;
    }
}

/// What the music does to the game this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpellrunInput {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    /// A kick just landed (rising edge of the low-band hit).
    pub kick: bool,
    /// A kick strong enough for a ground pound.
    pub big_kick: bool,
    /// A high-band transient: cast a spell, drop a mote.
    pub sparkle: bool,
    /// It just got loud after a quiet stretch.
    pub drop: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Hero {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub grounded: bool,
    pub pounding: bool,
    pub blink: f64,
    pub stride: f64,
    /// The staff boost (a magic double jump) recharges on landing or after a short cooldown.
    pub can_boost: bool,
    boost_cooldown: f64,
    pub boost_glow: f64,
    cast_cooldown: f64,
    stuck: f64,
}

impl Default for Hero {
    fn default() -> Self {
        Hero {
            x: 48.0,
            y: 48.0,
            vx: SpellrunGame::MIN_RUN_SPEED,
            vy: 0.0,
            grounded: true,
            pounding: false,
            blink: 0.0,
            stride: 0.0,
            can_boost: true,
            boost_cooldown: 0.0,
            boost_glow: 0.0,
            cast_cooldown: 0.0,
            stuck: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub x: f64,
    pub y: f64,
    base_y: f64,
    vy: f64,
    pub phase: f64,
    /// > 0 while showing the squished frame.
    squish: f64,
}

impl Enemy {
    pub fn dead(&self) -> bool {
        self.squish > 0.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Mote {
    pub x: f64,
    pub y: f64,
    pub phase: f64,
    rising: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Bolt {
    pub x: f64,
    pub y: f64,
    life: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    vx: f64,
    vy: f64,
    pub life: f64,
    pub max_life: f64,
    pub color: u32,
    pub size: i64,
    gravity: f64,
}

/// Running totals of what the music made happen; handy for tests and tuning.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub beat_hops: u32,
    pub pounds: u32,
    pub boosts: u32,
    pub spells: u32,
    pub stomps: u32,
    pub blocks_shattered: u32,
}

pub struct SpellrunGame {
    pub hero: Hero,
    pub camera_x: f64,
    pub camera_y: f64,
    pub motes_collected: u32,
    pub respawns: u32,
    pub time: f64,
    pub biome_index: usize,
    pub previous_biome: usize,
    /// 0...1 dissolve from the previous biome's backdrop to the current one.
    pub biome_fade: f64,
    /// Seconds left showing the biome name banner.
    pub banner_time: f64,
    pub shake: f64,
    pub input: SpellrunInput,
    pub stats: Stats,

    columns: VecDeque<Column>,
    first_column: i64,
    pub enemies: Vec<Enemy>,
    pub motes: Vec<Mote>,
    pub bolts: Vec<Bolt>,
    pub particles: Vec<Particle>,
    particle_cursor: usize,
    generator: LevelGenerator,
    rng: SplitMix64,
    biome_timer: f64,
    last_biome_change: f64,
    view_width: f64,
    view_height: f64,
}

impl SpellrunGame {
    pub const TILE: i64 = 16;
    const GRAVITY: f64 = 1000.0;
    const JUMP_SPEED: f64 = 340.0;
    const HOP_SPEED: f64 = 230.0;
    const POUND_SPEED: f64 = -540.0;
    pub const MIN_RUN_SPEED: f64 = 95.0;
    const HERO_WIDTH: f64 = 10.0;
    const HERO_HEIGHT: f64 = 14.0;

    pub fn new(seed: u64) -> SpellrunGame {
        let start_biome = (seed % BIOMES.len() as u64) as usize;
        let mut generator = LevelGenerator::new(seed);
        generator.biome = start_biome;
        let mut game = SpellrunGame {
            hero: Hero::default(),
            camera_x: 0.0,
            camera_y: 0.0,
            motes_collected: 0,
            respawns: 0,
            time: 0.0,
            biome_index: start_biome,
            previous_biome: start_biome,
            biome_fade: 1.0,
            banner_time: 3.0,
            shake: 0.0,
            input: SpellrunInput::default(),
            stats: Stats::default(),
            columns: VecDeque::new(),
            first_column: 0,
            enemies: Vec::new(),
            motes: Vec::new(),
            bolts: Vec::new(),
            particles: vec![Particle::default(); 320],
            particle_cursor: 0,
            generator,
            rng: SplitMix64::new(seed.wrapping_add(1)),
            biome_timer: 55.0,
            last_biome_change: 0.0,
            view_width: 256.0,
            view_height: 160.0,
        };
        game.ensure_columns();
        game
    }

    pub fn biome(&self) -> &'static super::art::Biome {
        &BIOMES[self.biome_index]
    }

    pub fn column(&self, index: i64) -> Option<&Column> {
        let local = index - self.first_column;
        if local >= 0 {
            self.columns.get(local as usize)
        } else {
            None
        }
    }

    // Step

    pub fn step(&mut self, dt: f64, input: SpellrunInput, view_width: f64, view_height: f64) {
        if dt <= 0.0 {
            return;
        }
        self.input = input;
        self.view_width = view_width;
        self.view_height = view_height;
        self.time += dt;

        self.generator.energy += (input.mid as f64 - self.generator.energy) * (1.0 - (-dt / 3.0).exp());
        self.biome_timer -= dt;
        if self.biome_timer <= 0.0 || (input.drop && self.time - self.last_biome_change > 15.0) {
            self.generator.biome = (self.generator.biome + 1) % BIOMES.len();
            self.biome_timer = 50.0 + self.rng.unit() * 30.0;
            self.last_biome_change = self.time;
        }

        let target_speed = Self::MIN_RUN_SPEED + 80.0 * input.mid as f64;
        self.hero.vx += (target_speed - self.hero.vx) * (1.0 - (-dt / 0.8).exp());
        self.update_hero(dt);
        self.update_enemies(dt);
        self.update_bolts(dt);
        self.update_motes(dt);
        self.update_particles(dt);

        if input.sparkle && self.rng.chance(0.6) {
            let x = self.hero.x + view_width * (0.5 + 0.3 * self.rng.unit());
            let ground = (self.column(x as i64 / Self::TILE).map_or(3, |c| c.ground) * Self::TILE) as f64;
            let y = ground + Self::TILE as f64 * (2.0 + 2.0 * self.rng.unit());
            let phase = self.rng.unit() * 6.0;
            self.motes.push(Mote { x, y, phase, rising: 0.0 });
        }

        let target_x = self.hero.x - view_width * 0.3;
        self.camera_x += (target_x - self.camera_x) * (1.0 - (-dt / 0.12).exp());
        let target_y = (self.hero.y - view_height * 0.35).max(0.0);
        self.camera_y += (target_y - self.camera_y) * (1.0 - (-dt / 0.45).exp());

        let center = ((self.camera_x + view_width / 2.0) / Self::TILE as f64) as i64;
        if let Some(biome_here) = self.column(center).map(|c| c.biome) {
            if biome_here != self.biome_index {
                self.previous_biome = self.biome_index;
                self.biome_index = biome_here;
                self.biome_fade = 0.0;
                self.banner_time = 3.0;
            }
        }
        self.biome_fade = (self.biome_fade + dt / 1.5).min(1.0);
        self.banner_time = (self.banner_time - dt).max(0.0);
        self.shake = (self.shake - dt * 3.0).max(0.0);

        self.ensure_columns();
        while self.first_column < (self.camera_x / Self::TILE as f64) as i64 - 4 && !self.columns.is_empty() {
            self.columns.pop_front();
            self.first_column += 1;
        }
        let camera_x = self.camera_x;
        self.enemies.retain(|e| !(e.x < camera_x - 40.0 || e.y < -60.0));
        self.motes.retain(|m| m.x >= camera_x - 20.0);
    }

    fn ensure_columns(&mut self) {
        let needed = ((self.camera_x + self.view_width) / Self::TILE as f64) as i64 + 10;
        while self.first_column + (self.columns.len() as i64) < needed {
            let index = self.first_column + self.columns.len() as i64;
            let column = self.generator.next();
            self.columns.push_back(column);
            let x = (index * Self::TILE) as f64;
            if column.mote >= 0 {
                self.motes.push(Mote { x: x + 5.5, y: (column.mote * Self::TILE) as f64 + 5.0, phase: index as f64, rising: 0.0 });
            }
            let enemy_y = match column.enemy {
                Some(EnemyKind::Slime) => Some((column.ground * Self::TILE) as f64),
                Some(EnemyKind::Bat) => Some((column.enemy_row * Self::TILE) as f64),
                None => None,
            };
            if let (Some(kind), Some(y)) = (column.enemy, enemy_y) {
                self.enemies.push(Enemy { kind, x: x + 2.0, y, base_y: y, vy: 0.0, phase: index as f64, squish: 0.0 });
            }
        }
    }

    // World queries

    fn is_solid(&self, col: i64, row: i64) -> bool {
        match self.column(col) {
            None => row < 3,
            Some(c) => row < c.ground || row == c.block,
        }
    }

    fn column_range(x: f64) -> std::ops::RangeInclusive<i64> {
        let tile = Self::TILE as f64;
        ((x + 1.0) / tile).floor() as i64..=((x + Self::HERO_WIDTH - 1.0) / tile).floor() as i64
    }

    /// The highest surface the hero's feet cross while falling from `from` to `to`, if any.
    fn landing(&self, x: f64, from: f64, to: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for col in Self::column_range(x) {
            let Some(c) = self.column(col) else { continue };
            let mut tops = vec![(c.ground * Self::TILE) as f64];
            if c.platform >= 0 {
                tops.push(((c.platform + 1) * Self::TILE) as f64);
            }
            if c.block >= 0 {
                tops.push(((c.block + 1) * Self::TILE) as f64);
            }
            for top in tops {
                if (c.ground > 0 || top > 0.0) && from >= top - 0.5 && to <= top {
                    best = Some(best.unwrap_or(-1.0).max(top));
                }
            }
        }
        best
    }

    /// The column and row of a rune block the hero's head hits while rising, if any.
    fn ceiling(&self, x: f64, from: f64, to: f64) -> Option<(i64, f64)> {
        for col in Self::column_range(x) {
            let Some(c) = self.column(col).filter(|c| c.block >= 0) else { continue };
            let bottom = (c.block * Self::TILE) as f64;
            if from <= bottom + 0.5 && to > bottom {
                return Some((col, bottom));
            }
        }
        None
    }

    fn surface(&self, x: f64) -> f64 {
        (self.column((x / Self::TILE as f64).floor() as i64).map_or(0, |c| c.ground) * Self::TILE) as f64
    }

    // Hero

    fn update_hero(&mut self, dt: f64) {
        let mut h = self.hero;
        let tile = Self::TILE as f64;

        if h.grounded {
            let front = h.x + Self::HERO_WIDTH;
            let next_col = (front / tile).floor() as i64 + 1;
            let to_edge = next_col as f64 * tile - front;
            let next = self.column(next_col).copied();
            let slime_ahead = self.enemies.iter().any(|e| {
                e.kind == EnemyKind::Slime && !e.dead() && e.x > h.x && e.x - front < 30.0 && (e.y - h.y).abs() < 8.0
            });
            // Landing right at a gap's edge can leave the front foot already over the pit, so check that column too.
            let under_front = self.column(((front - 0.01) / tile).floor() as i64).copied();
            let previous_platform = self.column(next_col - 1).map_or(-1, |c| c.platform);
            let jump = if under_front.is_some_and(|c| c.ground == 0) {
                Some(Self::JUMP_SPEED)
            } else if next.is_some_and(|n| n.ground == 0 && to_edge < 8.0) {
                Some(Self::JUMP_SPEED)
            } else if next.is_some_and(|n| n.ground as f64 * tile > h.y + 1.0 && to_edge < 10.0) {
                Some(Self::JUMP_SPEED)
            } else if slime_ahead {
                Some(Self::JUMP_SPEED)
            } else if next.is_some_and(|n| {
                n.platform >= 0 && previous_platform < 0 && to_edge < 6.0 && n.platform as f64 * tile > h.y
            }) {
                Some(Self::JUMP_SPEED)
            } else if self.input.kick && self.safe_to_hop(next_col - 1, h.y) {
                self.stats.beat_hops += 1;
                Some(Self::HOP_SPEED)
            } else if h.stuck > 0.4 {
                Some(Self::JUMP_SPEED)
            } else {
                None
            };
            if let Some(jump) = jump {
                h.vy = jump;
                h.grounded = false;
                h.stuck = 0.0;
                self.dust(h.x + Self::HERO_WIDTH / 2.0, h.y, 4);
            }
        } else {
            let center = h.x + Self::HERO_WIDTH / 2.0;
            let over_pit = Self::column_range(h.x).all(|col| self.column(col).map_or(1, |c| c.ground) == 0);
            if over_pit && h.vy < 0.0 && h.can_boost {
                // About to drop below the far ledge? Staff boost: a magic double jump out of the gap.
                let col = (center / tile).floor() as i64;
                let ledge = (col..=col + 3).filter_map(|c| self.column(c).map(|c| c.ground)).max().unwrap_or(0);
                if h.y < ledge as f64 * tile + 6.0 {
                    h.vy = Self::JUMP_SPEED * 0.9;
                    h.can_boost = false;
                    h.boost_cooldown = 0.45;
                    h.pounding = false;
                    h.boost_glow = 1.0;
                    self.stats.boosts += 1;
                    self.burst(center, h.y, rgb(0x74FFAD), 14, 70.0);
                }
            } else if self.input.big_kick && !h.pounding && !over_pit && h.vy < 80.0 && h.y - self.surface(center) > 24.0 {
                h.vy = Self::POUND_SPEED;
                h.pounding = true;
                self.stats.pounds += 1;
            }
        }
        h.boost_glow = (h.boost_glow - dt * 2.0).max(0.0);
        h.boost_cooldown -= dt;
        if !h.can_boost && h.boost_cooldown <= 0.0 {
            h.can_boost = true;
        }

        h.cast_cooldown -= dt;
        if self.input.sparkle && h.cast_cooldown <= 0.0 {
            self.bolts.push(Bolt { x: h.x + 11.0, y: h.y + 14.0, life: 0.9 });
            h.cast_cooldown = 0.3;
            self.stats.spells += 1;
            self.burst(h.x + 11.0, h.y + 14.0, rgb(0x9FFFD0), 5, 40.0);
        }

        // Horizontal: run, stopping at walls.
        let mut x = h.x + h.vx * dt;
        let front_col = ((x + Self::HERO_WIDTH) / tile).floor() as i64;
        let low_row = ((h.y + 1.0) / tile).floor() as i64;
        let high_row = ((h.y + Self::HERO_HEIGHT - 1.0) / tile).floor() as i64;
        if (low_row..=low_row.max(high_row)).any(|row| self.is_solid(front_col, row)) {
            x = front_col as f64 * tile - Self::HERO_WIDTH - 0.01;
            h.stuck += dt;
        } else {
            h.stuck = 0.0;
        }
        h.stride += x - h.x;
        h.x = x;

        // Vertical: gravity, landing, head bonks.
        h.vy = (h.vy - Self::GRAVITY * dt).max(-720.0);
        let previous_y = h.y;
        let mut y = h.y + h.vy * dt;
        let was_grounded = h.grounded;
        h.grounded = false;
        let landing = if h.vy <= 0.0 { self.landing(h.x, previous_y, y) } else { None };
        if let Some(top) = landing {
            y = top;
            if h.pounding {
                self.ground_pound(h.x + Self::HERO_WIDTH / 2.0, top);
                h.pounding = false;
            } else if !was_grounded && h.vy < -300.0 {
                self.dust(h.x + Self::HERO_WIDTH / 2.0, top, 3);
            }
            h.vy = 0.0;
            h.grounded = true;
            h.can_boost = true;
        } else if h.vy > 0.0 {
            if let Some((col, bottom)) = self.ceiling(h.x, previous_y + Self::HERO_HEIGHT, y + Self::HERO_HEIGHT) {
                y = bottom - Self::HERO_HEIGHT;
                h.vy = 0.0;
                self.shatter_block(col);
            }
        }
        h.y = y;

        // Stomping and bumping into enemies.
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            if e.dead() {
                continue;
            }
            let (width, height) = (12.0, if e.kind == EnemyKind::Slime { 9.0 } else { 8.0 });
            let overlaps = h.x < e.x + width && h.x + Self::HERO_WIDTH > e.x && h.y < e.y + height && h.y + Self::HERO_HEIGHT > e.y;
            if !overlaps {
                continue;
            }
            if h.vy < 0.0 && h.y > e.y + height * 0.4 {
                self.enemies[i].squish = 0.35;
                self.stats.stomps += 1;
                h.vy = 260.0;
                h.grounded = false;
                let color = if e.kind == EnemyKind::Slime { rgb(0x46D3A0) } else { rgb(0x9E7CC8) };
                self.burst(e.x + 6.0, e.y + 4.0, color, 8, 70.0);
            } else if h.blink <= 0.0 {
                self.enemies[i].squish = 0.35;
                h.blink = 0.6;
                self.burst(e.x + 6.0, e.y + 4.0, rgb(0xFFFFFF), 6, 50.0);
            }
        }
        h.blink = (h.blink - dt).max(0.0);

        // Safety net: if the hero ever falls into a pit, pop them back onto the next solid ground.
        if h.y < -40.0 {
            let mut col = (h.x / tile).floor() as i64 + 1;
            while self.column(col).map_or(1, |c| c.ground) == 0 {
                col += 1;
            }
            h.x = col as f64 * tile + 2.0;
            h.y = (self.column(col).map_or(3, |c| c.ground) * Self::TILE) as f64;
            h.vy = 0.0;
            h.grounded = true;
            h.pounding = false;
            self.respawns += 1;
            self.burst(h.x + 5.0, h.y + 7.0, rgb(0xE6FFF0), 12, 60.0);
        }
        self.hero = h;
    }

    fn safe_to_hop(&self, col: i64, y: f64) -> bool {
        let level = (y / Self::TILE as f64).round() as i64;
        (0..=4).all(|offset| self.column(col + offset).is_some_and(|c| c.ground == level))
    }

    fn ground_pound(&mut self, x: f64, y: f64) {
        self.shake = 1.0;
        self.dust(x, y, 14);
        for i in 0..self.enemies.len() {
            if !self.enemies[i].dead() && (self.enemies[i].x - x).abs() < 110.0 {
                self.enemies[i].squish = 0.35;
                let (ex, ey) = (self.enemies[i].x, self.enemies[i].y);
                self.burst(ex + 6.0, ey + 4.0, rgb(0xFFFFFF), 5, 60.0);
            }
        }
    }

    fn shatter_block(&mut self, col: i64) {
        let local = col - self.first_column;
        if local < 0 || local as usize >= self.columns.len() || self.columns[local as usize].block < 0 {
            return;
        }
        let column = &mut self.columns[local as usize];
        let row = column.block;
        column.block = -1;
        let glow = BIOMES[column.biome].block_glow;
        self.stats.blocks_shattered += 1;
        let (x, y) = ((col * Self::TILE) as f64 + 8.0, (row * Self::TILE) as f64 + 8.0);
        self.burst(x, y, glow, 16, 90.0);
        self.motes.push(Mote { x: x - 2.5, y, phase: 0.0, rising: 0.4 });
    }

    // Everything else

    fn update_enemies(&mut self, dt: f64) {
        for i in 0..self.enemies.len() {
            if self.enemies[i].dead() {
                self.enemies[i].squish -= dt;
                if self.enemies[i].squish <= 0.0 {
                    self.enemies[i].y = -100.0;
                }
                continue;
            }
            match self.enemies[i].kind {
                EnemyKind::Slime => {
                    self.enemies[i].x -= 12.0 * dt;
                    let ground = self.surface(self.enemies[i].x + 6.0);
                    let e = &mut self.enemies[i];
                    if self.input.kick && e.y <= ground + 0.5 {
                        e.vy = 120.0; // slimes dance too
                    }
                    e.vy -= Self::GRAVITY * dt;
                    e.y += e.vy * dt;
                    // Over a pit there's no ground, so the slime just falls out of the world.
                    if ground > 0.0 && e.y < ground {
                        e.y = ground;
                        e.vy = 0.0;
                    }
                }
                EnemyKind::Bat => {
                    let e = &mut self.enemies[i];
                    e.x -= 22.0 * dt;
                    e.y = e.base_y + (self.time * 3.0 + e.phase).sin() * 6.0;
                }
            }
        }
        self.enemies.retain(|e| e.y > -100.0);
    }

    fn update_bolts(&mut self, dt: f64) {
        for i in 0..self.bolts.len() {
            self.bolts[i].x += 280.0 * dt;
            self.bolts[i].life -= dt;
            let (bx, by) = (self.bolts[i].x, self.bolts[i].y);
            self.spawn_particle(Particle {
                x: bx,
                y: by,
                vx: -20.0,
                vy: fastrand::f64() * 20.0 - 10.0,
                life: 0.25,
                max_life: 0.25,
                color: rgb(0x74FFAD),
                size: 1,
                gravity: 0.0,
            });
            let col = (bx / Self::TILE as f64).floor() as i64;
            if self.column(col).is_some_and(|c| c.block >= 0 && (by / Self::TILE as f64).floor() as i64 == c.block) {
                self.shatter_block(col);
                self.bolts[i].life = 0.0;
            }
            for j in 0..self.enemies.len() {
                let e = self.enemies[j];
                if !e.dead() && bx > e.x && bx < e.x + 12.0 && by > e.y - 2.0 && by < e.y + 10.0 {
                    self.enemies[j].squish = 0.35;
                    self.bolts[i].life = 0.0;
                    self.burst(e.x + 6.0, e.y + 4.0, rgb(0xFFD84D), 10, 80.0);
                    self.motes.push(Mote { x: e.x + 4.0, y: e.y + 4.0, phase: 0.0, rising: 0.3 });
                }
            }
        }
        self.bolts.retain(|b| b.life > 0.0);
    }

    fn update_motes(&mut self, dt: f64) {
        let h = self.hero;
        let time = self.time;
        let mut collected = Vec::new();
        self.motes.retain(|mote| {
            let bob = (time * 3.0 + mote.phase).sin() * 2.0;
            let hit = mote.x + 5.0 > h.x - 2.0
                && mote.x < h.x + Self::HERO_WIDTH + 2.0
                && mote.y + bob + 5.0 > h.y - 2.0
                && mote.y + bob < h.y + Self::HERO_HEIGHT + 2.0;
            if hit {
                collected.push((mote.x, mote.y));
            }
            !hit
        });
        for (x, y) in collected {
            self.motes_collected += 1;
            self.burst(x + 2.5, y + 2.5, rgb(0xFFD84D), 6, 45.0);
        }
        for mote in self.motes.iter_mut().filter(|m| m.rising > 0.0) {
            mote.y += 40.0 * dt;
            mote.rising -= dt;
        }
    }

    fn update_particles(&mut self, dt: f64) {
        for p in self.particles.iter_mut().filter(|p| p.life > 0.0) {
            p.life -= dt;
            p.vy -= p.gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
    }

    /// Reuses the particle pool round-robin, overwriting the oldest when it's full.
    fn spawn_particle(&mut self, particle: Particle) {
        self.particles[self.particle_cursor] = particle;
        self.particle_cursor = (self.particle_cursor + 1) % self.particles.len();
    }

    fn burst(&mut self, x: f64, y: f64, color: u32, count: usize, speed: f64) {
        for _ in 0..count {
            let angle = self.rng.unit() * 2.0 * PI;
            let v = speed * (0.4 + 0.6 * self.rng.unit());
            let size = if self.rng.chance(0.3) { 2 } else { 1 };
            self.spawn_particle(Particle {
                x,
                y,
                vx: angle.cos() * v,
                vy: angle.sin() * v,
                life: 0.5,
                max_life: 0.5,
                color,
                size,
                gravity: 120.0,
            });
        }
    }

    fn dust(&mut self, x: f64, y: f64, count: usize) {
        let color = mix(self.biome().ground_top, rgb(0xFFFFFF), 0.4);
        for _ in 0..count {
            let direction = if self.rng.chance(0.5) { -1.0 } else { 1.0 };
            let vx = direction * (20.0 + 50.0 * self.rng.unit());
            let vy = 10.0 + 30.0 * self.rng.unit();
            self.spawn_particle(Particle { x, y: y + 1.0, vx, vy, life: 0.35, max_life: 0.35, color, size: 1, gravity: 60.0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every gap and step must be clearable: gaps ≤ 3 wide, steps up ≤ 2, wide gaps never land higher.
    #[test]
    fn generated_terrain_is_always_passable() {
        for seed in [1u64, 42, 9_001] {
            for energy in [0.0, 0.5, 1.0] {
                let mut generator = LevelGenerator::new(seed);
                generator.energy = energy;
                let columns: Vec<Column> = (0..3_000).map(|_| generator.next()).collect();
                let mut last_ground = columns[0].ground;
                let mut gap = 0;
                for column in columns {
                    if column.ground == 0 {
                        gap += 1;
                        assert!(gap <= LevelGenerator::MAX_GAP, "seed {seed} energy {energy}");
                        continue;
                    }
                    let rise = column.ground - last_ground;
                    assert!(rise <= if gap >= LevelGenerator::MAX_GAP { 0 } else { LevelGenerator::MAX_STEP_UP });
                    assert!((LevelGenerator::MIN_HEIGHT..=LevelGenerator::MAX_HEIGHT).contains(&column.ground));
                    last_ground = column.ground;
                    gap = 0;
                }
            }
        }
    }

    #[test]
    fn same_seed_same_level() {
        let (mut a, mut b) = (LevelGenerator::new(7), LevelGenerator::new(7));
        for _ in 0..500 {
            assert_eq!(a.next(), b.next());
        }
    }

    #[test]
    fn busy_music_makes_busier_terrain() {
        let features = |energy: f64| {
            let mut generator = LevelGenerator::new(3);
            generator.energy = energy;
            (0..2_000)
                .map(|_| generator.next())
                .filter(|c| c.ground == 0 || c.platform >= 0 || c.block >= 0)
                .count()
        };
        assert!(features(1.0) > features(0.0));
    }

    /// Two minutes of music-like input: the apprentice keeps moving forward and never falls into a pit.
    #[test]
    fn apprentice_survives_and_makes_progress() {
        for seed in 1u64..=5 {
            let mut game = SpellrunGame::new(seed);
            for frame in 0..60 * 120 {
                let t = frame as f64 / 60.0;
                let beat = frame % 30 == 0;
                let input = SpellrunInput {
                    low: (0.5 + 0.5 * (t * 2.0).sin()) as f32,
                    mid: (0.5 + 0.5 * (t * 0.3).sin()) as f32,
                    high: (0.5 + 0.5 * (t * 1.7).sin()) as f32,
                    kick: beat,
                    big_kick: beat && frame % 120 == 0,
                    sparkle: frame % 45 == 0,
                    drop: frame == 60 * 40,
                };
                game.step(1.0 / 60.0, input, 256.0, 160.0);
            }
            assert_eq!(game.respawns, 0, "seed {seed}: the apprentice fell into a pit");
            assert!(game.hero.x > 95.0 * 100.0, "seed {seed}: should cover ground at run speed");
            assert!(game.hero.y.is_finite());
            assert!(game.motes_collected > 0);
            let s = game.stats;
            assert!(s.beat_hops + s.pounds > 0 && s.spells > 0, "seed {seed}: the music should drive the game: {s:?}");
        }
    }
}
