"""compare-data-dir.py REPO DATA : byte-compare assets/ and public/ of the repo checkout against an extracted
BYOR data dir. Expected: identical except the documented door LOD variants (see native/BYOR.md) and README files."""
import sys, filecmp, os
from pathlib import Path
repo, data = Path(sys.argv[1]), Path(sys.argv[2])
ignore_only_repo = set(sys.argv[3:])
tot = same = 0
diff, only_data, only_repo = [], [], []
for top in ('assets', 'public'):
    seen = set()
    for base, dirs, files in os.walk(data / top):
        for f in files:
            p = Path(base) / f
            rel = p.relative_to(data)
            seen.add(rel)
            r = repo / rel
            tot += 1
            if not r.is_file():
                only_data.append(str(rel))
            elif filecmp.cmp(p, r, shallow=False):
                same += 1
            else:
                diff.append(str(rel))
    for base, dirs, files in os.walk(repo / top):
        for f in files:
            rel = (Path(base) / f).relative_to(repo)
            if rel not in seen:
                only_repo.append(str(rel))
print(f'data files {tot}, byte-identical {same}, differ {len(diff)}, only in data {len(only_data)}, only in repo {len(only_repo)}')
for name, l in (('DIFFER', diff), ('ONLY-DATA', only_data), ('ONLY-REPO', only_repo)):
    for x in sorted(l):
        print(name, x)
