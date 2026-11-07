//! A looping frame animation as a visualizer (it doesn't follow the music): the Wizard scene, from the
//! user's own artwork. Frames are JPEGs in a `.grma` file (see `scripts/make-animation.sh`), decoded one at a
//! time on a background thread so only a couple of frames are ever in memory. The picture fills the window,
//! trimming the edges rather than showing bars.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::{color_pass, fullscreen_pipeline, SceneAudio, WindowScene};

/// The Wizard: a hooded wizard reading a glowing grimoire by candle and firelight. Shipped next to the app (in
/// the macOS app's Resources, beside the Windows exe), or found in the source tree for development builds.
pub fn wizard_file() -> Result<std::path::PathBuf, String> {
    let name = "wizard.grma";
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf()));
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(dir) = exe_dir {
        candidates.push(dir.join(name));
        candidates.push(dir.join("../Resources").join(name));
    }
    candidates.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/animations").join(name));
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| format!("{name} isn't installed with this copy of Grimoire"))
}

/// A parsed `.grma`: frame timing and where each JPEG sits.
pub struct Animation {
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    frames: Vec<(usize, usize)>,
    data: Vec<u8>,
}

impl Animation {
    pub fn load(path: &std::path::Path) -> Result<Animation, String> {
        Self::parse(std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }

    pub fn parse(data: Vec<u8>) -> Result<Animation, String> {
        let u32_at = |offset: usize| {
            data.get(offset..offset + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or("animation file is truncated")
        };
        if data.get(0..4) != Some(b"GRMA") || u32_at(4)? != 1 {
            return Err("not a Grimoire animation".into());
        }
        let (milli_fps, width, height, count) = (u32_at(8)?, u32_at(12)?, u32_at(16)?, u32_at(20)? as usize);
        let mut offset = 24 + count * 4;
        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            let length = u32_at(24 + i * 4)? as usize;
            if offset + length > data.len() {
                return Err("animation file is truncated".into());
            }
            frames.push((offset, length));
            offset += length;
        }
        if frames.is_empty() || milli_fps == 0 {
            return Err("animation has no frames".into());
        }
        Ok(Animation { fps: milli_fps as f64 / 1000.0, width, height, frames, data })
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn decode(&self, index: usize) -> Option<image::RgbaImage> {
        let (offset, length) = self.frames[index % self.frames.len()];
        image::load_from_memory_with_format(&self.data[offset..offset + length], image::ImageFormat::Jpeg)
            .ok()
            .map(|frame| frame.to_rgba8())
    }
}

/// Shared between the scene and its decoder thread.
#[derive(Default)]
struct Decoded {
    /// The newest decoded frame and its index, waiting to be uploaded.
    frame: Mutex<Option<(usize, image::RgbaImage)>>,
    /// The frame the scene wants next.
    wanted: Mutex<Option<usize>>,
    wake: Condvar,
    stop: AtomicBool,
}

pub struct AnimationScene {
    animation: Arc<Animation>,
    decoded: Arc<Decoded>,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    texture: wgpu::Texture,
    shown: Option<usize>,
    start: Instant,
    /// Time spent hidden, so the loop resumes where it was.
    paused: Duration,
    hidden_since: Option<Instant>,
}

impl AnimationScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, path: &std::path::Path) -> Result<Self, String> {
        let animation = Arc::new(Animation::load(path)?);
        let decoded = Arc::new(Decoded::default());
        {
            let (animation, decoded) = (animation.clone(), decoded.clone());
            std::thread::Builder::new()
                .name("animation".into())
                .spawn(move || loop {
                    let index = {
                        let mut wanted = decoded.wanted.lock().unwrap();
                        while wanted.is_none() && !decoded.stop.load(Ordering::Relaxed) {
                            wanted = decoded.wake.wait(wanted).unwrap();
                        }
                        if decoded.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        wanted.take().unwrap()
                    };
                    if let Some(frame) = animation.decode(index) {
                        *decoded.frame.lock().unwrap() = Some((index, frame));
                    }
                })
                .map_err(|e| e.to_string())?;
        }

        let module = device.create_shader_module(wgpu::include_wgsl!("animation.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("animation"),
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
            label: Some("animation"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("animation frame"),
            size: wgpu::Extent3d { width: animation.width, height: animation.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Scenes write display values directly (a non-sRGB target), so the JPEG's colors go in as they are.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("animation uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("animation"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("animation"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
                },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        // Ask for the first frame right away so it's ready by the first draw.
        *decoded.wanted.lock().unwrap() = Some(0);
        decoded.wake.notify_one();
        Ok(AnimationScene {
            animation,
            decoded,
            pipeline: fullscreen_pipeline(device, "animation", &module, &pipeline_layout, "present", format),
            group,
            uniforms,
            texture,
            shown: None,
            start: Instant::now(),
            paused: Duration::ZERO,
            hidden_since: None,
        })
    }

    fn current_index(&self) -> usize {
        let hidden = self.hidden_since.map_or(Duration::ZERO, |t| t.elapsed());
        let playing = self.start.elapsed().saturating_sub(self.paused + hidden);
        (playing.as_secs_f64() * self.animation.fps) as usize % self.animation.len()
    }
}

impl Drop for AnimationScene {
    fn drop(&mut self) {
        self.decoded.stop.store(true, Ordering::Relaxed);
        self.decoded.wake.notify_all();
    }
}

impl WindowScene for AnimationScene {
    fn update(&mut self, _audio: SceneAudio, _dt: f64) {
        // Ask for the frame after the one coming up, so decoding stays a step ahead.
        let next = (self.current_index() + 1) % self.animation.len();
        let mut wanted = self.decoded.wanted.lock().unwrap();
        if *wanted != Some(next) {
            *wanted = Some(next);
            self.decoded.wake.notify_one();
        }
    }

    fn encode(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        _show_meters: bool,
    ) {
        // The very first frame is decoded right here, so the scene never opens on black.
        let ready = match self.shown {
            None => Some((self.current_index(), self.animation.decode(self.current_index()))).and_then(|(i, f)| f.map(|f| (i, f))),
            Some(_) => self.decoded.frame.lock().unwrap().take(),
        };
        if let Some((index, frame)) = ready {
            if Some(index) != self.shown {
                queue.write_texture(
                    self.texture.as_image_copy(),
                    frame.as_raw(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.animation.width * 4),
                        rows_per_image: None,
                    },
                    self.texture.size(),
                );
                self.shown = Some(index);
            }
        }
        let view = [size.0 as f32, size.1 as f32, self.animation.width as f32, self.animation.height as f32];
        queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&view));
        let mut pass = color_pass(encoder, "animation", target);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn reseed(&mut self) {
        // "From the top": restart the loop.
        self.start = Instant::now();
        self.paused = Duration::ZERO;
    }

    // Not `set_visible`: that also turns false when another window has focus, which would freeze the loop
    // whenever it's picked from the main window.
    fn set_on_screen(&mut self, on_screen: bool) {
        match (on_screen, self.hidden_since) {
            (false, None) => self.hidden_since = Some(Instant::now()),
            (true, Some(since)) => {
                self.paused += since.elapsed();
                self.hidden_since = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wizard_animation_parses_and_decodes() {
        let animation = Animation::load(&wizard_file().unwrap()).unwrap();
        assert_eq!((animation.width, animation.height), (1920, 996), "cropped to the picture, no black bars");
        assert_eq!(animation.len(), 242);
        assert!((animation.fps - 24.0).abs() < 1e-9);
        let frame = animation.decode(120).unwrap();
        assert_eq!(frame.dimensions(), (1920, 996));
        // The bars were cropped: the top row has picture in it, not black.
        let top: u32 = (0..1920).map(|x| frame.get_pixel(x, 0).0[..3].iter().map(|&c| c as u32).sum::<u32>()).sum();
        assert!(top > 1920 * 15, "top row is still black");
        assert!(Animation::parse(b"nope".to_vec()).is_err());
    }
}
