#!/usr/bin/env python3
"""trace_reads.py: run the real BYOR pipeline and record every file it reads under `reference/`.

  trace_reads.py --reference DIR --data-dir DIR [--sm64 ROM] [--mk64 ROM] [--oot ROM] [--bk ROM] [--out FILE]

DIR holds full copies of the decomp trees (`sm64 mk64 oot banjo-kazooie`, as in the repo's `reference/`).
It is traced IN PLACE, so give it a scratch copy: every file's atime is first reset to year 2000, the pipeline is
run in-process, and any file whose atime moved afterwards (and whose mtime did not) was read - by Python, by
make, cc, the SM64 asset tools, anything. The kernel does the recording, so child processes are covered
(fs_usage/dtruss would need sudo). Needs a filesystem that updates atime on read (APFS and ext4 relatime do).
The sm64 tree is copied into the pipeline's stage dir and deleted when done, so the stage copy is traced
(atimes reset after the copy, snapshot just before the stage is removed).

Output: one `<tree>/<path>` per line, relative to reference/ (the same format as decomp-manifest.txt).
"""
import argparse, os, shutil, sys, tempfile, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
TREES = ('sm64', 'mk64', 'oot', 'banjo-kazooie')
OLD = 946684800 * 10**9  # 2000-01-01


def reset_atimes(root: Path):
    n = 0
    for base, _, names in os.walk(root):
        for f in names:
            p = os.path.join(base, f)
            if os.path.islink(p):
                continue
            st = os.stat(p)
            os.utime(p, ns=(OLD, st.st_mtime_ns))
            n += 1
    return n


def read_files(root: Path, start_ns: int):
    """Files below root whose atime moved since start and whose content was not rewritten."""
    out = set()
    for base, _, names in os.walk(root):
        for f in names:
            p = os.path.join(base, f)
            if os.path.islink(p):
                continue
            st = os.stat(p)
            if st.st_atime_ns >= start_ns and st.st_mtime_ns < start_ns:
                out.add(os.path.relpath(p, root))
    return out


def trace(reference: Path, data_dir: Path, roms: dict, repo: Path = REPO):
    sys.path.insert(0, str(HERE))
    import pipeline
    for t in TREES:
        if (reference / t).is_dir():
            reset_atimes(reference / t)
    src = Path(tempfile.mkdtemp(prefix='trace-src-'))
    for d in ('scripts', 'assets', 'public'):
        os.symlink(repo / d, src / d)
    # Without an exporters.json the pipeline would hash every manifest file for the stamps, which reads them all.
    (src / 'exporters.json').write_text('{"version": 0, "games": {}}')
    (src / 'reference').mkdir()
    os.symlink(repo / 'reference/c2obj.py', src / 'reference/c2obj.py')
    for t in TREES:
        if (reference / t).is_dir():
            os.symlink((reference / t).resolve(), src / 'reference' / t)
    start_ns = time.time_ns() - 2 * 10**9
    seen = {t: set() for t in TREES}
    orig_stage, orig_rmtree = pipeline.stage, shutil.rmtree

    def stage(s, stg, games):
        orig_stage(s, stg, games)
        if (stg / 'reference/sm64').is_dir():  # copy2 re-stamps atime from the (just read) source
            reset_atimes(stg / 'reference/sm64')

    def rmtree(path, *a, **k):
        p = Path(path)
        for cand in ({p / 'reference/sm64', p} if p.name == '.stage' else {p}):
            if cand.name == 'sm64' and cand.parent.name == 'reference' and cand.is_dir():
                seen['sm64'] |= read_files(cand, start_ns)
        return orig_rmtree(path, *a, **k)

    pipeline.stage, shutil.rmtree = stage, rmtree
    argv = ['pipeline.py', '--source-root', str(src), '--data-dir', str(data_dir)]
    for g, r in roms.items():
        argv += [f'--{g}', str(r)]
    old_argv, sys.argv = sys.argv, argv
    try:
        rc = pipeline.main()
    finally:
        sys.argv, pipeline.stage, shutil.rmtree = old_argv, orig_stage, orig_rmtree
        shutil.rmtree(src, ignore_errors=True)
    for t in TREES[1:]:
        if (reference / t).is_dir():
            seen[t] = read_files(reference / t, start_ns)
    return rc, sorted(f'{t}/{r}' for t in TREES for r in seen[t])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--reference', type=Path, required=True)
    ap.add_argument('--data-dir', type=Path, required=True)
    ap.add_argument('--out', type=Path)
    for g in ('sm64', 'mk64', 'oot', 'bk'):
        ap.add_argument(f'--{g}', type=Path)
    a = ap.parse_args()
    roms = {g: getattr(a, g) for g in ('sm64', 'mk64', 'oot', 'bk') if getattr(a, g)}
    rc, files = trace(a.reference, a.data_dir, roms)
    text = '\n'.join(files) + '\n'
    (a.out.write_text(text) if a.out else sys.stdout.write(text))
    print(f'pipeline rc={rc}; {len(files)} reference files read', file=sys.stderr)
    return rc


if __name__ == '__main__':
    sys.exit(main())
