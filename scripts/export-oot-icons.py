"""Export the Ocarina of Time item / quest icons (`icon_item_static`, `icon_item_24_static`) from a local US 1.2 ROM into
assets/oot-icons/<id>.png plus assets/oot-icons/index.json.  The textures are plain RGBA32 (32x32 item icons at 0x1000 steps,
24x24 quest icons at 0x900 steps; the layout is `assets/xml/textures/icon_item_static.xml` / `icon_item_24_static.xml` of the
decomp).  The id list below is that XML's order; `native/byor-rs/src/oot/icons.rs` carries the same list for the on-device import.

Usage: python3 scripts/export-oot-icons.py [rom]   (default reference/roms/oot-us-1.2.z64)
"""
import binascii
import csv
import hashlib
import json
import runpy
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
n64 = runpy.run_path(str(ROOT / 'scripts/export-oot-link.py'))

ITEM_IDS = ('deku_stick deku_nut bomb bow arrow_fire dins_fire slingshot ocarina_fairy ocarina_time bombchu hookshot longshot '
            'arrow_ice farores_wind boomerang lens_of_truth magic_bean hammer arrow_light nayrus_love bottle_empty '
            'bottle_potion_red bottle_potion_green bottle_potion_blue bottle_fairy bottle_fish bottle_milk_full '
            'bottle_rutos_letter bottle_blue_fire bottle_bug bottle_big_poe bottle_milk_half bottle_poe weird_egg chicken '
            'zeldas_letter mask_keaton mask_skull mask_spooky mask_bunny_hood mask_goron mask_zora mask_gerudo mask_truth '
            'sold_out pocket_egg pocket_cucco cojiro odd_mushroom odd_potion poachers_saw broken_gorons_sword prescription '
            'eyeball_frog eye_drops claim_check bow_fire bow_ice bow_light sword_kokiri sword_master sword_biggoron '
            'shield_deku shield_hylian shield_mirror tunic_kokiri tunic_goron tunic_zora boots_kokiri boots_iron boots_hover '
            'bullet_bag_30 bullet_bag_40 bullet_bag_50 quiver_30 quiver_40 quiver_50 bomb_bag_20 bomb_bag_30 bomb_bag_40 '
            'gorons_bracelet silver_gauntlets golden_gauntlets scale_silver scale_golden broken_giants_knife adults_wallet '
            'giants_wallet deku_seeds fishing_pole').split()
QUEST_IDS = ('medallion_forest medallion_fire medallion_water medallion_spirit medallion_shadow medallion_light '
             'kokiri_emerald goron_ruby zora_sapphire stone_of_agony gerudos_card gold_skulltula heart_container heart_piece '
             'boss_key compass dungeon_map small_key magic_jar_small magic_jar_big').split()
# (segment name, ids, edge in pixels, bytes per icon)
FILES = (('icon_item_static', ITEM_IDS, 32, 0x1000), ('icon_item_24_static', QUEST_IDS, 24, 0x900))


def png(width, height, pixels):
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', binascii.crc32(tag + data))
    rows = b''.join(b'\0' + pixels[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def export(rom, names, out):
    out.mkdir(parents=True, exist_ok=True)
    index = {}
    for segment, ids, edge, step in FILES:
        data = n64['segment'](rom, names, segment)
        for i, name in enumerate(ids):
            pixels = data[i * step:i * step + edge * edge * 4]
            if len(pixels) != edge * edge * 4:
                raise ValueError(f'{segment} is too short for {name}')
            (out / f'{name}.png').write_bytes(png(edge, edge, pixels))
            index[name] = dict(file=f'{name}.png', w=edge, h=edge)
    (out / 'index.json').write_text(json.dumps(dict(version=1, icons=index), separators=(',', ':')) + '\n')
    print(f'oot-icons: {len(index)} icons')


if __name__ == '__main__':
    rom = (Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'reference/roms/oot-us-1.2.z64').read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError('Expected a big-endian US 1.2 OoT ROM')
    with (ROOT / 'reference/oot/baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    export(rom, names, ROOT / 'assets/oot-icons')
