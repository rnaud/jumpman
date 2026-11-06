// Host bindings for the US SM64 sequence engine and portable RSP audio mixer.
#include <ultra64.h>
#include "sm64.h"
#include "seq_ids.h"
#include "level_table.h"
#include "sounds.h"
#include "audio/external.h"
#include "audio/load.h"
#include "audio/seqplayer.h"
#include "game/level_update.h"
#include "game/object_list_processor.h"

s16 gCurrLevelNum, gCurrAreaIndex = 1, gMarioCurrentRoom;
struct MarioState gMarioStates[1];
ALIGNED16 u8 gAudioHeap[0x31200];
static s16 stereo[544 * 2];
static unsigned frame;
static int outdoors, stairs, power;
static Vec3f birds[3];

void create_next_audio_buffer(s16 *samples, u32 num_samples);
void music_init(void) {
    audio_init();
    sound_init();
    sound_reset(0);
}

void music_location(int level, int area, int room, float x, float y, float z, int stars) {
    gCurrLevelNum = level;
    gCurrAreaIndex = area;
    gMarioCurrentRoom = room;
    gMarioStates[0].pos[0] = x;
    gMarioStates[0].pos[1] = y;
    gMarioStates[0].pos[2] = z;
    int infinite = level == LEVEL_CASTLE && area == 2 && room == 6 && stars < 70 && z < 2540;
    if (infinite != stairs) {
        stairs = infinite;
        if (stairs) play_secondary_music(SEQ_EVENT_ENDLESS_STAIRS, 0, 255, 1000);
        else func_80321080(500);
    }
}

void music_start(int seq, int preset) {
    sound_reset(preset);
    stairs = power = 0;
    outdoors = gCurrLevelNum == LEVEL_CASTLE_GROUNDS && seq == SEQ_SOUND_PLAYER && preset != 7;
    if (outdoors) play_sound(SOUND_AIR_CASTLE_OUTDOORS_AMBIENT, gGlobalSoundSource);
    if (seq != 0) play_music(0, seq, 0);
}

void music_bird(int bird, float x, float y, float z) {
    if (bird < 0 || bird >= 3) return;
    birds[bird][0] = x;
    birds[bird][1] = y;
    birds[bird][2] = z;
}

// Power-up and shell music sit above the course in the game's priority queue.
void music_power(int sequence, int fade) {
    if (sequence != power) {
        if (sequence) play_music(SEQ_PLAYER_LEVEL, SEQUENCE_ARGS(4, sequence), 0);
        if (power) stop_background_music(SEQUENCE_ARGS(4, power));
        power = sequence;
    }
    if (power && fade) fadeout_background_music(SEQUENCE_ARGS(4, power), 600);
}

// The original queue retains the position pointer after play_sound returns.
static Vec3f sources[18];
void sound_effect(u32 sound, int variationCount, int source, float x, float y, float z) {
    if (source < 0 || source >= 18) return;
    Vec3f *pos = &sources[source];
    (*pos)[0] = x;
    (*pos)[1] = y;
    (*pos)[2] = z;
    if (variationCount > 1) sound += (gAudioRandom % variationCount) << 16;
    play_sound(sound, *pos);
}

void music_race(int active) {
    u16 race = SEQUENCE_ARGS(4, SEQ_LEVEL_SLIDE);
    if (active) {
        play_sound(SOUND_GENERAL_RACE_GUN_SHOT, gGlobalSoundSource);
        play_music(SEQ_PLAYER_LEVEL, race, 0);
    } else stop_background_music(race);
}

s16 *music_samples(void) { return stereo; }

int music_render(void) {
    // Two audio interrupts per game tick; 528/528/544 stereo frames average 32 kHz.
    unsigned samples = frame % 3 == 2 ? 544 : 528;
    if (frame % 2 == 0) {
        if (outdoors) {
            play_sound(SOUND_OBJ2_BIRD_CHIRP1, birds[0]);
            play_sound(SOUND_GENERAL2_BIRD_CHIRP2, birds[1]);
            play_sound(SOUND_OBJ_BIRD_CHIRP3, birds[2]);
        }
        audio_signal_game_loop_tick();
    }
    frame++;
    create_next_audio_buffer(stereo, samples);
    return samples;
}
