// Host glue for the Mario Kart 64 sound library (n64decomp/mk64 `src/audio/*.c`, unmodified) running
// as a WebAssembly reactor: libultra stand-ins (message queues, PI "DMA" = memcpy from the embedded
// ROM blobs, an AI model), the game-side state external.c reads, and a small exported API.
//
// ROM data (sequences, banks, samples, bank sets) is NOT part of this file or the template wasm: the
// four blobs below are zero-filled in the template and patched in by scripts/export-mk64-music.py.
#include <ultra64.h>
#include <macros.h>
#include <defines.h>
#include <mk64.h>
#include <common_structs.h>
#include <string.h>
#include <PR/ucode.h>

#include "audio/external.h"
#include "audio/internal.h"
#include "audio/data.h"
#include "audio/load.h"
#include "audio/port_eu.h"
#include "audio/seqplayer.h"
#include "buffers/audio_heap.h"
#include "seq_ids.h"
#include "main.h"

void audio_init(void);
struct SPTask* create_next_audio_frame_task(void);
extern u8 gAudioResetPresetIdToLoad;
void func_800CA730(u8);
void func_800C76C0(u8);
void func_800CB2C4(void);

// ---------------------------------------------------------------------------------------------
// ROM blobs (patched into the data segment; sizes must match scripts/export-mk64-music.py)
#define BLOB(name, size, tag) __attribute__((aligned(16), used, section(".data." #name))) u8 name[size] = { tag }
// The first byte is non-zero so the symbol lands in the data segment of the template; the patcher
// overwrites the whole range.
BLOB(gMk64Sequences, 143728, 0xA1);
BLOB(gMk64Banks, 79936, 0xA2);
BLOB(gMk64Tables, 2409664, 0xA3);
BLOB(gMk64BankSets, 256, 0xA4);

// The decomp reads the ROM segments through linker symbols; alias them to the blobs.
extern u8 _sequencesSegmentRomStart __attribute__((alias("gMk64Sequences")));
extern u8 _audio_banksSegmentRomStart __attribute__((alias("gMk64Banks")));
extern u8 _audio_tablesSegmentRomStart __attribute__((alias("gMk64Tables")));
extern u8 _instrument_setsSegmentRomStart __attribute__((alias("gMk64BankSets")));

// ---------------------------------------------------------------------------------------------
// libultra stand-ins
u32 osTvType = 1; // NTSC
u64 rspAspMainStart[1], rspAspMainEnd[1], rspAspMainDataStart[1], rspAspMainDataEnd[1];
u64 rspF3DBootStart[1], rspF3DBootEnd[1];

void osCreateMesgQueue(OSMesgQueue* mq, OSMesg* msg, s32 count) {
    mq->mtqueue = NULL;
    mq->fullqueue = NULL;
    mq->validCount = 0;
    mq->first = 0;
    mq->msgCount = count;
    mq->msg = msg;
}

s32 osSendMesg(OSMesgQueue* mq, OSMesg msg, s32 flag) {
    if (mq->validCount >= mq->msgCount) {
        return -1;
    }
    mq->msg[(mq->first + mq->validCount) % mq->msgCount] = msg;
    mq->validCount++;
    return 0;
}

s32 osRecvMesg(OSMesgQueue* mq, OSMesg* msg, s32 flag) {
    if (mq->validCount == 0) {
        return -1; // single threaded: a blocking receive would never be woken
    }
    if (msg != NULL) {
        *msg = mq->msg[mq->first];
    }
    mq->first = (mq->first + 1) % mq->msgCount;
    mq->validCount--;
    return 0;
}

// The decomp passes ROM addresses; here they are plain pointers into the blobs.
s32 osPiStartDma(OSIoMesg* mb, s32 priority, s32 direction, uintptr_t devAddr, void* vAddr, size_t nbytes, OSMesgQueue* mq) {
    memcpy(vAddr, (const void*) (uintptr_t) devAddr, nbytes);
    osSendMesg(mq, (OSMesg) mb, 0);
    return 0;
}

void osInvalDCache(void* a, size_t b) {}
void osWritebackDCacheAll(void) {}

static u32 count_reg;
u32 osGetCount(void) {
    count_reg += 0x2F3B1;
    return count_reg;
}

// --- audio interface model: buffers handed to osAiSetNextBuffer are appended to the output stream;
// the AI "plays" one video frame worth of samples per game frame, which is what makes the library's
// buffer-length feedback (osAiGetLength) settle at the right rate.
#define OUT_CAP 4096
static s16 out_pcm[OUT_CAP * 2];
static int out_frames;
static float ai_pending; // samples queued but not yet played
static u32 ai_frequency = 32000;

s32 osAiSetFrequency(u32 f) {
    ai_frequency = f;
    return f;
}

u32 osAiGetLength(void) {
    return (u32) (ai_pending > 0 ? ai_pending : 0) * 4;
}

s32 osAiSetNextBuffer(void* buf, u32 bytes) {
    int frames = bytes / 4;
    if (out_frames + frames > OUT_CAP) {
        frames = OUT_CAP - out_frames;
    }
    memcpy(out_pcm + out_frames * 2, buf, frames * 4);
    out_frames += frames;
    ai_pending += bytes / 4;
    return 0;
}

// ---------------------------------------------------------------------------------------------
// Game-side state external.c reads
Player gPlayers[NUM_PLAYERS];
Player* gPlayerOne = &gPlayers[0];
s32 gModeSelection = GRAND_PRIX;
s32 gCurrentCourseId;
s32 gScreenModeSelection;
s32 gPlayerCount = 1;
s32 gPlayerCountSelection1 = 1;
Camera cam_dummy;
Camera *camera1 = &cam_dummy, *camera2 = &cam_dummy, *camera3 = &cam_dummy, *camera4 = &cam_dummy;
ALIGNED8 u8 gAudioHeap[AUDIO_HEAP_SIZE];
s16 D_800DC5A8;
s8 D_801657E5;
u8 D_8018ED90;
s32 gPlayerWinningIndex;
void func_8001AAAC(s16 a, s16 b, s16 c) {}

// ---------------------------------------------------------------------------------------------
extern void rsp_run(const void* cmds, int n);
extern u32 rsp_unknown_ops;

static int sfx_on;
static int started;
static int frames_run;
static s16 pcm[1024 * 2];

__attribute__((export_name("music_init"))) int music_init(int preset) {
    gAudioResetPresetIdToLoad = 0;
    audio_init();
    func_800C5CB8(); // game_audio_init
    started = 1;
    return gAudioBufferParameters.frequency;
}

__attribute__((export_name("music_rate"))) int music_rate(void) {
    return ai_frequency;
}

__attribute__((export_name("music_samples"))) s16* music_samples(void) {
    return pcm;
}

// One game frame (video tick): the game thread's audio update, then the audio thread's task.
__attribute__((export_name("music_render"))) int music_render(void) {
    struct SPTask* task;
    float per_frame = (float) ai_frequency * D_803B7178 / 1000.0f;

    if (sfx_on) {
        // func_800CB2C4, the game thread's whole per-frame audio update (main.c thread5_game_loop): 3D positions
        // and doppler (func_800C1F8C), the sequence players, sound requests and banks, then func_800C8CCC, which runs
        // the per-player engine / tyre / lightning logic of external.c off gPlayers[] (and func_800C76C0, the
        // end-of-race jingles).
        func_800CB2C4();
    } else {
        func_800C76C0(0); // end-of-race jingles, driven by music_finish()
        func_800C3724(); // sequence-player level fades, tempo ramps, queued follow-ups
        func_800C3478(); // dispatch queued music commands
        func_800C5848(); // sound requests
        func_800C59C4(); // sound banks
        func_800CBC24(); // hand the command ring to the audio thread
    }

    ai_pending -= per_frame;
    if (ai_pending < 0) {
        ai_pending = 0;
    }
    out_frames = 0;
    task = create_next_audio_frame_task();
    if (task != NULL) {
        rsp_run(task->task.t.data_ptr, task->task.t.data_size / 8);
    }
    frames_run++;
    int n = out_frames > 1024 ? 1024 : out_frames;
    memcpy(pcm, out_pcm, n * 4);
    return n;
}

// --- music API (thin wrappers over the decomp's game-side entry points) ---------------------------
__attribute__((export_name("music_track"))) void music_track(int seq) { play_sequence(seq); }
__attribute__((export_name("music_event"))) void music_event(int seq) { play_sequence2(seq); }
__attribute__((export_name("music_command"))) void music_command(unsigned cmd) { func_800C3448(cmd); }
__attribute__((export_name("music_mode"))) void music_mode(int mode, int rank, int course) {
    gModeSelection = mode;
    gPlayers[0].currentRank = rank;
    gCurrentCourseId = course;
}
__attribute__((export_name("music_final_lap"))) void music_final_lap(void) { func_800CA49C(0); }
__attribute__((export_name("music_star_start"))) void music_star_start(void) { func_800CA59C(0); }
__attribute__((export_name("music_star_end"))) void music_star_end(void) { func_800CA730(0); }
__attribute__((export_name("music_lightning_start"))) void music_lightning_start(void) { func_800CAB4C(0); }
__attribute__((export_name("music_lightning_end"))) void music_lightning_end(void) { func_800CAC08(); }
__attribute__((export_name("music_finish"))) void music_finish(void) { func_800CA118(0); }

// --- sound effects -------------------------------------------------------------------------------
// The game-side half of the decomp's SFX system, driven from the host: the host mirrors the kart state into
// gPlayers[] (the fields external.c reads) and external.c's own per-player logic (func_800C8CCC) starts, retunes
// and stops the engine, tyre and lightning sounds; one-shot sounds go through the very entry points the game
// uses (func_800C9060 player sound, func_800C90F4 character voice, func_800C98B8 sound at a place, play_sound2).
// Positions the sound system keeps (it stores pointers) live in static pools below.
extern void func_800C2474(void);
extern void sound_init(void);
extern void func_800C9250(u8);
extern void func_800C97C4(u8);
extern void func_800C9D0C(u8);
extern void func_800C99E0(Vec3f, s32);
extern void func_800C98B8(Vec3f, Vec3f, u32);
extern void func_800C9D80(Vec3f, Vec3f, u32);
extern void func_800C9EF4(Vec3f, u32);
extern void func_800C550C(Vec3f*);

// math_util_2.c, used by external.c's 3D sound
static float sinf16(unsigned a) { return sinf((float) (a & 0xFFFF) * 3.14159265358979f / 32768.0f); }
static float cosf16(unsigned a) { return cosf((float) (a & 0xFFFF) * 3.14159265358979f / 32768.0f); }
f32 func_800416D8(f32 x, f32 z, u16 angle) { return cosf16(angle) * x - sinf16(angle) * z; }
f32 func_80041724(f32 x, f32 z, u16 angle) { return cosf16(angle) * z + sinf16(angle) * x; }

// external.c: the US build reads the rear tyres' surface.
#define AUDIO_LEFT_TYRE BACK_LEFT
#define AUDIO_RIGHT_TYRE BACK_RIGHT
#define SFX_SLOTS 32
static struct { Vec3f pos; Vec3f vel; } sfx_slot[SFX_SLOTS];

// 1 once sfx_begin ran: music_render then runs func_800CB2C4 (the game's whole audio frame) instead of the music-only subset.
__attribute__((export_name("sfx_begin"))) void sfx_begin(int course, int humans) {
    int i;
    sfx_on = 1;
    func_800C2474(); // game_audio_init's first half: cameras, doppler list, per-player state
    sound_init();    // sound banks
    gCurrentCourseId = course;
    gPlayerCountSelection1 = humans;
    D_800EA1C0 = humans - 1;
    for (i = 0; i < NUM_PLAYERS; i++) {
        gPlayers[i].type = PLAYER_EXISTS | (i < humans ? PLAYER_HUMAN : 0);
    }
    for (i = 0; i < SFX_SLOTS; i++) {
        sfx_slot[i].pos[0] = sfx_slot[i].pos[1] = sfx_slot[i].pos[2] = 0.0f;
        sfx_slot[i].vel[0] = sfx_slot[i].vel[1] = sfx_slot[i].vel[2] = 0.0f;
    }
}

__attribute__((export_name("sfx_end"))) void sfx_end(void) {
    int i;
    for (i = 0; i < 6; i++) {
        func_800C5278(i); // stop everything in the sound banks
    }
    sfx_on = 0;
}

__attribute__((export_name("sfx_active"))) int sfx_active(void) { return sfx_on; }

// One kart's state, in the units and flag words of the decomp's Player (see common_structs.h / defines.h).
__attribute__((export_name("sfx_player"))) void sfx_player(int i, int character, int type, int kartProps, unsigned effects,
                                                           int oob, float speed, float currentSpeed, float unk098, int unk0C0,
                                                           float unk20C, int surfLeft, int surfRight, float px, float py,
                                                           float pz, float vx, float vy, float vz) {
    Player* p = &gPlayers[i & 7];
    p->characterId = character;
    p->type = type;
    p->kartProps = kartProps;
    p->effects = effects;
    p->oobProps = oob;
    p->speed = speed;
    p->currentSpeed = currentSpeed;
    p->unk_098 = unk098;
    p->unk_0C0 = unk0C0;
    p->unk_20C = unk20C;
    p->tyres[AUDIO_LEFT_TYRE].surfaceType = surfLeft;
    p->tyres[AUDIO_RIGHT_TYRE].surfaceType = surfRight;
    p->pos[0] = px;
    p->pos[1] = py;
    p->pos[2] = pz;
    p->velocity[0] = vx;
    p->velocity[1] = vy;
    p->velocity[2] = vz;
}

__attribute__((export_name("sfx_camera"))) void sfx_camera(float x, float y, float z, int yaw) {
    cam_dummy.pos[0] = x;
    cam_dummy.pos[1] = y;
    cam_dummy.pos[2] = z;
    cam_dummy.rot[1] = yaw;
}

__attribute__((export_name("sfx_rank"))) void sfx_rank(int i, int rank) { gPlayers[i & 7].currentRank = rank; }

// The engine of a player's own kart (func_80027DA8: a human's func_800C94A4, a CPU's func_800C9A88) and its removal.
__attribute__((export_name("sfx_engine_start"))) void sfx_engine_start(int i, int human) {
    if (human) {
        func_800C94A4(i);
    } else {
        func_800C9A88(i);
    }
}
__attribute__((export_name("sfx_engine_stop"))) void sfx_engine_stop(int i, int human) {
    if (human) {
        func_800C97C4(i);
    } else {
        func_800C9D0C(i);
    }
}
// Character voice: func_800C90F4(player, characterId * 0x10 + 0x29008000 + sub) as effects.c / player_controller.c call it.
__attribute__((export_name("sfx_voice"))) void sfx_voice(int i, int sub) {
    func_800C90F4(i, gPlayers[i & 7].characterId * 0x10 + 0x29008000 + sub);
}
__attribute__((export_name("sfx_voice_random"))) void sfx_voice_random(int i) { func_800C9250(i); }
// A sound of the player's own kart (func_800C9060) and stopping it (func_800C9018).
__attribute__((export_name("sfx_player_sound"))) void sfx_player_sound(int i, unsigned bits) { func_800C9060(i, bits); }
__attribute__((export_name("sfx_player_stop"))) void sfx_player_stop(int i, unsigned bits) { func_800C9018(i, bits); }
// A sound that belongs to nobody's kart in particular: menus, starting lights (play_sound2).
__attribute__((export_name("sfx_global"))) void sfx_global(unsigned bits) { play_sound2(bits); }
// A sound at a place (func_800C98B8: distance volume, pan, doppler against the camera) from a pool slot the
// host keeps updating while the thing moves; func_800C99E0 ends it.
__attribute__((export_name("sfx_place_set"))) void sfx_place_set(int slot, float x, float y, float z, float vx, float vy, float vz) {
    slot &= SFX_SLOTS - 1;
    sfx_slot[slot].pos[0] = x;
    sfx_slot[slot].pos[1] = y;
    sfx_slot[slot].pos[2] = z;
    sfx_slot[slot].vel[0] = vx;
    sfx_slot[slot].vel[1] = vy;
    sfx_slot[slot].vel[2] = vz;
}
__attribute__((export_name("sfx_place_play"))) void sfx_place_play(int slot, unsigned bits) {
    slot &= SFX_SLOTS - 1;
    func_800C98B8(sfx_slot[slot].pos, sfx_slot[slot].vel, bits);
}
__attribute__((export_name("sfx_place_loop"))) void sfx_place_loop(int slot, unsigned bits) {
    slot &= SFX_SLOTS - 1;
    func_800C9D80(sfx_slot[slot].pos, sfx_slot[slot].vel, bits);
}
__attribute__((export_name("sfx_place_stop"))) void sfx_place_stop(int slot, unsigned bits) {
    slot &= SFX_SLOTS - 1;
    func_800C9EF4(sfx_slot[slot].pos, bits);
}
// The game's own tail of the race: func_800CA118 (finish voice / jingle for this player), the lightning flag.
__attribute__((export_name("sfx_finish_player"))) void sfx_finish_player(int i) { func_800CA118(i); }
// Pause (func_800CA0A0) / unpause (func_800CA0B8).
__attribute__((export_name("sfx_pause"))) void sfx_pause(int on) { D_800EA108 = on ? 1 : 0; }

// Introspection: sounds in a bank's used list, and the engine-related transient values the tests look at.
extern struct SoundCharacteristics sSoundBanks[SOUND_BANK_COUNT][20];
__attribute__((export_name("sfx_bank_sounds"))) int sfx_bank_sounds(int bank) {
    int n = 0;
    unsigned char at = sSoundBanks[bank][0].next;
    while (at != 0xFF && n < 20) {
        n++;
        at = sSoundBanks[bank][at].next;
    }
    return n;
}
// Is this exact sound id in a bank right now (playing or queued)? 1 / 0
__attribute__((export_name("sfx_bank_has"))) int sfx_bank_has(int bank, unsigned bits) {
    int n = 0;
    unsigned char at = sSoundBanks[bank][0].next;
    while (at != 0xFF && n < 20) {
        if (sSoundBanks[bank][at].soundBits == bits) {
            return 1;
        }
        n++;
        at = sSoundBanks[bank][at].next;
    }
    return 0;
}
// Pitch scale the engine of player i is playing at (D_800E9F7C[i].unk_0C, what func_800C6108 computes) and the
// sound request volume scale (unk_10).
extern struct Unk_800E9F7C D_800E9F7C[4];
__attribute__((export_name("sfx_engine_pitch"))) float sfx_engine_pitch(int i) { return D_800E9F7C[i & 3].unk_0C; }
__attribute__((export_name("sfx_engine_volume"))) float sfx_engine_volume(int i) { return D_800E9F7C[i & 3].unk_10; }

// --- introspection for tests and logging -----------------------------------------------------------
__attribute__((export_name("music_player_enabled"))) int music_player_enabled(int p) {
    return gSequencePlayers[p].enabled;
}
__attribute__((export_name("music_player_seq"))) int music_player_seq(int p) {
    return gSequencePlayers[p].seqId;
}
__attribute__((export_name("music_player_tempo"))) int music_player_tempo(int p) {
    return gSequencePlayers[p].tempo / TATUMS_PER_BEAT;
}
__attribute__((export_name("music_player_volume"))) float music_player_volume(int p) {
    return gSequencePlayers[p].fadeVolume;
}
__attribute__((export_name("music_unknown_ops"))) unsigned music_unknown_ops(void) { return rsp_unknown_ops; }
__attribute__((export_name("music_error_flags"))) int music_error_flags(void) { return gAudioErrorFlags; }

#ifdef MK64_DEBUG
extern u8 D_800EA1E4, D_800EA1E8, D_800EA3A0[], D_800EA3A4[];
extern volatile s32 gAudioFrameCount;
extern s32 gAudioResetStatus;
extern u8 gSeqLoadStatus[];
__attribute__((export_name("music_debug"))) int music_debug(int i) {
    switch (i) {
        case 0: return D_800EA1E4;
        case 1: return D_800EA1E8;
        case 2: return D_800EA3A0[0];
        case 3: return D_800EA3A4[0];
        case 4: return gAudioResetStatus;
        case 5: return gAudioFrameCount;
        case 6: return gAudioBufferParameters.presetUnk4;
        case 7: return gAudioLoadLock;
        case 8: return gSeqLoadStatus[3];
        case 9: return gAudioBufferParameters.samplesPerFrameTarget;
        case 10: return gAudioBufferParameters.updatesPerFrame;
    }
    return -1;
}
#endif
