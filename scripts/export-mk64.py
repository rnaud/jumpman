#!/usr/bin/env python3
"""Export Luigi Raceway and Mario's flat-ground kart views from a US MK64 ROM.
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
    kart = json.loads((SOURCE / 'assets/karts/mario_kart.json').read_text())
    def data_for(entry):
        start = int(entry['rom_offset'], 0)
        return mio0(rom[start:])[:entry['width'] * entry['height'] * (2 if entry['type'] == 'rgba16' else 1)]
    # Group 4: neutral pitch, 35 view angles; mirror the other side (src/kart_dma.c).
    atlas = bytearray(64 * 35 * 64 * 4)
    for column, frame in enumerate([*range(84, 105), *range(235, 249)]):
        entry = kart[f'mario_kart_frame{frame:03}']
        palette = rgba16(b''.join(data_for(kart[name]) for name in entry['tlut']))
        pixels = b''.join(palette[index * 4:index * 4 + 4] for index in data_for(entry))
        for y in range(64):
            dest = (y * 64 * 35 + column * 64) * 4
            atlas[dest:dest + 256] = pixels[y * 256:(y + 1) * 256]
    png(output / 'mario-kart.png', 64 * 35, 64, atlas)
    payload = {'source': 'n64decomp/mk64', 'romSha1': digest, 'vertices': vertices,
               'groups': groups, 'textures': textures, 'path': path, 'scale': 8}
    (output / 'luigi-raceway.json').write_text(json.dumps(payload, separators=(',', ':')))
    print(f'Exported {len(vertices)} vertices, {len(seen)} triangles, {len(textures)} textures and 35 kart views to {output}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / 'public/mk64')
    args = parser.parse_args()
    export(args.rom.expanduser(), args.output)
