#!/usr/bin/env python3
"""Export Luigi Raceway, Mario's flat-ground kart views and the HUD / item / racer-face / Lakitu textures from a US MK64 ROM.
Uses the pinned n64decomp/mk64 vertex/display-list sources and asset offsets.
No emulator, compiler, or third-party Python packages required.
"""
import argparse
import hashlib
import json
import re
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'reference/mk64'


def mio0(data):
    if data[:4] != b'MIO0':
        return data
    size, compressed, raw = struct.unpack_from('>III', data, 4)
    bits, mask, cursor = 0, 0, 16
    out = bytearray()
    while len(out) < size:
        if not mask:
            bits = data[cursor]
            cursor += 1
            mask = 128
        if bits & mask:
            out.append(data[raw])
            raw += 1
        else:
            pair = int.from_bytes(data[compressed:compressed + 2], 'big')
            compressed += 2
            for _ in range((pair >> 12) + 3):
                out.append(out[-((pair & 4095) + 1)])
        mask >>= 1
    return bytes(out[:size])


def rgba16(data):
    result = bytearray()
    for (pixel,) in struct.iter_unpack('>H', data):
        result.extend(((pixel >> 11) * 255 // 31, ((pixel >> 6) & 31) * 255 // 31,
                       ((pixel >> 1) & 31) * 255 // 31, (pixel & 1) * 255))
    return result


def png(path, width, height, pixels):
    assert len(pixels) == width * height * 4
    def chunk(kind, value):
        return struct.pack('>I', len(value)) + kind + value + struct.pack('>I', zlib.crc32(kind + value))
    rows = b''.join(b'\0' + pixels[y * width * 4:(y + 1) * width * 4] for y in range(height))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)) +
                     chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def decode(kind, data, w, h, palette=None):
    """N64 texture formats to RGBA bytes. I4/I8/IA4 become white with the intensity as alpha (the game tints them)."""
    out = bytearray()
    if kind == 'rgba16':
        return rgba16(data[:w * h * 2])
    if kind == 'ci8':
        for index in data[:w * h]:
            out.extend(palette[index * 4:index * 4 + 4])
    elif kind == 'i8':
        for v in data[:w * h]:
            out.extend((255, 255, 255, v))
    elif kind == 'i4':
        for byte in data[:w * h // 2]:
            for v in (byte >> 4, byte & 15):
                out.extend((255, 255, 255, v * 17))
    elif kind == 'ia4':
        for byte in data[:w * h // 2]:
            for v in (byte >> 4, byte & 15):
                out.extend((255, 255, 255, 255 if v & 1 else 0))
    elif kind == 'ia16':
        for i in range(0, w * h * 2, 2):
            out.extend((data[i],) * 3 + (data[i + 1],))
    else:
        raise ValueError(kind)
    return out


# reference/mk64/assets/*.json -> output file prefix
HUD_SOURCES = {
    'item_window': 'hud-item-',
    'character_portraits': 'hud-portrait-',
    'minimap_icons': 'hud-minimap-',
    'hud_type_c': 'hud-rank-',
    'lakitu/redlights': 'lakitu-red-',
    'lakitu/bluelight': 'lakitu-blue-',
    'lakitu/nolights': 'lakitu-none-',
    'lakitu/checkeredflag': 'lakitu-flag-',
    'lakitu/finallap': 'lakitu-final-',
    'lakitu/secondlap': 'lakitu-second-',
}


def export_hud(rom, output):
    """HUD, racer-face, item-window, minimap and Lakitu light textures used by the native kart mode."""
    assets = SOURCE / 'assets'
    common = {}
    def block(start):
        if start not in common:
            common[start] = mio0(rom[start:])
        return common[start]
    def raw(entry, size):
        start = int(entry['rom_offset'], 0)
        offset = int(entry['block_offset'], 0) if 'block_offset' in entry else 0
        return block(start)[offset:offset + size]
    size_of = {'rgba16': 2, 'ci8': 1, 'i8': 1, 'i4': .5, 'ia4': .5, 'ia16': 2}
    count = 0
    for name, prefix in HUD_SOURCES.items():
        table = json.loads((assets / f'{name}.json').read_text())
        for key, entry in table.items():
            if '_tlut_' in key:
                continue
            w, h, kind = entry['width'], entry['height'], entry['type']
            palette = rgba16(raw(table[entry['tlut']], 512)) if 'tlut' in entry else None
            pixels = decode(kind, raw(entry, int(w * h * size_of[kind])), w, h, palette)
            label = re.sub(r'^(common_texture_|gTexture)', '', key)
            label = re.sub(r'^(item_window_|portrait_|minimap_|hud_type_C_|Lakitu(Red|Blue|No|Checkered|Final|Second)\w*?)', '', label)
            label = label or key
            png(output / f'{prefix}{label.lower().replace("_", "-")}.png', w, h, pixels)
            count += 1
    # The green shell is drawn as an 8-frame spinning sprite: one 256 x 32 strip.
    shell = json.loads((assets / 'greenshell.json').read_text())
    palette = rgba16(raw(shell['common_tlut_green_shell'], 512))
    frames = [decode('ci8', raw(shell[f'texture_green_shell_{i}'], 1024), 32, 32, palette) for i in range(8)]
    strip = bytearray()
    for y in range(32):
        for frame in frames:
            strip.extend(frame[y * 128:(y + 1) * 128])
    png(output / 'mk64-green-shell.png', 256, 32, strip)
    count += 1
    # Standalone common textures (hud text, digits, particles, items) from assets.json.
    listing = json.loads((SOURCE / 'assets.json').read_text())
    wanted = re.compile(r'common_texture_(hud_(lap|123|time|total_time|lap_time|normal_digit|\dst|\dnd|\drd|\dth|lap_\d_on_3)|item_box_question_mark|banana|flat_banana|particle_(fire|spark_\d|smoke_\d))\.')
    for key, entry in listing.items():
        if not wanted.search(key):
            continue
        w, h = entry['meta']['dims']
        kind = key.split('.')[-2]
        start, offset = [int(v, 0) for v in entry['offsets']['us']]
        data = block(start)[offset:offset + int(w * h * size_of[kind])]
        label = key.split('/')[-1].split('.')[0].replace('common_texture_', '').replace('_', '-')
        png(output / f'mk64-{label}.png', w, h, decode(kind, data, w, h))
        count += 1
    # Luigi Raceway minimap outline (I4, 64 x 96).
    outline = json.loads((assets / 'course_outlines.json').read_text())['minimap_luigi_raceway']
    png(output / 'hud-minimap-luigi-raceway.png', 64, 96, decode('i4', block(int(outline['rom_offset'], 0)), 64, 96))
    print(f'Exported {count + 1} HUD textures to {output}')


def export(rom_path, output):
    rom = rom_path.read_bytes()
    if rom[:4] == bytes.fromhex('37804012'):
        rom = bytes(b for i in range(0, len(rom), 2) for b in rom[i:i + 2][::-1])
    elif rom[:4] == bytes.fromhex('40123780'):
        rom = bytes(b for i in range(0, len(rom), 4) for b in rom[i:i + 4][::-1])
    digest = hashlib.sha1(rom).hexdigest()
    if digest != '579c48e211ae952530ffc8738709f078d5dd215e':
        raise ValueError('Expected the US Mario Kart 64 ROM (SHA-1 579c48e211ae952530ffc8738709f078d5dd215e)')
    output.mkdir(parents=True, exist_ok=True)
    assets = json.loads((SOURCE / 'assets.json').read_text())
    aliases = dict(re.findall(r'"(gTexture\w+)":\s*\("([^"]+)"', (SOURCE / 'tools/linkonly_generator.py').read_text()))
    course = SOURCE / 'courses/luigi_raceway'
    vertices = []
    pattern = r'\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*MACRO_COLOR_FLAG\((0x\w+),\s*(0x\w+),\s*(0x\w+)'
    for row in re.findall(pattern, (course / 'course_vertices.inc.c').read_text()):
        vertices.append([int(v, 0) if v.startswith('0x') else int(v) for v in row])
    groups, textures, seen = {}, {}, set()
    # Each leaf list contains explicit geometry; parent lists only reference leaves.
    # Read each leaf once, avoiding duplicate geometry from visibility-sector lists.
    texture, cache = None, {}
    for command, args in re.findall(r'(gs\w+)\((.*?)\)', (course / 'course_displaylists.inc.c').read_text(), re.S):
        args = [v.strip() for v in args.split(',')]
        if command == 'gsDPSetTextureImage':
            texture = args[3]
            if texture not in textures:
                native = texture.replace('gLRTexture', 'gTexture')
                name = aliases[native]
                key = f'textures/standalone/{name}.rgba16.png'
                entry = assets[key]
                w, h = entry['meta']['dims']
                start, offset = [int(v, 0) for v in entry['offsets']['us']]
                data = mio0(rom[start:])[offset:offset + w * h * 2]
                filename = texture + '.png'
                png(output / filename, w, h, rgba16(data))
                textures[texture] = {'file': filename, 'width': w, 'height': h}
        elif command == 'gsSPVertex':
            address, count, slot = [int(v, 0) for v in args]
            base = (address & 0xffffff) // 16
            cache.update({slot + i: base + i for i in range(count)})
        elif command in ('gsSP2Triangles', 'gsSP1Triangle'):
            nums = [int(v, 0) for v in args]
            for i in range(0, len(nums), 4):
                tri = tuple(cache[v] for v in nums[i:i + 3])
                key = (texture, tri)
                if key not in seen:
                    seen.add(key)
                    groups.setdefault(texture, []).extend(tri)
    path_text = (course / 'course_data.c').read_text().split('d_course_luigi_raceway_track_path[] = {')[1].split('};')[0]
    path = [[int(v) for v in row] for row in re.findall(r'\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*\d+\s*\}', path_text)]
    def data_for(kart, entry):
        start = int(entry['rom_offset'], 0)
        return mio0(rom[start:])[:entry['width'] * entry['height'] * (2 if entry['type'] == 'rgba16' else 1)]
    # Group 4: neutral pitch, 35 view angles; mirror the other side (src/kart_dma.c). Mario's atlas is
    # mario-kart.png; the other racers (CPU karts) are <name>-kart.png.
    for name, out_name in [('mario', 'mario-kart.png'), *[(n, f'{n}-kart.png') for n in ('luigi', 'peach', 'toad', 'yoshi', 'wario', 'bowser', 'donkeykong')]]:
        kart = json.loads((SOURCE / f'assets/karts/{name}_kart.json').read_text())
        atlas = bytearray(64 * 35 * 64 * 4)
        for column, frame in enumerate([*range(84, 105), *range(235, 249)]):
            entry = kart[f'{name}_kart_frame{frame:03}']
            palette = rgba16(b''.join(data_for(kart, kart[tlut]) for tlut in entry['tlut']))
            pixels = b''.join(palette[index * 4:index * 4 + 4] for index in data_for(kart, entry))
            for y in range(64):
                dest = (y * 64 * 35 + column * 64) * 4
                atlas[dest:dest + 256] = pixels[y * 256:(y + 1) * 256]
        png(output / out_name, 64 * 35, 64, atlas)
    payload = {'source': 'n64decomp/mk64', 'romSha1': digest, 'vertices': vertices,
               'groups': groups, 'textures': textures, 'path': path, 'scale': 8}
    (output / 'luigi-raceway.json').write_text(json.dumps(payload, separators=(',', ':')))
    export_hud(rom, output)
    print(f'Exported {len(vertices)} vertices, {len(seen)} triangles, {len(textures)} textures and 35 kart views to {output}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / 'public/mk64')
    args = parser.parse_args()
    export(args.rom.expanduser(), args.output)
