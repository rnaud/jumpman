"""Export Banjo & Kazooie from a local US 1.0 ROM using n64decomp/banjo-kazooie layouts.

The asset table, model binary, geo-command, F3DEX display list and animation
formats follow include/core2/model.h, include/core2/animationfile.h and
src/core2/{modelRender,animationfile,code_630D0}.c in the decompilation.
"""
import argparse
import base64
import hashlib
import json
import re
import struct
import zlib
from pathlib import Path

ASSET_TABLE = 0x5E90  # decompressed.us.v10.yaml: [0x5E90, bin, assets]
MODEL = 0x34E  # ASSET_34E_MODEL_BANJOKAZOOIE_HIGH_POLY

# Appendage selectors set by modelAppendages_loadAppendage for the high-poly model.
PARTS = {1: 'kazooieUpper', 2: 'kazooieFeet', 8: 'kazooieAss'}
# Eyes open (0x1A/0x1B), plain feet without Turbo Trainers or Wading Boots.
FIXED_SELECTORS = {0x1A: 1, 0x1B: 1, 0x12: 1, 0x13: 1, 0x22: 1, 0x23: 1}

# Clip name -> (asset enum suffix, seconds per playthrough from the src/core2/bs/*.c states).
CLIPS = {
    'idle': ('6F_ANIM_BSSTAND_IDLE', 5.5),
    'idlePeck': ('95_ANIM_BSSTAND_KAZOOIE_PECK', 5.5),
    'creep': ('2_ANIM_BSWALK_CREEP', 0.8),
    'walk': ('3_ANIM_BSWALK', 0.43),
    'run': ('C_ANIM_BSWALK_RUN', 0.66),
    'turn': ('E_ANIM_BSTURN', 0.5),
    'jump': ('8_ANIM_BSJUMP', 1.9),
    'fall': ('B0_ANIM_BSJUMP_FALL', 0.38),
    'tumble': ('68_ANIM_BSJUMP_TUMBLE', 0.35),
    'flapEnter': ('18_ANIM_BSBFLAP_ENTER', 0.3),
    'flap': ('17_ANIM_BSBFLAP', 0.3),
    'flipEnter': ('4B_ANIM_BSBFLIP_ENTER', 2.2),
    'flipHold': ('4C_ANIM_BSBFLIP_HOLD', 0.13),
    'flipExit': ('61_ANIM_BSBFLIP_EXIT', 0.8),
    'claw': ('5_ANIM_BSPUNCH', 1.3),
    'roll': ('4F_ANIM_BSTWIRL', 0.9),
    'peckEnter': ('19_ANIM_BSBPECK_ENTER', 0.35),
    'peck': ('1A_ANIM_BSBPECK', 0.2),
    'barge': ('1C_ANIM_BSBBARGE', 1.0),
    'bust': ('1D_ANIM_BSBBUSTER', 1.02),
    'crouchEnter': ('1_ANIM_BSCROUCH_ENTER', 0.5),
    'crouch': ('10C_ANIM_BSCROUCH_IDLE', 0.5),
    'trotIdle': ('26_ANIM_BSBTROT_IDLE', 1.2),
    'trot': ('15_ANIM_BSBTROT_WALK', 0.45),
    'climb': ('A_ANIM_BSCLIMB_MOVE', 0.9),
    'climbIdle': ('B2_ANIM_BSCLIMB_IDLE_2', 2.64),
    'swimIdle': ('57_ANIM_BSSWIM_IDLE', 1.2),
    'swim': ('39_ANIM_BSSWIM_MOVE', 0.8),
    'dive': ('3F_ANIM_BSSWIM_DIVE_MOVE', 0.8),
    'slideFront': ('5A_ANIM_BSSLIDE_FRONT', 0.6),
    'slideBack': ('59_ANIM_BSSLIDE_BACK', 0.6),
    'ow': ('4D_ANIM_BSOW', 1.7),
    'rebound': ('F_ANIM_BSREBOUND', 1.0),
    'die': ('9_ANIM_BSDIE', 2.0),
    'jiggy': ('2E_ANIM_BSJIG_JIGGY', 3.0),
    'carry': ('73_ANIM_BSCARRY_WALK', 0.8),
    'carryIdle': ('72_ANIM_BSCARRY_IDLE', 1.0),
    'throw': ('11B_ANIM_BSTHROW', 0.6),
}


def assets(rom):
    count, = struct.unpack_from('>I', rom, ASSET_TABLE)
    table = ASSET_TABLE + 8
    data = table + 8 * count

    def read(index):
        offset, compressed, _ = struct.unpack_from('>IHH', rom, table + 8 * index)
        end, = struct.unpack_from('>I', rom, table + 8 * (index + 1))
        raw = rom[data + offset:data + end]
        if not compressed:
            return raw
        if raw[:2] != b'\x11\x72':
            raise ValueError(f'asset {index:#x} is not rarezip data')
        size, = struct.unpack_from('>I', raw, 2)
        out = zlib.decompressobj(-15).decompress(raw[6:])
        if len(out) != size:
            raise ValueError(f'asset {index:#x}: {len(out)} != {size}')
        return out
    return read


def rgba16(value):
    return [(value >> shift & 31) * 255 // 31 for shift in (11, 6, 1)] + [255 * (value & 1)]


def build_model(m):
    (_, geo, tex_list, _, gfx, vtx, _, anim_list, *_rest) = struct.unpack_from('>IiHHiiiiiiiiiHHf', m, 0)
    trans_scale, bone_count = struct.unpack_from('>fh', m, anim_list)
    bones = []
    for i in range(bone_count):
        x, y, z, bone_id, parent = struct.unpack_from('>fffhh', m, anim_list + 8 + 16 * i)
        bones.append(dict(pivot=[round(x, 3), round(y, 3), round(z, 3)], id=bone_id, parent=parent))

    _, tex_count = struct.unpack_from('>ih', m, tex_list)
    tex_data = tex_list + 8 + 16 * tex_count
    vertices = vtx + 0x18
    commands = gfx + 8

    textures = {}

    def texture(image, tlut, fmt, siz, width, height):
        key = f'{image:x}'
        if key in textures:
            return key
        pixels = bytearray()
        for i in range(width * height):
            if fmt == 2 and siz == 0:
                byte = m[tex_data + image + i // 2]
                index = byte >> 4 if i % 2 == 0 else byte & 15
                value, = struct.unpack_from('>H', m, tex_data + tlut + 2 * index)
            elif fmt == 2 and siz == 1:
                index = m[tex_data + image + i]
                value, = struct.unpack_from('>H', m, tex_data + tlut + 2 * index)
            elif fmt == 0 and siz == 2:
                value, = struct.unpack_from('>H', m, tex_data + image + 2 * i)
            else:
                raise ValueError(f'texture format {fmt}/{siz}')
            pixels.extend(rgba16(value))
        textures[key] = dict(width=width, height=height, rgba=base64.b64encode(pixels).decode())
        return key

    batches = {}
    cache = [None] * 64
    state = dict(tex_on=False, scale=(1.0, 1.0), timg=0, tlut=0, fmt=0, siz=0, wrap=(0, 0),
                 width=32, height=32, texgen=False)

    def run_dl(index, bone, part):
        p = commands + 8 * index
        while True:
            a, b = struct.unpack_from('>II', m, p)
            p += 8
            op = a >> 24
            if op == 0xB8:
                return
            if op == 0x04:
                n = (a >> 10) & 0x3F
                first = ((a >> 16) & 0xFF) // 2
                for i in range(n):
                    x, y, z, _, s, t, r, g, bl, al = struct.unpack_from('>hhhHhhBBBB', m, vertices + (b & 0xFFFFFF) + 16 * i)
                    cache[first + i] = (x, y, z, s, t, r, g, bl, al, bone)
            elif op in (0xBF, 0xB1):
                words = [b] if op == 0xBF else [a, b]
                textured = state['tex_on'] and not state['texgen']
                key = (part, texture(state['timg'], state['tlut'], state['fmt'], state['siz'], state['width'], state['height']) if textured else None, state['wrap'])
                out = batches.setdefault(key, [])
                for word in words:
                    for shift in (16, 8, 0):
                        x, y, z, s, t, r, g, bl, al, bn = cache[(word >> shift & 0xFF) // 2]
                        u = v = 0
                        if textured:
                            u = round((s * state['scale'][0] / 32 + 0.5) / state['width'], 4)
                            v = round((t * state['scale'][1] / 32 + 0.5) / state['height'], 4)
                        out.append([x, y, z, u, v, r, g, bl, al, bn])
            elif op == 0xBB:
                state['tex_on'] = bool(a & 0xFF)
                state['scale'] = ((b >> 16) / 65536, (b & 0xFFFF) / 65536)
            elif op == 0xB7:
                if b & 0x40000:
                    state['texgen'] = True
            elif op == 0xB6:
                if b & 0x40000:
                    state['texgen'] = False
            elif op == 0xFD:
                state['timg'] = b & 0xFFFFFF
            elif op == 0xF0:
                state['tlut'] = state['timg']
            elif op == 0xF5:
                if (b >> 24) & 7 == 0:
                    state['fmt'], state['siz'] = (a >> 21) & 7, (a >> 19) & 3
                    state['wrap'] = ((b >> 8) & 3, (b >> 18) & 3)
            elif op == 0xF2:
                if (b >> 24) & 7 == 0:
                    state['width'] = ((b >> 12) & 0xFFF) // 4 + 1
                    state['height'] = (b & 0xFFF) // 4 + 1
            elif op == 0x06 and b >> 24 != 3:
                raise ValueError(f'nested display list {b:x}')

    selectors = dict(FIXED_SELECTORS)

    def geo(p, bone, part):
        while True:
            cmd, nxt = struct.unpack_from('>Ii', m, p)
            if cmd == 0x02:
                branch, mtx = struct.unpack_from('>Bb', m, p + 8)
                if branch:
                    geo(p + branch, mtx, part)
            elif cmd in (0x03,):
                run_dl(struct.unpack_from('>h', m, p + 8)[0], bone, part)
            elif cmd == 0x07:
                run_dl(struct.unpack_from('>h', m, p + 10)[0], bone, part)
            elif cmd == 0x05:
                q = p + 8
                first = True
                while True:
                    index, = struct.unpack_from('>h', m, q)
                    q += 2
                    if index == 0 and not first:
                        break
                    first = False
                    run_dl(index, bone, part)
            elif cmd == 0x06:
                geo(p + struct.unpack_from('>i', m, p + 8)[0], bone, part)
            elif cmd == 0x08:
                # Keep only the nearest level of detail.
                lod_max, lod_min = struct.unpack_from('>ff', m, p + 8)
                branch, = struct.unpack_from('>i', m, p + 28)
                if lod_min == 0 and branch:
                    geo(p + branch, bone, part)
            elif cmd == 0x0C:
                count, index = struct.unpack_from('>hh', m, p + 8)
                if index in PARTS:
                    geo(p + struct.unpack_from('>i', m, p + 12)[0], bone, PARTS[index])
                else:
                    choice = selectors.get(index, 0)
                    if 0 < choice <= count:
                        geo(p + struct.unpack_from('>i', m, p + 12 + 4 * (choice - 1))[0], bone, part)
            elif cmd == 0x0D:
                branch, = struct.unpack_from('>h', m, p + 20)
                if branch:
                    geo(p + branch, bone, part)
            elif cmd not in (0x0A, 0x04, 0x09, 0x0B, 0x10):
                raise ValueError(f'unsupported geo command {cmd:#x}')
            if nxt == 0:
                return
            p += nxt

    geo(geo_offset(m), -1, 'body')
    return dict(transScale=trans_scale, bones=bones, textures=textures, batches=[
        dict(part=part, texture=tex, wrap=list(wrap), vertices=verts)
        for (part, tex, wrap), verts in batches.items()])


def geo_offset(m):
    return struct.unpack_from('>i', m, 4)[0]


def build_clip(a, duration):
    start, end, count = struct.unpack_from('>hhh', a, 0)
    p = 8
    elements = []
    for _ in range(count):
        head, keys = struct.unpack_from('>Hh', a, p)
        p += 4
        data = []
        for _ in range(keys):
            flags, value = struct.unpack_from('>Hh', a, p)
            p += 4
            data += [flags >> 14, flags & 0x3FFF, value]
        elements.append([head >> 4, head & 15, data])
    return dict(start=start, end=end, duration=duration, elements=elements)


def enum_ids(source):
    text = (source / 'include/enums.h').read_text()
    body = text[text.index('enum asset_e'):]
    body = body[:body.index('};')]
    ids, value = {}, -1
    for name, explicit in re.findall(r'(ASSET_\w+)\s*(?:=\s*(0x[0-9A-Fa-f]+|\d+))?\s*,', body):
        value = int(explicit, 0) if explicit else value + 1
        ids[name] = value
    return ids


def extract(rom_path, source):
    rom = rom_path.read_bytes()
    if hashlib.md5(rom).hexdigest() != 'b29599651a13f681c9923d69354bf4a3':
        raise ValueError('Expected a big-endian Banjo-Kazooie US 1.0 ROM')
    read = assets(rom)
    ids = enum_ids(source)
    asset = build_model(read(MODEL))
    asset['clips'] = {}
    for name, (suffix, duration) in CLIPS.items():
        index = ids['ASSET_' + suffix]
        asset['clips'][name] = build_clip(read(index), duration)
    return asset


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    root = Path(__file__).resolve().parents[1]
    parser.add_argument('rom', nargs='?', type=Path, default=root / 'reference/roms/bk-us-1.0.z64')
    parser.add_argument('decomp', nargs='?', type=Path, default=root / 'reference/banjo-kazooie')
    args = parser.parse_args()
    asset = extract(args.rom, args.decomp)
    out = root / 'public/models/bk/banjo.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(asset, separators=(',', ':')))
    tris = sum(len(b['vertices']) for b in asset['batches']) // 3
    print(f'Exported {tris} triangles, {len(asset["bones"])} bones, {len(asset["textures"])} textures '
          f'and {len(asset["clips"])} clips to {out}')
