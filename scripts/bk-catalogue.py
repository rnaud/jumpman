"""Banjo-Kazooie map catalogue, read from the n64decomp sources and cached as scripts/data/bk-catalogue.json.

The exporters (export-bk-levels.py, export-bk-setup.py) only need the cache, so a ROM-only checkout works; run this
script with `reference/banjo-kazooie` populated to regenerate it:

    git submodule update --init --depth 1 reference/banjo-kazooie
    python3 scripts/bk-catalogue.py

Sources used (all in n64decomp/banjo-kazooie):
* `src/core2/mapModel.c` `sMapModelDescriptions`: the OPA / XLU level model of every map with geometry;
* `include/enums.h` `enum map_e` (map ids and names) and the `WARP_*` exit-id enums of every map;
* `src/core2/code_AD110.c` `sWarpFunctions`: a category-3 setup node (a "warp", a sphere trigger) names its handler by
  `actorId`; the handler (`warp_*` in `src/core2/code_956B0.c`) calls `_func_8031CC8C(node, MAP_x, WARP_y)`, i.e.
  "load map x and arrive at its exit y";
* `src/core2/gccube.c` `nodeprop_getExitActorId`: an exit id is marked in the target map's setup by a node whose actor
  id is looked up there; the player spawns at that node's position facing its yaw (`player_spawnAtMapExit`).
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DECOMP = ROOT / 'reference/banjo-kazooie'
OUT = ROOT / 'scripts/data/bk-catalogue.json'


def hex_of(name):
    return int(re.match(r'[A-Z]+_([0-9A-F]+)_', name).group(1), 16)


def parse_enum_values(body):
    """name -> value for an enum body (explicit `= n` values and implicit increments)."""
    vals, nxt = {}, 0
    for item in re.sub(r'//[^\n]*', '', re.sub(r'/\*.*?\*/', '', body, flags=re.S)).split(','):
        item = item.strip()
        if not item:
            continue
        m = re.match(r'(\w+)\s*(?:=\s*(\w+))?$', item)
        if not m:
            continue
        if m.group(2) is not None:
            nxt = int(m.group(2), 0) if m.group(2)[0].isdigit() else vals.get(m.group(2), nxt)
        vals[m.group(1)] = nxt
        nxt += 1
    return vals


def main():
    enums = (DECOMP / 'include/enums.h').read_text()
    map_ids = {n: hex_of(n) for n in set(re.findall(r'\bMAP_[0-9A-F]+_\w+', enums))}  # MAP_x names carry their hex id
    warp_vals = {}
    for m in re.finditer(r'enum\s+\w+\s*\{(.*?)\};', enums, re.S):
        if 'WARP_' in m.group(1):
            warp_vals.update({k: v for k, v in parse_enum_values(m.group(1)).items() if k.startswith('WARP_')})

    table = (DECOMP / 'src/core2/mapModel.c').read_text()
    models = {}
    for m in re.finditer(r'\{(MAP_[0-9A-F]+_\w+),\s*(ASSET_[0-9A-F]+_\w+|0),\s*(ASSET_[0-9A-F]+_\w+|0)', table):
        models[hex_of(m.group(1))] = dict(name=m.group(1), opa=hex_of(m.group(2)), xlu=hex_of(m.group(3)) if m.group(3) != '0' else 0)

    ad = (DECOMP / 'src/core2/code_AD110.c').read_text()
    arr = re.search(r'sWarpFunctions\[\]\s*=\s*\{(.*?)\};', ad, re.S).group(1)
    fns = [x.strip() for x in arr.replace('\n', ' ').split(',') if x.strip()]

    bodies = {}
    for p in (DECOMP / 'src').rglob('*.c'):
        t = p.read_text(errors='ignore')
        for m in re.finditer(r'\nvoid (\w+)\(s32 arg0, s32 arg1\)\s*\{(.*?)\n\}', t, re.S):
            bodies[m.group(1)] = m.group(2)
    warps = {}
    for i, fn in enumerate(fns):
        body = bodies.get(fn)
        if not body:
            continue
        m = re.search(r'(MAP_[0-9A-F]+_\w+)\s*,\s*(\w+)', body)
        if not m:
            continue
        ex = m.group(2)
        exit_id = warp_vals.get(ex, int(ex, 0) if re.match(r'(0x[0-9A-Fa-f]+|\d+)$', ex) else None)
        if exit_id is None:
            continue
        warps[i] = dict(fn=fn, map=hex_of(m.group(1)), exit=exit_id, exitName=ex)

    code = (DECOMP / 'src/core2/gccube.c').read_text()
    sw = re.search(r'nodeprop_getExitActorId\(s32 exitId\)\{(.*?)\n\}', code, re.S).group(1)
    exits = {int(m.group(1), 16): hex_of(m.group(2)) for m in re.finditer(r'case (0x[0-9A-F]+): var_v1 = (ACTOR_[0-9A-F]+_\w+)', sw)}

    OUT.parent.mkdir(exist_ok=True)
    OUT.write_text(json.dumps(dict(maps=models, warps=warps, exitActors=exits, warpFunctionCount=len(fns)), indent=1, sort_keys=True))
    print(len(models), 'maps', len(warps), 'warp handlers resolved of', len(fns), len(exits), 'exit actors')


if __name__ == '__main__':
    main()
