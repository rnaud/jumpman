#!/usr/bin/env python3
"""Build `byor-src.zip`: the ROM-free inputs of the on-device SM64 import (native/ANDROID_IMPORT.md).

  make-byor-src.py OUT.zip [--repo-root DIR]

Contents (all paths relative to the zip root, which the app unpacks to `<files>/byor-src`):
  reference/<game>/...                  exactly the decomp files listed for the ported games in byor/decomp-manifest.txt (code and text only)
  scripts/music/{sm64-us,mk64}-template.*  the data-less music engine templates (the ROM blobs are zero)
  native/byor/sm64-us-sound-player.bin  the assembled decomp sound-player sequence (a decomp source product)
  assets/ui, assets/audio/*.json, public/models/quaternius, public/mk64/item-*.png, public/icon-*, favicon.*, manifest:
                                        the original (non-Nintendo) assets every data dir carries (ORIGINAL in pipeline.py)
`verify-apk.py` re-checks every member of the zip against this allowlist.
"""
import sys
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT_LIST = ['scripts/music/sm64-us-template.wasm', 'scripts/music/sm64-us-template.json',
               'scripts/music/mk64-template.wasm', 'scripts/music/mk64-template.json']
# decomp trees (first path component in decomp-manifest.txt) of the games the pure-Rust importer has ported
PORTED_TREES = ('sm64/', 'mk64/')


def members(repo: Path):
    manifest = (repo / 'native/byor/decomp-manifest.txt').read_text().split('\n')
    for line in manifest:
        if line.startswith(PORTED_TREES):
            yield f'reference/{line}', repo / 'reference' / line
    for rel in SCRIPT_LIST + ['native/byor/sm64-us-sound-player.bin', 'assets/audio/kart-voices.json',
                              'assets/audio/character-sounds.json', 'public/manifest.webmanifest']:
        yield rel, repo / rel
    for base in ('assets/ui', 'public/models/quaternius'):
        for p in sorted((repo / base).rglob('*')):
            if p.is_file() and p.suffix != '.md':
                yield str(p.relative_to(repo)), p
    for pattern in ('public/mk64/item-*.png', 'public/icon-*', 'public/favicon.*'):
        for p in sorted(repo.glob(pattern)):
            yield str(p.relative_to(repo)), p


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    repo = Path(sys.argv[sys.argv.index('--repo-root') + 1]) if '--repo-root' in sys.argv else HERE.parents[1]
    if not args:
        sys.exit(__doc__)
    out = Path(args[0])
    out.parent.mkdir(parents=True, exist_ok=True)
    n = 0
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for rel, path in members(repo):
            if not path.is_file():
                if rel.startswith(('public/icon', 'public/favicon', 'public/manifest')):
                    continue
                sys.exit(f'missing {path}')
            z.write(path, rel)
            n += 1
    print(f'{out}: {n} files, {out.stat().st_size / 1e6:.1f} MB')


if __name__ == '__main__':
    main()
