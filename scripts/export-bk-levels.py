"""Export Banjo-Kazooie worlds from a local US 1.0 ROM as Jumpman courses.

Each world is two level models in the ROM asset table (an opaque `*_OPA` and a translucent `*_XLU`
model, listed in src/core2/mapModel.c of n64decomp/banjo-kazooie). The display lists become one
textured, vertex-coloured glTF (assets/levels/<id>/terrain.glb) and the OPA/XLU collision lists
(include/core2/model.h `BKCollisionList`) become the SM64-style triangle soup in
assets/courses/<id>.json, so Mario, Link, Young Link and Banjo all walk the world with the SM64
physics engine. Shares the rarezip reader with export-bk-banjo.py.
"""
import argparse
import hashlib
import json
import math
import runpy
import struct
import zlib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
bk_banjo = runpy.run_path(str(ROOT / 'scripts/export-bk-banjo.py'))

# BK world units -> Jumpman units. The Banjo rig is drawn at 1.25 (characters/mod.rs).
WORLD_SCALE = 1.25

SKIES = {
    'bk_mad_monster_mansion': dict(top=[0.08, 0.08, 0.15], mid=[0.15, 0.15, 0.25], bottom=[0.2, 0.2, 0.3], fog=[0.12, 0.12, 0.2], fogStart=4000, fogEnd=20000),
    'bk_click_clock_wood': dict(top=[0.35, 0.6, 0.9], mid=[0.6, 0.8, 0.95], bottom=[0.85, 0.93, 1.0], fog=[0.7, 0.86, 1.0], fogStart=12000, fogEnd=40000),
    'bk_click_clock_wood_spring': dict(top=[0.4, 0.65, 0.95], mid=[0.65, 0.85, 0.98], bottom=[0.9, 0.95, 1.0], fog=[0.75, 0.9, 1.0], fogStart=12000, fogEnd=40000),
    'bk_click_clock_wood_summer': dict(top=[0.3, 0.58, 0.95], mid=[0.55, 0.8, 0.98], bottom=[0.85, 0.94, 1.0], fog=[0.7, 0.88, 1.0], fogStart=12000, fogEnd=40000),
    'bk_click_clock_wood_autumn': dict(top=[0.7, 0.55, 0.45], mid=[0.88, 0.72, 0.55], bottom=[0.95, 0.85, 0.7], fog=[0.85, 0.7, 0.55], fogStart=10000, fogEnd=36000),
    'bk_click_clock_wood_winter': dict(top=[0.65, 0.75, 0.9], mid=[0.82, 0.88, 0.96], bottom=[0.94, 0.96, 1.0], fog=[0.88, 0.92, 0.98], fogStart=8000, fogEnd=30000),
    'bk_spiral_mountain': dict(top=[0.3, 0.55, 0.9], mid=[0.55, 0.78, 0.97], bottom=[0.85, 0.93, 1.0], fog=[0.75, 0.88, 1.0], fogStart=14000, fogEnd=42000),
    'bk_mumbos_mountain': dict(top=[0.35, 0.55, 0.55], mid=[0.5, 0.65, 0.5], bottom=[0.65, 0.75, 0.55], fog=[0.45, 0.58, 0.42], fogStart=7000, fogEnd=26000),
    'bk_treasure_trove_cove': dict(top=[0.25, 0.55, 0.95], mid=[0.55, 0.8, 0.98], bottom=[0.85, 0.95, 1.0], fog=[0.7, 0.88, 1.0], fogStart=18000, fogEnd=52000),
    'bk_clankers_cavern': dict(top=[0.04, 0.07, 0.05], mid=[0.05, 0.09, 0.06], bottom=[0.06, 0.1, 0.07], fog=[0.04, 0.07, 0.05], fogStart=3500, fogEnd=17000),
    'bk_bubblegloop_swamp': dict(top=[0.25, 0.3, 0.2], mid=[0.4, 0.5, 0.3], bottom=[0.5, 0.55, 0.35], fog=[0.34, 0.42, 0.27], fogStart=5000, fogEnd=22000),
    'bk_gobis_valley': dict(top=[0.45, 0.6, 0.85], mid=[0.85, 0.8, 0.65], bottom=[0.95, 0.85, 0.6], fog=[0.9, 0.8, 0.6], fogStart=12000, fogEnd=40000),
    'bk_freezeezy_peak': dict(top=[0.55, 0.65, 0.85], mid=[0.8, 0.85, 0.95], bottom=[0.92, 0.95, 1.0], fog=[0.85, 0.9, 0.97], fogStart=9000, fogEnd=32000),
    'bk_rusty_bucket_bay': dict(top=[0.08, 0.1, 0.14], mid=[0.18, 0.2, 0.22], bottom=[0.3, 0.3, 0.3], fog=[0.15, 0.17, 0.2], fogStart=6000, fogEnd=26000),
}

# id -> dict(name, opa asset, xlu asset, spawn hint, sky colour, note)
WORLDS = {
    'bk_spiral_mountain': dict(name='Spiral Mountain', opa=0x14CF, xlu=0x14D0, sky=SKIES['bk_spiral_mountain'], spawn=[6400, 520, 0],
                               note='Banjo-Kazooie: the training grounds outside Gruntilda’s lair. Geometry and collision only.'),
    'bk_mumbos_mountain': dict(name='Mumbo’s Mountain', opa=0x14AA, xlu=0x14AB, sky=SKIES['bk_mumbos_mountain'],
                               note='Banjo-Kazooie: the termite-hill mountain. Geometry and collision only.'),
    'bk_treasure_trove_cove': dict(name='Treasure Trove Cove', opa=0x146B, xlu=0x146C, sky=SKIES['bk_treasure_trove_cove'],
                                   note='Banjo-Kazooie: the pirate beach. Geometry and collision only.'),
    'bk_clankers_cavern': dict(name='Clanker’s Cavern', opa=0x14ED, xlu=0x14EE, sky=SKIES['bk_clankers_cavern'],
                               note='Banjo-Kazooie: the polluted bay. Geometry and collision only.'),
    'bk_bubblegloop_swamp': dict(name='Bubblegloop Swamp', opa=0x14D1, xlu=0x14D2, sky=SKIES['bk_bubblegloop_swamp'],
                                 note='Banjo-Kazooie: Mr. Vile’s swamp. Geometry and collision only.'),
    'bk_gobis_valley': dict(name='Gobi’s Valley', opa=0x1474, xlu=0x1475, sky=SKIES['bk_gobis_valley'],
                            note='Banjo-Kazooie: the desert valley. Geometry and collision only.'),
    'bk_freezeezy_peak': dict(name='Freezeezy Peak', opa=0x14C8, xlu=0x14C9, sky=SKIES['bk_freezeezy_peak'],
                              note='Banjo-Kazooie: the snowy peak. Geometry and collision only.'),
    'bk_rusty_bucket_bay': dict(name='Rusty Bucket Bay', opa=0x14B0, xlu=0x14B1, sky=SKIES['bk_rusty_bucket_bay'],
                                note='Banjo-Kazooie: the industrial harbour. Geometry and collision only.'),
    'bk_mad_monster_mansion': dict(name='Mad Monster Mansion', opa=0x147F, xlu=0x1480, sky=SKIES['bk_mad_monster_mansion'],
                  note='Banjo-Kazooie: the haunted graveyard and mansion grounds. Geometry and collision only.'),
    'bk_click_clock_wood': dict(name='Click Clock Wood: Tree Trunk', opa=0x14D8, xlu=0x14E3, sky=SKIES['bk_click_clock_wood'],
                  note='Banjo-Kazooie: the giant tree’s hub. Geometry and collision only.'),
    'bk_click_clock_wood_spring': dict(name='Click Clock Wood: Spring', opa=0x14D9, xlu=0x14E4, sky=SKIES['bk_click_clock_wood_spring'],
                  note='Banjo-Kazooie: the spring branches. Geometry and collision only.'),
    'bk_click_clock_wood_summer': dict(name='Click Clock Wood: Summer', opa=0x14DA, xlu=0x14E5, sky=SKIES['bk_click_clock_wood_summer'],
                  note='Banjo-Kazooie: the summer branches. Geometry and collision only.'),
    'bk_click_clock_wood_autumn': dict(name='Click Clock Wood: Autumn', opa=0x14DB, xlu=0x14E6, sky=SKIES['bk_click_clock_wood_autumn'],
                  note='Banjo-Kazooie: the autumn branches. Geometry and collision only.'),
    'bk_click_clock_wood_winter': dict(name='Click Clock Wood: Winter', opa=0x14DC, xlu=0x14E7, sky=SKIES['bk_click_clock_wood_winter'],
                  note='Banjo-Kazooie: the winter branches. Geometry and collision only.'),
}

ASSET_TABLE_FLAGS = None


def f(m, fmt, off):
    return struct.unpack_from(fmt, m, off)


# ---------------------------------------------------------------------------------------------
# Textures


def rgba16(v):
    return (round((v >> 11 & 31) * 255 / 31), round((v >> 6 & 31) * 255 / 31), round((v >> 1 & 31) * 255 / 31), 255 * (v & 1))


def decode_texture(m, base, image, tlut, fmt, siz, width, height):
    """N64 image at `image` (offset in the texture block) -> RGBA bytes."""
    px = bytearray()
    n = width * height
    if fmt == 0 and siz == 2:
        for i in range(n):
            px.extend(rgba16(f(m, '>H', base + image + 2 * i)[0]))
    elif fmt == 0 and siz == 3:
        px.extend(m[base + image:base + image + 4 * n])
    elif fmt == 2 and siz in (0, 1):
        pal = [rgba16(f(m, '>H', base + tlut + 2 * k)[0]) for k in range(16 if siz == 0 else 256)]
        for i in range(n):
            if siz == 0:
                byte = m[base + image + i // 2]
                idx = byte >> 4 if i % 2 == 0 else byte & 15
            else:
                idx = m[base + image + i]
            px.extend(pal[idx])
    elif fmt == 3:  # IA
        for i in range(n):
            if siz == 0:
                byte = m[base + image + i // 2]
                v = byte >> 4 if i % 2 == 0 else byte & 15
                c, a = (v >> 1) * 255 // 7, 255 * (v & 1)
            elif siz == 1:
                v = m[base + image + i]
                c, a = (v >> 4) * 17, (v & 15) * 17
            else:
                c, a = m[base + image + 2 * i], m[base + image + 2 * i + 1]
            px.extend((c, c, c, a))
    elif fmt == 4:  # I
        for i in range(n):
            if siz == 0:
                byte = m[base + image + i // 2]
                v = (byte >> 4 if i % 2 == 0 else byte & 15) * 17
            else:
                v = m[base + image + i]
            px.extend((v, v, v, 255))
    else:
        raise ValueError(f'texture format {fmt}/{siz}')
    return bytes(px)


def png(width, height, rgba):
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data))
    rows = b''.join(b'\0' + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)) +
            chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b''))


# ---------------------------------------------------------------------------------------------
# Model parsing


class Level:
    """One BK level model binary (header per include/core2/model.h)."""

    def __init__(self, m):
        self.m = m
        (self.magic, self.geo, self.tex_list, _geo_type, self.gfx, self.vtx, _u14, _anim, self.coll,
         *_rest) = struct.unpack_from('>IiHHiiiiiiiiiHHf', m, 0)
        self.vtx_base = self.vtx + 0x18
        self.cmd_base = self.gfx + 8
        _, count = f(m, '>ih', self.tex_list)
        self.tex_count = count
        self.tex_data = self.tex_list + 8 + 16 * count
        self.tex_infos = sorted(
            (f(m, '>i', self.tex_list + 8 + 16 * k)[0], f(m, '>h', self.tex_list + 8 + 16 * k + 4)[0],
             m[self.tex_list + 8 + 16 * k + 8], m[self.tex_list + 8 + 16 * k + 9]) for k in range(count))
        self.vcount = f(m, '>h', self.vtx + 0x12)[0]

    def vertex(self, i):
        x, y, z, _flag, s, t, r, g, b, a = f(self.m, '>hhhHhhBBBB', self.vtx_base + 16 * i)
        return x, y, z, s, t, r, g, b, a

    def info_for(self, image):
        best = None
        for info in self.tex_infos:
            if info[0] <= image:
                best = info
        return best

    # -- display lists -------------------------------------------------------------------------
    def geometry(self, selectors=None, skip=frozenset()):
        """Walk the geo layout; return {(texture key): [vertices]} with N64 state resolved."""
        m = self.m
        out = defaultdict(list)
        textures = {}
        state = dict(tex_on=False, scale=(1.0, 1.0), timg=0, tlut=0,
                     texgen=False, lighting=False, tex_tile=0, tiles={})
        cache = [None] * 64
        selectors = selectors or {}

        def texture(image, tile):
            """Resolve the image the SETTIMG/tile state draws with. The texture list's own
            type/size wins; the SETTILE state is the fallback."""
            fmt, siz, wrap, tw, th = tile
            for off, kind, iw, ih in self.tex_infos:
                pal = 32 if kind & 1 else 512 if kind & 2 else 0
                if image == off + pal and iw and ih:
                    fmt, siz = ((2, 0) if kind & 1 else (2, 1) if kind & 2 else (0, 2) if kind & 4 else (0, 3))
                    key = (off, pal, fmt, siz, iw, ih)
                    if key not in textures:
                        textures[key] = (iw, ih, decode_texture(m, self.tex_data, image, off, fmt, siz, iw, ih))
                    return key, textures[key], wrap
            key = (image, state['tlut'], fmt, siz, tw, th)
            if key not in textures:
                textures[key] = (tw, th, decode_texture(m, self.tex_data, image, state['tlut'], fmt, siz, tw, th))
            return key, textures[key], wrap

        def run_dl(index):
            p = self.cmd_base + 8 * index
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
                        cache[first + i] = self.vertex((b & 0xFFFFFF) // 16 + i) + ((b & 0xFFFFFF) // 16 + i,)
                elif op in (0xBF, 0xB1):
                    words = [b] if op == 0xBF else [a, b]
                    textured = state['tex_on'] and not state['texgen']
                    if textured:
                        tile = state['tiles'].get(state['tex_tile']) or state['tiles'].get(7) or (0, 2, (0, 0), 32, 32)
                        key, (w, h, _), wrap = texture(state['timg'], tile)
                        mat = (key, wrap)
                    else:
                        w = h = 1
                        mat = (None, (0, 0))
                    bucket = out[mat]
                    for word in words:
                        tri = []
                        corners = [cache[(word >> shift & 0xFF) // 2] for shift in (16, 8, 0)]
                        if tuple(sorted(c[9] for c in corners)) in skip:
                            continue
                        for x, y, z, s, t, r, g, bl, al, _vi in corners:
                            u = v = 0.0
                            if textured:
                                u = (s * state['scale'][0] / 32 + 0.5) / w
                                v = (t * state['scale'][1] / 32 + 0.5) / h
                            if state['lighting']:
                                r = g = bl = 255
                            tri.append((x, y, z, u, v, r, g, bl, al))
                        bucket.append(tri)
                elif op == 0xBB:
                    state['tex_on'] = bool(a & 0xFF)
                    state['tex_tile'] = (a >> 8) & 7
                    state['scale'] = ((b >> 16) / 65536, (b & 0xFFFF) / 65536)
                elif op == 0xB7:
                    if b & 0x40000:
                        state['texgen'] = True
                    if b & 0x20000:
                        state['lighting'] = True
                elif op == 0xB6:
                    if b & 0x40000:
                        state['texgen'] = False
                    if b & 0x20000:
                        state['lighting'] = False
                elif op == 0xFD:
                    state['timg'] = b & 0xFFFFFF
                elif op == 0xF0:
                    state['tlut'] = state['timg']
                elif op == 0xF5:
                    t = (b >> 24) & 7
                    old = state['tiles'].get(t, (0, 2, (0, 0), 32, 32))
                    state['tiles'][t] = ((a >> 21) & 7, (a >> 19) & 3, ((b >> 8) & 3, (b >> 18) & 3), old[3], old[4])
                elif op == 0xF2:
                    t = (b >> 24) & 7
                    old = state['tiles'].get(t, (0, 2, (0, 0), 32, 32))
                    state['tiles'][t] = old[:3] + (((b >> 12) & 0xFFF) // 4 + 1, (b & 0xFFF) // 4 + 1)

        seen = set()

        def geo(p):
            while True:
                cmd, nxt = f(m, '>Ii', p)
                if (p, cmd) in seen:
                    return
                seen.add((p, cmd))
                if cmd == 0x00:
                    br = f(m, '>h', p + 8)[0]
                    if br:
                        geo(p + br)
                elif cmd == 0x01:  # SORT: both sides
                    b1, b2 = f(m, '>hi', p + 34)
                    if b1:
                        geo(p + b1)
                    if b2:
                        geo(p + b2)
                elif cmd == 0x02:
                    br = m[p + 8]
                    if br:
                        geo(p + br)
                elif cmd == 0x03:
                    run_dl(f(m, '>h', p + 8)[0])
                elif cmd == 0x07:
                    run_dl(f(m, '>h', p + 10)[0])
                elif cmd == 0x05:
                    q = p + 8
                    first = True
                    while True:
                        idx = f(m, '>h', q)[0]
                        q += 2
                        if idx == 0 and not first:
                            break
                        first = False
                        run_dl(idx)
                elif cmd == 0x06:
                    geo(p + f(m, '>i', p + 8)[0])
                elif cmd == 0x08:
                    lod_max, lod_min = f(m, '>ff', p + 8)
                    br = f(m, '>i', p + 28)[0]
                    if lod_min == 0 and br:
                        geo(p + br)
                elif cmd == 0x0C:
                    count, index = f(m, '>hh', p + 8)
                    choice = selectors.get(index, 0)
                    if 0 < choice <= count:
                        geo(p + f(m, '>i', p + 12 + 4 * (choice - 1))[0])
                elif cmd == 0x0D:
                    br = f(m, '>h', p + 20)[0]
                    if br:
                        geo(p + br)
                elif cmd == 0x0E:
                    br = f(m, '>h', p + 16)[0]
                    if br:
                        geo(p + br)
                elif cmd == 0x0F:
                    br = f(m, '>h', p + 8)[0]
                    if br:
                        geo(p + br)
                elif cmd not in (0x0A, 0x04, 0x09, 0x0B, 0x10):
                    raise ValueError(f'unsupported geo command {cmd:#x} at {p:#x}')
                if nxt == 0:
                    return
                p += nxt

        geo(self.geo)
        return out, textures

    # -- collision -----------------------------------------------------------------------------
    def water_triangles(self):
        """Vertex-index triples of the collision triangles flagged as water surface."""
        return frozenset(tuple(sorted(t[:3])) for t in self.collision() if t[3] & WATER)

    def collision(self):
        """Distinct (v0, v1, v2, flags) collision triangles with vertices in model units."""
        m = self.m
        if not self.coll:
            return []
        c = self.coll
        mn = f(m, '>hhh', c)
        mx = f(m, '>hhh', c + 6)
        ystride, zstride, geo_count, scale, tri_count = f(m, '>hhhhh', c + 12)
        cells = c + 0x18
        tris = cells + 4 * geo_count
        seen = {}
        for g in range(geo_count):
            start, n = f(m, '>hh', cells + 4 * g)
            for t in range(start, start + n):
                a, b, cc, _unk, flags = f(m, '>hhhhi', tris + 12 * t)
                key = (a, b, cc)
                if key not in seen:
                    seen[key] = flags & 0xFFFFFFFF
        return [(a, b, cc, fl) for (a, b, cc), fl in seen.items()]


# ---------------------------------------------------------------------------------------------
# glTF


class Glb:
    def __init__(self):
        self.data = bytearray()
        self.doc = dict(asset=dict(version='2.0', extras=dict(sm64UnitsToMeters=1)), buffers=[], bufferViews=[],
                        accessors=[], images=[], textures=[], samplers=[], materials=[],
                        meshes=[dict(primitives=[])], nodes=[dict(mesh=0)], scenes=[dict(nodes=[0])], scene=0,
                        extensionsUsed=['KHR_materials_unlit'])

    def view(self, data, target=None):
        self.data.extend(b'\0' * (-len(self.data) % 4))
        view = dict(buffer=0, byteOffset=len(self.data), byteLength=len(data))
        if target:
            view['target'] = target
        self.doc['bufferViews'].append(view)
        self.data.extend(data)
        return len(self.doc['bufferViews']) - 1

    def accessor(self, rows, kind, component, fmt, bounds=False, target=34962):
        acc = dict(bufferView=self.view(b''.join(struct.pack(fmt, *r) if isinstance(r, tuple) else struct.pack(fmt, r) for r in rows), target),
                   componentType=component, count=len(rows), type=kind)
        if bounds:
            acc.update(min=[min(c) for c in zip(*rows)], max=[max(c) for c in zip(*rows)])
        if component == 5121:
            acc['normalized'] = True
        self.doc['accessors'].append(acc)
        return len(self.doc['accessors']) - 1

    def material(self, png_bytes, wrap, mode, extras=None):
        mat = dict(doubleSided=True, extensions={'KHR_materials_unlit': {}}, alphaMode=mode)
        if mode == 'MASK':
            mat['alphaCutoff'] = 0.5
        if png_bytes:
            wraps = [10497, 33648, 33071, 33071]
            i = len(self.doc['images'])
            self.doc['images'].append(dict(bufferView=self.view(png_bytes), mimeType='image/png'))
            self.doc['samplers'].append(dict(magFilter=9729, minFilter=9987, wrapS=wraps[wrap[0]], wrapT=wraps[wrap[1]]))
            self.doc['textures'].append(dict(source=i, sampler=i))
            mat['pbrMetallicRoughness'] = dict(baseColorTexture=dict(index=i), metallicFactor=0, roughnessFactor=1)
        else:
            mat['pbrMetallicRoughness'] = dict(baseColorFactor=[1, 1, 1, 1], metallicFactor=0, roughnessFactor=1)
        if extras:
            mat['extras'] = extras
        self.doc['materials'].append(mat)
        return len(self.doc['materials']) - 1

    def primitive(self, verts, indices, material):
        attrs = dict(POSITION=self.accessor([v[0] for v in verts], 'VEC3', 5126, '<3f', bounds=True),
                     TEXCOORD_0=self.accessor([v[1] for v in verts], 'VEC2', 5126, '<2f'),
                     COLOR_0=self.accessor([v[2] for v in verts], 'VEC4', 5121, '<4B'))
        idx = self.accessor([(i,) for i in indices], 'SCALAR', 5125, '<I', bounds=True, target=34963)
        self.doc['meshes'][0]['primitives'].append(dict(attributes=attrs, indices=idx, material=material))

    def write(self, path):
        self.doc['buffers'] = [dict(byteLength=len(self.data))]
        meta = json.dumps(self.doc, separators=(',', ':')).encode()
        meta += b' ' * (-len(meta) % 4)
        self.data.extend(b'\0' * (-len(self.data) % 4))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(struct.pack('<4sIII4s', b'glTF', 2, 28 + len(meta) + len(self.data), len(meta), b'JSON') + meta +
                         struct.pack('<I4s', len(self.data), b'BIN\0') + self.data)


def srgb_to_linear8(c):
    x = c / 255
    return round(255 * (x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4))


LINEAR = [srgb_to_linear8(i) for i in range(256)]


def add_geometry(glb, level, translucent, scale, skip=frozenset()):
    batches, textures = level.geometry(skip=skip)
    stats = dict(tris=0, verts=0, materials=0)
    for (key, wrap), tris in sorted(batches.items(), key=lambda kv: str(kv[0])):
        has_alpha = False
        png_bytes = None
        if key is not None:
            w, h, rgba = textures[key]
            has_alpha = any(a < 255 for a in rgba[3::4])
            png_bytes = png(w, h, rgba)
        mode = 'BLEND' if translucent else 'MASK' if has_alpha else 'OPAQUE'
        material = glb.material(png_bytes, wrap, mode)
        index = {}
        verts = []
        indices = []
        for tri in tris:
            for x, y, z, u, v, r, g, b, a in tri:
                color = (LINEAR[r], LINEAR[g], LINEAR[b], a if translucent else 255)
                k = (x, y, z, round(u, 4), round(v, 4), color)
                if k not in index:
                    index[k] = len(verts)
                    verts.append(((x * scale, y * scale, z * scale), (round(u, 4), round(v, 4)), color))
                indices.append(index[k])
        glb.primitive(verts, indices, material)
        stats['tris'] += len(tris)
        stats['verts'] += len(verts)
        stats['materials'] += 1
    return stats


# ---------------------------------------------------------------------------------------------
# Collision -> SM64 triangle soup

# BK surface flag bits (include/core2/model.h): 0x1E0000 water surface; 0x10000 two-sided.
WATER = 0x1E0000
BK_WATER_ID = 45  # water-box id the water fx leaves alone: the BK model draws its own water
TWO_SIDED = 0x10000


def sm64_surface(flags):
    return 0


def build_collision(levels, scale):
    verts = []
    vindex = {}
    tris = []
    water_tris = []
    stats = defaultdict(int)

    def vid(p):
        k = tuple(round(c * scale, 2) for c in p)
        if k not in vindex:
            vindex[k] = len(verts)
            verts.append(list(k))
        return vindex[k]

    for level in levels:
        for a, b, c, flags in level.collision():
            pts = [level.vertex(i)[:3] for i in (a, b, c)]
            if flags & WATER:
                water_tris.append([[q * scale for q in p] for p in pts])
                stats['water'] += 1
                continue
            ids = [vid(p) for p in pts]
            if len(set(ids)) < 3:
                stats['degenerate'] += 1
                continue
            kind = sm64_surface(flags)
            tris.append([ids[0], ids[1], ids[2], kind, 0])
            if flags & TWO_SIDED:
                tris.append([ids[0], ids[2], ids[1], kind, 0])
                stats['two_sided'] += 1
    return verts, tris, water_tris, stats


def water_boxes(water_tris, cell=128.0):
    """Rasterise flat water-surface triangles (jumpman units) into [id, x1, z1, x2, z2, y] boxes."""
    groups = defaultdict(set)
    for tri in water_tris:
        y = round(sum(p[1] for p in tri) / 3)
        xs = [p[0] for p in tri]
        zs = [p[2] for p in tri]
        for ix in range(math.floor(min(xs) / cell), math.floor(max(xs) / cell) + 1):
            for iz in range(math.floor(min(zs) / cell), math.floor(max(zs) / cell) + 1):
                cx, cz = (ix + .5) * cell, (iz + .5) * cell
                (ax, _, az), (bx, _, bz), (qx, _, qz) = tri
                d = [(bx - ax) * (cz - az) - (bz - az) * (cx - ax), (qx - bx) * (cz - bz) - (qz - bz) * (cx - bx),
                     (ax - qx) * (cz - qz) - (az - qz) * (cx - qx)]
                if all(v >= 0 for v in d) or all(v <= 0 for v in d):
                    groups[y].add((ix, iz))
    boxes = []
    for y, cells in sorted(groups.items()):
        rows = defaultdict(list)
        for ix, iz in sorted(cells):
            rows[iz].append(ix)
        runs = {}
        for iz in sorted(rows):
            xs = rows[iz]
            start = prev = xs[0]
            for ix in xs[1:] + [None]:
                if ix is None or ix != prev + 1:
                    runs.setdefault((start, prev), []).append(iz)
                    if ix is not None:
                        start = ix
                if ix is not None:
                    prev = ix
        for (x0, x1), zs in runs.items():
            zs.sort()
            z0 = prev_z = zs[0]
            for z in zs[1:] + [None]:
                if z is None or z != prev_z + 1:
                    boxes.append([BK_WATER_ID, x0 * cell, z0 * cell, (x1 + 1) * cell, (prev_z + 1) * cell, y])
                    if z is not None:
                        z0 = z
                if z is not None:
                    prev_z = z
    return boxes


def pick_spawn(verts, tris, boxes, cell=100.0, want=7):
    """A broad, flat, dry clearing near the world's lowest dry ground: `want` cells of flat floor
    (all within 30 units of the same height) in every direction, so Mario can walk off in any direction."""
    floors = defaultdict(list)
    for a, b, c, _k, _f in tris:
        p, q, r = verts[a], verts[b], verts[c]
        ux, uy, uz = (q[i] - p[i] for i in range(3))
        vx, vy, vz = (r[i] - p[i] for i in range(3))
        nx, ny, nz = uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx
        m = math.sqrt(nx * nx + ny * ny + nz * nz)
        if m < 1e-6 or ny / m < 0.97:
            continue
        xs = [p[0], q[0], r[0]]
        zs = [p[2], q[2], r[2]]
        den = (q[2] - r[2]) * (p[0] - r[0]) + (r[0] - q[0]) * (p[2] - r[2])
        if abs(den) < 1e-9:
            continue
        for ix in range(math.floor(min(xs) / cell), math.floor(max(xs) / cell) + 1):
            for iz in range(math.floor(min(zs) / cell), math.floor(max(zs) / cell) + 1):
                x, z = (ix + .5) * cell, (iz + .5) * cell
                l1 = ((q[2] - r[2]) * (x - r[0]) + (r[0] - q[0]) * (z - r[2])) / den
                l2 = ((r[2] - p[2]) * (x - r[0]) + (p[0] - r[0]) * (z - r[2])) / den
                if min(l1, l2, 1 - l1 - l2) >= 0:
                    floors[(ix, iz)].append(l1 * p[1] + l2 * q[1] + (1 - l1 - l2) * r[1])

    lowest_water = min((b[5] for b in boxes), default=-1e9) - 100

    def wet(x, z, y):
        return y < lowest_water or any(b[1] <= x <= b[3] and b[2] <= z <= b[4] and y < b[5] + 300 for b in boxes)
    best = None
    for (ix, iz), ys in floors.items():
        for y in ys:
            if wet((ix + .5) * cell, (iz + .5) * cell, y):
                continue
            ok = all(any(abs(y2 - y) < 30 for y2 in floors.get((ix + dx, iz + dz), ()))
                     for dx in range(-want, want + 1) for dz in range(-want, want + 1)
                     if dx * dx + dz * dz <= want * want)
            if ok and (best is None or y < best[1]):
                best = ((ix + .5) * cell, y, (iz + .5) * cell)
    return [round(best[0]), round(best[1]) + 2, round(best[2])] if best else [0, 0, 0]


def export_world(read, wid, world, out_levels, out_courses):
    models = [Level(read(world['opa']))]
    if world.get('xlu'):
        models.append(Level(read(world['xlu'])))
    glb = Glb()
    total = defaultdict(int)
    for i, level in enumerate(models):
        s = add_geometry(glb, level, translucent=i == 1, scale=WORLD_SCALE, skip=frozenset())
        for k, v in s.items():
            total[k] += v
    glb.write(out_levels / wid / 'terrain.glb')
    verts, tris, water_tris, cstats = build_collision(models, WORLD_SCALE)
    xs = [v[0] for v in verts]
    zs = [v[2] for v in verts]
    boundary = math.ceil(max(max(abs(x) for x in xs), max(abs(z) for z in zs)) + 2000)
    boxes = water_boxes(water_tris)
    spawn = world.get('spawn') or pick_spawn(verts, tris, boxes)
    data = dict(model=f'levels/{wid}/terrain.glb', vertices=verts, triangles=tris,
                spawn=dict(pos=spawn or [0, 0, 0], faceYaw=world.get('yaw', 0)), waterBoxes=boxes,
                horizontalBoundary=boundary, objects=[], coins=[], sky=world['sky'], explorationOnly=True)
    (out_courses / f'{wid}.json').write_text(json.dumps(data, separators=(',', ':')))
    return dict(tris=total['tris'], mats=total['materials'], coll_tris=len(tris), water=len(boxes), spawn=spawn,
                x=(round(min(xs)), round(max(xs))), z=(round(min(zs)), round(max(zs))))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rom', type=Path, default=ROOT / 'reference/roms/bk-us-1.0.z64')
    parser.add_argument('worlds', nargs='*')
    parser.add_argument('--out', type=Path, default=ROOT / 'assets')
    args = parser.parse_args()
    rom = args.rom.read_bytes()
    if hashlib.md5(rom).hexdigest() != 'b29599651a13f681c9923d69354bf4a3':
        raise SystemExit('Expected a big-endian Banjo-Kazooie US 1.0 ROM')
    read = bk_banjo['assets'](rom)
    (args.out / 'courses').mkdir(parents=True, exist_ok=True)
    for wid, world in WORLDS.items():
        if args.worlds and wid not in args.worlds:
            continue
        print(wid, export_world(read, wid, world, args.out / 'levels', args.out / 'courses'))


if __name__ == '__main__':
    main()
