//! Grimoire, cross-platform build.
//!
//!     grimoire [audio files, folders or .m3u playlists…]
//!     grimoire --render-scene <scene> --track song.aiff --out frames/ [--seconds 40] [--every 5] [--size 1280x800]

// No console window behind the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod feed;
mod gpu;
mod library;
mod main_window;
mod render_tool;
mod scenes;
mod settings;
mod ui;
mod vis_window;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(request) = render_tool::RenderRequest::from_args(&args) {
        return match request.and_then(|r| render_tool::run(&r)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("render-scene: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(result) = render_tool::render_ui_from_args(&args) {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("render-ui: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("grimoire [audio files, folders or .m3u playlists…]\n\n{}", app::HELP);
        return ExitCode::SUCCESS;
    }
    match app::run(args.into_iter().map(PathBuf::from).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("grimoire: {e}");
            ExitCode::FAILURE
        }
    }
}
