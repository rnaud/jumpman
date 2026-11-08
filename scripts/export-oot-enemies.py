"""Export the enemies of the OoT courses: models, scene/room placements, damage tables, drop table.

Reads the US 1.2 ROM and the zeldaret/oot checkout (`reference/oot`), the same way
`export-oot-courses.py` does (this script borrows its model/glTF helpers), and writes

* `assets/levels/oot_enemies/<kind>.glb`      skinned models (+ the Deku Baba stem pieces)
* `assets/levels/oot_enemies/<course>.json`   `{enemies:[{kind,pos,yaw,param,actor,room,rot}], spawners:[...], skipped:{ACTOR:n}}`
* `native/sim/src/oot_enemy/tables.rs`        DamageTable rows of each overlay (parsed from the C source)
* `native/sim/src/oot_enemy/drop_table.rs`    `sItemDropIds` / `sDropQuantities` of z_en_item00.c

Positions use the course's own transform (collision-bounds centering, x4.4), identical to the
exporter that wrote `assets/courses/<course>.json`.

    python3 scripts/export-oot-enemies.py            # everything
    python3 scripts/export-oot-enemies.py --tables   # only the Rust tables (needs only the decomp)
"""
import argparse
import csv
import glob
import hashlib
import json
import re
import runpy
import struct
import sys
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
OOT = ROOT / 'reference/oot'
SIM = ROOT / 'native/sim/src/oot_enemy'
OUT = ROOT / 'assets/levels/oot_enemies'
MASK = 0xffffff

# actor overlay -> (kind, params -> kind) used by the placement pass. Kinds are the Rust `Kind::name()`s.
# Models: kind -> object bank, skeleton, idle clip, Actor_SetScale of the overlay, extra clips (label, symbol).
MODELS = {
    'deku_baba': dict(bank='object_dekubaba', skel='gDekubabaHeadSkel', idle='gDekubabaChompAnim', scale=.01,
                      clips=[('Wait', 'gDekubabaIdleAnim')]),
    'goma_larva': dict(bank='object_gol', skel='gObjectGolSkel', idle='gObjectGolStandAnim', scale=.01,
                       clips=[('Run', 'gObjectGolRunningAnim'), ('Jump', 'gObjectGolJumpHeadbuttAnim'),
                              ('Prepare', 'gObjectGolPrepareJumpAnim'), ('Land', 'gObjectGolLandFromJumpAnim'),
                              ('Damaged', 'gObjectGolDamagedAnim'), ('Death', 'gObjectGolDeathAnim'),
                              ('Walk', 'gObjectGolWalkAnim')]),
    'skulltula': dict(bank='object_st', skel='object_st_Skel_005298', idle='object_st_Anim_000304', scale=.04,
                      clips=[('Anim55A8', 'object_st_Anim_0055A8'), ('Anim5B98', 'object_st_Anim_005B98')]),
    'peahat': dict(bank='object_peehat', skel='gPeehatSkel', idle='gPeehatFlyingAnim', scale=.036,
                   clips=[('Landing', 'gPeehatLandingAnim'), ('Recoil', 'gPeehatRecoilAnim'), ('Rising', 'gPeehatRisingAnim')]),
    'tektite': dict(bank='object_tite', skel='object_tite_Skel_003A20', idle='object_tite_Anim_0012E4', scale=.01,
                    textures=('object_tite_Tex_001300', 'object_tite_Tex_001700', 'object_tite_Tex_001900'),
                    clips=[('Attack', 'object_tite_Anim_00083C'), ('Walk', 'object_tite_Anim_000A14'),
                           ('Land', 'object_tite_Anim_000C70'), ('Anim69C', 'object_tite_Anim_00069C'),
                           ('Anim4F8', 'object_tite_Anim_0004F8')]),
    'red_tektite': dict(bank='object_tite', skel='object_tite_Skel_003A20', idle='object_tite_Anim_0012E4', scale=.01,
                        textures=('object_tite_Tex_001B00', 'object_tite_Tex_001F00', 'object_tite_Tex_002100'), clips=[]),
    'octorok': dict(bank='object_okuta', skel='gOctorokSkel', idle='gOctorokFloatAnim', scale=.01,
                    clips=[('Shoot', 'gOctorokShootAnim'), ('Die', 'gOctorokDieAnim'), ('Hide', 'gOctorokHideAnim'),
                           ('Hit', 'gOctorokHitAnim'), ('Appear', 'gOctorokAppearAnim')]),
    'mad_scrub': dict(bank='object_dekunuts', skel='gDekuNutsSkel', idle='gDekuNutsStandAnim', scale=.01,
                      clips=[('Spit', 'gDekuNutsSpitAnim'), ('Damage', 'gDekuNutsDamageAnim'), ('Burrow', 'gDekuNutsBurrowAnim'),
                             ('Unburrow', 'gDekuNutsUnburrowAnim'), ('Die', 'gDekuNutsDieAnim'), ('Run', 'gDekuNutsRunAnim'),
                             ('Look', 'gDekuNutsLookAroundAnim')]),
    'wolfos': dict(bank='object_wf', skel='gWolfosGraySkel', idle='gWolfosIdleAnim', scale=.0075,
                   textures=('gWolfosGrayEyeOpenTex',),
                   clips=[('Run', 'gWolfosRunAnim'), ('Attack', 'gWolfosAttackAnim'), ('Backflip', 'gWolfosBackflipAnim'),
                          ('Block', 'gWolfosBlockAnim'), ('Damaged', 'gWolfosDamagedAnim'), ('Side', 'gWolfosSideStepAnim'),
                          ('Howl', 'gWolfosHowlAnim')]),
    'guay': dict(bank='object_crow', skel='gGuaySkel', idle='gGuayFlyAnim', scale=.01, clips=[]),
    'leever': dict(bank='object_reeba', skel='object_reeba_Skel_001EE8', idle='object_reeba_Anim_0001E4', scale=.04, clips=[]),
}

# Placed overlay -> kind. Parameters pick the variant (see `kind_for`).
ACTOR_KIND = {
    'ACTOR_EN_DEKUBABA': 'deku_baba', 'ACTOR_EN_KAREBABA': 'withered_baba', 'ACTOR_EN_GOMA': 'goma_larva',
    'ACTOR_EN_ST': 'skulltula', 'ACTOR_EN_SW': 'gold_skulltula', 'ACTOR_EN_PEEHAT': 'peahat',
    'ACTOR_EN_TITE': 'tektite', 'ACTOR_EN_OKUTA': 'octorok', 'ACTOR_EN_DEKUNUTS': 'mad_scrub',
    'ACTOR_EN_WF': 'wolfos', 'ACTOR_EN_CROW': 'guay', 'ACTOR_EN_REEBA': 'leever',
}
# Spawner actors are kept as `spawners` (EnEncount1 params: type bits 11..15, max live 6..10, total 0..5).
SPAWNERS = {'ACTOR_EN_ENCOUNT1'}
# Actors that look like enemies but are scene NPCs / props: reported separately in `skipped`.
NPC_LIKE = {'ACTOR_EN_HINTNUTS': 'quiz scrub NPC', 'ACTOR_EN_SHOPNUTS': 'business scrub shop NPC',
            'ACTOR_EN_SKJ': 'Skull Kid NPC', 'ACTOR_EN_ENCOUNT2': 'meteor/rock spawner of Death Mountain'}


def actor_categories():
    cats = {}
    for f in glob.glob(str(OOT / 'src/overlays/actors/*/z_*.c')) + glob.glob(str(OOT / 'src/overlays/bosses/*/z_*.c')):
        m = re.search(r'/\*\*/ (ACTOR_\w+),\s*/\*\*/ (ACTORCAT_\w+)', Path(f).read_text(errors='replace'))
        if m:
            cats[m[1]] = m[2]
    return cats


# ----------------------------------------------------------------------------------------------------------
# Rust tables from the decomp

def parse_damage_tables():
    """name -> (32 damages, 32 reaction codes) for every DamageTable in the overlays we port + the shared ones."""
    sources = {
        'dekubaba_normal': ('ovl_En_Dekubaba/z_en_dekubaba.c', 'sDamageTableNormal'),
        'dekubaba_big': ('ovl_En_Dekubaba/z_en_dekubaba.c', 'sDamageTableBig'),
        'okuta': ('ovl_En_Okuta/z_en_okuta.c', 'sDamageTable'),
        'peahat': ('ovl_En_Peehat/z_en_peehat.c', 'sDamageTable'),
        'tite': ('ovl_En_Tite/z_en_tite.c', 'sDamageTable'),
        'wolfos': ('ovl_En_Wf/z_en_wf.c', 'sDamageTable'),
        'leever': ('ovl_En_Reeba/z_en_reeba.c', 'sDamageTable'),
        'guay': ('ovl_En_Crow/z_en_crow.c', 'sDamageTable'),
        'dekunuts': ('ovl_En_Dekunuts/z_en_dekunuts.c', 'sDamageTable'),
    }
    out = {}
    for key, spec in sources.items():
        text = (OOT / 'src/overlays/actors' / spec[0]).read_text()
        start = text.index(f'DamageTable {spec[1]}')
        entries = []
        # An array of tables holds several 32-row blocks back to back.
        end = text.index('\n};', start)
        rows = re.findall(r'DMG_ENTRY\((\d+), (0x[0-9A-Fa-f]+|\w+)\)', text[start:end])
        which = spec[2] if len(spec) > 2 else 0
        rows = rows[which * 32:(which + 1) * 32]
        assert len(rows) == 32, (key, len(rows))
        out[key] = ([int(d) for d, _ in rows], [r for _, r in rows])
    # Shared tables of z_collision_btltbls.c (DamageTable_Get): 1 = Karebaba, 2 = Skulltula, 0xE = Gold Skulltula.
    text = (OOT / 'src/code/z_collision_btltbls.c').read_text()
    allrows = re.findall(r'DMG_ENTRY\((\d+), (0x[0-9A-Fa-f]+|\w+)\)', text)
    blocks = [allrows[i * 32:(i + 1) * 32] for i in range(len(allrows) // 32)]
    for key, index in (('karebaba', 1), ('skulltula', 2), ('gold_skulltula', 0xE)):
        rows = blocks[index]
        out[key] = ([int(d) for d, _ in rows], [r for _, r in rows])
    return out


def write_tables():
    SIM.mkdir(parents=True, exist_ok=True)
    tables = parse_damage_tables()
    lines = ['//! Generated by scripts/export-oot-enemies.py from the zeldaret/oot sources. Do not edit.',
             '//! Each row: `(damage, reaction)` in `DamageTable` order (nut, stick, slingshot, explosive, boomerang,',
             '//! arrow, hammer, hookshot, kokiri sword, master sword, giant knife, fire arrow, ice arrow, light arrow,',
             '//! 3 unknown arrows, fire magic, ice magic, light magic, shield, mirror ray, kokiri/giant/master spin,',
             '//! kokiri/giant/master jump, unknown, unblockable, hammer jump, unknown).', '']
    for key, (dmg, react) in tables.items():
        rows = ', '.join(f'({d}, {int(r, 16) if r.startswith("0x") else 0xE if "BOOMERANG" in r else 0xF if "SWORD" in r else 1 if "STUN" in r else 2 if "FIRE" in r else 0})'
                         for d, r in zip(dmg, react))
        lines.append(f'pub const {key.upper()}: [(u8, u8); 32] = [{rows}];')
    (SIM / 'tables.rs').write_text('\n'.join(lines) + '\n')

    text = (OOT / 'src/code/z_en_item00.c').read_text()
    a = text.index('static u8 sItemDropIds[]')
    ids = re.findall(r'ITEM00_\w+', text[a:text.index('};', a)])
    order = ['RUPEE_GREEN', 'RUPEE_BLUE', 'RUPEE_RED', 'RECOVERY_HEART', 'BOMBS_A', 'ARROWS_SINGLE', 'HEART_PIECE',
             'HEART_CONTAINER', 'ARROWS_SMALL', 'ARROWS_MEDIUM', 'ARROWS_LARGE', 'BOMBS_B', 'NUTS', 'STICK', 'MAGIC_LARGE',
             'MAGIC_SMALL', 'SEEDS', 'SMALL_KEY', 'FLEXIBLE', 'RUPEE_ORANGE', 'RUPEE_PURPLE']
    names = {n: i for i, n in enumerate(order)}
    b = text.index('static u8 sDropQuantities[]')
    quantities = [int(v) for v in re.findall(r'\b(\d+),', text[b:text.index('};', b)].split('{', 1)[1])]
    assert len(ids) == len(quantities) == 240, (len(ids), len(quantities))
    out = ['//! Generated by scripts/export-oot-enemies.py from `sItemDropIds` / `sDropQuantities` (z_en_item00.c). Do not edit.',
           '//! 15 tables of 16 rows: `Item_DropCollectibleRandom` rolls `Rand_ZeroOne() * 16` for the row.', '',
           '/// `ITEM00_*` values, in the decomp order; 0xFF is `ITEM00_NONE`.',
           'pub const ITEM_DROP_IDS: [u8; 240] = [' + ', '.join(str(names.get(i[7:], 0xFF)) for i in ids) + '];',
           'pub const DROP_QUANTITIES: [u8; 240] = [' + ', '.join(map(str, quantities)) + '];']
    (SIM / 'drop_table.rs').write_text('\n'.join(out) + '\n')
    print('wrote', SIM / 'tables.rs', 'and drop_table.rs')


# ----------------------------------------------------------------------------------------------------------
# Models and placements

def kind_for(actor, param):
    kind = ACTOR_KIND[actor]
    if actor == 'ACTOR_EN_DEKUBABA':
        # EnDekubaba params: 0 = normal, 1 = big.
        return 'big_deku_baba' if param == 1 else 'deku_baba'
    if actor == 'ACTOR_EN_TITE':
        return 'red_tektite' if param == -1 else 'tektite'   # TEKTITE_RED = -1, TEKTITE_BLUE = -2
    if actor == 'ACTOR_EN_WF':
        return 'wolfos'
    if actor == 'ACTOR_EN_PEEHAT':
        return 'peahat_larva' if param == 1 else 'peahat'   # PEAHAT_TYPE_FLYING 0, GROUNDED -1, LARVA 1
    return kind


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--tables', action='store_true', help='only write the Rust tables')
    args = parser.parse_args()
    # The Rust tables come from the decomp's C sources; a BYOR staging tree has only the asset XMLs.
    if (OOT / 'src/code/z_en_item00.c').is_file():
        write_tables()
    elif args.tables:
        raise SystemExit('reference/oot sources are not checked out')
    if args.tables:
        return
    courses = runpy.run_path(str(ROOT / 'scripts/export-oot-courses.py'))
    n64, commands = courses['n64'], courses['commands']
    rom = (ROOT / 'reference/roms/oot-us-1.2.z64').read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError('Expected a big-endian US 1.2 OoT ROM')
    with (OOT / 'baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    segment = lambda name: n64['segment'](rom, names, name)  # noqa: E731
    table = (OOT / 'include/tables/actor_table.h').read_text()
    ids = {int(a, 16): n for a, n in re.findall(r'/\* (0x[0-9A-Fa-f]+) \*/ DEFINE_ACTOR\w*\(\w+, (\w+),', table)}
    cats = actor_categories()
    enemyish = {k for k, v in cats.items() if v in ('ACTORCAT_ENEMY', 'ACTORCAT_BOSS')} | {'ACTOR_EN_SW', 'ACTOR_EN_GOMA', 'ACTOR_EN_REEBA'}
    OUT.mkdir(parents=True, exist_ok=True)

    used = set()
    summary = {}
    for course, (scene, _spawn) in courses['COURSES'].items():
        scene_data = segment(scene + '_scene')
        header = commands(scene_data)
        _, col = header[3]
        xmin, _, zmin, xmax, _, zmax = struct.unpack_from('>6h', scene_data, col)
        cx, cz = (xmin + xmax) / 2, (zmin + zmax) / 2
        tf = lambda x, y, z: [round((x - cx) * 4.4, 2), round(y * 4.4, 2), round((z - cz) * 4.4, 2)]  # noqa: E731
        enemies, spawners, skipped = [], [], {}
        for name in names:
            if not name.startswith(scene + '_room_'):
                continue
            room = segment(name)
            count, actor_list = commands(room).get(1, (0, 0))
            for i in range(count):
                actor_id, ax, ay, az, rx, ry, rz, param = struct.unpack_from('>Hhhh3hh', room, actor_list + 16 * i)
                actor = ids.get(actor_id, hex(actor_id))
                if actor in ACTOR_KIND:
                    kind = kind_for(actor, param)
                    enemies.append(dict(kind=kind, actor=actor_id, param=param, pos=tf(ax, ay, az), yaw=ry * 180 / 32768,
                                        rot=[rx, ry, rz], room=name, index=i))
                    used.add(ACTOR_KIND[actor])
                elif actor in SPAWNERS:
                    spawners.append(dict(kind='encount1', actor=actor_id, param=param, pos=tf(ax, ay, az),
                                         spawnType=(param >> 11) & 31, maxLive=(param >> 6) & 31, total=param & 63,
                                         range=120 + 40 * rz, room=name))
                elif actor in enemyish or actor in NPC_LIKE:
                    skipped[actor] = skipped.get(actor, 0) + 1
        (OUT / f'{course}.json').write_text(json.dumps(dict(enemies=enemies, spawners=spawners, skipped=skipped), separators=(',', ':')) + '\n')
        summary[course] = (len(enemies), len(spawners), skipped)
        print(f'{course}: {len(enemies)} enemies, {len(spawners)} spawners, skipped {skipped}')

    actor_model = courses['actor_model']
    for key, m in MODELS.items():
        try:
            actor_model(key, segment, m['bank'], m['skel'], m['idle'], resident=SimpleNamespace(scale=m['scale'], textures=m.get('textures', ())),
                        output_folder='oot_enemies', extra_clips=m['clips'])
            print('model', key)
        except Exception as e:  # noqa: BLE001 - report and continue; the loader skips kinds without a model
            print('MODEL FAILED', key, repr(e), file=sys.stderr)
    # Deku Baba stem pieces and the crown of leaves (EnDekubaba_Draw).
    SceneModel, display_mesh = courses['SceneModel'], courses['display_mesh']
    bank = 'object_dekubaba'
    specs = n64['definitions'](OOT / f'assets/xml/objects/{bank}.xml')
    for piece in ('Stem0', 'Stem1', 'Stem2', 'BaseLeaves'):
        mesh = SceneModel()
        display_mesh(mesh, bank, {6: segment(bank)}, 0x06000000 + n64['offset'](specs[f'gDekubaba{piece}DL']),
                     lambda x, y, z: [v * 0.01 * 4.4 for v in (x, y, z)])
        mesh.write(OUT / f'deku_baba_{piece.lower()}.glb')
    print('summary', {k: v[:2] for k, v in summary.items() if v[0] or v[1]})


if __name__ == '__main__':
    main()
