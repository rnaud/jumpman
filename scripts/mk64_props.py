"""Mario Kart 64 course data beyond geometry (helper for export-mk64.py / mk64_courses.py).

Everything here is read from the pinned n64decomp/mk64 sources, never hand-picked:
  * surface ids (`include/mk64.h` `SURFACE_TYPE`) kept per collision triangle,
  * the sky gradient (`yamls/courses/<course>_metadata.yml` `sky_colors`, applied by `course_set_skybox_colours`
    in `src/racing/skybox_and_splitscreen.c`: the game stores the low 8 bits of each s16) and the fog colour /
    far plane (`render_courses.c` `D_801625EC/F4/F0`, `skybox_and_splitscreen.c` `set_perspective_and_aspect_ratio`),
Only the Python standard library is used.
"""
import re

# course dir (mk64_courses.COURSES) -> COURSE_ enum name in the decomp
COURSE_ENUM = {
    'mario_raceway': 'COURSE_MARIO_RACEWAY', 'luigi_raceway': 'COURSE_LUIGI_RACEWAY',
    'koopa_troopa_beach': 'COURSE_KOOPA_BEACH', 'moo_moo_farm': 'COURSE_MOO_MOO_FARM',
    'choco_mountain': 'COURSE_CHOCO_MOUNTAIN', 'kalimari_desert': 'COURSE_KALAMARI_DESERT',
    'toads_turnpike': 'COURSE_TOADS_TURNPIKE', 'frappe_snowland': 'COURSE_FRAPPE_SNOWLAND',
    'wario_stadium': 'COURSE_WARIO_STADIUM', 'sherbet_land': 'COURSE_SHERBET_LAND',
    'royal_raceway': 'COURSE_ROYAL_RACEWAY', 'bowsers_castle': 'COURSE_BOWSER_CASTLE',
    'dks_jungle_parkway': 'COURSE_DK_JUNGLE', 'yoshi_valley': 'COURSE_YOSHI_VALLEY',
    'banshee_boardwalk': 'COURSE_BANSHEE_BOARDWALK', 'rainbow_road': 'COURSE_RAINBOW_ROAD',
    'big_donut': 'COURSE_BIG_DONUT', 'block_fort': 'COURSE_BLOCK_FORT',
    'double_deck': 'COURSE_DOUBLE_DECK', 'skyscraper': 'COURSE_SKYSCRAPER',
}
# yamls/courses/<file>_metadata.yml
METADATA_FILE = {'koopa_troopa_beach': 'koopa_beach'}

# include/mk64.h SURFACE_TYPE
SURFACE_ID = {
    'AIRBORNE': 0, 'ASPHALT': 1, 'DIRT': 2, 'SAND': 3, 'STONE': 4, 'SNOW': 5, 'BRIDGE': 6, 'SAND_OFFROAD': 7,
    'GRASS': 8, 'ICE': 9, 'WET_SAND': 0xA, 'SNOW_OFFROAD': 0xB, 'CLIFF': 0xC, 'DIRT_OFFROAD': 0xD,
    'TRAIN_TRACK': 0xE, 'CAVE': 0xF, 'ROPE_BRIDGE': 0x10, 'WOOD_BRIDGE': 0x11,
    'BOOST_RAMP_WOOD': 0xFC, 'OUT_OF_BOUNDS': 0xFD, 'BOOST_RAMP_ASPHALT': 0xFE, 'RAMP': 0xFF,
}
SURFACE_UNKNOWN = 1  # `SURFACE_DEFAULT` (-1) lists: treated as asphalt


def surface_id(name):
    return SURFACE_ID.get(name, SURFACE_UNKNOWN)


# gCourseFarPersp (set_perspective_and_aspect_ratio, in-race `gGamestate == 4` branch), course units.
FAR_PERSP = {
    'bowsers_castle': 2700, 'banshee_boardwalk': 2700, 'rainbow_road': 2700, 'block_fort': 2700, 'skyscraper': 2700,
    'choco_mountain': 1500, 'double_deck': 1500, 'koopa_troopa_beach': 5000, 'wario_stadium': 4800,
    'mario_raceway': 4500, 'yoshi_valley': 4500, 'frappe_snowland': 4500, 'royal_raceway': 4500,
    'luigi_raceway': 4500, 'moo_moo_farm': 4500, 'toads_turnpike': 4500, 'sherbet_land': 4500,
    'dks_jungle_parkway': 4500, 'kalimari_desert': 7000,
}
# render_courses.c course_init: D_800DC5BC = 1 makes the fog colour the whole sky; (D_801625EC, F4, F0) = fog RGB.
FOG_SKY = {'choco_mountain': (255, 255, 255), 'banshee_boardwalk': (0, 0, 0)}
FOG_COLOUR = {'toads_turnpike': (43, 13, 4)}


def _rgb(c):
    return (c[0] << 16) | (c[1] << 8) | c[2]


def look(source, name):
    """-> (sky top, mid, bottom, fog colour, fog near, fog far) as 0xRRGGBB / world units (course unit x 8)."""
    meta = (source / 'yamls/courses' / f'{METADATA_FILE.get(name, name)}_metadata.yml').read_text()
    values = [int(v) & 0xFF for v in re.search(r'^\s*sky_colors:\s*\[([^\]]*)\]', meta, re.M).group(1).split(',')]
    top, bottom = tuple(values[:3]), tuple(values[3:6])
    if name in FOG_SKY:
        top = bottom = FOG_SKY[name]
    mid = tuple((a + b) // 2 for a, b in zip(top, bottom))
    fog = FOG_SKY.get(name) or FOG_COLOUR.get(name) or bottom
    far = FAR_PERSP.get(name, 6800) * 8
    return _rgb(top), _rgb(mid), _rgb(bottom), _rgb(fog), int(far * 0.8), far
