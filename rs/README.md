# Grimoire, cross-platform (Rust)

The Mac and Windows rewrite of Grimoire. The Swift app in `../Sources` stays the daily player until this one
reaches feature parity.

| Crate | What it is | Ported from |
|---|---|---|
| `grimoire-core` | Playlist, m3u, FLAC tags, time math, spectrum and band analysis, three-body physics | `Sources/GrimoireCore` |
| `grimoire-audio` | symphonia decoding, rubato resampling, cpal output (CoreAudio / WASAPI), visualizer tap | `Sources/Grimoire/Audio` |
| `grimoire-app` | winit window, wgpu scenes (Metal / Direct3D 12), the `--render-scene` tool | `Sources/Grimoire/UI/SceneWindow` |

## Status: milestone 1
- Done: core logic with all the Swift tests ported; playback of MP3, WAV, AIFF and FLAC with sample-accurate seeking,
  gapless trimming, high-quality resampling and following output-device changes; the Three-Body scene in WGSL.
- Not yet: the main window (transport, playlist, spectrum, scope), the other three scenes (Viewscreen, Spellrun,
  Splatter), media keys and Now Playing, saved settings, packaging.

## Use
```sh
cargo run --release -- ~/Music/album/            # files, folders or .m3u playlists
cargo run --release -- --render-scene threeBody --track song.wav --out frames/ --seconds 20 --every 5
```
Keys: Space play/pause, ←/→ seek (Shift for 30 s), N/P next/previous, ↑/↓ volume, R new orbit, M band meters,
F or double-click for full screen, Q quit. Drop files on the window to add them.

## Test
```sh
cargo test                                                   # everything, incl. an offscreen GPU render
cargo test -p grimoire-audio --test device -- --ignored      # live playback on the real output (muted)
```
CI (`.github/workflows/rust.yml`) runs the tests on macOS and Windows and uploads both builds as artifacts.

## Checking against the Swift app
Both apps share the `--render-scene` flags, so frames can be compared directly. For Three-Body on the same track
they match to within about 1/255 for the first ~10 s. After that they drift apart, because the system is chaotic and
the two FFT libraries round slightly differently. The look stays the same.
