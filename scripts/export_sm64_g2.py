"""SM64 group 2 (courses 7-13: lll ssl ddd sl wdw ttm thi) object export.

Called by export-courses.py just before a course json is written. For every `OBJECT` /
`OBJECT_WITH_ACTS` / `MACRO_OBJECT` of the seven courses whose behavior the Rust sim ports
(`native/sim/src/behaviors/g2/`), it appends a placement to `data['objects']` (the sim reads
`behavior`, `model`, `pos`, `yaw` in degrees, `param` = the full `oBhvParams` word, `acts`) and
adds the object's collision model to `data['extraCollision']` under `g2/<symbol>` so the sim
can rebuild it every frame (`behaviors/g2/dyn_collision.rs`).

Nothing here touches the other exporters' output: objects of other courses are never read.
"""
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
G2_LEVELS = ('lll', 'ssl', 'ddd', 'sl', 'wdw', 'ttm', 'thi')
# Areas the base exporter does not list (volcano, pyramid, igloo, downtown, ...): (level, area).
EXTRA_AREAS = [('lll', 2), ('ssl', 2), ('wdw', 2), ('thi', 2), ('thi', 3)]


def clean(text):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)


def split_args(s):
    out, depth, cur = [], 0, ''
    for ch in s:
        if ch == '(':
            depth += 1
        if ch == ')':
            depth -= 1
        if ch == ',' and depth == 0:
            out.append(cur.strip())
            cur = ''
        else:
            cur += ch
    out.append(cur.strip())
    return out


def call_args(text, names):
    """Arguments of every `NAME(...)` call in `text` (balanced parentheses), in source order."""
    text = clean(text)
    found = []
    for m in re.finditer(r'\b(' + '|'.join(names) + r')\s*\(', text):
        i = m.end()
        depth, j = 1, i
        while depth:
            depth += {'(': 1, ')': -1}.get(text[j], 0)
            j += 1
        found.append((m.group(1), split_args(text[i:j - 1])))
    return found


_consts = None


def constants():
    global _consts
    if _consts is None:
        _consts = {}
        for name in ('include/object_constants.h', 'include/behavior_data.h'):
            for k, v in re.findall(r'#define\s+(\w+)\s+(-?(?:0x[0-9a-fA-F]+|\d+))\b', (REF / name).read_text()):
                _consts[k] = int(v, 0)
    return _consts


def evaluate(expr):
    """Evaluate a C bhvParam expression: BPARAMn(x) | BPARAMn(y) | CONSTANT | number."""
    consts = constants()
    expr = expr.strip()

    def bparam(m):
        shift = {'1': 24, '2': 16, '3': 8, '4': 0}[m.group(1)]
        return str((evaluate(m.group(2)) & 0xFF) << shift)
    expr = re.sub(r'BPARAM([1-4])\(([^()]*(?:\([^()]*\)[^()]*)*)\)', bparam, expr)
    total = 0
    for term in expr.split('|'):
        term = term.strip()
        if not term:
            continue
        if re.fullmatch(r'-?(0x[0-9a-fA-F]+|\d+)', term):
            total |= int(term, 0)
        elif term in consts:
            total |= consts[term]
        else:
            total |= 0
    return total


def act_list(expression):
    return [int(v) for v in re.findall(r'ACT_(\d+)', expression)]


_collision_symbols = None


def collision_file(symbol):
    global _collision_symbols
    if _collision_symbols is None:
        _collision_symbols = {}
        for path in list(REF.glob('levels/*/**/collision.inc.c')) + list(REF.glob('actors/*/collision.inc.c')):
            for sym in re.findall(r'const Collision (\w+)\[\]', path.read_text()):
                _collision_symbols[sym] = path
    return _collision_symbols.get(symbol)


def parse_collision(symbol):
    path = collision_file(symbol)
    if path is None:
        return None
    text = clean(path.read_text())
    body = re.search(r'\b' + symbol + r'\[\]\s*=\s*\{(.*?)\};', text, re.S)[1]
    vertices = [[int(v) for v in a] for _, a in call_args(body, ['COL_VERTEX'])]
    surfaces = {name: int(value, 0) for name, value in re.findall(r'#define\s+(SURFACE_\w+)\s+(0x[0-9A-Fa-f]+)', (REF / 'include/surface_terrains.h').read_text())}
    triangles, surface = [], 0
    for command, a in call_args(body, ['COL_TRI_INIT', 'COL_TRI_SPECIAL', 'COL_TRI']):
        if command == 'COL_TRI_INIT':
            surface = surfaces[a[0]]
        else:
            idx = list(map(int, a[:3]))
            triangles.append([*idx, surface, int(a[3], 0) if len(a) == 4 else 0])
    return {'vertices': vertices, 'triangles': triangles}


_behavior_collision = None


def behavior_collision(bhv):
    """`LOAD_COLLISION_DATA` symbol of a behavior script, if it has one."""
    global _behavior_collision
    if _behavior_collision is None:
        text = clean((REF / 'data/behavior_data.c').read_text())
        _behavior_collision = {}
        for name, body in re.findall(r'const BehaviorScript (\w+)\[\]\s*=\s*\{(.*?)\n\};', text, re.S):
            m = re.search(r'LOAD_COLLISION_DATA\((\w+)\)', body)
            if m:
                _behavior_collision[name] = m[1]
    return _behavior_collision.get(bhv)


# bhv name -> (sim kind, model).  model: 'level' = the OBJECT's own MODEL_* via LOAD_MODEL_FROM_GEO,
# None = invisible, or an explicit glb path under assets/.
# Per-course tables live in COURSE_TABLES below.
RED_FLAME = 'actors/flame/red_flame_geo.glb'

COMMON = {}
# Macro presets (macro_presets.inc.c) the sim ports: preset -> (kind, model).
MACROS = {'macro_circling_amp': ('circlingAmp', 'actors/amp/dAmpGeo.glb')}
COURSE_TABLES = {
    'lll': {
        'bhvLLLRotatingHexagonalPlatform': ('lllHexPlatform', 'level'),
        'bhvLLLRotatingHexagonalRing': ('lllHexRing', 'level'),
        'bhvLLLSinkingRectangularPlatform': ('lllSinkRect', 'level'),
        'bhvLLLSinkingSquarePlatforms': ('lllSinkSquare', 'level'),
        'bhvLLLTiltingInvertedPyramid': ('lllTiltPyramid', 'level'),
        'bhvLLLMovingOctagonalMeshPlatform': ('lllOctMesh', 'level'),
        'bhvLLLSinkingRockBlock': ('lllSinkRock', 'level'),
        'bhvLLLRollingLog': ('rollingLogLll', 'level'),
        'bhvLLLFloatingWoodBridge': ('lllWoodBridge', None),
        'bhvLLLDrawbridgeSpawner': ('lllDrawbridgeSpawner', None),
        'bhvLLLRotatingBlockWithFireBars': ('lllFireBars', 'level'),
        'bhvLLLVolcanoFallingTrap': ('volcanoTrap', 'level'),
        'bhvLLLBowserPuzzle': ('bowserPuzzle', None),
        'bhvFlamethrower': ('flamethrower', None),
        'bhvBouncingFireball': ('bouncingFireball', None),
        'bhvPoleGrabbing': ('pole', None),
    },
    'ssl': {
        'bhvPyramidTop': ('pyramidTop', 'level'),
        'bhvPyramidElevator': ('pyramidElevator', 'level'),
        'bhvSSLMovingPyramidWall': ('pyramidWall', 'level'),
        'bhvToxBox': ('toxBox', 'level'),
        'bhvGrindel': ('grindel', 'level'),
        'bhvHorizontalGrindel': ('horizontalGrindel', 'level'),
        'bhvSpindel': ('spindel', 'level'),
        'bhvPoleGrabbing': ('pole', None),
    },
    'thi': {
        'bhvTHIHugeIslandTop': ('thiHugeIslandTop', 'level'),
        'bhvTHITinyIslandTop': ('thiTinyIslandTop', 'level'),
        'bhvFirePiranhaPlant': ('firePiranhaPlant', 'actors/piranha_plant/piranha_plant_geo.glb'),
        'bhvPoleGrabbing': ('pole', None),
    },
    'wdw': {
        'bhvWDWSquareFloatingPlatform': ('squareFloatingPlatform', 'level'),
        'bhvWDWRectangularFloatingPlatform': ('rectFloatingPlatform', 'level'),
        'bhvArrowLift': ('arrowLift', 'level'),
        'bhvInitializeChangingWaterLevel': ('waterLevelInit', None, ''),
        'bhvWaterLevelDiamond': ('waterLevelDiamond', 'level'),
        'bhvWDWExpressElevator': ('expressElevator', 'level'),
        'bhvWDWExpressElevatorPlatform': ('expressElevatorPlatform', 'level'),
        'bhvRotatingPlatform': ('rotatingPlatformWdw', 'level', 'wdw_seg7_collision_070186B4'),
        'bhvFloorSwitchHiddenObjects': ('floorSwitchHidden', 'actors/purple_switch/purple_switch_geo.glb', 'purple_switch_seg8_collision_0800C7A8'),
        'bhvHiddenObject': ('hiddenObject', 'level', 'wdw_seg7_collision_07018528'),
        'bhvPoleGrabbing': ('pole', None),
    },
}

# Collision models the runtime-spawned children need even though no placement names them.
EXTRA_COLLISION = {
    'lll': ['lll_seg7_collision_wood_piece', 'lll_seg7_collision_drawbridge', 'lll_seg7_collision_puzzle_piece'],
}


def level_model(level, models, token):
    geo = models.get(token)
    return f'levels/{level}/{geo}.glb' if geo else None


def add_objects(level, area_index, area_script, data, models):
    """Append the group-2 placements of one area to `data`."""
    table = {**COMMON, **COURSE_TABLES.get(level, {})}
    added = 0
    for command, a in call_args(area_script, ['OBJECT_WITH_ACTS', 'OBJECT']):
        bhv = a[-2] if command == 'OBJECT_WITH_ACTS' else a[-1]
        if bhv == 'bhvHiddenStar':
            # Five `bhvHiddenStarTrigger` macro objects reveal it (hidden_star.inc.c).
            macro = REF / f'levels/{level}/areas/{area_index}/macro.inc.c'
            triggers = []
            if macro.exists():
                for _, m in call_args(macro.read_text(), ['MACRO_OBJECT']):
                    if m[0].strip() == 'macro_hidden_star_trigger':
                        triggers.append([int(float(v)) for v in m[2:5]])
            data['secretStar'] = dict(act=int(re.search(r'STAR_INDEX_ACT_(\d+)', a[7])[1]), pos=[int(float(v)) for v in a[1:4]], triggers=triggers)
            continue
        if bhv not in table:
            continue
        kind, model, *override = table[bhv]
        if model == 'level':
            model = level_model(level, models, a[0])
        pos = [int(float(v)) for v in a[1:4]]
        entry = dict(behavior=kind, pos=pos, yaw=int(a[5]), param=evaluate(a[7]), g2=True)
        if model:
            entry['model'] = model
        if command == 'OBJECT_WITH_ACTS':
            entry['acts'] = act_list(a[-1])
        symbol = override[0] if override else behavior_collision(bhv)
        if symbol:
            entry['collision'] = f'g2/{symbol}'
        data['objects'].append(entry)
        added += 1
    # Macro objects (macro_objs tables): MACRO_OBJECT(preset, yaw, x, y, z) and ..._WITH_BHV_PARAM(.., param).
    macro = REF / f'levels/{level}/areas/{area_index}/macro.inc.c'
    if macro.exists():
        for command, m in call_args(macro.read_text(), ['MACRO_OBJECT', 'MACRO_OBJECT_WITH_BHV_PARAM']):
            if m[0].strip() in MACROS:
                kind, model = MACROS[m[0].strip()]
                param = (evaluate(m[5]) & 0xFF) << 16 if command == 'MACRO_OBJECT_WITH_BHV_PARAM' else 0
                data['objects'].append(dict(behavior=kind, model=model, pos=[int(float(v)) for v in m[2:5]], yaw=int(m[1]), param=param, g2=True))
                added += 1
    keys = {o['collision'] for o in data['objects'] if o.get('g2') and o.get('collision')}
    keys |= {f'g2/{s}' for s in EXTRA_COLLISION.get(level, ())} if added else set()
    for key in sorted(keys):
        mesh = parse_collision(key[3:])
        if mesh:
            data['extraCollision'][key] = mesh
    return added


def add_links(level, area_index, area_script, data, warp_links):
    """In-level warps: `bhvWarp` / `bhvWarpPipe` to another area become `warps` entries (the
    sim's `warp` object switches area in place), `bhvFadingWarp` pairs in one area become
    `fadingWarps`."""
    pads = {}
    for command, a in call_args(area_script, ['OBJECT_WITH_ACTS', 'OBJECT']):
        bhv = a[-2] if command == 'OBJECT_WITH_ACTS' else a[-1]
        fields = [f.strip() for f in a[7].split('|')]
        node = next((f for f in fields if re.fullmatch(r'WARP_NODE_\w+', f)), None)
        if node is None:
            m = re.search(r'BPARAM2\((WARP_NODE_\w+)\)', a[7])
            node = m[1] if m else None
        if bhv == 'bhvFadingWarp' and node:
            pads[node] = a
        if bhv == 'bhvInstantActiveWarp' and node:
            # Arrival point of a warp the level joins with an INSTANT_WARP-less hole (thi area 3).
            data.setdefault('warpSpawns', {})[node] = dict(pos=[int(float(v)) for v in a[1:4]], faceYaw=round(int(a[5]) * 65536 / 360), behavior=bhv)
        if bhv == 'bhvWarpPipe' and node:
            # Mario comes out beside the pipe (the pipe mouth is also the warp trigger).
            yaw = int(a[5]) * math.pi / 180
            x, y, z = [int(float(v)) for v in a[1:4]]
            data.setdefault('warpSpawns', {})[node] = dict(pos=[round(x + 260 * math.sin(yaw)), y, round(z + 260 * math.cos(yaw))], faceYaw=round(int(a[5]) * 65536 / 360), behavior=bhv)
        if bhv not in ('bhvWarp', 'bhvWarpPipe') or not node:
            continue
        dest, dest_area, entry = warp_links.get(node, (None, None, None))
        if dest != level or dest_area == area_index:
            continue
        pos = [int(float(v)) for v in a[1:4]]
        first = (evaluate(a[7]) >> 24) & 0xFF
        data.setdefault('warps', []).append(dict(pos=pos, param=first, warpTo=level, warpEntry=entry, warpArea=dest_area, g2=True))
        if bhv == 'bhvWarpPipe':
            data['objects'].append(dict(model='actors/warp_pipe/warp_pipe_geo.glb', pos=pos, yaw=int(a[5]), collision='warp_pipe', g2=True))
            data['extraCollision']['warp_pipe'] = parse_collision('warp_pipe_seg3_collision_03009AC8') or data['extraCollision'].get('warp_pipe')
    for node, a in pads.items():
        dest, dest_area, entry = warp_links.get(node, (None, None, None))
        if dest != level or dest_area != area_index or entry not in pads:
            continue
        target = pads[entry]
        data.setdefault('fadingWarps', []).append(dict(pos=[int(float(v)) for v in a[1:4]], target=[int(float(v)) for v in target[1:4]],
                                                       faceYaw=round(int(target[5]) * 65536 / 360)))
