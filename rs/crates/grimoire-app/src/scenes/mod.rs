//! Full-window GPU visualizer scenes (port of `Sources/Grimoire/UI/SceneWindow`).

mod splatter;
mod spellrun;
mod three_body;
mod viewscreen;

use grimoire_core::BandLevels;

pub use spellrun::SpellrunScene;
pub use splatter::SplatterScene;
pub use three_body::ThreeBodyScene;
pub use viewscreen::ViewscreenScene;

/// What every window scene gets each frame: the three bands, smoothed and auto-gained, plus transient hits.
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneAudio {
    pub levels: BandLevels,
    pub hits: BandLevels,
}

/// A full-window, GPU-rendered visualizer. To add one: write a type implementing this,
/// then add a case to `SceneKind`.
pub trait WindowScene {
    fn update(&mut self, audio: SceneAudio, dt: f64);
    /// Records this frame's rendering, finishing in `target` (the window's surface, `size` pixels).
    fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        show_meters: bool,
    );
    /// Starts over from a fresh, slightly randomized state.
    fn reseed(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    ThreeBody,
    Viewscreen,
    Spellrun,
    Splatter,
}

impl SceneKind {
    pub const ALL: [SceneKind; 4] = [SceneKind::ThreeBody, SceneKind::Viewscreen, SceneKind::Spellrun, SceneKind::Splatter];

    /// Same names the Swift app stores and its `--render-scene` flag takes.
    pub fn raw(self) -> &'static str {
        match self {
            SceneKind::ThreeBody => "threeBody",
            SceneKind::Viewscreen => "viewscreen",
            SceneKind::Spellrun => "spellrun",
            SceneKind::Splatter => "splatter",
        }
    }

    pub fn from_raw(raw: &str) -> Option<SceneKind> {
        SceneKind::ALL.into_iter().find(|k| k.raw() == raw)
    }

    pub fn title(self) -> &'static str {
        match self {
            SceneKind::ThreeBody => "Three-Body",
            SceneKind::Viewscreen => "Viewscreen",
            SceneKind::Spellrun => "Spellrun",
            SceneKind::Splatter => "Splatter",
        }
    }

    pub fn make(self, device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Box<dyn WindowScene>, String> {
        match self {
            SceneKind::ThreeBody => Ok(Box::new(ThreeBodyScene::new(device, format))),
            SceneKind::Viewscreen => Ok(Box::new(ViewscreenScene::new(device, format))),
            SceneKind::Splatter => Ok(Box::new(SplatterScene::new(device, format))),
            SceneKind::Spellrun => Ok(Box::new(SpellrunScene::new(device, format))),
        }
    }
}

/// Shared bits for scenes drawn as full-screen triangles.
pub(crate) fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    fragment: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

pub(crate) fn color_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}
