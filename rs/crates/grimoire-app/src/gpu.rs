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
