"""Table-driven Mario Kart 64 course exporter (helper for export-mk64.py).

For every course in the n64decomp/mk64 `courses/` tree this produces
  * a walkable course for the regular SM64 engine: `assets/courses/mk64_<dir>.json` (collision triangles with
    SM64 surface kinds derived from the MK64 surface types, spawn, sky/fog) and the visual
    `assets/levels/mk64_<dir>/terrain.glb` (one primitive per material, textures embedded);
  * the helpers export-mk64.py uses for the kart-mode JSON (`public/mk64/<dir>.json`).

Collision is what the game itself builds at load time (`course_generate_collision_mesh`): the triangles of the
display lists named by the course's `TrackSections` table (plus a few per-course extra lists), each with the
section's surface type.  Only the Python standard library is used.
"""
import json
import math
import re
import struct
import zlib

# dir -> (texture symbol prefix, display name, kind).  `kind` is `race` or `battle`.
COURSES = {
    'mario_raceway': ('MR', 'Mario Raceway', 'race'),
    'luigi_raceway': ('LR', 'Luigi Raceway', 'race'),
    'koopa_troopa_beach': ('KTB', 'Koopa Troopa Beach', 'race'),
    'moo_moo_farm': ('MMF', 'Moo Moo Farm', 'race'),
    'choco_mountain': ('CM', 'Choco Mountain', 'race'),
    'kalimari_desert': ('KD', 'Kalimari Desert', 'race'),
    'toads_turnpike': ('TT', "Toad's Turnpike", 'race'),
    'frappe_snowland': ('FS', 'Frappe Snowland', 'race'),
    'wario_stadium': ('WS', 'Wario Stadium', 'race'),
    'sherbet_land': ('SL', 'Sherbet Land', 'race'),
    'royal_raceway': ('RRW', 'Royal Raceway', 'race'),
    'bowsers_castle': ('BC', "Bowser's Castle", 'race'),
    'dks_jungle_parkway': ('DKJ', "D.K.'s Jungle Parkway", 'race'),
    'yoshi_valley': ('YV', 'Yoshi Valley', 'race'),
    'banshee_boardwalk': ('BB', 'Banshee Boardwalk', 'race'),
    'rainbow_road': ('RR', 'Rainbow Road', 'race'),
    'big_donut': ('BD', 'Big Donut', 'battle'),
    'block_fort': ('BF', 'Block Fort', 'battle'),
    'double_deck': ('DD', 'Double Deck', 'battle'),
    'skyscraper': ('SS', 'Skyscraper', 'battle'),
}

# Collision display lists that are not in the course's TrackSections table (render_courses.c
# `course_generate_collision_mesh`): (packed dl offset, surface type).
EXTRA_COLLISION = {
    'mario_raceway': [(0x1140, -1), (0x08E8, -1)],
    'block_fort': [(0x15C0, 'ASPHALT')],
    'skyscraper': [(0x1110, 'ASPHALT'), (0x0258, 'ASPHALT')],
    'double_deck': [(0x0738, 'ASPHALT')],
    'big_donut': [(0x1018, 'BRIDGE'), (0x0450, 'BRIDGE'), (0x0AC0, 'BRIDGE'), (0x0B58, 'BRIDGE'), (0x0230, 'BRIDGE')],
}

# Display lists whose vertex alpha the game sets at load (`find_vtx_and_set_colours`): water, glass, translucent
# haze.  (packed dl offset, alpha).
TRANSLUCENT = {
    'koopa_troopa_beach': [(0xADE0, 150), (0xA540, 150), (0x9E70, 150), (0x0358, 150)],
    'sherbet_land': [(0x1EB8, 180), (0x2308, 150)],
    'rainbow_road': [(0x2068, 150), (0x1E18, 150), (0x1318, 255), (0x1FB8, 150)],
    'wario_stadium': [(0x0C50, 100), (0x0BD8, 100), (0x0B60, 100), (0x0AE8, 100), (0x0CC8, 100), (0x0D50, 100),
                      (0x0DD0, 100), (0x0E48, 100)],
    'dks_jungle_parkway': [(0x3FA8, 120)],
}

# MK64 surface type -> SM64 surface kind of the walkable course.  Road-like surfaces are SM64's default surface
# (what Luigi Raceway's asphalt is in the kart: `kart.rs` `Floor.grass` is `kind == NOT_SLIPPERY`), off-road ground is
# "not slippery" (grass-like: the kart loses thrust there, Mario never slides), snow and ice slip, out of bounds kills.
SM64_DEFAULT, SM64_NOT_SLIPPERY, SM64_SLIPPERY, SM64_VERY_SLIPPERY, SM64_DEATH_PLANE = 0x00, 0x15, 0x14, 0x13, 0x0A
OFF_ROAD = ('SAND_OFFROAD', 'GRASS', 'DIRT_OFFROAD', 'SNOW_OFFROAD')  # mk64.h: SAND / WET_SAND / SNOW are the road
SURFACE_KIND = {name: SM64_NOT_SLIPPERY for name in OFF_ROAD}
SURFACE_KIND.update({'SNOW': SM64_SLIPPERY, 'ICE': SM64_VERY_SLIPPERY, 'OUT_OF_BOUNDS': SM64_DEATH_PLANE})

# Hand-picked look per course (the game computes its sky at run time): sky top/mid/bottom, fog colour,
# fog near/far in world units.  Approximations of the in-game palettes.
LOOK = {
    'mario_raceway': (0x3f8fd6, 0x9ed4f5, 0xe8f6ff, 0xc7e8ff, 12000, 30000),
    'luigi_raceway': (0x2f6fc0, 0x8fc3ee, 0xe0f0fb, 0xbfe0f5, 12000, 30000),
    'koopa_troopa_beach': (0x2a9fe8, 0x8fdcf5, 0xf0fbff, 0xbfeaf7, 14000, 34000),
    'moo_moo_farm': (0x4a92dc, 0xa8d8f5, 0xeaf7ff, 0xcfeaf8, 12000, 30000),
    'choco_mountain': (0x6a4a38, 0xb88458, 0xe8c090, 0xc09870, 9000, 24000),
    'kalimari_desert': (0x3a2a6e, 0xd9825b, 0xf2c28a, 0xe8b88a, 14000, 34000),
    'toads_turnpike': (0x05061a, 0x14183a, 0x2a2f5a, 0x14183a, 8000, 22000),
    'frappe_snowland': (0x8fb6e0, 0xcfe2f2, 0xf0f6fb, 0xe6f0f8, 9000, 24000),
    'wario_stadium': (0x04051c, 0x10143a, 0x242a58, 0x10143a, 8000, 22000),
    'sherbet_land': (0x10204a, 0x3a4f8a, 0x9fb4d8, 0x6a82b8, 9000, 24000),
    'royal_raceway': (0x4a3a8c, 0xe08a6a, 0xf5c890, 0xe8a888, 12000, 30000),
    'bowsers_castle': (0x050308, 0x20080c, 0x502010, 0x180606, 7000, 20000),
    'dks_jungle_parkway': (0x2a4a7a, 0x6a9ac0, 0xc8d8a8, 0x9ab8a0, 10000, 26000),
    'yoshi_valley': (0x3a88d8, 0x9cd0f0, 0xe4f4ff, 0xc4e4f6, 12000, 30000),
    'banshee_boardwalk': (0x000004, 0x080818, 0x101028, 0x08081a, 6000, 18000),
    'rainbow_road': (0x000008, 0x05051a, 0x0a0a30, 0x05051a, 20000, 60000),
    'big_donut': (0x5a3a7a, 0xe08aa0, 0xf8d0b0, 0xd89aa8, 10000, 26000),
    'block_fort': (0x3f8fd6, 0x9ed4f5, 0xe8f6ff, 0xc7e8ff, 10000, 26000),
    'double_deck': (0x3f8fd6, 0x9ed4f5, 0xe8f6ff, 0xc7e8ff, 10000, 26000),
    'skyscraper': (0x050818, 0x141e48, 0x3a4a80, 0x141e48, 8000, 22000),
}

WALK_SCALE = 8  # world (SM64) units per MK64 course unit; shared with kart.rs `UNIT`.

TEX_RE = re.compile(r'^g(RRW|RR|KTB|MMF|DKJ|BB|BD|BF|BC|CM|DD|FS|KD|LR|MR|SL|SS|TT|WS|YV)Texture(\w+)$')
VERTEX_RE = re.compile(
    r'\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},\s*\{\s*(-?\d+),\s*(-?\d+)\s*\},'
    r'\s*\{\s*MACRO_COLOR_FLAG\((0x\w+),\s*(0x\w+),\s*(0x\w+),\s*(\w+)\)')
COMMAND_RE = re.compile(r'(gs\w+)\((.*?)\)', re.S)


def to_int(value):
    try:
        return int(value, 0)
    except ValueError:
        return 0


# ---------------------------------------------------------------------------------------------------------------
# Textures


class Textures:
    """Resolves the `g<Course>Texture<name>` symbols of the display lists to RGBA images from the ROM."""

    def __init__(self, rom, source, mio0, decode):
        self.rom, self.source, self.mio0, self.decode = rom, source, mio0, decode
        self.assets = json.loads((source / 'assets.json').read_text())
        self.aliases = {}
        for key, name, kind in re.findall(r'"(gTexture\w+)":\s*\("([^"]+)",\s*"(\w+)"\)',
                                          (source / 'tools/linkonly_generator.py').read_text()):
            self.aliases[key] = (name, kind)
        self.named = {}
        for path in sorted((source / 'assets/courses').glob('*.json')):
            self.named.update(json.loads(path.read_text()))
        self.blocks = {}
        self.cache = {}

    def block(self, start):
        if start not in self.blocks:
            self.blocks[start] = self.mio0(self.rom[start:])
        return self.blocks[start]

    def image(self, symbol):
        """-> (width, height, rgba bytes) or None."""
        if symbol in self.cache:
            return self.cache[symbol]
        result = None
        match = TEX_RE.match(symbol)
        base = 'gTexture' + match.group(2) if match else symbol
        if base in self.aliases:
            name, kind = self.aliases[base]
            entry = self.assets.get(f'textures/standalone/{name}.{kind}.png')
            if entry:
                w, h = entry['meta']['dims']
                start, offset = [int(v, 0) for v in entry['offsets']['us']]
                data = self.block(start)[offset:offset + w * h * 2]
                result = (w, h, bytes(self.decode(kind, data, w, h)))
        if result is None and base in self.named:
            entry = self.named[base]
            w, h, kind = entry['width'], entry['height'], entry['type']
            if kind in ('rgba16', 'ia16'):
                start = int(entry['rom_offset'], 0)
                offset = int(entry.get('block_offset', '0'), 0)
                data = self.block(start)[offset:offset + w * h * 2]
                result = (w, h, bytes(self.decode(kind, data, w, h)))
        self.cache[symbol] = result
        return result


# ---------------------------------------------------------------------------------------------------------------
# Course parsing


def wrap_mode(flags):
    if 'G_TX_CLAMP' in flags:
        return 'clamp'
    if 'G_TX_MIRROR' in flags and 'G_TX_NOMIRROR' not in flags:
        return 'mirror'
    return 'wrap'


class Course:
    def __init__(self, source, name):
        self.name = name
        self.dir = source / 'courses' / name
        self.abbr = COURSES[name][0]
        text = (self.dir / 'course_vertices.inc.c').read_text()
        # x y z u v r g b flag
        self.vertices = [[int(r[0]), int(r[1]), int(r[2]), int(r[3]), int(r[4]), to_int(r[5]), to_int(r[6]),
                          to_int(r[7]), to_int(r[8])] for r in VERTEX_RE.findall(text)]
        self.lists = {}
        self.order = []
        displaylists = (self.dir / 'course_displaylists.inc.c').read_text()
        for dl, body in re.findall(r'Gfx (\w+)\[\] = \{(.*?)\n\};', displaylists, re.S):
            self.lists[dl] = [(c, [a.strip() for a in args.split(',')]) for c, args in COMMAND_RE.findall(body)]
            self.order.append(dl)
        self.data_text = (self.dir / 'course_data.c').read_text()

    def packed(self, offset):
        return f'd_course_{self.name}_packed_dl_{offset:X}'

    def draw_order(self):
        """Display lists in the order the game reaches them from the course's sector lists, then the rest."""
        seen, order = set(), []

        def visit(dl):
            if dl in seen or dl not in self.lists:
                return
            seen.add(dl)
            order.append(dl)
            for command, args in self.lists[dl]:
                if command == 'gsSPDisplayList':
                    visit(args[0])
        for dl in re.findall(r'gsSPDisplayList\((d_course_\w+)\)', self.data_text):
            visit(dl)
        for dl in self.order:
            visit(dl)
        return order

    def sections(self):
        """[(display list, surface name or -1, section id, flags)] from the `TrackSections` table + extras."""
        match = re.search(r'TrackSections\s+\w+\[\]\s*=\s*\{(.*?)\n\};', self.data_text, re.S)
        rows = []
        if match:
            for dl, surface, section, flags in re.findall(
                    r'\{\s*(d_course_\w+),\s*(\w+),\s*(\d+),\s*(0x[0-9A-Fa-f]+)\s*\}', match.group(1)):
                rows.append((dl, surface, int(section), int(flags, 16)))
        for offset, surface in EXTRA_COLLISION.get(self.name, []):
            rows.append((self.packed(offset), surface, 0xFF, 0))
        return rows

    def track_path(self):
        match = re.search(r'TrackPathPoint\s+d_course_\w+_track_path\[\]\s*=\s*\{(.*?)\};', self.data_text, re.S)
        if not match:
            return []
        return [[int(v) for v in row] for row in
                re.findall(r'\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*\d+\s*\}', match.group(1))]

    def item_boxes(self):
        match = re.search(r'ActorSpawnData\s+d_course_\w+_item_box_spawns\[\]\s*=\s*\{(.*?)\};', self.data_text, re.S)
        if not match:
            return []
        rows = re.findall(r'\{\s*\{\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\s*\},', match.group(1))
        return [[int(v) for v in row] for row in rows if int(row[0]) != -32768]

    def run(self, dl, visit, state=None, depth=0):
        """Interpret a display list (recursing into sub-lists).  `visit(tri, state)` is called per triangle with
        the three global vertex indices and the texture state."""
        if state is None:
            state = {'cache': {}, 'texture': None, 'wrap_s': 'wrap', 'wrap_t': 'wrap'}
        for command, args in self.lists.get(dl, []):
            if command == 'gsSPDisplayList':
                if depth < 16:
                    self.run(args[0], visit, state, depth + 1)
            elif command == 'gsDPSetTextureImage':
                state['texture'] = args[3]
            elif command == 'gsDPSetTile' and args[4] == 'G_TX_RENDERTILE':
                state['wrap_t'] = wrap_mode(args[6])
                state['wrap_s'] = wrap_mode(args[9])
            elif command == 'gsDPSetCombineMode' and 'SHADE' in args[0] and 'TEXEL' not in args[0]:
                state['texture'] = None
            elif command == 'gsSPVertex':
                address, count, slot = [int(v, 0) for v in args]
                base = (address & 0xffffff) // 16
                state['cache'].update({slot + i: base + i for i in range(count)})
            elif command in ('gsSP2Triangles', 'gsSP1Triangle'):
                nums = [int(v, 0) for v in args]
                for i in range(0, len(nums), 4):
                    try:
                        tri = tuple(state['cache'][v] for v in nums[i:i + 3])
                    except KeyError:
                        continue
                    visit(tri, state)
        return state


# ---------------------------------------------------------------------------------------------------------------
# Triangles, collision, decal layers


def normal_of(p1, p2, p3):
    """The game's collision normal (v2 - v1) x (v3 - v2), unit length (None when degenerate)."""
    nx = (p2[1] - p1[1]) * (p3[2] - p2[2]) - (p2[2] - p1[2]) * (p3[1] - p2[1])
    ny = (p2[2] - p1[2]) * (p3[0] - p2[0]) - (p2[0] - p1[0]) * (p3[2] - p2[2])
    nz = (p2[0] - p1[0]) * (p3[1] - p2[1]) - (p2[1] - p1[1]) * (p3[0] - p2[0])
    mag = (nx * nx + ny * ny + nz * nz) ** .5
    if not mag:
        return None
    return [nx / mag, ny / mag, nz / mag]


def collect_visual(course):
    """Ordered, de-duplicated visible triangles: dicts with tri, texture, wraps, alpha."""
    translucent = {}
    for offset, alpha in TRANSLUCENT.get(course.name, []):
        def mark(tri, state, alpha=alpha):
            translucent[tri] = alpha
        course.run(course.packed(offset), mark)
    seen, out = set(), []

    def visit(tri, state):
        key = (state['texture'], tri)
        if key in seen:
            return
        seen.add(key)
        out.append(dict(tri=tri, texture=state['texture'], wrap_s=state['wrap_s'], wrap_t=state['wrap_t'],
                        alpha=translucent.get(tri)))
    for dl in course.draw_order():
        course.run(dl, visit)
    return out


def collect_collision(course):
    """-> [(tri, surface name, section id)] in the game's order (first list wins for a shared triangle)."""
    seen, out = set(), []
    vertices = course.vertices
    for dl, surface, section, flags in course.sections():
        def visit(tri, state, surface=surface, section=section, flags=flags):
            if tri in seen:
                return
            if all(vertices[i][8] == 4 for i in tri):  # vertex flag 4: no collision
                return
            p = [vertices[i][:3] for i in tri]
            if p[0][0] == p[1][0] and p[0][2] == p[1][2]:
                p = [p[0], p[2], p[1]]
            n = normal_of(*p)
            if n is None:
                return
            if flags & 0x8000 and abs(n[1]) > 0.9:  # walls and slopes only
                return
            if flags & 0x2000 and abs(n[1]) < 0.1:  # floors and ceilings only
                return
            seen.add(tri)
            out.append((tri, surface, section))
        course.run(dl, visit)
    return out


def inside(tri, c, u, v):
    def side(a, b):
        return (b[u] - a[u]) * (c[v] - a[v]) - (b[v] - a[v]) * (c[u] - a[u])
    s = [side(tri[0], tri[1]), side(tri[1], tri[2]), side(tri[2], tri[0])]
    return all(x >= -1e-6 for x in s) or all(x <= 1e-6 for x in s)


def decal_layers(tris):
    """Layer index per triangle: a triangle drawn over earlier ones lying in its plane is stacked above them
    (the N64 resolves coplanar overdraw by draw order, a depth buffer cannot)."""
    cells, layers = {}, []
    cell = 64
    for index, t in enumerate(tris):
        p, n = t['p'], t['n']
        layer = 0
        if n is not None:
            c = [(p[0][k] + p[1][k] + p[2][k]) / 3 for k in range(3)]
            ax = max(range(3), key=lambda k: abs(n[k]))
            u, v = [k for k in range(3) if k != ax]
            for j in cells.get((int(c[0] // cell), int(c[2] // cell)), ()):
                other = tris[j]
                on = other['n']
                if on is None:
                    continue
                dot = on[0] * n[0] + on[1] * n[1] + on[2] * n[2]
                if abs(dot) < 0.98:
                    continue
                if abs(sum((c[k] - other['p'][0][k]) * on[k] for k in range(3))) > 1.2:
                    continue
                if inside(other['p'], c, u, v):
                    layer = max(layer, layers[j] + 1)
        layers.append(layer)
        for cx in range(int(min(q[0] for q in p) // cell), int(max(q[0] for q in p) // cell) + 1):
            for cz in range(int(min(q[2] for q in p) // cell), int(max(q[2] for q in p) // cell) + 1):
                cells.setdefault((cx, cz), []).append(index)
    return layers


# ---------------------------------------------------------------------------------------------------------------
# glTF


def png_bytes(width, height, pixels):
    def chunk(kind, value):
        return struct.pack('>I', len(value)) + kind + value + struct.pack('>I', zlib.crc32(kind + value))
    rows = b''.join(b'\0' + pixels[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)) +
            chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b''))


class Glb:
    def __init__(self):
        self.data = bytearray()
        self.doc = dict(asset=dict(version='2.0', extras=dict(sm64UnitsToMeters=1)), buffers=[], bufferViews=[],
                        accessors=[], images=[], textures=[], samplers=[], materials=[], meshes=[dict(primitives=[])],
                        nodes=[dict(mesh=0)], scenes=[dict(nodes=[0])], scene=0,
                        extensionsUsed=['KHR_materials_unlit'])
        self.images = {}
        self.materials = {}

    def view(self, data):
        self.data.extend(b'\0' * (-len(self.data) % 4))
        self.doc['bufferViews'].append(dict(buffer=0, byteOffset=len(self.data), byteLength=len(data)))
        self.data.extend(data)
        return len(self.doc['bufferViews']) - 1

    def accessor(self, rows, kind, component, fmt, bounds=False, normalized=False):
        flat = b''.join(struct.pack(fmt, *row) if isinstance(row, (tuple, list)) else struct.pack(fmt, row)
                        for row in rows)
        acc = dict(bufferView=self.view(flat), componentType=component, count=len(rows), type=kind)
        if bounds:
            acc.update(min=[min(c) for c in zip(*rows)], max=[max(c) for c in zip(*rows)])
        if normalized:
            acc['normalized'] = True
        self.doc['accessors'].append(acc)
        return len(self.doc['accessors']) - 1

    def material(self, texture, image, wrap_s, wrap_t, blend):
        key = (texture, wrap_s, wrap_t, blend)
        if key in self.materials:
            return self.materials[key]
        material = dict(doubleSided=True, extensions={'KHR_materials_unlit': {}},
                        alphaMode='BLEND' if blend else 'MASK')
        if not blend:
            material['alphaCutoff'] = 0.5
        if image is not None:
            if texture not in self.images:
                w, h, pixels = image
                self.doc['images'].append(dict(bufferView=self.view(png_bytes(w, h, pixels)), mimeType='image/png'))
                self.images[texture] = len(self.doc['images']) - 1
            gl = {'wrap': 10497, 'mirror': 33648, 'clamp': 33071}
            self.doc['samplers'].append(dict(magFilter=9729, minFilter=9729, wrapS=gl[wrap_s], wrapT=gl[wrap_t]))
            self.doc['textures'].append(dict(source=self.images[texture], sampler=len(self.doc['samplers']) - 1))
            material['pbrMetallicRoughness'] = dict(baseColorTexture=dict(index=len(self.doc['textures']) - 1),
                                                    metallicFactor=0, roughnessFactor=1)
        else:
            material['pbrMetallicRoughness'] = dict(baseColorFactor=[1, 1, 1, 1], metallicFactor=0, roughnessFactor=1)
        self.doc['materials'].append(material)
        self.materials[key] = len(self.doc['materials']) - 1
        return self.materials[key]

    def primitive(self, positions, uvs, colors, indices, material):
        attrs = dict(POSITION=self.accessor(positions, 'VEC3', 5126, '<3f', bounds=True),
                     TEXCOORD_0=self.accessor(uvs, 'VEC2', 5126, '<2f'),
                     COLOR_0=self.accessor(colors, 'VEC4', 5121, '<4B', normalized=True))
        idx = self.accessor(indices, 'SCALAR', 5125, '<I')
        self.doc['meshes'][0]['primitives'].append(dict(attributes=attrs, indices=idx, material=material))

    def write(self, path):
        self.doc['buffers'] = [dict(byteLength=len(self.data))]
        meta = json.dumps(self.doc, separators=(',', ':')).encode()
        meta += b' ' * (-len(meta) % 4)
        self.data.extend(b'\0' * (-len(self.data) % 4))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(struct.pack('<4sIII4s', b'glTF', 2, 28 + len(meta) + len(self.data), len(meta), b'JSON') +
                         meta + struct.pack('<I4s', len(self.data), b'BIN\0') + self.data)


# ---------------------------------------------------------------------------------------------------------------
# The walkable course


def floor_height(tris, x, z, y_max):
    """Highest floor of `tris` (triples of world points, upward normals) at (x, z) not above y_max."""
    best = None
    for p in tris:
        if not (min(q[0] for q in p) <= x <= max(q[0] for q in p) and min(q[2] for q in p) <= z <= max(q[2] for q in p)):
            continue
        n = normal_of(*p)
        if n is None or n[1] <= 0.05 or not inside(p, [x, 0, z], 0, 2):
            continue
        y = p[0][1] - (n[0] * (x - p[0][0]) + n[2] * (z - p[0][2])) / n[1]
        if y <= y_max and (best is None or y > best):
            best = y
    return best


def export_walk(course, textures, assets_dir, scale=WALK_SCALE):
    """Write `assets/courses/mk64_<dir>.json` and `assets/levels/mk64_<dir>/terrain.glb`; returns a stats dict."""
    name = course.name
    course_id = f'mk64_{name}'
    verts = course.vertices
    visual = collect_visual(course)
    glb = Glb()
    tri_info = []
    for t in visual:
        p = [[verts[i][0], verts[i][1], verts[i][2]] for i in t['tri']]
        tri_info.append(dict(p=p, n=normal_of(*p)))
    layers = decal_layers(tri_info)

    # --- visual: one primitive per material
    groups = {}
    for t, info, layer in zip(visual, tri_info, layers):
        groups.setdefault((t['texture'], t['wrap_s'], t['wrap_t'], t['alpha'] is not None), []).append((t, info, layer))
    total_vertices = 0
    missing = set()
    for (texture, wrap_s, wrap_t, blend), items in groups.items():
        image = textures.image(texture) if texture else None
        if texture and image is None:
            missing.add(texture)
        w, h = (image[0], image[1]) if image else (32, 32)
        positions, uvs, colors, indices, lookup = [], [], [], [], {}
        for t, info, layer in items:
            n = info['n']
            lift = [0, 0, 0]
            if layer and n is not None:
                sign = -1.0 if (abs(n[1]) > 0.5 and n[1] < 0) else 1.0
                lift = [n[k] * sign * layer * 0.9 for k in range(3)]  # towards the sky for floors
            for index in t['tri']:
                v = verts[index]
                alpha = t['alpha'] if t['alpha'] is not None else 255
                key = (index, layer, alpha)
                if key not in lookup:
                    lookup[key] = len(positions)
                    positions.append((v[0] * scale + lift[0], v[1] * scale + lift[1], v[2] * scale + lift[2]))
                    uvs.append((v[3] / (32.0 * w), v[4] / (32.0 * h)))
                    colors.append((v[5], v[6], v[7], alpha))
                indices.append(lookup[key])
        total_vertices += len(positions)
        glb.primitive(positions, uvs, colors, indices, glb.material(texture, image, wrap_s, wrap_t, blend))
    glb_path = assets_dir / 'levels' / course_id / 'terrain.glb'
    glb.write(glb_path)

    # --- collision
    hits = collect_collision(course)
    vertex_index, out_vertices, triangles, floors = {}, [], [], []

    def vid(p):
        key = (p[0] * scale, p[1] * scale, p[2] * scale)
        if key not in vertex_index:
            vertex_index[key] = len(out_vertices)
            out_vertices.append(list(key))
        return vertex_index[key]

    for tri, surface, _section in hits:
        p = [verts[i][:3] for i in tri]
        if p[0][0] == p[1][0] and p[0][2] == p[1][2]:
            p = [p[0], p[2], p[1]]
        n = normal_of(*p)
        kind = SURFACE_KIND.get(surface, SM64_DEFAULT)
        a, b, c = vid(p[0]), vid(p[1]), vid(p[2])
        if abs(n[1]) >= 0.1:
            if n[1] > 0:
                triangles.append([a, b, c, kind, 0])
                floors.append([[q[0] * scale, q[1] * scale, q[2] * scale] for q in p])
            else:
                triangles.append([a, c, b, kind, 0])  # walkable from above
                triangles.append([a, b, c, kind, 0])  # and a ceiling from below
                floors.append([[q[0] * scale, q[1] * scale, q[2] * scale] for q in (p[0], p[2], p[1])])
        else:  # a wall is solid from both sides
            triangles.append([a, b, c, kind, 0])
            triangles.append([a, c, b, kind, 0])

    # --- spawn on the start line, facing down the track
    path = [p for p in course.track_path() if p[0] > -30000]
    xs = [v[0] for v in out_vertices] or [0]
    zs = [v[2] for v in out_vertices] or [0]
    if path:
        start, ahead = path[0], path[min(3, len(path) - 1)]
        sx, sz = start[0] * scale, start[2] * scale
        dx, dz = ahead[0] - start[0], ahead[2] - start[2]
        yaw = round(math.atan2(dx, dz) * 32768 / math.pi) if (dx or dz) else 0
        sy = floor_height(floors, sx, sz, start[1] * scale + 400)
    else:
        yaw = 0
        boxes = course.item_boxes()
        if boxes:  # battle arena: the item box nearest the middle of the arena
            cx = sum(b[0] for b in boxes) / len(boxes) * scale
            cz = sum(b[2] for b in boxes) / len(boxes) * scale
            sx, sz = min(((b[0] * scale, b[2] * scale) for b in boxes), key=lambda q: (q[0] - cx) ** 2 + (q[1] - cz) ** 2)
        else:
            sx, sz = sum(xs) / len(xs), sum(zs) / len(zs)
        sy = floor_height(floors, sx, sz, 1e9)
    if sy is None:
        best = min(floors, key=lambda p: (sum(q[0] for q in p) / 3 - sx) ** 2 + (sum(q[2] for q in p) / 3 - sz) ** 2)
        sx, sz = sum(q[0] for q in best) / 3, sum(q[2] for q in best) / 3
        sy = floor_height(floors, sx, sz, 1e9)
    boundary = int(max(max(abs(v) for v in xs), max(abs(v) for v in zs))) + 3000
    top, mid, bottom, fog, near, far = LOOK[name]
    data = dict(
        model=f'levels/{course_id}/terrain.glb',
        vertices=[[round(c, 2) for c in v] for v in out_vertices],
        triangles=triangles,
        spawn=dict(pos=[round(sx, 1), round(sy + 4, 1), round(sz, 1)], faceYaw=yaw),
        waterBoxes=[], horizontalBoundary=boundary, explorationOnly=True, objects=[], coins=[], extraCollision={},
        mk64=dict(course=name, name=COURSES[name][1], kind=COURSES[name][2], unit=scale,
                  sky=[top, mid, bottom], fog=[fog, near, far],
                  itemBoxes=[[b[0] * scale, b[1] * scale, b[2] * scale] for b in course.item_boxes()]),
    )
    out = assets_dir / 'courses' / f'{course_id}.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, separators=(',', ':')) + '\n')
    return dict(id=course_id, visual_triangles=len(visual), visual_vertices=total_vertices, primitives=len(groups),
                collision_triangles=len(triangles), collision_vertices=len(out_vertices), textures=len(glb.images),
                missing_textures=sorted(missing), glb_bytes=glb_path.stat().st_size, json_bytes=out.stat().st_size,
                spawn=data['spawn'], boundary=boundary)
