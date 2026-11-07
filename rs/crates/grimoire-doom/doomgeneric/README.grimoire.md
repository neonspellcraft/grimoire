# doomgeneric, as used by Grimoire

Vendored from https://github.com/ozkl/doomgeneric at commit `e257a09` (GPL-2.0, see `LICENSE`), which is based on
Chocolate Doom and id Software's 1997 GPL source release. Only the engine is here; the platform layers
(`doomgeneric_*.c`, `i_sdl*`, `i_allegro*`) were left out, because Grimoire provides its own (`../glue/` and
`../src/lib.rs`).

Changes, each marked `GRIMOIRE` in the source:

- `i_sound.c`: no `SDL_mixer.h` include; sound and music come from Grimoire's modules.
- `m_config.c`: configuration and savegames go in Grimoire's settings folder (`grimoire_doom_config_dir`).
- `i_system.c`: `I_Quit` hands the window back to Grimoire (`grimoire_doom_quit`) instead of exiting.
- Build flag (`../build.rs`): `exit()` is compiled as `grimoire_doom_exit()`, which leaves the engine without
  ending Grimoire.
