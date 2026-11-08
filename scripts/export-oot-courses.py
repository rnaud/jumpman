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
from functools import lru_cache
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Share the ROM DMA/Yaz0 reader and RGBA5551 decoder with the character exporter.
n64 = runpy.run_path(str(ROOT / 'scripts/export-oot-link.py'))
resident_data = runpy.run_path(str(ROOT/'scripts/oot-residents.py'))
scenery = runpy.run_path(str(ROOT/'scripts/oot-scenery.py'))
RESIDENTS = resident_data['RESIDENTS']
COURSES = {'kokiri_forest': ('spot04', 0), 'deku_tree': ('ydan', 0), 'deku_tree.2': ('ydan_boss',0), 'lon_lon_ranch': ('spot20', 3),
           'links_house': ('link_home', 0), 'kokiri_shop': ('kokiri_shop', 0),
           'know_it_all_house': ('kokiri_home',0), 'twins_house': ('kokiri_home3',0),
           'midos_house': ('kokiri_home4',0), 'sarias_house': ('kokiri_home5',0)}
# Daytime exploration scenes share the same collision, spawn and terrain importer.
EXPLORATION_SCENES = {
    'hyrule_field': ('spot00', 0),
    'kakariko_village': ('spot01', 0),
    'graveyard': ('spot02', 0),
    'lost_woods': ('spot10', 0),
    'sacred_forest_meadow': ('spot05', 0),
    'zoras_river': ('spot03', 0),
    'zoras_domain': ('spot07', 0),
    'zoras_fountain': ('spot08', 5),
    'lake_hylia': ('spot06', 0),
    'gerudo_valley': ('spot09', 0),
    'gerudos_fortress': ('spot12', 0),
    'haunted_wasteland': ('spot13', 0),
    'desert_colossus': ('spot11', 0),
    'hyrule_castle': ('spot15', 0),
    'death_mountain_trail': ('spot16', 0),
    'death_mountain_crater': ('spot17', 0),
    'goron_city': ('spot18', 0),
}
COURSES.update(EXPLORATION_SCENES)
OVERWORLD = set(EXPLORATION_SCENES) | {'kokiri_forest', 'lon_lon_ranch'}
DAY_TEXTURES = {
    'spot01':'gKakarikoVillageDayWindowTex', 'spot07':'gZorasDomainDayEntranceTex',
    'spot12':'gGerudoFortressDayWallTex', 'spot18':'gGoronCityDayEntranceTex',
    'spot20':'gLonLonRanchDayWindowTex',
}


def overworld_entrances():
    table = (ROOT/'reference/oot/include/tables/entrance_table.h').read_text()
    entries = re.findall(r'/\* (0x[0-9A-Fa-f]+) \*/ DEFINE_ENTRANCE\([^,]+, SCENE_(\w+), (\d+),', table)
    return {int(address,16):(scene.lower(),f'spawn_{spawn}') for address,scene,spawn in entries
            if scene.lower() in OVERWORLD}


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
        elif fmt == 3 and size == 2:
            color = [value >> 8]*3 + [value & 255]
        elif fmt == 3 and size in (0, 1):
            intensity = (value >> 1)*255//7 if size == 0 else (value >> 4)*17
            alpha = (value & 1)*255 if size == 0 else (value & 15)*17
            color = [intensity]*3 + [alpha]
        else:
            raise ValueError(f'Unsupported scene texture format {fmt}/{size}')
        pixels.extend(color)
    return png(width, height, pixels)


# A fixed 10 AM sample of Environment_UpdateLights for daytime exploration.
FIELD_SUN = (60, 104, 17)
FIELD_MOON = tuple(-v for v in FIELD_SUN)


def terrain_detail_image(segments, base, width, height, detail):
    """Flatten two native wrapping RGBA16 tiles over their common repeat period."""
    address, dw, dh, base_step, detail_step, out_w, out_h, alpha = detail
    def tile(addr,w,h):
        return [n64['rgba16'](v) for v, in struct.iter_unpack(
            '>H',segments[addr >> 24][addr & MASK:][:w*h*2])]
    def sample(pixels,w,h,x,y):
        ix,iy = int(x),int(y)
        fx,fy = x-ix,y-iy
        a,b,c,d = (pixels[(j%h)*w+i%w] for i,j in
                   ((ix,iy),(ix+1,iy),(ix,iy+1),(ix+1,iy+1)))
        return tuple((a[k]*(1-fx)+b[k]*fx)*(1-fy)+(c[k]*(1-fx)+d[k]*fx)*fy for k in range(3))
    first,second = tile(base,width,height),tile(address,dw,dh)
    pixels = bytearray()
    for y in range(out_h):
        for x in range(out_w):
            a = sample(first,width,height,x*base_step[0],y*base_step[1])
            b = sample(second,dw,dh,x*detail_step[0],y*detail_step[1])
            pixels.extend(round((v*(255-alpha)+u*alpha)/255) for v,u in zip(a,b))
            pixels.append(255)
    return png(out_w,out_h,pixels)


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
        address, palette, fmt, size, bank, w, h, wrap_s, wrap_t, mode, tint, decal, lit, detail = key
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
            if detail:
                image = terrain_detail_image(segments,address,w,h,detail)
            else:
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


def room_mesh(model, room_name, segments, transform, terrain_light=None):
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
                display_mesh(model, room_name, segments, pointer, transform, layer, batches=batches, terrain_light=terrain_light)
    for key, verts in batches.items():
        model.batch(verts,model.material(key,segments,room_name))


def display_mesh(model, name, segments, pointer, transform, layer=0, joint=None, matrices=(), batches=None, terrain_light=None, texture_defs=None, skin_bones=None):
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
    terrain = terrain_light is not None
    env_alpha = 128  # Default scene environment color
    terrain_blend = False
    detail_tile = None
    detail_size = None
    uv_scale = (1,1)
    def tile_scale(shift):
        return 2**(-shift if shift <= 10 else 16-shift)
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
            if terrain:
                uv_scale = (tile_scale(b & 15),tile_scale((b >> 10) & 15))
            tmem = a & 511
            fmt, size, bank = (a >> 21) & 7, (a >> 19) & 3, (b >> 20) & 15
            wrap_s, wrap_t = (b >> 8) & 3, (b >> 18) & 3
        elif terrain and op == 0xf5 and b >> 24 == 1:
            detail_tile = (a & 511,tile_scale(b & 15),tile_scale((b >> 10) & 15),(a>>21)&7,(a>>19)&3)
        elif terrain and op == 0xf2 and b >> 24 == 1:
            detail_size = (((b>>12 & 4095)-(a>>12 & 4095))//4+1, ((b&4095)-(a&4095))//4+1)
        elif terrain and op == 0xfb:
            env_alpha = b & 255
        elif op == 0xf2 and b >> 24 == 0:
            u0, v0 = (a >> 12 & 4095)/4, (a & 4095)/4
            w, h = int((b >> 12 & 4095)/4-u0+1), int((b & 4095)/4-v0+1)
        elif op == 0xfa:
            primitive = tuple(b.to_bytes(4, 'big'))
        elif op == 0xfc:
            # The outdoor combiners modulate baked vertex shade by the
            # base texture, then optionally multiply by primitive color.
            # (TEXEL1 - TEXEL0) * ENV_ALPHA + TEXEL0, then multiply by SHADE.
            terrain_blend = terrain and a == 0xfc267e04 and b == 0x1ffcfdf8
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
                if terrain and lighting:
                    # The RSP shades stationary terrain per vertex before texture modulation.
                    ambient,key,fill,sun_dir,fill_dir = terrain_light
                    sun = max(0,sum(n*v for n,v in zip(normal,sun_dir)))
                    moon = max(0,sum(n*v for n,v in zip(normal,fill_dir)))
                    shade = [min(1,(a+k*sun+f*moon)/255) for a,k,f in zip(ambient,key,fill)]
                    # glTF vertex colors are linear; OoT modulates in display space.
                    color = tuple(round(255*((v+.055)/1.055)**2.4) for v in shade) + (alpha,)
                bone = skin_bones[((b & MASK)//16)+i] if skin_bones and b>>24 == 8 else joint
                cache[first+i] = transform(x,y,z), s/32, t/32, color, bone, normal
        elif op in (5, 6):
            texture = loads.get(tmem, 0)
            rgb = env_tunic or (primitive[:3] if tint_rgb or blend_item_color else (255,255,255))
            tint = rgb + (primitive[3] if tint_alpha else 255,)
            mode = 'BLEND' if layer else 'MASK' if texture_alpha else 'OPAQUE'
            lit = joint is not None and lighting and name != 'gameplay_keep'
            if texture_defs and texture in texture_defs:
                spec = texture_defs[texture]
                w,h = int(spec['Width']),int(spec['Height'])
                encoding = re.fullmatch(r'(rgba|ci|ia|i)(4|8|16|32)',spec['Format'])
                fmt = {'rgba':0,'ci':2,'ia':3,'i':4}[encoding[1]]
                size = ('4','8','16','32').index(encoding[2])
                if fmt == 2 and 'TlutOffset' in spec:
                    palette = (6<<24) + int(spec['TlutOffset'],16)
            detail = None
            us,vs = uv_scale
            uw,vh = w,h
            if (terrain_blend and detail_tile and detail_size and detail_tile[0] != tmem and
                    detail_tile[0] in loads and (fmt,size) == detail_tile[3:] == (0,2)):
                slot,ds,dt = detail_tile[:3]
                dw,dh = detail_size
                # The finer layer can be either tile; S and T often use different shifts.
                pu,pv = max(w/us,dw/ds),max(h/vs,dh/dt)
                su,sv = max(us,ds),max(vs,dt)
                uw,vh = round(pu*su),round(pv*sv)
                detail = (loads[slot],dw,dh,(us/su,vs/sv),(ds/su,dt/sv),uw,vh,env_alpha)
                us,vs = su,sv
            key = (texture, palette if fmt == 2 else 0, fmt,size,bank,w,h,wrap_s,wrap_t, mode, tint, decal, lit, detail)
            for word in ([a,b] if op == 6 else [a]):
                for shift in (16,8,0):
                    pos,s,t,color,bone,normal = cache[(word >> shift & 255)//2]
                    rgba = color if vertex_alpha else color[:3]+(255,)
                    vertex = (pos,((s*us-u0+0.5)/uw,(t*vs-v0+0.5)/vh),rgba)
                    if bone is not None:
                        vertex += (bone,)
                    if lit:
                        vertex += (normal,)
                    batches[key].append(vertex)
    if standalone:
        for key, verts in batches.items():
            model.batch(verts,model.material(key,segments,name))


def actor_model(asset, segment, body_name, skeleton, animation, animation_bank=None, head=None, pose=None, resident=None, model_scale=.01, output_folder=None, extra_clips=None):
    """Standard OoT FlexSkeleton and joint-index animation to a skinned glTF."""
    body = segment(body_name)
    @lru_cache(None)
    def specs(bank):
        return n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
    def addr(bank, name):
        return name if isinstance(name,int) else n64['offset'](specs(bank)[name])
    skel = addr(body_name, skeleton)
    table, count = struct.unpack_from('>IB', body, skel)
    horse = resident and body_name in ('object_horse_normal','object_horse_link_child')
    limb_data = {}
    texture_defs = {0x06000000 + n64['offset'](v):v for v in specs(body_name).values()
                    if 'Format' in v and 'Offset' in v} if resident else None
    limbs = []
    for j in range(count):
        limb, = struct.unpack_from('>I', body, (table & MASK) + 4*j)
        x,y,z,child,sibling,dl = struct.unpack_from('>hhhBBI', body, limb & MASK)
        if horse:
            kind = dl
            dl, = struct.unpack_from('>I',body,(limb&MASK)+12)
            if kind == 4:
                # Convert the original deforming vertex buffer to rigid dominant-joint weights.
                nv,nm,mods,dl = struct.unpack_from('>HHII',body,dl&MASK)
                buf,bones = bytearray(nv*16),[0]*nv
                for m in range(nm):
                    n,nt,_,vs,ts = struct.unpack_from('>HHH2xII',body,(mods&MASK)+16*m)
                    transforms = [struct.unpack_from('>BxhhhBx',body,(ts&MASK)+10*t) for t in range(nt)]
                    bone,px,py,pz,_ = max(transforms,key=lambda v:v[4])
                    for v in range(n):
                        index,u,t,nx,ny,nz,alpha = struct.unpack_from('>HhhbbbB',body,(vs&MASK)+10*v)
                        struct.pack_into('>hhh2xhhbbbB',buf,index*16,px,py,pz,u,t,nx,ny,nz,alpha)
                        bones[index] = bone
                limb_data[j] = (buf,bones)
        limbs.append((x,y,z,child,sibling,dl))
    def units(x,y,z):
        scale = resident.scale if resident else 0.008 if asset == 'fairy' else model_scale
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
        skin_bones = None
        if j in limb_data:
            segments[8],skin_bones = limb_data[j]
        if horse and body_name == 'object_horse_link_child' and j not in limb_data:
            segments[8] = body[addr(bank,'gChildEponaEyeOpenTex'):]
        if resident:
            for slot,symbol in enumerate(resident.textures,8):
                segments[slot] = body[addr(bank,symbol):]
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
            if asset=='push_block':
                banks[8]=data[n64['offset'](table['gPushBlockBaseTex']):]
            if eye:
                segments[10] = data[addr(bank,eye):]
            dl = 0x06000000 + addr(bank,symbol)
        if body_name == 'object_box':
            dl = {0:0x060006F0,2:0x060010C0}.get(j,0)
        if body_name == 'object_daiku' and j == 14:
            # EnDaikuKakariko_PostLimbDraw adds the carpenter's individual hair mesh.
            hair = ('object_daiku_DL_005BD0','object_daiku_DL_005AC0','object_daiku_DL_005990','object_daiku_DL_005880')
            display_mesh(model,bank,segments,0x06000000+addr(bank,hair[int(asset[-1])]),units,joint=j,matrices=matrices)
        if dl:
            display_mesh(model,bank,segments,dl,units,layer=int(asset=='fairy'),joint=j,matrices=matrices,
                         texture_defs=texture_defs,skin_bones=skin_bones)
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
        'gohma':[('Stunned','gGohmaStunnedAnim',None),('Attack','gGohmaAttackAnim',None),('Death','gGohmaDeathAnim',None)],
        'sword_chest':[('Open','gTreasureChestAnim_000128',None)],
        'mido':[('Walk','gMidoWalkAnim',None),('Talk','gMidoHaltAnim',None),('Curious','gMidoCuriousAnim',None),('Annoyed','gMidoAnnoyedAnim',None)],
        'saria':[('Talk','gSariaRightArmExtendedWaitAnim',None),('Surprise','gSariaHandsOnFaceAnim',None)],
        'kokiri_0':[('Talk','gKokiriWipingForeheadAnim',None)],
        'kokiri_2':[('Talk','gKokiriBlockingAnim',1)],
        'kokiri_4':[('Talk','gKokiriBlockingAnim',1)],
    }.get(asset,[]) if extra_clips is None else [(label,symbol,None) for label,symbol in extra_clips]:
        add_clip(label,symbol,frame)
    folder = output_folder or ('oot_residents' if resident else 'kokiri_forest')
    model.write(ROOT/'assets/levels'/folder/f'{asset}.glb')


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
CHOICES = {0x1016:[0x1017,0x1018], 0x1035:[0x1036,0x1037], 0x1038:[0x1039,0x103A,0x103B], 0x103E:[0x103F,0x1040]}


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

    def pages(ident, greeting=False):
        style,p = starts[ident]
        result = []
        line = ''
        while True:
            c = text[p]
            p += 1
            if c == 0x0c:  # BOX_BREAK_DELAYED
                p += 1
                c = 4
            if c in (2,4,7):
                if line:
                    if greeting and ('\x1b' in line or '\x1c' in line):
                        intro = re.split('[\x1b\x1c]',line)[0].rstrip()
                        if intro:
                            result.append(dict(text=intro,style=style))
                        return result
                    page = dict(text=line, style=style)
                    if '\x1b' in line or '\x1c' in line:
                        page['branches'] = [pages(answer) for answer in CHOICES[ident]]
                    result.append(page)
                if c == 7 and greeting:
                    return result
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
            elif c == 0x1e:  # HIGHSCORE: no recorded score for these minigames yet
                p += 1
                line += '0'
            elif c == 0x1f:  # TIME, same daytime sample as the exploration lighting
                line += '10:00'
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


def world_prop(actor, param):
    """Room actor -> the interactive prop `jumpman_sim::link::props` simulates (None: not one), from the actors' param layouts."""
    flag = lambda shift, bits=6: (param >> shift) & ((1 << bits) - 1)
    if actor == 0x125:      # EnKusa: type (bits 0-1), drop table (bits 8-11)
        return dict(k='grass', table=flag(8, 4)) if param & 3 != 2 else None
    if actor == 0x111:      # ObjTsubo: fixed ITEM00 (bits 0-4), collectible flag
        item = param & 0x1f
        return dict(k='pot', item=item) if item < 0x1a else dict(k='pot')
    if actor in (0x1a0, 0x110):     # ObjKibako2 / ObjKibako: crates
        return dict(k='crate')
    if actor == 0x14e:      # EnIshi: size (bit 0), drop table (bits 8-11)
        return dict(k='large_rock') if param & 1 else dict(k='rock', table=flag(8, 4))
    if actor == 0x127:      # ObjBombiwa: bombable boulder, switch flag bits 0-5
        return dict(k='boulder', flag=flag(0))
    if actor == 0x1d2:      # ObjHamishi: hammer rock, switch flag bits 0-5
        return dict(k='hammer_rock', flag=flag(0))
    if actor == 0x59:       # BgBreakwall: cracked wall, switch flag bits 0-5
        return dict(k='cracked_wall', flag=flag(0), wall=flag(13, 2))
    if actor == 0x5e:       # ObjSyokudai: type (bits 12-15), count (6-9: torches of the group that must burn at once, each for count * 50 + 100 frames), flag (0-5), lit (bit 10: always lit)
        return dict(k='torch', flag=flag(0), count=flag(6, 4), timed=int(flag(6, 4) != 0 and not flag(10, 1)), lit=flag(10, 1), type=flag(12, 4))
    if actor == 0xf:        # BgYdanSp: floor (0) / wall (1) web; flag it sets, flag that burns it
        return dict(k='wall_web' if flag(12, 4) == 1 else 'web', flag=flag(0), burn=flag(6))
    if actor == 0x12a:      # ObjSwitch: type (0-2), sub type (4-6), flag (8-13)
        kind = param & 7
        if kind > 4:
            return None
        return dict(k=['floor_switch', 'rusty_switch', 'eye_switch', 'crystal_switch', 'crystal_switch'][kind], flag=flag(8), sub=flag(4, 3))
    return None


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
    # SurfaceType_CanHookshot (z_bgcheck.c): bit 17 of word 1 of the polygon's surface type. The collision header's
    # surface type list (offset 28) holds two big-endian words per type; a polygon's first halfword indexes it.
    surface_types = struct.unpack_from('>I', scene, col+28)[0] & MASK
    hookable = [i for i,(kind,a,_,_) in enumerate(p for p in table(20,'>4H8x') if not p[1] & 0x4000)
                if struct.unpack_from('>2I', scene, surface_types+8*kind)[1] >> 17 & 1]
    water = []
    for x,y,z,dx,dz in table(36,'>5h6x'):
        x1,level,z1 = transform(x,y,z)
        x2,_,z2 = transform(x+dx,y,z+dz)
        water.append([0,x1,z1,x2,z2,level])
    _, starts = header[0]
    x,y,z,yaw = struct.unpack_from('>3h2xh',scene,starts+spawn_index*16+2)
    model = SceneModel()
    objects, talks, exit_rooms, arrivals = [], [], {}, {}
    sim_props = []
    rupees = []
    treasure_chests = []
    warp_pad = None
    def object_collision(bank, symbol, scale):
        data = segment(bank)
        specs = n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
        offset = n64['offset'](specs[symbol])
        return dict(vertices=[[round(v*scale*WORLD_SCALE,3) for v in point]
                              for point in table(12,'>3h',data,offset)],
                    triangles=[[a&8191,b&8191,c&8191,0,0] for _,a,b,c in table(20,'>4H8x',data,offset)])
    quest = {}
    extra_collision = {}
    messages = kokiri_text(segment)
    exported_residents = set()
    resident_positions = set()
    fixed = None
    if course == 'kokiri_forest':
        # US 1.2 gFairySkel and gFairyAnim, shared with the Kokiri shopkeeper.
        extra_collision['sword_chest'] = object_collision('object_box','gTreasureChestCol',0.01)
        extra_collision['tree_mouth'] = object_collision('object_spot04_objects','gDekuTreeMouthCol',1)
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
    if course == 'deku_tree':
        bank = 'object_ydan_objects'
        specs = n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
        prop = SceneModel()
        display_mesh(prop,bank,{6:segment(bank)},0x06000000+n64['offset'](specs['gDTWebFloorDL']),
                     lambda x,y,z: [v*0.1*WORLD_SCALE for v in (x,y,z)],layer=1)
        prop.write(ROOT/'assets/levels/deku_tree/web.glb')
        extra_collision['web'] = object_collision(bank,'gDTWebFloorCol',0.1)
        # Dynamic dungeon actors use the same native model and collision scale.
        def dungeon_prop(asset, bank, symbol, scale, eye=False):
            table = n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
            slot = 5 if bank=='gameplay_dangeon_keep' else 6
            data = segment(bank)
            banks = {slot:data,4:segment('gameplay_keep')}
            if asset=='push_block':
                banks[8]=data[n64['offset'](table['gPushBlockBaseTex']):]
            if eye:
                banks[8] = data[n64['offset'](table['gEyeSwitchGoldOpenTex']):]
                banks[9] = banks[8]
            mesh = SceneModel()
            display_mesh(mesh,bank,banks,(slot<<24)+n64['offset'](table[symbol]),
                         lambda x,y,z:[v*scale*WORLD_SCALE for v in (x,y,z)])
            mesh.write(ROOT/f'assets/levels/deku_tree/{asset}.glb')
        dungeon_prop('ferry',bank,'gDTSlidingPlatformDL',.15)
        extra_collision['ferry'] = object_collision(bank,'gDTSlidingPlatformCol',.15)
        dungeon_prop('eye','gameplay_dangeon_keep','gEyeSwitch1DL',.1,True)
        dungeon_prop('switch','gameplay_dangeon_keep','gFloorSwitch1DL',.1)
        dungeon_prop('slingshot','object_gi_pachinko','gGiSlingshotDL',.22)
        dungeon_prop('gate',bank,'gDTDungeonDoor1DL',1)
        extra_collision['deku_gate']=dict(vertices=[[-176,0,0],[176,0,0],[176,484,0],[-176,484,0]], triangles=[[0,1,2,0,0],[0,2,3,0,0],[2,1,0,0,0],[3,2,0,0,0]])
        dungeon_prop('push_block','gameplay_dangeon_keep','gPushBlockDL',.1)
        extra_collision['push_block']=object_collision('gameplay_dangeon_keep','gPushBlockCol',.1)
        dungeon_prop('stick','object_gi_stick','gGiStickDL',.22)
        for role,asset,pos,collider in [
            ('block','push_block',(-1365,-880,1200),'push_block'),
            ('lowerWeb','web',(-635,-820,0),'web'),
            ('bossDoor','gate',(-896,-1880,-964),'deku_gate'),
            ('stick','stick',(-465,-780,430),None)]:
            objects.append(dict(model=f'levels/deku_tree/{asset}.glb',pos=transform(*pos),
                                yaw=23 if role=='bossDoor' else 0,dungeonRole=role,collision=collider))
        objects.append(dict(model='levels/deku_tree/gate.glb',pos=transform(-335,-880,960),yaw=90,dungeonRole='gate',collision='deku_gate'))

    if scene_name=='ydan_boss':
        # US 1.2 gGohmaSkel follows the table of 85 standard limbs.
        actor_model('gohma',segment,'object_goma',0x1C8F8,'gGohmaHangAnim',output_folder='deku_tree')
        objects.append(dict(model='levels/deku_tree/gohma.glb',pos=transform(-160,-300,-371),yaw=0,dungeonRole='gohma'))
        mesh=SceneModel()
        bank='object_gi_hearts'
        specs=n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
        for symbol in ['gGiHeartContainerDL','gGiHeartBorderDL']:
            display_mesh(mesh,bank,{6:segment(bank),4:segment('gameplay_keep')},0x06000000+n64['offset'](specs[symbol]),lambda x,y,z:[v*.35*WORLD_SCALE for v in (x,y,z)])
        mesh.write(ROOT/'assets/levels/deku_tree/heart_container.glb')
        objects.append(dict(model='levels/deku_tree/heart_container.glb',pos=transform(-160,-610,-371),yaw=0,dungeonRole='container'))
        # Begin just inside the shutter, facing the ceiling perch.
        x,y,z,yaw=150,-640,300,-28000
        icons=segment('icon_item_24_static')
        for name,offset in [('emerald',0x3600),('heart',0x6C00)]:
            (ROOT/f'assets/levels/deku_tree/{name}.png').write_bytes(png(24,24,icons[offset:offset+24*24*4]))
        pages=messages(0x1091)+messages(0x1093)+[dict(text='You received the Kokiri Emerald!',style=3)]+messages(0x102A)
        (ROOT/'assets/levels/deku_tree/farewell.json').write_text(json.dumps(pages,separators=(',',':'))+'\n')
    count, lights = header[15]
    offset = lights + (22 if count > 1 and course != 'deku_tree' else 0)
    ambient,d1,c1,d2,c2 = [list(struct.unpack_from(fmt,scene,offset+i*3)) for i,fmt in
                           enumerate(('>3B','>3b','>3B','>3b','>3B'))]
    if not any(d1) and not any(d2):
        d1,d2 = FIELD_SUN,FIELD_MOON
    def unit(v):
        length = math.hypot(*v) or 1
        return tuple(component/length for component in v)
    terrain_light = ambient,c1,c2,unit(d1),unit(d2)
    if course == 'kakariko_village':
        # Child daytime actor setup, using the display/collision banks selected by each overlay.
        props = {
            0x102: ('windmill_sails','object_spot01_objects','gKakarikoWindmillSailsDL',.1,None),
            0x103: ('well_arch','object_spot01_objects','gKakarikoWellArchDL',.1,'gKakarikoWellArchCol'),
            0x104: ('well_water','object_spot01_objects','gKakarikoWellWaterDL',.1,None),
            0x19D: ('construction','object_spot01_matoyab','gKakarikoConstructionSiteDL',.1,'object_spot01_matoyab_col'),
            0x100: ('trail_gate','object_spot01_matoyab','gKakarikoGuardGateDL',1,'gKakarikoGuardGateCol'),
            0x111: ('pot','object_tsubo','object_tsubo_DL_0017C0',.15,None),
            0x1A0: ('crate','object_kibako2','gLargeCrateDL',.1,'gLargeCrateCol'),
            0x14E: ('rock','gameplay_field_keep','gFieldKakeraDL',.1,None),
            0x125: ('shrub','gameplay_field_keep','gFieldBushDL',.4,None),
            0x77: ('tree','object_wood02','object_wood02_DL_0078D0',1,None),
            0x39: ('arrow_sign','gameplay_keep',0x3DAC0,.01,None),
            9: ('house_door','gameplay_field_keep','gFieldDoorLeftDL',.01,None),
        }
        for actor,(asset,bank,symbol,scale,collider) in props.items():
            prop = SceneModel()
            specs = n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
            slot = {'gameplay_keep':4,'gameplay_field_keep':5}.get(bank,6)
            banks = {2:scene,4:segment('gameplay_keep'),5:segment('gameplay_field_keep'),slot:segment(bank)}
            def local(x,y,z):
                if actor == 9:
                    # The door skeleton stands the leaf upright at its hinge.
                    x,y,z = x-2700,z,-y
                return [v*scale*WORLD_SCALE for v in (x,y,z)]
            parts = [symbol]
            if actor == 0x77:
                parts.append('object_wood02_DL_007968')
            if actor == 9:
                parts.append('gFieldDoorRightDL')
            for part in parts:
                dl = part if isinstance(part,int) else n64['offset'](specs[part])
                display_mesh(prop,bank,banks,(slot<<24)+dl,local,layer=int(actor==0x104))
            prop.write(ROOT/f'assets/levels/kakariko_village/{asset}.glb')
            if collider:
                extra_collision[asset] = object_collision(bank,collider,scale)
        # En_Door placements are held on the scene transition list, outside the room actor list.
        count,doors = header[14]
        for i in range(count):
            actor,dx,dy,dz,angle = struct.unpack_from('>H4h',scene,doors+16*i+4)
            if actor == 9:
                objects.append(dict(model='levels/kakariko_village/house_door.glb',pos=transform(dx,dy,dz),yaw=angle*180/32768))
    @lru_cache(None)
    def scenery_asset(prop):
        bank = prop.bank
        if bank:
            # Dedicated scene banks may reference the owning scene's texture segment.
            specs = n64['definitions'](ROOT/f'reference/oot/assets/xml/objects/{bank}.xml')
            slot = 5 if bank.startswith('gameplay_') else 6
            banks = {2:scene,4:segment('gameplay_keep'),5:segment('gameplay_field_keep'),slot:segment(bank)}
            if course=='deku_tree' or prop.name=='golden_torch':
                banks[5] = segment('gameplay_dangeon_keep')
            if prop.name.startswith('warp_pad_'):
                banks[8] = segment('object_mjin_'+prop.name.removeprefix('warp_pad_'))
            pointers = [(slot<<24)+n64['offset'](specs[symbol]) for symbol in prop.lists.split()]
            if prop.name=='river_waterfall':
                # BgSpot03Taki_Draw issues a vertex load between the material list and triangle list.
                start = len(banks[slot])
                material,triangles,spray = pointers
                banks[slot] += struct.pack('>8I',0xde000000,material,0x01019032,0x06000800,
                                          0xde000000,triangles,0xdf000000,0)
                pointers = [(slot<<24)+start,spray]
            model = SceneModel()
            for pointer in pointers:
                display_mesh(model,bank,banks,pointer,lambda x,y,z:[v*prop.scale*WORLD_SCALE for v in (x,y,z)],
                             layer=int(prop.translucent))
            asset = f'levels/oot_scenery/{prop.name}.glb'
            model.write(ROOT/'assets'/asset)
        else:
            asset = f'levels/{prop.name}.glb'
        if prop.collision:
            extra_collision[prop.name] = object_collision(bank or 'object_kibako2',prop.collision,prop.scale)
        return asset

    def place_prop(prop,x,y,z,yaw,**extra):
        obj = dict(model=scenery_asset(prop),pos=transform(x,y,z),yaw=yaw*180/32768,**extra)
        if prop.collision:
            obj['collision'] = prop.name
        objects.append(obj)

    if course in OVERWORLD and course not in ('kakariko_village','kokiri_forest'):
        count,doors = header.get(14,(0,0))
        for i in range(count):
            actor,dx,dy,dz,angle = struct.unpack_from('>H4h',scene,doors+16*i+4)
            if actor == 9:
                place_prop(scenery['Prop']('kakariko_village/house_door'),dx,dy,dz,angle)
    for name in names:
        if name.startswith(scene_name+'_room_'):
            segments = {2: scene, 3: segment(name)}
            if scene_name == 'ydan':
                specs = n64['definitions'](ROOT/'reference/oot/assets/xml/scenes/dungeons/ydan.xml')
                segments[8] = scene[n64['offset'](specs['gDekuTreeDayEntranceTex']):]
            if scene_name in DAY_TEXTURES:
                specs = n64['definitions'](ROOT/f'reference/oot/assets/xml/scenes/overworld/{scene_name}.xml')
                segments[8] = scene[n64['offset'](specs[DAY_TEXTURES[scene_name]]):]
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
            room_mesh(model, name, segments, transform, terrain_light)
            count, actor_list = rh.get(1,(0,0))
            placements = [struct.unpack_from('>8h',room,actor_list+16*i) for i in range(count)]
            for i,(actor,ax,ay,az,_,angle,_,param) in enumerate(placements):
                pos = transform(ax,ay,az)
                prop = world_prop(actor,param&0xffff)
                if prop:
                    sim_props.append(dict(k=prop.pop('k'),pos=pos,yaw=round(angle*180/32768,2),**prop))
                if course=='deku_tree':
                    role = None
                    if name=='ydan_room_5':
                        if actor==0x50 and param&255==0: role='ferry'
                        if actor==0x12a and param==0x3e00: role='switch'
                    if name=='ydan_room_4' and actor==0x12a: role='eye'
                    if role:
                        obj=dict(model=f'levels/deku_tree/{role}.glb',pos=pos,yaw=angle*180/32768,dungeonRole=role)
                        if role=='ferry': obj['collision']='ferry'
                        objects.append(obj)
                        continue
                if course != 'kakariko_village':
                    if actor == 0x26:
                        if 'flagpole' not in extra_collision:
                            actor_model('flagpole',segment,'object_hata','gFlagpoleSkel','gFlagpoleFlapAnim',
                                        model_scale=1/75,output_folder='oot_scenery')
                            extra_collision['flagpole'] = object_collision('object_hata','gFlagpoleCol',1/75)
                        objects.append(dict(model='levels/oot_scenery/flagpole.glb',pos=pos,yaw=angle*180/32768,
                                            collision='flagpole',idle=True))
                        continue
                    if actor==0x15 and param&255==3 and course in ('sarias_house','deku_tree'):
                        place_prop(scenery['RECOVERY_HEART'],ax,ay+8.6,az,angle,recoveryHeart=True)
                        continue
                    if actor==0x77 and param&255 in (3,6,8,13,15):
                        kind = param&255
                        for dx,dz,heading in scenery['wood_offsets'](kind,angle):
                            place_prop(scenery['prop_for'](actor,param),ax+dx,ay,az+dz,heading,
                                       groundedProp=True,actorScale=1.5 if kind==15 else 1)
                        continue
                    if actor == 0x151 and param&3 < 3:
                        kind = param&3
                        prop = scenery['prop_for'](0x14e if kind==2 else 0x125,0)
                        for dx,dz in scenery['cluster_offsets'](kind):
                            place_prop(prop,ax+dx,ay,az+dz,0,groundedProp=True)
                        continue
                    # Kokiri's previously restored square signs already have dialogue.
                    prop = scenery['prop_for'](actor,param&65535) if (actor,course)!=(0x141,'kokiri_forest') else None
                    if prop:
                        extra = {}
                        if actor==0x77:
                            extra['actorScale'] = {0:1.5,2:.6,12:1.5,15:1.5}.get(param&255,1)
                        if actor==0x102:
                            extra['spinZ'] = 100*20*math.pi/32768
                        if prop.name in ('river_waterfall','domain_falls_0','domain_falls_1','crater_smoke'):
                            extra['scrollT'] = -.2 if actor==0xbf else .32
                        if actor==0xbf:
                            extra['billboard'] = True
                        if actor in (0x125,0x14e,0x127):
                            extra['groundedProp'] = True
                        # Use the native open endpoints for gates along existing exploration routes.
                        py = ay + {0x190:200,0x1c4:180,0x18d:-120 if param&1 else 0}.get(actor,0)
                        place_prop(prop,ax,py,az,angle,**extra)
                        if actor==0x6e:
                            warp_pad = dict(pos=pos,course=course)
                            arrivals['warp-pad'] = dict(pos=pos.copy(),faceYaw=angle)
                        if actor==0x4a:
                            yaw = angle*math.pi/32768
                            for side in (-158,158):
                                place_prop(scenery['DRAWBRIDGE_CHAIN'],ax+side*math.cos(yaw)+400*math.sin(yaw),ay+10,
                                           az-side*math.sin(yaw)+400*math.cos(yaw),angle)
                        if actor in (0x39,0x141):
                            text_id = (0x300|(param>>8)) if actor==0x39 else (0x300|param)
                            if 0x300 <= text_id < 0x400:
                                talks.append(dict(kind='sign',pos=pos,pages=messages(text_id,True)))
                        continue
                    if actor==10 and course!='kokiri_forest':
                        # EnBox stores the get-item ID in bits 5..11 and its scene-local treasure flag in bits 0..4.
                        key = f'{course}:{param&31}'
                        treasure_chests.append(dict(key=key,pos=pos,item=(param>>5)&127))
                        small = ((param>>12)&15) in (5,6,7,8)
                        collider = 'small_chest' if small else 'large_chest'
                        if collider not in extra_collision:
                            extra_collision[collider] = object_collision('object_box','gTreasureChestCol',.005 if small else .01)
                        objects.append(dict(model='levels/kokiri_forest/sword_chest.glb',pos=pos,collision=collider,
                                            yaw=angle*180/32768,actorScale=.5 if small else 1,treasureKey=key))
                        continue
                if course == 'kakariko_village':
                    if actor in props:
                        asset,_,_,_,collider = props[actor]
                        if actor == 0x100:
                            # Use the guard's open endpoint to preserve the exploration route to Death Mountain.
                            pos = transform(-89,ay,-1375)
                        if actor == 0x104:
                            pos = transform(ax,52,az)
                            water[0][5] = pos[1]
                        if actor == 0x19D:
                            # The construction-site actor spawns Sabooro at its first path point during the day.
                            path = header[13][1] + 8*((param>>8)&255)
                            _,points = struct.unpack_from('>B3xI',scene,path)
                            cx,cy,cz = struct.unpack_from('>3h',scene,points&MASK)
                            placements.append((0x1BC,cx,cy,cz,0,angle,0,(param&0xff00)|1))
                        obj = dict(model=f'levels/kakariko_village/{asset}.glb',pos=pos,yaw=angle*180/32768)
                        if collider:
                            obj['collision'] = asset
                        if actor == 0x102:
                            obj['spinZ'] = 100*20*math.pi/32768  # BgSpot01Fusya at the normal 20 Hz rate
                        if actor == 0x39:
                            talks.append(dict(kind='sign',pos=pos,pages=messages(0x300|(param>>8))))
                        objects.append(obj)
                        continue
                    if actor == 0x141:
                        objects.append(dict(model='levels/kokiri_forest/sign.glb',pos=pos,yaw=angle*180/32768))
                        talks.append(dict(kind='sign',pos=pos,pages=messages(param|0x300)))
                        continue
                if course in EXPLORATION_SCENES or course in ('lon_lon_ranch','deku_tree'):
                    if actor == 0x146 and course == 'sacred_forest_meadow':
                        objects.append(dict(model='levels/kokiri_forest/saria.glb',pos=pos,yaw=angle*180/32768,idle=True))
                        talks.append(dict(pos=pos,kind='resident',id=f'{name}-{i}',label='Saria',pages=messages(0x10AD,True)))
                        continue
                    resident = RESIDENTS.get(actor)
                    if resident and resident_data['visible_actor'](actor,param):
                        placement = (actor,tuple(pos))
                        if placement in resident_positions:
                            continue
                        resident_positions.add(placement)
                        asset = resident.bank
                        if actor == 0x1BC:
                            asset += '_'+str(param&3)
                        if asset not in exported_residents:
                            actor_model(asset,segment,'object_'+resident.bank,resident.skeleton,resident.idle,
                                        resident.motion_bank or None,resident=resident)
                            exported_residents.add(asset)
                        extra = {}
                        if actor == 0x13D:
                            extra['actorScale'] = 5  # Medigoron
                        elif actor == 0x1AE and (param & 31) in (0,4):
                            extra['actorScale'] = 2.6 if (param & 31)==0 else 1.5
                        objects.append(dict(model=f'levels/oot_residents/{asset}.glb',pos=pos,
                                            yaw=angle*180/32768,idle=True,**extra))
                        greeting = resident_data['greeting_for'](actor,param,course)
                        label = {0x124:'Diving Game Host',0x17B:'Pierre',0x13D:'Medigoron'}.get(actor,resident.label)
                        if greeting:
                            objects[-1]['speaker'] = f'{name}-{i}'
                            talks.append(dict(pos=pos,kind='resident',id=f'{name}-{i}',label=label,
                                              **(dict(range=1150,height=2000) if actor==0x14D else {}),
                                              pages=messages(greeting,True)))
                        continue
                if course == 'deku_tree' and name == 'ydan_room_0' and actor == 15:
                    objects.append(dict(model='levels/deku_tree/web.glb',pos=pos,yaw=angle*180/32768,collision='web'))
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
    connections = overworld_entrances() if course in OVERWORLD else {}
    surface_types = struct.unpack_from('>I',scene,col+28)[0] & MASK
    for typ,a,_,_ in table(20,'>4H8x'):
        if a & 0x4000:
            continue
        idx = (struct.unpack_from('>I',scene,surface_types+typ*8)[0] >> 8) & 31
        rooms.append(idx)
        if idx:
            entrance = struct.unpack_from('>H',scene,header[0x13][1]+2*(idx-1))[0]
            route = {0x000:('deku_tree','door'),
                     0x209:('kokiri_forest','tree'),0x272:('links_house','door'),0xc1:('kokiri_shop','door'),
                     0x211:('kokiri_forest','house'),0x266:('kokiri_forest','shop'),
                     0xc9:('know_it_all_house','door'),0x26a:('kokiri_forest','brothers'),
                     0x9c:('twins_house','door'),0x33c:('kokiri_forest','twins'),
                     0x433:('midos_house','door'),0x443:('kokiri_forest','mido'),
                     0x437:('sarias_house','door'),0x447:('kokiri_forest','saria')}.get(entrance)
            if not route:
                route = connections.get(entrance)
            if route:
                exit_rooms[idx] = dict(course=route[0],entry=route[1])
    if course == 'kokiri_forest':
        for key,i in [('tree',1),('house',3),('shop',4),('brothers',5),('twins',8),('mido',9),('saria',10)]:
            xx,yy,zz,angle = struct.unpack_from('>3h2xh',scene,starts+16*i+2)
            arrivals[key] = dict(pos=transform(xx,yy,zz),faceYaw=angle)
    if course in OVERWORLD:
        for i in range(header[0][0]):
            xx,yy,zz,angle = struct.unpack_from('>3h2xh',scene,starts+16*i+2)
            arrivals[f'spawn_{i}'] = dict(pos=transform(xx,yy,zz),faceYaw=angle)
    if course == 'links_house':
        xx,yy,zz,angle = struct.unpack_from('>3h2xh',scene,starts+16+2)
        arrivals['door'] = dict(pos=transform(xx,yy,zz),faceYaw=angle)
    if course=='deku_tree':
        for obj in objects:
            prop_name = obj['model'].split('/')[-1]
            if 'dungeonRole' not in obj and prop_name in ('deku_ladder.glb','web.glb','deku_spike_log.glb','collapsing_platform.glb'):
                obj['dungeonRole'] = {'deku_ladder.glb':'ladder','web.glb':'web','deku_spike_log.glb':'log',
                                      'collapsing_platform.glb':'steppingStone'}[prop_name]
            brothers=[(-662,-1880,-957),(-311,-1880,-904),(-946,-1880,-767)]
            for i,xyz in enumerate(brothers):
                if prop_name=='hintnuts.glb' and obj['pos']==transform(*xyz):
                    obj['dungeonRole']=f'brother{i+1}'
            if prop_name=='hintnuts.glb' and obj['pos']==transform(-66,-880,962):
                obj['dungeonRole']='scrub'
            if prop_name=='hintnuts.glb' and obj.get('dungeonRole'):
                speaker=obj.pop('speaker',None)
                talks=[talk for talk in talks if talk.get('id')!=speaker]
    path = f'levels/{course}/terrain.glb'
    model.write(ROOT/'assets'/path)
    data = dict(model=path, props=sim_props, vertices=vertices, triangles=triangles, spawn=dict(pos=transform(x,y,z),faceYaw=yaw),
                waterBoxes=water, horizontalBoundary=boundary, explorationOnly=True, objects=objects, coins=[], extraCollision=extra_collision,
                hookable=hookable)
    if exit_rooms or talks or course in OVERWORLD or scene_name=='ydan_boss':
        data.update(rooms=rooms, kokiri=dict(exits=exit_rooms, talks=talks, arrivals=arrivals, fixed=fixed,rupees=rupees))
    if treasure_chests:
        data['kokiri']['treasureChests'] = treasure_chests
    if warp_pad:
        data['kokiri']['warpPad'] = warp_pad
    if 'kokiri' in data:
        if course in ('deku_tree','deku_tree.2', 'zoras_domain', 'goron_city'):
            data['kokiri']['enclosed'] = True
        data['kokiri']['light'] = dict(ambient=ambient,key=c1,keyDir=d1,fill=c2,fillDir=d2)
    if course == 'kokiri_shop':
        # Face the appropriate shelf from the customer side of the counter.
        data['kokiri']['counter'] = dict(shield=transform(50,0,35),heart=transform(-80,0,35))
    if course == 'kokiri_forest':
        # Face of the ladder below the house spans x=-45..-13, z=1004.
        # Land clear of its front collider at ground level and on the porch above.
        # BgTreemouth_Update interpolates this translation over 100 original gameplay frames.
        quest['mouth'] = dict(closed=transform(4029,136,-1255), opened=transform(3869,-263,-1163),
                             meeting=transform(3827,-161,-1142), eye=transform(3740,-141,-530),
                             lookFrom=transform(3777,-89,-605),lookTo=transform(3763,-126,-621),
                             first=messages(0x1015), retry=messages(0x1016))
        quest['crawl'] = [transform(-785,120,1030), transform(-785,120,1408)]
        data['kokiri']['quest'] = quest
        data['kokiri']['ladder'] = [transform(-29,-80,983), transform(-29,100,1037)]
        data['kokiri']['repeat'] = messages(0x1003)
        data['kokiri']['greeting'] = messages(0x1001)
        data['kokiri']['afterMido'] = [messages(0x1031),messages(0x1032)]
    (ROOT/'assets/courses'/f'{course}.json').write_text(json.dumps(data,separators=(',',':'))+'\n')
    print(f'{course}: {len(triangles)} collision triangles ({len(hookable)} hookshot surfaces), {len(model.doc["materials"])} materials')


if __name__ == '__main__':
    rom = (ROOT/'reference/roms/oot-us-1.2.z64').read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError('Expected a big-endian US 1.2 OoT ROM')
    with (ROOT/'reference/oot/baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    for course, (scene, spawn) in COURSES.items():
        export(rom, names, course, scene, spawn)
