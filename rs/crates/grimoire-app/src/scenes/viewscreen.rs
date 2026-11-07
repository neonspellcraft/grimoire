//! The view from a starship's bridge, with the music as the engine (port of `ViewscreenScene.swift`).
//!
//! - LOW is the throttle: a laggy, smoothed speed drives a layered 3D starfield, and stars stretch into warp
//!   streaks as it rises. It never drops below a slow drift.
//! - MID is the environment: the nebula's opacity and saturation follow it, and its hue drifts.
//! - HIGH is sparkle: near stars twinkle, shooting stars appear, and strong hits glint across the glass.
//!
//! Every 30–90 s a planet, moon or asteroid field drifts past on one side.

use glam::{Vec2, Vec3};

use super::{color_pass, SceneAudio, WindowScene};

/// Colors for the Viewscreen scene. Change these to restyle it.
struct Palette {
    /// Deep navy at the center of the view, fading to near-black at the edges.
    space: Vec3,
    space_edge: Vec3,
    /// White-blue starlight.
    star: Vec3,
    nebula_teal: Vec3,
    nebula_magenta: Vec3,
    nebula_violet: Vec3,
    /// HUD lines and readouts on the glass.
    hud: Vec3,
    /// Rim light on planets.
    atmosphere: Vec3,
    /// Surface color pairs for passing bodies; one pair is picked at random each time.
    planets: [(Vec3, Vec3); 3],
    moons: [(Vec3, Vec3); 2],
    asteroids: [(Vec3, Vec3); 1],
}

const PALETTE: Palette = Palette {
    space: Vec3::new(0.012, 0.018, 0.045),
    space_edge: Vec3::new(0.0, 0.002, 0.010),
    star: Vec3::new(0.78, 0.87, 1.0),
    nebula_teal: Vec3::new(0.04, 0.55, 0.58),
    nebula_magenta: Vec3::new(0.72, 0.10, 0.52),
    nebula_violet: Vec3::new(0.38, 0.18, 0.85),
    hud: Vec3::new(0.55, 0.80, 1.0),
    atmosphere: Vec3::new(0.40, 0.70, 1.0),
    planets: [
        (Vec3::new(0.55, 0.32, 0.42), Vec3::new(0.86, 0.68, 0.55)), // dusty rose and sand
        (Vec3::new(0.10, 0.28, 0.48), Vec3::new(0.45, 0.72, 0.78)), // ocean blues
        (Vec3::new(0.30, 0.18, 0.50), Vec3::new(0.78, 0.55, 0.85)), // violet giant
    ],
    moons: [
        (Vec3::new(0.30, 0.30, 0.33), Vec3::new(0.70, 0.70, 0.72)),
        (Vec3::new(0.36, 0.30, 0.26), Vec3::new(0.72, 0.64, 0.56)),
    ],
    asteroids: [(Vec3::new(0.22, 0.20, 0.19), Vec3::new(0.50, 0.46, 0.42))],
};

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view: [f32; 4],
    camera: [f32; 4],
    motion: [f32; 4],
    bands: [f32; 4],
    hits: [f32; 4],
    nebula: [f32; 4],
    body: [f32; 4],
    body_look: [f32; 4],
    body_color_a: [f32; 4],
    body_color_b: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPalette {
    colors: [[f32; 4]; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Star {
    /// xyz world, layer speed multiplier
    position: [f32; 4],
    /// size px, twinkle, tint, brightness
    look: [f32; 4],
}

/// Star depth layers, drawn far to near. Near layers move faster than their depth alone would make them,
/// which exaggerates the parallax.
struct Layer {
    count: usize,
    near: f32,
    far: f32,
    speed: f32,
    size: f32,
    brightness: f32,
}

const LAYERS: [Layer; 4] = [
    Layer { count: 900, near: 40.0, far: 80.0, speed: 0.25, size: 0.9, brightness: 0.55 },
    Layer { count: 600, near: 20.0, far: 40.0, speed: 0.45, size: 1.2, brightness: 0.70 },
    Layer { count: 400, near: 8.0, far: 20.0, speed: 0.70, size: 1.6, brightness: 0.85 },
    Layer { count: 220, near: 1.5, far: 8.0, speed: 1.00, size: 2.2, brightness: 1.00 },
];
/// Stars in these first layers are drawn behind a passing body; the rest in front of it.
const BACK_LAYER_COUNT: usize = 2;

#[derive(Clone, Copy, PartialEq)]
enum BodyKind {
    Planet = 1,
    Moon = 2,
    Asteroids = 3,
}

struct PassingBody {
    kind: BodyKind,
    position: Vec3,
    radius: f32,
    speed: f32,
    colors: (Vec3, Vec3),
    has_ring: bool,
    seed: f32,
    light_angle: f32,
    /// Asteroid field only: rock offsets from `position` (xyz) and radius (w), sorted far to near.
    rocks: Vec<[f32; 4]>,
}

#[derive(Clone, Copy, Default)]
struct Meteor {
    head: Vec2,
    velocity: Vec2,
    age: f32,
    life: f32,
    length: f32,
}

impl Meteor {
    fn is_active(&self) -> bool {
        self.age < self.life
    }
}

fn rand(low: f32, high: f32) -> f32 {
    low + fastrand::f32() * (high - low)
}

fn rand64(low: f64, high: f64) -> f64 {
    low + fastrand::f64() * (high - low)
}

/// Like `simd_smoothstep`, including reversed edges.
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct ViewscreenScene {
    nebula_pipeline: wgpu::RenderPipeline,
    star_pipeline: wgpu::RenderPipeline,
    body_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    palette_buffer: wgpu::Buffer,
    rock_buffer: wgpu::Buffer,
    meteor_buffer: wgpu::Buffer,
    star_buffer: wgpu::Buffer,

    stars: Vec<Star>,
    star_layer: Vec<usize>,
    meteors: [Meteor; 3],
    rock_scratch: [[f32; 4]; 16],
    meteor_scratch: [[f32; 4]; 6],
    body: Option<PassingBody>,
    /// The first body shows up fairly soon so it's discoverable; after that every 30–90 s.
    next_body_in: f64,

    audio: SceneAudio,
    time: f64,
    throttle: f64,
    sustained_low: f64,
    heading: f64,
    glint: f64,
    nebula_drift: [f64; 2],
    hue: f64,
    roll: f64,
    twinkle_budget: f64,
    last_high_hit: f32,
    aspect: f32,
}

impl ViewscreenScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("viewscreen.wgsl"));
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewscreen"),
            entries: &[uniform(0), uniform(1), uniform(2), uniform(3)],
        });
        let buffer = |label, size: usize, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniform_usage = wgpu::BufferUsages::UNIFORM;
        let uniform_buffer = buffer("viewscreen uniforms", std::mem::size_of::<Uniforms>(), uniform_usage);
        let palette_buffer = buffer("viewscreen palette", std::mem::size_of::<GpuPalette>(), uniform_usage);
        let rock_buffer = buffer("viewscreen rocks", 16 * 16, uniform_usage);
        let meteor_buffer = buffer("viewscreen meteors", 6 * 16, uniform_usage);
        let star_count: usize = LAYERS.iter().map(|l| l.count).sum();
        let star_buffer = buffer(
            "viewscreen stars",
            star_count * std::mem::size_of::<Star>(),
            wgpu::BufferUsages::VERTEX,
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewscreen"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: palette_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: rock_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: meteor_buffer.as_entire_binding() },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewscreen"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let over = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        let star_attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
        let star_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Star>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &star_attributes,
        }];
        let pipeline = |label, vertex, fragment, blend: Option<wgpu::BlendState>, stars: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: if stars { &star_buffers } else { &[] },
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: if stars {
                        wgpu::PrimitiveTopology::TriangleStrip
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };

        let mut scene = ViewscreenScene {
            nebula_pipeline: pipeline("viewscreen nebula", "fullscreen", "nebula_fragment", None, false),
            star_pipeline: pipeline("viewscreen stars", "star_vertex", "star_fragment", Some(additive), true),
            body_pipeline: pipeline("viewscreen body", "fullscreen", "body_fragment", Some(over), false),
            overlay_pipeline: pipeline("viewscreen overlay", "fullscreen", "overlay_fragment", Some(over), false),
            group,
            uniform_buffer,
            palette_buffer,
            rock_buffer,
            meteor_buffer,
            star_buffer,
            stars: Vec::with_capacity(star_count),
            star_layer: Vec::with_capacity(star_count),
            meteors: [Meteor::default(); 3],
            rock_scratch: [[0.0; 4]; 16],
            meteor_scratch: [[0.0; 4]; 6],
            body: None,
            next_body_in: 15.0,
            audio: SceneAudio::default(),
            time: 0.0,
            throttle: 0.0,
            sustained_low: 0.0,
            heading: rand64(0.0, 360.0),
            glint: 0.0,
            nebula_drift: [rand64(0.0, 50.0), rand64(0.0, 50.0)],
            hue: 0.0,
            roll: 0.0,
            twinkle_budget: 0.0,
            last_high_hit: 0.0,
            aspect: 1.6,
        };
        scene.seed_stars();
        scene
    }

    fn seed_stars(&mut self) {
        self.stars.clear();
        self.star_layer.clear();
        for (index, layer) in LAYERS.iter().enumerate() {
            for _ in 0..layer.count {
                let mut star = Star::default();
                respawn(&mut star, layer, true, self.aspect);
                self.stars.push(star);
                self.star_layer.push(index);
            }
        }
    }

    fn update_stars(&mut self, speed: f32, dt: f32) {
        let cover_x = self.aspect + 0.3;
        for (star, &layer_index) in self.stars.iter_mut().zip(&self.star_layer) {
            let layer = &LAYERS[layer_index];
            star.position[2] -= speed * layer.speed * dt;
            let z = star.position[2];
            if z < layer.near || (star.position[0] / z).abs() > cover_x || (star.position[1] / z).abs() > 1.3 {
                respawn(star, layer, false, self.aspect);
            }
            // Fade in at the far edge of the layer so respawns don't pop.
            let fade_in = smoothstep(layer.far, layer.far - 0.2 * (layer.far - layer.near), star.position[2]);
            star.look[3] = layer.brightness * fade_in;
            star.look[1] = (star.look[1] - dt / 0.3).max(0.0);
        }
    }

    fn update_sparkle(&mut self, high: f64, dt: f64) {
        let high_hit = self.audio.hits.high;
        let struck = high_hit > 0.9 && self.last_high_hit <= 0.9;
        self.last_high_hit = high_hit;

        // Twinkles on the near layers: a burst on each hit, plus a trickle that follows the highs.
        let near_start: usize = LAYERS[..2].iter().map(|l| l.count).sum();
        self.twinkle_budget += high * 20.0 * dt + if struck { 8.0 } else { 0.0 };
        while self.twinkle_budget >= 1.0 {
            let index = fastrand::usize(near_start..self.stars.len());
            self.stars[index].look[1] = 1.0;
            self.twinkle_budget -= 1.0;
        }

        if struck && self.glint < 0.2 && fastrand::f64() < 0.4 {
            self.glint = 1.0;
        }
        self.glint = (self.glint - dt / 1.2).max(0.0);

        let spawn_chance = (0.02 + 0.35 * high) * dt + if struck { 0.3 } else { 0.0 };
        if fastrand::f64() < spawn_chance {
            if let Some(slot) = self.meteors.iter().position(|m| !m.is_active()) {
                let direction = if fastrand::bool() { 1.0 } else { -1.0 };
                let angle = rand(0.35, 0.7);
                self.meteors[slot] = Meteor {
                    head: Vec2::new(rand(-self.aspect, self.aspect), rand(0.1, 0.9)),
                    velocity: Vec2::new(direction * angle.cos(), -angle.sin()) * rand(1.6, 2.6),
                    age: 0.0,
                    life: rand(0.55, 1.0),
                    length: rand(0.25, 0.45),
                };
            }
        }
        for meteor in self.meteors.iter_mut().filter(|m| m.is_active()) {
            meteor.age += dt as f32;
            meteor.head += meteor.velocity * dt as f32;
        }
    }

    fn update_body(&mut self, speed: f32, dt: f32) {
        let Some(current) = &mut self.body else {
            self.next_body_in -= dt as f64;
            if self.next_body_in <= 0.0 {
                self.body = Some(make_body());
            }
            return;
        };
        current.position.z -= (current.speed + speed * 0.05) * dt;
        let z = current.position.z;
        let reach = if current.kind == BodyKind::Asteroids {
            current.radius + 5.0
        } else {
            current.radius * if current.has_ring { 2.4 } else { 1.1 }
        };
        if z < 1.0 || (current.position.x / z).abs() - reach / z > self.aspect + 0.1 {
            self.body = None;
            self.next_body_in = rand64(30.0, 90.0);
        }
    }

    fn uniforms(&self, size: (u32, u32), show_meters: bool) -> Uniforms {
        let (levels, hits) = (self.audio.levels, self.audio.hits);
        let t = self.time;
        let sway_x = (0.010 * (t * 0.07).sin() + 0.004 * (t * 0.17 + 2.0).sin()) as f32;
        let sway_y = (0.008 * (t * 0.09 + 2.0).sin() + 0.003 * (t * 0.21).sin()) as f32;
        let streak = (1.2 + self.throttle * 48.0) as f32 * 0.06;
        let mut uniforms = Uniforms {
            view: [size.0 as f32, size.1 as f32, self.aspect, t as f32],
            camera: [self.roll as f32, sway_x, sway_y, streak],
            motion: [self.throttle as f32, self.heading as f32, self.glint as f32, 0.0],
            bands: [levels.low, levels.mid, levels.high, self.sustained_low as f32],
            hits: [hits.low, hits.mid, hits.high, if show_meters { 1.0 } else { 0.0 }],
            nebula: [self.nebula_drift[0] as f32, self.nebula_drift[1] as f32, self.hue as f32, 0.0],
            ..Default::default()
        };
        if let Some(body) = &self.body {
            let z = body.position.z;
            uniforms.body = [body.position.x / z, body.position.y / z, body.radius / z, body.kind as i32 as f32];
            uniforms.body_look = [if body.has_ring { 1.0 } else { 0.0 }, body.seed, body.light_angle, 0.0];
            uniforms.body_color_a = body.colors.0.extend(1.0).to_array();
            uniforms.body_color_b = body.colors.1.extend(1.0).to_array();
        }
        uniforms
    }

    /// Projects the asteroid field's rocks into screen space (xy, radius, seed); unused slots get radius 0.
    fn fill_rocks(&mut self) {
        self.rock_scratch = [[0.0; 4]; 16];
        let Some(body) = self.body.as_ref().filter(|b| b.kind == BodyKind::Asteroids) else { return };
        for (i, rock) in body.rocks.iter().enumerate().take(16) {
            let z = body.position.z + rock[2];
            if z <= 0.5 {
                continue;
            }
            self.rock_scratch[i] =
                [(body.position.x + rock[0]) / z, (body.position.y + rock[1]) / z, rock[3] / z, i as f32 * 7.31];
        }
    }

    /// Each meteor is two vec4s: head (xy, brightness) and tail (xy).
    fn fill_meteors(&mut self) {
        for (i, meteor) in self.meteors.iter().enumerate() {
            if !meteor.is_active() {
                self.meteor_scratch[i * 2] = [0.0; 4];
                self.meteor_scratch[i * 2 + 1] = [0.0; 4];
                continue;
            }
            let progress = meteor.age / meteor.life;
            let brightness = (progress * 6.0).min(1.0) * (1.0 - progress);
            let tail = meteor.head - meteor.velocity.normalize() * meteor.length * (progress * 4.0).min(1.0);
            self.meteor_scratch[i * 2] = [meteor.head.x, meteor.head.y, brightness, 0.0];
            self.meteor_scratch[i * 2 + 1] = [tail.x, tail.y, 0.0, 0.0];
        }
    }
}

fn respawn(star: &mut Star, layer: &Layer, anywhere: bool, aspect: f32) {
    let z = if anywhere {
        rand(layer.near, layer.far)
    } else {
        layer.far - rand(0.0, 0.08 * (layer.far - layer.near))
    };
    let cover_x = aspect + 0.25;
    star.position = [rand(-cover_x, cover_x) * z, rand(-1.25, 1.25) * z, z, layer.speed];
    star.look = [layer.size * rand(0.7, 1.3), 0.0, rand(0.0, 0.35), 0.0];
}

fn make_body() -> PassingBody {
    let roll = fastrand::f64();
    let kind = if roll < 0.45 {
        BodyKind::Planet
    } else if roll < 0.75 {
        BodyKind::Moon
    } else {
        BodyKind::Asteroids
    };
    let side = if fastrand::bool() { 1.0 } else { -1.0 };
    let (radius, colors) = match kind {
        BodyKind::Planet => (rand(7.0, 11.0), PALETTE.planets[fastrand::usize(..PALETTE.planets.len())]),
        BodyKind::Moon => (rand(3.5, 5.5), PALETTE.moons[fastrand::usize(..PALETTE.moons.len())]),
        BodyKind::Asteroids => (1.0, PALETTE.asteroids[0]),
    };
    let offset = if kind == BodyKind::Asteroids { rand(7.0, 10.0) } else { radius * 1.3 + rand(1.0, 4.0) };
    let mut rocks: Vec<[f32; 4]> = if kind == BodyKind::Asteroids {
        (0..16).map(|_| [rand(-5.0, 5.0), rand(-3.0, 3.0), rand(-6.0, 6.0), rand(0.25, 0.9)]).collect()
    } else {
        Vec::new()
    };
    rocks.sort_by(|a, b| b[2].total_cmp(&a[2]));
    PassingBody {
        kind,
        position: Vec3::new(side * offset, rand(-0.35, 0.35) * radius.max(4.0), 70.0),
        radius,
        speed: rand(1.8, 2.8),
        colors,
        has_ring: kind == BodyKind::Planet && fastrand::bool(),
        seed: rand(0.0, 100.0),
        light_angle: if side > 0.0 { rand(2.4, 3.4) } else { rand(-0.5, 0.5) },
        rocks,
    }
}

impl WindowScene for ViewscreenScene {
    fn update(&mut self, audio: SceneAudio, dt: f64) {
        self.audio = audio;
        self.time += dt;
        let (low, mid, high) = (audio.levels.low as f64, audio.levels.mid as f64, audio.levels.high as f64);

        // Throttle: rises over ~0.35 s and falls over ~1.6 s, so kicks feel like thrust surges.
        let target = (0.05 + 0.8 * low + 0.45 * audio.hits.low as f64).min(1.2);
        let time_constant = if target > self.throttle { 0.35 } else { 1.6 };
        self.throttle += (target - self.throttle) * (1.0 - (-dt / time_constant).exp());
        self.sustained_low += (low - self.sustained_low) * (1.0 - (-dt / 3.0).exp());
        let speed = (1.2 + self.throttle * 48.0) as f32;

        // Ship drift: slow sway and roll, banking a little on sustained bass.
        let t = self.time;
        self.roll = 0.012 * (t * 0.11).sin()
            + 0.007 * (t * 0.23 + 1.0).sin()
            + 0.05 * self.sustained_low * (t * 0.05 + 0.6).sin();
        self.heading = (self.heading + dt * (0.6 + 40.0 * self.roll)).rem_euclid(360.0);

        let speed_drift = 0.0004 * speed as f64 * dt;
        self.nebula_drift[0] += 0.006 * dt * (1.0 + 2.0 * mid);
        self.nebula_drift[1] += 0.003 * dt * (1.0 + 2.0 * mid) + speed_drift;
        self.hue += dt * (0.01 + 0.05 * mid);

        self.update_stars(speed, dt as f32);
        self.update_sparkle(high, dt);
        self.update_body(speed, dt as f32);
    }

    fn reseed(&mut self) {
        self.heading = rand64(0.0, 360.0);
        self.seed_stars();
        self.body = None;
        self.next_body_in = 3.0;
    }

    fn encode(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        show_meters: bool,
    ) {
        self.aspect = size.0 as f32 / size.1.max(1) as f32;
        self.fill_rocks();
        self.fill_meteors();
        let p = &PALETTE;
        let palette = GpuPalette {
            colors: [p.space, p.space_edge, p.star, p.nebula_teal, p.nebula_magenta, p.nebula_violet, p.hud, p.atmosphere]
                .map(|c| c.extend(1.0).to_array()),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms(size, show_meters)));
        queue.write_buffer(&self.palette_buffer, 0, bytemuck::bytes_of(&palette));
        queue.write_buffer(&self.rock_buffer, 0, bytemuck::cast_slice(&self.rock_scratch));
        queue.write_buffer(&self.meteor_buffer, 0, bytemuck::cast_slice(&self.meteor_scratch));
        queue.write_buffer(&self.star_buffer, 0, bytemuck::cast_slice(&self.stars));

        let back_count = LAYERS[..BACK_LAYER_COUNT].iter().map(|l| l.count).sum::<usize>() as u32;
        let total = self.stars.len() as u32;
        let mut pass = color_pass(encoder, "viewscreen", target);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(&self.nebula_pipeline);
        pass.draw(0..3, 0..1);

        pass.set_pipeline(&self.star_pipeline);
        pass.set_vertex_buffer(0, self.star_buffer.slice(..));
        pass.draw(0..4, 0..back_count);

        if self.body.is_some() {
            pass.set_pipeline(&self.body_pipeline);
            pass.draw(0..3, 0..1);
        }

        pass.set_pipeline(&self.star_pipeline);
        pass.draw(0..4, back_count..total);

        pass.set_pipeline(&self.overlay_pipeline);
        pass.draw(0..3, 0..1);
    }
}
