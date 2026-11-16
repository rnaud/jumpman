#!/usr/bin/env python3
"""Export the Super Smash Bros. (US) fighters from a local ROM: skinned glTF (.glb) with textures and every motion.

Formats follow VetriTheRetri/ssb-decomp-re (reference/ssb64, see native/SSB64.md):
  * relocData file system: LBTableEntry table at 0x1AC870, vpk0 compression, intern/extern pointer chains
    (src/lb/lbreloc.c, src/lb/lbtypes.h, src/sys/dma.c syDmaDecodeVpk0).
  * fighters: FTData (src/ft/ftdata.c) -> Main file FTAttributes -> commonparts_container -> DObjDesc joint tree,
    per-joint MObjSub materials and costume material scripts (src/ft/fttypes.h, src/lb/lbcommon.c).
  * display lists: F3DEX2 with a vertex cache shared across joints (the RSP keeps vertices a parent joint loaded, so a
    triangle can span two joints: that is the soft skin, exported here as glTF skin weights).
  * motions: per-joint AObjEvent16 "figatree" scripts (src/ft/ftanim.c ftAnimParseDObjFigatree, src/sys/objanim.c
    gcPlayDObjAnimJoint), sampled every 1/60 s frame.
Names come from the decomp: file names from tools/relocFileDescriptions.us.txt, the fighter table and motion tables
from src/ft/ftdata.c, common motion names from src/ft/ftdef.h, each fighter's own motions from
src/ft/ftchar/ft<name>/ft<name>.h. Standard library only, deterministic.

Writes public/models/ssb64/<fighter>.glb and public/models/ssb64/fighters.json.
"""
import argparse
import hashlib
import json
import math
import re
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECOMP = ROOT / 'reference/ssb64'
ROM = ROOT / 'reference/roms/ssb64-us.z64'
OUT = ROOT / 'public/models/ssb64'
SHA1 = 'e2929e10fccc0aa84e5776227e798abc07cedabf'

TABLE = 0x1AC870  # lLBRelocTableAddr (src/lb/lbreloc.h)
FILE_COUNT = 2132
DOBJ_ARRAY_MAX = 18
ATTR_SETUP_PARTS = 0x29C  # FTAttributes field offsets (src/ft/fttypes.h)
ATTR_COMMONPARTS = 0x2D4
UNIT = 0.004  # SSB64 units -> exported units: Mario (model size 1.12) stands about 1.8 units tall
FPS = 60.0

# fighter id (file name) -> FTData symbol stem in ftdata.c
FIGHTERS = ['Mario', 'Fox', 'Donkey', 'Samus', 'Luigi', 'Link', 'Yoshi', 'Captain', 'Kirby', 'Pikachu', 'Purin', 'Ness', 'Boss']
LABELS = {'Mario': 'Mario', 'Fox': 'Fox', 'Donkey': 'Donkey Kong', 'Samus': 'Samus', 'Luigi': 'Luigi', 'Link': 'Link',
          'Yoshi': 'Yoshi', 'Captain': 'Captain Falcon', 'Kirby': 'Kirby', 'Pikachu': 'Pikachu', 'Purin': 'Jigglypuff',
          'Ness': 'Ness', 'Boss': 'Master Hand'}


# ---------------------------------------------------------------------------------------------- relocData files

def vpk0(src):
    """syDmaDecodeVpk0 (src/sys/dma.c)."""
    if src[:4] != b'vpk0':
        raise ValueError('not vpk0 data')
    st = [4 * 0, 0, 0]  # pos, bit buffer, bit count

    def bits(n):
        while st[2] < n:
            st[1] = ((st[1] << 16) | (src[st[0]] << 8) | src[st[0] + 1]) & 0xFFFFFFFFFFFF
            st[0] += 2
            st[2] += 16
        st[2] -= n
        return (st[1] >> st[2]) & ((1 << n) - 1)

    bits(32)
    size = bits(32)
    method = bits(8)

    def tree():
        stack = []
        while True:
            if bits(1):
                if len(stack) < 2:
                    break
                r = stack.pop()
                stack[-1] = (stack[-1], r)
            else:
                stack.append(bits(8))
        return stack[0] if stack else 0

    def read(t):
        while isinstance(t, tuple):
            t = t[bits(1)]
        return bits(t)

    offsets, lengths = tree(), tree()
    out = bytearray()
    while len(out) < size:
        if not bits(1):
            out.append(bits(8))
            continue
        if method:
            extra = 0
            v = read(offsets)
            if v <= 2:
                extra = v + 1
                v = read(offsets)
            start = len(out) - v * 4 - extra + 8
        else:
            start = len(out) - read(offsets)
        for _ in range(read(lengths)):
            out.append(out[start])
            start += 1
    return bytes(out[:size])


class Files:
    """lbRelocLoadAndRelocFile: file bytes plus every pointer slot resolved to (file id, byte offset)."""

    def __init__(self, rom):
        self.rom = rom
        self.data_start = TABLE + (FILE_COUNT + 1) * 12
        self.cache = {}

    def get(self, fid):
        if fid not in self.cache:
            e = self.rom[TABLE + fid * 12:TABLE + fid * 12 + 12]
            word, intern, csize, extern, dsize = struct.unpack('>IHHHH', e)
            start = self.data_start + (word & 0x7FFFFFFF)
            raw = self.rom[start:start + csize * 4]
            data = vpk0(raw) if word >> 31 else raw[:dsize * 4]
            if len(data) != dsize * 4:
                raise ValueError(f'file {fid}: {len(data)} bytes, expected {dsize * 4}')
            ptrs = {}
            r = intern
            while r != 0xFFFF:
                w = struct.unpack_from('>I', data, r * 4)[0]
                ptrs[r * 4] = (fid, (w & 0xFFFF) * 4)
                r = w >> 16
            ids = start + csize * 4
            r, k = extern, 0
            while r != 0xFFFF:
                w = struct.unpack_from('>I', data, r * 4)[0]
                ptrs[r * 4] = (struct.unpack_from('>H', self.rom, ids + 2 * k)[0], (w & 0xFFFF) * 4)
                r = w >> 16
                k += 1
            self.cache[fid] = (data, ptrs)
        return self.cache[fid]

    def data(self, fid):
        return self.get(fid)[0]

    def ptr(self, ref, off=0):
        """The pointer stored at `ref` (+off), or None."""
        if ref is None:
            return None
        return self.get(ref[0])[1].get(ref[1] + off)

    def u32(self, ref, off=0):
        return struct.unpack_from('>I', self.data(ref[0]), ref[1] + off)[0]


# ---------------------------------------------------------------------------------------------- decomp tables

def file_names():
    """tools/relocFileDescriptions.us.txt: `-NNN: Name` lines -> {name: id}."""
    names = {}
    for line in (DECOMP / 'tools/relocFileDescriptions.us.txt').read_text().split('\n'):
        if line.startswith('# FILE CONTENTS'):
            break
        m = re.match(r'-(\d+): (\w+)', line)
        if m:
            names[m.group(2)] = int(m.group(1))
    return names


def common_motions():
    """src/ft/ftdef.h enum FTCommonMotion: motion id -> name (without the nFTCommonMotion prefix)."""
    text = (DECOMP / 'src/ft/ftdef.h').read_text()
    body = text[text.index('typedef enum FTCommonMotion'):]
    body = body[body.index('{') + 1:body.index('}')]
    out, value = {}, -1
    for raw in body.split(','):
        line = re.sub(r'//.*', '', raw).strip()
        m = re.match(r'nFTCommonMotion(\w+)\s*(?:=\s*(\S+))?$', line, re.S)
        if not m:
            continue
        if m.group(2) is None:
            value += 1
        elif re.match(r'-?\d+$', m.group(2)):
            value = int(m.group(2))
        else:
            # aliases do not advance the counter; `AttackAirN = AttackAirStart` names the first aerial
            target = m.group(2).replace('nFTCommonMotion', '')
            if target.endswith('Start') and out.get(value) == target:
                out[value] = m.group(1)
            continue
        out.setdefault(value, m.group(1))
    return out


def special_motions(name):
    """src/ft/ftchar/ft<name>/ft<name>.h enum FT<Name>Motion: the names of the motions from nFTCommonMotionSpecialStart on."""
    text = (DECOMP / f'src/ft/ftchar/ft{name.lower()}/ft{name.lower()}.h').read_text()
    m = re.search(r'nFT%sMotion\w+\s*=\s*nFTCommonMotionSpecialStart' % name, text)
    body = text[m.start():]
    body = body[:body.index('}')]
    out = []
    for raw in body.split(','):
        line = re.sub(r'//.*', '', raw).strip()
        mm = re.match(r'nFT%sMotion(\w+)\s*(?:=\s*(\S+))?$' % name, line, re.S)
        if mm and (not out or mm.group(2) is None):  # later `X = Y` lines are aliases
            out.append(mm.group(1))
    return out


def fighter_tables(names):
    """src/ft/ftdata.c: FTData (main / model file, attribute offset) and the main motion descriptor table per fighter."""
    text = (DECOMP / 'src/ft/ftdata.c').read_text()
    out = {}
    for f in FIGHTERS:
        m = re.search(r'FTData dFT%sData =\s*\{(.*?)\};' % f, text, re.S)
        fields = [x.strip() for x in m.group(1).split(',')]
        sym = lambda s: names[re.match(r'&ll(\w+)FileID', s).group(1)]
        main, model = sym(fields[0]), sym(fields[3])
        attr = int(fields[24], 16)
        table = re.search(r'FTMotionDesc %s\[\]\s*=\s*\{(.*?)\n\};' % re.escape(fields[25].lstrip('&')), text, re.S).group(1)
        # entries are {file, script offset, flags} triples; a few are written without braces, so read a flat list
        items = [x.strip() for x in re.sub(r'//[^\n]*|/\*.*?\*/|[{}]', ' ', table, flags=re.S).split(',') if x.strip()]
        motions = []
        for k in range(0, len(items) - 2, 3):
            fm = re.match(r'&ll(\w+)FileID', items[k])
            flags = items[k + 2]
            special = 'ANIMJOINT' in flags or 'SHIELDPOSE' in flags
            motions.append(fm.group(1) if fm and not special else None)
        out[f] = {'main': main, 'model': model, 'attr': attr, 'motions': motions, 'specials': special_motions(f)}
    return out


# ---------------------------------------------------------------------------------------------- math

def mat_trs(t, r, s):
    """syMatrixTraRotRpyRSca as a column-vector 4x4: T * Rz(r.z) * Ry(r.y) * Rx(r.x) * S."""
    cr, sr = math.cos(r[0]), math.sin(r[0])
    cp, sp = math.cos(r[1]), math.sin(r[1])
    cy, sy = math.cos(r[2]), math.sin(r[2])
    m = [[cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr],
         [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr],
         [-sp, cp * sr, cp * cr]]
    return [[m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2], t[0]],
            [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2], t[1]],
            [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2], t[2]],
            [0.0, 0.0, 0.0, 1.0]]


def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def mat_inv(m):
    """Inverse of an affine 4x4."""
    a = [row[:3] for row in m[:3]]
    det = (a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
           + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
    inv = [[(a[1][1] * a[2][2] - a[1][2] * a[2][1]) / det, (a[0][2] * a[2][1] - a[0][1] * a[2][2]) / det, (a[0][1] * a[1][2] - a[0][2] * a[1][1]) / det],
           [(a[1][2] * a[2][0] - a[1][0] * a[2][2]) / det, (a[0][0] * a[2][2] - a[0][2] * a[2][0]) / det, (a[0][2] * a[1][0] - a[0][0] * a[1][2]) / det],
           [(a[1][0] * a[2][1] - a[1][1] * a[2][0]) / det, (a[0][1] * a[2][0] - a[0][0] * a[2][1]) / det, (a[0][0] * a[1][1] - a[0][1] * a[1][0]) / det]]
    t = [-sum(inv[i][k] * m[k][3] for k in range(3)) for i in range(3)]
    return [inv[0] + [t[0]], inv[1] + [t[1]], inv[2] + [t[2]], [0.0, 0.0, 0.0, 1.0]]


def apply(m, v, w=1.0):
    return [m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3] * w for i in range(3)]


def quat(r):
    """Rz * Ry * Rx (roll r.x, pitch r.y, yaw r.z) as an xyzw quaternion."""
    cx, sx = math.cos(r[0] / 2), math.sin(r[0] / 2)
    cy, sy = math.cos(r[1] / 2), math.sin(r[1] / 2)
    cz, sz = math.cos(r[2] / 2), math.sin(r[2] / 2)
    return [sx * cy * cz - cx * sy * sz, cx * sy * cz + sx * cy * sz, cx * cy * sz - sx * sy * cz, cx * cy * cz + sx * sy * sz]


# ---------------------------------------------------------------------------------------------- textures

def png(width, height, rgba):
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xFFFFFFFF)
    rows = b''.join(b'\0' + bytes(rgba[y * width * 4:(y + 1) * width * 4]) for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b''))


def rgba16(v):
    return [(v >> 11 & 31) * 255 // 31, (v >> 6 & 31) * 255 // 31, (v >> 1 & 31) * 255 // 31, 255 if v & 1 else 0]


def tile_axis(size, mask, mode):
    """Texel columns (or rows) an exported image needs along one tile axis and its glTF wrap mode.
    Clamped axes keep the whole tile (mirror/mask applied here); wrapped ones keep one mask period and let the sampler repeat."""
    clamp, mirror = mode & 2, mode & 1
    if mask and not clamp:
        n = 1 << mask
        return list(range(n)), 33648 if mirror else 10497
    out = []
    for i in range(size):
        v = i
        if mask:
            v = i & ((1 << mask) - 1)
            if mirror and (i >> mask) & 1:
                v = (1 << mask) - 1 - v
        out.append(v)
    return out, 33071 if clamp else (33648 if mirror else 10497)


def decode_tile(tmem, tile, xs, ys, tlut_mode):
    """Texels (xs x ys TMEM coordinates) of `tile` (fmt, siz, line, tmem address, palette) from the 4 KB TMEM image, row-major RGBA."""
    fmt, siz, line, addr, pal = tile['fmt'], tile['siz'], tile['line'], tile['tmem'], tile['palette']
    stride = line * 8
    base = addr * 8
    out = []
    for y in ys:
        row = base + y * stride
        for x in xs:
            if siz == 0:  # 4b
                b = tmem[(row + x // 2) & 0xFFF]
                v = (b >> 4) if x % 2 == 0 else (b & 15)
                if fmt == 2:  # CI
                    p = 0x800 + (pal * 16 + v) * 2
                    c = (tmem[p & 0xFFF] << 8) | tmem[(p + 1) & 0xFFF]
                    out += ia16(c) if tlut_mode == 3 else rgba16(c)
                elif fmt == 3:  # IA
                    i = (v >> 1) * 255 // 7
                    out += [i, i, i, 255 if v & 1 else 0]
                else:  # I
                    out += [v * 17] * 4
            elif siz == 1:  # 8b
                v = tmem[(row + x) & 0xFFF]
                if fmt == 2:
                    p = 0x800 + v * 2
                    c = (tmem[p & 0xFFF] << 8) | tmem[(p + 1) & 0xFFF]
                    out += ia16(c) if tlut_mode == 3 else rgba16(c)
                elif fmt == 3:
                    i = (v >> 4) * 17
                    out += [i, i, i, (v & 15) * 17]
                else:
                    out += [v] * 4
            elif siz == 2:  # 16b
                p = row + x * 2
                c = (tmem[p & 0xFFF] << 8) | tmem[(p + 1) & 0xFFF]
                out += ia16(c) if fmt == 3 else rgba16(c)
            else:  # 32b, loaded linearly here (LOADBLOCK copies bytes in order), a line holds half a row
                p = base + y * stride * 2 + x * 4
                out += [tmem[(p + k) & 0xFFF] for k in range(4)]
    return out


def ia16(c):
    return [c >> 8, c >> 8, c >> 8, c & 0xFF]


# ---------------------------------------------------------------------------------------------- colour combiner

def combine(mode, two_cycle, texel0, texel1, prim, shade, env, prim_lod):
    """One pixel of the RDP colour combiner, values 0..1, (a - b) * c + d per cycle."""
    w0, w1 = mode
    cyc = [((w0 >> 20) & 15, (w1 >> 28) & 15, (w0 >> 15) & 31, (w1 >> 15) & 7, (w0 >> 12) & 7, (w1 >> 12) & 7, (w0 >> 9) & 7, (w1 >> 9) & 7),
           ((w0 >> 5) & 15, (w1 >> 24) & 15, w0 & 31, (w1 >> 6) & 7, (w1 >> 21) & 7, (w1 >> 3) & 7, (w1 >> 18) & 7, w1 & 7)]
    comb = [0.0, 0.0, 0.0, 0.0]
    for ci in ((0, 1) if two_cycle else (0,)):
        a, b, c, d, aa, ab, ac, ad = cyc[ci]
        src = {0: comb, 1: texel0, 2: texel1, 3: prim, 4: shade, 5: env}

        def col(sel, kind):
            if sel in src and not (kind == 'c' and sel > 5):
                return src[sel][:3]
            if kind == 'a':
                return [1.0] * 3 if sel == 6 else [0.0] * 3
            if kind == 'b':
                return [0.0] * 3
            if kind == 'd':
                return [1.0] * 3 if sel == 6 else [0.0] * 3
            # c
            alpha = {7: comb, 8: texel0, 9: texel1, 10: prim, 11: shade, 12: env}
            if sel in alpha:
                return [alpha[sel][3]] * 3
            if sel == 14:
                return [prim_lod] * 3
            return [0.0] * 3

        def alp(sel, is_c):
            if is_c:
                if sel == 0:
                    return 0.0
                if sel == 6:
                    return prim_lod
            elif sel == 0:
                return comb[3]
            if sel in (1, 2, 3, 4, 5):
                return src[sel][3]
            if sel == 6 and not is_c:
                return 1.0
            return 0.0

        ca, cb, cc, cd = col(a, 'a'), col(b, 'b'), col(c, 'c'), col(d, 'd')
        rgb = [min(1.0, max(0.0, (ca[i] - cb[i]) * cc[i] + cd[i])) for i in range(3)]
        al = min(1.0, max(0.0, (alp(aa, False) - alp(ab, False)) * alp(ac, True) + alp(ad, False)))
        comb = rgb + [al]
    return comb


def uses(mode, two_cycle, inputs):
    """True if the combiner reads any of the given inputs (1 TEXEL0, 2 TEXEL1, 3 PRIM, 4 SHADE, 5 ENV) in colour or alpha."""
    w0, w1 = mode
    cycles = [((w0 >> 20) & 15, (w1 >> 28) & 15, (w0 >> 15) & 31, (w1 >> 15) & 7, (w0 >> 12) & 7, (w1 >> 12) & 7, (w0 >> 9) & 7, (w1 >> 9) & 7),
              ((w0 >> 5) & 15, (w1 >> 24) & 15, w0 & 31, (w1 >> 6) & 7, (w1 >> 21) & 7, (w1 >> 3) & 7, (w1 >> 18) & 7, w1 & 7)]
    for a, b, c, d, aa, ab, ac, ad in cycles[:2 if two_cycle else 1]:
        sel = {a, b, d, aa, ab, ad, ac} | ({c} if c <= 5 else ({c - 7} if 8 <= c <= 12 else set()))
        if sel & set(inputs):
            return True
    return False


# ---------------------------------------------------------------------------------------------- display lists

G_LIGHTING = 0x00020000  # F3DEX2 geometry mode bits
G_CULL_BACK = 0x00000400
G_CULL_FRONT = 0x00000200
G_TEXTURE_GEN = 0x00040000


class Mesh:
    """Runs a fighter's display lists joint by joint (draw order = joint order) with one shared vertex cache."""

    def __init__(self, files):
        self.files = files
        self.cache = [None] * 64
        self.verts = []  # (joint, position, normal, uv, colour)
        self.tris = {}   # material key -> [vertex index]
        self.materials = {}
        self.textures = {}
        self.tmem = bytearray(4096)
        self.tiles = [dict(fmt=0, siz=0, line=0, tmem=0, palette=0, cms=0, cmt=0, masks=0, maskt=0, shifts=0, shiftt=0,
                           uls=0, ult=0, lrs=0, lrt=0) for _ in range(8)]
        self.timg = None
        # ftDisplayMainProcDisplay (src/ft/ftdisplaymain.c) before the joints: 2-cycle, z-buffered, back-face culled, lit
        self.geom = 0x00000001 | 0x00000004 | G_CULL_BACK | G_LIGHTING | 0x00200000
        self.combine = (0, 0)
        self.prim = [1.0] * 4
        self.prim_lod = 0.0
        self.env = [1.0] * 4
        self.light1 = [1.0, 1.0, 1.0]
        self.light2 = [0.3, 0.3, 0.3]
        self.tex_on = False
        self.tex_scale = (1.0, 1.0)
        self.tex_tile = 0
        self.other_h = 1 << 20
        self.mobjs = []
        self.unknown = set()
        self.gen = 0
        self.last = (-1, None)

    # -- materials (MObj) ---------------------------------------------------------------------------------------

    def mobj_dl(self, index):
        """gcDrawMObjForDObj for the joint's index-th MObj, applied to the state directly."""
        if index >= len(self.mobjs):
            return
        m = self.mobjs[index]
        flags = m['flags'] or (0x80 | 0x20 | 0x01)
        scau, scav, trau, trav = m['scau'], m['scav'], m['trau'], m['trav']
        if m['unk10'] == 1 and flags & (0x80 | 0x40 | 0x20):
            scau *= 0.5
            trau = ((trau - m['trau']) + 1.0 - (m['scau'] * 0.5)) * 0.5
        if flags & 0x04 and m['palette']:
            self.timg = dict(addr=m['palette'], fmt=0, siz=2, width=1)
            if flags & 0x03:
                self.tiles[5].update(fmt=0, siz=0, line=0, tmem=0x100, palette=0)
                self.load_tlut(5, 0xFF if m['siz'] == 1 else 0xF)
        if flags & 0x1000:
            self.light1 = m['light1']
        if flags & 0x2000:
            self.light2 = m['light2']
        if flags & (0x200 | 0x10 | 0x8):
            self.prim = m['prim']
            self.prim_lod = m['lfrac']
        if flags & 0x400:
            self.env = m['env']
        if flags & (0x10 | 0x02) and m['sprites']:
            self.timg = dict(addr=self.files.ptr(m['sprites'], 0), fmt=m['block_fmt'], siz=3 if m['block_siz'] == 3 else 2, width=1)
            if flags & (0x10 | 0x01):
                dxt, rows = m['block_dxt'], m['unk36']
                n = {0: ((dxt * rows + 3) >> 2) - 1, 1: ((dxt * rows + 1) >> 1) - 1}.get(m['block_siz'], dxt * rows - 1)
                self.tiles[6]['tmem'] = 0
                self.load_block(6, n)
        if flags & (0x10 | 0x01) and m['sprites']:
            self.timg = dict(addr=self.files.ptr(m['sprites'], 4 * m.get('texture_id', 0)), fmt=m['fmt'], siz=m['siz'], width=1)
        if flags & 0x20:
            if m['unk10'] == 2:
                uls = int((m['unk0C'] * trau / scau) * 4.0) if abs(scau) > 1 / 65535 else 0
                ult = int((m['unk0E'] * trav / scav) * 4.0) if abs(scav) > 1 / 65535 else 0
                uls, ult = max(uls, 0), max(ult, 0)
            else:
                uls = int(((m['unk0C'] * trau + m['unk0A']) / scau) * 4.0) if abs(scau) > 1 / 65535 else 0
                ult = int(((((1.0 - scav) - trav) * m['unk0E'] + m['unk0A']) / scav) * 4.0) if abs(scav) > 1 / 65535 else 0
            self.tiles[0].update(uls=uls, ult=ult, lrs=((m['unk0C'] - 1) << 2) + uls, lrt=((m['unk0E'] - 1) << 2) + ult)
        if flags & 0x80:
            if m['unk10'] == 2:
                s = int((m['unk0C'] * 64) / scau) if abs(scau) > 1 / 65535 else 0
                t = int((m['unk0E'] * 64) / scav) if abs(scav) > 1 / 65535 else 0
            else:
                s = int((2097152.0 / m['unk08']) / scau) if abs(scau) > 1 / 65535 and m['unk08'] else 0
                t = int((2097152.0 / m['unk08']) / scav) if abs(scav) > 1 / 65535 and m['unk08'] else 0
            self.tex_scale = (min(s, 0xFFFF) / 65536.0, min(t, 0xFFFF) / 65536.0)
            self.tex_on = True

    # -- TMEM ---------------------------------------------------------------------------------------------------

    def src_bytes(self, n):
        ref = self.timg['addr']
        d = self.files.data(ref[0])
        return d[ref[1]:ref[1] + n].ljust(n, b'\0')

    def load_block(self, tile, lrs):
        siz = self.timg['siz']
        n = ((lrs + 1) << siz) >> 1 if siz else (lrs + 1) // 2
        base = self.tiles[tile]['tmem'] * 8
        data = self.src_bytes(n)
        for i, b in enumerate(data):
            self.tmem[(base + i) & 0xFFF] = b

    def load_tile(self, tile, uls, ult, lrs, lrt):
        t = self.tiles[tile]
        siz = self.timg['siz']
        width = self.timg['width']
        x0, y0, x1, y1 = uls >> 2, ult >> 2, lrs >> 2, lrt >> 2
        bpp = (4 << siz) // 8 if siz else 0.5
        ref = self.timg['addr']
        d = self.files.data(ref[0])
        rowbytes = int((x1 - x0 + 1) * bpp)
        for y in range(y0, y1 + 1):
            src = ref[1] + int((y * width + x0) * bpp)
            dst = t['tmem'] * 8 + (y - y0) * t['line'] * 8
            for i in range(rowbytes):
                self.tmem[(dst + i) & 0xFFF] = d[src + i] if src + i < len(d) else 0

    def load_tlut(self, tile, count):
        base = self.tiles[tile]['tmem'] * 8
        data = self.src_bytes((count + 1) * 2)
        for i, b in enumerate(data):
            self.tmem[(base + i) & 0xFFF] = b

    # -- display list -------------------------------------------------------------------------------------------

    def run(self, ref, joint, world, depth=0):
        files = self.files
        data, ptrs = files.get(ref[0])
        o = ref[1]
        normal_m = [row[:3] for row in world[:3]]
        while True:
            w0, w1 = struct.unpack_from('>II', data, o)
            op = w0 >> 24
            target = ptrs.get(o + 4)
            if op not in (0x01, 0x02, 0x05, 0x06, 0x07, 0xDF, 0xE7, 0xE8, 0xE6, 0xE9, 0x00):
                self.gen += 1
            if op == 0x01 and not target:
                self.unknown.add('vtx-segment')
            elif op == 0x01:  # G_VTX
                n = (w0 >> 12) & 0xFF
                v0 = ((w0 >> 1) & 0x7F) - n
                vd = files.data(target[0])
                for i in range(n):
                    x, y, z, _, s, t, r, g, b, a = struct.unpack_from('>hhhHhhBBBB', vd, target[1] + 16 * i)
                    if self.geom & G_LIGHTING:
                        nv = [struct.unpack('b', bytes([c]))[0] / 127.0 for c in (r, g, b)]
                        col = None
                    else:
                        nv = None
                        col = [r / 255, g / 255, b / 255, a / 255]
                    self.cache[v0 + i] = dict(joint=joint, pos=apply(world, (x, y, z)), nrm=nv and apply(world, nv, 0.0), st=[s, t], col=col)
            elif op == 0x02:  # G_MODIFYVTX
                where, vi = (w0 >> 16) & 0xFF, (w0 & 0xFFFF) // 2
                if where == 0x14 and self.cache[vi]:
                    self.cache[vi] = dict(self.cache[vi], st=[struct.unpack('>h', struct.pack('>H', w1 >> 16))[0], struct.unpack('>h', struct.pack('>H', w1 & 0xFFFF))[0]])
            elif op in (0x05, 0x06, 0x07):
                tris = [((w0 >> 16) & 0xFF, (w0 >> 8) & 0xFF, w0 & 0xFF)]
                if op != 0x05:
                    tris.append(((w1 >> 16) & 0xFF, (w1 >> 8) & 0xFF, w1 & 0xFF))
                for tri in tris:
                    self.triangle([i // 2 for i in tri])
            elif op == 0xD7:
                self.tex_on = bool((w0 >> 1) & 0x7F)
                self.tex_tile = (w0 >> 8) & 7
                sc = lambda v: 1.0 if v == 0xFFFF else v / 65536.0
                self.tex_scale = (sc(w1 >> 16), sc(w1 & 0xFFFF))
            elif op == 0xD9:
                self.geom = (self.geom & (w0 & 0xFFFFFF)) | w1
            elif op == 0xFC:
                self.combine = (w0 & 0xFFFFFF, w1)
            elif op == 0xFA:
                self.prim = [(w1 >> 24) / 255, (w1 >> 16 & 255) / 255, (w1 >> 8 & 255) / 255, (w1 & 255) / 255]
                self.prim_lod = (w0 & 0xFF) / 255
            elif op == 0xFB:
                self.env = [(w1 >> 24) / 255, (w1 >> 16 & 255) / 255, (w1 >> 8 & 255) / 255, (w1 & 255) / 255]
            elif op == 0xFD:
                self.timg = dict(addr=target, fmt=(w0 >> 21) & 7, siz=(w0 >> 19) & 3, width=(w0 & 0xFFF) + 1)
            elif op == 0xF5:
                self.tiles[(w1 >> 24) & 7].update(fmt=(w0 >> 21) & 7, siz=(w0 >> 19) & 3, line=(w0 >> 9) & 0x1FF, tmem=w0 & 0x1FF,
                                                  palette=(w1 >> 20) & 15, cmt=(w1 >> 18) & 3, maskt=(w1 >> 14) & 15, shiftt=(w1 >> 10) & 15,
                                                  cms=(w1 >> 8) & 3, masks=(w1 >> 4) & 15, shifts=w1 & 15)
            elif op == 0xF2:
                self.tiles[(w1 >> 24) & 7].update(uls=(w0 >> 12) & 0xFFF, ult=w0 & 0xFFF, lrs=(w1 >> 12) & 0xFFF, lrt=w1 & 0xFFF)
            elif op == 0xF3:
                if self.timg and self.timg['addr']:
                    self.load_block((w1 >> 24) & 7, (w1 >> 12) & 0xFFF)
            elif op == 0xF4:
                if self.timg and self.timg['addr']:
                    self.load_tile((w1 >> 24) & 7, (w0 >> 12) & 0xFFF, w0 & 0xFFF, (w1 >> 12) & 0xFFF, w1 & 0xFFF)
            elif op == 0xF0:
                if self.timg and self.timg['addr']:
                    self.load_tlut((w1 >> 24) & 7, (w1 >> 14) & 0x3FF)
            elif op == 0xE3:
                length = (w0 & 0xFF) + 1
                shift = 32 - ((w0 >> 8) & 0xFF) - length
                mask = ((1 << length) - 1) << shift
                self.other_h = (self.other_h & ~mask) | (w1 & mask)
            elif op == 0xDB:
                if (w0 >> 16) & 0xFF == 0x0A:
                    c = [(w1 >> 24) / 255, (w1 >> 16 & 255) / 255, (w1 >> 8 & 255) / 255]
                    if w0 & 0xFFFF == 0x00:
                        self.light1 = c
                    elif w0 & 0xFFFF == 0x18:
                        self.light2 = c
            elif op == 0xDE:
                if w1 >> 24 == 0x0E:
                    self.mobj_dl((w1 & 0xFFFFFF) // 8)
                elif target:
                    self.run(target, joint, world, depth + 1)
                if (w0 >> 16) & 0xFF == 1:
                    return
            elif op == 0xDF:
                return
            elif op in (0xE7, 0xE8, 0xE6, 0xE9, 0xE2, 0xE1, 0xF7, 0xF8, 0xF9, 0xD8, 0x00):
                pass
            else:
                self.unknown.add(op)
            o += 8

    def triangle(self, idx):
        vs = [self.cache[i] for i in idx]
        if any(v is None for v in vs):
            return
        key = self.material_key()
        lst = self.tris.setdefault(key, [])
        mat = self.materials[key]
        for v in vs:
            st = v['st']
            if mat['uv'] is not None:
                tw, th, uls, ult, ss, ts, sh = mat['uv']
                u = (st[0] / 32.0 * ss * sh[0] - uls / 4.0) / tw
                vv = (st[1] / 32.0 * ts * sh[1] - ult / 4.0) / th
                uv = [u, vv]
            else:
                uv = [0.0, 0.0]
            col = v['col'] if v['col'] is not None and mat['vcol'] else None
            self.verts.append((v['joint'], v['pos'], v['nrm'], uv, col))
            lst.append(len(self.verts) - 1)

    def material_key(self):
        if self.last[0] == self.gen:
            return self.last[1]
        key = self.new_material()
        self.last = (self.gen, key)
        return key

    def new_material(self):
        two = ((self.other_h >> 20) & 3) == 1
        lit = bool(self.geom & G_LIGHTING)
        uv = None
        textured = self.tex_on and uses(self.combine, two, {1, 2})
        tiles = []
        flat = None
        if textured:
            for ti in (self.tex_tile, self.tex_tile + 1):
                t = self.tiles[ti & 7]
                xs, wrap_s = tile_axis(max(1, min(((t['lrs'] - t['uls']) >> 2) + 1, 1024)), t['masks'], t['cms'])
                ys, wrap_t = tile_axis(max(1, min(((t['lrt'] - t['ult']) >> 2) + 1, 1024)), t['maskt'], t['cmt'])
                tlut = (self.other_h >> 14) & 3
                px = decode_tile(self.tmem, t, xs, ys, tlut)
                tiles.append((len(xs), len(ys), px, dict(t, wrap=(wrap_s, wrap_t))))
            w, h, px, t = tiles[0]
            shift = lambda s: 1.0 / (1 << s) if 0 < s <= 10 else (float(1 << (16 - s)) if s > 10 else 1.0)
            uv = (w, h, t['uls'], t['ult'], self.tex_scale[0], self.tex_scale[1], (shift(t['shifts']), shift(t['shiftt'])))
            if self.geom & G_TEXTURE_GEN:
                # environment-mapped (Link's blade, visors): no fixed UVs, so the material takes the texture's mean colour
                flat = [[sum(tp[2][k::4]) / (255.0 * tp[0] * tp[1]) for k in range(4)] for tp in tiles]
                tiles, uv = [], None
        shade = (list(self.light1) + [1.0]) if lit else [1.0, 1.0, 1.0, 1.0]
        vcol = not lit and uses(self.combine, two, {4})
        # every input of the material, exactly (the Rust port builds the same key)
        key = (self.combine, two, tuple(self.prim), self.prim_lod, tuple(self.env), tuple(shade), vcol, bool(self.geom & G_CULL_BACK),
               bool(self.geom & G_CULL_FRONT), uv, flat and tuple(tuple(c) for c in flat),
               tuple((tp[0], tp[1], bytes(tp[2]), tp[3]['wrap']) for tp in tiles))
        if key not in self.materials:
            mat = {'uv': uv, 'vcol': vcol, 'double': not (self.geom & (G_CULL_BACK | G_CULL_FRONT))}
            if tiles:
                w, hh, px, t = tiles[0]
                w1, h1, px1, _ = tiles[1]
                out = []
                for i in range(w * hh):
                    t0 = [c / 255 for c in px[i * 4:i * 4 + 4]]
                    j = ((i // w) % h1) * w1 + (i % w) % w1
                    t1 = [c / 255 for c in px1[j * 4:j * 4 + 4]]
                    c = combine(self.combine, two, t0, t1, self.prim, shade, self.env, self.prim_lod)
                    out += [int(round(x * 255)) for x in c]
                mat['image'] = (w, hh, out)
                mat['wrap'] = t['wrap']
                alphas = out[3::4]
                mat['alpha'] = 'OPAQUE' if min(alphas) > 250 else ('MASK' if all(a < 6 or a > 250 for a in alphas) else 'BLEND')
                mat['color'] = [1.0, 1.0, 1.0, 1.0]
            else:
                t0, t1 = flat or ([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0])
                c = combine(self.combine, two, t0, t1, self.prim, shade, self.env, self.prim_lod)
                mat['color'] = c
                mat['alpha'] = 'OPAQUE' if c[3] > 0.98 else 'BLEND'
            self.materials[key] = mat
        return key


# ---------------------------------------------------------------------------------------------- fighters

def read_mobjs(files, ref):
    """The MObjSub list of one joint (NULL-terminated MObjSub *[])."""
    out = []
    k = 0
    while ref:
        sub = files.ptr(ref, 4 * k)
        if not sub:
            break
        d = files.data(sub[0])
        o = sub[1]
        f = lambda off: struct.unpack_from('>f', d, o + off)[0]
        h = lambda off: struct.unpack_from('>H', d, o + off)[0]
        col = lambda off: [c / 255 for c in d[o + off:o + off + 4]]
        pal = files.ptr(sub, 0x2C)
        out.append(dict(fmt=d[o + 2], siz=d[o + 3], sprites=files.ptr(sub, 4), unk08=h(8), unk0A=h(0xA), unk0C=h(0xC), unk0E=h(0xE),
                        unk10=struct.unpack_from('>i', d, o + 0x10)[0], trau=f(0x14), trav=f(0x18), scau=f(0x1C), scav=f(0x20),
                        palettes=pal, palette=files.ptr(pal, 0) if pal else None, flags=h(0x30), block_fmt=d[o + 0x32], block_siz=d[o + 0x33],
                        block_dxt=h(0x34), unk36=h(0x36), prim=col(0x50), lfrac=d[o + 0x54] / 255.0, env=col(0x58),
                        light1=col(0x60)[:3], light2=col(0x64)[:3], sub=sub))
        k += 1
    return out


def costume(files, mobj, script):
    """Costume 0 of a material script (AObjEvent32 at frame 0): the values the first block sets."""
    if not script:
        return
    d = files.data(script[0])
    o = script[1]
    for _ in range(64):
        w = struct.unpack_from('>I', d, o)[0]
        op, flags, payload = w >> 25, (w >> 15) & 0x3FF, w & 0x7FFF
        o += 4
        if payload and op in (3, 4, 5, 6, 8, 9, 10, 11, 18, 19, 20, 21):
            break  # this value is only reached after `payload` frames: frame 0 keeps the earlier one
        if op in (3, 4, 5, 6, 8, 9, 10, 11):  # SetVal* on material tracks 13..22
            rate = op in (5, 6)
            for i in range(10):
                if flags >> i & 1:
                    v = struct.unpack_from('>f', d, o)[0]
                    o += 4
                    if rate:
                        o += 4
                    if i == 9:
                        mobj['palette_id'] = int(v)
                    elif i == 0:
                        mobj['texture_id'] = int(v)
        elif op in (18, 19, 20, 21):  # SetExtVal*: colour tracks 37..41
            for i in range(5):
                if flags >> i & 1:
                    c = [x / 255 for x in d[o:o + 4]]
                    o += 4
                    key = ['prim', 'env', 'blend', 'light1', 'light2'][i]
                    mobj[key] = c if i < 3 else c[:3]
        elif op == 7:
            o += 4 * bin(flags).count('1')
        elif op == 15:
            pass
        else:
            break
        if op == 2 or op == 0:
            break
    if mobj.get('palettes') and 'palette_id' in mobj:
        mobj['palette'] = files.ptr(mobj['palettes'], 4 * mobj['palette_id'])
    if mobj.get('sprites') and mobj.get('texture_id'):
        pass  # texture_id_curr: the MObj DL reads sprites[texture_id]; joint scripts start at 0 for costume 0


class Fighter:
    def __init__(self, files, names, name, table):
        self.files, self.name = files, name
        main = (table['main'], 0)
        attr = (table['main'], table['attr'])
        self.attr = attr
        self.size = struct.unpack('>f', files.data(main[0])[table['attr']:table['attr'] + 4])[0]
        cpc = files.ptr(attr, ATTR_COMMONPARTS)
        setup = files.ptr(attr, ATTR_SETUP_PARTS)
        flags = (files.u32(setup, 0), files.u32(setup, 4)) if setup else (0xFFFFFFFF, 0xFFFFFFFF)
        desc = files.ptr(cpc, 0)
        self.mobj_table = files.ptr(cpc, 4)
        self.costume_table = files.ptr(cpc, 8)
        # FTCommonPart.flags & 0xF == 1 (Yoshi, Master Hand): each joint's dl is a Gfx *[2]; ftDisplayMainDrawDefault draws
        # dls[0] with the parent's matrix (vertices that belong to the parent) and dls[1], after the MObjs, with its own.
        self.split = files.data(cpc[0])[cpc[1] + 12] & 0xF == 1
        # lbCommonSetupFighterPartsDObjs: id & 0xFFF is the depth, the parent is the last joint one level up
        self.joints = []
        stack = {}
        dd = files.data(desc[0])
        i = 0
        while True:
            o = desc[1] + 44 * i
            jid = struct.unpack_from('>i', dd, o)[0]
            if jid == DOBJ_ARRAY_MAX:
                break
            bit = flags[0] >> (31 - i) & 1 if i < 32 else flags[1] >> (63 - i) & 1
            if bit:
                depth = jid & 0xFFF
                parent = stack.get(depth - 1) if depth else None
                vals = struct.unpack_from('>9f', dd, o + 8)
                j = dict(index=i, parent=parent, depth=depth, dl=files.ptr(desc, 44 * i + 4), t=list(vals[0:3]), r=list(vals[3:6]), s=list(vals[6:9]))
                stack[depth] = len(self.joints)
                self.joints.append(j)
            i += 1
        for j in self.joints:
            local = mat_trs(j['t'], j['r'], j['s'])
            j['world'] = mat_mul(self.joints[j['parent']]['world'], local) if j['parent'] is not None else local
        self.motions = table['motions']

    def mesh(self):
        files = self.files
        m = Mesh(files)
        for j in self.joints:
            mlist = files.ptr(self.mobj_table, 4 * j['index']) if self.mobj_table else None
            clist = files.ptr(self.costume_table, 4 * j['index']) if self.costume_table else None
            ji = self.joints.index(j)
            if self.split:
                if not j['dl']:
                    continue
                pre, own = files.ptr(j['dl'], 0), files.ptr(j['dl'], 4)
                if pre:
                    p = j['parent'] if j['parent'] is not None else ji
                    m.run(pre, p, self.joints[p]['world'])
                m.mobjs = read_mobjs(files, mlist)
                for k, mo in enumerate(m.mobjs):
                    costume(files, mo, files.ptr(clist, 4 * k) if clist else None)
                if own:
                    m.run(own, ji, j['world'])
                continue
            m.mobjs = read_mobjs(files, mlist)
            for k, mo in enumerate(m.mobjs):
                costume(files, mo, files.ptr(clist, 4 * k) if clist else None)
            if j['dl']:
                m.run(j['dl'], ji, j['world'])
        return m

    def animation(self, fid):
        """ftAnimParseDObjFigatree + gcPlayDObjAnimJoint per joint: [(frames x (t, r, s)) per joint], frame count."""
        files = self.files
        d, ptrs = files.get(fid)
        tracks = []
        length = 0
        for ji, j in enumerate(self.joints):
            script = ptrs.get(4 * ji)
            tracks.append(simulate(d, script[1], j) if script and script[0] == fid else None)
            if tracks[-1]:
                length = max(length, len(tracks[-1]))
        return tracks, max(length, 1)


FRACS = [1 / 512, 1 / 4, 1 / 4096, 1 / 16384, 1 / 512, 1 / 32, 1 / 8192, 1 / 16384]


def simulate(data, pos, joint):
    """Frame-by-frame run of one joint's AObjEvent16 script; returns [(t, r, s)] per frame until it ends."""
    t, r, s = list(joint['t']), list(joint['r']), list(joint['s'])
    aobjs = {}
    wait = 0.0
    frames = []

    def h(p):
        return struct.unpack_from('>H', data, p)[0]

    def sh(p):
        return struct.unpack_from('>h', data, p)[0]

    def value(raw, track, step):
        kind = 0 if track < 3 else (1 if track in (4, 5, 6) else (2 if track > 6 else 3))
        return raw * FRACS[kind + (4 if step else 0)]

    ended = False
    for frame in range(2000):
        if frame > 0:
            if ended:
                break
            wait -= 1.0
        while wait <= 0.0 and not ended:
            cmd = h(pos)
            op, flags, toggle = cmd >> 11, (cmd >> 1) & 0x3FF, cmd & 1
            pos += 2
            payload = 0.0
            if op in (2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 14):
                if toggle:
                    payload = float(h(pos))
                    pos += 2
            if op in (2, 3, 4, 5, 7, 8, 9, 10):
                for i in range(10):
                    if not flags:
                        break
                    if flags & 1:
                        a = aobjs.setdefault(i, dict(kind=0, vb=0.0, vt=0.0, rb=0.0, rt=0.0, length=0.0, inv=1.0))
                        if op == 6:
                            a['rt'] = value(sh(pos), i, True)
                            pos += 2
                        else:
                            a['vb'] = a['vt']
                            a['vt'] = value(sh(pos), i, False)
                            pos += 2
                            if op in (4, 5):
                                a['rb'] = a['rt']
                                a['rt'] = value(sh(pos), i, True)
                                pos += 2
                                a['kind'] = 3
                                if payload:
                                    a['inv'] = 1.0 / payload
                            elif op in (7, 8):
                                a['rb'] = a['rt']
                                a['rt'] = 0.0
                                a['kind'] = 3
                                if payload:
                                    a['inv'] = 1.0 / payload
                            elif op in (2, 3):
                                a['kind'] = 2
                                if payload:
                                    a['rb'] = (a['vt'] - a['vb']) / payload
                                a['rt'] = 0.0
                            else:  # 9, 10 step
                                a['kind'] = 1
                                a['inv'] = payload
                                a['rt'] = 0.0
                            a['length'] = -wait - 1.0
                    flags >>= 1
                if op in (2, 4, 7, 9):
                    wait += payload
            elif op == 6:
                for i in range(10):
                    if not flags:
                        break
                    if flags & 1:
                        a = aobjs.setdefault(i, dict(kind=0, vb=0.0, vt=0.0, rb=0.0, rt=0.0, length=0.0, inv=1.0))
                        a['rt'] = value(sh(pos), i, True)
                        pos += 2
                    flags >>= 1
            elif op == 1:  # Block
                if toggle:
                    wait += float(h(pos))
                    pos += 2
            elif op == 11:  # length adjust
                for i in range(10):
                    if not flags:
                        break
                    if flags & 1 and i in aobjs:
                        aobjs[i]['length'] += payload
                    flags >>= 1
            elif op == 12:  # SetTranslateInterp (spline offset): not used by the exported motions
                pos += 2
            elif op == 13:  # Loop: the motion repeats; one pass is exported
                ended = True
                break
            elif op == 0:
                ended = True
                for a in aobjs.values():
                    if a['kind']:
                        a['length'] += 1.0 + wait
                break
            elif op == 14:  # SetFlags
                if toggle:
                    pass
            else:
                ended = True
                break
        # gcPlayDObjAnimJoint
        for i, a in aobjs.items():
            if not a['kind']:
                continue
            if not ended:
                a['length'] += 1.0
            L = a['length']
            if a['kind'] == 2:
                v = a['vb'] + L * a['rb']
            elif a['kind'] == 3:
                inv = a['inv']
                f16, f12 = inv * inv, L * L
                f18 = inv * f12
                f14 = L * f12 * f16
                f20 = 2.0 * f14 * inv
                f22 = 3.0 * f12 * f16
                f24 = f14 - f18
                v = a['vb'] * ((f20 - f22) + 1.0) + a['vt'] * (f22 - f20) + a['rb'] * ((f24 - f18) + L) + a['rt'] * f24
            else:
                v = a['vt'] if a['inv'] <= L else a['vb']
            if i < 3:
                r[i] = v
            elif 4 <= i <= 6:
                t[i - 4] = v
            elif i >= 7:
                s[i - 7] = v
        frames.append((list(t), list(r), list(s)))
    return frames


# ---------------------------------------------------------------------------------------------- glTF

class Gltf:
    def __init__(self):
        self.bin = bytearray()
        self.j = {'asset': {'version': '2.0', 'generator': 'jumpman export-ssb64.py'}, 'scene': 0, 'scenes': [{'nodes': []}],
                  'nodes': [], 'meshes': [], 'buffers': [], 'bufferViews': [], 'accessors': [], 'materials': [], 'textures': [],
                  'images': [], 'samplers': [], 'skins': [], 'animations': []}

    def view(self, data, target=None):
        while len(self.bin) % 4:
            self.bin.append(0)
        v = {'buffer': 0, 'byteOffset': len(self.bin), 'byteLength': len(data)}
        if target:
            v['target'] = target
        self.bin += data
        self.j['bufferViews'].append(v)
        return len(self.j['bufferViews']) - 1

    def accessor(self, rows, kind, ctype=5126, target=None, minmax=False, normalized=False):
        fmt = {5126: 'f', 5121: 'B', 5123: 'H', 5125: 'I'}[ctype]
        flat = [x for row in rows for x in (row if isinstance(row, (list, tuple)) else [row])]
        data = struct.pack('<%d%s' % (len(flat), fmt), *flat)
        a = {'bufferView': self.view(data, target), 'componentType': ctype, 'count': len(rows), 'type': kind}
        if normalized:
            a['normalized'] = True
        if minmax:
            n = len(rows[0]) if isinstance(rows[0], (list, tuple)) else 1
            cols = [[(row if isinstance(row, (list, tuple)) else [row])[k] for row in rows] for k in range(n)]
            a['min'] = [min(c) for c in cols]
            a['max'] = [max(c) for c in cols]
        self.j['accessors'].append(a)
        return len(self.j['accessors']) - 1

    def glb(self):
        j = {k: v for k, v in self.j.items() if v != []}
        j['buffers'] = [{'byteLength': len(self.bin)}]
        js = json.dumps(j, separators=(',', ':')).encode()
        js += b' ' * (-len(js) % 4)
        b = bytes(self.bin) + b'\0' * (-len(self.bin) % 4)
        return (struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(b)) + struct.pack('<II', len(js), 0x4E4F534A) + js
                + struct.pack('<II', len(b), 0x004E4942) + b)


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def linear(c):
    """An RGBA colour in display (sRGB) space, as the RDP blends it, to glTF's linear factors / COLOR_0; alpha unchanged.
    The texture PNGs stay sRGB (the loader decodes them), so flat parts and textured parts match."""
    return [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c[:3]] + list(c[3:])


def export_fighter(files, names, cmotions, name, table, out_dir, clip_filter=None, log=print):
    fighter = Fighter(files, names, name, table)
    mesh = fighter.mesh()
    g = Gltf()
    J = fighter.joints
    scale = UNIT * fighter.size
    root_m = [[scale, 0, 0, 0], [0, scale, 0, 0], [0, 0, scale, 0], [0, 0, 0, 1.0]]
    # nodes: 0 = mesh, 1 = armature root (unit scale), 2.. joints
    g.j['nodes'].append({'name': f'{name}_mesh', 'mesh': 0, 'skin': 0})
    g.j['nodes'].append({'name': name, 'scale': [scale] * 3, 'children': []})
    g.j['scene'] = 0
    g.j['scenes'][0]['nodes'] = [0, 1]
    for ji, j in enumerate(J):
        node = {'name': f'joint_{j["index"]:02d}', 'translation': [f32(x) for x in j['t']], 'rotation': [f32(x) for x in quat(j['r'])]}
        if j['s'] != [1.0, 1.0, 1.0]:
            node['scale'] = [f32(x) for x in j['s']]
        g.j['nodes'].append(node)
        parent = 1 if j['parent'] is None else 2 + j['parent']
        g.j['nodes'][parent].setdefault('children', []).append(2 + ji)
    # mesh primitives, one per material
    prims = []
    image_index = {}
    for mi, key in enumerate(mesh.tris):  # materials in the order the display lists first use them
        idx = mesh.tris[key]
        mat = mesh.materials[key]
        vids = sorted(set(idx))
        remap = {v: i for i, v in enumerate(vids)}
        pos, nrm, uv, jt, wt, col = [], [], [], [], [], []
        for v in vids:
            joint, p, n, t, c = mesh.verts[v]
            pos.append([f32(x * scale) for x in p])
            if n is None:
                n = [0.0, 1.0, 0.0]
            ln = math.sqrt(sum(x * x for x in n)) or 1.0
            nrm.append([f32(x / ln) for x in n])
            uv.append([f32(x) for x in t])
            jt.append([joint, 0, 0, 0])
            wt.append([1.0, 0.0, 0.0, 0.0])
            col.append([f32(x) for x in linear(c or [1.0, 1.0, 1.0, 1.0])])
        attrs = {'POSITION': g.accessor(pos, 'VEC3', target=34962, minmax=True), 'NORMAL': g.accessor(nrm, 'VEC3', target=34962),
                 'JOINTS_0': g.accessor(jt, 'VEC4', 5121, 34962), 'WEIGHTS_0': g.accessor(wt, 'VEC4', target=34962)}
        if 'image' in mat:
            attrs['TEXCOORD_0'] = g.accessor(uv, 'VEC2', target=34962)
        if mat['vcol']:
            attrs['COLOR_0'] = g.accessor(col, 'VEC4', target=34962)
        indices = g.accessor([remap[i] for i in idx], 'SCALAR', 5123 if len(vids) < 65536 else 5125, 34963)
        m = {'name': f'mat{mi:03d}', 'pbrMetallicRoughness': {'baseColorFactor': [f32(x) for x in linear(mat['color'])], 'metallicFactor': 0.0, 'roughnessFactor': 1.0}}
        if mat['double']:
            m['doubleSided'] = True
        if mat['alpha'] != 'OPAQUE':
            m['alphaMode'] = mat['alpha']
            if mat['alpha'] == 'MASK':
                m['alphaCutoff'] = 0.5
        if 'image' in mat:
            w, h, px = mat['image']
            data = png(w, h, px)
            ik = (data, mat['wrap'])
            if ik not in image_index:
                g.j['images'].append({'bufferView': g.view(data), 'mimeType': 'image/png'})
                sampler = {'magFilter': 9729, 'minFilter': 9729, 'wrapS': mat['wrap'][0], 'wrapT': mat['wrap'][1]}
                if sampler not in g.j['samplers']:
                    g.j['samplers'].append(sampler)
                g.j['textures'].append({'source': len(g.j['images']) - 1, 'sampler': g.j['samplers'].index(sampler)})
                image_index[ik] = len(g.j['textures']) - 1
            m['pbrMetallicRoughness']['baseColorTexture'] = {'index': image_index[ik]}
        g.j['materials'].append(m)
        prims.append({'attributes': attrs, 'indices': indices, 'material': len(g.j['materials']) - 1})
    g.j['meshes'].append({'name': name, 'primitives': prims})
    ibm = [mat_inv(mat_mul(root_m, j['world'])) for j in J]
    g.j['skins'].append({'joints': [2 + i for i in range(len(J))], 'skeleton': 1,
                         'inverseBindMatrices': g.accessor([[f32(m[r][c]) for c in range(4) for r in range(4)] for m in ibm], 'MAT4')})
    # motions: one clip per distinct animation file. A clip is named after the first common motion (src/ft/ftdef.h
    # FTCommonMotion, from Wait on) that plays it; fighter-specific motions and Master Hand use the file name.
    clips = {}
    motion_map = {}
    special = next(k for k, v in cmotions.items() if v == 'SpecialStart')
    users = {}
    for mid, anim in enumerate(fighter.motions):
        if anim is not None and anim in names:
            users.setdefault(anim, []).append(mid)
    clip_of = {}
    for anim, mids in sorted(users.items(), key=lambda kv: kv[1][0]):
        stem = anim.split('Anim', 1)[1] if anim.startswith('FT') and 'Anim' in anim else anim  # Luigi plays some FTMarioAnim files
        common = [m for m in mids if 4 <= m < special]
        clip = cmotions[min(common)] if common and name != 'Boss' else stem
        if clip in clip_of.values():
            clip = f'{clip}_{stem}'
        clip_of[anim] = clip
    time_cache = {}
    for anim, mids in sorted(users.items(), key=lambda kv: kv[1][0]):
        clip = clip_of[anim]
        for mid in mids:
            motion_map[mid] = clip
        if clip_filter and clip not in clip_filter:
            continue
        tracks, length = fighter.animation(names[anim])
        channels, samplers = [], []
        n = max(length, 2)
        if n not in time_cache:
            time_cache[n] = g.accessor([f32(i / FPS) for i in range(n)], 'SCALAR', minmax=True)
        times = time_cache[n]
        for ji, j in enumerate(J):
            fr = tracks[ji] or [(j['t'], j['r'], j['s'])]
            fr = fr + [fr[-1]] * (n - len(fr))
            for path, kind, rows in (('translation', 'VEC3', [[f32(x) for x in f[0]] for f in fr]),
                                     ('rotation', 'VEC4', [[f32(x) for x in quat(f[1])] for f in fr]),
                                     ('scale', 'VEC3', [[f32(x) for x in f[2]] for f in fr])):
                # every joint gets all three channels: an engine keeps whatever the previous clip left in an unkeyed one
                inp = times
                if all(row == rows[0] for row in rows):  # a held pose: two keys are enough
                    rows = [rows[0], rows[0]]
                    key = ('held', n)
                    if key not in time_cache:
                        time_cache[key] = g.accessor([0.0, f32((n - 1) / FPS)], 'SCALAR', minmax=True)
                    inp = time_cache[key]
                samplers.append({'input': inp, 'output': g.accessor(rows, kind), 'interpolation': 'LINEAR'})
                channels.append({'sampler': len(samplers) - 1, 'target': {'node': 2 + ji, 'path': path}})
        g.j['animations'].append({'name': clip, 'channels': channels, 'samplers': samplers})
        clips[clip] = {'frames': length, 'seconds': round(length / FPS, 4), 'file': names[anim], 'source': anim}
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / f'{name.lower()}.glb').write_bytes(g.glb())
    if mesh.unknown:
        log(f'{name}: unhandled display list opcodes {sorted(x if isinstance(x, str) else hex(x) for x in mesh.unknown)}')
    specials = table['specials']
    named = {str(mid): {'clip': clip, 'motion': cmotions[mid] if mid < special else
                        (specials[mid - special] if mid - special < len(specials) else f'Special{mid - special}')}
             for mid, clip in sorted(motion_map.items())}
    return {'label': LABELS[name], 'file': f'{name.lower()}.glb', 'joints': len(J), 'vertices': len(mesh.verts),
            'triangles': sum(len(v) for v in mesh.tris.values()) // 3, 'materials': len(mesh.materials), 'size': fighter.size,
            'unit': UNIT, 'clips': clips, 'motions': named, 'attributes': attributes(files, table)}


# FTAttributes fields (src/ft/fttypes.h) a controller can tune a fighter with: byte offset -> name (f32 unless noted)
ATTRIBUTES = [(0x08, 'walkmiddle_anim_length'), (0x20, 'walk_speed_mul'), (0x24, 'traction'), (0x28, 'dash_speed'),
              (0x30, 'run_speed'), (0x34, 'kneebend_anim_length'), (0x38, 'jump_vel_x'), (0x3C, 'jump_height_mul'),
              (0x40, 'jump_height_base'), (0x48, 'jumpaerial_height'), (0x4C, 'air_accel'), (0x50, 'air_speed_max_x'),
              (0x58, 'gravity'), (0x5C, 'tvel_base'), (0x60, 'tvel_fast'), (0x64, 'jumps_max'), (0x68, 'weight')]


def attributes(files, table):
    d, a = files.data(table['main']), table['attr']
    return {k: struct.unpack_from('>i' if k == 'jumps_max' else '>f', d, a + o)[0] for o, k in ATTRIBUTES}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rom', type=Path, default=ROM)
    ap.add_argument('--out', type=Path, default=OUT)
    ap.add_argument('--fighters', default=','.join(FIGHTERS))
    ap.add_argument('--clips', default='', help='comma-separated clip names (debug: export only these)')
    a = ap.parse_args()
    rom = a.rom.read_bytes()
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise SystemExit(f'{a.rom} is not Super Smash Bros. (US) (sha1 {SHA1})')
    files = Files(rom)
    names = file_names()
    cmotions = common_motions()
    tables = fighter_tables(names)
    index = {'game': 'ssb64', 'fps': FPS, 'fighters': {}}
    for name in a.fighters.split(','):
        info = export_fighter(files, names, cmotions, name, tables[name], a.out, set(a.clips.split(',')) if a.clips else None)
        index['fighters'][name.lower()] = info
        print(f'{name}: {info["joints"]} joints, {info["triangles"]} triangles, {info["materials"]} materials, {len(info["clips"])} clips', flush=True)
    (a.out / 'fighters.json').write_text(json.dumps(index, indent=1, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()
