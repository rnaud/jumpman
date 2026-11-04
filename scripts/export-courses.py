"""Export local decomp collision/placements into the gitignored asset library.
No ROM extraction. Run after copying the GLB library and reference/sm64.
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
    text = re.sub(r'BPARAM[1-4]\([^()]*\)', '0', text)
    return [tuple(s.strip() for s in m.split(',')) for m in re.findall(r'\b' + command + r'\s*\(([^()]*)\)', clean(text))]

surfaces = {name: int(value, 0) for name, value in re.findall(r'#define\s+(SURFACE_\w+)\s+(0x[0-9A-Fa-f]+)', (REF / 'include/surface_terrains.h').read_text())}

def collision(path):
    text = clean(path.read_text())
    vertices = [list(map(int, a)) for a in args(text, 'COL_VERTEX')]
    triangles = []
    surface = 0
    for command, body in re.findall(r'\b(COL_TRI_INIT|COL_TRI_SPECIAL|COL_TRI)\s*\(([^()]*)\)', text):
        a = [s.strip() for s in body.split(',')]
        if command == 'COL_TRI_INIT':
            surface = surfaces[a[0]]
        else:
            indices = list(map(int, a[:3]))
            assert all(0 <= i < len(vertices) for i in indices), path
            triangles.append([*indices, surface, int(a[3], 0) if len(a) == 4 else 0])
    assert vertices and triangles, path
    return {'vertices': vertices, 'triangles': triangles}

OUT.mkdir(exist_ok=True)
for level in ('bob', 'castle_grounds', 'wf', 'pss'):
    folder = REF / 'levels' / level
    script = (folder / 'script.c').read_text()
    geo = args(script, 'AREA')[0][1]
    spawn = args(script, 'MARIO_POS')[0]
    terrain_path = folder / 'areas/1/collision.inc.c'
    data = collision(terrain_path)
    data.update(model=f'levels/{level}/{geo}.glb', spawn={'pos': list(map(int, spawn[2:5])), 'faceYaw': round(int(spawn[1]) * 65536 / 360)}, objects=[], coins=[])
    models = dict(args(script, 'LOAD_MODEL_FROM_GEO'))
    frozen = {'bhvWFRotatingWoodenPlatform': 'rotating_wooden_platform', 'bhvWFSlidingPlatform': 'sliding_platform', 'bhvRotatingPlatform': 'rotating_platform', 'bhvSmallBomp': 'small_bomp', 'bhvLargeBomp': 'large_bomp', 'bhvWFBreakableWallRight': 'breakable_wall_right', 'bhvWFBreakableWallLeft': 'breakable_wall_left'} if level == 'wf' else {}
    # Static geometry in WF is partly placed by scripts, partly by collision presets.
    for a in args(script, 'OBJECT'):
        if a[-1] == 'bhvStaticObject' and a[0] in models:
            data['objects'].append({'model': f'levels/{level}/{models[a[0]]}.glb', 'pos': list(map(int, a[1:4])), 'yaw': int(a[5])})
    for a in args(script, 'OBJECT'):
        if a[-1] in frozen and a[0] in models:
            data['objects'].append({'model': f'levels/{level}/{models[a[0]]}.glb', 'pos': list(map(int, a[1:4])), 'yaw': int(a[5]), 'collision': frozen[a[-1]] + '/collision.inc.c'})
    for a in args(terrain_path.read_text(), 'SPECIAL_OBJECT(?:_WITH_YAW)?'):
        preset = a[0]
        if preset == 'special_bubble_tree':
            data['objects'].append({'model': 'actors/tree/bubbly_tree_geo.glb', 'pos': list(map(int, a[1:4])), 'yaw': 0})
        elif preset.startswith('special_level_geo_'):
            model = models.get('MODEL_LEVEL_GEOMETRY_' + preset.removeprefix('special_level_geo_'))
            if model:
                data['objects'].append({'model': f'levels/{level}/{model}.glb', 'pos': list(map(int, a[1:4])), 'yaw': int(a[4]) * 360 / 256})
    macro = folder / 'areas/1/macro.inc.c'
    if macro.exists():
        for a in args(macro.read_text(), 'MACRO_OBJECT'):
            if a[0] in ('macro_yellow_coin', 'macro_yellow_coin_2'):
                data['coins'].append(list(map(int, a[2:5])))
    # Keep native collision for additional static terrain objects, where supplied.
    extras = {'bob': ['seesaw_platform/collision.inc.c'], 'wf': ['areas/1/10/collision.inc.c', 'areas/1/11/collision.inc.c'], 'castle_grounds': ['areas/1/7/collision.inc.c', 'areas/1/8/collision.inc.c']}.get(level, [])
    extras += sorted(set(p + '/collision.inc.c' for p in frozen.values()))
    data['extraCollision'] = {p: collision(folder / p) for p in extras}
    (OUT / f'{level}.json').write_text(json.dumps(data, separators=(',', ':')))
    print(f'{level}: {len(data["triangles"])} terrain triangles, {len(data["objects"])} static objects')
