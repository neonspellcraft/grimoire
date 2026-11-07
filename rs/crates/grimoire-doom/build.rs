//! Compiles the vendored doomgeneric engine (doomgeneric/, GPL-2.0) and Grimoire's C glue (glue/).

const ENGINE: &[&str] = &[
    "dummy", "am_map", "doomdef", "doomstat", "dstrings", "d_event", "d_items", "d_iwad", "d_loop", "d_main",
    "d_mode", "d_net", "f_finale", "f_wipe", "g_game", "hu_lib", "hu_stuff", "info", "i_cdmus", "i_endoom",
    "i_joystick", "i_scale", "i_sound", "i_system", "i_timer", "memio", "m_argv", "m_bbox", "m_cheat", "m_config",
    "m_controls", "m_fixed", "m_menu", "m_misc", "m_random", "p_ceilng", "p_doors", "p_enemy", "p_floor",
    "p_inter", "p_lights", "p_map", "p_maputl", "p_mobj", "p_plats", "p_pspr", "p_saveg", "p_setup", "p_sight",
    "p_spec", "p_switch", "p_telept", "p_tick", "p_user", "r_bsp", "r_data", "r_draw", "r_main", "r_plane",
    "r_segs", "r_sky", "r_things", "sha1", "sounds", "statdump", "st_lib", "st_stuff", "s_sound", "tables",
    "v_video", "wi_stuff", "w_checksum", "w_file", "w_main", "w_wad", "z_zone", "w_file_stdc", "i_input",
    "i_video", "doomgeneric",
];

fn main() {
    println!("cargo:rerun-if-changed=doomgeneric");
    println!("cargo:rerun-if-changed=glue");
    // `cargo check --target x86_64-pc-windows-msvc` from a Mac has no MSVC to compile C with. Checking doesn't
    // link, so skip the engine there; real Windows builds (CI) compile it as usual.
    let target_msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if target_msvc && !cfg!(windows) && std::env::var("GRIMOIRE_FORCE_DOOM_BUILD").is_err() {
        println!("cargo:warning=skipping the Doom engine's C code: no MSVC on this host (fine for cargo check)");
        return;
    }
    let mut build = cc::Build::new();
    build
        .include("doomgeneric")
        .define("FEATURE_SOUND", None)
        // Every exit() in the engine jumps back to Grimoire instead (glue/doomgeneric_grimoire.c).
        .define("exit", "grimoire_doom_exit")
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .warnings(false)
        .file("glue/doomgeneric_grimoire.c");
    if !build.get_compiler().is_like_msvc() {
        build.flag("-std=gnu99").flag("-fno-strict-aliasing");
    }
    for name in ENGINE {
        build.file(format!("doomgeneric/{name}.c"));
    }
    build.compile("doomgeneric");
    // The engine's fatal-error dialog (I_Error) uses MessageBoxW on Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=user32");
    }
}
