//! The three-body problem as a music-driven light show (port of `ThreeBodyScene.swift`).
//!
//! Each body belongs to one band: low, mid and high. Its mass follows that band, bass also pumps the strength
//! of gravity, and the mids set how fast simulated time runs. The figure-eight orbit drifts into chaos as the
//! music pushes the masses around. The shader turns the bodies' potential field into flowing rings with feedback trails.

use grimoire_core::ThreeBodySystem;

use super::{color_pass, fullscreen_pipeline, SceneAudio, WindowScene};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view: [f32; 4],
    bodies: [[f32; 4]; 3],
    bands: [f32; 4],
    hits: [f32; 4],
    feedback: [f32; 4],
}

/// Ping-pong feedback buffers at half the drawable's resolution.
struct Feedback {
    width: u32,
    height: u32,
    views: [wgpu::TextureView; 2],
    /// `evolve_groups[i]` samples texture i; so does `present_groups[i]`.
    evolve_groups: [wgpu::BindGroup; 2],
    present_groups: [wgpu::BindGroup; 2],
}

pub struct ThreeBodyScene {
    evolve: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    evolve_uniforms: wgpu::Buffer,
    present_uniforms: wgpu::Buffer,
    feedback: Option<Feedback>,
    current: usize,

    system: ThreeBodySystem,
    /// Energy of the unmodulated figure-eight; the thermostat target scales from this.
    baseline_energy: f64,
    audio: SceneAudio,
    flow: f64,
    hue: f64,
    camera_scale: f64,
    frame_scale: f64,
}

const FEEDBACK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl ThreeBodyScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("three_body.wgsl"));
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("three-body"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                }),
                entry(1, wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }),
                entry(2, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("three-body"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let uniforms = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let system = fresh_system();
        ThreeBodyScene {
            evolve: fullscreen_pipeline(device, "three-body evolve", &module, &pipeline_layout, "evolve", FEEDBACK_FORMAT),
            present: fullscreen_pipeline(device, "three-body present", &module, &pipeline_layout, "present", format),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("three-body"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default() // clamp to edge
            }),
            layout,
            evolve_uniforms: uniforms("three-body evolve uniforms"),
            present_uniforms: uniforms("three-body present uniforms"),
            feedback: None,
            current: 0,
            baseline_energy: system.total_energy(),
            system,
            audio: SceneAudio::default(),
            flow: 0.0,
            hue: 0.0,
            camera_scale: 0.7,
            frame_scale: 1.0,
        }
    }

    fn make_feedback(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, width: u32, height: u32) -> Feedback {
        let make_view = || {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("three-body feedback"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FEEDBACK_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let views = [make_view(), make_view()];
        // New textures start cleared to black so the feedback starts from nothing.
        for view in &views {
            drop(color_pass(encoder, "three-body clear", view));
        }
        let group = |uniforms: &wgpu::Buffer, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("three-body"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            })
        };
        Feedback {
            width,
            height,
            evolve_groups: [group(&self.evolve_uniforms, &views[0]), group(&self.evolve_uniforms, &views[1])],
            present_groups: [group(&self.present_uniforms, &views[0]), group(&self.present_uniforms, &views[1])],
            views,
        }
    }

    fn uniforms(&self, resolution: [f32; 2], aspect: f32, show_meters: bool) -> Uniforms {
        let (levels, hits) = (self.audio.levels, self.audio.hits);
        let body = |i: usize| {
            let b = &self.system.bodies[i];
            [
                (b.position.x * self.camera_scale) as f32,
                (b.position.y * self.camera_scale) as f32,
                b.mass as f32,
                levels[i],
            ]
        };
        // Per-frame feedback amounts, normalized to 60 fps so the look doesn't depend on frame rate.
        let zoom = 1.0 - (0.0012 + 0.004 * levels.low as f64 + 0.02 * hits.low as f64) * self.frame_scale;
        let rotation = (0.001 + 0.006 * levels.mid as f64) * self.frame_scale;
        let decay = (0.90 - 0.06 * levels.high as f64).powf(self.frame_scale);
        Uniforms {
            view: [resolution[0], resolution[1], self.flow as f32, aspect],
            bodies: [body(0), body(1), body(2)],
            bands: [levels.low, levels.mid, levels.high, levels.loudest()],
            hits: [hits.low, hits.mid, hits.high, self.hue as f32],
            feedback: [zoom as f32, rotation as f32, decay as f32, if show_meters { 1.0 } else { 0.0 }],
        }
    }
}

/// The figure-eight, softened and in a gentle bowl so all three bodies keep dancing instead of decaying into
/// a tight pair and a loner (the usual fate of a chaotic three-body system).
fn fresh_system() -> ThreeBodySystem {
    let mut system = ThreeBodySystem::figure_eight();
    system.softening = 0.15;
    system.trap_strength = 1.0;
    system.max_kinetic_energy = 30.0;
    system
}

impl WindowScene for ThreeBodyScene {
    fn update(&mut self, audio: SceneAudio, dt: f64) {
        self.audio = audio;
        let (low, mid, high) = (audio.levels.low as f64, audio.levels.mid as f64, audio.levels.high as f64);

        let masses = [1.0 + 0.8 * low, 1.0 + 0.8 * mid, 1.0 + 0.8 * high];
        self.system.gravity = 1.0 + 0.9 * low + 0.5 * audio.hits.low as f64;
        self.system.set_masses(&masses);
        // Energy scales with G·m² at a fixed size, so this holds the orbit's size while the music pumps it.
        let mean_mass = masses.iter().sum::<f64>() / 3.0;
        self.system.target_energy = Some(self.baseline_energy * self.system.gravity * mean_mass * mean_mass);
        self.system.advance(dt * (0.45 + 0.9 * mid + 0.3 * high));
        let extent = self.system.extent();
        if extent > self.system.wall_radius * 1.8 || !extent.is_finite() {
            self.reseed();
        }

        self.flow += dt * (0.5 + 1.8 * mid + 1.2 * low);
        self.hue += dt * (0.015 + 0.12 * mid + 0.2 * audio.hits.mid as f64);
        // Keep the whole system in view, easing the zoom so it doesn't jitter.
        let target_scale = 0.8 / self.system.extent().max(0.3);
        self.camera_scale += (target_scale - self.camera_scale) * (dt * 1.2).min(1.0);
        self.frame_scale = dt * 60.0;
    }

    fn reseed(&mut self) {
        let mut next = fresh_system();
        next.rotate(fastrand::f64() * std::f64::consts::TAU);
        next.perturb(0.04);
        self.system = next;
        // Drop the feedback buffers so the old trails vanish and the restart is obvious.
        self.feedback = None;
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
        let width = (size.0 / 2).max(16);
        let height = (size.1 / 2).max(16);
        if self.feedback.as_ref().is_none_or(|f| f.width != width || f.height != height) {
            self.feedback = Some(self.make_feedback(device, encoder, width, height));
            self.current = 0;
        }
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let mut uniforms = self.uniforms([width as f32, height as f32], aspect, show_meters);
        queue.write_buffer(&self.evolve_uniforms, 0, bytemuck::bytes_of(&uniforms));
        uniforms.view[0] = size.0 as f32;
        uniforms.view[1] = size.1 as f32;
        queue.write_buffer(&self.present_uniforms, 0, bytemuck::bytes_of(&uniforms));

        let feedback = self.feedback.as_ref().unwrap();
        let (source, destination) = (self.current, 1 - self.current);
        {
            let mut pass = color_pass(encoder, "three-body evolve", &feedback.views[destination]);
            pass.set_pipeline(&self.evolve);
            pass.set_bind_group(0, &feedback.evolve_groups[source], &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = color_pass(encoder, "three-body present", target);
            pass.set_pipeline(&self.present);
            pass.set_bind_group(0, &feedback.present_groups[destination], &[]);
            pass.draw(0..3, 0..1);
        }
        self.current = destination;
    }
}
