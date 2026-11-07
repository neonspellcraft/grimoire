//! Device setup. wgpu picks Metal on macOS and Direct3D 12 on Windows.

pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// `surface` is the window to draw into, or None for offscreen rendering (falls back to a software
    /// adapter such as WARP if there's no GPU, which is what CI machines have).
    pub fn new(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Result<Gpu, String> {
        let request = |fallback| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                force_fallback_adapter: fallback,
                compatible_surface: surface,
            }))
        };
        let adapter = request(false)
            .or_else(|e| if surface.is_none() { request(true) } else { Err(e) })
            .map_err(|e| format!("no graphics adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("grimoire"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("couldn't open the graphics device: {e}"))?;
        Ok(Gpu { adapter, device, queue })
    }

    pub fn instance() -> wgpu::Instance {
        wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default())
    }
}

/// The scenes write display values directly, like the Swift app's `bgra8Unorm` drawable, so draw through a
/// non-sRGB view even when the surface itself is sRGB.
pub fn surface_formats(capabilities: &wgpu::SurfaceCapabilities) -> (wgpu::TextureFormat, wgpu::TextureFormat) {
    use wgpu::TextureFormat as F;
    let surface = capabilities
        .formats
        .iter()
        .copied()
        .find(|f| matches!(f.remove_srgb_suffix(), F::Bgra8Unorm | F::Rgba8Unorm))
        .unwrap_or(capabilities.formats[0]);
    (surface, surface.remove_srgb_suffix())
}

/// A window plus its wgpu surface, configured for the scenes' non-sRGB drawing.
pub struct WindowSurface {
    pub window: std::sync::Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub view_format: wgpu::TextureFormat,
}

impl WindowSurface {
    pub fn new(
        instance: &wgpu::Instance,
        gpu: Option<&Gpu>,
        window: std::sync::Arc<winit::window::Window>,
    ) -> Result<(WindowSurface, Option<Gpu>), String> {
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        // The first window also picks the adapter, so it's one that can present to it.
        let created = match gpu {
            Some(_) => None,
            None => Some(Gpu::new(instance, Some(&surface))?),
        };
        let gpu_ref = gpu.or(created.as_ref()).unwrap();
        let capabilities = surface.get_capabilities(&gpu_ref.adapter);
        let (format, view_format) = surface_formats(&capabilities);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: if view_format != format { vec![view_format] } else { vec![] },
        };
        surface.configure(&gpu_ref.device, &config);
        Ok((WindowSurface { window, surface, config, view_format }, created))
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn is_drawable(&self) -> bool {
        self.config.width > 0 && self.config.height > 0
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        if self.is_drawable() {
            self.surface.configure(&gpu.device, &self.config);
        }
    }

    /// The next frame to draw into, or None if this frame should be skipped.
    pub fn acquire(&mut self, gpu: &Gpu) -> Option<(wgpu::SurfaceTexture, wgpu::TextureView)> {
        if !self.is_drawable() {
            return None;
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&gpu.device, &self.config);
                return None;
            }
            Err(e) => {
                eprintln!("grimoire: skipped a frame: {e}");
                return None;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        Some((frame, view))
    }

    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        self.window.pre_present_notify();
        frame.present();
    }
}
