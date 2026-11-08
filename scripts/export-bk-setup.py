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
import re
import runpy
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
bk_banjo = runpy.run_path(str(ROOT / 'scripts/export-bk-banjo.py'))
WORLD_SCALE = 1.25

# world course id -> BK map id (enum map_e)
MAPS = {
    'bk_spiral_mountain': 0x01, 'bk_mumbos_mountain': 0x02, 'bk_treasure_trove_cove': 0x07, 'bk_clankers_cavern': 0x0B,
    'bk_bubblegloop_swamp': 0x0D, 'bk_gobis_valley': 0x12, 'bk_freezeezy_peak': 0x27, 'bk_rusty_bucket_bay': 0x31,
    'bk_mad_monster_mansion': 0x1B, 'bk_click_clock_wood': 0x40, 'bk_click_clock_wood_spring': 0x43,
    'bk_click_clock_wood_summer': 0x44, 'bk_click_clock_wood_autumn': 0x45, 'bk_click_clock_wood_winter': 0x46,
}

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
    for wid, map_id in MAPS.items():
        if args.worlds and wid not in args.worlds:
            continue
        nodes, props = parse_setup(read(map_id + 0x71C))
        actors, pickups = decode(nodes, props)
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
        path.write_text(json.dumps(course, separators=(',', ':')))
        every = collections.Counter(a['id'] for a in actors)
        kinds = collections.Counter(k for k, *_ in pickups)
        report[wid] = dict(
            map=map_id,
            enemies={k: v for k, v in sorted(placed.items())},
            skippedEnemies={f'{i:#x} {SKIPPED_ENEMIES[i]}': c for i, c in sorted(every.items()) if i in SKIPPED_ENEMIES},
            pickups={['note', 'egg', 'redFeather', 'goldFeather', 'honeycomb', 'jiggy'][k]: v for k, v in sorted(kinds.items())},
            actorIds={f'{i:#x}': c for i, c in sorted(every.items())})
        print(wid, dict(placed), len(pickups), 'pickups')
    if not args.worlds:
        (args.out / 'bk_setup_report.json').write_text(json.dumps(
            dict(implemented={f'{i:#x}': k for i, k in sorted(ENEMIES.items())},
                 skipped={f'{i:#x}': v for i, v in sorted(SKIPPED_ENEMIES.items())}, worlds=report), indent=1))


if __name__ == '__main__':
    main()
