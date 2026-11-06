"""Build the browser's US sequence engine and sound banks from the extracted game data.

Requires a native C compiler, WASI SDK 34 and the pinned sm64-port checkout.
No ROM, network access or audio toolchain is needed to run the checked-in build.
"""
import argparse
import concurrent.futures
import json
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / 'reference/sm64'
SOUND = REFERENCE / 'sound'
EXTRACTED = ROOT / 'assets/extracted/sound'
OUT = ROOT / 'assets/audio/music'
PORT_REVISION = '2b17d081c9798b31b91dc71f37994b0da28cffc9'


def run(*args, **kwargs):
    subprocess.run(args, check=True, **kwargs)


def course_settings():
    ids = dict(re.findall(r'(SEQ_\w+),\s*// (0x\w+)', (REFERENCE / 'include/seq_ids.h').read_text()))
    levels = [line for line in (REFERENCE / 'levels/level_defines.h').read_text().splitlines()
              if line.startswith(('STUB_LEVEL(', 'DEFINE_LEVEL('))]
    courses = {}
    for level, line in enumerate(levels, start=1):
        if line.startswith('STUB_LEVEL'):
            continue
        name = line.split(',')[3].strip()
        script = (REFERENCE / 'levels' / name / 'script.c').read_text()
        terrain_ids = ['GRASS', 'STONE', 'SNOW', 'SAND', 'SPOOKY', 'WATER', 'SLIDE']
        terrains = re.findall(r'TERRAIN_TYPE\(/\*terrainType\*/\s*TERRAIN_(\w+)\)', script)
        areas = []
        for preset, seq, variation in re.findall(
                r'SET_BACKGROUND_MUSIC\(/\*settingsPreset\*/\s*(0x\w+),\s*/\*seq\*/\s*\(?(SEQ_\w+)(\s*\|\s*SEQ_VARIATION)?', script):
            label = seq.removeprefix('SEQ_LEVEL_').lower()
            areas.append(dict(terrain=terrain_ids.index(terrains[len(areas)]), name={'inside_castle': 'inside', 'seq_sound_player': 'ambient'}.get(label, label),
                              sequence=int(ids[seq], 16) | (128 if variation else 0), preset=int(preset, 16)))
        if areas:
            courses[name] = dict(level=level, areas=areas)
    return courses


def sound_identifiers(tmp):
    # Evaluate the US header macros, including priority, bank and status bits.
    names = """
    ACTION_TERRAIN_JUMP ACTION_TERRAIN_LANDING ACTION_TERRAIN_HEAVY_LANDING ACTION_TERRAIN_STEP
    ACTION_TERRAIN_STEP_TIPTOE ACTION_METAL_STEP_TIPTOE ACTION_METAL_JUMP ACTION_METAL_LANDING ACTION_METAL_HEAVY_LANDING ACTION_METAL_STEP
    AIR_ROUGH_SLIDE OBJ_POUNDING_LOUD GENERAL_RACE_GUN_SHOT OBJ2_BABY_PENGUIN_YELL OBJ_BABY_PENGUIN_DIVE GENERAL_WALL_EXPLOSION GENERAL_ACTIVATE_CAP_SWITCH MENU_STAR_SOUND OBJ_CANNON1 OBJ_CANNON2 OBJ_CANNON3 OBJ_CANNON4 OBJ_SOMETHING_LANDING OBJ_WATER_BOMB_BOUNCING OBJ_DIVING_IN_WATER MOVING_AIM_CANNON ACTION_FLYING_FAST OBJ_POUNDING_CANNON ACTION_THROW ACTION_SPIN ACTION_BONK ACTION_SWIM ACTION_SWIM_FAST ACTION_UNKNOWN434 ACTION_UNKNOWN430 MARIO_PUNCH_HOO
    MARIO_YAH_WAH_HOO MARIO_YAHOO_WAHA_YIPPEE MARIO_YAHOO MARIO_HOOHOO MARIO_HERE_WE_GO
    MARIO_ATTACKED MARIO_DYING MARIO_GROUND_POUND_WAH GENERAL_COIN GENERAL_COIN_WATER
    MARIO_HRMM MARIO_SO_LONGA_BOWSER OBJ_BOWSER_TAIL_PICKUP OBJ_BOWSER_SPINNING
    MENU_CAMERA_BUZZ GENERAL2_RIGHT_ANSWER GENERAL_BOWSER_BOMB_EXPLOSION
    MENU_COLLECT_RED_COIN MENU_COLLECT_SECRET GENERAL_BREAK_BOX GENERAL2_BOBOMB_EXPLOSION
    PEACH_MARIO PEACH_POWER_OF_THE_STARS PEACH_THANKS_TO_YOU PEACH_THANK_YOU_MARIO
    PEACH_SOMETHING_SPECIAL PEACH_BAKE_A_CAKE PEACH_FOR_MARIO PEACH_MARIO2
    """.split()
    source = tmp/'ids.c'
    statements = [f'printf("%u\\n", (unsigned)SOUND_{name});' for name in names]
    source.write_text('#include <stdio.h>\ntypedef unsigned int u32;\n#include "sounds.h"\nint main(void){\n'
                      + '\n'.join(statements) + '\n}\n')
    run('cc', '-DVERSION_US', '-I', REFERENCE/'include', source, '-o', tmp/'ids')
    values = subprocess.check_output([tmp/'ids'], text=True).splitlines()
    assert len(names) == len(values)
    return dict(zip(names, map(int, values)))


def build_banks(tmp, sdk):
    tools = REFERENCE / 'tools'
    adpcm = tools / 'sdk-tools/adpcm'
    run('cc', '-O2', tools/'aiff_extract_codebook.c', '-o', tmp/'extract')
    run('cc', '-O2', *(adpcm/(name+'.c') for name in ('vadpcm_enc','vpredictor','quant','util','vencode')),
        '-lm', '-o', tmp/'encode')

    def encode(source):
        out = tmp/'samples'/source.relative_to(EXTRACTED/'samples').with_suffix('.aifc')
        out.parent.mkdir(parents=True, exist_ok=True)
        table = out.with_suffix('.table')
        with table.open('wb') as stream:
            run(tmp/'extract', source, stdout=stream)
        run(tmp/'encode', '-c', table, source, out)

    with concurrent.futures.ThreadPoolExecutor(6) as pool:
        list(pool.map(encode, sorted((EXTRACTED/'samples').rglob('*.aiff'))))
    # Sequence zero is assembly in the decomp. All its macros emit literal bytes.
    run('clang', '-target', 'x86_64-unknown-linux-gnu', '-x', 'assembler-with-cpp', '-DVERSION_US',
        '-I', REFERENCE/'include', '-c', SOUND/'sequences/00_sound_player.s', '-o', tmp/'sfx.o')
    sfx = tmp/'00_sound_player.m64'
    run(sdk/'bin/llvm-objcopy', '-O', 'binary', '--only-section=.rodata', tmp/'sfx.o', sfx)
    assembler = tools/'assemble_sound.py'
    flags = ['-DVERSION_US', '--endian', 'little', '--bitwidth', '32']
    run('python3', assembler, tmp/'samples', SOUND/'sound_banks', tmp/'native.ctl', tmp/'ch',
        tmp/'native.tbl', tmp/'th', *flags)
    run('python3', assembler, '--sequences', tmp/'sequences.bin', tmp/'sh', tmp/'sets.bin',
        SOUND/'sound_banks', SOUND/'sequences.json', sfx, *sorted((EXTRACTED/'sequences/us').glob('*.m64')), *flags)


def compile_engine(tmp, sdk, port):
    symbols = {'gMusicData': 'sequences.bin', 'gBankSetsData': 'sets.bin',
               'gSoundDataADSR': 'native.ctl', 'gSoundDataRaw': 'native.tbl'}
    assembly = tmp/'music.s'
    with assembly.open('w') as f:
        for symbol, filename in symbols.items():
            f.write(f'.section .data.{symbol},"",@\n.globl {symbol}\n.type {symbol},@object\n'
                    f'.p2align 4\n{symbol}:\n.incbin "{tmp/filename}"\n.size {symbol}, .-{symbol}\n')
    exports = ['music_init', 'music_start', 'music_render', 'music_samples', 'music_location',
               'sound_effect', 'seq_player_fade_out', 'music_bird', 'music_power', 'music_race', 'play_course_clear', 'play_power_star_jingle', 'play_puzzle_jingle']
    run(sdk/'bin/clang', '-std=gnu11', '-O2', '-fno-strict-aliasing', '-fsigned-char',
        '-ffunction-sections', '-fdata-sections', '-Wno-error=incompatible-pointer-types',
        '-Wno-error=int-conversion', '-Wno-deprecated-non-prototype', '-DVERSION_US', '-DNON_MATCHING', '-DAVOID_UB',
        '-DNO_SEGMENTED_MEMORY', '-DUSE_SYSTEM_MALLOC', '-I', port, '-I', port/'include', '-I', port/'src',
        '-I', port/'lib/src', '-mexec-model=reactor', *sorted((port/'src/audio').glob('*.c')),
        port/'src/pc/mixer.c', port/'src/pc/ultra_reimplementation.c', port/'lib/src/alBnkfNew.c',
        assembly, ROOT/'scripts/music/player.c', *['-Wl,--export='+name for name in exports],
        '-Wl,--strip-all', '-Wl,-z,stack-size=131072', '-o', tmp/'music.wasm')
    (OUT/'sm64-us.wasm').write_bytes((tmp/'music.wasm').read_bytes())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=Path, required=True)
    parser.add_argument('--wasi-sdk', type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(['git', '-C', args.port, 'rev-parse', 'HEAD'], text=True).strip()
    if revision != PORT_REVISION:
        raise SystemExit('Expected sm64-port revision ' + PORT_REVISION)
    OUT.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as scratch:
        (OUT/'sounds.json').write_text(json.dumps(sound_identifiers(Path(scratch)), indent=2) + '\n')
        build_banks(Path(scratch), args.wasi_sdk.resolve())
        compile_engine(Path(scratch), args.wasi_sdk.resolve(), args.port.resolve())
    (OUT/'courses.json').write_text(json.dumps(course_settings(), separators=(',', ':')) + '\n')
