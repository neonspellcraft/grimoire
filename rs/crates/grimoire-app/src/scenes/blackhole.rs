//! Black Hole: a never-ending psychedelic fall into a black hole, with real gravitational lensing (see
//! blackhole.wgsl for the physics and for how each fall hands over seamlessly to the next).
//!
//! - LOW: the accretion disk and photon ring burn brighter; each kick sends a ripple of light out through the disk.
//! - MID: how fast you fall (and the zoom blur that comes with it), and how fast the disk turns.
//! - HIGH: the colours drift faster through the palette; stars twinkle.
//!
//! Tracing light through curved spacetime is heavy, so the frame is traced at a capped pixel count into a
//! floating-point texture, then scaled up, tone mapped and finished in a second, cheap pass.

use super::{color_pass, fullscreen_pipeline, SceneAudio, WindowScene};

/// Most pixels traced per frame; larger windows trace at a lower resolution and are scaled up.
const TRACE_BUDGET: f64 = 1_300_000.0;
const TRACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TraceUniforms {
    view: [f32; 4],
    ride: [f32; 4],
    mood: [f32; 4],
    look: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PresentUniforms {
    window: [f32; 4],
    meters: [f32; 4],
}

struct Target {
    size: (u32, u32),
    view: wgpu::TextureView,
    present_group: wgpu::BindGroup,
}

pub struct BlackHoleScene {
    trace_pipeline: wgpu::RenderPipeline,
    trace_group: wgpu::BindGroup,
    trace_uniforms: wgpu::Buffer,
    present_pipeline: wgpu::RenderPipeline,
    present_layout: wgpu::BindGroupLayout,
    present_uniforms: wgpu::Buffer,
    sampler: wgpu::Sampler,
    target: Option<Target>,
    time: f64,
    cycle: f64,
    phase: f64,
    hue: f64,
    disk_clock: f64,
    speed: f64,
    low: f64,
    kick: f64,
    audio: SceneAudio,
}

fn ease(value: &mut f64, target: f64, seconds: f64, dt: f64) {
    *value += (target - *value) * (1.0 - (-dt / seconds).exp());
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl BlackHoleScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let trace_module = device.create_shader_module(wgpu::include_wgsl!("blackhole.wgsl"));
        let present_module = device.create_shader_module(wgpu::include_wgsl!("blackhole_present.wgsl"));

        let trace_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("black hole trace"),
            entries: &[uniform_entry(0)],
        });
        let trace_uniforms = uniform_buffer(device, "black hole trace uniforms", std::mem::size_of::<TraceUniforms>());
        let trace_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("black hole trace"),
            layout: &trace_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: trace_uniforms.as_entire_binding() }],
        });
        let trace_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("black hole trace"),
            bind_group_layouts: &[&trace_layout],
            push_constant_ranges: &[],
        });

        let present_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("black hole present"),
            entries: &[
                uniform_entry(0),
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
        let present_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("black hole present"),
            bind_group_layouts: &[&present_layout],
            push_constant_ranges: &[],
        });

        BlackHoleScene {
            trace_pipeline: fullscreen_pipeline(device, "black hole trace", &trace_module, &trace_pipeline_layout, "trace_frame", TRACE_FORMAT),
            trace_group,
            trace_uniforms,
            present_pipeline: fullscreen_pipeline(device, "black hole present", &present_module, &present_pipeline_layout, "present", format),
            present_layout,
            present_uniforms: uniform_buffer(device, "black hole present uniforms", std::mem::size_of::<PresentUniforms>()),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("black hole"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            target: None,
            time: 0.0,
            cycle: (fastrand::u32(..1000)) as f64,
            phase: 0.15,
            hue: fastrand::f64(),
            disk_clock: 0.0,
            speed: 0.0,
            low: 0.0,
            kick: 0.0,
            audio: SceneAudio::default(),
        }
    }

    /// The traced frame's size for a window: the window's own size, scaled down to fit the pixel budget.
    fn trace_size(window: (u32, u32)) -> (u32, u32) {
        let pixels = window.0 as f64 * window.1 as f64;
        let scale = (TRACE_BUDGET / pixels.max(1.0)).sqrt().min(1.0);
        (((window.0 as f64 * scale).ceil() as u32).max(16), ((window.1 as f64 * scale).ceil() as u32).max(16))
    }

    fn ensure_target(&mut self, device: &wgpu::Device, window: (u32, u32)) {
        let size = Self::trace_size(window);
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("black hole frame"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TRACE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let present_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("black hole present"),
            layout: &self.present_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.present_uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.target = Some(Target { size, view, present_group });
    }
}

impl WindowScene for BlackHoleScene {
    fn update(&mut self, audio: SceneAudio, dt: f64) {
        self.audio = audio;
        self.time += dt;
        let (low, mid, high) = (audio.levels.low as f64, audio.levels.mid as f64, audio.levels.high as f64);
        ease(&mut self.low, low, 0.25, dt);
        ease(&mut self.speed, mid, 1.5, dt);
        self.kick = (self.kick - dt / 0.9).max(0.0).max(audio.hits.low as f64);
        // One fall takes about 40 s in quiet music and about 20 s when it's busy.
        self.phase += dt / 40.0 * (0.8 + 1.4 * self.speed);
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.cycle += 1.0;
        }
        self.hue += dt * (0.008 + 0.05 * high);
        self.disk_clock += dt * (0.6 + 0.8 * self.speed);
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
        self.ensure_target(device, size);
        let traced = self.target.as_ref().unwrap();
        let t = self.time;
        let trace = TraceUniforms {
            view: [traced.size.0 as f32, traced.size.1 as f32, traced.size.0 as f32 / traced.size.1 as f32, (t % 3600.0) as f32],
            ride: [(self.cycle % 10_000.0) as f32, self.phase as f32, (self.hue % 1000.0) as f32, (self.disk_clock % 100_000.0) as f32],
            mood: [self.low as f32, self.speed as f32, self.audio.levels.high, self.kick as f32],
            look: [((t * 0.13).sin() * 0.10) as f32, ((t * 0.09).cos() * 0.07) as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.trace_uniforms, 0, bytemuck::bytes_of(&trace));
        let levels = self.audio.levels;
        let present = PresentUniforms {
            // The zoom blur grows as the fall speeds up and peaks mid-fall (zero at the handover, so it never jumps).
            window: [size.0 as f32, size.1 as f32, (0.006 + 0.02 * self.speed + 0.012 * (1.0 - (self.phase * std::f64::consts::TAU).cos())) as f32, (0.01 + 0.02 * self.kick) as f32],
            meters: [levels.low, levels.mid, levels.high, if show_meters { 1.0 } else { 0.0 }],
        };
        queue.write_buffer(&self.present_uniforms, 0, bytemuck::bytes_of(&present));
        {
            let mut pass = color_pass(encoder, "black hole trace", &traced.view);
            pass.set_pipeline(&self.trace_pipeline);
            pass.set_bind_group(0, &self.trace_group, &[]);
            pass.draw(0..3, 0..1);
        }
        let mut pass = color_pass(encoder, "black hole present", target);
        pass.set_pipeline(&self.present_pipeline);
        pass.set_bind_group(0, &traced.present_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn reseed(&mut self) {
        // A different black hole, from the top of its fall.
        self.cycle = fastrand::u32(..1000) as f64;
        self.phase = 0.0;
        self.hue = fastrand::f64();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_size_fits_the_budget() {
        assert_eq!(BlackHoleScene::trace_size((1000, 800)), (1000, 800));
        let (w, h) = BlackHoleScene::trace_size((3840, 2160));
        assert!((w as f64 * h as f64) <= TRACE_BUDGET * 1.01);
        assert!(((w as f64 / h as f64) - 3840.0 / 2160.0).abs() < 0.01);
    }
}
