//! Neon paint thrown at a black canvas by the music (port of `SplatterScene.swift`).
//!
//! Every splat is shaped by a piece of theoretical physics (see `Theory`): superstring harmonics, quantum orbitals,
//! Einstein rings, inflation fields, Kerr black holes, Lorenz attractors, Feynman diagrams, double-slit fringes
//! and Brownian fractals. All parameters are fresh continuous random values, so no two splats are ever the same.
//!
//! - LOW: kicks throw big cosmic splats (Einstein ring, inflation, Kerr, superstring, Brownian), sized by the hit.
//! - MID: quantum and particle splats on mid hits (orbitals, Feynman diagrams); drips run longer and faster with
//!   the mids; the color group drifts with them.
//! - HIGH: Lorenz attractors, double-slit fringes and brush flicks, plus a fine speckle mist.
//!
//! Paint accumulates on a persistent canvas. It glistens while wet, dries, and slowly fades so the painting keeps
//! evolving. A drop triggers a pour, and the old paint dries off faster to make room.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};
use grimoire_core::BandLevels;

use super::{color_pass, SceneAudio, WindowScene};

/// Each splat picks from a small group of neighboring swatches, and the group drifts over time, so the painting
/// goes through color phases instead of being uniformly rainbow.
const SWATCHES: [Vec3; 6] = [
    Vec3::new(1.00, 0.18, 0.53), // hot pink
    Vec3::new(1.00, 0.48, 0.00), // laser orange
    Vec3::new(1.00, 0.95, 0.00), // highlighter yellow
    Vec3::new(0.49, 1.00, 0.00), // acid green
    Vec3::new(0.00, 0.94, 1.00), // electric cyan
    Vec3::new(0.64, 0.23, 1.00), // UV purple
];
/// How many neighboring swatches are in play at once.
const GROUP_SIZE: usize = 3;
const MAX_STAMPS_PER_FRAME: usize = 3072;
const CANVAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view: [f32; 4],
    dry: [f32; 4],
    bands: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Stamp {
    shape: [f32; 4], // center xy, radius, rotation
    color: [f32; 4], // rgb, seed
    style: [f32; 4], // kind (0 blob, 1 round, 2...5 theories), stretch, two parameters
}

#[derive(Clone, Copy, Default)]
struct Drip {
    position: Vec2,
    speed: f32,
    radius: f32,
    remaining: f32,
    color: Vec3,
}

/// The physics each splat is shaped by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theory {
    Superstring,
    QuantumOrbital,
    EinsteinRing,
    Inflation,
    KerrBlackHole,
    LorenzAttractor,
    FeynmanDiagram,
    DoubleSlit,
    BrownianFractal,
    BrushFlick,
}

impl Theory {
    const ALL: [Theory; 10] = [
        Theory::Superstring,
        Theory::QuantumOrbital,
        Theory::EinsteinRing,
        Theory::Inflation,
        Theory::KerrBlackHole,
        Theory::LorenzAttractor,
        Theory::FeynmanDiagram,
        Theory::DoubleSlit,
        Theory::BrownianFractal,
        Theory::BrushFlick,
    ];
}

fn rand(low: f32, high: f32) -> f32 {
    low + fastrand::f32() * (high - low)
}

fn polar(angle: f32, distance: f32) -> Vec2 {
    Vec2::new(angle.cos(), angle.sin()) * distance
}

/// Ping-pong canvases with their mip views and bind groups.
struct Canvases {
    size: (u32, u32),
    /// Full-chain views, for rendering into level 0 we use `levels[i][0]`.
    levels: [Vec<wgpu::TextureView>; 2],
    /// Samples canvas i (all mips): dry, stamp and present passes.
    full_groups: [wgpu::BindGroup; 2],
    /// `level_groups[i][l]` samples only level l of canvas i, for building level l + 1.
    level_groups: [Vec<wgpu::BindGroup>; 2],
}

pub struct SplatterScene {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    dry_pipeline: wgpu::RenderPipeline,
    stamp_pipeline: wgpu::RenderPipeline,
    mip_pipeline: wgpu::RenderPipeline,
    present_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    stamp_buffer: wgpu::Buffer,
    canvases: Option<Canvases>,
    current: usize,
    needs_clear: bool,

    stamps: Vec<Stamp>,
    drips: Vec<Drip>,
    drip_cursor: usize,
    audio: SceneAudio,
    time: f64,
    aspect: f32,
    last_hits: BandLevels,
    last_splat: Vec2,
    last_splat_radius: f32,
    color_base: f64,
    mist_budget: f64,
    ambient_budget: f64,
    fast_level: f64,
    slow_level: f64,
    last_pour: f64,
    /// Seconds left of fast drying after a pour.
    wipe: f64,
    last_theory: Option<Theory>,
}

impl SplatterScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("splatter.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("splatter"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("splatter"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let stamp_attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
        let stamp_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Stamp>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &stamp_attributes,
        }];
        // Color goes "over" the old paint; wetness (alpha) keeps whichever is wetter.
        let paint_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Max,
            },
        };
        let pipeline = |label, vertex, fragment, target, blend, stamps: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: if stamps { &stamp_buffers } else { &[] },
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: target, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: if stamps {
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
        SplatterScene {
            dry_pipeline: pipeline("splatter dry", "fullscreen", "dry_fragment", CANVAS_FORMAT, None, false),
            stamp_pipeline: pipeline("splatter stamps", "stamp_vertex", "stamp_fragment", CANVAS_FORMAT, Some(paint_blend), true),
            mip_pipeline: pipeline("splatter mips", "fullscreen", "downsample", CANVAS_FORMAT, None, false),
            present_pipeline: pipeline("splatter present", "fullscreen", "present_fragment", format, None, false),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("splatter"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
            uniform_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("splatter uniforms"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            stamp_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("splatter stamps"),
                size: (MAX_STAMPS_PER_FRAME * std::mem::size_of::<Stamp>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            canvases: None,
            current: 0,
            needs_clear: true,
            stamps: Vec::with_capacity(MAX_STAMPS_PER_FRAME),
            drips: vec![Drip::default(); 256],
            drip_cursor: 0,
            audio: SceneAudio::default(),
            time: 0.0,
            aspect: 1.6,
            last_hits: BandLevels::ZERO,
            last_splat: Vec2::ZERO,
            last_splat_radius: 0.1,
            color_base: fastrand::f64() * 6.0,
            mist_budget: 0.0,
            ambient_budget: 0.0,
            fast_level: 0.0,
            slow_level: 0.0,
            last_pour: -100.0,
            wipe: 0.0,
            last_theory: None,
        }
    }

    fn random_spot(&self) -> Vec2 {
        Vec2::new(rand(-0.85, 0.85) * self.aspect, rand(-0.8, 0.85))
    }

    fn pick_color(&self) -> Vec3 {
        let offset = fastrand::usize(..GROUP_SIZE.min(SWATCHES.len()));
        SWATCHES[(self.color_base as usize + offset) % SWATCHES.len()]
    }

    /// The group's swatch, blended a random amount toward a neighbor and brightened or dimmed a little.
    fn unique_color(&self) -> Vec3 {
        self.pick_color().lerp(self.pick_color(), rand(0.0, 0.45)) * rand(0.88, 1.1)
    }

    /// Picks one of `options`, never the same theory twice in a row.
    fn pick(&mut self, options: &[Theory]) -> Theory {
        let fresh: Vec<Theory> = options.iter().copied().filter(|&t| Some(t) != self.last_theory).collect();
        let pool = if fresh.is_empty() { options.to_vec() } else { fresh };
        let theory = pool[fastrand::usize(..pool.len())];
        self.last_theory = Some(theory);
        theory
    }

    fn paint(&mut self, theory: Theory, center: Vec2, r: f32, mid: f64, energy: f64) {
        let color = self.unique_color();
        let spin = rand(0.0, TAU);
        let seed = rand(0.0, 1000.0);
        match theory {
            Theory::Superstring => {
                // A closed string vibrating in 6...16 overtones; a steep spectrum is smooth, a shallow one jagged.
                let tilt = rand(0.9, 2.2);
                let modes = fastrand::i32(6..=16);
                self.blob(0.0, center, r, spin, color, seed, [tilt, modes as f32]);
                self.harmonic_spray(center, r, color, modes, seed, 10 + (16.0 * energy) as usize);
                self.drips_from(center, r, color, mid);
            }
            Theory::QuantumOrbital => {
                let m = fastrand::i32(1..=4);
                let sharpness = rand(0.6, 3.0);
                self.blob(2.0, center, r, spin, color, seed, [sharpness, m as f32]);
                let cloud_color = self.unique_color();
                self.probability_cloud(center, r, cloud_color, m, seed + spin, 50 + (60.0 * energy) as usize);
                self.drips_from(center, r * 0.6, color, mid * 0.6);
            }
            Theory::EinsteinRing => {
                self.blob(3.0, center, r, spin, color, seed, [rand(0.7, 1.4), 0.0]);
                self.lensed_arcs(center, r, color, seed);
                self.drips_from(center, r * 0.3, color, mid);
            }
            Theory::Inflation => {
                self.blob(4.0, center, r * 1.2, spin, color, seed, [rand(0.05, 0.25), rand(1.4, 3.2)]);
                self.power_law_speckles(center, r, color, 40 + (50.0 * energy) as usize);
                self.drips_from(center, r * 0.8, color, mid);
            }
            Theory::KerrBlackHole => {
                let arms = fastrand::i32(2..=5);
                let twist = rand(1.2, 3.5) * if fastrand::bool() { 1.0 } else { -1.0 };
                self.blob(5.0, center, r * 1.15, spin, color, seed, [arms as f32, twist]);
                self.spiral_stream(center, r, color, arms, twist, seed, 30 + (30.0 * energy) as usize);
            }
            // Line-based theories need more room than a blob to read clearly.
            Theory::LorenzAttractor => self.lorenz(center, r * 2.1, color, spin),
            Theory::FeynmanDiagram => {
                let line_color = self.unique_color();
                self.feynman(center, r * 1.8, color, line_color, spin, mid);
            }
            Theory::DoubleSlit => self.interference(center, r * 1.5, color, spin, 90 + (70.0 * energy) as usize),
            Theory::BrownianFractal => {
                self.blob(0.0, center, r * 0.8, spin, color, seed, [1.6, 9.0]);
                self.brownian_branches(center, r, color);
                self.drips_from(center, r * 0.8, color, mid);
            }
            Theory::BrushFlick => self.flick(center, color, energy),
        }
        self.last_splat = center;
        self.last_splat_radius = r;
    }

    // Theory painters

    #[allow(clippy::too_many_arguments)]
    fn blob(&mut self, kind: f32, center: Vec2, radius: f32, spin: f32, color: Vec3, seed: f32, params: [f32; 2]) {
        self.add_stamp(Stamp {
            shape: [center.x, center.y, radius, spin],
            color: [color.x, color.y, color.z, seed],
            style: [kind, 1.0, params[0], params[1]],
        });
    }

    fn droplet(&mut self, position: Vec2, radius: f32, color: Vec3, angle: f32, stretch: f32) {
        self.add_stamp(Stamp {
            shape: [position.x, position.y, radius.max(0.0012), angle],
            color: [color.x, color.y, color.z, 0.0],
            style: [1.0, stretch, 0.0, 0.0],
        });
    }

    fn dot(&mut self, position: Vec2, radius: f32, color: Vec3) {
        self.droplet(position, radius, color, 0.0, 1.0);
    }

    /// Droplets flung outward, most often along the string's strongest overtone.
    fn harmonic_spray(&mut self, center: Vec2, r: f32, color: Vec3, modes: i32, phase: f32, count: usize) {
        let n = fastrand::i32(2..=(modes / 2).max(2)) as f32;
        let mut placed = 0;
        while placed < count {
            let angle = rand(0.0, TAU);
            if fastrand::f32() >= 0.35 + 0.65 * (0.5 + 0.5 * (n * angle + phase).sin()) {
                continue;
            }
            let distance = r * rand(1.1, 2.8);
            let size = r * rand(0.03, 0.12) * (2.8 - distance / r) / 1.7;
            self.droplet(center + polar(angle, distance), size, color, angle, rand(1.3, 3.2));
            placed += 1;
        }
    }

    /// |ψ|² for a hydrogen-like state in the plane: r^2l e^(-2r/n) cos²(mθ), sampled by rejection.
    fn probability_cloud(&mut self, center: Vec2, r: f32, color: Vec3, m: i32, phase: f32, count: usize) {
        let (n, l) = ((m + 1) as f32, m as f32);
        let peak = (l * n).powf(2.0 * l) * (-2.0 * l).exp(); // the radial density's maximum, at ρ = l·n
        let (mut placed, mut attempts) = (0, 0);
        while placed < count && attempts < count * 40 {
            attempts += 1;
            let rho = rand(0.0, 4.0 * n);
            let theta = rand(0.0, TAU);
            let density = rho.powf(2.0 * l) * (-2.0 * rho / n).exp() * (m as f32 * (theta - phase)).cos().powi(2);
            if rand(0.0, peak.max(1e-6)) >= density {
                continue;
            }
            self.dot(center + polar(theta, r * 0.55 * rho / n), rand(0.0013, 0.0032), color);
            placed += 1;
        }
    }

    /// Secondary lensed images: short bright arcs at wider radii along the lens axis.
    fn lensed_arcs(&mut self, center: Vec2, r: f32, color: Vec3, axis: f32) {
        for image in 0..fastrand::i32(2..=4) {
            let base = axis + if image % 2 == 0 { 0.0 } else { PI } + rand(-0.4, 0.4);
            let distance = r * rand(1.35, 2.1);
            let span = rand(0.25, 0.7);
            let steps = (distance * span / 0.004) as usize + 4;
            for i in 0..steps {
                let t = i as f32 / (steps - 1) as f32;
                let angle = base - span / 2.0 + span * t;
                let taper = (PI * t).sin();
                self.dot(center + polar(angle, distance), r * 0.05 * taper + 0.0012, color);
            }
        }
    }

    /// Primordial-style speckles: Gaussian scatter with power-law sizes (lots of small, a few large).
    fn power_law_speckles(&mut self, center: Vec2, r: f32, color: Vec3, count: usize) {
        for _ in 0..count {
            let u = rand(0.02, 1.0);
            let v = rand(0.0, TAU);
            let distance = r * 1.4 * (-2.0 * rand(0.0001, 1.0).ln()).sqrt();
            let size = (0.0012 * u.powf(-0.65)).min(r * 0.12);
            self.dot(center + polar(v, distance), size, color);
        }
    }

    /// Matter flung outward along logarithmic spirals, stretched along the flow.
    #[allow(clippy::too_many_arguments)]
    fn spiral_stream(&mut self, center: Vec2, r: f32, color: Vec3, arms: i32, twist: f32, phase: f32, count: usize) {
        for _ in 0..count {
            let arm = fastrand::i32(0..arms) as f32;
            let distance = r * rand(1.0, 2.6);
            let angle = (arm * TAU - phase) / arms as f32 + twist * (distance / r).ln() + rand(-0.08, 0.08);
            let tangent = angle + PI / 2.0 * if twist > 0.0 { 1.0 } else { -1.0 };
            let size = r * rand(0.02, 0.06) * (2.8 - distance / r);
            self.droplet(center + polar(angle, distance), size, color, tangent, rand(1.5, 3.5));
        }
    }

    /// A Lorenz-attractor orbit from a random start with randomized ρ and β, projected onto a random plane.
    fn lorenz(&mut self, center: Vec2, r: f32, color: Vec3, spin: f32) {
        let (sigma, rho, beta) = (10.0f32, rand(24.0, 32.0), rand(2.4, 2.9));
        let mut p = Vec3::new(rand(-5.0, 5.0), rand(-5.0, 5.0), rand(10.0, 30.0));
        let dt = 0.008;
        let derivative = |p: Vec3| Vec3::new(sigma * (p.y - p.x), p.x * (rho - p.z) - p.y, p.x * p.y - beta * p.z);
        for _ in 0..250 {
            p += derivative(p) * dt; // settle onto the attractor
        }
        let plane = fastrand::i32(0..=1);
        let scale = r * 1.3 / 25.0;
        let (c, sn) = (spin.cos(), spin.sin());
        for step in 0..640 {
            let velocity = derivative(p);
            p += velocity * dt;
            if step % 4 != 0 {
                continue;
            }
            let flat = if plane == 0 { Vec2::new(p.x, p.z - rho + 3.0) } else { Vec2::new(p.y, p.z - rho + 3.0) };
            let rotated = Vec2::new(c * flat.x - sn * flat.y, sn * flat.x + c * flat.y) * scale;
            let speed = (velocity.length() / 160.0).min(1.0);
            self.dot(center + rotated, r * (0.018 + 0.04 * (1.0 - speed)), color);
        }
    }

    /// Two vertices joined by a wavy photon propagator, four fermion legs, and sometimes a vacuum-polarization loop.
    fn feynman(&mut self, center: Vec2, r: f32, color: Vec3, line_color: Vec3, spin: f32, mid: f64) {
        let half = polar(spin, r * rand(0.7, 1.0));
        let (a, b) = (center - half, center + half);
        let normal = Vec2::new(-spin.sin(), spin.cos());
        let waves = fastrand::i32(3..=6) as f32;
        let amplitude = r * rand(0.1, 0.2);
        for i in 0..=48 {
            let t = i as f32 / 48.0;
            self.dot(a + (b - a) * t + normal * (t * waves * TAU).sin() * amplitude, r * 0.035, line_color);
        }
        for (vertex, outward) in [(a, spin + PI), (b, spin)] {
            self.blob(0.0, vertex, r * 0.26, rand(0.0, TAU), color, rand(0.0, 1000.0), [1.8, 8.0]);
            for side in [-1.0, 1.0] {
                let angle = outward + side * rand(0.45, 0.9);
                let length = r * rand(0.9, 1.5);
                for i in 1..=20 {
                    self.dot(vertex + polar(angle, length * i as f32 / 20.0), r * 0.03, color);
                }
            }
            self.drips_from(vertex, r * 0.25, color, mid);
        }
        if fastrand::bool() {
            let loop_center = center + normal * r * 0.55;
            let loop_radius = r * 0.25;
            for i in 0..28 {
                self.dot(loop_center + polar(i as f32 / 28.0 * TAU, loop_radius), r * 0.028, line_color);
            }
        }
    }

    /// Droplets landing in double-slit fringes: cos² fringes under a sinc² envelope, sampled by rejection.
    fn interference(&mut self, center: Vec2, r: f32, color: Vec3, spin: f32, count: usize) {
        let spacing = r * rand(0.22, 0.45);
        let envelope = r * rand(1.0, 1.6);
        let (c, sn) = (spin.cos(), spin.sin());
        let (mut placed, mut attempts) = (0, 0);
        while placed < count && attempts < count * 30 {
            attempts += 1;
            let x = rand(-2.6, 2.6) * r;
            let u = PI * x / envelope;
            let sinc = if u.abs() < 1e-4 { 1.0 } else { u.sin() / u };
            if fastrand::f32() >= (PI * x / spacing).cos().powi(2) * sinc * sinc {
                continue;
            }
            let y = rand(-0.6, 0.6) * r;
            self.dot(center + Vec2::new(c * x - sn * y, sn * x + c * y), rand(0.0014, 0.0034), color);
            placed += 1;
        }
    }

    /// Branching random walks growing outward from the rim, like diffusion-limited aggregation.
    fn brownian_branches(&mut self, center: Vec2, r: f32, color: Vec3) {
        fn grow(scene: &mut SplatterScene, start: Vec2, heading: f32, size: f32, depth: u32, r: f32, color: Vec3) {
            let (mut position, mut direction, mut width) = (start, heading, size);
            for _ in 0..fastrand::i32(14..=30) {
                direction += rand(-0.45, 0.45);
                position += polar(direction, r * 0.055);
                width *= 0.95;
                scene.dot(position, width, color);
                if depth < 2 && fastrand::f32() < 0.07 {
                    grow(scene, position, direction + rand(-0.9, 0.9), width * 0.8, depth + 1, r, color);
                }
            }
        }
        for _ in 0..fastrand::i32(5..=9) {
            let angle = rand(0.0, TAU);
            grow(self, center + polar(angle, r * 0.7), angle, r * 0.07, 0, r, color);
        }
    }

    /// A flick of the brush: a curved line of droplets that shrink along the arc.
    fn flick(&mut self, start: Vec2, color: Vec3, energy: f64) {
        let heading = rand(0.0, TAU);
        let bend = rand(-1.2, 1.2);
        let length = (0.35 + 0.5 * energy) as f32;
        let count = 16 + (24.0 * energy) as usize;
        for i in 0..count {
            let t = i as f32 / (count - 1) as f32;
            let angle = heading + bend * t;
            let jitter = Vec2::new(rand(-1.0, 1.0), rand(-1.0, 1.0)) * 0.008;
            let size = rand(0.002, 0.006) * (1.2 - t);
            self.droplet(start + polar(angle, length * t) + jitter, size, color, angle, rand(1.0, 2.2));
        }
    }

    fn drips_from(&mut self, center: Vec2, radius: f32, color: Vec3, mid: f64) {
        for _ in 0..(fastrand::f64() + mid * 3.5) as usize {
            let x = center.x + radius * rand(-0.7, 0.7);
            let start = Vec2::new(x, center.y - radius * rand(0.5, 0.85));
            self.start_drip(start, radius * rand(0.06, 0.14), color, mid as f32);
        }
    }

    /// A drop: a cluster of four different theories at once, and the old paint dries off faster to make room.
    fn pour(&mut self, mid: f64) {
        self.wipe = 1.2;
        let center = Vec2::new(rand(-0.4, 0.4) * self.aspect, rand(-0.2, 0.3));
        let mut theories: Vec<Theory> = Theory::ALL.into_iter().filter(|&t| t != Theory::BrushFlick).collect();
        fastrand::shuffle(&mut theories);
        for theory in theories.into_iter().take(4) {
            let offset = Vec2::new(rand(-0.3, 0.3), rand(-0.22, 0.22));
            self.paint(theory, center + offset, rand(0.1, 0.18), mid.max(0.8), 1.0);
        }
    }

    fn start_drip(&mut self, position: Vec2, radius: f32, color: Vec3, mid: f32) {
        self.drips[self.drip_cursor] = Drip {
            position,
            speed: rand(0.05, 0.12) * (0.6 + 1.4 * mid),
            radius: radius.max(0.002),
            remaining: rand(0.08, 0.45) * (0.5 + mid),
            color,
        };
        self.drip_cursor = (self.drip_cursor + 1) % self.drips.len();
    }

    /// Drips run down, slowing and thinning. Each frame they stamp along their path, so the trail is unbroken.
    fn update_drips(&mut self, dt: f32) {
        for i in 0..self.drips.len() {
            let mut drip = self.drips[i];
            if drip.remaining <= 0.0 {
                continue;
            }
            let travel = (drip.speed * dt).min(drip.remaining);
            let steps = ((travel / (drip.radius * 0.6)).ceil() as usize).max(1);
            for step in 1..=steps {
                let y = drip.position.y - travel * step as f32 / steps as f32;
                self.dot(Vec2::new(drip.position.x, y), drip.radius, drip.color);
            }
            drip.position.y -= travel;
            drip.remaining -= travel;
            drip.speed *= 1.0 - 0.35 * dt;
            drip.radius *= 1.0 - 0.25 * dt;
            // A bead forms at the end of the run.
            if drip.remaining <= 0.0 {
                self.dot(drip.position, drip.radius * 1.5, drip.color);
            }
            self.drips[i] = drip;
        }
    }

    fn add_stamp(&mut self, stamp: Stamp) {
        if self.stamps.len() < MAX_STAMPS_PER_FRAME {
            self.stamps.push(stamp);
        }
    }

    fn make_canvases(&self, device: &wgpu::Device, width: u32, height: u32) -> Canvases {
        let mip_count = 32 - width.max(height).leading_zeros();
        let make = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("splatter canvas"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: mip_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: CANVAS_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = [make(), make()];
        let group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("splatter"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniform_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            })
        };
        let levels = textures.each_ref().map(|texture| {
            (0..mip_count)
                .map(|level| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        base_mip_level: level,
                        mip_level_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>()
        });
        let full_groups = textures.each_ref().map(|t| group(&t.create_view(&Default::default())));
        let level_groups = levels.each_ref().map(|views| views.iter().map(&group).collect());
        Canvases { size: (width, height), levels, full_groups, level_groups }
    }
}

impl WindowScene for SplatterScene {
    fn update(&mut self, audio: SceneAudio, dt: f64) {
        self.audio = audio;
        self.time += dt;
        let (levels, hits) = (audio.levels, audio.hits);
        let (low, mid, high) = (levels.low as f64, levels.mid as f64, levels.high as f64);
        let struck = |band: usize| hits[band] > 0.9 && self.last_hits[band] <= 0.9;
        let (kick, mid_hit, high_hit) = (struck(0), struck(1), struck(2));
        self.last_hits = hits;

        self.color_base += dt * (0.02 + 0.12 * mid);

        let loudness = levels.loudest() as f64;
        self.fast_level += (loudness - self.fast_level) * (1.0 - (-dt / 2.0).exp());
        self.slow_level += (loudness - self.slow_level) * (1.0 - (-dt / 8.0).exp());
        if self.fast_level > 0.6 && self.slow_level < 0.4 && self.time - self.last_pour > 12.0 {
            self.last_pour = self.time;
            self.pour(mid);
        }

        // Heavy, cosmic theories on the kicks; quantum and particle physics on the mids; chaos and waves on the highs.
        use Theory::*;
        if kick {
            let theory = self.pick(&[EinsteinRing, Inflation, KerrBlackHole, Superstring, BrownianFractal]);
            let spot = self.random_spot();
            self.paint(theory, spot, (0.05 + 0.09 * low + 0.03 * hits.low as f64) as f32, mid, low);
        }
        if mid_hit {
            let theory = self.pick(&[QuantumOrbital, FeynmanDiagram, Superstring, BrownianFractal, Inflation]);
            let spot = self.random_spot();
            self.paint(theory, spot, (0.035 + 0.045 * mid) as f32, mid, mid);
        }
        if high_hit {
            let theory = self.pick(&[LorenzAttractor, DoubleSlit, BrushFlick]);
            let spot = self.random_spot();
            self.paint(theory, spot, (0.05 + 0.06 * high) as f32, mid, high);
        }

        // Even without hits, sustained mids keep a light patter of small splats going.
        self.ambient_budget += dt * 0.3 * mid;
        if self.ambient_budget >= 1.0 {
            self.ambient_budget -= 1.0;
            let theory = self.pick(&[Superstring, QuantumOrbital, Inflation, BrownianFractal]);
            let spot = self.random_spot();
            self.paint(theory, spot, (0.02 + 0.025 * mid) as f32, mid, 0.3);
        }
        self.mist_budget += dt * 50.0 * high * high;
        while self.mist_budget >= 1.0 {
            self.mist_budget -= 1.0;
            let angle = rand(0.0, TAU);
            let distance = self.last_splat_radius * rand(1.2, 4.0);
            let color = self.pick_color();
            self.dot(self.last_splat + polar(angle, distance), rand(0.0012, 0.003), color);
        }

        self.update_drips(dt as f32);
        self.wipe = (self.wipe - dt).max(0.0);
    }

    fn reseed(&mut self) {
        self.needs_clear = true;
        for drip in &mut self.drips {
            drip.remaining = 0.0;
        }
    }

    fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        show_meters: bool,
    ) {
        self.aspect = size.0 as f32 / size.1.max(1) as f32;
        let (width, height) = (size.0.max(16), size.1.max(16));
        if self.canvases.as_ref().is_none_or(|c| c.size != (width, height)) {
            self.canvases = Some(self.make_canvases(device, width, height));
            self.current = 0;
            self.needs_clear = true;
        }
        // Paint dries in a few tenths of a second and loses half its brightness in about 4 s, keeping plenty of
        // black canvas showing; a pour fades it much faster.
        let color_decay = if self.wipe > 0.0 { 0.95 } else { 0.997 };
        let uniforms = Uniforms {
            view: [width as f32, height as f32, self.aspect, self.time as f32],
            dry: [color_decay, 0.965, 0.85 + 0.7 * self.audio.levels.low, if show_meters { 1.0 } else { 0.0 }],
            bands: [self.audio.levels.low, self.audio.levels.mid, self.audio.levels.high, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        if !self.stamps.is_empty() {
            queue.write_buffer(&self.stamp_buffer, 0, bytemuck::cast_slice(&self.stamps));
        }

        let canvases = self.canvases.as_ref().unwrap();
        let (source, destination) = (self.current, 1 - self.current);
        {
            // Cleared to transparent black; the dry pass then copies the old paint over every pixel.
            let mut pass = color_pass_clear(encoder, "splatter paint", &canvases.levels[destination][0]);
            pass.set_bind_group(0, &canvases.full_groups[source], &[]);
            if !self.needs_clear {
                pass.set_pipeline(&self.dry_pipeline);
                pass.draw(0..3, 0..1);
            }
            if !self.stamps.is_empty() {
                pass.set_pipeline(&self.stamp_pipeline);
                pass.set_vertex_buffer(0, self.stamp_buffer.slice(..));
                pass.draw(0..4, 0..self.stamps.len() as u32);
            }
        }
        self.stamps.clear();
        self.needs_clear = false;

        for level in 1..canvases.levels[destination].len() {
            let mut pass = color_pass(encoder, "splatter mip", &canvases.levels[destination][level]);
            pass.set_pipeline(&self.mip_pipeline);
            pass.set_bind_group(0, &canvases.level_groups[destination][level - 1], &[]);
            pass.draw(0..3, 0..1);
        }

        let mut pass = color_pass(encoder, "splatter present", target);
        pass.set_pipeline(&self.present_pipeline);
        pass.set_bind_group(0, &canvases.full_groups[destination], &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.current = destination;
    }
}

/// A pass that starts from transparent black (the canvas's "no paint").
fn color_pass_clear<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}
