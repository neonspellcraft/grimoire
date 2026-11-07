//! Full-window GPU visualizer scenes (port of `Sources/Grimoire/UI/SceneWindow`).

mod animation;
mod clouds;
mod doom;
mod splatter;
mod spellrun;
mod three_body;
mod viewscreen;

use grimoire_core::BandLevels;

pub use animation::AnimationScene;
pub use clouds::CloudsScene;
pub use doom::DoomScene;
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

    // Games (Doom) use these; the music-driven scenes ignore them.

    /// A key press or release in the visualizer window. True if the scene used it (it then skips shortcuts).
    fn key(&mut self, _event: &winit::event::KeyEvent) -> bool {
        false
    }
    /// A game controller button or stick moved.
    fn pad(&mut self, _input: crate::controller::PadInput) {}
    /// Whether the scene is on screen and focused, so a game can pause when it isn't.
    fn set_visible(&mut self, _visible: bool) {}
    /// The scene's own sound effects (Grimoire's setting; off by default).
    fn set_sound(&mut self, _on: bool) {}
    /// Doom's keyboard controls (Grimoire's settings).
    fn set_doom_controls(&mut self, _controls: &crate::doom_controls::DoomControls) {}
    /// True once the scene wants to hand the window back (Quit in Doom's menu).
    fn wants_to_leave(&mut self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    ThreeBody,
    Viewscreen,
    Spellrun,
    Splatter,
    /// The user's looping wizard animation (doesn't follow the music).
    Wizard,
    /// A dim night sky with clouds drifting past the moon; gently music-driven.
    Clouds,
    /// A game rather than a visualizer: Freedoom on the Doom engine, played with the keyboard or a controller.
    Doom,
}

impl SceneKind {
    /// In menu order. Doom always stays last.
    pub const ALL: [SceneKind; 7] = [
        SceneKind::ThreeBody,
        SceneKind::Viewscreen,
        SceneKind::Spellrun,
        SceneKind::Splatter,
        SceneKind::Wizard,
        SceneKind::Clouds,
        SceneKind::Doom,
    ];

    /// Doom doesn't follow the music, can't be rendered offscreen and has no "reseed".
    pub fn is_game(self) -> bool {
        self == SceneKind::Doom
    }

    /// Same names the Swift app stores and its `--render-scene` flag takes.
    pub fn raw(self) -> &'static str {
        match self {
            SceneKind::ThreeBody => "threeBody",
            SceneKind::Viewscreen => "viewscreen",
            SceneKind::Spellrun => "spellrun",
            SceneKind::Splatter => "splatter",
            SceneKind::Wizard => "wizard",
            SceneKind::Clouds => "clouds",
            SceneKind::Doom => "doom",
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
            SceneKind::Wizard => "Wizard",
            SceneKind::Clouds => "Night Clouds",
            SceneKind::Doom => "Doom",
        }
    }

    /// Menu title for `reseed()`.
    pub fn reseed_title(self) -> &'static str {
        match self {
            SceneKind::ThreeBody => "New Orbit",
            SceneKind::Viewscreen => "New Course",
            SceneKind::Spellrun => "New World",
            SceneKind::Splatter => "Fresh Canvas",
            SceneKind::Wizard => "From the Top",
            SceneKind::Clouds => "New Sky",
            SceneKind::Doom => "",
        }
    }

    pub fn make(self, device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Box<dyn WindowScene>, String> {
        match self {
            SceneKind::ThreeBody => Ok(Box::new(ThreeBodyScene::new(device, format))),
            SceneKind::Viewscreen => Ok(Box::new(ViewscreenScene::new(device, format))),
            SceneKind::Splatter => Ok(Box::new(SplatterScene::new(device, format))),
            SceneKind::Spellrun => Ok(Box::new(SpellrunScene::new(device, format))),
            SceneKind::Wizard => Ok(Box::new(AnimationScene::new(device, format, &animation::wizard_file()?)?)),
            SceneKind::Clouds => Ok(Box::new(CloudsScene::new(device, format))),
            SceneKind::Doom => Ok(Box::new(DoomScene::new(device, format)?)),
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
