#!/usr/bin/env python3
"""Fail if a bring-your-own-ROM app bundle contains any ROM or ROM-derived file.

  verify-byor-app.py [path/to/Jumpman-BYOR.app] [--repo-root DIR]

Checks, over every file of the bundle:
  1. ROM magic: the first four bytes are a z64/v64/n64 header (0x80371240, 0x37804012, 0x40123780).
  2. File names/extensions of ROM dumps (*.z64, *.n64, *.v64, *.rom, baserom*).
  3. Allowlist: the bundle's staged `byor-src/assets` and `byor-src/public` hold ONLY the original
     (non-Nintendo) assets named in ORIGINAL below, and no other game asset folder exists anywhere.
  4. Denylist: path patterns of everything the exporters produce (see native/BYOR.md "What is extracted").
  5. Binary assets inside the decomp source trees (`byor-src/reference`): none of png/jpg/glb/wav/aiff/aifc/
     m64/bin/tbl/ctl/z64 may appear (the decomps carry code and text, no ROM data).
  6. Content hashes: no file in the bundle is byte-identical to a ROM-derived file of the repo checkout
     (every file under assets/ and public/ that is not an ORIGINAL asset) when --repo-root is given or detectable.
Exit status 0 = clean, 1 = violations (listed).
"""
import fnmatch
import hashlib
import os
import sys
from pathlib import Path

MAGICS = {bytes.fromhex('80371240'): 'z64', bytes.fromhex('37804012'): 'v64', bytes.fromhex('40123780'): 'n64'}
ROM_SUFFIXES = {'.z64', '.n64', '.v64', '.rom'}
# Original assets that may ship (relative to byor-src or a repo root).
ORIGINAL = ['assets/ui/*', 'assets/audio/kart-voices.json', 'assets/audio/character-sounds.json',
            'public/models/quaternius/*', 'public/mk64/item-*.png', 'public/icon-*', 'public/favicon.*',
            'public/manifest.webmanifest']
# Anything matching these is ROM-derived (relative path anywhere in the bundle).
DENY = ['*/assets/actors/*', '*/assets/levels/*', '*/assets/extracted/*', '*/assets/courses/*',
        '*/assets/oot-dialogue/*', '*/assets/animations.json', '*/assets/layouts.json', '*/assets/export-report.json',
        '*/assets/audio/music/*', '*/assets/audio/manifest.json', '*/assets/audio/sfx_*', '*/assets/audio/instruments/*',
        '*/assets/audio/bowser_organ/*', '*/assets/audio/course_start/*', '*/assets/audio/piranha_music_box/*',
        '*/public/models/oot/*', '*/public/models/bk/*', '*/public/mk64/gLR*', '*/public/mk64/hud-*',
        '*/public/mk64/lakitu-*', '*/public/mk64/*-kart.png', '*/public/mk64/mk64-*', '*/public/mk64/luigi-raceway*',
        '*/public/mk64/kart-*']
BINARY_IN_SOURCE = {'.png', '.jpg', '.jpeg', '.glb', '.gltf', '.wav', '.aiff', '.aifc', '.m64', '.bin', '.tbl', '.ctl',
                    '.z64', '.n64', '.v64', '.mp3', '.ogg', '.o', '.a'}


def matches(rel, patterns):
    return any(fnmatch.fnmatch(rel, p) for p in patterns)


def sha(path):
    h = hashlib.sha1()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    repo = None
    if '--repo-root' in sys.argv:
        repo = Path(sys.argv[sys.argv.index('--repo-root') + 1])
        args = [a for a in args if a != str(repo)]
    app = Path(args[0] if args else Path(__file__).resolve().parents[1] / 'target/Jumpman-BYOR.app')
    if repo is None:
        guess = Path(__file__).resolve().parents[2]
        repo = guess if (guess / 'assets').is_dir() else None
    if not app.is_dir():
        sys.exit(f'{app}: not found')
    bad = []
    files = []
    for base, dirs, names in os.walk(app):
        for n in names:
            p = Path(base) / n
            if p.is_symlink():
                continue
            files.append(p)
    n_src_binary = 0
    for p in files:
        rel = '/' + str(p.relative_to(app))
        try:
            with open(p, 'rb') as f:
                head = f.read(4)
        except OSError:
            continue
        if head in MAGICS:
            bad.append(f'ROM magic ({MAGICS[head]} header): {rel}')
        if p.suffix.lower() in ROM_SUFFIXES or p.name.lower().startswith('baserom'):
            bad.append(f'ROM file name: {rel}')
        if '/byor-src/reference/' not in rel and matches(rel, DENY):
            bad.append(f'ROM-derived path: {rel}')
        if '/byor-src/reference/' in rel and p.suffix.lower() in BINARY_IN_SOURCE:
            bad.append(f'binary asset inside decomp source tree: {rel}')
            n_src_binary += 1
    # Staged asset folders: allowlist only.
    for top in ('assets', 'public'):
        for p in files:
            rel = str(p.relative_to(app))
            marker = f'/byor-src/{top}/'
            i = ('/' + rel).find(marker)
            if i >= 0:
                sub = ('/' + rel)[i + len('/byor-src/'):]
                if not matches(sub, ORIGINAL):
                    bad.append(f'not on the original-asset allowlist: /{rel}')
    # Content hashes against the repo's ROM-derived files.
    checked = 0
    if repo and (repo / 'assets').is_dir():
        derived = {}
        for top in ('assets', 'public'):
            for base, dirs, names in os.walk(repo / top):
                for n in names:
                    p = Path(base) / n
                    rel = str(p.relative_to(repo))
                    if not matches(rel, ORIGINAL) and p.stat().st_size > 0 and not n.endswith('.md'):
                        derived[(p.stat().st_size)] = derived.get(p.stat().st_size, []) + [p]
        sizes = set(derived)
        hashes = {}
        for p in files:
            if p.stat().st_size in sizes:
                hashes.setdefault(p.stat().st_size, []).append(p)
        for size, ps in hashes.items():
            theirs = {sha(q) for q in derived[size]}
            for p in ps:
                checked += 1
                if sha(p) in theirs:
                    bad.append(f'byte-identical to a ROM-derived repo file: /{p.relative_to(app)}')
    total = sum(p.stat().st_size for p in files)
    print(f'{app}: {len(files)} files, {total / 1e6:.1f} MB scanned; {checked} size-matched candidates hashed')
    if bad:
        print(f'FAIL: {len(bad)} violation(s)')
        for b in bad[:60]:
            print('  ' + b)
        sys.exit(1)
    print('OK: no ROM data, ROM magic, ROM-derived path or ROM-derived file content in the bundle')


if __name__ == '__main__':
    main()
