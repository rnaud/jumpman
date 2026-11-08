"""Shared map registry of the Banjo-Kazooie exporters: which BK maps become Jumpman courses, and under which ids.

`export-bk-levels.py` (geometry + collision), `export-bk-setup.py` (placements, warps, exits) and the Rust tests all
agree on this table. The raw data comes from scripts/data/bk-catalogue.json (written by scripts/bk-catalogue.py from
the decomp: `sMapModelDescriptions`, `sWarpFunctions`, the `WARP_*` enums).

* The 14 main world maps keep their original course ids (`bk_spiral_mountain`, ...).
* Every other map with a level model is a *sub-area* `bk_<map name>` (`bk_ttc_blubbers_ship`, `bk_gl_mm_lobby`, ...),
  reachable through the warps (doors, pipes, windows, the lair's world doors) and from the LEVELS tab's collapsed
  "sub-areas" list.
* Skipped maps: the cutscene / intro / ending / file-select maps (their models are the world models with scripted
  cameras) and nothing else; the reasons are kept in SKIPPED_MAPS for the world-graph test and the report.
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CATALOGUE = json.loads((ROOT / 'scripts/data/bk-catalogue.json').read_text())

# course id -> BK map id (enum map_e)
WORLD_MAPS = {
    'bk_spiral_mountain': 0x01, 'bk_mumbos_mountain': 0x02, 'bk_treasure_trove_cove': 0x07, 'bk_clankers_cavern': 0x0B,
    'bk_bubblegloop_swamp': 0x0D, 'bk_gobis_valley': 0x12, 'bk_freezeezy_peak': 0x27, 'bk_rusty_bucket_bay': 0x31,
    'bk_mad_monster_mansion': 0x1B, 'bk_click_clock_wood': 0x40, 'bk_click_clock_wood_spring': 0x43,
    'bk_click_clock_wood_summer': 0x44, 'bk_click_clock_wood_autumn': 0x45, 'bk_click_clock_wood_winter': 0x46,
}

CUTSCENE_REASON = 'cutscene / intro / ending map (scripted camera, the world model re-used; no gameplay)'
SKIPPED_MAPS = {
    0x1E: 'Nintendo logo intro', 0x1F: 'Rareware logo intro', 0x20: 'ending cutscene (beach)',
    0x7B: CUTSCENE_REASON, 0x7C: CUTSCENE_REASON, 0x7D: CUTSCENE_REASON, 0x7E: CUTSCENE_REASON,
    0x81: CUTSCENE_REASON, 0x82: CUTSCENE_REASON, 0x83: CUTSCENE_REASON, 0x84: CUTSCENE_REASON,
    0x85: CUTSCENE_REASON, 0x86: CUTSCENE_REASON, 0x87: CUTSCENE_REASON, 0x88: CUTSCENE_REASON,
    0x89: CUTSCENE_REASON, 0x8A: CUTSCENE_REASON, 0x91: 'file select screen', 0x94: CUTSCENE_REASON,
    0x95: CUTSCENE_REASON, 0x96: CUTSCENE_REASON, 0x97: CUTSCENE_REASON, 0x98: CUTSCENE_REASON, 0x99: CUTSCENE_REASON,
}

WORLD_NAMES = {
    'SM': 'Spiral Mountain', 'MM': 'Mumbo’s Mountain', 'TTC': 'Treasure Trove Cove', 'CC': 'Clanker’s Cavern',
    'BGS': 'Bubblegloop Swamp', 'GV': 'Gobi’s Valley', 'MMM': 'Mad Monster Mansion', 'FP': 'Freezeezy Peak',
    'RBB': 'Rusty Bucket Bay', 'CCW': 'Click Clock Wood', 'GL': 'Gruntilda’s Lair',
}
WORD_FIXES = {
    'Nippers': 'Nipper’s', 'Blubbers': 'Blubber’s', 'Tumblars': 'Tumblar’s', 'Nappers': 'Napper’s', 'Sandybutts': 'Sandybutt’s',
    'Rubees': 'Rubee’s', 'Mumbos': 'Mumbo’s', 'Nabnuts': 'Nabnut’s', 'Boggys': 'Boggy’s', 'Wozzas': 'Wozza’s',
    'Banjos': 'Banjo’s', 'Captians': 'Captain’s', 'Captains': 'Captain’s', 'Jinxy': 'Jinxy', 'Gv': 'GV', 'Ttc': 'TTC', 'Cc': 'CC', 'Mmm': 'MMM',
    'Rbb': 'RBB', 'Ccw': 'CCW', 'Mm': 'MM', 'Fp': 'FP', 'Bgs': 'BGS', 'Sns': 'SNS', 'Ff': 'FF', 'Xmas': 'Christmas',
    'Mr': 'Mr.', 'Hive': 'Hive', 'Tiptup': 'Tiptup’s Choir', 'Loggo': 'Loggo',
}
SEASONS = ('spring', 'summer', 'autumn', 'winter')


def split_name(name):
    """MAP_5_TTC_BLUBBERS_SHIP -> (0x5, 'TTC', ['BLUBBERS', 'SHIP'])"""
    m = re.match(r'MAP_([0-9A-F]+)_([A-Z]+)_(.*)$', name)
    return int(m.group(1), 16), m.group(2), m.group(3).split('_')


def maps():
    """map id -> catalogue entry (name, opa, xlu)"""
    return {int(k): v for k, v in CATALOGUE['maps'].items()}


def course_id(map_id):
    for cid, mid in WORLD_MAPS.items():
        if mid == map_id:
            return cid
    entry = maps().get(map_id)
    if not entry or map_id in SKIPPED_MAPS:
        return None
    _, group, words = split_name(entry['name'])
    return 'bk_' + '_'.join([group.lower()] + [w.lower() for w in words])


def display_name(map_id):
    entry = maps()[map_id]
    _, group, words = split_name(entry['name'])
    words = [WORD_FIXES.get(w.capitalize(), w.capitalize()) for w in words]
    return f'{WORLD_NAMES.get(group, group)}: ' + ' '.join(words)


def group_of(map_id):
    return split_name(maps()[map_id]['name'])[1]


def exported_maps():
    """map id -> course id of every map that becomes a course (worlds first, then sub-areas by map id)."""
    out = {mid: cid for cid, mid in WORLD_MAPS.items()}
    order = list(WORLD_NAMES)
    for mid in sorted(maps(), key=lambda m: (order.index(group_of(m)) if group_of(m) in order else 99, m)):
        cid = course_id(mid)
        if cid and mid not in out:
            out[mid] = cid
    return out


def is_subarea(course):
    return course not in WORLD_MAPS


def sky_key(map_id):
    """Which of the 14 world skies a map uses (CCW sub-areas follow their season)."""
    _, group, words = split_name(maps()[map_id]['name'])
    if group == 'CCW':
        for s in SEASONS:
            if s.upper() in words:
                return f'bk_click_clock_wood_{s}'
        return 'bk_click_clock_wood'
    return {
        'SM': 'bk_spiral_mountain', 'MM': 'bk_mumbos_mountain', 'TTC': 'bk_treasure_trove_cove', 'CC': 'bk_clankers_cavern',
        'BGS': 'bk_bubblegloop_swamp', 'GV': 'bk_gobis_valley', 'FP': 'bk_freezeezy_peak', 'RBB': 'bk_rusty_bucket_bay',
        'MMM': 'bk_mad_monster_mansion', 'GL': 'lair',
    }.get(group, 'lair')
