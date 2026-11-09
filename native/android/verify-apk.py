#!/usr/bin/env python3
"""Fail if the Android APK contains ROM data or anything resembling extracted game assets, or is not a real arm64 app.

  verify-apk.py path/to/app.apk

The APK must carry code only: classes.dex, the arm64-v8a Rust library, the manifest and the launcher icon. The game data is
imported at runtime into the app's external files dir (native/ANDROID.md). Checks:
  1. the shared checks of `byor/verify-byor-app.py` (ROM magic, ROM file names, ROM-derived path patterns) on the unpacked APK;
  2. no `assets/` content except the one allowlisted `assets/byor-src.zip` (ROM-free decomp sources, music template and original
   assets for the on-device import; every member is checked by `check_byor_zip`), and no model / audio / texture / json resource
   outside `res/`;
  3. the only resources are launcher icons under `res/`;
  4. `lib/arm64-v8a/libjumpman_game.so` exists and is an AArch64 ELF; `AndroidManifest.xml` and `classes.dex` exist;
  5. no ROM magic anywhere inside the native library's first 4 bytes of any member, and no member larger than 400 MB.
Exit status 0 = clean.
"""
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
BYOR_ZIP = 'assets/byor-src.zip'  # the one allowed asset: the ROM-free inputs of the on-device import (check_byor_zip)
MAGICS = (bytes.fromhex('80371240'), bytes.fromhex('37804012'), bytes.fromhex('40123780'))
BAD_EXT = {'.glb', '.gltf', '.wav', '.aiff', '.aifc', '.ogg', '.mp3', '.m64', '.z64', '.n64', '.v64', '.rom', '.tbl', '.ctl',
           '.bin', '.json', '.jpg', '.jpeg', '.ktx2', '.mio0', '.bmp'}


def check_byor_zip(data: bytes, repo: Path):
    """Every member of byor-src.zip must be on the allowlist of make-byor-src.py (decomp files named in decomp-manifest.txt, the
    music template, the sound-player sequence and the original assets), carry no ROM magic, and the music template's four ROM blob
    ranges must be all zero (it is the engine code only)."""
    bad = []
    spec = importlib.util.spec_from_file_location('make_byor_src', HERE / 'make-byor-src.py')
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    allowed = {rel for rel, _ in mod.members(repo)}
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        for info in z.infolist():
            if info.filename not in allowed:
                bad.append(f'{BYOR_ZIP}: member not on the allowlist: {info.filename}')
            elif z.read(info.filename)[:4] in MAGICS:
                bad.append(f'{BYOR_ZIP}: ROM magic in {info.filename}')
            if info.file_size > 30 << 20:
                bad.append(f'{BYOR_ZIP}: large member {info.filename}')
        try:
            layout = json.loads(z.read('scripts/music/sm64-us-template.json'))
            wasm = z.read('scripts/music/sm64-us-template.wasm')
            for symbol, v in layout.items():
                if any(wasm[v['offset']:v['offset'] + v['length']]):
                    bad.append(f'{BYOR_ZIP}: music template blob {symbol} is not blank')
        except KeyError as e:
            bad.append(f'{BYOR_ZIP}: missing {e}')
    return bad


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    apk = Path(sys.argv[1])
    bad = []
    with zipfile.ZipFile(apk) as z:
        names = z.namelist()
        for n in names:
            p = Path(n)
            # AGP adds the androidx baseline profiles under assets/dexopt/ (compiled-code hints, not game data).
            if (n.startswith('assets/') or n == 'assets') and not n.startswith('assets/dexopt/') and n not in ('DebugProbesKt.bin', BYOR_ZIP, 'assets/'):  # kotlinx-coroutines debug probe blob
                bad.append(f'assets/ content in the APK: {n}')
            if p.suffix.lower() in BAD_EXT and not n.startswith('assets/dexopt/') and n != 'DebugProbesKt.bin':  # kotlinx-coroutines debug probe blob
                bad.append(f'asset-like file type: {n}')
            if p.suffix.lower() == '.png' and not n.startswith('res/'):
                bad.append(f'png outside res/: {n}')
            if z.getinfo(n).file_size > 400 << 20:
                bad.append(f'suspiciously large member: {n}')
        if BYOR_ZIP in names:
            bad.extend(check_byor_zip(z.read(BYOR_ZIP), HERE.parents[1]))
        for need in ('AndroidManifest.xml', 'classes.dex', 'lib/arm64-v8a/libjumpman_game.so'):
            if need not in names:
                bad.append(f'missing {need}')
        other_abis = [n for n in names if n.startswith('lib/') and not n.startswith('lib/arm64-v8a/')]
        if other_abis:
            bad.append(f'unexpected ABIs: {other_abis[:3]}')
        so = 'lib/arm64-v8a/libjumpman_game.so'
        if so in names:
            head = z.read(so)[:20]
            # ELF, 64-bit, little endian, e_machine 0xB7 (AArch64).
            if head[:4] != b'\x7fELF' or head[4] != 2 or head[5] != 1 or int.from_bytes(head[18:20], 'little') != 0xB7:
                bad.append(f'{so} is not an AArch64 ELF shared object')
        with tempfile.TemporaryDirectory() as tmp:
            z.extractall(tmp)
            r = subprocess.run([sys.executable, str(HERE.parent / 'byor' / 'verify-byor-app.py'), tmp], capture_output=True, text=True)
            sys.stdout.write(r.stdout)
            if r.returncode != 0:
                bad.append('verify-byor-app.py failed on the unpacked APK')
        print(f'{apk}: {len(names)} entries, {apk.stat().st_size / 1e6:.1f} MB')
        for n in sorted(names, key=lambda n: -z.getinfo(n).file_size)[:6]:
            print(f'  {z.getinfo(n).file_size / 1e6:8.1f} MB  {n}')
    if bad:
        print(f'FAIL: {len(bad)} violation(s)')
        for b in bad[:60]:
            print('  ' + b)
        sys.exit(1)
    print('OK: arm64 app, code only: no ROM data; the only asset is the audited assets/byor-src.zip')


if __name__ == '__main__':
    main()
