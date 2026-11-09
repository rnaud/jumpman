#!/usr/bin/env python3
"""Export the Mario Kart 64 mode structure from the pinned n64decomp/mk64 sources (no ROM needed):

  public/mk64/modes.json       cups (gCupCourseOrder, menus.c), Grand Prix point rewards (gGPPointRewards, menu_items.c),
                               trophy rule, CC names, and per battle arena the item box spawns (course_data.c) and the
                               4-player start grid (spawn_players.c: D_80165210 = x, D_80165230 = z, height hint, rotation[1]).
  public/mk64/<arena>.json     the kart file of each battle arena (model, a ring "lap path" through the start grid that
                               the kart mode uses for Lakitu rescues, item boxes) so the arenas load like the race tracks.
  public/mk64/hud-minimap-<arena>.png   top-down footprint of the arena floor (64 x 96, like the race minimaps).

The walkable arenas (assets/courses/mk64_<arena>.json, from export-mk64.py) must exist first.
"""
import importlib.util
import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'reference/mk64'
ASSETS = ROOT / 'assets'
OUT = ROOT / 'public/mk64'
sys.path.insert(0, str(Path(__file__).resolve().parent))
import mk64_courses  # noqa: E402

ARENAS = ['big_donut', 'block_fort', 'double_deck', 'skyscraper']
NAMES = {'big_donut': 'Big Donut', 'block_fort': 'Block Fort', 'double_deck': 'Double Deck', 'skyscraper': 'Skyscraper'}
CONST = {  # menus.c COURSE_* constants -> course directory
    'COURSE_LUIGI_RACEWAY': 'luigi_raceway', 'COURSE_MOO_MOO_FARM': 'moo_moo_farm', 'COURSE_KOOPA_BEACH': 'koopa_troopa_beach',
    'COURSE_KALAMARI_DESERT': 'kalimari_desert', 'COURSE_TOADS_TURNPIKE': 'toads_turnpike', 'COURSE_FRAPPE_SNOWLAND': 'frappe_snowland',
    'COURSE_CHOCO_MOUNTAIN': 'choco_mountain', 'COURSE_MARIO_RACEWAY': 'mario_raceway', 'COURSE_WARIO_STADIUM': 'wario_stadium',
    'COURSE_SHERBET_LAND': 'sherbet_land', 'COURSE_ROYAL_RACEWAY': 'royal_raceway', 'COURSE_BOWSER_CASTLE': 'bowsers_castle',
    'COURSE_DK_JUNGLE': 'dks_jungle_parkway', 'COURSE_YOSHI_VALLEY': 'yoshi_valley', 'COURSE_BANSHEE_BOARDWALK': 'banshee_boardwalk',
    'COURSE_RAINBOW_ROAD': 'rainbow_road', 'COURSE_BIG_DONUT': 'big_donut', 'COURSE_BLOCK_FORT': 'block_fort',
    'COURSE_DOUBLE_DECK': 'double_deck', 'COURSE_SKYSCRAPER': 'skyscraper',
}
CUP_NAMES = ['Mushroom', 'Flower', 'Star', 'Special']


def cups():
    text = (SOURCE / 'src/menus.c').read_text()
    body = re.search(r'gCupCourseOrder\[5\]\[4\]\s*=\s*\{(.*?)\n\};', text, re.S).group(1)
    rows = [re.findall(r'COURSE_\w+', row) for row in re.findall(r'\{([^{}]*)\}', body)]
    assert len(rows) == 5 and all(len(r) == 4 for r in rows), rows
    return [[CONST[c] for c in row] for row in rows]


def point_rewards():
    text = (SOURCE / 'src/menu_items.c').read_text()
    return [int(v) for v in re.search(r'gGPPointRewards\[\]\s*=\s*\{([^}]*)\}', text).group(1).split(',') if v.strip()]


def battle_starts():
    """course -> {'height': n, 'x': [4], 'z': [4], 'rotation': [4]} from the 3P/4P split-screen branch of spawn_players.c."""
    text = (SOURCE / 'src/spawn_players.c').read_text()
    # rotation[1] of players 1-4 (spawn_players_4p_battle): Big Donut circles the ring, the others face the centre.
    four = re.search(r'void spawn_players_4p_battle\(.*?\n\}', text, re.S).group(0)
    rotations = {}
    for branch, block in (('donut', re.search(r'COURSE_BIG_DONUT\)\s*\{(.*?)\}\s*else', four, re.S).group(1)),
                          ('other', re.search(r'else\s*\{(.*?)\n    \}', four, re.S).group(1))):
        rotations[branch] = [int(float(v)) for v in re.findall(r'spawn_player\(gPlayer\w+, \d, arg0\[\d\], arg1\[\d\], arg2, (-?[\d.]+)f', block)]
        assert len(rotations[branch]) == 4, (branch, rotations[branch])
    out = {}
    for course in ('BLOCK_FORT', 'SKYSCRAPER', 'DOUBLE_DECK', 'BIG_DONUT'):
        cases = re.findall(r'case COURSE_%s:(.*?)(?=case COURSE_|\Z)' % course, text, re.S)
        body = next(c for c in cases if 'spawn_players_4p_battle' in c)
        # Double Deck's label is the literal `case 3:` (the value of SCREEN_MODE_3P_4P_SPLITSCREEN)
        body = body[re.search(r'case (SCREEN_MODE_3P_4P_SPLITSCREEN|3):', body).start():]
        body = body[:body.index('spawn_players_3p_battle')]
        height = int(re.search(r'temp = (0x[0-9A-Fa-f]+|\d+);', body).group(1), 0)
        x, z = [None] * 4, [None] * 4
        for arr, idx, value in re.findall(r'D_8016(5210|5230)\[(\d)\] = (-?[\d.]+)f;', body):
            (x if arr == '5210' else z)[int(idx)] = float(value)
        assert None not in x and None not in z, (course, x, z)
        out[course.lower()] = {'height': height, 'x': x, 'z': z, 'rotation': rotations['donut' if course == 'BIG_DONUT' else 'other']}
    return out


def floor_triangles(arena):
    data = json.loads((ASSETS / 'courses' / f'mk64_{arena}.json').read_text())
    vs = data['vertices']
    tris = []
    for a, b, c, *_ in data['triangles']:
        p = [vs[a], vs[b], vs[c]]
        ux, uz = p[1][0] - p[0][0], p[1][2] - p[0][2]
        vx, vz = p[2][0] - p[0][0], p[2][2] - p[0][2]
        if abs(ux * vz - uz * vx) > 1e-6:
            tris.append(p)
    return data, tris


def floor_y(tris, x, z, near):
    """Height of the floor triangle under (x, z) closest to `near` (so double-deck karts find their deck)."""
    best = None
    for p in tris:
        d = (p[1][2] - p[2][2]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][2] - p[2][2])
        if abs(d) < 1e-9:
            continue
        a = ((p[1][2] - p[2][2]) * (x - p[2][0]) + (p[2][0] - p[1][0]) * (z - p[2][2])) / d
        b = ((p[2][2] - p[0][2]) * (x - p[2][0]) + (p[0][0] - p[2][0]) * (z - p[2][2])) / d
        c = 1 - a - b
        if min(a, b, c) < -1e-6:
            continue
        y = a * p[0][1] + b * p[1][1] + c * p[2][1]
        if best is None or abs(y - near) < abs(best - near):
            best = y
    return best


def footprint_png(path, tris):
    """64 x 96 white-on-transparent top-down footprint of every floor triangle (z down the picture, like the race minimaps)."""
    spec = importlib.util.spec_from_file_location('export_mk64', Path(__file__).resolve().parent / 'export-mk64.py')
    export = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(export)
    xs = [q[0] for p in tris for q in p]
    zs = [q[2] for p in tris for q in p]
    scale = min(56 / max(max(xs) - min(xs), 1), 88 / max(max(zs) - min(zs), 1))
    cx, cz = (max(xs) + min(xs)) / 2, (max(zs) + min(zs)) / 2
    pixels = bytearray(64 * 96 * 4)
    for p in tris:
        pts = [(31.5 + (q[0] - cx) * scale, 47.5 + (q[2] - cz) * scale) for q in p]
        x0, x1 = int(min(q[0] for q in pts)), int(max(q[0] for q in pts)) + 1
        y0, y1 = int(min(q[1] for q in pts)), int(max(q[1] for q in pts)) + 1
        for py in range(max(y0, 0), min(y1, 95) + 1):
            for px in range(max(x0, 0), min(x1, 63) + 1):
                a, b, c = pts
                d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
                if abs(d) < 1e-9:
                    continue
                u = ((b[1] - c[1]) * (px + .5 - c[0]) + (c[0] - b[0]) * (py + .5 - c[1])) / d
                v = ((c[1] - a[1]) * (px + .5 - c[0]) + (a[0] - c[0]) * (py + .5 - c[1])) / d
                if min(u, v, 1 - u - v) >= -0.02:
                    i = (py * 64 + px) * 4
                    pixels[i:i + 4] = b'\xff\xff\xff\xff'
    export.png(path, 64, 96, pixels)
    return dict(cx=cx, cz=cz, scale=scale)


def main():
    cup_rows = cups()
    modes = {
        'source': 'n64decomp/mk64',
        'cups': [{'name': CUP_NAMES[i], 'tracks': cup_rows[i]} for i in range(4)],
        'battle': cup_rows[4],
        'gpPoints': point_rewards(),
        'ccNames': ['50cc', '100cc', '150cc', 'Extra'],
        'trophies': ['none', 'bronze', 'silver', 'gold'],
        'arenas': {},
    }
    starts = battle_starts()
    OUT.mkdir(parents=True, exist_ok=True)
    for arena in ARENAS:
        data, tris = floor_triangles(arena)
        scale = data['mk64']['unit']
        s = starts[arena]
        grid = []
        for i in range(4):
            x, z = s['x'][i], s['z'][i]
            y = floor_y(tris, x * scale, z * scale, (s['height'] + 50) * scale)
            if y is None:
                y = (s['height']) * scale
            grid.append({'pos': [x, round(y / scale, 2), z], 'rotation': int(s['rotation'][i])})
        radius = sum(math.hypot(g['pos'][0], g['pos'][2]) for g in grid) / 4
        ring = []
        for k in range(32):
            ang = 2 * math.pi * k / 32
            x, z = radius * math.sin(ang), radius * math.cos(ang)
            y = floor_y(tris, x * scale, z * scale, grid[0]['pos'][1] * scale)
            ring.append([round(x), round((y if y is not None else grid[0]['pos'][1] * scale) / scale), round(z)])
        boxes = [list(b) for b in mk64_courses.Course(SOURCE, arena).item_boxes()]
        view = footprint_png(OUT / f'hud-minimap-{arena.replace("_", "-")}.png', tris)
        modes['arenas'][arena] = {'name': NAMES[arena], 'grid': grid, 'boxes': boxes, 'height': s['height'], 'minimap': view}
        kart = {'source': 'n64decomp/mk64', 'romSha1': '579c48e211ae952530ffc8738709f078d5dd215e',
                'model': f'levels/mk64_{arena}/terrain.glb', 'path': ring, 'scale': 8, 'itemBoxes': boxes}
        (OUT / (arena.replace('_', '-') + '.json')).write_text(json.dumps(kart, separators=(',', ':')))
    (OUT / 'modes.json').write_text(json.dumps(modes, separators=(',', ':')) + '\n')
    print('modes.json: %d cups, %d arenas, points %s' % (len(modes['cups']), len(modes['arenas']), modes['gpPoints']))


if __name__ == '__main__':
    main()
