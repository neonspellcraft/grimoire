//! Night Clouds: watching clouds pass on a dark night. Chill and dim on purpose; the music only nudges it.
//!
//! - LOW: a slow swell in how much of the sky the clouds cover (smoothed over seconds, so it breathes).
//! - MID: the drift speeds up a little, and the moonlit cloud edges shimmer.
//! - HIGH: the stars twinkle.
//! - Kicks: the moon's glow brightens softly and fades over a few seconds.

use super::{fullscreen_pipeline, SceneAudio, WindowScene};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view: [f32; 4],
    drift: [f32; 4],
    mood: [f32; 4],
    meters: [f32; 4],
}

pub struct CloudsScene {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    time: f64,
    far: [f64; 2],
    near: [f64; 2],
    swell: f64,
    shimmer: f64,
    twinkle: f64,
    glow: f64,
    audio: SceneAudio,
}

/// Moves `value` toward `target` with time constant `seconds`.
fn ease(value: &mut f64, target: f64, seconds: f64, dt: f64) {
    *value += (target - *value) * (1.0 - (-dt / seconds).exp());
}

impl CloudsScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("clouds.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clouds"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clouds"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("clouds uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clouds"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        CloudsScene {
            pipeline: fullscreen_pipeline(device, "clouds", &module, &pipeline_layout, "sky", format),
            group,
            uniforms,
            time: 0.0,
            far: [fastrand::f64() * 50.0, fastrand::f64() * 50.0],
            near: [fastrand::f64() * 50.0, fastrand::f64() * 50.0],
            swell: 0.0,
            shimmer: 0.0,
            twinkle: 0.0,
            glow: 0.0,
            audio: SceneAudio::default(),
        }
    }
}

impl WindowScene for CloudsScene {
    fn update(&mut self, audio: SceneAudio, dt: f64) {
        self.audio = audio;
        self.time += dt;
        let (low, mid, high) = (audio.levels.low as f64, audio.levels.mid as f64, audio.levels.high as f64);
        // Everything is smoothed over seconds: the sky responds to the music's mood, not its every beat.
        ease(&mut self.swell, low, 4.0, dt);
        ease(&mut self.shimmer, mid, 2.0, dt);
        ease(&mut self.twinkle, high, 1.0, dt);
        self.glow = (self.glow - dt / 3.0).max(0.0).max(audio.hits.low as f64 * 0.8);
        // Wind from the west: the near layer moves faster than the far one (parallax).
        let wind = 0.010 + 0.016 * self.shimmer;
        self.far[0] += dt * wind * 0.6;
        self.far[1] += dt * wind * 0.05;
        self.near[0] += dt * wind;
        self.near[1] += dt * wind * 0.08;
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
        let levels = self.audio.levels;
        let uniforms = Uniforms {
            view: [size.0 as f32, size.1 as f32, size.0 as f32 / size.1.max(1) as f32, self.time as f32],
            // Offsets wrap far from zero so float precision never degrades the noise on long nights.
            drift: [
                (self.far[0] % 1000.0) as f32,
                (self.far[1] % 1000.0) as f32,
                (self.near[0] % 1000.0) as f32,
                (self.near[1] % 1000.0) as f32,
            ],
            mood: [self.swell as f32, self.shimmer as f32, self.twinkle as f32, self.glow as f32],
            meters: [levels.low, levels.mid, levels.high, if show_meters { 1.0 } else { 0.0 }],
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut pass = super::color_pass(encoder, "clouds", target);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn reseed(&mut self) {
        // A different patch of sky.
        self.far = [fastrand::f64() * 500.0, fastrand::f64() * 500.0];
        self.near = [fastrand::f64() * 500.0, fastrand::f64() * 500.0];
    }
}
