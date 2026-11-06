"""Convert OoT US 1.2 outdoor rooms and scene collision to Jumpman courses."""
import binascii
import csv
import hashlib
import json
import runpy
import struct
import zlib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Share the ROM DMA/Yaz0 reader and RGBA5551 decoder with the character exporter.
n64 = runpy.run_path(str(ROOT / 'scripts/export-oot-link.py'))
COURSES = {'kokiri_forest': ('spot04', 0), 'lon_lon_ranch': ('spot20', 3)}
MASK = 0xffffff
# Jumpman's OoT rig draws model vertices at 0.044; OoT actors use 0.01
# (Actor_Init in z_actor.c). Preserve that model-to-world relationship.
WORLD_SCALE = 4.4


def commands(data):
    header = {}
    for pos in range(0, len(data), 8):
        op, count, pointer = struct.unpack_from('>BB2xI', data, pos)
        if op == 0x14:
            return header
        header[op] = count, pointer & MASK
    raise ValueError('Unterminated scene header')


def png(width, height, pixels):
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', binascii.crc32(tag + data))
    rows = b''.join(b'\0' + pixels[y*width*4:(y+1)*width*4] for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def decode_image(segments, address, palette, fmt, size, bank, width, height):
    src = segments[address >> 24]
    start = address & MASK
    pixels = bytearray()
    for i in range(width * height):
        if size == 0:
            value = (src[start + i//2] >> (0 if i % 2 else 4)) & 15
        elif size == 1:
            value = src[start + i]
        else:
            value, = struct.unpack_from('>H', src, start + i*2)
        if fmt == 0 and size == 2:
            color = n64['rgba16'](value)
        elif fmt == 2:
            pal = segments[palette >> 24]
            index = value + (16 * bank if size == 0 else 0)
            color = n64['rgba16'](struct.unpack_from('>H', pal, (palette & MASK) + 2*index)[0])
        elif fmt == 4:
            intensity = value * (17 if size == 0 else 1)
            color = [intensity]*4
        elif fmt == 3 and size in (0, 1):
            intensity = (value >> 1)*255//7 if size == 0 else (value >> 4)*17
            alpha = (value & 1)*255 if size == 0 else (value & 15)*17
            color = [intensity]*3 + [alpha]
        else:
            raise ValueError(f'Unsupported scene texture format {fmt}/{size}')
        pixels.extend(color)
    return png(width, height, pixels)


class SceneModel:
    """Pack triangle batches and their original textures into one embedded glTF."""
    def __init__(self):
        self.data = bytearray()
        self.doc = dict(asset=dict(version='2.0', extras=dict(sm64UnitsToMeters=1)),
                        buffers=[], bufferViews=[], accessors=[], images=[], textures=[], samplers=[],
                        materials=[], meshes=[dict(primitives=[])], nodes=[dict(mesh=0)],
                        scenes=[dict(nodes=[0])], scene=0, extensionsUsed=['KHR_materials_unlit'])
        self.materials = {}

    def view(self, data):
        self.data.extend(b'\0' * (-len(self.data) % 4))
        views = self.doc['bufferViews']
        views.append(dict(buffer=0, byteOffset=len(self.data), byteLength=len(data)))
        self.data.extend(data)
        return len(views)-1

    def attribute(self, rows, kind, component, fmt):
        accessor = dict(bufferView=self.view(b''.join(struct.pack(fmt, *row) for row in rows)),
                        componentType=component, count=len(rows), type=kind)
        if kind == 'VEC3':
            accessor.update(min=list(map(min, zip(*rows))), max=list(map(max, zip(*rows))))
        if component == 5121:
            accessor['normalized'] = True
        self.doc['accessors'].append(accessor)
        return len(self.doc['accessors'])-1

    def material(self, key, segments, room):
        address, palette, fmt, size, bank, w, h, wrap_s, wrap_t, mode, tint, decal = key
        # Room-local addresses repeat across rooms.
        cache_key = (room, key)
        if cache_key in self.materials:
            return self.materials[cache_key]
        material = dict(doubleSided=True, extensions={'KHR_materials_unlit': {}},
                        alphaMode=mode)
        if decal:
            material['extras'] = dict(ootDecal=True)
        if mode == 'MASK':
            material['alphaCutoff'] = 0.4
        if address:
            image = decode_image(segments, address, palette, fmt, size, bank, w, h)
            self.doc['images'].append(dict(bufferView=self.view(image), mimeType='image/png'))
            wrap = [10497, 33648, 33071, 33648]
            self.doc['samplers'].append(dict(magFilter=9729, minFilter=9987, wrapS=wrap[wrap_s], wrapT=wrap[wrap_t]))
            i = len(self.doc['textures'])
            self.doc['textures'].append(dict(source=i, sampler=i))
            # N64 primitive tint is encoded in display RGB. glTF factors are linear.
            linear = [((channel/255 + 0.055)/1.055)**2.4 if channel > 10 else channel/3294.6 for channel in tint[:3]]
            material['pbrMetallicRoughness'] = dict(baseColorTexture=dict(index=i), baseColorFactor=linear+[tint[3]/255])
        index = len(self.doc['materials'])
        self.doc['materials'].append(material)
        self.materials[cache_key] = index
        return index

    def batch(self, vertices, material):
        attrs = dict(POSITION=self.attribute([v[0] for v in vertices], 'VEC3', 5126, '<3f'),
                     TEXCOORD_0=self.attribute([v[1] for v in vertices], 'VEC2', 5126, '<2f'),
                     COLOR_0=self.attribute([v[2] for v in vertices], 'VEC4', 5121, '<4B'))
        self.doc['meshes'][0]['primitives'].append(dict(attributes=attrs, material=material))

    def write(self, path):
        self.doc['buffers'] = [dict(byteLength=len(self.data))]
        meta = json.dumps(self.doc, separators=(',', ':')).encode()
        meta += b' ' * (-len(meta) % 4)
        self.data.extend(b'\0' * (-len(self.data) % 4))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(struct.pack('<4sIII4s', b'glTF', 2, 28+len(meta)+len(self.data), len(meta), b'JSON') +
                         meta + struct.pack('<I4s', len(self.data), b'BIN\0') + self.data)


def room_mesh(model, room_name, segments, transform):
    room = segments[3]
    _, shape = commands(room)[0x0a]
    kind, count, entries = struct.unpack_from('>BB2xI', room, shape)
    if kind not in (0, 2):
        raise ValueError(f'Unsupported room shape {kind}')
    stride = 16 if kind == 2 else 8
    batches = defaultdict(list)
    for entry in range(count):
        lists = struct.unpack_from('>II', room, (entries & MASK) + (entry+1)*stride - 8)
        for layer, pointer in enumerate(lists):
            if not pointer:
                continue
            cache = {}
            image = texture = palette = 0
            fmt = size = bank = wrap_s = wrap_t = 0
            w = h = 1
            u0 = v0 = 0
            lighting = False
            decal = False
            primitive = (255,255,255,255)
            tint_rgb = tint_alpha = vertex_alpha = texture_alpha = False
            loads = {}
            tmem = load_tmem = 0
            p = pointer & MASK
            while True:
                a, b = struct.unpack_from('>II', room, p)
                p += 8
                op = a >> 24
                if op == 0xdf:
                    break
                if op == 0xde:
                    # These outdoor scenes call runtime texture-scroll / color lists.
                    if b >> 24 in segments:
                        raise ValueError(f'Unhandled nested geometry list {b:x}')
                elif op == 0xd9:
                    lighting = (lighting and bool(a & 0x20000)) or bool(b & 0x20000)
                elif a == 0xe200001c:  # gsDPSetRenderMode, ZMODE_DEC
                    decal = (b & 0xc00) == 0xc00
                elif op == 0xfd:
                    image = b
                elif op in (0xf3, 0xf4):
                    loads[load_tmem] = image
                elif op == 0xf0:
                    palette = image
                elif op == 0xf5 and b >> 24 == 7:
                    load_tmem = a & 511
                elif op == 0xf5 and b >> 24 == 0:
                    tmem = a & 511
                    fmt, size, bank = (a >> 21) & 7, (a >> 19) & 3, (b >> 20) & 15
                    wrap_s, wrap_t = (b >> 8) & 3, (b >> 18) & 3
                elif op == 0xf2 and b >> 24 == 0:
                    u0, v0 = (a >> 12 & 4095)/4, (a & 4095)/4
                    w, h = int((b >> 12 & 4095)/4-u0+1), int((b & 4095)/4-v0+1)
                elif op == 0xfa:
                    primitive = tuple(b.to_bytes(4, 'big'))
                elif op == 0xfc:
                    # The outdoor combiners modulate baked vertex shade by the
                    # base texture, then optionally multiply by primitive color.
                    tint_rgb = (a & 31) == 3
                    alpha_inputs = (a >> 12 & 7, b >> 12 & 7, a >> 9 & 7, b >> 9 & 7,
                                    b >> 21 & 7, b >> 3 & 7, b >> 18 & 7, b & 7)
                    tint_alpha = 3 in alpha_inputs
                    vertex_alpha = 4 in alpha_inputs
                    texture_alpha = 1 in alpha_inputs or 2 in alpha_inputs
                elif op == 1:
                    n = (a >> 12) & 255
                    first = (a & 255)//2 - n
                    for i in range(n):
                        x,y,z,s,t,r,g,blue,alpha = struct.unpack_from('>3h2x2h4B', segments[b>>24], (b & MASK)+i*16)
                        color = (255,255,255,alpha) if lighting else (r,g,blue,alpha)
                        cache[first+i] = transform(x,y,z), s/32, t/32, color
                elif op in (5, 6):
                    texture = loads.get(tmem, 0)
                    tint = (primitive[:3] if tint_rgb else (255,255,255)) + (primitive[3] if tint_alpha else 255,)
                    mode = 'BLEND' if layer else 'MASK' if texture_alpha else 'OPAQUE'
                    key = (texture, palette if fmt == 2 else 0, fmt,size,bank,w,h,wrap_s,wrap_t, mode, tint, decal)
                    for word in ([a,b] if op == 6 else [a]):
                        for shift in (16,8,0):
                            pos,s,t,color = cache[(word >> shift & 255)//2]
                            rgba = color if vertex_alpha else color[:3]+(255,)
                            batches[key].append((pos,((s-u0+0.5)/w,(t-v0+0.5)/h),rgba))
    for key, verts in batches.items():
        model.batch(verts,model.material(key,segments,room_name))


def export(rom, names, course, scene_name, spawn_index):
    def segment(name):
        return n64['segment'](rom,names,name)
    scene = segment(scene_name+'_scene')
    header = commands(scene)
    _, col = header[3]
    xmin, _, zmin, xmax, _, zmax = struct.unpack_from('>6h', scene, col)
    # Center the collision and display geometry together at a common scale.
    boundary = round(WORLD_SCALE * max(xmax-xmin, zmax-zmin) / 2) + 512
    def transform(x,y,z):
        return [round((x-(xmin+xmax)/2)*WORLD_SCALE, 2), round(y*WORLD_SCALE, 2), round((z-(zmin+zmax)/2)*WORLD_SCALE, 2)]
    def table(offset, fmt):
        count, ptr = struct.unpack_from('>H2xI', scene, col+offset)
        return [struct.unpack_from(fmt, scene, (ptr & MASK)+i*struct.calcsize(fmt)) for i in range(count)]
    vertices = [transform(*v) for v in table(12,'>3h')]
    triangles = [[a&8191,b&8191,c&8191,0,0] for _,a,b,c in table(20,'>4H8x') if not a & 0x4000]
    water = []
    for x,y,z,dx,dz in table(36,'>5h6x'):
        x1,level,z1 = transform(x,y,z)
        x2,_,z2 = transform(x+dx,y,z+dz)
        water.append([0,x1,z1,x2,z2,level])
    _, starts = header[0]
    x,y,z,yaw = struct.unpack_from('>3h2xh',scene,starts+spawn_index*16+2)
    model = SceneModel()
    for name in names:
        if name.startswith(scene_name+'_room_'):
            segments = {2: scene, 3: segment(name)}
            if scene_name == 'spot20':
                # Scene_DrawConfigLonLonRanch binds the daytime window to segment 8.
                specs = n64['definitions'](ROOT/'reference/oot/assets/xml/scenes/overworld/spot20.xml')
                segments[8] = scene[n64['offset'](specs['gLonLonRanchDayWindowTex']):]
            room_mesh(model, name, segments, transform)
    path = f'levels/{course}/terrain.glb'
    model.write(ROOT/'assets'/path)
    data = dict(model=path, vertices=vertices, triangles=triangles, spawn=dict(pos=transform(x,y,z),faceYaw=yaw),
                waterBoxes=water, horizontalBoundary=boundary, explorationOnly=True, objects=[], coins=[], extraCollision={})
    (ROOT/'assets/courses'/f'{course}.json').write_text(json.dumps(data,separators=(',',':'))+'\n')
    print(f'{course}: {len(triangles)} collision triangles, {len(model.doc["materials"])} materials')


if __name__ == '__main__':
    rom = (ROOT/'reference/roms/oot-us-1.2.z64').read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError('Expected a big-endian US 1.2 OoT ROM')
    with (ROOT/'reference/oot/baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    for course, (scene, spawn) in COURSES.items():
        export(rom, names, course, scene, spawn)
