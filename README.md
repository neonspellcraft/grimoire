# Grimoire

A lightweight retro music player for macOS. Plays MP3, WAV, AIFF and FLAC. Native Swift and has no dependencies.

This is a personal project, built with [Claude Code](https://claude.com/claude-code).

## Build & run

```sh
./bundle.sh            # release build → build/Grimoire.app
open build/Grimoire.app
swift test             # core logic tests
open Package.swift     # work on it in Xcode
```

Requires macOS 14+.

## Using it

- Drop files or folders on the main window to play them, or on the playlist to add them.
- **Space** plays/pauses, **← / →** seek 5 s, **⌘O** opens files, **⌘⇧O** opens a folder, **⌘L** shows/hides the playlist, **⌘S** saves the playlist.
- Click the time to switch between elapsed and remaining.
- Drag the playlist away to float it. Drag it back under the main window to dock it.
- Media keys and Control Center work. The playlist is restored on the next launch.

## Layout

- `Sources/GrimoireCore` contains pure logic (playlist, m3u, FLAC tags, time math) and is unit tested.
- `Sources/Grimoire/Audio` has `PlayerEngine` (AVAudioEngine playback) and `TrackLoader` (tags).
- `Sources/Grimoire/Model` has `PlayerController` (app state) and `NowPlaying` (media keys).
- `Sources/Grimoire/UI` holds the SwiftUI views. `App/` holds windows, menus and the app delegate.

## Next: visualizers

The space under the time display is reserved for them. The plan is a tap on the engine's main mixer, an FFT with vDSP, and a `Visualization` protocol so styles can plug in. Spectrum bars come first.
