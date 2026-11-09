#!/usr/bin/env python3
"""trace_reads_rs.py: run the pure-Rust importer (byor-rs) over full copies of the decomp trees and record every file it reads.

  trace_reads_rs.py --byor-rs BIN --work DIR [--sm64 ROM] [--mk64 ROM] [--oot ROM] [--bk ROM] [--update]

Copies reference/{sm64,mk64,oot,banjo-kazooie} to DIR/ref, resets every atime to year 2000, runs the importer with that
tree as its source root (plus the repo's non-decomp inputs from a freshly built bundle), then lists files whose atime moved.
With --update the missing files are appended to decomp-manifest.txt. Use it whenever an importer starts reading a new
decomp file: the on-device bundle only ships the files in the manifest.
"""
import argparse, os, shutil, subprocess, sys, tempfile, time, zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
TREES = ('sm64', 'mk64', 'oot', 'banjo-kazooie')
OLD = 946684800 * 10**9


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--byor-rs', type=Path, required=True)
    ap.add_argument('--work', type=Path, required=True)
    ap.add_argument('--update', action='store_true')
    for g in ('sm64', 'mk64', 'oot', 'bk'):
        ap.add_argument(f'--{g}', type=Path)
    a = ap.parse_args()
    work = a.work
    ref = work / 'ref'
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    for t in TREES:
        shutil.copytree(REPO / 'reference' / t, ref / t, symlinks=True, ignore=shutil.ignore_patterns('.git'))
    n = 0
    for base, _, names in os.walk(ref):
        for f in names:
            p = os.path.join(base, f)
            if not os.path.islink(p):
                os.utime(p, ns=(OLD, os.stat(p).st_mtime_ns))
                n += 1
    # non-decomp inputs (music templates, catalogue, original assets) come from the real bundle
    z = work / 'bundle.zip'
    subprocess.run([sys.executable, str(REPO / 'native/android/make-byor-src.py'), str(z)], check=True)
    src = work / 'src'
    with zipfile.ZipFile(z) as zf:
        zf.extractall(src)
    shutil.rmtree(src / 'reference', ignore_errors=True)
    (src / 'reference').symlink_to(ref.resolve())
    start = time.time_ns() - 2 * 10**9
    argv = [str(a.byor_rs), '--source-root', str(src), '--data-dir', str(work / 'data')]
    for g in ('sm64', 'mk64', 'oot', 'bk'):
        if getattr(a, g):
            argv += [f'--{g}', str(getattr(a, g))]
    rc = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    fails = [l for l in rc.stdout.splitlines() if '@@FAIL' in l or 'FAILED' in l]
    read = set()
    for t in TREES:
        for base, _, names in os.walk(ref / t):
            for f in names:
                p = os.path.join(base, f)
                if os.path.islink(p):
                    continue
                st = os.stat(p)
                if st.st_atime_ns >= start and st.st_mtime_ns < start:
                    read.add(f'{t}/{os.path.relpath(p, ref / t)}')
    manifest = (HERE / 'decomp-manifest.txt').read_text().split('\n')
    have = {l.strip() for l in manifest if l.strip() and not l.startswith('#')}
    missing = sorted(read - have)
    print(f'importer rc={rc.returncode}; {len(read)} decomp files read; {len(missing)} not in the manifest; {len(fails)} failures')
    for l in fails[:10]:
        print('  ', l[:200])
    for m in missing[:60]:
        print('  missing', m)
    if a.update and missing:
        with (HERE / 'decomp-manifest.txt').open('a') as f:
            f.write('\n'.join(missing) + '\n')
        print('appended', len(missing), 'files to decomp-manifest.txt')
    return 1 if (missing or fails) and not a.update else 0


if __name__ == '__main__':
    sys.exit(main())
