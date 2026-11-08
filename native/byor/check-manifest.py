#!/usr/bin/env python3
"""check-manifest.py: keep byor/decomp-manifest.txt (and scripts-manifest.txt) in step with the exporters.

  check-manifest.py [--repo-root DIR]
      static (no ROMs, runs anywhere; --reference DIR picks the trees): every manifest line exists in reference/ (trees that are not checked
      out are reported and skipped unless --strict), the list is sorted and unique, and every scripts/*.py
      that pipeline.py runs is in scripts-manifest.txt and exists.
  check-manifest.py --trace --sm64 ROM [--mk64 ROM] [--oot ROM] [--bk ROM] [--reference DIR] [--in-place]
      dynamic: runs the real pipeline (byor/trace_reads.py) over a scratch copy of the decomp trees and FAILS if it
      read a reference file that is not in the manifest.  Manifest entries that were not read are listed as
      unused (a failure with --strict-unused).  Only the games whose ROM is given are traced, so unused entries
      of the other games are not reported.

Maintenance: when an exporter starts reading a new decomp file this check fails and names it; append the lines it
prints to decomp-manifest.txt (`--update` does that and re-sorts), then re-run the end-to-end parity check.
"""
import argparse, re, shutil, sys, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
TREES = ('sm64', 'mk64', 'oot', 'banjo-kazooie')
GAME_TREE = {'sm64': 'sm64', 'mk64': 'mk64', 'oot': 'oot', 'bk': 'banjo-kazooie'}


def load(path):
    return [l.rstrip('\n') for l in path.read_text().splitlines() if l.strip()]


def static(repo: Path, strict: bool, ref: Path):
    errs = []
    manifest = load(HERE / 'decomp-manifest.txt')
    if manifest != sorted(set(manifest)):
        errs.append('decomp-manifest.txt is not sorted/unique (LC_ALL=C sort -u)')
    for t in TREES:
        lines = [m for m in manifest if m.split('/')[0] == t]
        d = ref / t
        if not d.is_dir() or not any(d.iterdir()):
            msg = f'reference/{t} is not checked out ({len(lines)} manifest lines not verified)'
            (errs if strict else []).append(msg)
            print('note:', msg)
            continue
        missing = [m for m in lines if not (ref / m).is_file()]
        errs += [f'manifest lists a file that no longer exists: {ref}/{m}' for m in missing]
    stray = [m for m in manifest if m.split('/')[0] not in TREES]
    errs += [f'unknown tree in manifest: {m}' for m in stray]
    scripts = load(HERE / 'scripts-manifest.txt')
    errs += [f'scripts-manifest.txt lists a missing script: scripts/{s}' for s in scripts if not (repo / 'scripts' / s).is_file()]
    import pipeline
    used = {s.replace('.wasm', '.wasm') for ss in pipeline.GAME_SCRIPTS.values() for s in ss}
    listed = set(scripts) | {'music/sm64-us-template.wasm'}
    for s in sorted(used ^ listed):
        errs.append(f'pipeline.GAME_SCRIPTS and scripts-manifest.txt disagree on scripts/{s}')
    for s in sorted(set(re.findall(r"scripts/([\w.-]+\.py)", (HERE / 'pipeline.py').read_text()))):
        if s not in scripts:
            errs.append(f'pipeline.py runs scripts/{s} but scripts-manifest.txt does not list it')
    print(f'static: {len(manifest)} decomp files, {len(scripts)} scripts checked')
    return errs


def dynamic(a, repo: Path):
    import trace_reads
    roms = {g: getattr(a, g) for g in GAME_TREE if getattr(a, g)}
    if not roms:
        sys.exit('--trace needs at least --sm64 ROM')
    src = a.reference or repo / 'reference'
    work = None
    if not a.in_place:
        work = Path(tempfile.mkdtemp(prefix='manifest-trace-'))
        for t in TREES:
            if (src / t).is_dir():
                shutil.copytree(src / t, work / t, ignore=shutil.ignore_patterns('.git'), symlinks=True)
        src = work
    data = Path(tempfile.mkdtemp(prefix='manifest-data-'))
    try:
        rc, read = trace_reads.trace(src.resolve(), data, {g: p.resolve() for g, p in roms.items()}, repo)
    finally:
        shutil.rmtree(data, ignore_errors=True)
        if work:
            shutil.rmtree(work, ignore_errors=True)
    manifest = set(load(HERE / 'decomp-manifest.txt'))
    traced = {GAME_TREE[g] for g in roms}
    new = sorted(set(read) - manifest)
    unused = sorted(m for m in manifest - set(read) if m.split('/')[0] in traced)
    errs = []
    if rc:
        errs.append(f'pipeline exited {rc}')
    errs += [f'read but not in the manifest: {n}' for n in new]
    if a.strict_unused:
        errs += [f'in the manifest but never read: {u}' for u in unused]
    elif unused:
        print(f'note: {len(unused)} manifest entries were not read in this run (e.g. {unused[0]})')
    if a.update and new:
        merged = sorted(manifest | set(new))
        (HERE / 'decomp-manifest.txt').write_text('\n'.join(merged) + '\n')
        print(f'updated decomp-manifest.txt with {len(new)} new lines')
        errs = [e for e in errs if not e.startswith('read but not')]
    print(f'dynamic: pipeline read {len(read)} reference files')
    return errs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--repo-root', type=Path, default=HERE.parents[1])
    ap.add_argument('--strict', action='store_true', help='fail when a reference tree is not checked out')
    ap.add_argument('--trace', action='store_true')
    ap.add_argument('--reference', type=Path)
    ap.add_argument('--in-place', action='store_true', help='trace the trees as they are (resets their atimes)')
    ap.add_argument('--strict-unused', action='store_true')
    ap.add_argument('--update', action='store_true', help='append newly read files to the manifest')
    for g in GAME_TREE:
        ap.add_argument(f'--{g}', type=Path)
    a = ap.parse_args()
    errs = static(a.repo_root, a.strict, a.reference or a.repo_root / 'reference')
    if a.trace:
        errs += dynamic(a, a.repo_root)
    for e in errs:
        print('FAIL:', e)
    print('OK' if not errs else f'{len(errs)} problem(s)')
    return 1 if errs else 0


if __name__ == '__main__':
    sys.exit(main())
