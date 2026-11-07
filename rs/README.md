# Grimoire, cross-platform (Rust)

The Mac and Windows rewrite of Grimoire. The Swift app in `../Sources` stays the daily player until this one
reaches feature parity.

| Crate | What it is | Ported from |
|---|---|---|
| `grimoire-core` | Playlist, m3u, FLAC tags, time math, spectrum and band analysis, three-body physics, the Spellrun game and its pixel renderer | `Sources/GrimoireCore` |
| `grimoire-audio` | symphonia decoding, rubato resampling, cpal output (CoreAudio / WASAPI), visualizer tap | `Sources/Grimoire/Audio` |
| `grimoire-app` | The terminal main window, the four wgpu scenes (Metal / Direct3D 12), media keys, settings, the render tools | `Sources/Grimoire/UI`, `Sources/Grimoire/Model` |

## Status
- Milestone 1: core logic with all the Swift tests ported; playback of MP3, WAV, AIFF and FLAC with sample-accurate
  seeking, gapless trimming, high-quality resampling and following output-device changes; the Three-Body scene in WGSL.
- Milestone 2: the "terminal grimoire" main window. It's a character grid with a CRT finish (phosphor bloom,
  scanlines, a beat-reactive glow) and holds the prompt (type to search, Enter plays), seek bar, spectrum,
  transport, volume, shuffle and repeat, and the playlist. Track changes play a rune glitch reveal. The visualizer
  is a second window you can toggle. Settings persist, and the first launch imports the Swift app's last playlist.
- Milestone 3: all four visualizer scenes (Three-Body, Viewscreen, Spellrun, Splatter), switched with 1–4 or Tab in
  the visualizer window; media keys and Now Playing (macOS Control Center, the Windows media overlay); dragging
  playlist rows to reorder; packaging as a universal macOS DMG and a Windows zip with an embedded icon.
- Not yet: Finder's "Open With" on macOS (winit doesn't deliver those events), a right-click scene menu, code signing.

## Use
```sh
cargo run --release -- ~/Music/album/            # files, folders or .m3u playlists (or drop them on the window)
cargo run --release -- --render-ui ui.png        # the main window with sample data, as a PNG
cargo run --release -- --render-scene threeBody --track song.wav --out frames/ --seconds 20 --every 5
```
Main window: click the prompt to search the playlist, Enter plays the match. Click or drag the seek line and the
volume, scroll the list or the volume, drag rows to reorder, double-click a row to play it. ⌘-click (Ctrl-click on Windows)
adds a row to the selection and Shift-click selects a range; the × on a selected row, or Delete, removes the selection. ⚙ opens settings
(keyboard shortcuts, visualizer scene, band meters, reseed). ⌘/Ctrl+Q quits.

Keyboard shortcuts are on by default and can be turned off in settings: Space play/pause, ←/→ seek (Shift for 30 s),
↑/↓ select, Enter play, Delete removes, type anywhere to search, ⌘/Ctrl+←/→ previous/next, ⌘/Ctrl+O open,
⌘/Ctrl+V visualizer; in the visualizer window 1–4 or Tab switch scenes, R reseed, M band meters, F full screen.
Double-click the visualizer for full screen and Esc to leave it either way.

The main window redraws at 60 fps only while something moves and the visualizer only while it's visible, so a
paused Grimoire uses about 1% CPU.

Settings live in `~/Library/Application Support/Grimoire/settings.json` (macOS) or `%APPDATA%\Grimoire` (Windows).
Set `GRIMOIRE_SETTINGS` to use a different file.

Fonts: JetBrains Mono, with Noto Sans Runic, Symbols, Symbols 2 and Math as fallbacks, all under the SIL Open Font
License (see `crates/grimoire-app/assets/fonts`).

## Package
```sh
./scripts/package-macos.sh        # rs/dist/Grimoire-<version>.dmg, universal (NATIVE_ONLY=1 for this Mac only)
```
CI builds `Grimoire-windows-x64.zip` (exe, read-me, font licenses) and the DMG, and uploads both.

## Test
```sh
cargo test                                                   # everything, incl. an offscreen GPU render
cargo test -p grimoire-audio --test device -- --ignored      # live playback on the real output (muted)
```
CI (`.github/workflows/rust.yml`) runs the tests on macOS and Windows and uploads both builds as artifacts.

## Checking against the Swift app
Both apps share the `--render-scene` flags, so frames can be compared directly. For Three-Body on the same track
they match to within about 1/255 for the first ~10 s. After that they drift apart, because the system is chaotic and
the two FFT libraries round slightly differently. The look stays the same. The other scenes are random, so they're
compared by eye and by overall brightness.

One caveat: the Swift tool doesn't wait for the GPU between frames, so in Splatter (and probably Viewscreen) it
overwrites stamp and star buffers before they're drawn and its frames come out darker than the live Swift app.
Adding `commands.waitUntilCompleted()` after `commands.commit()` in `SceneRenderTool.swift` makes them match the
Rust renders.
