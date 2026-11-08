"""Add SM64 surface objects (moving platforms, TTC, Bowser arena parts) to the exported courses.

Run after scripts/export-courses.py (it post-processes assets/courses/<level>.json in place and is
idempotent). It reads the level scripts, macro lists and behavior tables of reference/sm64 and writes:

  solids          [{behavior, model, collision, pos, angle[pitch,yaw,roll in s16], bparams, track?}]
  extraCollision  the collision meshes those solids use (keyed by decomp symbol)

for ttc, rr, bitdw, bitfs, bits, bowser_1, bowser_2 and bowser_3, and re-points the Bowser arena
objects to the faithful behaviors (`sm64Bowser`, `sm64BowserBomb`). The behaviors themselves are
native/sim/src/solids/*.rs and native/sim/src/behaviors/bowser_sm64.rs.
"""
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve()
src = (HERE.parent / 'export-courses.py').read_text().split('OUT.mkdir')[0]
__file__ = str(HERE.parent / 'export-courses.py')
exec(src)

LEVELS = ['ttc', 'rr', 'bitdw', 'bitfs', 'bits', 'bowser_1', 'bowser_2', 'bowser_3']

# ---- symbol index -------------------------------------------------------------------------
SYMBOLS = {}
for path in list((REF / 'levels').glob('*/**/collision.inc.c')) + list((REF / 'actors').glob('*/collision.inc.c')):
    for sym in re.findall(r'const Collision (\w+)\[\]', path.read_text()):
        SYMBOLS[sym] = path

# ---- constants ----------------------------------------------------------------------------
DEFINES = {}
def _load_defines():
    text = clean((REF / 'include/object_constants.h').read_text())
    pending = re.findall(r'#define\s+(\w+)\s+([^\n]+)', text)
    for _ in range(6):
        for name, expr in pending:
            if name in DEFINES:
                continue
            try:
                DEFINES[name] = int(eval(re.sub(r'\b[A-Za-z_]\w*\b', lambda m: str(DEFINES[m[0]]) if m[0] in DEFINES else m[0], expr.strip().rstrip('\\')), {}))
            except Exception:
                pass
_load_defines()

def eval_bparam(expr):
    expr = expr.strip()
    expr = re.sub(r'BPARAM([1-4])\(([^()]*(?:\([^()]*\))?[^()]*)\)',
                  lambda m: f'((({m[2]})&0xFF)<<{ {"1": 24, "2": 16, "3": 8, "4": 0}[m[1]] })', expr)
    expr = re.sub(r'\b[A-Za-z_]\w*\b', lambda m: str(DEFINES[m[0]]) if m[0] in DEFINES else '0', expr)
    return int(eval(expr, {})) & 0xFFFFFFFF

def deg_to_s16(deg):
    v = round(float(deg) * 65536 / 360) & 0xFFFF
    return v - 65536 if v >= 32768 else v

def split_args(body):
    parts, depth, cur = [], 0, ''
    for ch in body:
        if ch == '(':
            depth += 1
        elif ch == ')':
            depth -= 1
        if ch == ',' and depth == 0:
            parts.append(cur.strip())
            cur = ''
        else:
            cur += ch
    parts.append(cur.strip())
    return parts

def commands(text, name):
    """Balanced-paren argument lists of every `name(...)`."""
    out = []
    for m in re.finditer(r'\b' + name + r'\s*\(', text):
        depth, i = 1, m.end()
        while depth:
            depth += {'(': 1, ')': -1}.get(text[i], 0)
            i += 1
        out.append(split_args(text[m.end():i - 1]))
    return out

def macro_presets():
    text = clean((REF / 'include/macro_presets.inc.c').read_text())
    presets = {}
    for name, bhv, model, param in re.findall(r'/\*\s*(macro_\w+)\s*\*/\s*\{\s*(\w+)\s*,\s*(\w+)\s*,\s*([^}]*)\}', (REF / 'include/macro_presets.inc.c').read_text()):
        param = re.sub(r'/\*.*?\*/|//.*', '', param).strip().rstrip(',')
        try:
            value = eval_bparam(param) if param else 0
        except Exception:
            value = 0
        presets[name] = (bhv, model, value)
    return presets
PRESETS = macro_presets()

def collision_for(symbol, cache):
    path = SYMBOLS[symbol]
    if symbol not in cache:
        cache[symbol] = collision(path, symbol)
    return symbol

# ---- per-behavior model -> collision selection (the tables of the decomp behaviors) ----------
SLIDING2 = ['bits_seg7_collision_0701A9A0', 'bits_seg7_collision_0701AA0C', 'bitfs_seg7_collision_07015714',
            'bitfs_seg7_collision_07015768', 'rr_seg7_collision_070295F8', 'rr_seg7_collision_0702967C', None,
            'bitdw_seg7_collision_0700F688']
OCTAGON = ['bits_seg7_collision_0701AA84', 'rr_seg7_collision_07029508']
ACTIVATED = ['bits_seg7_collision_0701AD54', 'bitfs_seg7_collision_070157E0', 'bitfs_seg7_collision_07015124']
SEESAW = ['bitdw_seg7_collision_0700F70C', 'bits_seg7_collision_0701ADD8', 'bits_seg7_collision_0701AE5C',
          'bob_seg7_collision_bridge', 'bitfs_seg7_collision_07015928', 'rr_seg7_collision_07029750',
          'rr_seg7_collision_07029858', None]
FERRIS = {'bits': ('bits_seg7_collision_0701ACAC', 'bits_seg7_collision_0701AC28', 'MODEL_BITS_BLUE_PLATFORM'),
          'bitdw': ('bitdw_seg7_collision_0700F7F0', 'bitdw_seg7_collision_0700F898', 'MODEL_BITDW_BLUE_PLATFORM')}
FIXED = {
    'bhvSwingPlatform': 'rr_seg7_collision_pendulum',
    'bhvRRElevatorPlatform': 'rr_seg7_collision_elevator_platform',
    'bhvRRRotatingBridgePlatform': 'rr_seg7_collision_rotating_platform_with_fire',
    'bhvSquarishPathMoving': 'bitdw_seg7_collision_moving_pyramid',
    'bhvBitFSTiltingInvertedPyramid': 'bitfs_seg7_collision_inverted_pyramid',
    'bhvBitFSSinkingPlatforms': 'bitfs_seg7_collision_sinking_platform',
    'bhvBitFSSinkingCagePlatform': 'bitfs_seg7_collision_sinking_cage_platform',
    'bhvSquishablePlatform': 'bitfs_seg7_collision_squishable_platform',
    'bhvTiltingBowserLavaPlatform': 'bowser_2_seg7_collision_tilting_platform',
}
TTC_COLLISION = {
    ('bhvTTCRotatingSolid', 0): 'ttc_seg7_collision_07014F70', ('bhvTTCRotatingSolid', 1): 'ttc_seg7_collision_07015008',
    ('bhvTTCCog', 0): 'ttc_seg7_collision_07015584', ('bhvTTCCog', 2): 'ttc_seg7_collision_07015650',
    ('bhvTTCPitBlock', 0): 'ttc_seg7_collision_07015754', ('bhvTTCPitBlock', 1): 'ttc_seg7_collision_070157D8',
    ('bhvTTCTreadmill', 0): 'ttc_seg7_collision_070152B4', ('bhvTTCTreadmill', 1): 'ttc_seg7_collision_070153E0',
    ('bhvTTC2DRotator', 0): 'ttc_seg7_collision_clock_main_rotation',
    ('bhvTTCElevator', 0): 'ttc_seg7_collision_clock_platform',
    ('bhvTTCMovingBar', 0): 'ttc_seg7_collision_sliding_surface',
    ('bhvTTCPendulum', 0): 'ttc_seg7_collision_clock_pendulum',
    ('bhvTTCSpinner', 0): 'ttc_seg7_collision_rotating_clock_platform2',
}
FALLING = {i: f'bowser_3_seg7_collision_{h}' for i, h in enumerate(
    ['', '07004B94', '07004C18', '07004C9C', '07004D20', '07004DA4', '07004E28', '07004EAC', '07004F30', '07004FB4', '07005038'])}

def pick_collision(bhv, bp):
    hi = (bp >> 16) & 0xFFFF
    b2 = (bp >> 16) & 0xFF
    if bhv == 'bhvSlidingPlatform2':
        return SLIDING2[(hi & 0x0380) >> 7]
    if bhv == 'bhvOctagonalPlatformRotating':
        return OCTAGON[b2]
    if bhv == 'bhvActivatedBackAndForthPlatform':
        return ACTIVATED[(hi & 0x0300) >> 8]
    if bhv == 'bhvSeesawPlatform':
        return SEESAW[b2]
    if bhv == 'bhvFallingBowserPlatform':
        return FALLING[b2]
    if bhv == 'bhvPlatformOnTrack':
        return TRACK_COLLISION[(hi & 0x0070) >> 4]
    if bhv in FIXED:
        return FIXED[bhv]
    if bhv == 'bhvTTCCog':
        return TTC_COLLISION[(bhv, bp >> 16 & 2)]
    if bhv == 'bhvTTCTreadmill':
        return TTC_COLLISION[(bhv, b2 & 1)]
    if bhv in ('bhvTTCRotatingSolid', 'bhvTTCPitBlock'):
        return TTC_COLLISION[(bhv, b2)]
    if bhv == 'bhvTTC2DRotator':
        return TTC_COLLISION[(bhv, 0)] if b2 == 0 else None
    return TTC_COLLISION.get((bhv, 0))

HANDLED = {
    'bhvTTCRotatingSolid', 'bhvTTC2DRotator', 'bhvTTCCog', 'bhvTTCElevator', 'bhvTTCMovingBar', 'bhvTTCPendulum',
    'bhvTTCPitBlock', 'bhvTTCSpinner', 'bhvTTCTreadmill',
    'bhvSlidingPlatform2', 'bhvSquarishPathMoving', 'bhvOctagonalPlatformRotating', 'bhvSwingPlatform',
    'bhvActivatedBackAndForthPlatform', 'bhvFerrisWheelAxle', 'bhvBitFSSinkingPlatforms',
    'bhvBitFSSinkingCagePlatform', 'bhvSquishablePlatform', 'bhvSeesawPlatform', 'bhvRRCruiserWing',
    'bhvRRRotatingBridgePlatform', 'bhvRRElevatorPlatform', 'bhvBitFSTiltingInvertedPyramid',
    'bhvTiltingBowserLavaPlatform', 'bhvFallingBowserPlatform', 'bhvPlatformOnTrack',
}

# sPlatformOnTrackPaths / sPlatformOnTrackCollisionModels (platform_on_track.inc.c)
TRACK_PATHS = [('rr', 'rr_seg7_trajectory_0702EC3C'), ('rr', 'rr_seg7_trajectory_0702ECC0'), ('ccm', 'ccm_seg7_trajectory_0701669C'),
               ('bitfs', 'bitfs_seg7_trajectory_070159AC'), ('hmc', 'hmc_seg7_trajectory_0702B86C'), ('lll', 'lll_seg7_trajectory_0702856C'),
               ('lll', 'lll_seg7_trajectory_07028660'), ('rr', 'rr_seg7_trajectory_0702ED9C'), ('rr', 'rr_seg7_trajectory_0702EEE0')]
TRACK_COLLISION = ['rr_seg7_collision_07029038', None, None, 'bitfs_seg7_collision_070157E0']

def load_trajectory(level, symbol):
    text = clean((REF / 'levels' / level / 'areas/1/trajectory.inc.c').read_text())
    body = re.search(r'\b' + symbol + r'\[\]\s*=\s*\{(.*?)\};', text, re.S)[1]
    rows = [[int(v) for v in a] for a in args(body, 'TRAJECTORY_POS')]
    rows.append([-1, 0, 0, 0])
    return rows

def model_path(level, models, name):
    geo = models.get(name)
    if not geo:
        return ''
    if geo == 'warp_pipe_geo':
        return 'actors/warp_pipe/warp_pipe_geo.glb'
    path = f'levels/{level}/{geo}.glb'
    return path if (ROOT / 'assets' / path).exists() else ''

def build(level):
    folder = REF / 'levels' / level
    script = (folder / 'script.c').read_text()
    models = dict(args(script, 'LOAD_MODEL_FROM_GEO'))
    area = area_commands(script, 1)
    cache = {}
    solids = []
    missing = []

    def add(bhv, model, pos, angle, bp, collision_sym=None, **extra):
        sym = collision_sym if collision_sym is not None else pick_collision(bhv, bp)
        key = collision_for(sym, cache) if sym else None
        solids.append(dict(behavior=bhv, model=model_path(level, models, model) if model.startswith('MODEL_') else model,
                           collision=key, pos=[int(p) for p in pos], angle=angle, bparams=bp, **extra))

    for cmd in ('OBJECT', 'OBJECT_WITH_ACTS'):
        for a in commands(area, cmd):
            model, x, y, z, rx, ry, rz, bparam, bhv = a[:9]
            if bhv not in HANDLED and bhv not in ('bhvFerrisWheelAxle',):
                continue
            bp = eval_bparam(bparam)
            angle = [deg_to_s16(rx), deg_to_s16(ry), deg_to_s16(rz)]
            if bhv == 'bhvFerrisWheelAxle':
                axle_col, plat_col, plat_model = FERRIS[level]
                b2 = (bp >> 16) & 0xFF
                if level == 'bits':
                    b2 = 0
                add(bhv, model, (x, y, z), angle, bp, collision_sym=axle_col)
                for i in range(4):
                    add('bhvFerrisWheelPlatform', plat_model, (x, y, z), angle, i << 16, collision_sym=plat_col)
                continue
            if bhv == 'bhvPlatformOnTrack':
                lvl, sym = TRACK_PATHS[((bp >> 16) & 0xFFFF) & 0xF]
                add(bhv, model, (x, y, z), angle, bp, track=load_trajectory(lvl, sym))
                continue
            add(bhv, model, (x, y, z), angle, bp)
            if bhv == 'bhvBitFSSinkingCagePlatform':
                solids.append(dict(behavior='bhvDDDMovingPole', model=model_path(level, models, 'MODEL_BITFS_BLUE_POLE'),
                                   collision=None, pos=[int(x), int(y), int(z)], angle=angle, bparams=bp))
    flamethrowers = []
    for cmd in ('OBJECT', 'OBJECT_WITH_ACTS'):
        for a in commands(area, cmd):
            if a[8] == 'bhvFlamethrower':
                flamethrowers.append(dict(model='', pos=[int(a[1]), int(a[2]), int(a[3])], yaw=deg_to_s16(a[5]) * 360 / 65536,
                                          behavior='sm64Flamethrower', param=(eval_bparam(a[7]) >> 16) & 0xFF))
    # TTC macros (and every other macro object of a handled preset)
    macro = folder / 'areas/1/macro.inc.c'
    if macro.exists():
        text = clean(macro.read_text())
        for cmd in ('MACRO_OBJECT', 'MACRO_OBJECT_WITH_BHV_PARAM'):
            for a in commands(text, cmd):
                preset, yaw, x, y, z = a[:5]
                if preset not in PRESETS:
                    continue
                given = eval_bparam(a[5]) if len(a) > 5 else 0
                bhv, model, pparam = PRESETS[preset]
                if bhv not in HANDLED:
                    continue
                params = given if pparam == 0 else (given & 0xFF00) + (pparam & 0xFF)
                bp = ((params & 0xFF) << 16) + (params & 0xFF00)
                macro_yaw = (int(int(yaw) * 16 / 45) & 127) << 9
                macro_yaw = macro_yaw - 65536 if macro_yaw >= 32768 else macro_yaw
                add(bhv, model, (x, y, z), [0, macro_yaw, 0], bp)
    # Bowser arena: falling platforms keep their model-by-number
    return solids, cache, flamethrowers

def main():
    for level in LEVELS:
        path = OUT / f'{level}.json'
        data = json.loads(path.read_text())
        solids, cache, flamethrowers = build(level)
        data['solids'] = solids
        data['objects'] = [o for o in data['objects'] if o.get('behavior') != 'sm64Flamethrower'] + flamethrowers
        extra = data.setdefault('extraCollision', {})
        for sym, mesh in cache.items():
            extra[sym] = {'vertices': mesh['vertices'], 'triangles': mesh['triangles'], 'waterBoxes': mesh.get('waterBoxes', [])}
        # Bowser arenas: faithful behaviors; the platform meshes become solids
        if level.startswith('bowser_'):
            arena = int(level[-1]) - 1
            solid_models = {s['model'] for s in solids if s['model']}
            kept = []
            for obj in data['objects']:
                if obj.get('behavior') == 'bowser':
                    obj['behavior'] = 'sm64Bowser'
                    obj['param'] = arena
                elif obj.get('behavior') == 'bowserMine':
                    obj['behavior'] = 'sm64BowserBomb'
                if obj.get('behavior') is None and obj.get('model') in solid_models:
                    continue
                kept.append(obj)
            data['objects'] = kept
        path.write_text(json.dumps(data, separators=(',', ':')))
        counts = {}
        for s in solids:
            counts[s['behavior']] = counts.get(s['behavior'], 0) + 1
        print(f'{level}: {len(solids)} solids {counts}')

if __name__ == '__main__':
    main()
