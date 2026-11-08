#!/usr/bin/env python3
"""Bring-your-own-ROM extraction pipeline for the native app.

Runs the repo's exporters against user-supplied ROMs and installs the results into a data dir
(`<data>/assets`, `<data>/public`).  Only the Python standard library and the Xcode Command Line
Tools (make + a C compiler, for the decomp's tools/sm64tools) are needed.

  pipeline.py --source-root <repo-or-bundle> --data-dir <dir> [--sm64 ROM] [--mk64 ROM] [--oot ROM] [--bk ROM]

Progress goes to stdout as `@@PROGRESS <game> <percent> <message>` and `@@DONE <game>` /
`@@FAIL <game> <message>` lines, which the Bevy setup screen parses.  Each game that succeeds
gets `<data>/stamps/<game>.json`; a failure of one game never removes another game's output.
"""
import argparse, hashlib, json, os, shutil, subprocess, sys, time
from pathlib import Path

VERSION = 2
# game -> (label, hash algo, accepted digests).  Digests are the same ones the exporters check.
ROMS = {
    'sm64': ('Super Mario 64 (US)', 'sha1', {'9bef1128717f958171a4afac3ed78ee2bb4e86ce'}),
    'mk64': ('Mario Kart 64 (US 1.0)', 'sha1', {'579c48e211ae952530ffc8738709f078d5dd215e'}),
    'oot': ('Ocarina of Time (US 1.2)', 'md5', {'57a9719ad547c516342e1a15d5c28c3d'}),
    'bk': ('Banjo-Kazooie (US 1.0)', 'md5', {'b29599651a13f681c9923d69354bf4a3'}),
}
# Original (non-Nintendo) assets copied from the source root into every data dir.
ORIGINAL = ['assets/ui', 'assets/audio/kart-voices.json', 'assets/audio/character-sounds.json',
            'public/models/quaternius', 'public/manifest.webmanifest']
ORIGINAL_GLOBS = ['public/mk64/item-*.png', 'public/icon-*', 'public/favicon.*']
# Reference trees the exporters read.  sm64 is copied (the extractor writes into it); the rest are linked.
LINKED = {'mk64': 'reference/mk64', 'oot': 'reference/oot', 'bk': 'reference/banjo-kazooie'}


def say(*a):
    print(*a, flush=True)


def prog(game, pct, msg):
    say(f'@@PROGRESS {game} {int(pct)} {msg}')


def byteswap_to_z64(data: bytes) -> bytes:
    """Accept .v64 (byte-swapped) and .n64 (little-endian) dumps."""
    if data[:4] == b'\x80\x37\x12\x40':
        return data
    if data[:4] == b'\x37\x80\x40\x12':
        b = bytearray(data); b[0::2], b[1::2] = data[1::2], data[0::2]; return bytes(b)
    if data[:4] == b'\x40\x12\x37\x80':
        b = bytearray(data)
        for i in range(4): b[i::4] = data[3 - i::4]
        return bytes(b)
    return data


def identify(path: Path):
    """Return (game, z64 bytes) for a ROM file in any byte order, or (None, None)."""
    data = byteswap_to_z64(path.read_bytes())
    for game, (_, algo, digests) in ROMS.items():
        if hashlib.new(algo, data).hexdigest() in digests:
            return game, data
    return None, None


def run(cmd, cwd, game, lo, hi, env=None):
    """Run a subprocess, mapping its line count loosely onto lo..hi percent."""
    p = subprocess.Popen(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
    n = 0
    tail = []
    for line in p.stdout:
        n += 1
        tail = (tail + [line.rstrip()])[-12:]
        if n % 25 == 0:
            prog(game, lo + (hi - lo) * (1 - 1 / (1 + n / 400)), line.strip()[:80])
    if p.wait():
        raise RuntimeError(f'{" ".join(map(str, cmd[:3]))} failed ({p.returncode}):\n' + '\n'.join(tail))


def copy_original(src: Path, dst: Path):
    for rel in ORIGINAL:
        s = src / rel
        if s.is_dir():
            shutil.copytree(s, dst / rel, dirs_exist_ok=True)
        elif s.is_file():
            (dst / rel).parent.mkdir(parents=True, exist_ok=True); shutil.copy2(s, dst / rel)
    for g in ORIGINAL_GLOBS:
        for s in src.glob(g):
            (dst / s.relative_to(src)).parent.mkdir(parents=True, exist_ok=True); shutil.copy2(s, dst / s.relative_to(src))


def stage(src: Path, stg: Path, games):
    if stg.exists():
        shutil.rmtree(stg)
    stg.mkdir(parents=True)
    # Real copy: exporters derive ROOT from Path(__file__).resolve(), so scripts must not be symlinks.
    shutil.copytree(src / 'scripts', stg / 'scripts', ignore=shutil.ignore_patterns('*.mjs', '__pycache__'))
    (stg / 'reference/roms').mkdir(parents=True)
    for g, rel in LINKED.items():
        if g in games:
            if not (src / rel).is_dir() or not any((src / rel).iterdir()):
                raise RuntimeError(f'decomp sources missing: {src / rel}')
            os.symlink((src / rel).resolve(), stg / rel)
    if 'sm64' in games:
        shutil.copytree(src / 'reference/sm64', stg / 'reference/sm64', symlinks=True)
    for f in ('c2obj.py',):
        if (src / 'reference' / f).exists():
            shutil.copy2(src / 'reference' / f, stg / 'reference' / f)
    for d in ('assets/courses', 'assets/levels', 'assets/oot-dialogue', 'public/models/oot'):
        (stg / d).mkdir(parents=True, exist_ok=True)
    copy_original(src, stg)


def do_sm64(stg: Path, rom: bytes, game='sm64'):
    ref = stg / 'reference/sm64'
    (ref / 'baserom.us.z64').write_bytes(rom)
    prog(game, 2, 'Building decomp asset tools (needs Xcode Command Line Tools)')
    run([sys.executable, 'extract_assets.py', 'us'], ref, game, 2, 40)
    prog(game, 42, 'Exporting courses'); run([sys.executable, 'scripts/export-courses.py'], stg, game, 42, 50)
    prog(game, 50, 'Exporting models and animations'); run([sys.executable, 'scripts/export-sm64.py'], stg, game, 50, 92)
    # export-courses drops decor whose glb has no meshes, which it can only judge once the models exist
    # (the repo's committed courses were produced by a second run for the same reason).
    prog(game, 91, 'Finalising courses'); run([sys.executable, 'scripts/export-courses.py'], stg, game, 91, 92)
    prog(game, 92, 'Converting audio samples'); run([sys.executable, 'scripts/export-audio.py'], stg, game, 92, 96)
    prog(game, 96, 'Building the music engine'); build_music(stg, game)
    # The pristine ROM and the build products are not needed at runtime.
    shutil.rmtree(ref)


def build_music(stg: Path, game):
    """assets/audio/music/{sm64-us.wasm,sounds.json,courses.json}: patch the user's sequences, banks and
    samples (built with the decomp's own tools from the files just extracted) into the data-less engine
    template shipped in scripts/music.  Byte-identical to the output of scripts/render-music.py."""
    run([sys.executable, 'scripts/render-music-template.py'], stg, game, 96, 99)


def do_mk64(stg, rom, game='mk64'):
    p = stg / 'reference/roms/mk64-us-1.0.z64'; p.write_bytes(rom)
    prog(game, 5, 'Exporting Luigi Raceway and karts')
    run([sys.executable, 'scripts/export-mk64.py', str(p)], stg, game, 5, 99)


def do_oot(stg, rom, game='oot'):
    (stg / 'reference/roms/oot-us-1.2.z64').write_bytes(rom)
    prog(game, 5, 'Exporting Link')
    run([sys.executable, 'scripts/export-oot-link.py'], stg, game, 5, 30)
    (stg / 'assets/oot-dialogue').mkdir(parents=True, exist_ok=True)
    prog(game, 30, 'Exporting Kokiri Forest, Deku Tree and Lon Lon Ranch')
    run([sys.executable, 'scripts/export-oot-courses.py'], stg, game, 30, 99)
    prog(game, 99, 'Exporting Link voices'); run([sys.executable, 'scripts/export-character-voices.py'], stg, game, 99, 100)


def do_bk(stg, rom, game='bk'):
    (stg / 'reference/roms/bk-us-1.0.z64').write_bytes(rom)
    prog(game, 5, 'Exporting Banjo and Kazooie')
    run([sys.executable, 'scripts/export-bk-banjo.py'], stg, game, 5, 40)
    prog(game, 40, 'Exporting Banjo-Kazooie worlds')
    run([sys.executable, 'scripts/export-bk-levels.py'], stg, game, 40, 99)
    prog(game, 99, 'Exporting Banjo voices'); run([sys.executable, 'scripts/export-character-voices.py'], stg, game, 99, 100)


DO = {'sm64': do_sm64, 'mk64': do_mk64, 'oot': do_oot, 'bk': do_bk}


def install(stg: Path, data: Path):
    """Move the staged outputs over the data dir (atomic per top-level entry)."""
    for top in ('assets', 'public'):
        for p in (stg / top).rglob('*'):
            if p.is_file():
                d = data / p.relative_to(stg)
                d.parent.mkdir(parents=True, exist_ok=True)
                os.replace(p, d)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--source-root', type=Path, required=True)
    ap.add_argument('--data-dir', type=Path, required=True)
    ap.add_argument('--identify', type=Path, help='print the game id of a ROM file and exit')
    for g in ROMS:
        ap.add_argument(f'--{g}', type=Path)
    a = ap.parse_args()
    if a.identify:
        g, _ = identify(a.identify)
        say(g or 'unknown'); return 0 if g else 3
    roms = {}
    for g in ROMS:
        path = getattr(a, g)
        if path:
            found, data = identify(path)
            if found != g:
                say(f'@@FAIL {g} {path.name} is not a recognised {ROMS[g][0]} ROM'); continue
            roms[g] = data
    if not roms:
        say('@@FAIL all no valid ROMs given'); return 2
    data_dir = a.data_dir
    stg = data_dir / '.stage'
    (data_dir / 'stamps').mkdir(parents=True, exist_ok=True)
    failed = False
    for g, rom in roms.items():
        try:
            prog(g, 0, 'Preparing')
            stage(a.source_root, stg, {g})
            DO[g](stg, rom)
            install(stg, data_dir)
            (data_dir / 'stamps' / f'{g}.json').write_text(json.dumps({'version': VERSION, 'game': g, 'time': int(time.time())}))
            say(f'@@DONE {g}')
        except Exception as e:  # noqa: BLE001 - report and continue with the next game
            failed = True
            say(f'@@FAIL {g} {str(e)[-600:]}'.replace('\n', ' | '))
        finally:
            shutil.rmtree(stg, ignore_errors=True)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
