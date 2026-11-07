//! Draws a `SpellrunGame` into a `PixelCanvas` (port of `SpellrunRenderer.swift`): dithered sky, sun or moon,
//! stars and clouds, two parallax backdrop layers, tiles, entities, particles and a tiny HUD.

use super::art::{Backdrop, Biome, Weather, ART, BIOMES, OUTLINE, RUNES};
use super::canvas::{bayer, mix, rgb, PixelCanvas};
use super::game::{EnemyKind, SpellrunGame};
use crate::BandLevels;

#[derive(Default)]
pub struct SpellrunRenderer {
    width: i64,
    height: i64,
    camera_x: i64,
    camera_y: i64,
}

impl SpellrunRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&mut self, game: &SpellrunGame, canvas: &mut PixelCanvas, levels: BandLevels, show_meters: bool) {
        self.width = canvas.width;
        self.height = canvas.height;
        let time = game.time;
        let shake_x = ((time * 83.0).sin() * game.shake * 2.5).round() as i64;
        let shake_y = ((time * 71.0).cos() * game.shake * 2.0).round() as i64;
        self.camera_x = game.camera_x.floor() as i64 - shake_x;
        self.camera_y = game.camera_y.floor() as i64 + shake_y;

        self.draw_backdrop(game, canvas, levels);
        self.draw_tiles(game, canvas, levels.low);
        self.draw_entities(game, canvas, levels.high);
        self.draw_weather(game, canvas, levels);
        self.draw_hud(game, canvas, levels, show_meters);
    }

    // Coordinates

    fn screen_x(&self, world_x: f64) -> i64 {
        world_x.floor() as i64 - self.camera_x
    }

    /// Canvas row of the top of something `h` px tall whose bottom sits at `world_y`.
    fn screen_top(&self, world_y: f64, h: i64) -> i64 {
        self.height - (world_y.floor() as i64 - self.camera_y) - h
    }

    // Backdrop

    fn draw_backdrop(&self, game: &SpellrunGame, canvas: &mut PixelCanvas, levels: BandLevels) {
        let high = levels.high;
        let current = &BIOMES[game.biome_index];
        let previous = &BIOMES[game.previous_biome];
        let fade = game.biome_fade as f32;
        let twinkle = game.time as f32;
        let (width, height) = (self.width, self.height);

        for x in 0..width {
            let far_u = self.camera_x as f64 * 0.2 + x as f64;
            let near_u = self.camera_x as f64 * 0.5 + x as f64;
            let (far_now, near_now) = (self.far_height(current, far_u), self.near_height(current, near_u));
            let far_before = if fade < 1.0 { self.far_height(previous, far_u) } else { far_now };
            let near_before = if fade < 1.0 { self.near_height(previous, near_u) } else { near_now };
            for y in 0..height {
                // During a biome change, each pixel picks old or new by an ordered-dither threshold: a pixel dissolve.
                let use_previous = fade < 1.0 && bayer(x, y) > fade;
                let b = if use_previous { previous } else { current };
                let far = if use_previous { far_before } else { far_now };
                let near = if use_previous { near_before } else { near_now };
                let from_bottom = height - y;
                let prop_u = self.camera_x as f64 * 0.35 + x as f64;
                let color = if from_bottom < near {
                    if from_bottom < near - 3 && bayer(x, y) < 0.5 { b.near_shade } else { b.near }
                } else if let Some(prop) = self.prop(b, prop_u, x, from_bottom, game.time, levels.low) {
                    prop
                } else if from_bottom < far {
                    if b.backdrop == Backdrop::Library && (far_u as i64).rem_euclid(42) < 34 && (from_bottom % 13) > 1 {
                        // Book spines on the shelves.
                        mix(b.far, b.accent, 0.04 + 0.16 * hash((far_u / 3.0).floor() as i64, from_bottom / 13) as f32)
                    } else {
                        b.far
                    }
                } else {
                    let t = y as f32 / height as f32;
                    let band = (t * 7.0 + bayer(x, y) - 0.5).floor();
                    let mut sky = mix(b.sky_top, b.sky_bottom, band.clamp(0.0, 7.0) / 7.0);
                    if b.has_stars && y < height * 2 / 3 {
                        let seed = hash((self.camera_x as f64 * 0.05).floor() as i64 + x, y);
                        if seed > 0.993 {
                            let sparkle = 0.45 + 0.55 * (twinkle * (2.0 + (seed * 9.0) as f32) + (seed * 40.0) as f32).sin();
                            sky = mix(sky, rgb(0xFFFFFF), (sparkle * (0.5 + high)).max(0.2));
                        }
                    }
                    sky
                };
                canvas.set(x, y, color);
            }
        }

        if let Some(orb) = current.orb {
            self.draw_orb(canvas, orb, current.has_stars, levels.low);
        }
        if current.has_clouds {
            self.draw_clouds(canvas, current);
        }
    }

    fn far_height(&self, biome: &Biome, u: f64) -> i64 {
        let base = self.height as f64 * 0.42;
        match biome.backdrop {
            Backdrop::Mountains => (base + 26.0 * noise(u * 0.011) + 12.0 * noise(u * 0.035) - 8.0) as i64,
            Backdrop::Pines => {
                let slot = (u / 9.0).floor();
                let local = u - slot * 9.0;
                let tree = 14.0 + 16.0 * hash(slot as i64, 7);
                (base - 6.0 + 8.0 * noise(u * 0.02) + tree * (1.0 - (local - 4.5).abs() / 4.5)) as i64
            }
            Backdrop::Crystals => {
                let slot = (u / 13.0).floor();
                let local = u - slot * 13.0;
                let shard = 10.0 + 40.0 * hash(slot as i64, 3);
                let profile = (1.0 - (local - 6.5).abs() / 3.5).max(0.0);
                (base - 14.0 + 6.0 * noise(u * 0.03) + shard * profile) as i64
            }
            Backdrop::Castle => {
                let slot = (u / 36.0).floor();
                let local = u - slot * 36.0;
                let is_tower = hash(slot as i64, 11) > 0.45 && local > 8.0 && local < 26.0;
                let tower = if is_tower { 18.0 + 26.0 * hash(slot as i64, 5) } else { 0.0 };
                let battlement = if is_tower && (local as i64) % 4 < 2 { 3.0 } else { 0.0 };
                (base - 10.0 + tower + battlement) as i64
            }
            // The mushrooms themselves are props (they overhang); behind them, low rolling hills.
            Backdrop::Mushrooms => (base - 12.0 + 14.0 * noise(u * 0.015) + 4.0 * noise(u * 0.06)) as i64,
            Backdrop::Volcano => {
                let slot = (u / 150.0).floor();
                let local = u - slot * 150.0;
                let mut cone = if hash(slot as i64, 13) > 0.35 { 58.0 * (1.0 - (local - 75.0).abs() / 62.0).max(0.0) } else { 0.0 };
                if cone > 0.0 && (local - 75.0).abs() < 7.0 {
                    cone -= 7.0 - (local - 75.0).abs(); // the crater
                }
                let jag = 5.0 * hash((u / 3.0).floor() as i64, 14);
                (base - 8.0 + 12.0 * noise(u * 0.02) + jag + cone) as i64
            }
            Backdrop::Glacier => {
                let slot = (u / 11.0).floor();
                let local = u - slot * 11.0;
                let spire = (8.0 + 38.0 * hash(slot as i64, 15) * hash(slot as i64, 16)) * (1.0 - (local - 5.5).abs() / 5.5);
                (base - 12.0 + 8.0 * noise(u * 0.02) + spire) as i64
            }
            Backdrop::Ruins => {
                let slot = (u / 28.0).floor();
                let local = u - slot * 28.0;
                let standing = hash(slot as i64, 17) > 0.3;
                if standing && local > 7.0 && local < 15.0 {
                    let top = base + 8.0 + 34.0 * hash(slot as i64, 18);
                    let capital = if local < 8.0 || local > 14.0 { -2.0 } else { 0.0 };
                    let broken = if hash(slot as i64, 19) > 0.5 { 3.0 * hash(u as i64, 20) } else { 0.0 };
                    (top + capital - broken) as i64
                } else {
                    (base - 14.0 + 5.0 * noise(u * 0.04)) as i64
                }
            }
            Backdrop::Library => {
                let slot = (u / 42.0).floor();
                let local = u - slot * 42.0;
                if local < 34.0 {
                    let top = base + 12.0 + 24.0 * hash(slot as i64, 21);
                    let finial = if local < 2.0 || local > 32.0 { 3.0 } else { 0.0 };
                    (top + finial) as i64
                } else {
                    (base - 6.0) as i64
                }
            }
            Backdrop::Islands => (base - 40.0 + 6.0 * noise(u * 0.02)) as i64,
        }
    }

    fn near_height(&self, biome: &Biome, u: f64) -> i64 {
        let base = self.height as f64 * 0.26;
        match biome.backdrop {
            Backdrop::Mountains => (base + 10.0 * noise(u * 0.02) + 4.0 * noise(u * 0.07)) as i64,
            Backdrop::Pines => {
                let slot = (u / 7.0).floor();
                let local = u - slot * 7.0;
                (base + 2.0 + (10.0 + 10.0 * hash(slot as i64, 2)) * (1.0 - (local - 3.5).abs() / 3.5)) as i64
            }
            Backdrop::Crystals => {
                let slot = (u / 10.0).floor();
                let local = u - slot * 10.0;
                (base - 4.0 + (4.0 + 18.0 * hash(slot as i64, 9)) * (1.0 - (local - 5.0).abs() / 2.5).max(0.0)) as i64
            }
            Backdrop::Castle => (base + 4.0 + if (u as i64) % 8 < 4 { 3.0 } else { 0.0 }) as i64,
            Backdrop::Mushrooms => {
                let slot = (u / 13.0).floor();
                let local = (u - slot * 13.0 - 6.5) / 6.5;
                let cap = (6.0 + 9.0 * hash(slot as i64, 22)) * (1.0 - local * local).max(0.0).sqrt();
                (base - 2.0 + cap) as i64
            }
            Backdrop::Volcano => (base + 6.0 * noise(u * 0.03) + 3.0 * hash((u / 2.0).floor() as i64, 23)) as i64,
            Backdrop::Glacier => {
                let slot = (u / 8.0).floor();
                let local = u - slot * 8.0;
                (base + (3.0 + 12.0 * hash(slot as i64, 24)) * (1.0 - (local - 4.0).abs() / 4.0)) as i64
            }
            Backdrop::Ruins => {
                let slot = (u / 30.0).floor();
                let local = u - slot * 30.0;
                let stump = if hash(slot as i64, 25) > 0.5 && local > 10.0 && local < 16.0 { 6.0 + 12.0 * hash(slot as i64, 26) } else { 0.0 };
                (base - 3.0 + 5.0 * noise(u * 0.05) + stump) as i64
            }
            Backdrop::Library => {
                // Stacks of books: each a few pixels wide and a few tall.
                let slot = (u / 5.0).floor();
                (base + 1.0 + 7.0 * hash(slot as i64, 27) * noise(u * 0.02 + 3.0)) as i64
            }
            Backdrop::Islands => (base - 60.0) as i64,
        }
    }

    /// Scenery that overhangs (so a height map can't draw it): giant mushrooms, floating islands. Mid parallax.
    fn prop(&self, b: &Biome, u: f64, x: i64, from_bottom: i64, time: f64, low: f32) -> Option<u32> {
        let y = from_bottom as f64;
        match b.backdrop {
            Backdrop::Mushrooms => {
                let slot = (u / 58.0).floor();
                let s = slot as i64;
                if hash(s, 41) < 0.35 {
                    return None;
                }
                let center = slot * 58.0 + 16.0 + 26.0 * hash(s, 42);
                let stalk = self.height as f64 * (0.36 + 0.26 * hash(s, 43));
                let (dx, rx) = (u - center, 10.0 + 9.0 * hash(s, 44));
                let ry = rx * 0.55;
                let cy = y - stalk;
                if cy >= -2.0 && (dx / rx).powi(2) + (cy / ry).powi(2) <= 1.0 {
                    let spot = hash(((dx + 40.0) / 4.0).floor() as i64 + s * 13, ((cy + 20.0) / 3.0).floor() as i64) > 0.78;
                    return Some(if cy < 0.5 {
                        mix(b.platform_shade, b.far, 0.45)
                    } else if spot {
                        mix(mix(b.accent, b.far, 0.35), rgb(0xFFFFFF), 0.25 * low)
                    } else if cy > ry * 0.55 && bayer(x, from_bottom) < 0.5 {
                        mix(mix(b.platform, rgb(0xFFFFFF), 0.15), b.far, 0.4)
                    } else {
                        mix(b.platform, b.far, 0.4)
                    });
                }
                let half_width = 1.5 + 1.5 * hash(s, 45) + (stalk - y).max(0.0) * 0.03;
                (y < stalk && dx.abs() < half_width).then(|| mix(b.far, rgb(0xE8D8F0), if dx < 0.0 { 0.3 } else { 0.18 }))
            }
            Backdrop::Islands => {
                let slot = (u / 64.0).floor();
                let s = slot as i64;
                if hash(s, 51) < 0.3 {
                    return None;
                }
                let center = slot * 64.0 + 14.0 + 36.0 * hash(s, 52);
                let half_width = 13.0 + 14.0 * hash(s, 53);
                let top = self.height as f64 * (0.42 + 0.3 * hash(s, 54)) + (time * 0.6 + s as f64).sin() * 2.0;
                let dx = (u - center).abs();
                if dx < half_width && y <= top && y >= top - half_width * 0.9 * (1.0 - dx / half_width) - 1.0 {
                    return Some(if y > top - 2.0 {
                        mix(b.ground_top, b.far, 0.35)
                    } else if bayer(x, from_bottom) < (dx / half_width) as f32 {
                        mix(b.ground_dark, b.far, 0.3)
                    } else {
                        mix(b.ground_fill, b.far, 0.3)
                    });
                }
                // A thin glowing crystal on some islands.
                let crystal_x = center + half_width * (hash(s, 55) - 0.5);
                (hash(s, 56) > 0.5 && (u - crystal_x).abs() < 1.0 && y > top && y < top + 5.0 + 4.0 * hash(s, 57))
                    .then(|| mix(b.block_glow, rgb(0xFFFFFF), 0.3 * low))
            }
            _ => None,
        }
    }

    fn draw_orb(&self, canvas: &mut PixelCanvas, color: u32, moon: bool, low: f32) {
        let (cx, cy, r) = (self.width * 4 / 5, 24, 9);
        canvas.glow(cx, cy, r + 7 + (low * 5.0) as i64, color, 0.12 + 0.14 * low);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let mut c = color;
                if moon && hash(dx + 40, dy + 40) > 0.86 {
                    c = mix(color, rgb(0x9A9AB0), 0.5);
                }
                canvas.set(cx + dx, cy + dy, c);
            }
        }
    }

    fn draw_clouds(&self, canvas: &mut PixelCanvas, biome: &Biome) {
        let cloud = mix(biome.sky_bottom, rgb(0xFFFFFF), 0.65);
        let shade = mix(biome.sky_bottom, rgb(0xFFFFFF), 0.3);
        let parallax = self.camera_x as f64 * 0.1;
        let spacing = 90.0;
        let first = (parallax / spacing).floor() as i64 - 1;
        for slot in first..=first + (self.width as f64 / spacing) as i64 + 2 {
            if hash(slot, 21) <= 0.35 {
                continue;
            }
            let cx = (slot as f64 * spacing + 40.0 * hash(slot, 4) - parallax) as i64;
            let cy = 14 + (30.0 * hash(slot, 8)) as i64;
            for (dx, r) in [(0, 6), (7, 8), (15, 5)] {
                for y in -r..=r {
                    for x in -r..=r {
                        if x * x + y * y * 3 <= r * r {
                            canvas.set(cx + dx + x, cy + y, if y > r / 3 { shade } else { cloud });
                        }
                    }
                }
            }
        }
    }

    // Tiles

    fn draw_tiles(&self, game: &SpellrunGame, canvas: &mut PixelCanvas, low: f32) {
        let tile = SpellrunGame::TILE;
        let first_visible = (self.camera_x as f64 / tile as f64).floor() as i64 - 1;
        let last_visible = first_visible + self.width / tile + 3;
        for col in first_visible..=last_visible {
            let Some(c) = game.column(col) else { continue };
            let biome = &BIOMES[c.biome];
            let x0 = col * tile - self.camera_x;
            for row in 0..c.ground {
                let top = self.screen_top((row * tile) as f64, tile);
                if top >= self.height || top + tile <= 0 {
                    continue;
                }
                draw_ground(canvas, biome, x0, top, col, row, row == c.ground - 1, c.ground - 1 - row);
            }
            if c.platform >= 0 {
                draw_platform(canvas, biome, x0, self.screen_top((c.platform * tile) as f64, tile), col);
            }
            if c.block >= 0 {
                draw_block(canvas, biome, x0, self.screen_top((c.block * tile) as f64, tile), col, low);
            }
            if c.ground > 0 {
                let ground_top = self.screen_top((c.ground * tile) as f64, 0);
                if c.lantern {
                    draw_lantern(canvas, biome, x0 + 7, ground_top, low);
                }
                draw_decor(canvas, biome, c.decor, x0, ground_top, col);
            }
        }
    }

    // Entities

    fn draw_entities(&self, game: &SpellrunGame, canvas: &mut PixelCanvas, high: f32) {
        let time = game.time;
        let art = &*ART;

        for mote in &game.motes {
            let bob = (time * 3.0 + mote.phase).sin() * 2.0;
            let frame = &art.mote[(time * 8.0 + mote.phase) as i64 as usize & 3];
            let (x, y) = (self.screen_x(mote.x), self.screen_top(mote.y + bob, 5));
            canvas.glow(x + 2, y + 2, 5, rgb(0xFFD84D), 0.15 + 0.35 * high);
            canvas.draw(frame, x, y, false);
        }

        for enemy in &game.enemies {
            let x = self.screen_x(enemy.x);
            match enemy.kind {
                EnemyKind::Slime => {
                    let sprite = if enemy.dead() {
                        &art.slime_squished
                    } else {
                        &art.slime[(time * 4.0 + enemy.phase) as i64 as usize & 1]
                    };
                    canvas.draw(sprite, x, self.screen_top(enemy.y, sprite.height()), true);
                }
                EnemyKind::Bat => {
                    let sprite = &art.bat[(time * 9.0 + enemy.phase) as i64 as usize & 1];
                    if !enemy.dead() {
                        canvas.draw(sprite, x, self.screen_top(enemy.y, sprite.height()), false);
                    }
                }
            }
        }

        let hero = &game.hero;
        let casting = hero.cast_time > 0.0 && !hero.pounding;
        let sprite = if casting {
            &art.hero_cast[if hero.grounded { (hero.stride / 7.0) as i64 as usize & 1 } else { 1 }]
        } else if hero.pounding {
            &art.hero_pound
        } else if !hero.grounded {
            &art.hero_jump
        } else {
            &art.hero_run[(hero.stride / 7.0) as i64 as usize & 1]
        };
        let (hero_x, hero_y) = (self.screen_x(hero.x) - 1, self.screen_top(hero.y, sprite.height()));
        let visible = hero.blink <= 0.0 || (time * 14.0) as i64 % 2 == 0;
        if visible {
            canvas.draw(sprite, hero_x, hero_y, false);
        }
        // The staff gem glows with the highs, and flares while casting.
        let (gem_x, gem_y) = if casting { (hero_x + 14, hero_y + 10) } else { (hero_x + 10, hero_y + 2) };
        let flare = if casting { 0.5 } else { 0.0 };
        canvas.glow(gem_x, gem_y, 3 + (high * 5.0) as i64 + if casting { 4 } else { 0 }, rgb(0x74FFAD), 0.2 + 0.6 * high + flare);

        for bolt in &game.bolts {
            let size = if bolt.big { 3 } else { 2 };
            let (x, y) = (self.screen_x(bolt.x), self.screen_top(bolt.y, size));
            canvas.glow(x + 1, y + 1, if bolt.big { 8 } else { 5 }, bolt.color, 0.6);
            canvas.fill_rect(x, y, 3, size, mix(bolt.color, rgb(0xFFFFFF), 0.75));
        }

        for particle in game.particles.iter().filter(|p| p.life > 0.0) {
            let (x, y) = (self.screen_x(particle.x), self.screen_top(particle.y, particle.size));
            let fade_out = (particle.life / particle.max_life) as f32;
            if fade_out > 0.5 || bayer(x, y) < fade_out * 2.0 {
                canvas.fill_rect(x, y, particle.size, particle.size, particle.color);
            }
        }
    }

    // Weather

    /// Ambient particles for the current biome. Positions come from a hash of each particle's index and the time,
    /// so they need no state; they drift with their own parallax and react to the music.
    fn draw_weather(&self, game: &SpellrunGame, canvas: &mut PixelCanvas, levels: BandLevels) {
        let b = game.biome();
        let count = match b.weather {
            Weather::Clear => return,
            Weather::Snow => 80,
            Weather::Bubbles => 26,
            Weather::Embers => 46,
            Weather::Spores => 34,
            Weather::Fireflies => 22,
            Weather::Dust => 44,
        };
        let (w, h, t) = (self.width as f64 + 40.0, self.height as f64 + 20.0, game.time);
        let wrap = |v: f64, span: f64| v.rem_euclid(span);
        let fade = game.biome_fade as f32;
        for i in 0..count {
            let (r1, r2, r3) = (hash(i, 1), hash(i, 2), hash(i, 3));
            // During a biome change the new weather dissolves in with the backdrop.
            if bayer(i, 7) > fade {
                continue;
            }
            let depth = 0.4 + 0.6 * r3;
            let drift_x = r1 * w - self.camera_x as f64 * 0.6 * depth;
            let color = b.weather_color;
            match b.weather {
                Weather::Snow => {
                    let x = wrap(drift_x + (t * 0.8 + i as f64).sin() * 6.0 - t * 8.0 * depth, w) as i64 - 20;
                    let y = wrap(r2 * h + t * (10.0 + 18.0 * depth) * (0.8 + 0.4 * levels.mid as f64), h) as i64 - 10;
                    let size = if depth > 0.85 { 2 } else { 1 };
                    canvas.fill_rect(x, y, size, size, mix(b.sky_bottom, color, 0.5 + 0.5 * depth as f32));
                }
                Weather::Embers => {
                    let x = wrap(drift_x + (t * 2.0 + i as f64).sin() * 3.0, w) as i64 - 20;
                    let y = self.height - wrap(r2 * h + t * (16.0 + 22.0 * depth), h) as i64 + 10;
                    let flicker = 0.5 + 0.5 * (t * (6.0 + 5.0 * r1) + i as f64).sin() as f32;
                    canvas.set(x, y, mix(rgb(0xB2301E), color, flicker));
                    canvas.glow(x, y, 2, color, (0.1 + 0.35 * levels.low) * flicker);
                }
                Weather::Bubbles => {
                    let x = wrap(drift_x + (t * 3.0 + i as f64).sin() * 2.0, w) as i64 - 20;
                    let y = self.height - wrap(r2 * h + t * (12.0 + 16.0 * depth), h) as i64 + 10;
                    let c = mix(b.sky_bottom, color, 0.35 + 0.4 * depth as f32);
                    if depth > 0.7 {
                        for (dx, dy) in [(1, 0), (0, 1), (2, 1), (1, 2)] {
                            canvas.set(x + dx, y + dy, c);
                        }
                        canvas.set(x + 1, y, mix(c, rgb(0xFFFFFF), 0.5));
                    } else {
                        canvas.set(x, y, c);
                    }
                }
                Weather::Spores => {
                    let x = wrap(drift_x + (t * 0.7 + i as f64 * 1.7).sin() * 8.0, w) as i64 - 20;
                    let y = self.height - wrap(r2 * h + t * (4.0 + 6.0 * depth), h) as i64 + 10;
                    canvas.set(x, y, mix(b.sky_bottom, color, 0.7));
                    canvas.glow(x, y, 3, color, 0.08 + 0.3 * levels.high);
                }
                Weather::Fireflies => {
                    let x = wrap(drift_x + (t * 0.4 + r2 * 9.0).sin() * 18.0, w) as i64 - 20;
                    let y = (self.height as f64 * (0.3 + 0.45 * r2) + (t * 0.7 + i as f64 * 1.3).sin() * 9.0) as i64;
                    let blink = ((t * 0.9 + r1 * 10.0).sin() as f32 - 0.2).max(0.0) * (0.6 + 0.6 * levels.high);
                    if blink > 0.05 {
                        canvas.set(x, y, mix(b.near, color, blink.min(1.0)));
                        canvas.glow(x, y, 4, color, 0.25 * blink);
                    }
                }
                Weather::Dust => {
                    let x = wrap(drift_x + t * 3.0 * depth, w) as i64 - 20;
                    let y = (r2 * self.height as f64 + (t * 0.4 + i as f64).sin() * 6.0) as i64;
                    let twinkle = 0.3 + 0.7 * ((t * 1.5 + r1 * 20.0).sin() as f32 * 0.5 + 0.5) * (0.4 + 0.6 * levels.high);
                    canvas.set(x, y, mix(b.sky_bottom, color, twinkle));
                }
                Weather::Clear => {}
            }
        }
    }

    // HUD

    fn draw_hud(&self, game: &SpellrunGame, canvas: &mut PixelCanvas, levels: BandLevels, show_meters: bool) {
        let shadow = OUTLINE;
        // Top right, clear of the window's title-bar buttons on the left.
        let count = format!("{:03}", game.motes_collected.min(999));
        let count_x = self.width - 5 - (count.len() as i64 * 4 - 1);
        canvas.draw(&ART.mote[0], count_x - 8, 5, false);
        canvas.text(&count, count_x + 1, 6, shadow);
        canvas.text(&count, count_x, 5, rgb(0xFFF4C8));

        if game.banner_time > 0.0 {
            let name = game.biome().name.to_uppercase();
            let text_width = name.chars().count() as i64 * 4 - 1;
            let (x, y) = ((self.width - text_width) / 2, 18);
            let alpha = game.banner_time.min(1.0).min((3.0 - game.banner_time) * 3.0) as f32;
            for (offset, color) in [(1, shadow), (0, rgb(0xFFFFFF))] {
                for (i, character) in name.chars().enumerate() {
                    let cx = x + i as i64 * 4;
                    if bayer(cx, y) < alpha {
                        canvas.text(&character.to_string(), cx + offset, y + offset, color);
                    }
                }
            }
        }

        if show_meters {
            let colors = [rgb(0xFF5C7A), rgb(0x74FFAD), rgb(0x7CC8FF)];
            for (band, &color) in colors.iter().enumerate() {
                let (x, bottom, full) = (5 + band as i64 * 5, self.height - 5, 28);
                let filled = (full as f32 * levels[band]) as i64;
                canvas.fill_rect(x, bottom - full, 3, full, rgb(0x15101E));
                canvas.fill_rect(x, bottom - filled, 3, filled, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_ground(canvas: &mut PixelCanvas, b: &Biome, x: i64, y: i64, col: i64, row: i64, is_top: bool, depth: i64) {
    let tile = SpellrunGame::TILE;
    let fill = if depth >= 2 { mix(b.ground_fill, b.ground_dark, 0.45) } else { b.ground_fill };
    canvas.fill_rect(x, y, tile, tile, fill);
    for py in 0..tile {
        for px in 0..tile {
            if hash(col * tile + px, row * tile + py) > 0.9 {
                canvas.set(x + px, y + py, b.ground_dark);
            }
        }
    }
    if !is_top {
        return;
    }
    for px in 0..tile {
        let lip = 3 + if hash(col * tile + px, 99) > 0.6 { 1 } else { 0 };
        canvas.fill_rect(x + px, y, 1, lip, b.ground_top);
        canvas.set(x + px, y + lip, b.ground_top_shade);
        if hash(col * tile + px, 77) > 0.85 {
            canvas.set(x + px, y - 1, b.ground_top);
        }
    }
}

fn draw_platform(canvas: &mut PixelCanvas, b: &Biome, x: i64, y: i64, col: i64) {
    canvas.fill_rect(x, y, 16, 5, b.platform);
    canvas.fill_rect(x, y + 5, 16, 1, b.platform_shade);
    canvas.fill_rect(x, y, 16, 1, mix(b.platform, rgb(0xFFFFFF), 0.25));
    canvas.set(x + 3, y + 2, b.platform_shade);
    canvas.set(x + 12, y + 2, b.platform_shade);
    if col % 2 == 0 {
        canvas.fill_rect(x + 15, y + 1, 1, 4, b.platform_shade);
    }
}

fn draw_block(canvas: &mut PixelCanvas, b: &Biome, x: i64, y: i64, col: i64, low: f32) {
    canvas.fill_rect(x, y, 16, 16, OUTLINE);
    canvas.fill_rect(x + 1, y + 1, 14, 14, b.block);
    canvas.fill_rect(x + 1, y + 1, 14, 1, mix(b.block, rgb(0xFFFFFF), 0.3));
    let glyph = &RUNES[col.unsigned_abs() as usize % RUNES.len()];
    let glow = mix(b.block, b.block_glow, 0.45 + 0.55 * low);
    for (row, line) in glyph.iter().enumerate() {
        for (column, bit) in line.chars().enumerate() {
            if bit == '#' {
                canvas.set(x + 4 + column as i64, y + 4 + row as i64, glow);
            }
        }
    }
    canvas.glow(x + 8, y + 8, 11, b.block_glow, 0.08 + 0.25 * low);
}

fn draw_lantern(canvas: &mut PixelCanvas, b: &Biome, x: i64, ground_y: i64, low: f32) {
    canvas.fill_rect(x, ground_y - 14, 2, 14, b.platform_shade);
    canvas.fill_rect(x - 1, ground_y - 18, 4, 4, OUTLINE);
    let lamp = mix(mix(b.accent, rgb(0x3A2A20), 0.6), mix(b.accent, rgb(0xFFFFFF), 0.4), low);
    canvas.fill_rect(x, ground_y - 17, 2, 2, lamp);
    canvas.glow(x + 1, ground_y - 16, 9, b.accent, 0.05 + 0.5 * low * low);
}

fn draw_decor(canvas: &mut PixelCanvas, b: &Biome, kind: u8, x: i64, ground_y: i64, col: i64) {
    let px = x + 3 + (9.0 * hash(col, 31)) as i64;
    match kind {
        1 => {
            canvas.set(px, ground_y - 1, b.ground_top_shade);
            canvas.set(px + 1, ground_y - 2, b.ground_top);
            canvas.set(px + 2, ground_y - 1, b.ground_top_shade);
        }
        2 => {
            canvas.fill_rect(px, ground_y - 3, 1, 3, b.ground_top_shade);
            canvas.set(px, ground_y - 4, b.accent);
            canvas.set(px - 1, ground_y - 4, mix(b.accent, rgb(0xFFFFFF), 0.4));
        }
        3 => {
            canvas.fill_rect(px, ground_y - 4, 2, 4, b.accent);
            canvas.set(px, ground_y - 5, mix(b.accent, rgb(0xFFFFFF), 0.5));
        }
        _ => {}
    }
}

fn hash(x: i64, y: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth 1D value noise in about 0...1.
fn noise(u: f64) -> f64 {
    let i = u.floor();
    let f = u - i;
    let t = f * f * (3.0 - 2.0 * f);
    hash(i as i64, 0) * (1.0 - t) + hash(i as i64 + 1, 0) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spellrun::game::SpellrunInput;

    #[test]
    fn renders_a_frame() {
        let mut game = SpellrunGame::new(11);
        for frame in 0..60 * 6 {
            let input = SpellrunInput { low: 0.7, mid: 0.6, high: 0.5, kick: frame % 28 == 0, sparkle: frame % 40 == 0, ..Default::default() };
            game.step(1.0 / 60.0, input, 256.0, 160.0);
        }
        let mut canvas = PixelCanvas::new(256, 160);
        SpellrunRenderer::new().render(&game, &mut canvas, BandLevels::new(0.7, 0.6, 0.5), true);
        let colors: std::collections::HashSet<u32> = canvas.pixels.iter().copied().collect();
        assert!(colors.len() > 20, "a real frame has lots of colors, got {}", colors.len());
        if let Ok(path) = std::env::var("SPELLRUN_PREVIEW") {
            let bytes: Vec<u8> = canvas.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
            std::fs::write(path, bytes).unwrap(); // raw RGBA, 256x160
        }
    }
}

