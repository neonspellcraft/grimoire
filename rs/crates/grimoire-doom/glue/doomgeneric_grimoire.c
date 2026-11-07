// Grimoire's platform layer for doomgeneric. The drawing, timing and input hooks (DG_*) are implemented in Rust
// (src/lib.rs); this file holds the parts that have to be C:
//
// - Every `exit()` in the engine is compiled as `grimoire_doom_exit()` (see build.rs), which jumps back out of
//   the engine instead of ending Grimoire. Only fatal engine errors get there: quitting from Doom's menu is
//   intercepted earlier, in I_Quit (patched in i_system.c).
// - The sound module plays effects through Rust; music is a silent stub for now.
//
// Copyright (C) 2026 Grimoire contributors. GPL-2.0-or-later, like the engine.

#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "doomgeneric.h"
#include "doomtype.h"
#include "deh_str.h"
#include "i_sound.h"
#include "m_misc.h"
#include "w_wad.h"
#include "z_zone.h"

static jmp_buf grimoire_jump;
static int grimoire_running = 0;

void grimoire_doom_exit(int code)
{
    fprintf(stderr, "grimoire-doom: the engine stopped (code %d)\n", code);
    if (grimoire_running)
    {
        longjmp(grimoire_jump, 1);
    }
    abort();
}

// Runs the engine until it stops for good. Only returns after a fatal engine error.
int grimoire_doom_run(int argc, char **argv)
{
    if (setjmp(grimoire_jump))
    {
        grimoire_running = 0;
        return 1;
    }
    grimoire_running = 1;
    doomgeneric_Create(argc, argv);
    for (;;)
    {
        doomgeneric_Tick();
    }
}

// Whether Doom's menu is open, so Grimoire can keep menu keys working whatever the game controls are bound to.
extern boolean menuactive;

int grimoire_doom_menu_active(void)
{
    return menuactive ? 1 : 0;
}

// Settings the SDL sound module would normally define.
int use_libsamplerate = 0;
float libsamplerate_scale = 0.65f;

void I_InitTimidityConfig(void)
{
}

// Sound effects, played by Rust.

extern int grimoire_doom_sfx_init(void);
extern void grimoire_doom_sfx_start(int channel, const unsigned char *data, int length, int lump, int volume, int separation);
extern void grimoire_doom_sfx_params(int channel, int volume, int separation);
extern void grimoire_doom_sfx_stop(int channel);
extern int grimoire_doom_sfx_playing(int channel);

static boolean use_sfx_prefix;

static snddevice_t sound_devices[] = {
    SNDDEVICE_SB, SNDDEVICE_PAS, SNDDEVICE_GUS, SNDDEVICE_WAVEBLASTER, SNDDEVICE_SOUNDCANVAS, SNDDEVICE_AWE32,
};

static boolean Sfx_Init(boolean prefix)
{
    use_sfx_prefix = prefix;
    return grimoire_doom_sfx_init() ? true : false;
}

static void Sfx_Shutdown(void)
{
}

static int Sfx_GetLumpNum(sfxinfo_t *sfx)
{
    char name[9];
    if (sfx->link != NULL)
    {
        sfx = sfx->link;
    }
    if (use_sfx_prefix)
    {
        M_snprintf(name, sizeof(name), "ds%s", DEH_String(sfx->name));
    }
    else
    {
        M_StringCopy(name, DEH_String(sfx->name), sizeof(name));
    }
    return W_CheckNumForName(name);
}

static void Sfx_Update(void)
{
}

static void Sfx_UpdateParams(int channel, int volume, int separation)
{
    grimoire_doom_sfx_params(channel, volume, separation);
}

static int Sfx_Start(sfxinfo_t *sfx, int channel, int volume, int separation)
{
    int lump = sfx->lumpnum;
    if (lump < 0)
    {
        return -1;
    }
    const unsigned char *data = W_CacheLumpNum(lump, PU_STATIC);
    grimoire_doom_sfx_start(channel, data, W_LumpLength(lump), lump, volume, separation);
    W_ReleaseLumpNum(lump);
    return channel;
}

static void Sfx_Stop(int channel)
{
    grimoire_doom_sfx_stop(channel);
}

static boolean Sfx_IsPlaying(int channel)
{
    return grimoire_doom_sfx_playing(channel) ? true : false;
}

static void Sfx_Cache(sfxinfo_t *sounds, int count)
{
    (void)sounds;
    (void)count;
}

sound_module_t DG_sound_module = {
    sound_devices,
    sizeof(sound_devices) / sizeof(*sound_devices),
    Sfx_Init,
    Sfx_Shutdown,
    Sfx_GetLumpNum,
    Sfx_Update,
    Sfx_UpdateParams,
    Sfx_Start,
    Sfx_Stop,
    Sfx_IsPlaying,
    Sfx_Cache,
};

// Music: silent for now (Grimoire is a music player; the game's soundtrack would play over it).

static snddevice_t music_devices[] = { SNDDEVICE_SB, SNDDEVICE_GENMIDI, SNDDEVICE_GUS };
static boolean Music_Init(void) { return true; }
static void Music_Shutdown(void) {}
static void Music_SetVolume(int volume) { (void)volume; }
static void Music_Pause(void) {}
static void Music_Resume(void) {}
static void *Music_Register(void *data, int length) { (void)data; (void)length; return (void *)1; }
static void Music_Unregister(void *handle) { (void)handle; }
static void Music_Play(void *handle, boolean looping) { (void)handle; (void)looping; }
static void Music_Stop(void) {}
static boolean Music_IsPlaying(void) { return false; }
static void Music_Poll(void) {}

music_module_t DG_music_module = {
    music_devices,
    sizeof(music_devices) / sizeof(*music_devices),
    Music_Init,
    Music_Shutdown,
    Music_SetVolume,
    Music_Pause,
    Music_Resume,
    Music_Register,
    Music_Unregister,
    Music_Play,
    Music_Stop,
    Music_IsPlaying,
    Music_Poll,
};
