#!/usr/bin/env python3
"""Super Mario 64 star completability audit -> native/SM64_STARS.md.

Star by star (120 power stars + the 3 cap switches): which decomp mechanism spawns it, whether the native engine has
that mechanism today, and what is missing otherwise.  Regenerate after every exporter / behavior change:

    python3 scripts/sm64-star-audit.py            # rewrite native/SM64_STARS.md
    python3 scripts/sm64-star-audit.py --check    # exit 1 when the file is stale (CI / pre-commit)

Inputs (nothing is hard-coded about the engine's current state):
  * assets/courses/*.json (+ the `.ext.json` world extensions): placed stars, red coins, secret stars, star boxes,
    chests, penguins, snowman, objects per area, coin supply.
  * native/sim/src/object.rs `behavior_kinds!` and native/sim/src/behaviors/mod.rs `register`: the behavior registry.
  * native/src/progress/stars.rs: the courses with a stand-in star location (`star_location`).
  * native/src/game/courses_table.rs: act names (`MISSIONS`); reference/sm64/text/us/courses.h when present, to flag
    names that differ from the decomp.
The decomp side (what spawns each star) is the STAR_TABLE below, written from levels/*/script.c (the OBJECT lines with
STAR_INDEX_ACT_n, the macro_box_star_act_n presets) and src/game/behaviors/*.inc.c (spawn_default_star callers); each row
cites its source.  Route names: placed (a bhvStar object), red (bhvHiddenRedCoinStar), box (exclamation box star),
secret (bhvHiddenStar / bhvCCMTouchedStarSpawn), snowman / penguin / chest (the TS ports of those behaviors), boss (a
boss dies and the act's star appears), race (Koopa the Quick), event (anything else), coins (100 coins).
"""
import json
import re
import string
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'native/SM64_STARS.md'
COURSES = ROOT / 'assets/courses'
REF = ROOT / 'reference/sm64'

# ---------------------------------------------------------------------------------------------------------------------
# decomp side: (act, mechanism, route, kinds the engine needs in the course, decomp behaviors the sim lacks)

T = {}
T['bob'] = [
    (1, 'bhvKingBobomb defeated -> spawn_default_star (king_bobomb.inc.c:218)', 'boss', ['kingBobomb'], []),
    (2, 'Koopa the Quick race won -> spawn_default_star (koopa.inc.c:741)', 'race', ['koopaTheQuick', 'koopaFlag'], []),
    (3, 'macro_box_star_act_3 on the sky island (bob/areas/1/macro.inc.c:87); the cannon is opened by the buddy', 'box', ['bobombBuddy'], []),
    (4, 'bhvHiddenRedCoinStar (bob/script.c:50)', 'red', [], []),
    (5, 'bhvHiddenStar + 5 macro_hidden_star_trigger (script.c:51, hidden_star.inc.c)', 'secret', [], []),
    (6, 'bhvStar behind the Chain Chomp gate (script.c:52)', 'placed', ['chainChomp', 'chainChompGate'], []),
]
T['wf'] = [
    (1, 'bhvWhompKingBoss defeated (wf/script.c:85)', 'boss', ['whompKing'], []),
    (2, 'bhvStar on top of the fortress (script.c:86)', 'placed', [], []),
    (3, 'bhvStar reached by the fortress cannon (script.c:87)', 'placed', [], []),
    (4, 'bhvHiddenRedCoinStar (script.c:88)', 'red', [], []),
    (5, 'bhvStar on the caged island (script.c:89)', 'placed', [], []),
    (6, 'bhvStar behind the breakable wall (script.c:90)', 'placed', ['blastWall'], []),
]
T['jrb'] = [
    (1, 'macro_box_star_act_1 in the sunken ship (jrb/areas/2/macro.inc.c:3)', 'box', [], []),
    (2, 'bhvUnagi: grab the eel\'s tail (script.c:32)', 'event', [], ['bhvUnagi']),
    (3, 'bhvTreasureChestsJRB: open the chests in order (treasure_chest.inc.c:169)', 'chest', ['chest'], []),
    (4, 'bhvHiddenRedCoinStar (script.c:96)', 'red', [], []),
    (5, 'bhvStar + star box on the stone pillar, cannon (script.c:98-100)', 'placed', [], []),
    (6, 'bhvStar at the jet stream end (script.c:102)', 'placed', [], []),
]
T['ccm'] = [
    (1, 'bhvCCMTouchedStarSpawn at the slide exit (ccm/script.c:34, spawn_star_exit.inc.c:8)', 'secret', [], []),
    (2, 'bhvTuxiesMother: bring the lost baby penguin (script.c:35, snowman/penguin.inc.c)', 'penguin', ['tuxiesMother', 'babyPenguin'], []),
    (3, 'bhvRacingPenguin race (script.c:43, racing_penguin.inc.c:151)', 'penguin', ['racingPenguin'], []),
    (4, 'bhvHiddenRedCoinStar (script.c:36)', 'red', [], []),
    (5, 'bhvSnowmansHead: roll the snowman body (script.c:37, snowman.inc.c:221)', 'snowman', ['snowmanBody', 'snowmanHead'], []),
    (6, 'bhvStar (script.c:38)', 'placed', [], []),
]
T['bbh'] = [
    (1, 'bhvGhostHuntBigBoo: kill the ghost-hunt Boos then the Big Boo (script.c:41, boo.inc.c)', 'boss', ['bigBoo', 'boo'], []),
    (2, 'bhvMerryGoRoundBooManager: 5 Boos then the merry-go-round Big Boo (script.c:56, boo.inc.c:746)', 'boss', [], ['bhvMerryGoRoundBooManager']),
    (3, 'bhvStar in the library, behind the bookshelf (script.c:73)', 'placed', [], []),
    (4, 'bhvHiddenRedCoinStar (script.c:74)', 'red', [], []),
    (5, 'bhvBalconyBigBoo (script.c:57)', 'boss', ['bigBoo'], []),
    (6, 'bhvMrI (script.c:75, mr_i.inc.c:137)', 'event', [], ['bhvMrI']),
]
T['hmc'] = [
    (1, 'bhvStar across the lake, Dorrie carries Mario (script.c:62)', 'placed', [], ['bhvDorrie']),
    (2, 'bhvHiddenRedCoinStar, elevators (script.c:63)', 'red', [], []),
    (3, 'bhvStar for metal Mario (script.c:64)', 'placed', [], []),
    (4, 'bhvStar in the toxic maze (script.c:65)', 'placed', ['poisonGas'], []),
    (5, 'bhvStar at the emergency exit (script.c:66)', 'placed', [], []),
    (6, 'bhvStar among the rolling rocks (script.c:67)', 'placed', [], ['bhvRollingRock']),
]
T['lll'] = [
    (1, 'bhvBigBully on the island, knock it into the lava (script.c:70, bully.inc.c:212)', 'boss', ['bigBully'], []),
    (2, 'bhvBigBullyWithMinions (script.c:71, bully.inc.c:214)', 'boss', ['bigBully', 'bully'], []),
    (3, 'bhvHiddenRedCoinStar (script.c:90)', 'red', [], []),
    (4, 'bhvStar over the rolling log (script.c:91)', 'placed', [], ['bhvLllRollingLog']),
    (5, 'bhvStar in the volcano (area 2 via the crater warp) (script.c:114)', 'placed', [], []),
    (6, 'bhvStar at the end of the volcano elevator tour (script.c:115)', 'placed', [], ['bhvLllMovingOctagonalMesh']),
]
T['ssl'] = [
    (1, 'bhvKlepto steals the star: chase and punch the Klepto (script.c:30, klepto.inc.c:367)', 'boss', ['klepto'], []),
    (2, 'bhvStar on top of the pyramid (script.c:36)', 'placed', [], []),
    (3, 'bhvStar high above the pyramid (script.c:60)', 'placed', [], []),
    (4, 'bhvEyerokBoss in the pyramid (script.c:66, eyerok.inc.c:122)', 'boss', [], ['bhvEyerokBoss']),
    (5, 'bhvHiddenRedCoinStar, flying (script.c:37)', 'red', [], []),
    (6, 'bhvHiddenStar + 5 macro_hidden_star_trigger in the pyramid (script.c:61; ssl/areas/2/macro.inc.c)', 'secret', [], []),
]
T['ddd'] = [
    (1, 'bhvStar on Bowser\'s submarine (script.c:56)', 'placed', ['bowserSub'], []),
    (2, 'bhvTreasureChestsDDD (script.c:30)', 'chest', ['chest'], []),
    (3, 'bhvHiddenRedCoinStar on the poles (script.c:57)', 'red', [], []),
    (4, 'bhvJetStreamRingSpawner: swim through the rings (script.c:58)', 'event', [], ['bhvJetStreamRingSpawner']),
    (5, 'bhvMantaRay: swim through its rings (script.c:31, manta_ray.inc.c:117)', 'event', [], ['bhvMantaRay']),
    (6, 'bhvStar for Mario with metal and vanish caps (script.c:59)', 'placed', [], []),
]
T['sl'] = [
    (1, 'bhvStar on the snowman\'s head (script.c:20)', 'placed', [], []),
    (2, 'bhvBigChillBully (script.c:35)', 'boss', [], ['bhvBigChillBully']),
    (3, 'bhvStar in the deep freeze (script.c:21)', 'placed', [], []),
    (4, 'macro_box_star_act_4 above the freezing pond (sl/areas/1/macro.inc.c:46)', 'box', [], []),
    (5, 'bhvHiddenRedCoinStar, shell (script.c:22)', 'red', [], []),
    (6, 'bhvStar in the igloo (area 2) (script.c:41)', 'placed', [], []),
]
T['wdw'] = [
    (1, 'macro_box_star_act_1 on the arrow lifts (wdw/areas/1/macro.inc.c:12)', 'box', [], []),
    (2, 'macro_box_star_act_2 on top of the town (macro.inc.c:61)', 'box', [], []),
    (3, 'bhvHiddenStar + 5 triggers in the shallows and sky (script.c:59)', 'secret', [], []),
    (4, 'bhvStar at the top of the express elevator (script.c:60)', 'placed', [], []),
    (5, 'bhvHiddenRedCoinStar in the town (area 2) (script.c:65)', 'red', [], []),
    (6, 'bhvStar at the end of the downtown race (script.c:66)', 'placed', [], []),
]
T['ttm'] = [
    (1, 'bhvStar at the summit (ACT_1 only) (script.c:45)', 'placed', [], []),
    (2, 'bhvUkikiCage: Ukiki returns the hat, Mario opens the cage (script.c:46)', 'event', [], ['bhvUkiki', 'bhvUkikiCage']),
    (3, 'bhvHiddenRedCoinStar among the mushrooms (script.c:47)', 'red', [], []),
    (4, 'bhvStar at the end of the slide (script.c:48)', 'placed', [], []),
    (5, 'bhvStar on the bridge (script.c:49)', 'placed', [], []),
    (6, 'bhvStar on the lonely mushroom, cannon (script.c:50)', 'placed', [], []),
]
T['thi'] = [
    (1, 'bhvFirePiranhaPlant x5: pluck the flower (script.c:38-42, fire_piranha_plant.inc.c:72)', 'event', [], ['bhvFirePiranhaPlant']),
    (2, 'macro_box_star_act_2 on top of the huge island (thi/areas/1/macro.inc.c:36)', 'box', [], []),
    (3, 'Koopa the Quick race, KOOPA_THE_QUICK_THI (script.c:34, koopa.inc.c:741)', 'race', ['koopaTheQuick'], []),
    (4, 'bhvHiddenStar + 5 triggers in the tiny island (script.c:23; thi/areas/2/macro.inc.c)', 'secret', [], []),
    (5, 'bhvHiddenRedCoinStar, Wiggler\'s cave (area 3) (script.c:28)', 'red', [], []),
    (6, 'bhvWigglerHead (script.c:29)', 'boss', [], ['bhvWigglerHead']),
]
T['ttc'] = [
    (1, 'bhvStar in the cage, roll in (script.c:25)', 'placed', [], []),
    (2, 'bhvStar in the pit and pendulums (script.c:26)', 'placed', [], []),
    (3, 'bhvStar, get a hand (script.c:27)', 'placed', [], []),
    (4, 'bhvStar, stomp on the thwomp (script.c:28)', 'placed', [], []),
    (5, 'bhvStar, timed jumps on moving bars (script.c:29)', 'placed', [], []),
    (6, 'bhvHiddenRedCoinStar, stop time (script.c:30)', 'red', [], []),
]
T['rr'] = [
    (1, 'bhvStar, cruiser crossing the rainbow (script.c:63)', 'placed', [], []),
    (2, 'bhvStar, the big house in the sky (script.c:64)', 'placed', [], []),
    (3, 'bhvHiddenRedCoinStar (script.c:65)', 'red', [], []),
    (4, 'bhvStar, swingin\' in the breeze (script.c:66)', 'placed', [], []),
    (5, 'bhvStar, tricky triangles (script.c:67)', 'placed', [], []),
    (6, 'macro_box_star_act_6, somewhere over the rainbow (rr/areas/1/macro.inc.c:62)', 'box', [], []),
]
# Secret / Bowser / cap courses: one star each.
SECRET = [
    ('pss', 1, 'macro_box_star_act_1 at the end of the slide (pss/areas/1/macro.inc.c:43)', 'box', [], []),
    ('pss', 2, 'slide time < 630 frames: pss_end_slide -> spawn_default_star (interaction.c:1846; SURFACE_TIMER_START/END)', 'event', [], ['SURFACE_TIMER_START', 'SURFACE_TIMER_END']),
    ('sa', 1, 'bhvHiddenRedCoinStar (sa/script.c:25)', 'red', [], []),
    ('totwc', 1, 'bhvHiddenRedCoinStar (totwc/script.c:25)', 'red', [], []),
    ('cotmc', 1, 'bhvHiddenRedCoinStar (cotmc/script.c:27)', 'red', [], []),
    ('vcutm', 1, 'bhvHiddenRedCoinStar (vcutm/script.c:36)', 'red', [], []),
    ('wmotr', 1, 'bhvHiddenRedCoinStar (wmotr/script.c:29)', 'red', [], []),
    ('bitdw', 1, 'bhvBowserCourseRedCoinStar (bitdw/script.c:42)', 'red', [], []),
    ('bitfs', 1, 'bhvBowserCourseRedCoinStar (bitfs/script.c:49)', 'red', [], []),
    ('bits', 1, 'bhvBowserCourseRedCoinStar (bits/script.c:42)', 'red', [], []),
]
# Castle stars: bhvToadMessage x3 and bhvMips x2 (castle_inside/script.c:260-296, mips.inc.c, toad.inc.c).
CASTLE = [
    ('Toad 1 (main hall, 12 stars)', 'DIALOG_082 bhvToadMessage', 12),
    ('Toad 2 (upstairs, 25 stars)', 'DIALOG_083 bhvToadMessage', 25),
    ('Toad 3 (basement stairs, 35 stars)', 'DIALOG_084 bhvToadMessage', 35),
    ('MIPS 1 (basement, 15 stars)', 'bhvMips MIPS_BP_15_STARS', 15),
    ('MIPS 2 (basement, 50 stars)', 'bhvMips MIPS_BP_50_STARS', 50),
]
MAIN = list(T)
SECRET_NAMES = {
    ('pss', 1): 'The Princess\u2019s Secret Slide: reach the end', ('pss', 2): 'The Princess\u2019s Secret Slide: under 21 seconds',
    ('sa', 1): 'The Secret Aquarium', ('totwc', 1): 'Tower of the Wing Cap', ('cotmc', 1): 'Cavern of the Metal Cap',
    ('vcutm', 1): 'Vanish Cap under the Moat', ('wmotr', 1): 'Wing Mario over the Rainbow', ('bitdw', 1): 'Bowser in the Dark World',
    ('bitfs', 1): 'Bowser in the Fire Sea', ('bits', 1): 'Bowser in the Sky',
}
# decomp object behavior -> sim kind, for the "behaviors not ported" lists
BHV_KIND = {
    'bhvKingBobomb': 'kingBobomb', 'bhvBobomb': 'bobomb', 'bhvBobombBuddy': 'bobombBuddy', 'bhvBobombBuddyOpensCannon': 'bobombBuddy',
    'bhvChainChomp': 'chainChomp', 'bhvChainChompGate': 'chainChompGate', 'bhvWhompKingBoss': 'whompKing', 'bhvSmallWhomp': 'whomp',
    'bhvGhostHuntBigBoo': 'bigBoo', 'bhvBalconyBigBoo': 'bigBoo', 'bhvBoo': 'boo', 'bhvGhostHuntBoo': 'boo', 'bhvTuxiesMother': 'tuxiesMother',
    'bhvSmallPenguin': 'babyPenguin', 'bhvRacingPenguin': 'racingPenguin', 'bhvSnowmansBottom': 'snowmanBody', 'bhvSnowmansHead': 'snowmanHead',
    'bhvMrBlizzard': 'mrBlizzard', 'bhvKlepto': 'klepto', 'bhvBigBully': 'bigBully', 'bhvBigBullyWithMinions': 'bigBully',
    'bhvSmallBully': 'bully', 'bhvFlame': 'flame', 'bhvPokey': 'pokey', 'bhvKoopa': 'koopa', 'bhvKoopaRaceEndpoint': 'koopaFlag',
    'bhvTreasureChestsJRB': 'chest', 'bhvTreasureChestsDDD': 'chest', 'bhv1Up': 'oneUp', 'bhvBowsersSub': 'bowserSub',
    'bhvWFBreakableWallLeft': 'blastWall', 'bhvWFBreakableWallRight': 'blastWall', 'bhvStar': 'star', 'bhvHiddenRedCoinStar': 'redCoinStar',
    'bhvHiddenStar': 'secret', 'bhvBowserCourseRedCoinStar': 'redCoinStar', 'bhvCCMTouchedStarSpawn': 'secret',
    'bhvBowser': 'bowser', 'bhvBowserBomb': 'bowserMine', 'bhvCannonClosed': 'cannon', 'bhvWaterBombCannon': 'waterBombCannon',
    'bhvBreakableBox': 'breakableBox', 'bhvCapSwitch': 'capSwitch', 'bhvWarp': 'warp', 'bhvWarpPipe': 'warp', 'bhvFadingWarp': 'warp',
    'bhvDoorWarp': 'door', 'bhvStarDoor': 'door', 'bhvStaticObject': 'static', 'bhvPitBowlingBall': 'pitBowlingBall',
    'bhvBoBBowlingBallSpawner': 'mountainBallSpawner', 'bhvTTMBowlingBallSpawner': 'mountainBallSpawner', 'bhvFloorSwitchGrills': 'purpleSwitch',
    'bhvOpenableGrill': 'openableGrill', 'bhvMetalCap': 'wingBox', 'bhvTree': 'tree', 'bhvCheckerboardElevatorGroup': 'platform',
    'bhvRotatingPlatform': 'platform', 'bhvPlatformOnTrack': 'platform', 'bhvTowerPlatformGroup': 'platform',
}
# Harmless markers that never need a behavior (sounds, spawn markers, rooms, water mist, ...).
IGNORED = re.compile(r'Sound|Ambient|Warp|bhvMario|Mist|Splash|Waterfall|Wave|Fish|Bird|Butterfly|Light|Steam|Shadow|Spawner$|Static|Door|Room|Dust|Bubble|Cloud|Grass|Flower', re.I)

NAMING_FIXES = {}


# ---------------------------------------------------------------------------------------------------------------------
# engine side


def merge(base, ext):
    for k, v in ext.items():
        if isinstance(v, list) and isinstance(base.get(k), list):
            base[k] = base[k] + [x for x in v if x not in base[k]]
        elif isinstance(v, dict) and isinstance(base.get(k), dict):
            base[k] = {**base[k], **v}
        else:
            base[k] = v
    return base


def load(course, area=1):
    stem = course if area == 1 else f'{course}.{area}'
    path = COURSES / f'{stem}.json'
    if not path.exists():
        return None
    data = json.loads(path.read_text())
    ext = COURSES / f'{stem}.ext.json'
    if ext.exists():
        merge(data, json.loads(ext.read_text()))
    return data


def areas_of(course):
    out = []
    for a in range(1, 6):
        d = load(course, a)
        if d is not None:
            out.append(d)
    return out


def registry():
    src = (ROOT / 'native/sim/src/object.rs').read_text()
    block = src[src.index('behavior_kinds! {'):]
    block = block[:block.index('\n}\n')]
    kinds = dict(re.findall(r'(\w+)\s*=>\s*"(\w+)"', block))
    # a kind has behavior code when it is wired in behaviors/mod.rs (`register` / `set_update`) or inline in the table
    om = (ROOT / 'native/sim/src/object_manager.rs').read_text()
    registered = set(re.findall(r'K::(\w+)', (ROOT / 'native/sim/src/behaviors/mod.rs').read_text()))
    registered |= set(re.findall(r'(?:K|ObjectBehaviorKind)::(\w+)', om[om.index('impl BehaviorTable'):][:6000]))
    return {name for variant, name in kinds.items()}, {kinds[v] for v in registered if v in kinds}


def stand_in_courses():
    src = (ROOT / 'native/src/progress/stars.rs').read_text()
    body = src[src.index('fn star_location'):]
    body = body[:body.index('\n}\n')]
    out = set()
    for arm in re.findall(r'((?:"\w+"\s*\|?\s*)+)=>', body):
        out.update(re.findall(r'"(\w+)"', arm))
    return out


def missions():
    src = (TABLE := ROOT / 'native/src/game/courses_table.rs').read_text()
    block = src[src.index('pub const MISSIONS'):]
    out = {}
    for course, body in re.findall(r'\("(\w+)",\s*&\[(.*?)\]\)', block, re.S):
        out[course] = {int(a): n for a, n in re.findall(r'\((\d+),\s*"([^"]*)"\)', body)}
    return out


def decomp_names():
    path = REF / 'text/us/courses.h'
    if not path.exists():
        return {}
    text = path.read_text()
    out = {}
    for course, body in re.findall(r'COURSE_ACTS\(COURSE_(\w+),\s*_\("[^"]*"\),(.*?)\)\n', text, re.S):
        names = re.findall(r'_\("([^"]*)"\)', body)
        out[course.lower()] = {i + 1: n for i, n in enumerate(names)}
    return out


def norm(s):
    return re.sub(r'[^a-z0-9]+', ' ', s.lower().replace('’', "'").replace("'", '')).strip()


class Engine:
    def __init__(self):
        self.kinds, self.registered = registry()
        self.stand_in = stand_in_courses()
        self.names = missions()
        self.decomp = decomp_names()

    def objects(self, course):
        return [o for d in areas_of(course) for o in d.get('objects', [])]

    def has_kind(self, course, kind):
        if kind == 'bowserSub':  # Bowser's submarine is an exported moving collision object, not a sim behavior
            return None if any(o.get('submarine') for o in self.objects(course)) else 'submarine not exported'
        if kind not in self.kinds:
            return f'{kind} is not a sim behavior'
        if kind not in self.registered:
            return f'{kind} has no registered behavior'
        if kind == 'poisonGas':
            return None  # spawned from the course's gas macros by the sim itself
        if kind == 'babyPenguin' or kind == 'snowmanHead':
            pass
        if any(o.get('behavior') == kind for o in self.objects(course)):
            return None
        if kind in ('chest',) and any(d.get('chestStar') for d in areas_of(course)):
            return None
        return f'{kind} not placed in the course export'

    def route(self, course, act, route):
        """(ok, note) for the engine route of a star."""
        data = areas_of(course)
        if route == 'placed':
            for d in data:
                for p in d.get('placedStars', []):
                    if p['act'] == act and (not p.get('acts') or act in p['acts']):
                        return True, 'placed star exported'
            return False, 'bhvStar not exported for this act'
        if route == 'red':
            for d in data:
                if d.get('redCoinStar') and d.get('redCoinAct', 1) == act:
                    n = len(d.get('redCoins', []))
                    return (n >= 8, f'{n} of 8 red coins exported')
            return False, 'red coin star not exported for this act'
        if route == 'box':
            for o in self.objects(course):
                if o.get('behavior') == 'starBox' and int(o.get('param') or 0) == act:
                    return True, 'star box exported'
            return False, f'macro_box_star_act_{act} not exported'
        if route == 'secret':
            for d in data:
                if d.get('secretStar') and d['secretStar']['act'] == act:
                    n = len(d['secretStar'].get('triggers', []))
                    if course == 'ccm':
                        return True, 'slide-exit star exported'
                    return (n >= 1, f'{n} secret triggers exported')
            return False, 'hidden star not exported for this act'
        if route in ('snowman', 'penguin', 'chest'):
            key = {'snowman': 'snowmanStar', 'penguin': 'penguinStar', 'chest': 'chestStar'}[route]
            for d in data:
                if d.get(key) and d[key]['act'] == act:
                    return True, f'{key} exported'
            return False, f'{key} not exported for this act'
        if route == 'boss':
            for d in data:
                for o in d.get('objects', []):
                    if o.get('behavior') in ('kingBobomb', 'bigBully', 'whompKing', 'bigBoo', 'klepto') and (not o.get('acts') or act in o['acts']):
                        return True, 'boss placed for this act'
            return False, 'boss not placed for this act'
        if route == 'race':
            # `race_won` only pays out for bob act 2 (stars.rs); THI's race object is not exported.
            if course == 'bob' and act == 2:
                return True, 'race star wired'
            return False, 'race star wired for bob act 2 only'
        return False, 'no engine route'

    def coin_supply(self, course):
        """Static coins of the engine export, by source (a coin star also takes enemy / box drops on top)."""
        out = dict(yellow=0, formation=0, red=0, blue=0, box=0)
        for d in areas_of(course):
            out['yellow'] += len(d.get('coins', []))
            out['formation'] += sum(5 if f['shape'].startswith('line_') else 8 for f in d.get('coinFormations', []))
            out['red'] += 2 * len(d.get('redCoins', []))
            for o in d.get('objects', []):
                if o.get('behavior') == 'breakableBox':
                    out['box'] += {1: 3, 2: 5}.get(int(o.get('param') or 0), 0)
        return out

    def decomp_coin_supply(self, course):
        """The same, counted from the decomp macro presets (levels/<course>/areas/*/macro.inc.c)."""
        out = dict(yellow=0, formation=0, red=0, blue=0, box=0)
        for mf in sorted((REF / 'levels' / course / 'areas').glob('*/macro.inc.c')):
            for name in re.findall(r'\bmacro_(\w+)', re.sub(r'/\*.*?\*/|//[^\n]*', '', mf.read_text(), flags=re.S)):
                if name in ('yellow_coin_1', 'yellow_coin_2'):
                    out['yellow'] += 1
                elif name.startswith('coin_line'):
                    out['formation'] += 5
                elif name.startswith('coin_ring') or name.startswith('coin_arrow'):
                    out['formation'] += 8
                elif name == 'red_coin':
                    out['red'] += 2
                elif name in ('hidden_blue_coin', 'moving_blue_coin'):
                    out['blue'] += 5
                elif name == 'box_three_coins' or name == 'breakable_box_three_coins':
                    out['box'] += 3
                elif name == 'box_ten_coins':
                    out['box'] += 10
        return out

    def stand_in_ok(self, course):
        return course in self.stand_in


def audit():
    e = Engine()
    rows = []
    for course in MAIN:
        for act, how, route, kinds, missing in T[course]:
            rows.append(star_row(e, course, act, how, route, kinds, missing))
        have, want = e.coin_supply(course), e.decomp_coin_supply(course)
        missing = [f"{k} coins {want[k] - have[k]} short ({have[k]} of {want[k]})" for k in want if have[k] < want[k]]
        ok = not missing
        rows.append(dict(course=course, act=7, name='100 coins', how='numCoins >= 100 -> bhv_spawn_star_no_level_exit (interaction.c:750); coins come from macros, boxes, enemies',
                         route='coins', status='completable' if ok else 'blocked',
                         why='; '.join(missing), engine=f'static coins {sum(have.values())} of {sum(want.values())} (decomp), plus enemy drops',
                         standin=False))
    for course, act, how, route, kinds, missing in SECRET:
        rows.append(star_row(e, course, act, how, route, kinds, missing))
    return e, rows


def star_row(e, course, act, how, route, kinds, missing):
    name = SECRET_NAMES.get((course, act)) or e.names.get(course, {}).get(act) or e.decomp.get(course, {}).get(act) or course
    if name.isupper():
        name = string.capwords(name.lower())
    ok, route_note = e.route(course, act, route)
    problems = []
    if course == 'pss' and act == 1:
        # The star at the end of the slide is shown without the exclamation box (stars.rs `star_location("pss")`).
        ok, route_note = True, 'star shown at the box position (no box)'
    if not ok and not (missing and (route_note == 'no engine route' or route in ('boss', 'event'))):
        problems.append(route_note)
    for k in kinds:
        why = e.has_kind(course, k)
        if why:
            problems.append(why)
    for m in missing:
        problems.append(f'{m} (decomp behavior without a sim port)')
    standin = e.stand_in_ok(course) and not (ok and not problems)
    return dict(course=course, act=act, name=name, how=how, route=route, status='completable' if not problems else 'blocked',
                why='; '.join(problems), engine=route_note, standin=standin)


# ---------------------------------------------------------------------------------------------------------------------
# bookkeeping checks (source greps; each names the Rust test that exercises it)


def grep(path, pattern):
    p = ROOT / path
    return p.exists() and re.search(pattern, p.read_text()) is not None


def bookkeeping():
    rs = 'native/src/progress'
    rows = [
        ('Star count (HUD / doors)', grep(f'{rs}/save.rs', r'pub fn stars\(&self\)'), 'save.rs `stars()` counts every course:act key except castle_grounds, castle_inside, bowser_3, playground, mushroom2d', 'progress::save::tests'),
        ('Star-select act availability (6 acts + 100-coin star)', grep(f'{rs}/menus.rs', r'fn act_choices'), 'menus.rs `act_choices`: collected acts are marked, the next open one is preselected', 'progress::menus::tests'),
        ('Course complete flow (dance, result, save prompt)', grep(f'{rs}/flow.rs', r'fn finish_reward') and grep(f'{rs}/flow.rs', r'fn step_dance'), 'flow.rs `finish_reward` -> `begin_dance` -> `step_dance` -> result', 'progress::flow::tests::collecting_a_star_dances_wipes_and_returns_to_the_castle_with_a_result'),
        ('Exit target after a star (castle, grounds, courtyard)', grep(f'{rs}/flow.rs', r'success_course'), '`successCourse` / `successEntry` / `successArea` of the course json', 'game::sm64_warp_tests::every_arrival_exists_and_every_special_target_is_wired'),
        ('Coin high score per course', grep(f'{rs}/save.rs', r'best_coins') and grep(f'{rs}/save.rs', r'high_score'), 'save.rs `collect_star` keeps the best coin count of the 15 score courses (low byte, like the original)', 'progress::save::tests::stars_and_high_scores'),
        ('100-coin star saved as act 7', grep(f'{rs}/stars.rs', r'HUNDRED_COIN_ACT'), 'stars.rs `hundred_coins` -> `Collected{reward: None, act: 7}` -> `collect_star(course, 7)`', 'progress::flow::tests::hundred_coin_star_prompts_to_save'),
        ('Star doors 1 / 3 / 8 / 30 / 50 / 70', grep(f'{rs}/doors.rs', r'fn unlock_flag') and grep(f'{rs}/doors.rs', r'8 => 1 << 14'), 'doors.rs `unlock_flag` (get_door_save_file_flag) + `RoomDoors::step`; the 70-star door is the instant-warp lockout in flow.rs', 'progress::sm64_progress_tests'),
        ('Key doors (basement / upstairs) and the Bowser keys', grep(f'{rs}/save.rs', r'fn unlock_door') and grep(f'{rs}/stars.rs', r'reward_key'), 'save.rs `collect_key` / `unlock_door`; bowser_1 / bowser_2 drop the keys', 'progress::sm64_progress_tests'),
        ('Cap switches persist (wing / metal / vanish)', grep(f'{rs}/save.rs', r'FLAG_VANISH_SWITCH') and grep(f'{rs}/save.rs', r'fn press_cap_switch'), 'save.rs `FLAG_WING_SWITCH` 2, `FLAG_METAL_SWITCH` 4, `FLAG_VANISH_SWITCH` 8 (SAVE_FLAG_HAVE_*_CAP)', 'progress::sm64_progress_tests'),
        ('Cap boxes open with their switch', grep('native/src/progress/flow.rs', r'cap_switch_on\(o\.param'), 'flow.rs `set_wing_boxes`: wingBox param 0 / 1 / 2 follows the matching save flag', 'progress::sm64_progress_tests'),
        ('Warps into a level need stars (courtyard cage: 12)', grep(f'{rs}/flow.rs', r'warp_gates'), 'ProgressData.warp_gates, checked in `Progress::begin_warp`', 'progress::sm64_progress_tests'),
        ('Moat drained by the two water-level pillars', grep(f'{rs}/save.rs', r'FLAG_MOAT_DRAINED') and grep('native/src/progress/moat.rs', r'fn '), 'save.rs `FLAG_MOAT_DRAINED` 0x200 (SAVE_FLAG_MOAT_DRAINED); progress/moat.rs', 'progress::moat::tests'),
        ('Toad / MIPS castle stars', False, 'no bhvToadMessage / bhvMips: `castle_inside` stars are excluded from the star count and never spawn', ''),
        ('Pyramid / volcano / tiny THI / TTM slide / igloo areas', all(load(c, a) for c, a in (('lll', 2), ('ssl', 2), ('ssl', 3), ('sl', 2), ('thi', 2), ('thi', 3), ('ttm', 2), ('ttm', 3), ('ttm', 4), ('wdw', 2))), 'assets/courses/<id>.<area>.json via scripts/sm64_world.py; warps / pipes / instant warps / painting-warp floors lead into them', 'game::sm64_warp_tests'),
    ]
    return rows


# ---------------------------------------------------------------------------------------------------------------------
# course-level lists


def decomp_behaviors(course):
    """Object behaviors the decomp places in the level script and macros that have no sim port."""
    script = (REF / 'levels' / course / 'script.c')
    if not script.exists():
        return {}
    text = re.sub(r'/\*.*?\*/|//[^\n]*', '', script.read_text(), flags=re.S)
    counts = {}
    for m in re.finditer(r'OBJECT(?:_WITH_ACTS)?\s*\(([^()]*(?:\([^()]*\)[^()]*)*)\)', text):
        parts = [x.strip() for x in re.split(r',(?![^()]*\))', m.group(1))]
        bhv = parts[8] if len(parts) > 8 else ''
        if bhv.startswith('bhv') and not IGNORED.search(bhv):
            counts[bhv] = counts.get(bhv, 0) + 1
    return counts


def unported(e, course):
    out = {}
    for bhv, n in decomp_behaviors(course).items():
        kind = BHV_KIND.get(bhv)
        if kind is None or (kind in e.kinds and kind not in e.registered):
            out[bhv] = n
    return out


# ---------------------------------------------------------------------------------------------------------------------


def render():
    e, rows = audit()
    lines = []
    w = lines.append
    w('# Super Mario 64 star completability')
    w('')
    w('Generated by `scripts/sm64-star-audit.py` from the exported course JSON (`assets/courses`), the sim behavior registry '
      '(`native/sim/src/object.rs`, `behaviors/mod.rs`) and the star director (`native/src/progress/stars.rs`). '
      'Do not edit by hand: re-run the script after changing an exporter or a behavior (`--check` fails when the file is stale).')
    w('')
    done = [r for r in rows if r['status'] == 'completable']
    blocked = [r for r in rows if r['status'] == 'blocked']
    stars = [r for r in rows]
    w(f'* **{len(done)} of {len(stars) + len(CASTLE)}** stars have their real decomp mechanism in the engine today '
      f'(power stars: {len(stars)} audited rows + {len(CASTLE)} castle stars = {len(stars) + len(CASTLE)}; the game has 120).')
    w(f'* **{len(blocked)}** audited stars are blocked by a missing mechanism (below), and the {len(CASTLE)} castle stars (Toad, MIPS) are blocked by missing NPC behaviors.')
    w('* "stand-in" = the engine still lets that act finish through the course\'s generic goal star (`star_location` in `stars.rs`), '
      'so the star count and saves work, but the decomp objective is not what unlocks it.')
    w('* 3 cap switches: wing (Tower of the Wing Cap), metal (Cavern of the Metal Cap), vanish (Vanish Cap under the Moat) all exist as '
      'ground switches that set `SAVE_FLAG_HAVE_*_CAP` and unlock the matching ! boxes (see the table at the end).')
    w('* "completable" means the star exists in the engine with the decomp mechanism wired and every behavior the audit knows it needs is present; '
      'it does not prove that every platform on the way is traversable (see the per-course lists of decomp behaviors without a sim port).')
    w('')
    blockers = {}
    for r in blocked:
        for part in r['why'].split('; '):
            if part:
                key = re.sub(r'^.*?(bhv\w+|SURFACE_\w+).*$', r'\1', part) if re.search(r'(bhv\w+|SURFACE_\w+)', part) else part
                if ' short' in key:
                    key = re.sub(r'\s*\d+$', '', key.split(' short')[0]) + ' not exported (100-coin supply)'
                blockers.setdefault(key, []).append(f"{r['course']}:{r['act']}")
    w('## What blocks the stars (steer the behavior ports with this)')
    w('')
    w('| Blocker | Stars |')
    w('| --- | --- |')
    for key, who in sorted(blockers.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        w(f"| {key} | {len(who)}: {', '.join(who)} |")
    w('')
    for course in MAIN + [c for c, *_ in SECRET if c not in MAIN]:
        cr = [r for r in rows if r['course'] == course]
        if not cr or (course in [c for c, *_ in SECRET] and cr[0]['act'] == 2 and False):
            continue
        title = (e.names and None) or course
        w(f'## {course}')
        w('')
        w('| Act | Star | Decomp mechanism | Engine today | Status |')
        w('| --- | --- | --- | --- | --- |')
        for r in cr:
            status = 'completable' if r['status'] == 'completable' else 'blocked by ' + r['why']
            if r['status'] == 'blocked' and r['standin']:
                status += ' (stand-in star keeps the act finishable)'
            w(f"| {r['act']} | {r['name']} | {r['how']} | {r['engine']} | {status} |")
        gaps = unported(e, course)
        if gaps:
            w('')
            w('Decomp objects in the level script without a sim behavior: ' + ', '.join(f'`{b}` x{n}' for b, n in sorted(gaps.items())) + '.')
        w('')
    w('## Castle stars')
    w('')
    w('| Star | Decomp mechanism | Status |')
    w('| --- | --- | --- |')
    for name, how, need in CASTLE:
        w(f'| {name} | {how}: talk when you own {need} stars -> spawn_default_star | blocked by bhvToadMessage / bhvMips (no NPC, no dialog trigger) |')
    w('')
    w('## Cap switches')
    w('')
    w('| Switch | Level | Status |')
    w('| --- | --- | --- |')
    for name, level, flag in (('Wing cap (red)', 'totwc', 'FLAG_WING_SWITCH 2'), ('Metal cap (green)', 'cotmc', 'FLAG_METAL_SWITCH 4'), ('Vanish cap (blue)', 'vcutm', 'FLAG_VANISH_SWITCH 8')):
        d = load(level)
        ok = bool(d and d.get('wingSwitch'))
        w(f"| {name} | {level} | {'completable: switch collider + ' + flag + ', unlocks the ! boxes' if ok else 'blocked: switch not exported'} |")
    w('')
    w('## Bookkeeping')
    w('')
    w('| Mechanism | Status | Where | Test |')
    w('| --- | --- | --- | --- |')
    for name, ok, where, test in bookkeeping():
        w(f"| {name} | {'present' if ok else 'missing'} | {where} | {test or '-'} |")
    w('')
    mism = []
    for course, acts in e.names.items():
        for act, n in acts.items():
            d = e.decomp.get(course, {}).get(act)
            if d and norm(d) != norm(n):
                mism.append(f'`{course}` act {act}: engine "{n}" vs decomp "{d}"')
    if mism:
        w('## Act names that differ from the decomp text')
        w('')
        for m in mism:
            w(f'* {m}')
        w('')
    return '\n'.join(lines) + '\n'


def main():
    text = render()
    if '--check' in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            print(f'{OUT} is stale: run python3 scripts/sm64-star-audit.py')
            return 1
        return 0
    OUT.write_text(text)
    print(f'wrote {OUT}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
