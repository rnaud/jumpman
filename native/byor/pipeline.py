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
import argparse, gzip, hashlib, inspect, json, os, shutil, subprocess, sys, time
from pathlib import Path

VERSION = 3  # bump together with PIPELINE_VERSION in src/byor.rs; per-game exporter hashes catch everything else
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


# Exporter scripts (relative to scripts/) each game's step runs, directly or via import/runpy.  check-manifest.py
# verifies the union equals scripts-manifest.txt.  Their contents, the game's decomp manifest files and the step
# function itself form the per-game hash that stamps record (see game_hashes).
GAME_SCRIPTS = {
    'sm64': ['export-courses.py', 'sm64_world.py', 'export_sm64_g2.py', 'export-sm64-g1-actors.py', 'export-sm64-solids.py', 'export-sm64-bosses.py', 'export-sm64-castle.py', 'export-sm64.py', 'export-audio.py', 'render-music-template.py', 'render-music.py',
             'music/sm64-us-template.json', 'music/sm64-us-template.wasm'],
    'mk64': ['export-mk64.py', 'mk64_courses.py', 'export-mk64-cpu.py', 'export-mk64-music.py', 'music/mk64-template.json', 'music/mk64-template.wasm'],
    'oot': ['export-oot-items.py', 'export-oot-link.py', 'export-oot-epona.py', 'export-oot-courses.py', 'export-oot-enemies.py',
            'oot-residents.py', 'oot-scenery.py', 'export-character-voices.py', 'export-oot-sfx.py'],
    'bk': ['bk-maps.py', 'data/bk-catalogue.json', 'export-bk-banjo.py', 'export-bk-levels.py', 'export-bk-enemies.py', 'export-bk-setup.py',
           'export-character-voices.py', 'export-bk-sfx.py'],
}
GAME_TREE = {'sm64': 'sm64', 'mk64': 'mk64', 'oot': 'oot', 'bk': 'banjo-kazooie'}


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


# iOS (and JUMPMAN_INPROC=1 on a Mac, for testing) has no fork/exec: exporters run inside this interpreter and the
# decomp's C tools are linked into the app.  See inproc.py and native/IOS_IMPORT.md.
INPROC = sys.platform == 'ios' or bool(os.environ.get('JUMPMAN_INPROC'))
if INPROC:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import inproc
    inproc.install()


def run(cmd, cwd, game, lo, hi, env=None):
    """Run a subprocess, mapping its line count loosely onto lo..hi percent."""
    if INPROC:
        n = [0]

        def on_line(line):
            n[0] += 1
            if n[0] % 25 == 0:
                prog(game, lo + (hi - lo) * (1 - 1 / (1 + n[0] / 400)), line.strip()[:80])
        return inproc.run_step(cmd, cwd, on_line)
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


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _script_bytes(root: Path, rel: str):
    f = root / 'scripts' / rel
    if f.is_file():
        return f.read_bytes()
    gz = f.with_name(f.name + '.gz')  # the bundle ships the audio template gzipped
    return gzip.decompress(gz.read_bytes()) if gz.is_file() else None


def game_hashes(root: Path, manifest=None):
    """sha256 per game over everything that decides what its extraction writes: this step's code, the exporter
    scripts it runs, c2obj.py, and the decomp files it reads (manifest paths + contents; a file that is not
    there hashes as `absent`).  `root` is a repo checkout or a bundle's byor-src; the manifest only exists in a
    checkout, so the bundle gets its hashes precomputed (exporters.json)."""
    manifest = manifest or Path(__file__).resolve().parent / 'decomp-manifest.txt'
    lines = manifest.read_text().split('\n') if manifest.is_file() else []
    c2 = root / 'reference/c2obj.py'
    out = {}
    for g, scripts in GAME_SCRIPTS.items():
        h = hashlib.sha256()
        h.update(f'pipeline {VERSION}\n'.encode())
        for fn in [DO[g]] + ([build_music] if g == 'sm64' else []):
            h.update(_sha(inspect.getsource(fn).encode()).encode())
        for rel in sorted(scripts):
            b = _script_bytes(root, rel)
            h.update(f'{rel}\0{_sha(b) if b is not None else "absent"}\n'.encode())
        h.update(f'c2obj\0{_sha(c2.read_bytes()) if c2.is_file() else "absent"}\n'.encode())
        for line in lines:
            if line and line.split('/')[0] == GAME_TREE[g]:
                f = root / 'reference' / line
                h.update(f'{line}\0{_sha(f.read_bytes()) if f.is_file() else "absent"}\n'.encode())
        out[g] = h.hexdigest()
    return out


def expected_hashes(src: Path):
    """The bundle's precomputed hashes (byor-src/exporters.json), else computed from a checkout."""
    f = src / 'exporters.json'
    if f.is_file():
        return json.loads(f.read_text())['games']
    return game_hashes(src)


def stamp_state(data: Path, game: str, expected):
    """'missing' | 'stale' | 'current' - the same rule as `stamp_state` in src/byor.rs."""
    f = data / 'stamps' / f'{game}.json'
    if not f.is_file():
        return 'missing'
    try:
        st = json.loads(f.read_text())
    except ValueError:
        return 'stale'
    if st.get('version') != VERSION or (expected and st.get('hash') != expected):
        return 'stale'
    return 'current'


def remembered_rom(data: Path, game: str):
    """The ROM path of the last successful import of `game`, if the file is still there and still identifies as it."""
    try:
        path = Path(json.loads((data / 'roms.json').read_text())[game]['path'])
        return path if path.is_file() and identify(path)[0] == game else None
    except (OSError, ValueError, KeyError, TypeError):
        return None


def remember_rom(data: Path, game: str, path: Path):
    f = data / 'roms.json'
    try:
        d = json.loads(f.read_text())
    except (OSError, ValueError):
        d = {}
    d[game] = {'path': str(path.resolve())}
    f.write_text(json.dumps(d, indent=1))


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
    prog(game, 50, 'Placing SM64 enemies'); run([sys.executable, 'scripts/export-sm64-g1-actors.py'], stg, game, 50, 51)
    prog(game, 51, 'Placing SM64 platforms and Bowser arenas'); run([sys.executable, 'scripts/export-sm64-solids.py'], stg, game, 51, 52)
    prog(game, 52, 'Placing SM64 bosses'); run([sys.executable, 'scripts/export-sm64-bosses.py'], stg, game, 52, 52)
    prog(game, 52, 'Placing SM64 castle NPCs, coins and boxes'); run([sys.executable, 'scripts/export-sm64-castle.py'], stg, game, 52, 52)
    prog(game, 50, 'Exporting models and animations'); run([sys.executable, 'scripts/export-sm64.py'], stg, game, 50, 92)
    # export-courses drops decor whose glb has no meshes, which it can only judge once the models exist
    # (the repo's committed courses were produced by a second run for the same reason).
    prog(game, 91, 'Finalising courses'); run([sys.executable, 'scripts/export-courses.py'], stg, game, 91, 92)
    run([sys.executable, 'scripts/export-sm64-g1-actors.py'], stg, game, 92, 92)
    run([sys.executable, 'scripts/export-sm64-solids.py'], stg, game, 92, 92)
    run([sys.executable, 'scripts/export-sm64-bosses.py'], stg, game, 92, 92)
    run([sys.executable, 'scripts/export-sm64-castle.py'], stg, game, 92, 92)
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
    prog(game, 5, 'Exporting the 20 courses (kart + walkable) and karts')
    run([sys.executable, 'scripts/export-mk64.py', str(p)], stg, game, 5, 94)
    # The CPU drivers' per-course speed tables, path behaviours and the racers' kart tables: public/mk64/cpu-ai.json.
    prog(game, 94, 'Exporting the CPU driver data')
    run([sys.executable, 'scripts/export-mk64-cpu.py', str(p)], stg, game, 94, 96)
    # The sound library's sequences, instrument banks and samples go into the checked-in engine template
    # (scripts/music/mk64-template.wasm holds the code only): assets/audio/music/mk64/mk64-audio.wasm.
    prog(game, 96, 'Building the Mario Kart 64 music engine')
    run([sys.executable, 'scripts/export-mk64-music.py', str(p)], stg, game, 96, 99)


def do_oot(stg, rom, game='oot'):
    (stg / 'reference/roms/oot-us-1.2.z64').write_bytes(rom)
    prog(game, 5, 'Exporting Link')
    run([sys.executable, 'scripts/export-oot-link.py'], stg, game, 5, 26)
    prog(game, 26, 'Exporting the item models'); run([sys.executable, 'scripts/export-oot-items.py'], stg, game, 26, 27)
    prog(game, 26, 'Exporting Epona'); run([sys.executable, 'scripts/export-oot-epona.py'], stg, game, 26, 30)
    (stg / 'assets/oot-dialogue').mkdir(parents=True, exist_ok=True)
    prog(game, 30, 'Exporting Kokiri Forest, Deku Tree and Lon Lon Ranch')
    run([sys.executable, 'scripts/export-oot-courses.py'], stg, game, 30, 96)
    prog(game, 96, 'Exporting the OoT enemies')
    run([sys.executable, 'scripts/export-oot-enemies.py'], stg, game, 96, 99)
    prog(game, 99, 'Exporting Link voices'); run([sys.executable, 'scripts/export-character-voices.py'], stg, game, 99, 100)
    prog(game, 99, 'Exporting item and combat sounds'); run([sys.executable, 'scripts/export-oot-sfx.py'], stg, game, 99, 100)


def do_bk(stg, rom, game='bk'):
    (stg / 'reference/roms/bk-us-1.0.z64').write_bytes(rom)
    prog(game, 5, 'Exporting Banjo and Kazooie')
    run([sys.executable, 'scripts/export-bk-banjo.py'], stg, game, 5, 35)
    prog(game, 35, 'Exporting Banjo move clips'); run([sys.executable, 'scripts/export-bk-banjo.py', '--extras'], stg, game, 35, 40)
    # worlds, then every sub-area (interiors, lair rooms, boss rooms); the map catalogue is scripts/data/bk-catalogue.json
    prog(game, 40, 'Exporting Banjo-Kazooie worlds and sub-areas')
    run([sys.executable, 'scripts/export-bk-levels.py'], stg, game, 40, 90)
    prog(game, 90, 'Exporting Banjo-Kazooie enemies'); run([sys.executable, 'scripts/export-bk-enemies.py'], stg, game, 90, 96)
    # the setup files add the enemy and collectible placements to the world courses written just above
    # ... and the warps (doors, pipes, windows, the lair's world doors), exits and arrival points of every map
    prog(game, 96, 'Placing enemies, collectibles and doors'); run([sys.executable, 'scripts/export-bk-setup.py'], stg, game, 96, 99)
    prog(game, 99, 'Exporting Banjo voices'); run([sys.executable, 'scripts/export-character-voices.py'], stg, game, 99, 100)
    prog(game, 99, 'Exporting Banjo-Kazooie sound effects'); run([sys.executable, 'scripts/export-bk-sfx.py'], stg, game, 99, 100)


DO = {'sm64': do_sm64, 'mk64': do_mk64, 'oot': do_oot, 'bk': do_bk}


def rel_files(stg: Path):
    return {str(p.relative_to(stg)) for top in ('assets', 'public') for p in (stg / top).rglob('*') if p.is_file()}


def owned_by_others(data: Path, game: str):
    keep = set()
    for f in (data / 'stamps').glob('*.json'):
        if f.stem != game:
            try:
                keep |= set(json.loads(f.read_text()).get('files', []))
            except (OSError, ValueError):
                pass
    return keep


def install(stg: Path, data: Path, game: str, owned):
    """Swap `game`'s freshly built output (already complete in the stage dir, which sits inside the data dir so
    every move is a same-volume rename) over the install, then drop files the previous import produced that this
    one no longer does.  Nothing here runs unless the whole extraction succeeded, so a failed or cancelled import
    leaves the previous install untouched.  The stamp goes first (a crash mid-swap reads as 'not installed', never
    as a current install with mixed files); the caller rewrites it last."""
    stamp = data / 'stamps' / f'{game}.json'
    try:
        old = set(json.loads(stamp.read_text()).get('files', []))
    except (OSError, ValueError):
        old = set()
    stamp.unlink(missing_ok=True)
    for rel in sorted(rel_files(stg)):
        d = data / rel
        d.parent.mkdir(parents=True, exist_ok=True)
        os.replace(stg / rel, d)
    for rel in sorted(old - set(owned) - owned_by_others(data, game)):
        (data / rel).unlink(missing_ok=True)
        parent = (data / rel).parent
        while parent != data and parent.is_dir() and not any(parent.iterdir()):
            parent.rmdir(); parent = parent.parent


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--source-root', type=Path, required=True)
    ap.add_argument('--data-dir', type=Path, required=True)
    ap.add_argument('--identify', type=Path, help='print the game id of a ROM file and exit')
    ap.add_argument('--status', action='store_true', help='print `@@STATUS <game> <missing|stale|current> [remembered ROM]` per game')
    ap.add_argument('--print-hashes', action='store_true', help='print the exporters.json the packager bundles')
    for g in ROMS:
        ap.add_argument(f'--{g}', type=Path)
    a = ap.parse_args()
    if a.identify:
        g, _ = identify(a.identify)
        say(g or 'unknown'); return 0 if g else 3
    if a.print_hashes:
        say(json.dumps({'version': VERSION, 'games': game_hashes(a.source_root)}, indent=1)); return 0
    if a.status:
        exp = expected_hashes(a.source_root)
        for g in ROMS:
            state = stamp_state(a.data_dir, g, exp.get(g))
            rom = remembered_rom(a.data_dir, g) if state == 'stale' else None
            say(f'@@STATUS {g} {state} {rom or ""}'.rstrip())
        return 0
    roms, paths = {}, {}
    for g in ROMS:
        path = getattr(a, g)
        if path:
            found, data = identify(path)
            if found != g:
                say(f'@@FAIL {g} {path.name} is not a recognised {ROMS[g][0]} ROM'); continue
            roms[g] = data; paths[g] = path
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
            before = rel_files(stg)  # only the original (non-ROM) assets so far
            DO[g](stg, rom)
            owned = sorted(rel_files(stg) - before)
            install(stg, data_dir, g, owned)
            remember_rom(data_dir, g, paths[g])
            stamp = {'version': VERSION, 'game': g, 'time': int(time.time()), 'hash': expected_hashes(a.source_root).get(g), 'files': owned}
            tmp = data_dir / 'stamps' / f'{g}.json.tmp'
            tmp.write_text(json.dumps(stamp)); os.replace(tmp, data_dir / 'stamps' / f'{g}.json')
            say(f'@@DONE {g}')
        except Exception as e:  # noqa: BLE001 - report and continue with the next game
            failed = True
            say(f'@@FAIL {g} {str(e)[-600:]}'.replace('\n', ' | '))
        finally:
            shutil.rmtree(stg, ignore_errors=True)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
