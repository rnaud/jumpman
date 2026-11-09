#!/usr/bin/env python3
"""Export every Mario Kart 64 course (kart JSON + walkable course), Mario's flat-ground kart views and the HUD / item / racer-face / Lakitu textures from a US MK64 ROM.
Uses the pinned n64decomp/mk64 vertex/display-list sources and asset offsets.
No emulator, compiler, or third-party Python packages required.
"""
import argparse
import hashlib
import json
import re
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'reference/mk64'
ASSETS = ROOT / 'assets'
sys.path.insert(0, str(Path(__file__).resolve().parent))
import mk64_courses  # noqa: E402


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


# `common_data` (segment 0x0D) is the MIO0 block at ROM 0x132B50. The item roulette curves live in it
# (`yamls/us/common_data.yml`, `common_*_item_curve*`): 100 bytes per rank, an `ITEM_*` id each; `gen_random_item`
# (src/update_objects.c) indexes `curve[rank * 100 + random]`.
ITEM_CURVES = {
    'grandPrixHuman': (0x8150, 8),    # common_grand_prix_human_item_curve .. 8
    'grandPrixCpu': (0x8470, 8),      # common_grand_prix_cpu_item_curve .. 8
    'versus2': (0x8790, 2),           # common_versus_2_player_item_curve, 2
    'versus3': (0x8858, 3),
    'versus4': (0x8984, 4),
    'battle': (0x8B14, 1),            # common_battle_item_curve
}


def export_item_curves(rom, output):
    """public/mk64/item-curves.json: the roulette tables as hex strings (one 200-character string per rank)."""
    common = mio0(rom[0x132B50:])
    tables = {name: [common[off + 100 * r:off + 100 * (r + 1)].hex() for r in range(rows)] for name, (off, rows) in ITEM_CURVES.items()}
    (output / 'item-curves.json').write_text(json.dumps(tables, indent=1) + '\n')
    print(f'Exported item roulette curves ({sum(len(v) for v in tables.values())} ranks)')


def export_item_sprites(rom, output):
    """Blue and red shell sprite strips (256 x 32, 8 frames like mk64-green-shell.png). The red shell is the green
    palette with red and green swapped (`init_red_shell_texture`, src/racing/actors.c)."""
    assets = SOURCE / 'assets'
    common = mio0(rom[0x132B50:])
    def strip(frames):
        out = bytearray()
        for y in range(32):
            for frame in frames:
                out.extend(frame[y * 128:(y + 1) * 128])
        return out
    blue = json.loads((assets / 'blueshell.json').read_text())
    tlut = blue['common_tlut_blue_shell']
    palette = rgba16(common[int(tlut['block_offset'], 0):][:512])
    frames = []
    for i in range(8):
        entry = blue[f'texture_blue_shell_{i}']
        data = mio0(rom[int(entry['rom_offset'], 0):])[:1024]
        frames.append(decode('ci8', data, 32, 32, palette))
    png(output / 'mk64-blue-shell.png', 256, 32, strip(frames))
    green = json.loads((assets / 'greenshell.json').read_text())
    tlut = green['common_tlut_green_shell']
    raw = bytearray(common[int(tlut['block_offset'], 0):][:512])
    for i in range(0, 512, 2):
        c = (raw[i] << 8) | raw[i + 1]
        c = ((c & 0xF800) >> 5) | ((c & 0x07C0) << 5) | (c & 0x3F)
        raw[i], raw[i + 1] = c >> 8, c & 255
    palette = rgba16(bytes(raw))
    frames = []
    for i in range(8):
        entry = green[f'texture_green_shell_{i}']
        data = mio0(rom[int(entry['rom_offset'], 0):])[:1024]
        frames.append(decode('ci8', data, 32, 32, palette))
    png(output / 'mk64-red-shell.png', 256, 32, strip(frames))
    print('Exported blue and red shell sprites')


def minimap(path, nodes):
    """64 x 96 white-on-transparent outline of the lap path, fitted like kart_hud.rs `MapFit::from_path`
    (a 25 x 79 pixel box centred at (31.5, 46.5)); the in-game outlines are only extracted for Luigi Raceway."""
    pts = [n for n in nodes if n[0] > -30000]
    lo = [min(p[0] for p in pts), min(p[2] for p in pts)]
    hi = [max(p[0] for p in pts), max(p[2] for p in pts)]
    scale = min(25 / max(hi[0] - lo[0], 1), 79 / max(hi[1] - lo[1], 1))
    cx, cz = (lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2
    pixels = bytearray(64 * 96 * 4)
    for a, b in zip(pts, pts[1:] + pts[:1]):
        for t in (0, .5):
            x = 31.5 + (a[0] + (b[0] - a[0]) * t - cx) * scale
            y = 46.5 + (a[2] + (b[2] - a[2]) * t - cz) * scale
            for py in range(int(y - 2), int(y + 3)):
                for px in range(int(x - 2), int(x + 3)):
                    if 0 <= px < 64 and 0 <= py < 96 and (px + .5 - x) ** 2 + (py + .5 - y) ** 2 <= 2.4:
                        i = (py * 64 + px) * 4
                        pixels[i:i + 4] = b'\xff\xff\xff\xff'
    png(path, 64, 96, pixels)


def export_kart_course(textures_rom, name, output):
    """public/mk64/<name>.json in the format kart.rs loads (vertices, display-list groups, textures, path).
    Returns the file name, or None for courses without a lap path (battle arenas)."""
    course = SOURCE / 'courses' / name
    path = mk64_courses.Course(SOURCE, name).track_path()
    if not path:
        return None
    if name != 'luigi_raceway':
        # The other courses are drawn from their walkable-course glb (decal layers, wrap modes and translucent
        # water baked in); the kart file only carries the lap path, the item boxes and the minimap.
        minimap(output / f'hud-minimap-{name.replace("_", "-")}.png', path)
        boxes = mk64_courses.Course(SOURCE, name).item_boxes()
        payload = {'source': 'n64decomp/mk64', 'romSha1': '579c48e211ae952530ffc8738709f078d5dd215e',
                   'model': f'levels/mk64_{name}/terrain.glb', 'path': path, 'scale': 8, 'itemBoxes': boxes}
        filename = name.replace('_', '-') + '.json'
        (output / filename).write_text(json.dumps(payload, separators=(',', ':')))
        return filename
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
                image = textures_rom.image(texture)
                if image is None:
                    print(f'warning: {name}: texture {texture} not found, using grey')
                    image = (32, 32, bytes([128, 128, 128, 255]) * 1024)
                w, h, pixels = image
                filename = texture + '.png'
                png(output / filename, w, h, pixels)
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
    payload = {'source': 'n64decomp/mk64', 'romSha1': '579c48e211ae952530ffc8738709f078d5dd215e', 'vertices': vertices,
               'groups': groups, 'textures': textures, 'path': path, 'scale': 8}
    filename = name.replace('_', '-') + '.json'
    (output / filename).write_text(json.dumps(payload, separators=(',', ':')))
    return filename



KART_CHARACTERS = ('mario', 'luigi', 'yoshi', 'toad', 'donkeykong', 'wario', 'peach', 'bowser')  # gCharacterSize / characterId order
KART_FRAMES = 321          # frames 000-320 per racer: 0-288 view angles (src/kart_dma.c groups), 289-320 tumble
KART_ATLAS_COLUMNS = 16    # public/mk64/<racer>-kart-frames.png: frame n at column n % 16, row n // 16, 64 x 64 px each


def export_kart_frames(rom, output):
    """Every pre-rendered frame of each racer (the sprite the game picks by view angle, slope, spin-out and tumble;
    native/src/experiments/kart_sprite.rs holds the `DECLARE_KART_GROUP` tables) in one 1024 x 1344 atlas per racer."""
    def data_for(kart, entry):
        start = int(entry['rom_offset'], 0)
        return mio0(rom[start:])[:entry['width'] * entry['height'] * (2 if entry['type'] == 'rgba16' else 1)]
    rows = (KART_FRAMES + KART_ATLAS_COLUMNS - 1) // KART_ATLAS_COLUMNS
    for name in KART_CHARACTERS:
        kart = json.loads((SOURCE / f'assets/karts/{name}_kart.json').read_text())
        width = 64 * KART_ATLAS_COLUMNS
        atlas = bytearray(width * 64 * rows * 4)
        for frame in range(KART_FRAMES):
            entry = kart[f'{name}_kart_frame{frame:03}']
            palette = rgba16(b''.join(data_for(kart, kart[tlut]) for tlut in entry['tlut']))
            pixels = b''.join(palette[index * 4:index * 4 + 4] for index in data_for(kart, entry))
            ox, oy = (frame % KART_ATLAS_COLUMNS) * 64, (frame // KART_ATLAS_COLUMNS) * 64
            for y in range(64):
                dest = ((oy + y) * width + ox) * 4
                atlas[dest:dest + 256] = pixels[y * 256:(y + 1) * 256]
        png(output / f'{name}-kart-frames.png', width, 64 * rows, atlas)
    print(f'Exported {KART_FRAMES} frames for {len(KART_CHARACTERS)} racers')


# standalone textures (assets.json `textures/standalone/*`): file label, ROM offset of the MIO0 block, format, size
KART_EFFECT_TEXTURES = [
    ('kart-shadow', 0x69A830, 'i8', 64, 64),          # gTextureKartShadow: render_player_shadow
    ('drift-puff-small', 0x69B03C, 'i8', 16, 16),     # D_8018D420: drift smoke while driftDuration < 50
    ('drift-puff-large', 0x69B140, 'i8', 32, 32),     # D_8018D424: drift smoke from driftDuration 50
    ('ground-dust', 0x69C4E4, 'i8', 32, 32),          # gGroundDust: tyre dust off the road
    ('smoke-puff', 0x69C354, 'ia8', 32, 32),          # gTextureSmokePuff
]


def export_kart_effect_textures(rom, output):
    for label, offset, kind, w, h in KART_EFFECT_TEXTURES:
        data = mio0(rom[offset:])
        if kind == 'i8':
            pixels = bytearray()
            for v in data[:w * h]:
                pixels.extend((255, 255, 255, v))
        else:  # IA8: intensity in the high nibble, alpha in the low one
            pixels = bytearray()
            for b in data[:w * h]:
                pixels.extend(((b >> 4) * 17,) * 3 + ((b & 15) * 17,))
        png(output / f'mk64-{label}.png', w, h, pixels)
    grass = mio0(rom[0x69B378:])[:32 * 64 * 2]  # gGrassParticle, RGBA16
    png(output / 'mk64-grass-particle.png', 32, 64, rgba16(grass))
    print(f'Exported {len(KART_EFFECT_TEXTURES) + 1} kart effect textures')


def export(rom_path, output, only=()):
    rom = rom_path.read_bytes()
    if rom[:4] == bytes.fromhex('37804012'):
        rom = bytes(b for i in range(0, len(rom), 2) for b in rom[i:i + 2][::-1])
    elif rom[:4] == bytes.fromhex('40123780'):
        rom = bytes(b for i in range(0, len(rom), 4) for b in rom[i:i + 4][::-1])
    digest = hashlib.sha1(rom).hexdigest()
    if digest != '579c48e211ae952530ffc8738709f078d5dd215e':
        raise ValueError('Expected the US Mario Kart 64 ROM (SHA-1 579c48e211ae952530ffc8738709f078d5dd215e)')
    output.mkdir(parents=True, exist_ok=True)
    textures_rom = mk64_courses.Textures(rom, SOURCE, mio0, decode)
    summary = []
    for name in mk64_courses.COURSES:
        if only and name not in only:
            continue
        kart = export_kart_course(textures_rom, name, output)
        walk = mk64_courses.export_walk(mk64_courses.Course(SOURCE, name), textures_rom, ASSETS)
        summary.append((name, kart, walk))
        print(f"{name}: kart {kart or 'none'}; walk {walk['visual_vertices']} vertices, {walk['collision_triangles']} collision triangles, "
              f"{walk['textures']} textures, {walk['glb_bytes'] // 1024} KB glb" + (f", MISSING {walk['missing_textures']}" if walk['missing_textures'] else ''))
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
    export_kart_frames(rom, output)
    export_kart_effect_textures(rom, output)
    export_hud(rom, output)
    export_item_curves(rom, output)
    export_item_sprites(rom, output)
    print(f'Exported {len(summary)} courses and 35 kart views to {output}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / 'public/mk64')
    parser.add_argument('--only', nargs='*', default=[], help='course directory names to export (default: all)')
    args = parser.parse_args()
    export(args.rom.expanduser(), args.output, set(args.only))
