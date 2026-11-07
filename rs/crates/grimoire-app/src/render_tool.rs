//! Developer tool: plays a track through the real band analysis and a real visualizer scene, rendering offscreen
//! to PNG frames. Same interface as the Swift app's, so the two can be compared frame by frame:
//!
//!     grimoire --render-scene threeBody --track song.aiff --out frames/ [--seconds 40] [--every 5] [--size 1280x800]

use std::path::{Path, PathBuf};

use grimoire_audio::Source;
use grimoire_core::{BandFollower, MultibandAnalyzer};

use crate::gpu::Gpu;
use crate::scenes::{SceneAudio, SceneKind};
use crate::ui::{layout, Effects, Screen, UiRenderer};

pub struct RenderRequest {
    pub kind: SceneKind,
    pub track: PathBuf,
    pub out: PathBuf,
    pub seconds: f64,
    pub every: f64,
    pub width: u32,
    pub height: u32,
}

impl RenderRequest {
    /// None if the arguments don't ask for a render; Err for a malformed request.
    pub fn from_args(args: &[String]) -> Option<Result<RenderRequest, String>> {
        let flag = args.iter().position(|a| a == "--render-scene")?;
        let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        Some((|| {
            let raw = args.get(flag + 1).ok_or("--render-scene needs a scene name")?;
            let kind = SceneKind::from_raw(raw).ok_or_else(|| {
                let names: Vec<_> = SceneKind::ALL.iter().map(|k| k.raw()).collect();
                format!("Unknown scene '{raw}'. Scenes: {}", names.join(", "))
            })?;
            let (Some(track), Some(out)) = (value("--track"), value("--out")) else {
                return Err("Needs --track and --out".to_string());
            };
            let size: Vec<u32> = value("--size").unwrap_or("1280x800".into()).split('x').filter_map(|s| s.parse().ok()).collect();
            Ok(RenderRequest {
                kind,
                track: track.into(),
                out: out.into(),
                seconds: value("--seconds").and_then(|s| s.parse().ok()).unwrap_or(40.0),
                every: value("--every").and_then(|s| s.parse().ok()).unwrap_or(5.0),
                width: size.first().copied().unwrap_or(1280),
                height: size.get(1).copied().unwrap_or(800),
            })
        })())
    }
}

pub fn run(request: &RenderRequest) -> Result<(), String> {
    if request.kind.is_game() {
        return Err(format!("{} is a game, not a visualizer, so it can't be rendered from a track", request.kind.title()));
    }
    let (mono, rate) = decode_mono(&request.track, request.seconds)?;
    std::fs::create_dir_all(&request.out).map_err(|e| e.to_string())?;
    let mut saved = 0;
    let frames = render(request.kind, &mono, rate, request.every, request.width, request.height, |seconds, rgba| {
        let path = request.out.join(format!("{}_{:03}s.png", request.kind.raw(), seconds));
        image::save_buffer(&path, rgba, request.width, request.height, image::ColorType::Rgba8)
            .map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        saved += 1;
        Ok(())
    })?;
    println!("Rendered {frames} frames of {}; saved {saved} PNGs to {}", request.kind.title(), request.out.display());
    Ok(())
}

/// Mixes the first `seconds` of the file down to mono at the file's own rate, like the Swift tool.
fn decode_mono(path: &Path, seconds: f64) -> Result<(Vec<f32>, f64), String> {
    let mut source = Source::open(path).map_err(|e| e.to_string())?;
    let rate = source.info.sample_rate as f64;
    let limit = (seconds * rate) as usize;
    let mut mono = Vec::with_capacity(limit);
    while mono.len() < limit {
        let more = source
            .next_chunk(|channels| {
                let scale = 1.0 / channels.len() as f32;
                for i in 0..channels[0].len() {
                    mono.push(channels.iter().map(|c| c[i]).sum::<f32>() * scale);
                }
            })
            .map_err(|e| e.to_string())?;
        if !more {
            break;
        }
    }
    mono.truncate(limit);
    Ok((mono, rate))
}

/// Steps the analysis and the scene at 60 fps over `mono`; every `every` seconds hands `shot` the frame as RGBA.
/// Returns the number of frames rendered.
pub fn render(
    kind: SceneKind,
    mono: &[f32],
    rate: f64,
    every: f64,
    width: u32,
    height: u32,
    mut shot: impl FnMut(u32, &[u8]) -> Result<(), String>,
) -> Result<usize, String> {
    let gpu = Gpu::new(&Gpu::instance(), None)?;
    let format = wgpu::TextureFormat::Bgra8Unorm; // the Swift tool's format
    let mut scene = kind.make(&gpu.device, format)?;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let row_bytes = width * 4;
    let padded_row = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut analyzer = MultibandAnalyzer::default();
    let mut follower = BandFollower::default();
    let window = analyzer.fft_size();
    let dt = 1.0 / 60.0;
    let hop = (rate * dt) as usize;
    let frames_per_shot = ((every / dt) as usize).max(1);
    let (mut frame, mut end) = (0usize, window);
    let mut rgba = vec![0u8; (row_bytes * height) as usize];
    while end <= mono.len() {
        let levels = analyzer.levels(&mono[end - window..end], rate);
        follower.step(Some(levels), dt);
        scene.update(SceneAudio { levels: follower.envelope, hits: follower.hits }, dt);

        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        scene.encode(&gpu.device, &gpu.queue, &mut encoder, &view, (width, height), true);
        let take = frame > 0 && frame % frames_per_shot == 0;
        if take {
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(height),
                    },
                },
                target.size(),
            );
        }
        gpu.queue.submit([encoder.finish()]);
        if take {
            let slice = readback.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            gpu.device.poll(wgpu::PollType::Wait).map_err(|e| e.to_string())?;
            {
                let data = slice.get_mapped_range();
                for (row, out) in data.chunks(padded_row as usize).zip(rgba.chunks_mut(row_bytes as usize)) {
                    for (bgra, px) in row[..row_bytes as usize].chunks_exact(4).zip(out.chunks_exact_mut(4)) {
                        px.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
                    }
                }
            }
            readback.unmap();
            shot((frame as f64 * dt) as u32, &rgba)?;
        }
        end += hop;
        frame += 1;
    }
    Ok(frame)
}

/// `--render-ui out.png [--size 520x420] [--scale 2] [--reveal 1] [--settings]`:
/// draws the main window with sample data, for reviewing the look without a running player.
pub fn render_ui_from_args(args: &[String]) -> Option<Result<(), String>> {
    let flag = args.iter().position(|a| a == "--render-ui")?;
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    Some((|| {
        let out = args.get(flag + 1).ok_or("--render-ui needs an output path")?;
        let size: Vec<u32> = value("--size").unwrap_or("520x420".into()).split('x').filter_map(|s| s.parse().ok()).collect();
        let scale: f32 = value("--scale").and_then(|s| s.parse().ok()).unwrap_or(2.0);
        let mut view = layout::sample_view();
        if let Some(reveal) = value("--reveal").and_then(|s| s.parse().ok()) {
            view.reveal = reveal;
        }
        if args.iter().any(|a| a == "--settings") {
            view.settings = Some(crate::ui::SettingsView {
                shortcuts: false,
                scene: "Three-Body".into(),
                meters: false,
                visualizer_open: true,
                doom_sound: false,
            });
        }
        let width = (size.first().copied().unwrap_or(520) as f32 * scale) as u32;
        let height = (size.get(1).copied().unwrap_or(420) as f32 * scale) as u32;
        let rgba = render_ui(&view, scale, width, height)?;
        image::save_buffer(out, &rgba, width, height, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
        println!("Saved {width}x{height} UI to {out}");
        Ok(())
    })())
}

pub fn render_ui(view: &layout::View, scale: f32, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let gpu = Gpu::new(&Gpu::instance(), None)?;
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut ui = UiRenderer::new(&gpu.device, format, scale);
    let (cols, rows) = ui.grid_size(width, height);
    let mut screen = Screen::new(cols, rows);
    layout::layout(view, &mut screen);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ui target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let effects = Effects { time: view.clock as f32, glow: view.beat };
    ui.render(&gpu.device, &gpu.queue, &mut encoder, &target.create_view(&Default::default()), (width, height), &screen, effects);
    gpu.queue.submit([encoder.finish()]);
    capture(&gpu, &target)
}

/// Reads a BGRA texture back as RGBA bytes.
fn capture(gpu: &Gpu, target: &wgpu::Texture) -> Result<Vec<u8>, String> {
    let (width, height) = (target.width(), target.height());
    let row_bytes = width * 4;
    let padded_row = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_row), rows_per_image: Some(height) },
        },
        target.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::Wait).map_err(|e| e.to_string())?;
    let mut rgba = vec![0u8; (row_bytes * height) as usize];
    {
        let data = slice.get_mapped_range();
        for (row, out) in data.chunks(padded_row as usize).zip(rgba.chunks_mut(row_bytes as usize)) {
            for (bgra, px) in row[..row_bytes as usize].chunks_exact(4).zip(out.chunks_exact_mut(4)) {
                px.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
            }
        }
    }
    readback.unmap();
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders every scene offscreen with a synthetic beat. Proves each WGSL shader compiles and draws something on
    /// whatever adapter the machine has (WARP on Windows CI). Skips if there's no adapter at all.
    #[test]
    fn every_scene_renders_offscreen() {
        let rate = 44_100.0;
        let mono: Vec<f32> = (0..(rate as usize * 3))
            .map(|i| {
                let t = i as f64 / rate;
                let kick = if (t * 2.0).fract() < 0.1 { (2.0 * std::f64::consts::PI * 60.0 * t).sin() } else { 0.0 };
                let hat = if (t * 8.0).fract() < 0.02 { fastrand::f64() * 2.0 - 1.0 } else { 0.0 };
                (0.4 * kick + 0.1 * (2.0 * std::f64::consts::PI * 440.0 * t).sin() + 0.2 * hat) as f32
            })
            .collect();
        for kind in SceneKind::ALL.into_iter().filter(|k| !k.is_game()) {
            let mut brightest = 0u8;
            let result = render(kind, &mono, rate, 1.0, 320, 200, |_, rgba| {
                brightest = brightest.max(rgba.chunks_exact(4).map(|p| p[0].max(p[1]).max(p[2])).max().unwrap_or(0));
                Ok(())
            });
            match result {
                Ok(frames) => {
                    assert!(frames > 100);
                    assert!(brightest > 40, "{} rendered black (max channel {brightest})", kind.title());
                }
                Err(e) if e.starts_with("no graphics adapter") => {
                    eprintln!("skipped: {e}");
                    return;
                }
                Err(e) => panic!("{}: {e}", kind.title()),
            }
        }
    }
}
