"""Super Mario 64 world graph: the levels/areas beyond the original TypeScript export, their warps, and the report the
Rust world-graph test checks (`native/src/game/sm64_warp_tests.rs`).  Imported by `export-courses.py` (like
`mk64_courses.py`); standalone `python3 scripts/sm64_world.py` prints the decomp warp graph.

Two kinds of output:
  * brand-new courses/areas (`castle_courtyard`, `cotmc`, `vcutm`, `wmotr` and the sub-areas of lll, ssl, sl, thi, ttm,
    wdw) get everything merged into their `assets/courses/<id>[.<area>].json`;
  * courses that were already exported only get an *extension* file `<id>[.<area>].ext.json` with the additions (more
    warps, doors, fading warps, cap boxes, arrival points ...).  The native loader merges it (`course_data::
    read_course_value`); the golden fixtures that replay the TypeScript sim bit-for-bit load the courses without it.
All positions/angles follow the decomp (`levels/<level>/script.c`, `areas/<n>/collision.inc.c`, `macro.inc.c`).
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / 'reference/sm64'
OUT = ROOT / 'assets/courses'

LEGACY_LEVELS = ('bob', 'castle_grounds', 'castle_inside', 'wf', 'bbh', 'ccm', 'jrb', 'lll', 'ssl', 'wdw', 'sl', 'ttm',
                 'thi', 'ttc', 'rr', 'bitdw', 'bitfs', 'bits', 'bowser_1', 'bowser_2', 'bowser_3', 'pss', 'hmc', 'sa',
                 'ddd', 'totwc')
LEGACY = frozenset([(level, 1) for level in LEGACY_LEVELS] + [('castle_inside', 2), ('castle_inside', 3), ('ddd', 2),
                                                               ('jrb', 2), ('ccm', 2)])
NEW_LEVELS = ('castle_courtyard', 'cotmc', 'vcutm', 'wmotr')
NEW_AREAS = (('lll', 2), ('ssl', 2), ('ssl', 3), ('sl', 2), ('thi', 2), ('thi', 3), ('ttm', 2), ('ttm', 3), ('ttm', 4),
             ('wdw', 2))
ALL_LEVELS = LEGACY_LEVELS + NEW_LEVELS
EXPORTS = sorted(LEGACY | {(level, 1) for level in NEW_LEVELS} | set(NEW_AREAS))
# Level script names that are not simply the lower-cased directory name.
DEST_NAMES = {'LEVEL_CASTLE': 'castle_inside'}
# Not playable levels (title screen, ending cutscene, file select): never part of the world graph.
UNPLAYABLE = ('ending', 'intro', 'menu')
# `EXCLAMATION_BOX_BP_*` / `SAVE_FLAG_HAVE_*_CAP`: the box kind is also the anim state of the box model.
CAP_BOX = {'macro_box_wing_cap': 0, 'macro_box_metal_cap': 1, 'macro_box_vanish_cap': 2}
CAP_FLAG = {0: 2, 1: 4, 2: 8}
# `bhvCapSwitch` BPARAM2: 0 wing (TotWC), 1 metal (CotMC), 2 vanish (VCutM).
SPAWN_MARKERS = {'bhvFlyingWarp', 'bhvSwimmingWarp', 'bhvAirborneWarp', 'bhvSpinAirborneCircleWarp', 'bhvSpinAirborneWarp',
                 'bhvPaintingStarCollectWarp', 'bhvAirborneStarCollectWarp', 'bhvLaunchStarCollectWarp', 'bhvDeathWarp',
                 'bhvPaintingDeathWarp', 'bhvAirborneDeathWarp', 'bhvLaunchDeathWarp'}
# Arrival markers that only name a standing spot (`warp_action` has no action for them).
# (`sWarpBhvSpawnTable`: a warp or pipe is a MARIO_SPAWN_UNKNOWN_03 arrival, Mario stands there.)
STANDING_MARKERS = {'bhvInstantActiveWarp', 'bhvExitPodiumWarp', 'bhvHardAirKnockBackWarp', 'bhvFadingWarp', 'bhvWarp',
                    'bhvWarpPipe'}
# GEO_BACKGROUND_COLOR(0x0001) is opaque black; the sky boxes other than the default ocean sky.
SKIES = {
    'black': dict(top=[0.0, 0.0, 0.0], mid=[0.0, 0.0, 0.0], bottom=[0.0, 0.0, 0.0], fog=[0.0, 0.0, 0.0],
                  fogStart=30000.0, fogEnd=60000.0),
    # wmotr: BACKGROUND_BELOW_CLOUDS, a bright sea of clouds
    'below_clouds': dict(top=[0.38, 0.62, 0.92], mid=[0.78, 0.88, 0.97], bottom=[0.97, 0.98, 1.0],
                         fog=[0.9, 0.94, 1.0], fogStart=14000.0, fogEnd=40000.0),
}


def clean(text):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', text, flags=re.S)


def args(text, command):
    text = re.sub(r'BPARAM([1-4])\(([^()]*)\)',
                  lambda m: m[2] if m[1] == '2' else ('first_' + m[2] if m[1] == '1' else '0'), text)
    return [tuple(s.strip() for s in m.split(',')) for m in re.findall(r'\b' + command + r'\s*\(([^()]*)\)', clean(text))]


def area_commands(script, index):
    source = clean(script)
    area = re.search(r'\bAREA\(\s*' + str(index) + r'\s*,[^)]*\),(.*?)END_AREA\(', source, re.S)
    blocks = dict(re.findall(r'LevelScript\s+(\w+)\[\]\s*=\s*\{(.*?)\};', source, re.S))

    def expand(body):
        return re.sub(r'JUMP_LINK\(\s*(\w+)\s*\)', lambda match: expand(blocks[match[1]]), body)

    return expand(area[1])


def course_of(dest):
    return DEST_NAMES.get(dest, dest[6:].lower())


def key(level, area):
    return f'{level}.{area}'


def yaw_s16(degrees):
    return round(int(degrees) * 65536 / 360)


def node_of(param):
    return next((f for f in (x.strip() for x in param.split('|')) if f.startswith('WARP_NODE_')), None)


def first_of(param):
    return next((int(f[6:], 0) for f in (x.strip() for x in param.split('|')) if f.startswith('first_')), 0)


def level_script(level):
    return (REF / 'levels' / level / 'script.c').read_text()


def area_indices(level):
    return [int(i) for i in re.findall(r'\bAREA\(\s*(\d+)', clean(level_script(level)))]


def objects_of(commands):
    """(model, pos, yaw, param, behavior) for OBJECT and OBJECT_WITH_ACTS."""
    for o in args(commands, 'OBJECT'):
        yield o[0], list(map(int, o[1:4])), int(o[5]), o[7], o[8] if len(o) > 8 else o[-1]
    for o in args(commands, 'OBJECT_WITH_ACTS'):
        yield o[0], list(map(int, o[1:4])), int(o[5]), o[7], o[8]


def terrain_text(level, area):
    return (REF / 'levels' / level / f'areas/{area}/collision.inc.c').read_text()


def triggers(level, area):
    """Every way the decomp leaves (level, area): the ground truth of the world graph."""
    script = level_script(level)
    commands = area_commands(script, area)
    links = {n: (course_of(d), int(a), e) for n, d, a, e, _ in args(commands, 'WARP_NODE')}
    out = []
    for model, pos, yaw, param, bhv in objects_of(commands):
        node = node_of(param)
        if bhv in ('bhvWarp', 'bhvWarpPipe', 'bhvFadingWarp') and node in links:
            kind = {'bhvWarp': 'warp', 'bhvWarpPipe': 'pipe', 'bhvFadingWarp': 'fading'}[bhv]
            to = links[node]
            out.append(dict(kind=kind, node=node, pos=pos, to=to[0], toArea=to[1], toNode=to[2]))
        elif bhv == 'bhvBooWithCage' and node in links:
            to = links[node]
            out.append(dict(kind='cage', node=node, pos=pos, to=to[0], toArea=to[1], toNode=to[2]))
        elif bhv == 'bhvDoorWarp' and node in links:
            to = links[node]
            out.append(dict(kind='door', node=node, pos=pos, to=to[0], toArea=to[1], toNode=to[2]))
    for preset, x, y, z, yaw, param in args(terrain_text(level, area), 'SPECIAL_OBJECT_WITH_YAW_AND_PARAM'):
        if 'door' in preset and 'warp' in preset:
            node = f'WARP_NODE_{int(param, 0):02X}'
            if node in links:
                to = links[node]
                out.append(dict(kind='door', node=node, pos=[int(x), int(y), int(z)], to=to[0], toArea=to[1], toNode=to[2]))
    for index, dest_area, *delta in args(commands, 'INSTANT_WARP'):
        out.append(dict(kind='instant', node=f'INSTANT_{index}', pos=[0, 0, 0], to=level, toArea=int(dest_area), toNode='',
                        delta=list(map(int, delta))))
    for node, dest, dest_area, entry, _ in args(commands, 'PAINTING_WARP_NODE'):
        out.append(dict(kind='painting', node=node, pos=[0, 0, 0], to=course_of(dest), toArea=int(dest_area), toNode=entry))
    return out


def specials(level, area):
    """death / success / warp-floor targets of the area (`WARP_NODE_DEATH`, `_SUCCESS`, `_WARP_FLOOR`)."""
    commands = area_commands(level_script(level), area)
    out = {}
    for node, dest, dest_area, entry, _ in args(commands, 'WARP_NODE'):
        name = {'WARP_NODE_DEATH': 'death', 'WARP_NODE_SUCCESS': 'success', 'WARP_NODE_WARP_FLOOR': 'floor'}.get(node)
        if name:
            out[name] = dict(to=course_of(dest), toArea=int(dest_area), toNode=entry)
    return out


def arrivals(level, area):
    """Arrival markers: node -> behavior of the object that places Mario there."""
    commands = area_commands(level_script(level), area)
    out = {}
    for model, pos, yaw, param, bhv in objects_of(commands):
        node = node_of(param)
        if node and (bhv in SPAWN_MARKERS or bhv in STANDING_MARKERS):
            out.setdefault(node, bhv)
    return out


def report():
    """The decomp world graph for every exported (course, area), for the Rust test."""
    exported = {key(*e) for e in EXPORTS}
    rep = dict(exported=sorted(exported), unplayable=list(UNPLAYABLE), areas={})
    for level, area in EXPORTS:
        rep['areas'][key(level, area)] = dict(triggers=triggers(level, area), specials=specials(level, area),
                                              arrivals=arrivals(level, area))
    # Every level directory of the decomp is either exported or named unplayable.
    levels = {p.name for p in (REF / 'levels').iterdir() if (p / 'script.c').exists()}
    rep['levelsInDecomp'] = sorted(levels)
    rep['levelAreasInDecomp'] = {lv: area_indices(lv) for lv in sorted(levels) if lv not in UNPLAYABLE}
    return rep


def write_report():
    OUT.mkdir(exist_ok=True)
    (OUT / 'sm64_warp_report.json').write_text(json.dumps(report(), separators=(',', ':')))


# --------------------------------------------------------------------------------------------------------------------
# course JSON additions


def _door_objects(level, area, data, collision):
    """Castle-style doors whose destination the original exporter did not know (the courtyard door)."""
    script = level_script(level)
    commands = area_commands(script, area)
    links = {n: (course_of(d), int(a), e) for n, d, a, e, _ in args(commands, 'WARP_NODE')}
    have = {d['node'] for d in data.get('doors', [])}
    doors, objects = [], []
    for preset, x, y, z, yaw, param in args(terrain_text(level, area), 'SPECIAL_OBJECT_WITH_YAW_AND_PARAM'):
        if 'door' not in preset or 'warp' not in preset:
            continue
        node = f'WARP_NODE_{int(param, 0):02X}'
        if node in have or node not in links:
            continue
        dest, dest_area, entry = links[node]
        model = {'special_metal_door_warp': 'metal_door_geo', 'special_wooden_door_warp': 'wooden_door_geo'}.get(preset, 'castle_door_geo')
        pos, door_yaw = [int(x), int(y), int(z)], int(yaw, 0) * 360 / 256
        doors.append(dict(pos=pos, yaw=door_yaw, node=node, destNode=entry, destArea=dest_area, destination=dest))
        objects.append(dict(model=f'actors/door/{model}.glb', pos=pos, yaw=door_yaw))
    return doors, objects


def _ensure_door_collision(ext, data, collision):
    if not data.get('extraCollision', {}).get('door'):
        ext.setdefault('extraCollision', {})['door'] = collision(REF / 'actors/warp_collision/collision.inc.c',
                                                                 'door_seg3_collision_0301CE78')


def extend(level, area, data, script, folder, models, collision):
    """Additions for (level, area).  `script` is the area's command text, `data` the base export (read only).
    Returns the extension dict; the caller merges it (new courses) or writes it to the `.ext.json` file (legacy)."""
    ext = {}
    links = {n: (course_of(d), int(a), e) for n, d, a, e, _ in args(script, 'WARP_NODE')}
    exported = {(l, a) for l, a in EXPORTS}
    have_pos = {tuple(w['pos']) for w in data.get('warps', [])}
    pads = {}
    for model, pos, yaw, param, bhv in objects_of(script):
        node = node_of(param)
        if bhv == 'bhvFadingWarp' and node:
            pads[node] = (pos, yaw)
    for model, pos, yaw, param, bhv in objects_of(script):
        node = node_of(param)
        # arrival markers that the original exporter ignored
        if node and bhv in STANDING_MARKERS and node not in data.get('warpSpawns', {}):
            ext.setdefault('warpSpawns', {})[node] = dict(pos=pos, faceYaw=yaw_s16(yaw), behavior=bhv)
        if bhv in ('bhvWarp', 'bhvWarpPipe') and node in links:
            dest, dest_area, entry = links[node]
            if (dest, dest_area) not in exported or tuple(pos) in have_pos:
                continue
            ext.setdefault('warps', []).append(dict(pos=pos, param=first_of(param), warpTo=dest, warpEntry=entry, warpArea=dest_area))
            if bhv == 'bhvWarpPipe':
                ext.setdefault('objects', []).append(dict(model='actors/warp_pipe/warp_pipe_geo.glb', pos=pos, yaw=yaw,
                                                          collision='warp_pipe'))
                if 'warp_pipe' not in data.get('extraCollision', {}):
                    ext.setdefault('extraCollision', {})['warp_pipe'] = collision(REF / 'actors/warp_pipe/collision.inc.c')
        elif bhv == 'bhvFadingWarp' and node in links and not data.get('fadingWarps'):
            dest, dest_area, entry = links[node]
            if dest != level or dest_area != area or entry not in pads:
                continue
            target = pads[entry]
            ext.setdefault('fadingWarps', []).append(dict(pos=pos, target=target[0], faceYaw=yaw_s16(target[1])))
        elif bhv == 'bhvBooWithCage' and node in links:
            # The caged Boo of the courtyard opens the Big Boo's Haunt: INTERACT_BBH_ENTRANCE, hitbox 120 x 300
            # (`boo_cage.inc.c`), only present from 12 stars (`SPAWN_CASTLE_BOO_STAR_REQUIREMENT`).
            dest, dest_area, entry = links[node]
            ext.setdefault('warps', []).append(dict(pos=pos, param=12 + 30 * 1000, warpTo=dest, warpEntry=entry,
                                                    warpArea=dest_area, minStars=12))
            ext.setdefault('warpGates', {})[dest] = 12
    # arrival at the far end of a fading warp is the far pad itself: a pad is also an arrival marker (handled above).
    doors, door_objects = _door_objects(level, area, data, collision)
    if doors:
        ext['doors'] = doors
        ext.setdefault('objects', []).extend(door_objects)
        _ensure_door_collision(ext, data, collision)
    # death / success / floor warps whose destination the original `levels` filter did not know
    sp = specials(level, area)
    if 'death' in sp and not data.get('deathWarp') and (sp['death']['to'], sp['death']['toArea']) in exported:
        d = sp['death']
        ext['deathWarp'] = dict(course=d['to'], area=d['toArea'], entry=d['toNode'])
    if 'floor' in sp and not data.get('fallWarp') and (sp['floor']['to'], sp['floor']['toArea']) in exported and level != 'jrb':
        d = sp['floor']
        ext['fallWarp'] = dict(course=d['to'], area=d['toArea'], entry=d['toNode'])
    if 'success' in sp and sp['success']['to'] != 'castle_inside' and sp['success']['to'] in LEGACY_LEVELS + NEW_LEVELS:
        d = sp['success']
        ext['successCourse'] = d['to']
        ext['successEntry'] = d['toNode']
        ext['successArea'] = d['toArea']
    # painting warps: the castle's table (THI's tiny painting leads into area 2) and the course-local nodes (the TTM slide
    # is entered through SURFACE_PAINTING_WARP_D3..D5 floors).  Value: `course` or `course:area`.
    table = {}
    for node, dest, dest_area, entry, _ in args(script, 'PAINTING_WARP_NODE'):
        index = int(node[-2:], 16)
        if (course_of(dest), int(dest_area)) in exported and index not in data.get('paintings', {}):
            table[str(index)] = course_of(dest) if int(dest_area) == 1 else f'{course_of(dest)}:{dest_area}'
    if table:
        ext['paintings'] = table
    # cap switches (CotMC metal, VCutM vanish) and the metal / vanish ! boxes
    for model, pos, yaw, param, bhv in objects_of(script):
        if bhv == 'bhvCapSwitch' and level != 'totwc':
            ext['wingSwitch'] = pos
            ext['capSwitchKind'] = 1 if level == 'cotmc' else 2
    macro = folder / f'areas/{area}/macro.inc.c'
    if macro.exists():
        for a in args(macro.read_text(), 'MACRO_OBJECT'):
            if a[0] in CAP_BOX and CAP_BOX[a[0]] > 0:
                kind = CAP_BOX[a[0]]
                ext.setdefault('objects', []).append(dict(
                    model=f'actors/exclamation_box/variants/exclamation_box_geo/geo_switch_anim_state_{kind}.glb',
                    behavior='wingBox', param=kind, pos=list(map(int, a[2:5])), yaw=int(a[1])))
        if any(o.get('behavior') == 'wingBox' for o in ext.get('objects', [])) and 'starBox' not in data.get('extraCollision', {}):
            box = collision(REF / 'actors/exclamation_box_outline/collision.inc.c')
            box['vertices'] = [[2 * axis for axis in v] for v in box['vertices']]
            ext.setdefault('extraCollision', {})['starBox'] = box
    # skies of the new areas: black interiors, the cloud sea of WMotR
    if (level, area) not in LEGACY:
        geo = (folder / f'areas/{area}/geo.inc.c').read_text()
        if re.search(r'GEO_BACKGROUND_COLOR\(\s*0x0001\s*\)', geo):
            ext['sky'] = SKIES['black']
        elif 'BACKGROUND_BELOW_CLOUDS' in geo:
            ext['sky'] = SKIES['below_clouds']
        # a standalone load of a sub-area starts where its arrival marker is
        if area > 1 and 'WARP_NODE_0A' in {**data.get('warpSpawns', {}), **ext.get('warpSpawns', {})}:
            spawn = data.get('warpSpawns', {}).get('WARP_NODE_0A') or ext['warpSpawns']['WARP_NODE_0A']
            ext['spawn'] = dict(pos=spawn['pos'], faceYaw=spawn['faceYaw'])
    return ext


def merge(base, ext):
    """Apply an extension: arrays append, objects merge key-wise, scalars replace."""
    for k, v in ext.items():
        if isinstance(v, list) and isinstance(base.get(k), list):
            base[k] = base[k] + [x for x in v if x not in base[k]]
        elif isinstance(v, dict) and isinstance(base.get(k), dict):
            base[k] = {**base[k], **v}
        else:
            base[k] = v
    return base


def write_extension(name, ext):
    path = OUT / f'{name}.ext.json'
    if ext:
        path.write_text(json.dumps(ext, separators=(',', ':')))
    elif path.exists():
        path.unlink()


if __name__ == '__main__':
    rep = report()
    for k, v in rep['areas'].items():
        print(k)
        for t in v['triggers']:
            print('   ', t['kind'], t['node'], t['pos'], '->', f"{t['to']}.{t['toArea']} {t['toNode']}")
        print('    specials', v['specials'])
