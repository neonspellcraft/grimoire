//! Doom, played in the visualizer window: Freedoom on the Doom engine (grimoire-doom). It ignores the music;
//! the keyboard and game controllers play it. Choosing Quit in Doom's menu hands the window back to the
//! previous scene, and the game waits, paused, until it's picked again.

use std::collections::HashSet;

use grimoire_doom::{keys, Doom, HEIGHT, WIDTH};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use super::{color_pass, fullscreen_pipeline, SceneAudio, WindowScene};
use crate::controller::{PadButton, PadInput, PadStick};

pub struct DoomScene {
    doom: Doom,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    texture: wgpu::Texture,
    pixels: Vec<u32>,
    serial: u64,
    pad: PadMapper,
}

impl DoomScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<DoomScene, String> {
        let wad = grimoire_doom::bundled_freedoom()
            .ok_or("Freedoom isn't installed with this copy of Grimoire (run scripts/fetch-freedoom.sh)")?;
        let config_dir = crate::settings::Settings::path()
            .and_then(|p| p.parent().map(|d| d.join("doom")))
            .ok_or("no settings folder for Doom's saves")?;
        let doom = Doom::start(&wad, &config_dir)?;
        if doom.has_stopped() {
            return Err("Doom stopped after an error; restart Grimoire to play again".into());
        }
        doom.set_active(true);

        let module = device.create_shader_module(wgpu::include_wgsl!("doom.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("doom"),
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
            label: Some("doom"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        // The engine writes 0x00RRGGBB, which is B, G, R, x in memory.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("doom frame"),
            size: wgpu::Extent3d { width: WIDTH as u32, height: HEIGHT as u32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("doom uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("doom"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("doom"),
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
        Ok(DoomScene {
            doom,
            pipeline: fullscreen_pipeline(device, "doom", &module, &pipeline_layout, "present", format),
            group,
            uniforms,
            texture,
            pixels: Vec::with_capacity(WIDTH * HEIGHT),
            serial: u64::MAX,
            pad: PadMapper::default(),
        })
    }
}

impl Drop for DoomScene {
    fn drop(&mut self) {
        self.doom.set_active(false); // the game waits, paused, for next time
    }
}

impl WindowScene for DoomScene {
    fn update(&mut self, _audio: SceneAudio, _dt: f64) {}

    fn encode(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        _show_meters: bool,
    ) {
        let serial = self.doom.frame(self.serial, &mut self.pixels);
        if serial != self.serial && self.pixels.len() == WIDTH * HEIGHT {
            self.serial = serial;
            queue.write_texture(
                self.texture.as_image_copy(),
                bytemuck_bytes(&self.pixels),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(WIDTH as u32 * 4), rows_per_image: None },
                self.texture.size(),
            );
        }
        let view = [size.0 as f32, size.1 as f32, 0.0, 0.0];
        queue.write_buffer(&self.uniforms, 0, bytemuck_bytes(&view));
        let mut pass = color_pass(encoder, "doom", target);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn reseed(&mut self) {}

    fn key(&mut self, event: &KeyEvent) -> bool {
        if event.repeat {
            return true; // the engine tracks held keys itself
        }
        let pressed = event.state == ElementState::Pressed;
        for code in keyboard_keys(&event.logical_key) {
            self.doom.key(pressed, code);
        }
        true
    }

    fn pad(&mut self, input: PadInput) {
        for (pressed, code) in self.pad.map(input) {
            self.doom.key(pressed, code);
        }
    }

    fn set_visible(&mut self, visible: bool) {
        self.doom.set_active(visible);
    }

    fn set_sound(&mut self, on: bool) {
        self.doom.set_sound(on);
    }

    fn wants_to_leave(&mut self) -> bool {
        self.doom.take_quit_request()
    }
}

fn bytemuck_bytes<T: bytemuck::Pod>(values: &[T]) -> &[u8] {
    bytemuck::cast_slice(values)
}

/// Keyboard → Doom keys. Classic Doom controls (arrows, Ctrl fire, Space use, Shift run, Alt strafe, `,`/`.`
/// strafe) plus WASD; letters are also passed through as themselves for menus, y/n prompts and cheat codes.
pub fn keyboard_keys(key: &Key) -> Vec<u8> {
    match key {
        Key::Named(named) => match named {
            NamedKey::ArrowUp => vec![keys::UP],
            NamedKey::ArrowDown => vec![keys::DOWN],
            NamedKey::ArrowLeft => vec![keys::LEFT],
            NamedKey::ArrowRight => vec![keys::RIGHT],
            NamedKey::Control => vec![keys::FIRE],
            NamedKey::Space => vec![keys::USE],
            NamedKey::Shift => vec![keys::RUN],
            NamedKey::Alt => vec![keys::STRAFE],
            NamedKey::Enter => vec![keys::ENTER],
            NamedKey::Escape => vec![keys::ESCAPE],
            NamedKey::Tab => vec![keys::TAB],
            NamedKey::Backspace => vec![keys::BACKSPACE],
            NamedKey::Pause => vec![keys::PAUSE],
            NamedKey::F1 => vec![keys::function(1)],
            NamedKey::F2 => vec![keys::function(2)],
            NamedKey::F3 => vec![keys::function(3)],
            NamedKey::F4 => vec![keys::function(4)],
            NamedKey::F5 => vec![keys::function(5)],
            NamedKey::F6 => vec![keys::function(6)],
            NamedKey::F7 => vec![keys::function(7)],
            NamedKey::F8 => vec![keys::function(8)],
            NamedKey::F9 => vec![keys::function(9)],
            NamedKey::F10 => vec![keys::function(10)],
            NamedKey::F11 => vec![keys::function(11)],
            NamedKey::F12 => vec![keys::function(12)],
            _ => vec![],
        },
        Key::Character(text) => {
            let Some(c) = text.chars().next() else { return vec![] };
            let mut codes: Vec<u8> = keys::character(c).into_iter().collect();
            codes.extend(match c.to_ascii_lowercase() {
                'w' => Some(keys::UP),
                's' => Some(keys::DOWN),
                'a' => Some(keys::STRAFE_LEFT),
                'd' => Some(keys::STRAFE_RIGHT),
                ',' | '<' => Some(keys::STRAFE_LEFT),
                '.' | '>' => Some(keys::STRAFE_RIGHT),
                _ => None,
            });
            codes
        }
        _ => vec![],
    }
}

/// Controller → Doom keys, twin-stick style: left stick moves and strafes, right stick (or the d-pad) turns.
/// A / right trigger fire (A also selects in menus), B / X use (B also backs out of menus), Y switches weapon,
/// bumpers strafe, left trigger or clicking the left stick runs, Start is the menu, Select the map.
#[derive(Default)]
pub struct PadMapper {
    /// Doom keys held because of a stick, so they're released as the stick returns.
    stick_held: HashSet<u8>,
    weapon: u8,
}

const STICK_ON: f32 = 0.45;
const STICK_OFF: f32 = 0.3;

impl PadMapper {
    pub fn map(&mut self, input: PadInput) -> Vec<(bool, u8)> {
        match input {
            PadInput::Button(button, pressed) => {
                let codes: Vec<u8> = match button {
                    PadButton::South => vec![keys::FIRE, keys::ENTER],
                    PadButton::RightTrigger => vec![keys::FIRE],
                    PadButton::East => vec![keys::USE, keys::BACKSPACE],
                    PadButton::West => vec![keys::USE],
                    PadButton::North => {
                        if !pressed {
                            return vec![];
                        }
                        self.weapon = self.weapon % 7 + 1;
                        let digit = b'0' + self.weapon;
                        return vec![(true, digit), (false, digit)];
                    }
                    PadButton::LeftBumper => vec![keys::STRAFE_LEFT],
                    PadButton::RightBumper => vec![keys::STRAFE_RIGHT],
                    PadButton::LeftTrigger | PadButton::LeftStick => vec![keys::RUN],
                    PadButton::RightStick => vec![],
                    PadButton::Start => vec![keys::ESCAPE],
                    PadButton::Select => vec![keys::TAB],
                    PadButton::Up => vec![keys::UP],
                    PadButton::Down => vec![keys::DOWN],
                    PadButton::Left => vec![keys::LEFT],
                    PadButton::Right => vec![keys::RIGHT],
                };
                codes.into_iter().map(|c| (pressed, c)).collect()
            }
            PadInput::Stick(stick, value) => {
                let (negative, positive) = match stick {
                    PadStick::LeftY => (keys::DOWN, keys::UP),
                    PadStick::LeftX => (keys::STRAFE_LEFT, keys::STRAFE_RIGHT),
                    PadStick::RightX => (keys::LEFT, keys::RIGHT),
                    PadStick::RightY => return vec![],
                };
                // Releases first, so flicking the stick across never holds both directions at once.
                let (mut releases, mut presses) = (Vec::new(), Vec::new());
                for (code, amount) in [(negative, -value), (positive, value)] {
                    let held = self.stick_held.contains(&code);
                    if !held && amount > STICK_ON {
                        self.stick_held.insert(code);
                        presses.push((true, code));
                    } else if held && amount < STICK_OFF {
                        self.stick_held.remove(&code);
                        releases.push((false, code));
                    }
                }
                releases.extend(presses);
                releases
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticks_press_and_release_with_hysteresis() {
        let mut pad = PadMapper::default();
        assert_eq!(pad.map(PadInput::Stick(PadStick::LeftY, 0.2)), vec![]);
        assert_eq!(pad.map(PadInput::Stick(PadStick::LeftY, 0.8)), vec![(true, keys::UP)]);
        assert_eq!(pad.map(PadInput::Stick(PadStick::LeftY, 0.4)), vec![], "still held between the thresholds");
        assert_eq!(pad.map(PadInput::Stick(PadStick::LeftY, -0.9)), vec![(false, keys::UP), (true, keys::DOWN)]);
        assert_eq!(pad.map(PadInput::Stick(PadStick::LeftY, 0.0)), vec![(false, keys::DOWN)]);
    }

    #[test]
    fn buttons_map_to_doom_controls() {
        let mut pad = PadMapper::default();
        assert_eq!(pad.map(PadInput::Button(PadButton::South, true)), vec![(true, keys::FIRE), (true, keys::ENTER)]);
        assert_eq!(pad.map(PadInput::Button(PadButton::Start, false)), vec![(false, keys::ESCAPE)]);
        assert_eq!(pad.map(PadInput::Button(PadButton::North, true)), vec![(true, b'1'), (false, b'1')]);
        assert_eq!(pad.map(PadInput::Button(PadButton::North, true)), vec![(true, b'2'), (false, b'2')]);
        assert_eq!(pad.map(PadInput::Button(PadButton::North, false)), vec![]);
    }

    #[test]
    fn keyboard_has_classic_and_wasd_controls() {
        assert_eq!(keyboard_keys(&Key::Named(NamedKey::Control)), vec![keys::FIRE]);
        assert_eq!(keyboard_keys(&Key::Character("W".into())), vec![b'w', keys::UP]);
        assert_eq!(keyboard_keys(&Key::Character("y".into())), vec![b'y']);
    }
}
