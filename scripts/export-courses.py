"""Export local decomp collision/placements into the gitignored asset library.
No ROM extraction. Run after copying the GLB library and reference/sm64.
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
OUT = ROOT / 'assets/courses'

def act_list(expression):
    return [int(value) for value in re.findall(r'ACT_(\d+)', expression)]

def clean(text):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)

def args(text, command):
    text = re.sub(r'BPARAM([1-4])\(([^()]*)\)', lambda m: m[2] if m[1] == '2' else ('first_' + m[2] if m[1] == '1' else '0'), text)
    return [tuple(s.strip() for s in m.split(',')) for m in re.findall(r'\b' + command + r'\s*\(([^()]*)\)', clean(text))]

surfaces = {name: int(value, 0) for name, value in re.findall(r'#define\s+(SURFACE_\w+)\s+(0x[0-9A-Fa-f]+)', (REF / 'include/surface_terrains.h').read_text())}

params = dict(re.findall(r'#define\s+(\w+)\s+(-?(?:0x[0-9a-fA-F]+|\d+))\b', (REF / 'include/object_constants.h').read_text()))
BBH_MOVING = {
    'bhvMeshElevator': 'mesh_elevator',
    'bhvMerryGoRound': 'merry_go_round',
    'bhvHauntedBookshelf': 'moving_bookshelf',
    'bhvBBHTiltingTrapPlatform': 'tilting_trap_platform',
}

def platform_param(expression, behavior):
    fields = [field.strip() for field in expression.split('|')]
    if behavior == 'bhvRotatingPlatform':
        field = next((v[6:] for v in fields if v.startswith('first_')), '0')
        value = int(params.get(field, field), 0) & 255
        return value if value < 128 else value - 256
    field = next((v for v in fields if not v.startswith('first_')), '0')
    return int(params.get(field, field), 0)

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
    water = [[int(v, 0) for v in box] for box in args(text, 'COL_WATER_BOX')]
    return {'vertices': vertices, 'triangles': triangles, 'waterBoxes': water}

OUT.mkdir(exist_ok=True)
for level in ('bob', 'castle_grounds', 'castle_inside', 'wf', 'bbh', 'ccm', 'pss'):
    folder = REF / 'levels' / level
    script = (folder / 'script.c').read_text()
    geo = args(script, 'AREA')[0][1]
    spawn = next(iter(args(script, 'MARIO_POS')), ('1', '0', '0', '0', '0'))
    terrain_path = folder / 'areas/1/collision.inc.c'
    data = collision(terrain_path)
    data.update(model=f'levels/{level}/{geo}.glb', spawn={'pos': list(map(int, spawn[2:5])), 'faceYaw': round(int(spawn[1]) * 65536 / 360)}, objects=[], coins=[], goombas=[])
    models = dict(args(script, 'LOAD_MODEL_FROM_GEO'))
    # Front door pairs in area 1 share warp-node IDs 00/01 across both maps.
    if level in ('castle_grounds', 'castle_inside'):
        destination = 'LEVEL_CASTLE' if level == 'castle_grounds' else 'LEVEL_CASTLE_GROUNDS'
        links = {a[0]: a[3] for a in args(script, 'WARP_NODE') if a[1] == destination and a[2] == '1'}
        doors = []
        def add_door(pos, yaw, node):
            if node.startswith('WARP_NODE_'):
                key = node
            else:
                key = f'WARP_NODE_{int(node, 0):02X}'
            if key not in ('WARP_NODE_00', 'WARP_NODE_01') or key not in links:
                return
            doors.append(dict(pos=pos, yaw=yaw, node=key, destNode=links[key],
                              destination='castle_inside' if level == 'castle_grounds' else 'castle_grounds'))
            data['objects'].append(dict(model='actors/door/castle_door_geo.glb', pos=pos, yaw=yaw))
        for a in args(terrain_path.read_text(), 'SPECIAL_OBJECT_WITH_YAW_AND_PARAM'):
            if 'door' in a[0] and 'warp' in a[0]:
                add_door(list(map(int, a[1:4])), int(a[4], 0) * 360 / 256, a[5])
        for a in args(script, 'OBJECT'):
            if a[-1] == 'bhvDoorWarp' and 'CASTLE_DOOR' in a[0]:
                add_door(list(map(int, a[1:4])), int(a[5]), a[7])
        data['doors'] = doors
        if len({d['node'] for d in doors}) != 2:
            raise ValueError(f'Could not resolve both main entrance warp nodes in {level}')
    frozen = {'bhvWFRotatingWoodenPlatform': 'rotating_wooden_platform', 'bhvWFSlidingPlatform': 'sliding_platform', 'bhvRotatingPlatform': 'rotating_platform', 'bhvSmallBomp': 'small_bomp', 'bhvLargeBomp': 'large_bomp', 'bhvWFBreakableWallRight': 'breakable_wall_right', 'bhvWFBreakableWallLeft': 'breakable_wall_left'} if level == 'wf' else {}
    # Act-dependent script objects. Only King Bob-omb is implemented so far.
    for a in args(script, 'OBJECT_WITH_ACTS'):
        if a[8] == 'bhvKingBobomb':
            data['objects'].append({
                'model': 'actors/king_bobomb/king_bobomb_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'kingBobomb',
                'acts': act_list(a[-1]),
            })
        elif a[8] in ('bhvBalconyBigBoo', 'bhvGhostHuntBigBoo'):
            data['objects'].append({
                'model': 'actors/boo/boo_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'bigBoo',
                'acts': act_list(a[-1]),
            })
        elif a[8] in ('bhvBoo', 'bhvGhostHuntBoo'):
            data['objects'].append({
                'model': 'actors/boo/boo_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'boo',
                'acts': act_list(a[-1]),
            })
        elif a[8] == 'bhvKoopa' and 'KOOPA_THE_QUICK_BOB' in a[7]:
            data['objects'].append({
                'model': 'actors/koopa/koopa_with_shell_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'koopaTheQuick',
                'acts': act_list(a[-1]),
            })
        elif a[8] == 'bhvKoopa' and 'KOOPA_BP_NORMAL' in a[7]:
            data['objects'].append({
                'model': 'actors/koopa/koopa_with_shell_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'koopa',
                'acts': act_list(a[-1]),
            })
        elif a[8] in ('bhvBobombBuddy', 'bhvBobombBuddyOpensCannon'):
            data['objects'].append({
                'model': 'actors/bobomb/bobomb_buddy_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'bobombBuddy',
                'param': 1 if a[8] == 'bhvBobombBuddyOpensCannon' else 0,
                'acts': act_list(a[-1]),
            })
        elif a[8] == 'bhvWaterBombCannon':
            data['objects'].append({
                'model': 'actors/cannon_base/cannon_base_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'waterBombCannon',
                'acts': act_list(a[-1]),
            })
        elif a[8] == 'bhvKoopaRaceEndpoint':
            data['objects'].append({
                'model': 'actors/koopa_flag/koopa_flag_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'koopaFlag',
                'acts': act_list(a[-1]),
            })
    # Static geometry in WF is partly placed by scripts, partly by collision presets.
    script_behaviors = {'bhvChainChompGate': 'chainChompGate'}
    actor_behaviors = {
        'bhvFloorSwitchGrills': ('actors/purple_switch/purple_switch_geo.glb', 'purpleSwitch'),
        'bhvPitBowlingBall': ('actors/bowling_ball/bowling_ball_geo.glb', 'pitBowlingBall'),
    }
    for a in args(script, 'OBJECT'):
        if a[-1] in script_behaviors and a[0] in models:
            data['objects'].append({
                'model': f'levels/{level}/{models[a[0]]}.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': script_behaviors[a[-1]],
            })
        elif a[-1] == 'bhvOpenableGrill' and 'MODEL_BOB_BARS_GRILLS' in models:
            data['objects'].append({
                'model': f'levels/{level}/{models["MODEL_BOB_BARS_GRILLS"]}.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'openableGrill',
            })
        elif a[-1] in BBH_MOVING and a[0] in models:
            data['objects'].append({
                'model': f'levels/bbh/{models[a[0]]}.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'collision': BBH_MOVING[a[-1]] + '/collision.inc.c',
            })
        elif a[-1] == 'bhvFlame':
            data['objects'].append({
                'model': 'actors/flame/red_flame_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'flame',
            })
        elif a[-1] in actor_behaviors:
            model, behavior = actor_behaviors[a[-1]]
            data['objects'].append({
                'model': model,
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': behavior,
            })
        elif a[-1] == 'bhvStaticObject' and a[0] in models:
            data['objects'].append({'model': f'levels/{level}/{models[a[0]]}.glb', 'pos': list(map(int, a[1:4])), 'yaw': int(a[5])})
    for a in args(script, 'OBJECT'):
        if a[-1] in frozen and a[0] in models:
            data['objects'].append({'model': f'levels/{level}/{models[a[0]]}.glb', 'pos': list(map(int, a[1:4])), 'yaw': int(a[5]), 'param': platform_param(a[7], a[-1]), 'collision': frozen[a[-1]] + '/collision.inc.c'})
    for a in args(terrain_path.read_text(), 'SPECIAL_OBJECT(?:_WITH_YAW)?'):
        preset = a[0]
        if preset == 'special_bubble_tree':
            data['objects'].append({'model': 'actors/tree/bubbly_tree_geo.glb', 'pos': list(map(int, a[1:4])), 'yaw': 0, 'behavior': 'tree'})
        elif preset.startswith('special_level_geo_'):
            model = models.get('MODEL_LEVEL_GEOMETRY_' + preset.removeprefix('special_level_geo_'))
            if model:
                data['objects'].append({'model': f'levels/{level}/{model}.glb', 'pos': list(map(int, a[1:4])), 'yaw': (int(a[4], 0) if len(a) > 4 else 0) * 360 / 256})
    macro = folder / 'areas/1/macro.inc.c'
    # Interactive macro objects. The preset's parameter travels in the low byte
    # (oBhvParams2ndByte) exactly as in macro_special_objects.c.
    macro_behaviors = {
        'macro_bobomb': ('actors/bobomb/black_bobomb_geo.glb', 'bobomb', 0),
        'macro_bobomb_stationary': ('actors/bobomb/black_bobomb_geo.glb', 'bobomb', 1),
        'macro_breakable_box_no_coins': ('actors/breakable_box/breakable_box_geo.glb', 'breakableBox', 0),
        'macro_breakable_box_three_coins': ('actors/breakable_box/breakable_box_geo.glb', 'breakableBox', 1),
        'macro_breakable_box_five_coins': ('actors/breakable_box/breakable_box_geo.glb', 'breakableBox', 2),
        'macro_breakable_box_small': ('actors/breakable_box/breakable_box_small_geo.glb', 'breakableBoxSmall', 0),
        'macro_chain_chomp': ('actors/chain_chomp/chain_chomp_geo.glb', 'chainChomp', 0),
        'macro_wooden_post_1': ('actors/poundable_pole/wooden_post_geo.glb', 'woodenPost', 0),
    }
    if macro.exists():
        for a in args(macro.read_text(), 'MACRO_OBJECT'):
            if a[0] in macro_behaviors:
                model, behavior, param = macro_behaviors[a[0]]
                data['objects'].append(dict(
                    model=model,
                    pos=list(map(int, a[2:5])),
                    yaw=int(a[1]),
                    behavior=behavior,
                    param=param,
                ))
            elif a[0] in ('macro_goomba', 'macro_goomba_triplet_spawner'):
                data['goombas'].append(dict(pos=list(map(int, a[2:5])), triplet=a[0].endswith('spawner')))
            elif a[0] in ('macro_yellow_coin', 'macro_yellow_coin_2'):
                data['coins'].append(list(map(int, a[2:5])))
    # Keep native collision for additional static terrain objects, where supplied.
    extras = {'bob': ['seesaw_platform/collision.inc.c'], 'wf': ['areas/1/10/collision.inc.c', 'areas/1/11/collision.inc.c'], 'castle_grounds': ['areas/1/7/collision.inc.c', 'areas/1/8/collision.inc.c']}.get(level, [])
    extras += sorted(set(p + '/collision.inc.c' for p in frozen.values()))
    if level == 'bbh':
        extras += [d + '/collision.inc.c' for d in BBH_MOVING.values()]
    data['extraCollision'] = {p: collision(folder / p) for p in extras}
    (OUT / f'{level}.json').write_text(json.dumps(data, separators=(',', ':')))
    print(f'{level}: {len(data["triangles"])} terrain triangles, {len(data["objects"])} static objects')
