"""Convert OoT US 1.2 outdoor rooms and scene collision to Jumpman courses."""
import binascii
import csv
import hashlib
import json
import math
import re
import runpy
import struct
import zlib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Share the ROM DMA/Yaz0 reader and RGBA5551 decoder with the character exporter.
n64 = runpy.run_path(str(ROOT / 'scripts/export-oot-link.py'))
COURSES = {'kokiri_forest': ('spot04', 0), 'lon_lon_ranch': ('spot20', 3),
           'links_house': ('link_home', 0), 'kokiri_shop': ('kokiri_shop', 0),
           'know_it_all_house': ('kokiri_home',0), 'twins_house': ('kokiri_home3',0),
           'midos_house': ('kokiri_home4',0), 'sarias_house': ('kokiri_home5',0)}
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
        if kind in ('SCALAR', 'VEC3'):
            accessor.update(min=list(map(min, zip(*rows))), max=list(map(max, zip(*rows))))
        if component == 5121:
            accessor['normalized'] = True
        self.doc['accessors'].append(accessor)
        return len(self.doc['accessors'])-1

    def material(self, key, segments, room):
        address, palette, fmt, size, bank, w, h, wrap_s, wrap_t, mode, tint, decal, lit = key
        # Room-local addresses repeat across rooms.
        cache_key = (room, key)
        if cache_key in self.materials:
            return self.materials[cache_key]
        material = dict(doubleSided=True, extensions={'KHR_materials_unlit': {}},
                        alphaMode=mode)
        if lit:
            material.pop('extensions')
            material['pbrMetallicRoughness'] = dict(metallicFactor=0, roughnessFactor=1)
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
            material.setdefault('pbrMetallicRoughness',{}).update(baseColorTexture=dict(index=i), baseColorFactor=linear+[tint[3]/255])
        index = len(self.doc['materials'])
        self.doc['materials'].append(material)
        self.materials[cache_key] = index
        return index

    def batch(self, vertices, material):
        attrs = dict(POSITION=self.attribute([v[0] for v in vertices], 'VEC3', 5126, '<3f'),
                     TEXCOORD_0=self.attribute([v[1] for v in vertices], 'VEC2', 5126, '<2f'),
                     COLOR_0=self.attribute([v[2] for v in vertices], 'VEC4', 5121, '<4B'))
        if len(vertices[0]) == 5:
            attrs['NORMAL'] = self.attribute([v[4] for v in vertices], 'VEC3',5126,'<3f')
        if len(vertices[0]) >= 4:
            attrs['JOINTS_0'] = self.attribute([(v[3],0,0,0) for v in vertices], 'VEC4', 5123, '<4H')
            attrs['WEIGHTS_0'] = self.attribute([(1,0,0,0) for v in vertices], 'VEC4', 5126, '<4f')
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
    if kind not in (0, 1, 2):
        raise ValueError(f'Unsupported room shape {kind}')
    stride = 16 if kind == 2 else 8
    batches = defaultdict(list)
    for entry in range(count):
        lists = struct.unpack_from('>II', room, (entries & MASK) + (entry+1)*stride - 8)
        for layer, pointer in enumerate(lists):
            if pointer:
                display_mesh(model, room_name, segments, pointer, transform, layer, batches=batches)
    for key, verts in batches.items():
        model.batch(verts,model.material(key,segments,room_name))


def display_mesh(model, name, segments, pointer, transform, layer=0, joint=None, matrices=(), batches=None):
    standalone = batches is None
    if standalone:
        batches = defaultdict(list)
    stack = []
    list_segment = pointer >> 24
    room = segments[list_segment]
    cache = {}
    image = texture = palette = 0
    fmt = size = bank = wrap_s = wrap_t = 0
    w = h = 1
    u0 = v0 = 0
    lighting = joint is not None and name != 'gameplay_keep'
    decal = False
    primitive = (255,255,255,255)
    blend_item_color = False
    env_tunic = None
    tint_rgb = tint_alpha = vertex_alpha = texture_alpha = False
    loads = {}
    tmem = load_tmem = 0
    p = pointer & MASK
    while True:
        a, b = struct.unpack_from('>II', room, p)
        p += 8
        op = a >> 24
        if op == 0xdf:
            if stack:
                p = stack.pop()
                continue
            break
        if op == 0xde:
            # These outdoor scenes call runtime texture-scroll / color lists.
            if name == 'object_kw1' and b >> 24 in (8,9):
                env_tunic = (70,190,60) if b >> 24 == 8 else (100,30,0)
            if name == 'object_km1' and b >> 24 in (8,9):
                env_tunic = (0,130,70) if b >> 24 == 8 else (110,170,20)
            if b >> 24 == list_segment:
                if not (a >> 16 & 255):
                    stack.append(p)
                p = b & MASK
        elif op == 0xda and joint is not None and b >> 24 == 13:
            joint = matrices[(b & MASK)//64]
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
            blend_item_color = name.startswith('object_gi_') and ((a >> 20 & 15) == 3 or (b >> 28) == 3)
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
                normal = tuple(n if n<128 else n-256 for n in (r,g,blue))
                length = math.hypot(*normal)
                normal = tuple(n/length for n in normal) if length else (0,1,0)
                cache[first+i] = transform(x,y,z), s/32, t/32, color, joint, normal
        elif op in (5, 6):
            texture = loads.get(tmem, 0)
            rgb = env_tunic or (primitive[:3] if tint_rgb or blend_item_color else (255,255,255))
            tint = rgb + (primitive[3] if tint_alpha else 255,)
            mode = 'BLEND' if layer else 'MASK' if texture_alpha else 'OPAQUE'
            lit = joint is not None and lighting and name != 'gameplay_keep'
            key = (texture, palette if fmt == 2 else 0, fmt,size,bank,w,h,wrap_s,wrap_t, mode, tint, decal, lit)
            for word in ([a,b] if op == 6 else [a]):
                for shift in (16,8,0):
                    pos,s,t,color,bone,normal = cache[(word >> shift & 255)//2]
                    rgba = color if vertex_alpha else color[:3]+(255,)
                    vertex = (pos,((s-u0+0.5)/w,(t-v0+0.5)/h),rgba)
                    if bone is not None:
                        vertex += (bone,)
                    if lit:
                        vertex += (normal,)
                    batches[key].append(vertex)
    if standalone:
        for key, verts in batches.items():
            model.batch(verts,model.material(key,segments,name))


def actor_model(asset, segment, body_name, skeleton, animation, animation_bank=None, head=None, pose=None):
    """Standard OoT FlexSkeleton and joint-index animation to a skinned glTF."""
    body = segment(body_name)
    def specs(bank):
        return n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
    def addr(bank, name):
        return name if isinstance(name,int) else n64['offset'](specs(bank)[name])
    skel = addr(body_name, skeleton)
    table, count = struct.unpack_from('>IB', body, skel)
    limbs = []
    for j in range(count):
        limb, = struct.unpack_from('>I', body, (table & MASK) + 4*j)
        limbs.append(struct.unpack_from('>hhhBBI', body, limb & MASK))
    def units(x,y,z):
        scale = 0.008 if asset == 'fairy' else 0.01
        return [v * scale * WORLD_SCALE for v in (x,y,z)]
    model = SceneModel()
    nodes = model.doc['nodes']
    matrices = [j for j, limb in enumerate(limbs) if limb[-1] or (head and j == count-1)]
    for j,(x,y,z,child,sibling,dl) in enumerate(limbs):
        children = []
        while child != 255:
            children.append(child+1)
            child = limbs[child][4]
        nodes.append(dict(name=f'joint_{j}', translation=units(x,y,z)))
        if children:
            nodes[-1]['children'] = children
        bank = body_name
        segments = {4 if body_name == 'gameplay_keep' else 6:body}
        if body_name == 'object_md':
            segments[8] = body[addr(bank,'gMidoEyeOpenTex'):]
        if body_name == 'object_sa':
            for s,eye in [(8,'gSariaEyeOpenTex'),(9,'gSariaEyeOpenTex'),(10,'gSariaMouthClosedTex')]:
                segments[s] = body[addr(bank, eye):]
        if head and j == count-1:
            bank = head
            data = segment(head)
            symbol, eye = {
                'object_masterkokirihead': ('gKokiriShopkeeperHeadDL','gKokiriShopkeeperEyeDefaultTex'),
                'object_km1': ('gKm1DL',None),
                'object_kw1': ('object_kw1_DL_002C10','gKw1EyeOpenTex'),
                'object_fa': ('gFaDL','gFaEyeOpenTex'),
            }[head]
            segments = {6:data}
            if eye:
                segments[10] = data[addr(bank,eye):]
            dl = 0x06000000 + addr(bank,symbol)
        if body_name == 'object_box':
            dl = {0:0x060006F0,2:0x060010C0}.get(j,0)
        if dl:
            display_mesh(model,bank,segments,dl,units,layer=int(asset=='fairy'),joint=j,matrices=matrices)
    joints = list(range(1,count+1))
    identity = (1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1)
    inverses = model.attribute([identity]*count,'MAT4',5126,'<16f')
    model.doc['skins'] = [dict(joints=joints, skeleton=1, inverseBindMatrices=inverses)]
    nodes[0]['skin'] = 0
    model.doc['scenes'][0]['nodes'].append(1)
    bank = animation_bank or body_name
    data = segment(bank)
    model.doc['animations'] = []
    def add_clip(label, animation, pose=None):
        frames, values, indices, static = struct.unpack_from('>H2xIIH',data,addr(bank,animation))
        tracks = dict(name=label, samplers=[], channels=[])
        times = model.attribute([(i/30,) for i in range(frames+1)], 'SCALAR',5126,'<f')
        for j in range(count+1):
            offsets = struct.unpack_from('>3H',data,(indices & MASK)+6*j)
            samples = []
            for frame in range(frames+1):
                sample = min(frame,frames-1) if label == 'Open' else frame%frames
                xyz = [struct.unpack_from('>h',data,(values & MASK)+2*(o+((pose if pose is not None else sample) if o>=static else 0)))[0] for o in offsets]
                if j == 0:
                    samples.append(units(*xyz))
                else:
                    ax,ay,az = [v*math.pi/65536 for v in xyz]
                    a,b,c = map(math.cos,(ax,ay,az))
                    x,y,z = map(math.sin,(ax,ay,az))
                    samples.append((x*b*c-a*y*z, a*y*c+x*b*z, a*b*z-x*y*c, a*b*c+x*y*z))
            path = 'rotation' if j else 'translation'
            if label == 'Idle':
                nodes[max(j,1)][path] = samples[0]
            output = model.attribute(samples,'VEC4' if j else 'VEC3',5126,'<4f' if j else '<3f')
            tracks['samplers'].append(dict(input=times,output=output))
            tracks['channels'].append(dict(sampler=j,target=dict(node=max(j,1),path=path)))
        model.doc['animations'].append(tracks)
    add_clip('Idle',animation,pose)
    for label, symbol, frame in {
        'sword_chest':[('Open','gTreasureChestAnim_000128',None)],
        'mido':[('Walk','gMidoWalkAnim',None),('Talk','gMidoHaltAnim',None),('Curious','gMidoCuriousAnim',None),('Annoyed','gMidoAnnoyedAnim',None)],
        'saria':[('Talk','gSariaRightArmExtendedWaitAnim',None),('Surprise','gSariaHandsOnFaceAnim',None)],
        'kokiri_0':[('Talk','gKokiriWipingForeheadAnim',None)],
        'kokiri_2':[('Talk','gKokiriBlockingAnim',1)],
        'kokiri_4':[('Talk','gKokiriBlockingAnim',1)],
    }.get(asset,[]):
        add_clip(label,symbol,frame)
    model.write(ROOT/'assets/levels/kokiri_forest'/f'{asset}.glb')


# EnKo_GetTextIdChild, sModelInfo, sOsAnimeLookup (child start column).
KOKIRI_CHILDREN = [
    ('LiftingRock',0x1004,0x1004,None), ('StandUp',0x1005,0x1006,0),
    ('Punching',0x1007,0x1007,None), ('Blocking',0x1008,0x1009,None),
    ('CuttingGrass',0x100A,0x100B,None), ('StandUp',0x100C,0x100D,1),
    ('StandUp',0x100E,0x1019,2), ('Standing',0x1035,0x1035,None),
    ('SittingArmsUp',0x1038,0x1038,None), ('StandUp',0x103C,0x103C,1),
    ('StandUp',0x103D,0x103D,0), ('Sitting',0x103E,0x103E,None),
    ('Idle',0x10D7,0x10D8,None),
]
DAY_RESIDENTS = {'kokiri_forest': {0,1,2,3,4,5,6,12}, 'twins_house': {9},
                 'know_it_all_house': {7,8,11}, 'kokiri_shop': {10}}
CHOICES = {0x1035:[0x1036,0x1037], 0x1038:[0x1039,0x103A,0x103B], 0x103E:[0x103F,0x1040]}


def kokiri_text(segment):
    code = segment('code')
    text = segment('nes_message_data_static')
    pos = 0x8010F13C - 0x800116E0
    starts = {}
    while True:
        ident, style, pointer = struct.unpack_from('>HBxI',code,pos)
        pos += 8
        if ident == 0xffff:
            break
        starts[ident] = (style,pointer & MASK)

    def pages(ident):
        style,p = starts[ident]
        result = []
        line = ''
        while True:
            c = text[p]
            p += 1
            if c in (2,4,7):
                if line:
                    page = dict(text=line, style=style)
                    if '\x1b' in line or '\x1c' in line:
                        page['branches'] = [pages(answer) for answer in CHOICES[ident]]
                    result.append(page)
                if c == 7:
                    return result + pages(struct.unpack_from('>H',text,p)[0])
                if c == 2:
                    return result
                line = ''
            elif c in (5,6):
                line += chr(c)+chr(text[p])
                p += 1
            elif c == 0x12:
                p += 2
            elif c in (0x13,0x14):
                p += 1
            elif c == 0x0f:
                line += '{name}'
            elif c == 1:
                line += '\n'
            elif 32 <= c < 0xac or c in (8,9,0x1b,0x1c):
                line += chr(c)
            elif c not in (0x0b,0x1a):
                raise ValueError(f'Unsupported message control {c:x} in {ident:x}')
    return pages


def export_message_art(segment):
    folder = ROOT/'assets/oot-dialogue'
    folder.mkdir(exist_ok=True)
    font = segment('nes_font_static')
    atlas = bytearray(256*144*4)
    for g in range(min(144,len(font)//128)):
        for y in range(16):
            for x in range(16):
                value = font[g*128+y*8+x//2]
                alpha = ((value >> 4) if x%2 == 0 else value & 15)*17
                off = ((g//16*16+y)*256+g%16*16+x)*4
                atlas[off:off+4] = bytes((255,255,255,alpha))
    (folder/'glyphs.png').write_bytes(png(256,144,atlas))
    src = (ROOT/'reference/oot/src/code/z_message.c').read_text()
    src = re.sub(r'#if OOT_NTSC\n([^#]+)#else\n[^#]+#endif',r'\1',src)
    widths = re.findall(r'(\d+)\.0f',src.split('sFontWidths[144] = {')[1].split('};')[0])
    assert len(widths)==144
    (folder/'widths.json').write_text(json.dumps(list(map(int,widths))))
    data = segment('message_static')
    for name, start,w,h,fmt in [('black',0,128,64,4),('wood',4096,128,64,3),('next',0x4000,16,16,4),
                                ('end',0x4080,16,16,4),('arrow',0x4100,16,16,4)]:
        (folder/(name+'.png')).write_bytes(decode_image({7:data},0x07000000+start,0,fmt,0,0,w,h))


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
    def table(offset, fmt, data=scene, header_offset=col):
        count, ptr = struct.unpack_from('>H2xI', data, header_offset+offset)
        return [struct.unpack_from(fmt, data, (ptr & MASK)+i*struct.calcsize(fmt)) for i in range(count)]
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
    objects, talks, exit_rooms, arrivals = [], [], {}, {}
    rupees = []
    quest = {}
    extra_collision = {}
    messages = kokiri_text(segment) if course != 'lon_lon_ranch' else None
    fixed = None
    if course == 'kokiri_forest':
        # US 1.2 gFairySkel and gFairyAnim, shared with the Kokiri shopkeeper.
        chest_bank = segment('object_box')
        specs = n64['definitions'](ROOT/'reference/oot/assets/xml/objects/object_box.xml')
        chest_col = n64['offset'](specs['gTreasureChestCol'])
        extra_collision['sword_chest'] = dict(
            vertices=[[round(coord*0.01*WORLD_SCALE,3) for coord in vertex]
                      for vertex in table(12,'>3h',chest_bank,chest_col)],
            triangles=[[a&8191,b&8191,c&8191,0,0] for _,a,b,c in table(20,'>4H8x',chest_bank,chest_col)])
        actor_model('sword_chest',segment,'object_box','gTreasureChestSkel','gTreasureChestAnim_000128',pose=0)
        for asset,bank,symbol,scale in [('kokiri_sword','object_gi_sword_1','gGiKokiriSwordDL',1.1),
                                        ('deku_tree_mouth','object_spot04_objects','gDekuTreeMouthDL',WORLD_SCALE)]:
            prop = SceneModel()
            dl = n64['offset'](n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')[symbol])
            display_mesh(prop,bank,{6:segment(bank),4:segment('gameplay_keep')},0x06000000+dl,
                         lambda x,y,z: [v*scale for v in (x,y,z)])
            prop.write(ROOT/'assets/levels/kokiri_forest'/f'{asset}.glb')
        actor_model('fairy',segment,'gameplay_keep',0x16EE8,0x15044)
        actor_model('saria',segment,'object_sa','gSariaSkel','gSariaWaitArmsToSideAnim')
        prop = SceneModel()
        display_mesh(prop,'sign',{4:segment('gameplay_keep')},0x0403D560,
                     lambda x,y,z: [x*.044,y*.044,(z-100)*.044])
        prop.write(ROOT/'assets/levels/kokiri_forest/sign.glb')
        # US 1.2 gameplay_keep: six 4x4 RGBA5551 textures precede gRupeeVtx.
        keep = segment('gameplay_keep')
        for idx, color in enumerate(('green','blue')):
            prop = SceneModel()
            texture = 0x44F10 - (6-idx)*32
            display_mesh(prop,color,{4:keep,8:keep[texture:]},0x04045150,
                         lambda x,y,z: [v*.015*WORLD_SCALE for v in (x,y+750,z)])
            prop.write(ROOT/'assets/levels/kokiri_forest'/f'rupee_{color}.glb')
        export_message_art(segment)
    if course == 'kokiri_shop':
        actor_model('keeper',segment,'object_km1','gKm1Skel','object_masterkokiri_Anim_0004A8',
                    'object_masterkokiri','object_masterkokirihead')
        prop = SceneModel()
        display_mesh(prop,'shelves',{6:segment('object_shop_dungen')},0x06000B80,
                     lambda x,y,z: [v*WORLD_SCALE for v in (x,y,z)])
        # sShopkeeperStores[OSSAN_TYPE_KOKIRI], relative to the wooden shelf actor.
        stock = [
            ('shield_1','gGiDekuShieldDL',50,52,-20), ('nuts','gGiNutDL',50,76,-20),
            ('nuts','gGiNutDL',80,52,-3), ('stick','gGiStickDL',80,76,-3),
            ('seed','gGiSeedDL',-50,52,-20), ('arrow','gGiArrowMediumDL',-50,76,-20),
            ('arrow','gGiArrowLargeDL',-80,52,-3), ('heart','gGiRecoveryHeartDL',-80,76,-3),
        ]
        keep = segment('gameplay_keep')
        for item, symbol, dx, dy, dz in stock:
            bank = 'object_gi_'+item
            dl = n64['offset'](n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')[symbol])
            angle = math.copysign(0x1554*math.pi/32768,-dx)
            def on_shelf(x,y,z):
                return [WORLD_SCALE*(dx + .25*(x*math.cos(angle)+z*math.sin(angle))),
                        WORLD_SCALE*(dy+.25*(y+24)),
                        WORLD_SCALE*(dz+.25*(z*math.cos(angle)-x*math.sin(angle)))]
            display_mesh(prop,bank,{6:segment(bank),4:keep},0x06000000+dl,on_shelf)
        prop.write(ROOT/'assets/levels/kokiri_forest/shelves.glb')
    for name in names:
        if name.startswith(scene_name+'_room_'):
            segments = {2: scene, 3: segment(name)}
            if scene_name == 'spot20':
                # Scene_DrawConfigLonLonRanch binds the daytime window to segment 8.
                specs = n64['definitions'](ROOT/'reference/oot/assets/xml/scenes/overworld/spot20.xml')
                segments[8] = scene[n64['offset'](specs['gLonLonRanchDayWindowTex']):]
            room = segments[3]
            rh = commands(room)
            shape = rh[0x0a][1]
            if room[shape] == 1:
                image = struct.unpack_from('>I',room,shape+8)[0] & MASK
                end = room.index(b'\xff\xd9',image)+2
                folder = ROOT/'assets/levels'/course
                folder.mkdir(parents=True, exist_ok=True)
                (folder/'background.jpg').write_bytes(room[image:end])
                cam_table = struct.unpack_from('>I',scene,col+32)[0] & MASK
                cam = struct.unpack_from('>I',scene,cam_table+4)[0] & MASK
                ex,ey,ez,pitch,yaw_cam,_,fov = struct.unpack_from('>7h',scene,cam)
                fixed = dict(eye=transform(ex,ey,ez),pitch=pitch,yaw=yaw_cam,fov=fov/100)
            room_mesh(model, name, segments, transform)
            count, actor_list = rh.get(1,(0,0))
            for i in range(count):
                actor,ax,ay,az,_,angle,_,param = struct.unpack_from('>8h',room,actor_list+16*i)
                pos = transform(ax,ay,az)
                if course == 'kokiri_forest' and actor == 10 and (param>>5)&127 == 0x27:
                    reward = messages(0xA4)
                    # Equipment is carried automatically in this game.
                    reward[0]['text'] = reward[0]['text'].split('\n',1)[0]
                    quest.update(chest=pos, reward=reward)
                    objects.append(dict(model='levels/kokiri_forest/sword_chest.glb',pos=pos,yaw=angle*180/32768,collision='sword_chest'))
                    objects.append(dict(model='levels/kokiri_forest/kokiri_sword.glb',pos=pos,yaw=0))
                    continue
                if course == 'kokiri_forest' and actor == 0x3e:
                    objects.append(dict(model='levels/kokiri_forest/deku_tree_mouth.glb',pos=transform(4029,136,-1255),yaw=0))
                if course == 'kokiri_forest' and actor == 0x15 and (param & 255) in (0,1):
                    blue = (param & 255) == 1
                    flag = (param >> 8) & 63
                    key = f'{name}-{i}'
                    rupees.append(dict(key=key,pos=pos,value=5 if blue else 1,permanent=flag<32))
                    objects.append(dict(model='levels/kokiri_forest/rupee_'+('blue' if blue else 'green')+'.glb',
                                        pos=pos,yaw=0,rupee=key))
                    continue
                if course in ('kokiri_forest','kokiri_shop') and actor == 0x112 and ((param >> 11) & 31) == 2:
                    drop = (param >> 6) & 31
                    if drop in (8,9,10):
                        value = {8:1,9:5,10:20}[drop]*max(1,struct.unpack_from('>h',room,actor_list+16*i+12)[0]&255)
                        rupees.append(dict(key=f'{name}-{i}',pos=pos,value=value,hidden=True,permanent=(param&63)<32))
                    continue
                if actor == 0x163 and (param&255) in DAY_RESIDENTS.get(course,set()):
                    kid = param&255
                    # Know-It-All brother 7 has three preplaced story-state variants.
                    if kid == 7 and struct.unpack_from('>h',room,actor_list+16*i+12)[0] != 0:
                        continue
                    asset = f'kokiri_{kid}'
                    girl = kid in (1,5,6,9,10,12)
                    body = 'object_kw1' if girl else 'object_km1'
                    anim, first, repeat, pose = KOKIRI_CHILDREN[kid]
                    actor_model(asset,segment,body,'gKw1Skel' if girl else 'gKm1Skel',
                                'gKokiri'+anim+'Anim','object_os_anime','object_fa' if kid==12 else body,pose)
                    label = 'Fado' if kid==12 else 'Know-It-All Brother' if course=='know_it_all_house' else 'Kokiri'
                    talks.append(dict(pos=pos,kind='resident',id=asset,label=label,pages=messages(first),repeat=messages(repeat)))
                elif course == 'kokiri_forest' and actor == 0x16d:
                    asset = 'mido'
                    path = header[13][1] + 8*((param>>8)&255)
                    length, points = struct.unpack_from('>B3xI',scene,path)
                    quest.update(path=[transform(*struct.unpack_from('>3h',scene,(points&MASK)+6*j)) for j in range(length)],
                                 approved=messages(0x1033), passed=messages(0x1034))
                    actor_model(asset,segment,'object_md','gMidoSkel','gMidoIdleAnim',pose=0)
                    talks.append(dict(pos=pos,kind='resident',id=asset,label='Mido',pages=messages(0x102f),repeat=messages(0x1030)))
                elif course == 'kokiri_forest' and actor in (0x141,0x146):
                    saria = actor == 0x146
                    asset = 'saria' if saria else 'sign'
                    talks.append(dict(pos=pos, kind=asset, pages=messages(0x1002 if saria else param|0x300)))
                elif course == 'kokiri_shop' and actor in (0x3d,0xc2):
                    asset = 'keeper' if actor == 0x3d else 'shelves'
                    if asset == 'keeper':
                        pos = transform(ax,ay,az+33)
                        talks.append(dict(pos=transform(ax,ay,35),kind='resident',id='keeper',label='Shopkeeper',pages=messages(0x9e)))
                else:
                    continue
                if asset not in ('sign','shelves'):
                    objects.append(dict(model='levels/kokiri_forest/fairy.glb',pos=pos,yaw=0,idle=True,fairy=True,**(dict(owner=asset) if asset=="mido" else {})))
                objects.append(dict(model=f'levels/kokiri_forest/{asset}.glb',pos=pos,
                                    yaw=angle*180/32768, idle=asset not in ('sign','shelves')))
    # Scene exit indices live on the actual floor polygons. Resolve the two
    # exterior doorways and their return spawns from the US entrance table.
    rooms = []
    surface_types = struct.unpack_from('>I',scene,col+28)[0] & MASK
    for typ,a,_,_ in table(20,'>4H8x'):
        if a & 0x4000:
            continue
        idx = (struct.unpack_from('>I',scene,surface_types+typ*8)[0] >> 8) & 31
        rooms.append(idx)
        if idx:
            entrance = struct.unpack_from('>H',scene,header[0x13][1]+2*(idx-1))[0]
            route = {0x272:('links_house','door'),0xc1:('kokiri_shop','door'),
                     0x211:('kokiri_forest','house'),0x266:('kokiri_forest','shop'),
                     0xc9:('know_it_all_house','door'),0x26a:('kokiri_forest','brothers'),
                     0x9c:('twins_house','door'),0x33c:('kokiri_forest','twins'),
                     0x433:('midos_house','door'),0x443:('kokiri_forest','mido'),
                     0x437:('sarias_house','door'),0x447:('kokiri_forest','saria')}.get(entrance)
            if route:
                exit_rooms[idx] = dict(course=route[0],entry=route[1])
    if course == 'kokiri_forest':
        for key,i in [('house',3),('shop',4),('brothers',5),('twins',8),('mido',9),('saria',10)]:
            xx,yy,zz,angle = struct.unpack_from('>3h2xh',scene,starts+16*i+2)
            arrivals[key] = dict(pos=transform(xx,yy,zz),faceYaw=angle)
    if course == 'links_house':
        xx,yy,zz,angle = struct.unpack_from('>3h2xh',scene,starts+16+2)
        arrivals['door'] = dict(pos=transform(xx,yy,zz),faceYaw=angle)
    path = f'levels/{course}/terrain.glb'
    model.write(ROOT/'assets'/path)
    data = dict(model=path, vertices=vertices, triangles=triangles, spawn=dict(pos=transform(x,y,z),faceYaw=yaw),
                waterBoxes=water, horizontalBoundary=boundary, explorationOnly=True, objects=objects, coins=[], extraCollision=extra_collision)
    if exit_rooms or talks:
        data.update(rooms=rooms, kokiri=dict(exits=exit_rooms, talks=talks, arrivals=arrivals, fixed=fixed,rupees=rupees))
    if 'kokiri' in data:
        count, lights = header[15]
        # Outdoors the second setting is daylight; fixed rooms have one setup.
        offset = lights + (22 if count > 1 else 0)
        ambient,d1,c1,d2,c2 = [list(struct.unpack_from(fmt,scene,offset+i*3)) for i,fmt in
                               enumerate(('>3B','>3b','>3B','>3b','>3B'))]
        data['kokiri']['light'] = dict(ambient=ambient,key=c1,keyDir=d1,fill=c2,fillDir=d2)
    if course == 'kokiri_shop':
        # Face the appropriate shelf from the customer side of the counter.
        data['kokiri']['counter'] = dict(shield=transform(50,0,35),heart=transform(-80,0,35))
    if course == 'kokiri_forest':
        # Face of the ladder below the house spans x=-45..-13, z=1004.
        # Land clear of its front collider at ground level and on the porch above.
        quest['crawl'] = [transform(-785,120,1030), transform(-785,120,1408)]
        data['kokiri']['quest'] = quest
        data['kokiri']['ladder'] = [transform(-29,-80,983), transform(-29,100,1037)]
        data['kokiri']['repeat'] = messages(0x1003)
        data['kokiri']['greeting'] = messages(0x1001)
        data['kokiri']['afterMido'] = [messages(0x1031),messages(0x1032)]
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
