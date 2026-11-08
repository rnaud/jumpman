#!/usr/bin/env python3
"""Build the Mario Kart 64 audio engine (CODE only, no ROM data) as a WebAssembly template.

Maintainers run this once; the result (`scripts/music/mk64-template.wasm` + `.json`) is checked in so the
bring-your-own-ROM pipeline needs no compiler for it: `scripts/export-mk64-music.py` patches the user's
converted sequences / banks / samples into the template.

Inputs (all pinned):
  * n64decomp/mk64 checkout (`reference/mk64`): `src/audio/*.c` are compiled unmodified except for the
    endianness fixes listed in PATCHES (the library assumes a big-endian console);
  * sm64-port (`2b17d08`): `src/pc/mixer.c`, the portable RSP audio mixer, built for the "new" microcode
    (the one MK64 shares with SM64 EU/SH) with the DMEM base moved to 0;
  * scripts/music/mk64/{player.c,rsp.c}: libultra stand-ins, the exported API, and the command-list
    interpreter that feeds the mixer;
  * WASI SDK 34 (clang, wasm-ld).

  render-music-mk64.py --decomp reference/mk64 --port /tmp/sm64-port --wasi-sdk /tmp/wasi-sdk-34.0-arm64-macos
"""
import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HERE = Path(__file__).resolve().parent
OUT = HERE / 'music'
PORT_REVISION = '2b17d081c9798b31b91dc71f37994b0da28cffc9'
DECOMP_REVISION = '58cfcb0'
BLOBS = {'gMk64Sequences': (143728, 0xA1), 'gMk64Banks': (79936, 0xA2),
         'gMk64Tables': (2409664, 0xA3), 'gMk64BankSets': (256, 0xA4)}
SOURCES = ['data', 'effects', 'heap', 'load', 'playback', 'port_eu', 'seqplayer', 'synthesis',
           'audio_session_presets', 'external']
EXPORTS = ['music_init', 'music_rate', 'music_samples', 'music_render', 'music_track', 'music_event',
           'music_command', 'music_mode', 'music_final_lap', 'music_star_start', 'music_star_end',
           'music_lightning_start', 'music_lightning_end', 'music_finish', 'music_player_enabled',
           'music_player_seq', 'music_player_tempo', 'music_player_volume', 'music_unknown_ops',
           'music_error_flags']

# Textual patches to the decomp's headers.  The sound library was written for a big-endian console:
# these two structs are overlaid on raw words/bytes and need their field order mirrored on wasm.
PATCHES = {
    'audio_session_presets.c': [('#include "../buffers/audio_heap.h"', '#include "buffers/audio_heap.h"')],
    # N64 pointers live above 0x80000000; wasm addresses do not.
    'playback.c': [('if ((uintptr_t) gCtlEntries[bankId].drums < 0x80000000U) {', 'if (gCtlEntries[bankId].drums == NULL) {')],
    'internal.h': [
        ('''struct EuAudioCmd {
    union {
        struct {
            u8 op;
            u8 bankId;
            u8 arg2;
            u8 arg3;
        } s;
        u32 first;
    } u;
    union {
        s32 as_s32;
        u32 as_u32;
        f32 as_f32;
        u8 as_u8;
        s8 as_s8;
    } u2;
};''', '''struct EuAudioCmd {
    union {
        struct {
            u8 arg3;
            u8 arg2;
            u8 bankId;
            u8 op;
        } s;
        u32 first;
    } u;
    union {
        s32 as_s32;
        u32 as_u32;
        f32 as_f32;
        struct { u8 pad0[3]; u8 as_u8; };
        struct { u8 pad1[3]; s8 as_s8; };
    } u2;
};'''),
        ('''struct ReverbBitsData {
    /* 0x00 */ u8 bit0 : 1;
    /* 0x00 */ u8 bit1 : 1;
    /* 0x00 */ u8 bit2 : 1;
    /* 0x00 */ u8 usesHeadsetPanEffects : 1;
    /* 0x00 */ u8 stereoHeadsetEffects : 2;
    /* 0x00 */ u8 strongRight : 1;
    /* 0x00 */ u8 strongLeft : 1;
};''', '''struct ReverbBitsData {
    /* 0x00 */ u8 strongLeft : 1;
    /* 0x00 */ u8 strongRight : 1;
    /* 0x00 */ u8 stereoHeadsetEffects : 2;
    /* 0x00 */ u8 usesHeadsetPanEffects : 1;
    /* 0x00 */ u8 bit2 : 1;
    /* 0x00 */ u8 bit1 : 1;
    /* 0x00 */ u8 bit0 : 1;
};'''),
    ],
}

# Declarations the audio files take from game headers that are not compiled here.
SHIMS = {
    'cpu_vehicles_camera_path.h': '#ifndef SHIM_CPU_H\n#define SHIM_CPU_H\n#include "camera.h"\nvoid func_8001AAAC(s16, s16, s16);\n#endif\n',
    'menu_items.h': '#ifndef SHIM_MENU_H\n#define SHIM_MENU_H\n#include <common_structs.h>\n#include "main.h"\nextern u8 D_8018ED90;\n#endif\n',
}


def run(*args, **kwargs):
    subprocess.run([str(a) for a in args], check=True, **kwargs)


def git_head(path):
    return subprocess.check_output(['git', '-C', str(path), 'rev-parse', 'HEAD'], text=True).strip()


def stage_sources(decomp, tmp):
    """Copy src/audio with the patches applied; returns the include root that shadows decomp/src."""
    root = tmp / 'src'
    (root / 'audio').mkdir(parents=True)
    for f in (decomp / 'src/audio').iterdir():
        if f.suffix in ('.c', '.h'):
            text = f.read_text()
            for old, new in PATCHES.get(f.name, []):
                if old not in text:
                    raise SystemExit(f'{f.name}: patch target not found (is {decomp} the pinned decomp?)')
                text = text.replace(old, new)
            (root / 'audio' / f.name).write_text(text)
    shim = tmp / 'shim'
    shim.mkdir()
    for name, text in SHIMS.items():
        (shim / name).write_text(text)
    return root, shim


def build_mixer(port, tmp, sdk, cflags):
    """The portable RSP mixer for the "new" ucode, with DMEM addressed from 0 (MK64 uses 0x0 - 0xB40)."""
    text = (port / 'src/pc/mixer.c').read_text()
    if '#define BUF_SIZE 2880' not in text or '((a) - 0x450)' not in text:
        raise SystemExit('sm64-port mixer.c does not look like the pinned revision')
    text = text.replace('#define BUF_SIZE 2880', '#define BUF_SIZE 4608').replace('((a) - 0x450)', '(a)')
    src = tmp / 'mixer_patched.c'
    src.write_text(text)
    run(sdk / 'bin/clang', *cflags, '-O2', '-DVERSION_SH', '-I', port, '-I', port / 'include', '-I', port / 'src',
        '-I', port / 'src/pc', '-c', src, '-o', tmp / 'mixer.o')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--decomp', type=Path, default=ROOT / 'reference/mk64')
    ap.add_argument('--port', type=Path, required=True)
    ap.add_argument('--wasi-sdk', type=Path, required=True)
    ap.add_argument('--debug', action='store_true', help='also export music_debug (not for the checked-in template)')
    a = ap.parse_args()
    sdk, decomp, port = a.wasi_sdk.resolve(), a.decomp.resolve(), a.port.resolve()
    if git_head(port) != PORT_REVISION:
        raise SystemExit('Expected sm64-port revision ' + PORT_REVISION)
    if not git_head(decomp).startswith(DECOMP_REVISION):
        raise SystemExit('Expected n64decomp/mk64 revision ' + DECOMP_REVISION)
    tmp = Path(tempfile.mkdtemp())
    try:
        root, shim = stage_sources(decomp, tmp)
        base = ['--target=wasm32-wasip1', '--sysroot=' + str(sdk / 'share/wasi-sysroot'), '-std=gnu11', '-fsigned-char',
                '-ffunction-sections', '-fdata-sections', '-w', '-Wno-int-conversion', '-DNON_MATCHING', '-DAVOID_UB', '-DNO_SEGMENTED_MEMORY']
        game = base + ['-O2', '-DVERSION_US=1', '-D_LANGUAGE_C'] + (['-DMK64_DEBUG=1'] if a.debug else [])
        incs = ['-I', shim, '-I', root, '-I', decomp / 'include', '-I', decomp / 'src', '-I', decomp]
        objs = []
        for name in SOURCES:
            o = tmp / f'{name}.o'
            run(sdk / 'bin/clang', *game, *incs, '-c', root / 'audio' / f'{name}.c', '-o', o)
            objs.append(o)
        for name, extra in (('player', incs), ('rsp', [])):
            o = tmp / f'{name}.o'
            run(sdk / 'bin/clang', *game, *extra, '-c', HERE / 'music/mk64' / f'{name}.c', '-o', o)
            objs.append(o)
        build_mixer(port, tmp, sdk, base)
        objs.append(tmp / 'mixer.o')
        wasm = tmp / 'mk64.wasm'
        run(sdk / 'bin/clang', '--target=wasm32-wasip1', '--sysroot=' + str(sdk / 'share/wasi-sysroot'),
            '-mexec-model=reactor', '-O2', *objs, '-Wl,--strip-all', '-Wl,-z,stack-size=131072',
            *['-Wl,--export=' + e for e in EXPORTS + (['music_debug'] if a.debug else [])], '-o', wasm)
        data = bytearray(wasm.read_bytes())
        layout = {}
        for symbol, (length, tag) in BLOBS.items():
            pattern = bytes([tag]) + bytes(length - 1)
            at = bytes(data).find(pattern)
            if at < 0 or bytes(data).count(pattern) != 1:
                raise SystemExit(f'{symbol} not found exactly once in the linked module')
            data[at:at + length] = bytes(length)
            layout[symbol] = {'offset': at, 'length': length}
        OUT.mkdir(exist_ok=True)
        (OUT / 'mk64-template.wasm').write_bytes(data)
        (OUT / 'mk64-template.json').write_text(json.dumps(layout, indent=2) + '\n')
        print('template written:', len(data), 'bytes', layout)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == '__main__':
    main()
