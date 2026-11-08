"""SM64 behavior group 1 (castle + first courses): add the placements that have a native
behavior port (native/sim/src/behaviors/g1) to the already exported assets/courses/*.json.

Run AFTER scripts/export-courses.py (that script rewrites the course files and does not know
these objects); it is idempotent: previously added entries carry `"g1": true` and are replaced.

    python3 scripts/export-sm64-g1-actors.py

Reads reference/sm64 (level scripts, macro files, trajectories, actor collision). No ROM needed.
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
OUT = ROOT / 'assets/courses'


def clean(text):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)


def args(text, command):
    text = re.sub(r'BPARAM([1-4])\(([^()]*)\)', lambda m: m[2] if m[1] == '2' else ('first_' + m[2] if m[1] == '1' else '0'), text)
    return [tuple(s.strip() for s in m.split(',')) for m in re.findall(r'\b' + command + r'\s*\(([^()]*)\)', clean(text))]


def act_list(expression):
    return [int(v) for v in re.findall(r'ACT_(\d+)', expression)]


def area_commands(script, index):
    source = clean(script)
    area = re.search(r'\bAREA\(\s*' + str(index) + r'\s*,[^)]*\),(.*?)END_AREA\(', source, re.S)
    blocks = dict(re.findall(r'LevelScript\s+(\w+)\[\]\s*=\s*\{(.*?)\};', source, re.S))

    def expand(body):
        return re.sub(r'JUMP_LINK\(\s*(\w+)\s*\)', lambda m: expand(blocks[m[1]]), body)

    return expand(area[1])


surfaces = {n: int(v, 0) for n, v in re.findall(r'#define\s+(SURFACE_\w+)\s+(0x[0-9A-Fa-f]+)', (REF / 'include/surface_terrains.h').read_text())}


def collision(path, symbol, scale=1.0):
    text = clean(path.read_text())
    text = re.search(r'\b' + symbol + r'\[\]\s*=\s*\{(.*?)\};', text, re.S)[1]
    vertices = [[round(int(v) * scale, 3) for v in a] for a in args(text, 'COL_VERTEX')]
    triangles, surface = [], 0
    for command, body in re.findall(r'\b(COL_TRI_INIT|COL_TRI_SPECIAL|COL_TRI)\s*\(([^()]*)\)', text):
        a = [s.strip() for s in body.split(',')]
        if command == 'COL_TRI_INIT':
            surface = surfaces[a[0]]
        else:
            triangles.append([*map(int, a[:3]), surface, int(a[3], 0) if len(a) == 4 else 0])
    return {'vertices': vertices, 'triangles': triangles, 'waterBoxes': []}


ACTOR = lambda n: f'actors/{n}/{n}_geo.glb'
# level-script behavior -> (behavior kind, model, collision key)
SCRIPT = {
    'bhvThwomp': ('thwomp', ACTOR('thwomp'), 'thwomp'),
    'bhvThwomp2': ('thwomp2', ACTOR('thwomp'), 'thwomp2'),
    'bhvPiranhaPlant': ('piranhaPlant', ACTOR('piranha_plant'), None),
    'bhvBulletBill': ('bulletBill', ACTOR('bullet_bill'), None),
    'bhvMips': ('mips', ACTOR('mips'), None),
    'bhvButterfly': ('butterfly', ACTOR('butterfly'), None),
    'bhvBird': ('bird', 'actors/bird/birds_geo.glb', None),
}
# macro preset -> (behavior kind, model, bhvParam)
MACRO = {
    'macro_swoop_1': ('swoop', ACTOR('swoop'), 0),
    'macro_swoop_2': ('swoop', ACTOR('swoop'), 1),
    'macro_scuttlebug': ('scuttlebug', ACTOR('scuttlebug'), 0),
    'macro_scuttlebug_spawner': ('scuttlebugSpawner', None, 0),
    'macro_spindrift': ('spindrift', ACTOR('spindrift'), 0),
    'macro_monty_mole': ('montyMole', ACTOR('monty_mole'), 1),
    'macro_monty_mole_no_rock': ('montyMole', ACTOR('monty_mole'), 0),
    'macro_monty_mole_hole': ('montyMoleHole', None, 0),
    'macro_snufit': ('snufit', ACTOR('snufit'), 0),
}
# (course file, level folder, area index)
TARGETS = [
    ('castle_grounds', 'castle_grounds', 1), ('castle_inside', 'castle_inside', 1), ('castle_inside.2', 'castle_inside', 2),
    ('castle_inside.3', 'castle_inside', 3), ('bob', 'bob', 1), ('wf', 'wf', 1), ('jrb', 'jrb', 1), ('ccm', 'ccm', 1),
    ('bbh', 'bbh', 1), ('hmc', 'hmc', 1),
]


def mips_paths(folder):
    """inside_castle_seg7_trajectory_mips[10] flattened with a (0, -1e6, 0) break between paths."""
    source = clean((folder / 'areas/3/trajectory.inc.c').read_text())
    order = re.search(r'inside_castle_seg7_trajectory_mips\[\]\s*=\s*\{(.*?)\};', source, re.S)
    names = [n.strip() for n in order[1].split(',') if n.strip()] if order else \
        [f'inside_castle_seg7_trajectory_mips_{i}' for i in range(10)]
    flat = []
    for name in names:
        body = re.search(r'\b' + name + r'\[\]\s*=\s*\{(.*?)\};', source, re.S)[1]
        if flat:
            flat.append([0, -1000000, 0])
        flat += [list(map(int, row[1:])) for row in args(body, 'TRAJECTORY_POS')]
    return flat


def build(level, area):
    folder = REF / 'levels' / level
    script = (folder / 'script.c').read_text()
    area_script = area_commands(script, area)
    out = []
    for kind in ('OBJECT', 'OBJECT_WITH_ACTS'):
        for a in args(area_script, kind):
            bhv = a[-1] if kind == 'OBJECT' else a[8]
            if bhv not in SCRIPT:
                continue
            behavior, model, collision_key = SCRIPT[bhv]
            param_expr = a[7]
            param = 1 if 'BIRD_BP_SPAWNER' in param_expr else (int(param_expr, 0) if re.fullmatch(r'-?(0x[0-9a-fA-F]+|\d+)', param_expr) else 0)
            obj = dict(model=model, behavior=behavior, pos=list(map(int, a[1:4])), yaw=int(a[5]), param=param, g1=True)
            if kind == 'OBJECT_WITH_ACTS':
                obj['acts'] = act_list(a[-1])
            if collision_key:
                obj['collisionKey'] = collision_key
            if behavior == 'mips':
                obj['track'] = mips_paths(folder)
            out.append(obj)
    macro = folder / f'areas/{area}/macro.inc.c'
    if macro.exists():
        for a in args(macro.read_text(), 'MACRO_OBJECT'):
            if a[0] in MACRO:
                behavior, model, param = MACRO[a[0]]
                obj = dict(behavior=behavior, pos=list(map(int, a[2:5])), yaw=int(a[1]), param=param, g1=True)
                if model:
                    obj['model'] = model
                out.append(obj)
    return out


def main():
    thwomp_script = REF / 'actors/thwomp/collision.inc.c'
    for course, level, area in TARGETS:
        path = OUT / f'{course}.json'
        data = json.loads(path.read_text())
        data['objects'] = [o for o in data['objects'] if not o.get('g1')]
        added = build(level, area)
        data['objects'] += added
        if any(o['behavior'] in ('thwomp', 'thwomp2') for o in added):
            # Dynamic surfaces are scaled by the object scale (SCALE 140 in bhvThwomp).
            data['extraCollision']['thwomp'] = collision(thwomp_script, 'thwomp_seg5_collision_0500B92C', 1.4)
            data['extraCollision']['thwomp2'] = collision(thwomp_script, 'thwomp_seg5_collision_0500B7D0', 1.4)
        path.write_text(json.dumps(data, separators=(',', ':')))
        counts = {}
        for o in added:
            counts[o['behavior']] = counts.get(o['behavior'], 0) + 1
        print(course, counts)


if __name__ == '__main__':
    main()
