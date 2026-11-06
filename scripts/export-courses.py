"""Export local decomp collision/placements into the gitignored asset library.
No ROM extraction. Run after copying the GLB library and reference/sm64.
"""
import json
import math
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
OUT = ROOT / 'assets/courses'

def glb_has_meshes(path):
    """Skip geo-only decor GLBs that contain no drawable mesh."""
    try:
        data = path.read_bytes()
        if data[:4] != b'glTF':
            return False
        size = struct.unpack_from('<I', data, 12)[0]
        doc = json.loads(data[20:20 + size])
        return bool(doc.get('meshes'))
    except (OSError, ValueError, struct.error):
        return True


def act_list(expression):
    return [int(value) for value in re.findall(r'ACT_(\d+)', expression)]

def clean(text):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)

def args(text, command):
    text = re.sub(r'BPARAM([1-4])\(([^()]*)\)', lambda m: m[2] if m[1] == '2' else ('first_' + m[2] if m[1] == '1' else '0'), text)
    return [tuple(s.strip() for s in m.split(',')) for m in re.findall(r'\b' + command + r'\s*\(([^()]*)\)', clean(text))]

def area_commands(script, index):
    # Level scripts keep placements either inline or in linked local scripts.
    source = clean(script)
    area = re.search(r'\bAREA\(\s*' + str(index) + r'\s*,[^)]*\),(.*?)END_AREA\(', source, re.S)
    blocks = dict(re.findall(r'LevelScript\s+(\w+)\[\]\s*=\s*\{(.*?)\};', source, re.S))

    def expand(body):
        return re.sub(r'JUMP_LINK\(\s*(\w+)\s*\)', lambda match: expand(blocks[match[1]]), body)

    return expand(area[1])

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

def collision(path, symbol=None):
    text = clean(path.read_text())
    if symbol:
        text = re.search(r'\b' + symbol + r'\[\]\s*=\s*\{(.*?)\};', text, re.S)[1]
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

def castle_paintings(folder, area):
    # geo_painting_draw indexes sInsideCastlePaintings by the first PAINTING_ID argument.
    painting = clean((folder / 'painting.inc.c').read_text())
    geo = clean((folder / f'areas/{area}/geo.inc.c').read_text())
    visible = {int(i) for i in re.findall(r'PAINTING_ID\(\s*(\d+),\s*1\)', geo)}
    arrays = dict(re.findall(r'\b(\w+)\[\]\s*=\s*\{(.*?)\};', painting, re.S))
    images = dict(re.findall(r'(\w+)\[\]\s*=\s*\{\s*#include "levels/castle_inside/([^"]+)\.inc\.c"', (folder / 'texture.inc.c').read_text()))
    mesh_body = re.search(r'seg2_painting_triangle_mesh\[\]\s*=\s*\{(.*?)\};', clean((REF / 'bin/segment2.c').read_text()), re.S)[1]
    mesh = list(map(int, re.findall(r'-?\d+', mesh_body)))
    shapes, layouts = {}, []
    for index, (name, body) in enumerate(re.findall(r'struct Painting (\w+)_painting = \{(.*?)\};', painting, re.S)):
        if index not in visible:
            continue
        f = [v.strip() for v in body.split(',')]
        names = lambda key: [v.strip() for v in arrays[key].split(',') if v.strip()]
        parts = []
        assert len(names(f[28])) == len(names(f[29])) == int(f[1], 0)
        for mapping, texture in zip(names(f[28]), names(f[29])):
            if mapping not in shapes:
                values = list(map(int, re.findall(r'-?\d+', arrays[mapping])))
                stop = 1 + 3 * values[0]
                vertices = []
                for j in range(1, stop, 3):
                    point, u, v = values[j:j+3]
                    x, y, movable = mesh[1+point*3:4+point*3]
                    vertices.append([x, y, movable, u, v])
                assert len(values[stop+1:]) == values[stop] * 3
                shapes[mapping] = dict(vertices=vertices, triangles=values[stop+1:])
            parts.append(dict(shape=mapping, texture=images[texture] + '.png'))
        numbers = lambda start, count: [float(v.rstrip('f')) for v in f[start:start+count]]
        layouts.append(dict(name=name, index=index, id=int(f[0], 0), pos=numbers(9, 3), rotation=numbers(7, 2),
                            size=numbers(38, 1)[0], dimensions=numbers(30, 2), opacity=int(f[34], 0)/255,
                            environment=f[2] == 'PAINTING_ENV_MAP', magnitude=numbers(13, 2),
                            decay=numbers(16, 2), rate=numbers(19, 2), dispersion=numbers(22, 2), parts=parts))
    return dict(shapes=shapes, layouts=layouts)

def door_motion(clip):
    source = (REF / f'assets/anims/anim_{clip}.inc.c').read_text()
    def words(name):
        body = re.search(r'_' + name + r'\[\]\s*=\s*\{(.*?)\};', source, re.S)[1]
        return [int(word, 16) for word in re.findall(r'0x[\da-fA-F]+', body)]
    indices, values = words('indices'), words('values')
    def component(axis, frame):
        count, offset = indices[axis * 2:axis * 2 + 2]
        word = values[offset + min(count - 1, frame)]
        return (word if word < 32768 else word - 65536) / 4
    return [[component(0, frame), component(2, frame)] for frame in range(indices[0])]

OUT.mkdir(exist_ok=True)
levels = ('bob', 'castle_grounds', 'castle_inside', 'wf', 'bbh', 'ccm', 'jrb', 'lll', 'ssl', 'wdw', 'sl', 'ttm', 'thi', 'ttc', 'rr', 'bitdw', 'bitfs', 'bits', 'bowser_1', 'bowser_2', 'bowser_3', 'pss', 'hmc', 'sa', 'ddd', 'totwc')
exports = [(level, 1) for level in levels] + [('castle_inside', 2), ('castle_inside', 3), ('ddd', 2), ('jrb', 2), ('ccm', 2)]
for level, area_index in exports:
    folder = REF / 'levels' / level
    script = (folder / 'script.c').read_text()
    geo = dict(args(script, 'AREA'))[str(area_index)]
    spawn = next(iter(args(script, 'MARIO_POS')), ('1', '0', '0', '0', '0'))
    terrain_path = folder / f'areas/{area_index}/collision.inc.c'
    area_script = area_commands(script, area_index)
    data = collision(terrain_path, args(area_script, 'TERRAIN')[0][0])
    data.update(model=f'levels/{level}/{geo}.glb', spawn={'pos': list(map(int, spawn[2:5])), 'faceYaw': round(int(spawn[1]) * 65536 / 360)}, objects=[], coins=[], goombas=[])
    models = dict(args(script, 'LOAD_MODEL_FROM_GEO'))
    script = area_script
    if args(script, 'ROOMS'):
        room_source = clean((folder / f'areas/{area_index}/room.inc.c').read_text())
        data['rooms'] = list(map(int, re.findall(r'-?\d+', room_source.split('{', 1)[1].split('}', 1)[0])))
        assert len(data['rooms']) == len(data['triangles']), level
        geo_source = clean((folder / f'areas/{area_index}/geo.inc.c').read_text())
        count = int(re.search(r'GEO_SWITCH_CASE\(\s*(\d+),\s*geo_switch_area', geo_source)[1])
        data['roomModels'] = [data['model']] + [f'levels/{level}/variants/{geo}/geo_switch_area_{i}.glb' for i in range(1, count)]
        if level == 'castle_inside':
            layouts = dict(re.findall(r'GeoLayout (\w+)\[\] = \{(.*?)\};', geo_source, re.S))
            branches = args(layouts[geo], 'GEO_BRANCH')
            data['roomPaintings'] = [[int(i) for i in re.findall(r'PAINTING_ID\(\s*(\d+),\s*1\)', layouts[name])]
                                     for _, name in branches]
            assert len(branches) == count
    if ('TERRAIN_SNOW',) in args(script, 'TERRAIN_TYPE'):
        data['snowTerrain'] = True
    spawn_behaviors = {'bhvFlyingWarp', 'bhvSwimmingWarp', 'bhvAirborneWarp', 'bhvSpinAirborneCircleWarp', 'bhvSpinAirborneWarp', 'bhvPaintingStarCollectWarp', 'bhvAirborneStarCollectWarp', 'bhvLaunchStarCollectWarp'}
    for obj in args(script, 'OBJECT'):
        if obj[-1] in spawn_behaviors:
            data.setdefault('warpSpawns', {})[obj[7]] = dict(
                pos=list(map(int, obj[1:4])), faceYaw=round(int(obj[5]) * 65536 / 360), behavior=obj[-1])
    for node, dest, area, entry, _ in args(script, 'WARP_NODE'):
        if node == 'WARP_NODE_WARP_FLOOR' and dest == 'LEVEL_JRB':
            data['floorWarp'] = dict(area=int(area), entry=entry)
        if node == 'WARP_NODE_SUCCESS' and dest == 'LEVEL_CASTLE':
            data['successEntry'] = entry
            data['successArea'] = int(area)
    instant_warps = args(script, 'INSTANT_WARP')
    if instant_warps:
        data['instantWarps'] = {int(index): dict(area=int(area), delta=list(map(int, delta)))
                                for index, area, *delta in instant_warps}
    if level == 'castle_inside':
        data['paintingArt'] = castle_paintings(folder, area_index)
        data['paintings'] = {int(node[-2:], 16): dest[6:].lower()
                             for node, dest, area, _, _ in args(script, 'PAINTING_WARP_NODE')
                             if dest[6:].lower() in levels and area == '1'}
    warp_links = {node: (dest[6:].lower(), int(area), entry) for node, dest, area, entry, _ in args(script, 'WARP_NODE')}
    chest_sets = {'bhvTreasureChestsJRB': 'jrb', 'bhvTreasureChestsDDD': 'ddd', 'bhvTreasureChestsShip': 'ship'}
    for obj in args(script, 'OBJECT') + args(script, 'OBJECT_WITH_ACTS'):
        behavior = obj[8]
        if behavior not in chest_sets:
            continue
        kind = chest_sets[behavior]
        source = clean((REF / 'src/game/behaviors/treasure_chest.inc.c').read_text())
        init = re.search(r'void bhv_treasure_chest_' + kind + r'_init\(void\) \{(.*?)\}', source, re.S)[1]
        loop = source.split(f'void bhv_treasure_chest_{kind}_loop(void)', 1)[1]
        if kind == 'ship':
            data['drainWater'] = True
        else:
            target = args(loop, 'spawn_default_star')[0]
            data['chestStar'] = dict(act=int(re.search(r'STAR_INDEX_ACT_(\d+)', obj[7])[1]),
                                     pos=[float(v.rstrip('f')) for v in target])
        for index, x, y, z, yaw in args(init, 'spawn_treasure_chest'):
            data['objects'].append(dict(model='actors/treasure_chest/treasure_chest_base_geo.glb',
                                        behavior='chest', param=int(index), pos=list(map(int, (x, y, z))),
                                        yaw=int(yaw, 0) * 360 / 65536))
    for obj in args(script, 'OBJECT'):
        if obj[-1] == 'bhvCapSwitch' and level == 'totwc':
            data['wingSwitch'] = list(map(int,obj[1:4]))
        if obj[-1] in ('bhvHiddenRedCoinStar', 'bhvBowserCourseRedCoinStar') and level in ('sa','totwc','bitdw','bitfs','bits'):
            data['redCoinStar'] = list(map(int, obj[1:4]))
        if obj[-1] not in ('bhvWarp', 'bhvWarpPipe'):
            continue
        fields = [field.strip() for field in obj[7].split('|')]
        node = next((field for field in fields if field.startswith('WARP_NODE_')), None)
        destination, dest_area, entry = warp_links.get(node, (None, None, None))
        slide = level == 'ccm' and destination == level and dest_area == 2
        if not slide and (dest_area != 1 or destination not in ('sa', 'pss', 'bitdw', 'bitfs', 'bits', 'bowser_1', 'bowser_2', 'bowser_3')):
            continue
        pos = list(map(int, obj[1:4]))
        param = next((int(field[6:], 0) for field in fields if field.startswith('first_')), 0)
        data.setdefault('warps', []).append(dict(pos=pos, param=param, warpTo=destination, warpEntry=entry, **({'warpArea':dest_area} if slide else {})))
        if obj[-1] == 'bhvWarpPipe':
            data['objects'].append(dict(model='actors/warp_pipe/warp_pipe_geo.glb', pos=pos,
                                        yaw=int(obj[5]), collision='warp_pipe'))
    # Front door pairs in area 1 share warp-node IDs 00/01 across both maps.
    if level in ('castle_grounds', 'castle_inside', 'ccm'):
        links = {a[0]: a[1:4] for a in args(script, 'WARP_NODE')}
        doors = []
        def add_door(pos, yaw, node, model='castle_door_geo', key=None):
            if not node.startswith('WARP_NODE_'):
                node = f'WARP_NODE_{int(node, 0):02X}'
            dest, area, entry = links.get(node, (None, None, None))
            if dest not in ('LEVEL_CASTLE', 'LEVEL_CASTLE_GROUNDS','LEVEL_CCM'):
                return
            door = dict(pos=pos, yaw=yaw, node=node, destNode=entry, destArea=int(area),
                        destination='castle_inside' if dest == 'LEVEL_CASTLE' else dest[6:].lower())
            if key:
                door['requiredKey'] = key
            doors.append(door)
            data['objects'].append(dict(model=f'actors/door/{model}.glb', pos=pos, yaw=yaw))
        for a in args(terrain_path.read_text(), 'SPECIAL_OBJECT_WITH_YAW_AND_PARAM'):
            if 'door' in a[0] and 'warp' in a[0]:
                model = {'special_metal_door_warp': 'metal_door_geo', 'special_wooden_door_warp': 'wooden_door_geo'}.get(a[0], 'castle_door_geo')
                add_door(list(map(int, a[1:4])), int(a[4], 0) * 360 / 256, a[5], model)
        for a in args(script, 'OBJECT'):
            if a[-1] != 'bhvDoorWarp':
                continue
            fields = [field.strip() for field in a[7].split('|')]
            node = next((field for field in fields if field.startswith('WARP_NODE_')), None)
            if not node:
                continue
            if a[0] == 'MODEL_CASTLE_KEY_DOOR':
                door_id = next((int(field[6:], 0) for field in fields if field.startswith('first_')), 0)
                add_door(list(map(int, a[1:4])), int(a[5]), node, 'key_door_geo', {1: 2, 2: 1}.get(door_id))
            elif 'CASTLE_DOOR' in a[0]:
                add_door(list(map(int, a[1:4])), int(a[5]), node)
        room_doors = []
        door_models = {'special_0stars_door': ('castle_door_0_star_geo', 0),
                       'special_1star_door': ('castle_door_1_star_geo', 1),
                       'special_3star_door': ('castle_door_3_stars_geo', 3),
                       'special_wooden_door': ('wooden_door_geo', 0)}
        for preset, x, y, z, yaw in args(terrain_path.read_text(), 'SPECIAL_OBJECT_WITH_YAW'):
            if preset in door_models:
                model, stars = door_models[preset]
                room_doors.append(dict(pos=list(map(int, [x, y, z])), yaw=int(yaw, 0) * 360 / 256,
                                       stars=stars, sliding=False, model=f'actors/door/{model}.glb'))
        for obj in args(script, 'OBJECT'):
            if obj[-1] == 'bhvStarDoor':
                room_doors.append(dict(pos=list(map(int, obj[1:4])), yaw=int(obj[5]), stars=int(obj[7][6:]),
                                       sliding=True, model=f'levels/{level}/{models[obj[0]]}.glb'))
        for door in room_doors:
            data['objects'].append({field: door[field] for field in ('pos', 'yaw', 'model')})
        if room_doors:
            data['roomDoors'] = room_doors
            data['doorMotion'] = {mode: door_motion(clip) for mode, clip in [('pull', '5F'), ('push', '60')]}
        data['doors'] = doors
        if level == 'castle_inside' and area_index > 1:
            data['spawn'] = dict(pos=doors[0]['pos'], faceYaw=0)
    frozen = {'bhvWFRotatingWoodenPlatform': 'rotating_wooden_platform', 'bhvWFSlidingPlatform': 'sliding_platform', 'bhvRotatingPlatform': 'rotating_platform', 'bhvSmallBomp': 'small_bomp', 'bhvLargeBomp': 'large_bomp', 'bhvWFBreakableWallRight': 'breakable_wall_right', 'bhvWFBreakableWallLeft': 'breakable_wall_left'} if level == 'wf' else {}
    # Act-dependent script objects. Only King Bob-omb is implemented so far.
    for a in args(script, 'OBJECT_WITH_ACTS'):
        if a[8] in ('bhvSunkenShipPart2', 'bhvShipPart3'):
            rotation = [0xE958, 0xEE6C, 0x0C80] if a[8] == 'bhvSunkenShipPart2' else [0, 0, 0]
            data['objects'].append(dict(model=f'levels/{level}/{models[a[0]]}.glb', pos=list(map(int, a[1:4])),
                                        yaw=0, rotation=rotation, acts=act_list(a[-1])))
        elif a[8] in ('bhvInSunkenShip', 'bhvInSunkenShip2', 'bhvInSunkenShip3'):
            names = {'bhvInSunkenShip': 'in_sunken_ship', 'bhvInSunkenShip2': 'in_sunken_ship_2', 'bhvInSunkenShip3': 'in_sunken_ship_3'}
            rotation = [0, 0, 0] if a[8].endswith('3') else [0xE958, 0xEE6C, 0x0C80]
            data.setdefault('shipCollision', []).append(dict(name=names[a[8]], pos=list(map(int, a[1:4])),
                                                             rotation=rotation, acts=act_list(a[-1])))
        elif level == 'wf' and a[8] in ('bhvTower', 'bhvBulletBillCannon'):
            data['objects'].append(dict(model=f'levels/wf/{models[a[0]]}.glb', pos=list(map(int,a[1:4])),
                                        yaw=int(a[5]), acts=act_list(a[-1])))
        elif a[8] == 'bhvTowerPlatformGroup':
            source = clean((REF/'src/game/behaviors/tower_platform.inc.c').read_text())
            children = args(source.split('void spawn_wf_platform_group(void)',1)[1], 'spawn_and_init_wf_platforms')
            kinds = {'bhvWFSolidTowerPlatform':'tower_solid','bhvWFSlidingTowerPlatform':'tower_slide',
                     'bhvWFElevatorTowerPlatform':'tower_elevator'}
            x,y,z = map(int,a[1:4])
            for index,(model,behavior) in enumerate(children):
                angle = index*math.pi/4
                data['objects'].append(dict(model=f'levels/wf/{models[model]}.glb',yaw=index*45,param=index,
                                            pos=[x+704*math.sin(angle),y+index*100,z+704*math.cos(angle)],
                                            collision=kinds[behavior],acts=act_list(a[-1])))
        elif a[8] == 'bhvCannonClosed':
            param = int(a[7], 0)
            data.setdefault('cannons', []).append(dict(pos=list(map(int,a[1:4])), yaw=int(a[5]),
                                                       param=param, acts=act_list(a[-1])))
        elif a[8] == 'bhvKingBobomb':
            data['objects'].append({
                'model': 'actors/king_bobomb/king_bobomb_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'kingBobomb',
                'acts': act_list(a[-1]),
            })
        elif level == 'bob' and a[8] == 'bhvHiddenStar':
            data['secretStar'] = dict(act=int(re.search(r'STAR_INDEX_ACT_(\d+)', a[7])[1]),
                                      pos=list(map(int, a[1:4])), triggers=[])
        elif a[8] == 'bhvStar':
            act = int(re.search(r'STAR_INDEX_ACT_(\d+)', a[7])[1])
            data.setdefault('placedStars', []).append(dict(act=act, pos=list(map(int, a[1:4])), acts=act_list(a[-1])))
        elif a[8] == 'bhvCCMTouchedStarSpawn':
            source = clean((REF/'src/game/behaviors/spawn_star_exit.inc.c').read_text())
            pos = [float(v.rstrip('f')) for v in args(source,'spawn_default_star')[0]]
            data['secretStar'] = dict(act=1,pos=pos,triggers=[],origin=[2780,int(a[2])+100,4666])
        elif a[8] == 'bhvHiddenRedCoinStar':
            data['redCoinAct'] = int(re.search(r'STAR_INDEX_ACT_(\d+)', a[7])[1])
            data['redCoinStar'] = list(map(int, a[1:4]))
        elif a[8] == 'bhvWhompKingBoss':
            data['objects'].append({
                'model': 'actors/whomp/whomp_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'whompKing',
                'acts': act_list(a[-1]),
            })
        elif a[8] == 'bhv1Up':
            data['objects'].append({
                'model': 'actors/mushroom_1up/mushroom_1up_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'oneUp',
                'acts': act_list(a[-1]),
            })
        elif a[8] in ('bhvKlepto',):
            data['objects'].append({
                'model': 'actors/klepto/klepto_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'klepto',
                'acts': act_list(a[-1]),
            })
        elif a[8] in ('bhvSmallPenguin', 'bhvTuxiesMother', 'bhvRacingPenguin'):
            data['objects'].append({
                'model': 'actors/penguin/penguin_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'penguin',
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
                'param': int(a[7], 0),
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
        elif a[-1] == 'bhvBowserBomb':
            data['objects'].append(dict(model='actors/bomb/bowser_bomb_geo.glb', behavior='bowserMine',
                                        pos=list(map(int, a[1:4])), yaw=int(a[5])))
        elif a[-1] in ('bhvTiltingBowserLavaPlatform', 'bhvFallingBowserPlatform'):
            platform = 'tilting_platform' if level == 'bowser_2' else f'falling_platform_{int(a[7])}'
            data['objects'].append(dict(model=f'levels/{level}/{models[a[0]]}.glb',
                                        pos=list(map(int, a[1:4])), yaw=int(a[5]),
                                        collision=platform + '/collision.inc.c'))
        elif a[-1] in ('bhvBowsersSub', 'bhvBowserSubDoor'):
            part = 'submarine' if a[-1] == 'bhvBowsersSub' else 'sub_door'
            data['objects'].append(dict(model=f'levels/{level}/{models[a[0]]}.glb', pos=list(map(int, a[1:4])),
                                        yaw=int(a[5]), collision=part + '/collision.inc.c', submarine=True))
        elif a[-1] in BBH_MOVING and a[0] in models:
            data['objects'].append({
                'model': f'levels/bbh/{models[a[0]]}.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'collision': BBH_MOVING[a[-1]] + '/collision.inc.c',
            })
        elif a[-1] in ('bhvBigBully', 'bhvBigBullyWithMinions'):
            data['objects'].append({
                'model': 'actors/bully/bully_boss_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'bigBully',
            })
        elif a[-1] == 'bhvSmallBully':
            data['objects'].append({
                'model': 'actors/bully/bully_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'bully',
            })
        elif a[-1] == 'bhvSmallWhomp':
            data['objects'].append({
                'model': 'actors/whomp/whomp_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'whomp',
            })
        elif a[-1] == 'bhvMrBlizzard':
            data['objects'].append({
                'model': 'actors/snowman/mr_blizzard_geo.glb',
                'pos': list(map(int, a[1:4])),
                'yaw': int(a[5]),
                'behavior': 'mrBlizzard',
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
            spec = dict(model=f'levels/{level}/{models[a[0]]}.glb', pos=list(map(int,a[1:4])), yaw=int(a[5]),
                        param=platform_param(a[7],a[-1]), collision=frozen[a[-1]]+'/collision.inc.c')
            if a[-1] in ('bhvWFBreakableWallLeft','bhvWFBreakableWallRight'):
                spec.update(behavior='blastWall',param=int(a[-1]=='bhvWFBreakableWallRight'))
            data['objects'].append(spec)
    for a in args(terrain_path.read_text(), 'SPECIAL_OBJECT(?:_WITH_YAW)?'):
        preset = a[0]
        if preset == 'special_bowser':
            data['objects'].append(dict(model='actors/bowser/bowser_geo_no_shadow.glb',
                                        pos=list(map(int, a[1:4])), yaw=0, behavior='bowser', param=int(level[-1])))
        elif preset == 'special_mine':
            data['objects'].append(dict(model='actors/bomb/bowser_bomb_geo.glb', behavior='bowserMine',
                                        pos=list(map(int, a[1:4])), yaw=0))
        elif preset == 'special_bubble_tree':
            data['objects'].append({'model': 'actors/tree/bubbly_tree_geo.glb', 'pos': list(map(int, a[1:4])), 'yaw': 0, 'behavior': 'tree'})
        elif preset.startswith('special_level_geo_'):
            model = models.get('MODEL_LEVEL_GEOMETRY_' + preset.removeprefix('special_level_geo_'))
            if model:
                path = f'actors/yellow_sphere/{model}.glb' if model == 'bowser_1_yellow_sphere_geo' else f'levels/{level}/{model}.glb'
                data['objects'].append({'model': path, 'pos': list(map(int, a[1:4])), 'yaw': (int(a[4], 0) if len(a) > 4 else 0) * 360 / 256})
    macro = folder / f'areas/{area_index}/macro.inc.c'
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
        for a in args(macro.read_text(), 'MACRO_OBJECT_WITH_BHV_PARAM'):
            if a[0] == 'macro_water_bomb_spawner':
                data.setdefault('waterBombSpawners', []).append(dict(pos=list(map(int,a[2:5])),param=int(a[5],0)))
            if a[0] == 'macro_cannon_closed':
                data.setdefault('cannons', []).append(dict(pos=list(map(int,a[2:5])), yaw=int(a[1]), param=int(a[5],0)))
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
            elif a[0] in ('macro_water_bomb_cannon_1', 'macro_water_bomb_cannon_2'):
                data['objects'].append(dict(model='actors/cannon_base/cannon_base_geo.glb', behavior='waterBombCannon',
                                            param=0, pos=list(map(int,a[2:5])), yaw=int(a[1])))
            elif a[0].startswith('macro_bobomb_buddy_opens_cannon'):
                data['objects'].append(dict(model='actors/bobomb/bobomb_buddy_geo.glb', behavior='bobombBuddy',
                                            param=1, pos=list(map(int,a[2:5])), yaw=int(a[1])))
            elif a[0] in ('macro_pokey_1', 'macro_pokey_2'):
                data['objects'].append(dict(
                    model='actors/pokey/pokey_head_geo.glb',
                    pos=list(map(int, a[2:5])),
                    yaw=int(a[1]),
                    behavior='pokey',
                ))
            elif a[0] in ('macro_goomba', 'macro_goomba_triplet_spawner'):
                data['goombas'].append(dict(pos=list(map(int, a[2:5])), triplet=a[0].endswith('spawner')))
            elif a[0] == 'macro_hidden_star_trigger' and 'secretStar' in data:
                data['secretStar']['triggers'].append(list(map(int, a[2:5])))
            elif a[0] == 'macro_box_wing_cap':
                data['objects'].append(dict(model='actors/exclamation_box/exclamation_box_geo.glb',behavior='wingBox',
                                            pos=list(map(int,a[2:5])),yaw=int(a[1])))
            elif level in ('bob', 'jrb') and a[0].startswith('macro_box_star_act_'):
                data['objects'].append(dict(model='actors/exclamation_box/variants/exclamation_box_geo/geo_switch_anim_state_3.glb',
                                            behavior='starBox', pos=list(map(int, a[2:5])), yaw=int(a[1]),
                                            param=int(a[0][-1])))
            elif 'redCoinStar' in data and a[0] == 'macro_red_coin':
                data.setdefault('redCoins', []).append(list(map(int, a[2:5])))
            elif a[0].startswith('macro_coin_'):
                shape = a[0].removeprefix('macro_coin_').removesuffix('_flying')
                # MACRO_OBJECT encodes yaw in seven bits.
                yaw = (int(int(a[1]) * 16 / 45) & 127) << 9
                data.setdefault('coinFormations', []).append(dict(shape=shape, pos=list(map(int, a[2:5])),
                                                                  yaw=yaw, flying=a[0].endswith('_flying')))
            elif a[0] in ('macro_yellow_coin_1', 'macro_yellow_coin_2'):
                data['coins'].append(list(map(int, a[2:5])))
    # Keep native collision for additional static terrain objects, where supplied.
    extras = {'bob': ['seesaw_platform/collision.inc.c'], 'wf': ['areas/1/10/collision.inc.c', 'areas/1/11/collision.inc.c'], 'castle_grounds': ['areas/1/7/collision.inc.c', 'areas/1/8/collision.inc.c']}.get(level, [])
    extras += sorted(set(p + '/collision.inc.c' for p in frozen.values()))
    if level == 'bbh':
        extras += [d + '/collision.inc.c' for d in BBH_MOVING.values()]
    extras.extend(obj['collision'] for obj in data['objects'] if obj.get('collision', '').endswith('/collision.inc.c'))
    data['extraCollision'] = {p: collision(folder / p) for p in dict.fromkeys(extras)}
    if any(obj.get('collision') == 'warp_pipe' for obj in data['objects']):
        data['extraCollision']['warp_pipe'] = collision(REF / 'actors/warp_pipe/collision.inc.c')
    if data.get('cannons'):
        data['extraCollision']['cannonLid'] = collision(REF / 'actors/cannon_lid/collision.inc.c')
    for ship in data.get('shipCollision', []):
        data['extraCollision'][ship['name']] = collision(folder / 'wooden_ship/collision.inc.c', 'jrb_seg7_collision_' + ship['name'])
    if level == 'bob':
        data['extraCollision']['chainGate'] = collision(folder / 'chain_chomp_gate/collision.inc.c')
        post = collision(REF / 'actors/poundable_pole/collision.inc.c')
        post['vertices'] = [[int(axis/2) for axis in v] for v in post['vertices']]
        data['extraCollision']['woodenPost'] = post
    if level == 'wf':
        platform = collision(folder/'extending_platform/collision.inc.c')
        for kind in ('tower_solid','tower_slide','tower_elevator'):
            data['extraCollision'][kind] = platform
    if data.get('wingSwitch'):
        for name,symbol in [('switchTop','capswitch_collision_050033D0'),('switchBase','capswitch_collision_05003448')]:
            data['extraCollision'][name] = collision(REF/'actors/capswitch/collision.inc.c',symbol)
    if any(obj.get('behavior') in ('starBox','wingBox') for obj in data['objects']):
        box = collision(REF / 'actors/exclamation_box_outline/collision.inc.c')
        box['vertices'] = [[2 * axis for axis in v] for v in box['vertices']]
        data['extraCollision']['starBox'] = box
    if any(obj[-1] == 'bhvDDDWarp' for obj in args(script, 'OBJECT')):
        for name in ('ddd_warp', 'ddd_warp_2'):
            data['extraCollision'][name] = collision(terrain_path, 'inside_castle_seg7_collision_' + name)
    if data.get('doors') or data.get('roomDoors'):
        data['extraCollision']['door'] = collision(REF / 'actors/warp_collision/collision.inc.c', 'door_seg3_collision_0301CE78')
    if any(door['sliding'] for door in data.get('roomDoors', [])):
        data['extraCollision']['star_door'] = collision(folder / 'star_door/collision.inc.c')
    kept = []
    skipped = 0
    for obj in data['objects']:
        model = obj.get('model')
        if model and not glb_has_meshes(ROOT / 'assets' / model):
            skipped += 1
            continue
        kept.append(obj)
    data['objects'] = kept
    (OUT / (f'{level}.json' if area_index == 1 else f'{level}.{area_index}.json')).write_text(json.dumps(data, separators=(',', ':')))
    print(f'{level}: {len(data["triangles"])} terrain triangles, {len(data["objects"])} static objects, {skipped} meshless decor skipped')
