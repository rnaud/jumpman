"""SM64 round 2, part B placements: castle Toads, blue coins and switches, exclamation boxes with coin /
1-up / shell contents, recovery hearts, 1-up variants, the THI Koopa the Quick race and the PSS slide
timer. Adds the placements that have a native behavior port (native/sim/src/behaviors/part_b) to the
already exported assets/courses/*.json.

Run AFTER scripts/export-courses.py and export-sm64-g1-actors.py / export-sm64-solids.py (those rewrite the
course files and do not know these objects); it is idempotent: previously added entries carry
`"pb": true` and are replaced.

    python3 scripts/export-sm64-castle.py

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


EXCLAMATION = 'actors/exclamation_box/exclamation_box_geo.glb'
ONE_UP = 'actors/mushroom_1up/mushroom_1up_geo.glb'
BLUE_COIN = 'actors/coin/blue_coin_geo.glb'
# macro preset -> (behavior kind, model, bhvParam). EXCLAMATION_BOX_BP_*: 3 shell, 4 / 5 / 6 coins, 7 / 9 1-up.
MACRO = {
    'macro_blue_coin_switch': ('blueCoinSwitch', 'actors/blue_coin_switch/blue_coin_switch_geo.glb', 0),
    'macro_hidden_blue_coin': ('hiddenBlueCoin', BLUE_COIN, 0),
    'macro_moving_blue_coin': ('movingBlueCoin', BLUE_COIN, 0),
    'macro_recovery_heart': ('recoveryHeart', 'actors/heart/heart_geo.glb', 0),
    'macro_box_koopa_shell': ('exclamationBox', EXCLAMATION, 3),
    'macro_box_one_coin': ('exclamationBox', EXCLAMATION, 4),
    'macro_box_three_coins': ('exclamationBox', EXCLAMATION, 5),
    'macro_box_ten_coins': ('exclamationBox', EXCLAMATION, 6),
    'macro_box_1up': ('exclamationBox', EXCLAMATION, 7),
    'macro_box_1up_running_away': ('exclamationBox', EXCLAMATION, 9),
    'macro_1up': ('oneUp', ONE_UP, 0),
    'macro_1up_sliding': ('oneUpSliding', ONE_UP, 0),
    'macro_hidden_1up': ('hidden1Up', ONE_UP, 0),
    'macro_hidden_1up_trigger': ('hidden1UpTrigger', None, 0),
}
SWITCH_COLLISION = 'blue_coin_switch_seg8_collision_08000E98'


def trajectory(folder, name):
    source = clean((folder / 'areas/1/trajectory.inc.c').read_text())
    body = re.search(r'\b' + name + r'\[\]\s*=\s*\{(.*?)\};', source, re.S)[1]
    return [list(map(int, row[1:])) for row in args(body, 'TRAJECTORY_POS')]


def script_objects(level, area, folder):
    """Level-script placements: the castle Toads, the THI race and the PSS timer marker."""
    script = (folder / 'script.c').read_text()
    if not re.search(r'\bAREA\(\s*' + str(area) + r'\s*,', clean(script)):
        return []
    area_script = area_commands(script, area)
    out = []
    for kind in ('OBJECT', 'OBJECT_WITH_ACTS'):
        for a in args(area_script, kind):
            bhv = a[-1] if kind == 'OBJECT' else a[8]
            acts = act_list(a[-1]) if kind == 'OBJECT_WITH_ACTS' else None
            pos, yaw = list(map(int, a[1:4])), int(a[5])
            if bhv == 'bhvToadMessage':
                dialog = int(re.search(r'DIALOG_(\d+)', a[7])[1])
                out.append(dict(model='actors/toad/toad_geo.glb', behavior='toadMessage', pos=pos, yaw=yaw, param=dialog))
            elif level == 'thi' and bhv == 'bhvKoopa' and 'KOOPA_BP_KOOPA_THE_QUICK_THI' in a[7]:
                # Koopa the Quick, THI race (koopa.inc.c, sKoopaTheQuickProperties[1]): the route is `thi_seg7_trajectory_koopa`.
                out.append(dict(model='actors/koopa/koopa_with_shell_geo.glb', behavior='koopaTheQuick', pos=pos, yaw=yaw,
                                param=1, acts=acts, track=trajectory(folder, 'thi_seg7_trajectory_koopa')))
            elif level == 'thi' and bhv == 'bhvKoopaRaceEndpoint':
                out.append(dict(model='actors/koopa_flag/koopa_flag_geo.glb', behavior='koopaFlag', pos=pos, yaw=yaw, param=1, acts=acts))
    if level == 'castle_grounds' and area == 1:
        # bhvMoatGrills (moat_grill.inc.c): the grills over the Vanish Cap hole, gone once the moat is drained.
        out.append(dict(model='levels/castle_grounds/castle_grounds_geo_00070C.glb', behavior='moatGrill', pos=[0, 0, 0], yaw=0, param=0))
    if level == 'castle_inside' and area == 3:
        for a in args(area_script, 'OBJECT'):
            if a[-1] == 'bhvWaterLevelPillar':
                out.append(dict(model='levels/castle_inside/castle_geo_001940.glb', behavior='waterLevelPillar',
                                pos=list(map(int, a[1:4])), yaw=int(a[5]), param=0))
    if level == 'pss' and area == 1:
        # pss_begin_slide / pss_end_slide are driven by SURFACE_TIMER_START / END floors (interaction.c).
        out.append(dict(behavior='pssTimer', pos=[0, 0, 0], yaw=0, param=0))
    return out


def macro_objects(level, area, folder):
    macro = folder / f'areas/{area}/macro.inc.c'
    out = []
    if not macro.exists():
        return out
    text = macro.read_text()
    for command in ('MACRO_OBJECT', 'MACRO_OBJECT_WITH_BHV_PARAM'):
        for a in args(text, command):
            if a[0] not in MACRO:
                continue
            behavior, model, param = MACRO[a[0]]
            if command == 'MACRO_OBJECT_WITH_BHV_PARAM':
                param = int(a[5], 0)
            obj = dict(behavior=behavior, pos=list(map(int, a[2:5])), yaw=int(a[1]), param=param)
            if model:
                obj['model'] = model
            out.append(obj)
    return out


def targets():
    """(course file stem, level folder, area index) for every exported SM64 course area."""
    for path in sorted(OUT.glob('*.json')):
        stem = path.stem
        if stem.endswith('.ext') or stem.startswith(('bk_', 'oot_', 'mk64_', 'z_')):
            continue
        m = re.fullmatch(r'([a-z_0-9]+?)(?:\.(\d+))?', stem)
        level, area = m[1], int(m[2] or 1)
        if (REF / 'levels' / level / 'script.c').exists():
            yield stem, level, area


def main():
    box_collision = collision(REF / 'actors/exclamation_box_outline/collision.inc.c', 'exclamation_box_outline_seg8_collision_08025F78')
    box_collision['vertices'] = [[2 * axis for axis in v] for v in box_collision['vertices']]
    summary = {}
    for stem, level, area in targets():
        path = OUT / f'{stem}.json'
        data = json.loads(path.read_text())
        if 'objects' not in data:
            continue
        data['objects'] = [o for o in data['objects'] if not o.get('pb')]
        folder = REF / 'levels' / level
        added = script_objects(level, area, folder) + macro_objects(level, area, folder)
        for o in added:
            o['pb'] = True
            if o.get('acts') is None:
                o.pop('acts', None)
        data['objects'] += added
        kinds = {o['behavior'] for o in added}
        if 'blueCoinSwitch' in kinds:
            data['extraCollision']['g2/' + SWITCH_COLLISION] = collision(REF / 'actors/blue_coin_switch/collision.inc.c', SWITCH_COLLISION)
        if 'moatGrill' in kinds:
            data['extraCollision']['g2/castle_grounds_seg7_collision_moat_grills'] = collision(
                REF / 'levels/castle_grounds/areas/1/7/collision.inc.c', 'castle_grounds_seg7_collision_moat_grills')
        if 'waterLevelPillar' in kinds:
            data['extraCollision']['g2/inside_castle_seg7_collision_water_level_pillar'] = collision(
                REF / 'levels/castle_inside/water_level_pillar/collision.inc.c', 'inside_castle_seg7_collision_water_level_pillar')
        if 'exclamationBox' in kinds and 'starBox' not in data['extraCollision']:
            data['extraCollision']['starBox'] = box_collision
        path.write_text(json.dumps(data, separators=(',', ':')))
        counts = {}
        for o in added:
            counts[o['behavior']] = counts.get(o['behavior'], 0) + 1
        if counts:
            summary[stem] = counts
            print(stem, counts)


if __name__ == '__main__':
    main()
