"""Place the Banjo-Kazooie enemies and collectibles in the exported worlds from the ROM's map setup files.

Every map has a "setup" asset (`file_openMap(map) = asset map + 0x71C`, src/core2/file.c and gsworld.c). Its
`GS_WORLD_START_INDICATOR_1_CUBES` section is a grid of 1000-unit cubes (src/core2/gccube.c
`cubeList_fromFile`, src/core2/code_A5BC0.c `code7AF80_initCubeFromFile`), each holding

* "prop 1" nodes, 20 bytes (`NodeProp` in include/prop.h): position (s16 x3), radius/selector (9 bits),
  category (6 bits; 6 = actor, `cube_spawnActors`), actor id, marker, yaw (9 bits, degrees), scale (23 bits, %);
* "prop 2" props, 12 bytes (`Prop`): model props and sprite props. The notes, eggs and feathers are sprite props
  (`SPRITE_ASSET_OFFSET` 0x572 + sprite id: 0x6D6 music note, 0x6D7 blue egg, 0x580 red feather, 0x6D1 gold feather).

The actors that are enemies go into the course's `objects` as `{"behavior": "bkEnemy", "param": id | scale % << 16}`
(native/sim/src/bk_enemy.rs); the collectibles go into `bkPickups` as `[kind, x, y, z]` (native/src/bk_enemies/).
`assets/courses/bk_setup_report.json` records every placed actor id per world, which the tests check against the
implemented / skipped lists below. Positions are multiplied by the worlds' 1.25 scale (export-bk-levels.py).

    python3 scripts/export-bk-setup.py [world ...]
"""
import argparse
import collections
import hashlib
import json
import math
import re
import runpy
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
bk_banjo = runpy.run_path(str(ROOT / 'scripts/export-bk-banjo.py'))
WORLD_SCALE = 1.25

bk_maps = runpy.run_path(str(ROOT / 'scripts/bk-maps.py'))
# course id -> BK map id (enum map_e): the 14 worlds and every sub-area (scripts/bk-maps.py)
MAPS = {cid: mid for mid, cid in bk_maps['exported_maps']().items()}

# actor id (enum actor_e / ActorInfo.actorId) -> species key of public/models/bk/enemies and sim/src/bk_enemy.rs
ENEMIES = {
    0x04: 'bigbutt', 0x05: 'termite', 0x06: 'grublin', 0x12: 'beehive', 0x56: 'shrapnel', 0x67: 'snippet', 0xF5: 'mutie_snippet',
    0x124: 'sir_slush', 0x163: 'bat', 0x1E9: 'snarebear', 0x34E: 'limbo', 0x375: 'grublin_hood',
}
# Enemy-like or hazardous actors that are placed but not implemented, with the reason (tests count these).
SKIPPED_ENEMIES = {
    0xE: 'small bull (unplaced)', 0xA: 'chump fish (spawned by script)', 0x69: 'clam (minigame prop)', 0x117: 'Nipper boss',
    0x133: 'red Flibbit (skeletal-animation model)', 0x137: 'yellow Flibbit (minigame)', 0x134: 'Buzzbomb (skeletal-animation model)',
    0x13B: 'Flotsam (skeletal-animation model)', 0x1C6: 'Grimlet (skeletal-animation model)', 0x1CC: 'Grille Chompa (skeletal-animation model)',
    0x1FA: 'Croctus (feeding minigame)', 0x147: 'Ancient One (statue)', 0x380: 'Scarab Beetle (skeletal-animation model)',
    0x120: 'Slappa (boss)', 0x29F: 'Clucker (skeletal-animation model)', 0x2A2: 'Caterpillar (skeletal-animation model)',
    0x337: 'Twinkly Muncher (minigame)', 0x336: 'Twinkly box (minigame)', 0x37D: 'ice cube (scenery)', 0x34D: 'bee swarm (effect)',
    0xCA: 'Tee-Hee (ghost)', 0x354: 'drips (effect)', 0xC7: 'gravestone (scenery)', 0x30F: 'Whipcrack (scenery)',
    0x36D: 'Collywobble (tutorial boss)', 0x36E: 'Bawl (tutorial boss)', 0x36F: 'Topper (tutorial boss)', 0x167: 'attack tutorial',
    0x37E: 'dead Snare-bear (scenery)', 0xF6: 'big alligator (boss)', 0xE8: 'Tanktup (boss)', 0x118: 'Grabba (boss)',
}

# sprite id (sprite asset - 0x572) -> pickup kind index (sim PickupKind order: note, egg, red feather, gold feather, honeycomb, jiggy)
SPRITE_PICKUPS = {0x164: 0, 0x165: 1, 0xE: 2, 0x15F: 3}
ACTOR_PICKUPS = {0x52: 1, 0x47: 4, 0x46: 5}


class Stream:
    """`File` of src/core2/file.c: one-byte look-ahead markers (`file_isNextByteExpected`)."""

    def __init__(self, data):
        self.d, self.p, self.last = data, 0, -1

    def byte(self):
        b = self.d[self.p]
        self.p += 1
        return b

    def expect(self, e):
        if self.last == -1:
            b = self.byte()
            if b == e:
                return True
            self.last = b
            return False
        if e == self.last:
            self.last = -1
            return True
        return False

    def words(self, n):
        w = struct.unpack_from('>%di' % n, self.d, self.p)
        self.p += 4 * n
        return w

    def raw(self, n):
        r = self.d[self.p:self.p + n]
        self.p += n
        return r


def parse_setup(data):
    """-> (node props, props): the 20-byte actor/trigger nodes and the 12-byte model/sprite props."""
    f = Stream(data)
    nodes, props = [], []
    while not f.expect(0):
        if f.expect(2):
            continue
        if not f.expect(1):
            break
        lo = f.words(3) if f.expect(1) else None
        hi = f.words(3)
        for _x in range(lo[0], hi[0] + 1):
            for _y in range(lo[1], hi[1] + 1):
                for _z in range(lo[2], hi[2] + 1):
                    while not f.expect(1):
                        if f.expect(0):
                            f.words(3)
                            f.words(3)
                        elif not (f.expect(2) and (f.words(3) or True)) and f.expect(3):
                            if f.expect(0x0A):
                                n = f.byte()
                                if f.expect(0x0B):
                                    nodes += [f.raw(20) for _ in range(n)]
                            elif f.expect(6):
                                n = f.byte()
                                if f.expect(7):
                                    f.raw(12 * n)
                            if f.expect(8):
                                n = f.byte()
                                if f.expect(9):
                                    props += [f.raw(12) for _ in range(n)]
        f.expect(0)
    return nodes, props


def decode(nodes, props):
    actors, pickups = [], []
    for n in nodes:
        x, y, z, bf, actor, marker = struct.unpack_from('>hhhHHB', n)
        category = (bf >> 1) & 63
        if category != 6:
            continue
        word = struct.unpack_from('>I', n, 12)[0]
        yaw, scale = word >> 23, word & 0x7FFFFF
        actors.append(dict(id=actor, pos=(x, y, z), yaw=yaw, scale=scale, radius=bf >> 7))
    for p in props:
        flags = p[11]
        if flags & 2:  # model prop
            continue
        x, y, z = struct.unpack_from('>hhh', p, 4)
        sprite = struct.unpack_from('>I', p)[0] >> 20
        if flags & 1:
            continue
        if sprite in SPRITE_PICKUPS:
            pickups.append((SPRITE_PICKUPS[sprite], x, y, z))
    for a in actors:
        if a['id'] in ACTOR_PICKUPS:
            pickups.append((ACTOR_PICKUPS[a['id']],) + a['pos'])
    return actors, pickups


CAT = bk_maps['CATALOGUE']
EXIT_ACTORS = {v: int(k) for k, v in CAT['exitActors'].items()}   # actor id -> exit id (nodeprop_getExitActorId)
WARP_HANDLERS = {int(k): v for k, v in CAT['warps'].items()}      # category-3 node actor id -> {fn, map, exit}
EXIT_SKIP_IDS = {0x63: 'restores a saved position', 0x65: 'placement skipped (a cutscene positions the player)'}
BK_TO_JUMPMAN_YAW = 65536 / 360


def yaw_units(deg):
    """BK yaw (degrees, 0 = +z, sin/cos like SM64) -> SM64 s16 face angle."""
    u = round(deg * BK_TO_JUMPMAN_YAW)
    return ((u + 32768) % 65536) - 32768


def decode_transitions(nodes):
    """-> (warps, exits). Warps are category-3 nodes (sphere triggers: position + radius + handler index into
    `sWarpFunctions`, code_AD110.c); exits are the actor nodes whose id marks "exit n" (position + yaw where the
    player arrives). Nodes with bit0 set are not NodeProps (the decomp filters on `bit0 == FALSE`)."""
    warps, exits = [], {}
    for n in nodes:
        x, y, z, bf, actor, _marker = struct.unpack_from('>hhhHHB', n)
        if bf & 1:
            continue
        category = (bf >> 1) & 63
        if category == 3:
            warps.append(dict(handler=actor, pos=(x, y, z), radius=bf >> 7))
        elif category == 6 and actor in EXIT_ACTORS:
            word = struct.unpack_from('>I', n, 12)[0]
            exits.setdefault(EXIT_ACTORS[actor], dict(pos=(x, y, z), yaw=word >> 23))
    return warps, exits


def warp_object_pos(pos):
    """Trigger sphere centre (BK units) -> the sim warp object's position (Jumpman units). The sim warp is a
    cylinder 50 units tall standing on the object; Mario's body spans 160 above his feet, so the object sits
    100 units below the (body height) sphere centre: the trigger fires for feet in [-135, +75] around the floor
    under a door."""
    x, y, z = (round(c * WORLD_SCALE, 1) for c in pos)
    return [x, round(y - 100, 1), z]


# The game's pause menu "EXIT TO WITCH'S LAIR" (src/core2/gc/pauseMenu.c `D_8036C560`): lair lobby -> (world, lobby exit
# the player arrives at). Jumpman has no such menu entry, so the world gets a return pad on its entrance instead.
LOBBY_RETURNS = {
    'bk_gl_mm_lobby': ('bk_mumbos_mountain', 0x2), 'bk_gl_ttc_lobby': ('bk_treasure_trove_cove', 0x4),
    'bk_gl_cc_lobby': ('bk_clankers_cavern', 0x2), 'bk_gl_bgs_lobby': ('bk_bubblegloop_swamp', 0x2),
    'bk_gl_fp_lobby': ('bk_freezeezy_peak', 0x6), 'bk_gl_gv_lobby': ('bk_gobis_valley', 0x3),
    'bk_gl_ccw_lobby': ('bk_click_clock_wood', 0x6), 'bk_gl_rbb_lobby': ('bk_rusty_bucket_bay', 0x2),
    'bk_gl_mmm_lobby': ('bk_mad_monster_mansion', 0x2),
}
PAD_RADIUS = 90   # BK units of the return / mirror pads
# Sub-areas the game only enters from a script, a minigame or a cutscene: exported, reachable from the LEVELS tab only.
UNREACHABLE = {
    'bk_gl_furnace_fun': 'entered from the Furnace Fun minigame start (minigame.c), not from a trigger',
    'bk_gl_dingpot': 'entered from Furnace Fun / the final boss script',
    'bk_gl_battlements': 'entered from the final battle script',
    'bk_ttc_sharkfood_island': 'entered by the Sharkfood Island script (TTC/code_26D0.c); its exit uses a hard-coded position',
}


def enemy_param(actor):
    """param = actor id | (scale percent << 16); scale 0 means 1.0 (cube_spawnActors)."""
    scale = actor['scale'] if actor['scale'] else 100
    return actor['id'] | (min(scale, 1000) << 16)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('worlds', nargs='*')
    parser.add_argument('--rom', type=Path, default=ROOT / 'reference/roms/bk-us-1.0.z64')
    parser.add_argument('--out', type=Path, default=ROOT / 'assets/courses')
    args = parser.parse_args()
    rom = args.rom.read_bytes()
    if hashlib.md5(rom).hexdigest() != 'b29599651a13f681c9923d69354bf4a3':
        raise SystemExit('Expected a big-endian Banjo-Kazooie US 1.0 ROM')
    read = bk_banjo['assets'](rom)
    report = {}
    warp_report = {}
    plan = {}
    for wid, map_id in MAPS.items():
        nodes, props = parse_setup(read(map_id + 0x71C))
        plan[wid] = dict(map=map_id, actors_pickups=decode(nodes, props), transitions=decode_transitions(nodes), warps=[], skipped=[])
    course_of = {mid: cid for cid, mid in MAPS.items()}

    # ---- warps the setup files place (category-3 sphere triggers -> handler -> target map and exit)
    for wid, p in plan.items():
        warps, exits = p['transitions']
        for w in warps:
            h = WARP_HANDLERS.get(w['handler'])
            if not h:
                p['skipped'].append(dict(handler=w['handler'], pos=list(w['pos']), reason='handler is not a map change (orange pad, jiggy pad, cutscene or unused slot)'))
                continue
            target = course_of.get(h['map'])
            reason = None
            if target is None:
                reason = f"target map {h['map']:#x} is not exported ({bk_maps['SKIPPED_MAPS'].get(h['map'], 'no level model')})"
            elif h['exit'] in EXIT_SKIP_IDS or h['exit'] >= 0x80:
                reason = f"exit {h['exit']:#x}: {EXIT_SKIP_IDS.get(h['exit'], 'hard-coded exit position')}"
            elif h['exit'] not in plan[target]['transitions'][1]:
                reason = f"exit {h['exit']:#x} has no node in the target map"
            if reason:
                p['skipped'].append(dict(handler=w['handler'], fn=h['fn'], pos=list(w['pos']), reason=reason))
                continue
            p['warps'].append(dict(pos=w['pos'], radius=w['radius'], to=target, exit=h['exit'], fn=h['fn'], kind='setup'))

    def has_edge(a, b):
        return any(w['to'] == b for w in plan[a]['warps'])

    def nearest_exit(wid, pos):
        exits = plan[wid]['transitions'][1]
        return min((e for e in exits if e < 0x1E), key=lambda e: sum((exits[e]['pos'][i] - pos[i]) ** 2 for i in range(3)), default=None)

    # ---- leaving a world: the game's pause menu "EXIT TO WITCH'S LAIR" (pauseMenu.c `D_8036C560`) lands at a lair
    # lobby exit. Here it is a pad on the world's entrance: stepping back onto it returns to the lobby.
    for lobby, (world, lobby_exit) in LOBBY_RETURNS.items():
        if lobby not in plan or world not in plan:
            continue
        enter = next((w for w in plan[lobby]['warps'] if w['to'] == world), None)
        if enter is None or lobby_exit not in plan[lobby]['transitions'][1]:
            continue
        plan[world]['warps'].append(dict(pos=plan[world]['transitions'][1][enter['exit']]['pos'], radius=PAD_RADIUS, to=lobby, exit=lobby_exit,
                                         fn='pause menu: exit to the lair', kind='return'))

    # ---- one-way doors (entered by an actor or script in the game, e.g. Clanker's gills, Mr. Vile's nostrils, the
    # rain barrel) get the mirror trigger at the arrival point they lead back to.
    for a in list(plan):
        for w in list(plan[a]['warps']):
            b = w['to']
            if b == a or has_edge(b, a) or w['kind'] != 'setup':
                continue
            ex = plan[b]['transitions'][1].get(w['exit'])
            back = nearest_exit(a, w['pos'])
            if ex is None or back is None or bk_maps['group_of'](MAPS[a]) == 'GL' and bk_maps['group_of'](MAPS[b]) != 'GL':
                continue
            plan[b]['warps'].append(dict(pos=ex['pos'], radius=PAD_RADIUS, to=a, exit=back, fn=f"mirror of {w['fn']}", kind='mirror'))

    for wid, p in plan.items():
        if args.worlds and wid not in args.worlds:
            continue
        (actors, pickups) = p['actors_pickups']
        (_, exits) = p['transitions']
        path = args.out / f'{wid}.json'
        course = json.loads(path.read_text())
        course['objects'] = [o for o in course.get('objects', []) if o.get('behavior') != 'bkEnemy']
        placed = collections.Counter()
        for a in actors:
            key = ENEMIES.get(a['id'])
            if not key:
                continue
            placed[key] += 1
            pos = [round(c * WORLD_SCALE, 1) for c in a['pos']]
            course['objects'].append(dict(model='', pos=pos, yaw=a['yaw'], param=enemy_param(a), behavior='bkEnemy'))
        course['bkPickups'] = [[k, round(x * WORLD_SCALE, 1), round(y * WORLD_SCALE, 1), round(z * WORLD_SCALE, 1)] for k, x, y, z in pickups]

        out_warps = []
        for w in p['warps']:
            param = min(254, max(1, round(w['radius'] * WORLD_SCALE / 10)))
            out_warps.append(dict(pos=warp_object_pos(w['pos']), param=param, warpTo=w['to'], warpEntry=f"bk_exit_{w['exit']}", fn=w['fn'], kind=w['kind']))
        course['warps'] = out_warps
        spawns = {}
        for ex, e in sorted(exits.items()):
            pos = [round(c * WORLD_SCALE, 1) for c in e['pos']]
            yaw = e['yaw']
            # arrive outside every trigger (BK walks the player in from a distance): push along the facing direction
            for _ in range(40):
                inside = any(
                    (pos[0] - ow['pos'][0]) ** 2 + (pos[2] - ow['pos'][2]) ** 2 < (ow['param'] * 10 + 37) ** 2
                    and ow['pos'][1] - 185 <= pos[1] <= ow['pos'][1] + 50 for ow in out_warps)
                if not inside:
                    break
                pos[0] = round(pos[0] + 40 * math.sin(math.radians(yaw)), 1)
                pos[2] = round(pos[2] + 40 * math.cos(math.radians(yaw)), 1)
            spawns[f'bk_exit_{ex}'] = dict(pos=pos, faceYaw=yaw_units(yaw), behavior='')
        course['warpSpawns'] = spawns
        if bk_maps['is_subarea'](wid):
            first = next((ex for ex in sorted(exits) if ex < 0x1E), None)
            if first is not None:
                sp = spawns[f'bk_exit_{first}']
                course['spawn'] = dict(pos=sp['pos'], faceYaw=sp['faceYaw'])
        path.write_text(json.dumps(course, separators=(',', ':')))
        warp_report[wid] = dict(map=p['map'], warps=len(out_warps), exits=sorted(exits), skippedWarps=p['skipped'])
        every = collections.Counter(a['id'] for a in actors)
        kinds = collections.Counter(k for k, *_ in pickups)
        report[wid] = dict(
            map=p['map'],
            enemies={k: v for k, v in sorted(placed.items())},
            skippedEnemies={f'{i:#x} {SKIPPED_ENEMIES[i]}': c for i, c in sorted(every.items()) if i in SKIPPED_ENEMIES},
            pickups={['note', 'egg', 'redFeather', 'goldFeather', 'honeycomb', 'jiggy'][k]: v for k, v in sorted(kinds.items())},
            actorIds={f'{i:#x}': c for i, c in sorted(every.items())})
        print(wid, dict(placed), len(pickups), 'pickups', len(out_warps), 'warps', len(spawns), 'exits', len(p['skipped']), 'skipped')
    if not args.worlds:
        (args.out / 'bk_setup_report.json').write_text(json.dumps(
            dict(implemented={f'{i:#x}': k for i, k in sorted(ENEMIES.items())},
                 skipped={f'{i:#x}': v for i, v in sorted(SKIPPED_ENEMIES.items())}, worlds=report), indent=1))
        (args.out / 'bk_warp_report.json').write_text(json.dumps(
            dict(skippedMaps={f'{i:#x} {bk_maps["maps"]()[i]["name"]}': r for i, r in sorted(bk_maps['SKIPPED_MAPS'].items())},
                 unreachable=UNREACHABLE, courses=warp_report), indent=1))


if __name__ == '__main__':
    main()
