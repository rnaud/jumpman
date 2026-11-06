"""Export Adult Link from a local US 1.2 ROM using zeldaret/oot asset definitions."""
import argparse
import base64
import csv
import hashlib
import json
import struct
import xml.etree.ElementTree as ET
from pathlib import Path


def decompress(data):
    if data[:4] != b'Yaz0':
        raise ValueError("Expected a Yaz0 segment")
    length, = struct.unpack_from('>I', data, 4)
    out = bytearray()
    cursor = 16
    while len(out) < length:
        flags = data[cursor]
        cursor += 1
        for bit in range(7, -1, -1):
            if flags & (1 << bit):
                out.append(data[cursor])
                cursor += 1
            else:
                a, b = data[cursor:cursor + 2]
                cursor += 2
                distance = ((a & 15) << 8) + b + 1
                count = (a >> 4) + 2
                if count == 2:
                    count = data[cursor] + 18
                    cursor += 1
                for _ in range(count):
                    out.append(out[-distance])
            if len(out) >= length:
                break
    return out


def segment(rom, names, name):
    start, end, physical, compressed_end = struct.unpack_from('>4I', rom, 0x7960 + 16 * names.index(name))
    if compressed_end:
        return decompress(rom[physical:compressed_end])
    return rom[physical:physical + end - start]


def definitions(path):
    return {el.get('Name'): el.attrib for el in ET.parse(path).getroot().find('File') if el.get('Name')}


def offset(item):
    return int(item['Offset'], 16)


def extract(rom_path, source):
    rom = rom_path.read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError("Expected a big-endian OoT US 1.2 ROM")
    with (source / 'baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    model = segment(rom, names, 'object_link_boy')
    motion = segment(rom, names, 'link_animetion')
    xml = source / 'assets/xml'
    objects = definitions(xml / 'objects/object_link_boy.xml')
    animations = definitions(xml / 'misc/link_animetion.xml')
    keep = segment(rom, names, 'gameplay_keep')
    shared = definitions(xml / 'objects/gameplay_keep.xml')
    return build(model, keep, motion, objects, shared, animations)


def rgba16(value):
    return [(value >> shift & 31) * 255 // 31 for shift in (11, 6, 1)] + [255 * (value & 1)]


def texture(data, spec):
    start = offset(spec)
    w, h = int(spec['Width']), int(spec['Height'])
    pixels = bytearray()
    for i in range(w * h):
        fmt = spec['Format']
        if fmt == 'i8':
            pixels.extend([data[start+i]] * 3 + [255])
        elif fmt == 'ia16':
            intensity, alpha = data[start+2*i:start+2*i+2]
            pixels.extend([intensity] * 3 + [alpha])
        elif fmt in ('rgba16', 'ci8'):
            p = (int(spec['TlutOffset'], 16) + 2 * data[start+i]
                 if fmt == 'ci8' else start + 2*i)
            pixels.extend(rgba16(struct.unpack_from('>H', data, p)[0]))
        else:
            raise ValueError(fmt)
    return dict(width=w, height=h, rgba=base64.b64encode(pixels).decode())


def build(model, keep, motion, objects, shared, animations):
    textures = {}
    specs = {seg + offset(v): (data, v) for seg, data, entries in
             [(0x06000000, model, objects), (0x04000000, keep, shared)]
             for v in entries.values() if 'Format' in v and 'Width' in v and v.get('Offset', '').startswith('0x')}
    limbs = []
    matrices = []
    lists = []
    for i in range(21):
        x, y, z, child, sibling, dl = struct.unpack_from('>hhhBBI', model, offset(objects['gLinkAdultRootLimb']) + 16*i)
        limbs.append(dict(pos=[x,y,z], child=child, sibling=sibling))
        if dl:
            matrices.append(i)
            lists.append((i, dl & 0xffffff, 'base'))
    # The stock player draw callback substitutes equipment on the hand and back limbs.
    equipment = {
        15: ('LeftHandNear', 'LeftHandHoldingMasterSwordNear'),
        19: ('BackHylianShieldSwordInSheathNear', 'BackMasterSheathNear'),
    }
    equipment[18] = ('RightHandNear', 'RightHandHoldingHylianShieldNear')
    lists = [item for item in lists if item[0] not in equipment]
    for limb, variants in equipment.items():
        for mode, suffix in zip(('sheathed', 'drawn'), variants):
            lists.append((limb, offset(objects['gLinkAdult'+suffix+'DL']), mode))

    batches = {}
    for limb, p, mode in lists:
        cache = {}
        wrap_s = wrap_t = 0
        bone = limb
        tex = None
        color = 0xffffff
        while True:
            a, b = struct.unpack_from('>II', model, p)
            p += 8
            op = a >> 24
            if op == 0xdf:
                break
            if op == 0xda:
                bone = matrices[(b & 0xffffff) // 64]
            elif op == 0xfd:
                addr = {0x08000000: 0x06000000, 0x09000000: 0x06004000}.get(b, b)
                data, spec = specs[addr]
                if 'tlut' not in spec['Name'].lower():
                    tex = spec['Name']
                    if tex not in textures:
                        textures[tex] = texture(data, spec)
            elif op == 0xf5 and b >> 24 == 0:
                wrap_s, wrap_t = (b >> 8) & 3, (b >> 18) & 3
            elif op == 0xfa:
                color = b >> 8
            elif op == 1:
                n = (a >> 12) & 255
                first = ((a & 255) >> 1) - n
                for i in range(n):
                    v = struct.unpack_from('>hhh2xhhbbbB', model, (b & 0xffffff) + i*16)
                    cache[first+i] = [*v[:7], v[7], bone]
            elif op in (5, 6):
                batch = batches.setdefault((mode, tex, color, wrap_s, wrap_t), [])
                for word in ([a] if op == 5 else [a, b]):
                    batch.extend(cache[(word >> shift & 255) // 2] for shift in (16,8,0))
            elif op == 0xde and b >> 24 != 12:
                raise ValueError(f'Unsupported nested display list {b:x}')

    clips = {}
    for key, suffix in dict(backflip='fighter_backturn_jump', leftHop='fighter_Lside_jump',
                            rightHop='fighter_Rside_jump', strafe='fighter_side_walk_long', guard='fighter_defense_wait', idle='normal_wait_free', walk='normal_walk_free', run='normal_run_free',
                            roll='normal_landing_roll_free', jump='normal_run_jump', fall='normal_fall_wait',
                            focus='fighter_waitL_long', finish='fighter_normal_kiru_finsh',
                            finishRecover='fighter_normal_kiru_finsh_end', slash='fighter_normal_kiru', recover='fighter_normal_kiru_end',
                            swim='swimer_swim', hang='normal_jump_climb_wait_free').items():
        entry = animations['gPlayerAnim_link_'+suffix+'_Data']
        # 22 Vec3s (root translation, 21 limb rotations), then two face-expression bytes.
        clips[key] = [struct.unpack_from('>66h', motion, offset(entry) + i*134)
                      for i in range(int(entry['FrameCount']))]
    return dict(limbs=limbs, textures=textures, clips=clips, batches=[
        dict(mode=mode, texture=tex, color=color, wrap=[wrap_s,wrap_t], vertices=vertices)
        for (mode, tex, color, wrap_s, wrap_t), vertices in batches.items()])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    root = Path(__file__).resolve().parents[1]
    parser.add_argument('rom', nargs='?', type=Path, default=root / 'reference/roms/oot-us-1.2.z64')
    parser.add_argument('decomp', nargs='?', type=Path, default=root / 'reference/oot')
    args = parser.parse_args()
    asset = extract(args.rom, args.decomp)
    out = root / 'public/models/oot/link.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(asset, separators=(',', ':')))
    print(f'Exported {len(asset["textures"])} textures and {len(asset["clips"])} clips to {out}')
