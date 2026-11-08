"""SM64 round 2, part A (bosses and big enemies): add the placements that have a native behavior
port (native/sim/src/behaviors/r2a) to the already exported assets/courses/*.json.

Run AFTER scripts/export-courses.py, export-sm64-g1-actors.py and export-sm64-solids.py (those
rewrite the course files and do not know these objects); idempotent: previously added entries carry
`"r2a": true` and are replaced. The entries also carry `"g2": true` so the loader treats them like the
group-2 actors (only loaded in the app, never in the legacy golden fixtures), and the collision models
the objects switch between are added to `extraCollision` under `g2/<symbol>`
(behaviors/g2/dyn_collision.rs).

    python3 scripts/export-sm64-bosses.py

Reads reference/sm64 (level scripts, macro files, collision). No ROM needed.
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_sm64_g2 as g2  # noqa: E402  (clean, call_args, evaluate, act_list, parse_collision)

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
OUT = ROOT / 'assets/courses'

# decomp level-script behavior -> dict(kind, model, collisions)
#   model: path under assets/ (None = invisible); collisions: extra decomp collision symbols the behavior loads at runtime.
BHV = {
    'bhvEyerokBoss': dict(kind='eyerokBoss', model=None, collisions=[
        'ssl_seg7_collision_07028274', 'ssl_seg7_collision_070282F8', 'ssl_seg7_collision_07028370', 'ssl_seg7_collision_070284B0']),
    'bhvWigglerHead': dict(kind='wigglerHead', model='actors/wiggler_head/wiggler_head_geo.glb', collisions=[]),
    'bhvMrI': dict(kind='mrI', model='actors/mr_i_eyeball/mr_i_geo.glb', collisions=[]),
    'bhvMerryGoRoundBooManager': dict(kind='merryGoRoundBooManager', model=None, collisions=[]),
    'bhvDorrie': dict(kind='dorrie', model='actors/dorrie/dorrie_geo.glb', collisions=['dorrie_seg6_collision_0600F644', 'dorrie_seg6_collision_0600FBB8']),
    'bhvUnagi': dict(kind='unagi', model='actors/unagi/unagi_geo.glb', collisions=[]),
    'bhvMantaRay': dict(kind='mantaRay', model='actors/manta/manta_seg5_geo_05008D14.glb', collisions=[]),
    'bhvJetStreamRingSpawner': dict(kind='jetStreamRingSpawner', model=None, collisions=[]),
    'bhvTweester': dict(kind='tweester', model='actors/tornado/tweester_geo.glb', collisions=[]),
    'bhvHoot': dict(kind='hoot', model='actors/hoot/hoot_geo.glb', collisions=[]),
}
# macro preset -> dict(kind, model, param)
MACRO = {
    'macro_mr_i': dict(kind='mrI', model='actors/mr_i_eyeball/mr_i_geo.glb', param=0),
}
# (course file stem, level folder, area) -> stars the area's bosses spawn with spawn_default_star: [(act, [x, y, z])]
TARGETS = [
    ('ssl.3', 'ssl', 3),
    ('thi.3', 'thi', 3),
    ('bbh', 'bbh', 1),
    ('lll', 'lll', 1),
    ('hmc', 'hmc', 1),
    ('jrb', 'jrb', 1),
    ('jrb.2', 'jrb', 2),
    ('ddd', 'ddd', 1),
    ('ddd.2', 'ddd', 2),
    ('ssl', 'ssl', 1),
    ('wf', 'wf', 1),
]
EVENT_STARS = {
    'ssl.3': [(4, [0, -900, -3700])],
    'thi.3': [(6, [0, 2048, 0])],
    'bbh': [(2, [-1600, -2100, 205]), (6, [1370, 2000, -320])],
    'jrb': [(2, [6833, -3654, 2230])],
    'ddd': [(5, [-3180, -3600, 120])],
    'ddd.2': [(4, [3400, -3200, -500])],
}


def area_commands(script, index):
    source = g2.clean(script)
    area = re.search(r'\bAREA\(\s*' + str(index) + r'\s*,[^)]*\),(.*?)END_AREA\(', source, re.S)
    blocks = dict(re.findall(r'LevelScript\s+(\w+)\[\]\s*=\s*\{(.*?)\};', source, re.S))

    def expand(body):
        return re.sub(r'JUMP_LINK\(\s*(\w+)\s*\)', lambda m: expand(blocks[m[1]]), body)

    return expand(area[1])


def build(level, area):
    folder = REF / 'levels' / level
    area_script = area_commands((folder / 'script.c').read_text(), area)
    out = []
    for command, a in g2.call_args(area_script, ['OBJECT_WITH_ACTS', 'OBJECT']):
        bhv = a[-2] if command == 'OBJECT_WITH_ACTS' else a[-1]
        if bhv not in BHV:
            continue
        spec = BHV[bhv]
        obj = dict(behavior=spec['kind'], pos=[int(float(v)) for v in a[1:4]], yaw=int(a[5]), param=g2.evaluate(a[7]), g2=True, r2a=True)
        if spec.get('model'):
            obj['model'] = spec['model']
        if command == 'OBJECT_WITH_ACTS':
            obj['acts'] = g2.act_list(a[-1])
        out.append(obj)
    macro = folder / f'areas/{area}/macro.inc.c'
    if macro.exists():
        for command, m in g2.call_args(macro.read_text(), ['MACRO_OBJECT', 'MACRO_OBJECT_WITH_BHV_PARAM']):
            preset = m[0].strip()
            if preset in MACRO:
                spec = MACRO[preset]
                param = (g2.evaluate(m[5]) & 0xFF) << 16 if command == 'MACRO_OBJECT_WITH_BHV_PARAM' else spec.get('param', 0)
                obj = dict(behavior=spec['kind'], pos=[int(float(v)) for v in m[2:5]], yaw=int(m[1]), param=param, g2=True, r2a=True)
                if spec.get('model'):
                    obj['model'] = spec['model']
                out.append(obj)
    return out


def main():
    for stem, level, area in TARGETS:
        path = OUT / f'{stem}.json'
        data = json.loads(path.read_text())
        data['objects'] = [o for o in data['objects'] if not o.get('r2a')]
        added = build(level, area)
        data['objects'] += added
        symbols = set()
        for spec in BHV.values():
            if any(o['behavior'] == spec['kind'] for o in added):
                symbols.update(spec['collisions'])
        for symbol in sorted(symbols):
            mesh = g2.parse_collision(symbol)
            if mesh:
                data['extraCollision'][f'g2/{symbol}'] = mesh
        if stem in EVENT_STARS:
            data['eventStars'] = [dict(act=act, pos=pos) for act, pos in EVENT_STARS[stem]]
        else:
            data.pop('eventStars', None)
        path.write_text(json.dumps(data, separators=(',', ':')))
        counts = {}
        for o in added:
            counts[o['behavior']] = counts.get(o['behavior'], 0) + 1
        print(stem, counts)


if __name__ == '__main__':
    main()
