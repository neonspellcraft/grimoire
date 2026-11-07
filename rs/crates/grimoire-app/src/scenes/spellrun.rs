//! Spellrun: a generated pixel platformer that the music plays (port of `SpellrunScene.swift`).
//!
//! The game and its renderer live in grimoire-core. This scene turns the band envelopes and hits into game
//! input, renders a low-resolution frame on the CPU, and has the GPU scale it up by a whole number so every
//! game pixel stays perfectly sharp.
//!
//! - LOW: the apprentice learns the tempo from the kicks and hops on the beat, each hop lasting exactly one beat
//!   so he lands on the next kick; big kicks mid-air become ground pounds; lanterns, rune blocks and the sun or
//!   moon glow with the bass.
//! - MID: run speed and how lively the generated terrain is (gaps, hills, platforms, blocks).
//! - HIGH: every hi-hat transient casts a spell that seeks the nearest enemy (quick runs fan out three bolts);
//!   motes drop out of the air, stars twinkle, and fireflies, spores and dust glow.
//! - A drop (loud after quiet): a nova of bolts in every direction and, soon after, a new world.

use grimoire_core::spellrun::{PixelCanvas, SpellrunGame, SpellrunInput, SpellrunRenderer};

use super::{color_pass, fullscreen_pipeline, SceneAudio, WindowScene};

/// About how many game pixels tall the view is; the exact size follows from a whole-number scale.
const TARGET_HEIGHT: f64 = 160.0;

struct Frame {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
}

pub struct SpellrunScene {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    frame: Option<Frame>,
    game: SpellrunGame,
    renderer: SpellrunRenderer,
    canvas: PixelCanvas,
    scale: u32,
    audio: SceneAudio,
    last_low_hit: f32,
    last_high_hit: f32,
    fast_level: f64,
    slow_level: f64,
}

impl SpellrunScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("spellrun.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("spellrun"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("spellrun"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        SpellrunScene {
            pipeline: fullscreen_pipeline(device, "spellrun", &module, &pipeline_layout, "present", format),
            layout,
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("spellrun uniforms"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            frame: None,
            game: SpellrunGame::new(fastrand::u64(..)),
            renderer: SpellrunRenderer::new(),
            canvas: PixelCanvas::new(256, 160),
            scale: 5,
            audio: SceneAudio::default(),
            last_low_hit: 0.0,
            last_high_hit: 0.0,
            fast_level: 0.0,
            slow_level: 0.0,
        }
    }

    /// Picks a whole-number scale near 160 game pixels tall, so each game pixel is an exact square of screen pixels.
    fn resize_if_needed(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        let scale = ((size.1 as f64 / TARGET_HEIGHT).round() as u32).max(2);
        let width = ((size.0 as f64 / scale as f64).ceil() as u32).max(64);
        let height = ((size.1 as f64 / scale as f64).ceil() as u32).max(64);
        let same = self.canvas.width == width as i64 && self.canvas.height == height as i64 && self.scale == scale;
        if self.frame.is_some() && same {
            return;
        }
        self.scale = scale;
        self.canvas = PixelCanvas::new(width as usize, height as usize);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("spellrun frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spellrun"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
                },
            ],
        });
        self.frame = Some(Frame { texture, group });
    }
}

impl WindowScene for SpellrunScene {
    fn update(&mut self, audio: SceneAudio, dt: f64) {
        self.audio = audio;
        let kick = audio.hits.low > 0.9 && self.last_low_hit <= 0.9;
        let sparkle = audio.hits.high > 0.9 && self.last_high_hit <= 0.9;
        self.last_low_hit = audio.hits.low;
        self.last_high_hit = audio.hits.high;

        let loudness = audio.levels.loudest() as f64;
        self.fast_level += (loudness - self.fast_level) * (1.0 - (-dt / 2.0).exp());
        self.slow_level += (loudness - self.slow_level) * (1.0 - (-dt / 8.0).exp());
        let drop = self.fast_level > 0.6 && self.slow_level < 0.4;

        let input = SpellrunInput {
            low: audio.levels.low,
            mid: audio.levels.mid,
            high: audio.levels.high,
            kick,
            big_kick: kick && audio.levels.low > 0.75,
            sparkle,
            drop,
        };
        self.game.step(dt, input, self.canvas.width as f64, self.canvas.height as f64);
    }

    fn reseed(&mut self) {
        self.game = SpellrunGame::new(fastrand::u64(..));
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
        self.resize_if_needed(device, size);
        self.renderer.render(&self.game, &mut self.canvas, self.audio.levels, show_meters);
        let frame = self.frame.as_ref().unwrap();
        queue.write_texture(
            frame.texture.as_image_copy(),
            bytemuck::cast_slice(&self.canvas.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.canvas.width as u32 * 4),
                rows_per_image: None,
            },
            frame.texture.size(),
        );
        let info = [self.scale as f32, self.canvas.width as f32, self.canvas.height as f32, 0.0];
        queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&info));
        let mut pass = color_pass(encoder, "spellrun", target);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &frame.group, &[]);
        pass.draw(0..3, 0..1);
    }
}
