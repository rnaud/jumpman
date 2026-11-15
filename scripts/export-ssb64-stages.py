#!/usr/bin/env python3
"""Export the Super Smash Bros. (US) stages from a local ROM as walkable courses (geometry, collision, spawns, bounds).

Formats follow VetriTheRetri/ssb-decomp-re (reference/ssb64, see native/SSB64.md "Stages"):
  * A stage is a GR<Name>Map file (src/mp/mpcollision.c dMPCollisionGroundFileInfos) holding an MPGroundData
    (src/mp/mptypes.h): four geometry layers (DObjDesc tree, AObjEvent32 joint animations, MObjSub materials and their
    animations, drawn by grDisplayMakeGeometryLayer, src/gr/grdisplay.c; layer_mask bit = the layer's DObjs carry
    DObjDLLink lists instead of one display list), the map collision (MPGeometryData), camera and blast-zone bounds
    (map_bound_*), the BGM id and the map objects (player starts, item spawns, respawn point).
  * MPGeometryData: yakumono (collision groups; group i moves with DObj i of layer 1, mpCollisionAllocYakumono), each
    with four line sets (floor, ceiling, right wall, left wall: MPLineInfo). A line is a polyline of vertex ids
    (MPVertexLinks {first, count} into the id array); each vertex is {s16 x, y; u16 flags} with the pass-through (0x4000)
    and ledge (0x8000) bits in the high byte and the material in the low byte (src/mp/mpdef.h).
The stages are 2D (x right, y up); the exporter extrudes every collision segment over a depth band (z in [-depth,
depth]) and closes the band with two invisible walls (surface 0x76, no camera collision), so a 3D character walks the
stage's real lines. Display lists go through the fighter exporter's F3DEX2 interpreter (scripts/export-ssb64.py).

Writes <out>/courses/ssb64_<id>.json (course document: collision, spawn, `ssb64` block with bounds, blast zones, spawn
points, BGM, pass-through triangles) and <out>/levels/ssb64_<id>/stage.glb. native/byor-rs/src/ssb64_stages.rs is
the byte-identical Rust port.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROM = ROOT / 'reference/roms/ssb64-us.z64'
OUT = ROOT / 'assets'
SHA1 = 'e2929e10fccc0aa84e5776227e798abc07cedabf'

_spec = importlib.util.spec_from_file_location('export_ssb64', Path(__file__).resolve().parent / 'export-ssb64.py')
ex = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ex)

K = 0.348           # SM64 units per SSB64 unit (native/sim/src/smash.rs: Smash Mario 460 tall = SM64 Mario 160)
GLB = K * 0.01      # glb metres per SSB64 unit (the course models are in SM64 units x 0.01)
DEPTH = 360.0       # half depth of the collision band, SM64 units
DOBJ_END = 18
SURF_DEFAULT = 0
SURF_NO_CAM = 0x76  # SM64 SURFACE_NO_CAM_COLLISION: the depth walls
LINE_KINDS = ['floor', 'ceil', 'rwall', 'lwall']
ANIM_FRAMES = 7200  # longest stage animation period looked for (Peach's Castle's platform loops every 4800 frames)
FPS = 60.0

# (id, map file, label, kind): dMPCollisionGroundFileInfos order (src/gr/grdef.h GRKind)
BONUS_FIGHTERS = [('mario', 'Mario'), ('fox', 'Fox'), ('donkey', 'Donkey Kong'), ('samus', 'Samus'), ('luigi', 'Luigi'),
                  ('link', 'Link'), ('yoshi', 'Yoshi'), ('captain', 'Captain Falcon'), ('kirby', 'Kirby'),
                  ('pikachu', 'Pikachu'), ('purin', 'Jigglypuff'), ('ness', 'Ness')]
STAGES = [
    ('castle', 'GRCastleMap', "Peach's Castle", 'battle'),
    ('sector', 'GRSectorMap', 'Sector Z', 'battle'),
    ('jungle', 'GRJungleMap', 'Congo Jungle', 'battle'),
    ('zebes', 'GRZebesMap', 'Planet Zebes', 'battle'),
    ('hyrule', 'GRHyruleMap', 'Hyrule Castle', 'battle'),
    ('yoster', 'GRYosterMap', "Yoshi's Island", 'battle'),
    ('pupupu', 'GRPupupuMap', 'Dream Land', 'battle'),
    ('yamabuki', 'GRYamabukiMap', 'Saffron City', 'battle'),
    ('inishie', 'GRInishieMap', 'Mushroom Kingdom', 'battle'),
    ('pupupu_small', 'GRPupupuSmallMap', 'Dream Land (beta)', 'extra'),
    ('pupupu_test', 'GRPupupuTestMap', 'Test Stage', 'extra'),
    ('explain', 'GRExplainMap', 'How to Play', 'extra'),
    ('yoster_small', 'GRYosterSmallMap', "Mini Yoshi's Island", 'extra'),
    ('metal', 'GRMetalMap', 'Meta Crystal', 'extra'),
    ('zako', 'GRZakoMap', 'Duel Zone', 'extra'),
    ('bonus3', 'GRBonus3Map', 'Race to the Finish', 'extra'),
    ('last', 'GRLastMap', 'Final Destination', 'extra'),
] + [(f'bonus1_{f}', '', f'Break the Targets: {n}', 'bonus1') for f, n in BONUS_FIGHTERS] \
  + [(f'bonus2_{f}', '', f'Board the Platforms: {n}', 'bonus2') for f, n in BONUS_FIGHTERS]


def map_file_name(stage):
    """The map file of a STAGES row (the bonus names follow the fighter file stems: Donkey, Captain, Purin)."""
    sid, name, _, kind = stage
    if kind in ('bonus1', 'bonus2'):
        stem = {'mario': 'Mario', 'fox': 'Fox', 'donkey': 'Donkey', 'samus': 'Samus', 'luigi': 'Luigi', 'link': 'Link',
                'yoshi': 'Yoshi', 'captain': 'Captain', 'kirby': 'Kirby', 'pikachu': 'Pikachu', 'purin': 'Purin',
                'ness': 'Ness'}[sid.split('_', 1)[1]]
        return f'GR{"Bonus1" if kind == "bonus1" else "Bonus2"}{stem}Map'
    return name


def r32(x):
    return struct.unpack('>f', struct.pack('>f', x))[0]


def rnd(x, n=3):
    """Round for json (a plain float that both exporters print the same way)."""
    v = round(x, n)
    return 0.0 if v == 0 else v


# ---------------------------------------------------------------------------------------------- map header

def header_offset(files, fid):
    """Where the MPGroundData sits in the map file: VS maps keep their item weights first (header at 0x14, item_weights
    pointer at +0x84), the others start with it. The map geometry pointer (+0x40) is always set."""
    d, p = files.get(fid)
    for o in range(0, len(d) - 0xA8 + 1, 4):
        if (o + 0x40) in p and (o + 0x84) in p:
            return o
    if 0x40 in p:
        return 0
    raise ValueError(f'file {fid}: no MPGroundData')


def read_header(files, fid):
    o = header_offset(files, fid)
    base = (fid, o)
    d = files.data(fid)
    h = lambda off: struct.unpack_from('>h', d, o + off)[0]
    layers = []
    for i in range(4):
        layers.append(dict(dobjdesc=files.ptr(base, 16 * i), anim=files.ptr(base, 16 * i + 4), mobjs=files.ptr(base, 16 * i + 8),
                           matanim=files.ptr(base, 16 * i + 12)))
    return dict(offset=o, layers=layers, geometry=files.ptr(base, 0x40), layer_mask=d[o + 0x44], fog=list(d[o + 0x4C:o + 0x4F]),
                light=list(struct.unpack_from('>3f', d, o + 0x60)),
                camera=dict(top=h(0x6C), bottom=h(0x6E), right=h(0x70), left=h(0x72)),
                blast=dict(top=h(0x74), bottom=h(0x76), right=h(0x78), left=h(0x7A)),
                bgm=struct.unpack_from('>I', d, o + 0x7C)[0], alt_warning=h(0x88))


# ---------------------------------------------------------------------------------------------- AObjEvent32

def simulate32(files, script, t, r, s, frames=ANIM_FRAMES):
    """gcParseDObjAnimJoint + gcPlayDObjAnimJoint on one DObj's AObjEvent32 script, frame by frame:
    [(t, r, s)] per frame until End or a jump back (one period of a looping animation)."""
    t, r, s = list(t), list(r), list(s)
    aobjs = {}
    wait = 0.0
    out = []
    ref = script
    ended = False
    d = files.data(ref[0])
    for frame in range(frames):
        if frame > 0:
            if ended:
                break
            wait -= 1.0
        while wait <= 0.0 and not ended:
            w = struct.unpack_from('>I', d, ref[1])[0]
            op, flags, payload = w >> 25, (w >> 15) & 0x3FF, float(w & 0x7FFF)
            nxt = (ref[0], ref[1] + 4)
            if op in (3, 4, 5, 6, 7, 8, 9, 10, 11):
                ref = nxt
                for i in range(10):
                    if not flags:
                        break
                    if flags & 1:
                        a = aobjs.setdefault(i, dict(kind=0, vb=0.0, vt=0.0, rb=0.0, rt=0.0, length=0.0, inv=1.0))
                        v = struct.unpack_from('>f', d, ref[1])[0]
                        ref = (ref[0], ref[1] + 4)
                        if op == 7:
                            a['rt'] = v
                        else:
                            a['vb'] = a['vt']
                            a['vt'] = v
                            if op in (5, 6):
                                a['rb'] = a['rt']
                                a['rt'] = struct.unpack_from('>f', d, ref[1])[0]
                                ref = (ref[0], ref[1] + 4)
                                a['kind'] = 3
                                if payload:
                                    a['inv'] = 1.0 / payload
                            elif op in (8, 9):
                                a['rb'] = a['rt']
                                a['rt'] = 0.0
                                a['kind'] = 3
                                if payload:
                                    a['inv'] = 1.0 / payload
                            elif op in (3, 4):
                                a['kind'] = 2
                                if payload:
                                    a['rb'] = (a['vt'] - a['vb']) / payload
                                a['rt'] = 0.0
                            else:  # 10, 11: step
                                a['kind'] = 1
                                a['inv'] = payload
                                a['rt'] = 0.0
                            a['length'] = -wait - 1.0
                    flags >>= 1
                if op in (3, 5, 8, 10):
                    wait += payload
            elif op == 2:  # Wait
                wait += payload
                ref = nxt
            elif op in (1, 14):  # Jump / SetAnim: a loop back ends the period
                ended = True
                for a in aobjs.values():
                    if a['kind']:
                        a['length'] += 1.0 + wait
                break
            elif op in (15, 16):  # SetFlags, the anim callback: wait only here
                wait += payload
                ref = nxt
            elif op == 17:  # callback values: skipped
                wait += payload
                ref = (ref[0], ref[1] + 4 + 4 * bin(flags).count('1'))
            elif op == 13:  # SetInterp (spline): not followed
                ref = (ref[0], ref[1] + 8)
            elif op == 12:
                ref = nxt
                for i in range(10):
                    if not flags:
                        break
                    if flags & 1 and i in aobjs:
                        aobjs[i]['length'] += payload
                    flags >>= 1
            else:  # 0 End (and anything unknown)
                ended = True
                for a in aobjs.values():
                    if a['kind']:
                        a['length'] += 1.0 + wait
                break
        for i, a in sorted(aobjs.items()):
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
        out.append((list(t), list(r), list(s)))
    return out


# ---------------------------------------------------------------------------------------------- geometry

def layer_joints(files, layer, li):
    """gcSetupCustomDObjs: the layer's DObjs (id & 0xFFF = depth, parent = last DObj one level up), posed at frame 0.
    An animated DObj keeps one period of its animation (`frames`); `moving` = the pose changes during it."""
    desc = layer['dobjdesc']
    if not desc:
        return []
    dd = files.data(desc[0])
    joints, stack = [], {}
    i = 0
    while True:
        o = desc[1] + 44 * i
        jid = struct.unpack_from('>i', dd, o)[0]
        if jid == DOBJ_END:
            break
        depth = jid & 0xFFF
        vals = struct.unpack_from('>9f', dd, o + 8)
        j = dict(index=i, layer=li, parent=stack.get(depth - 1) if depth else None, kind=jid & 0xF000, dl=files.ptr(desc, 44 * i + 4),
                 t=list(vals[0:3]), r=list(vals[3:6]), s=list(vals[6:9]), anim=None, frames=None, moving=False)
        if layer['anim']:
            j['anim'] = files.ptr(layer['anim'], 4 * i)
        if j['anim']:
            frames = simulate32(files, j['anim'], j['t'], j['r'], j['s'])
            if frames:
                j['t'], j['r'], j['s'] = frames[0]
                j['frames'] = frames
                j['moving'] = any(f != frames[0] for f in frames)
        stack[depth] = len(joints)
        joints.append(j)
        i += 1
    for j in joints:
        j['local'] = ex.mat_trs(j['t'], j['r'], j['s'])
        j['world'] = ex.mat_mul(joints[j['parent']]['world'], j['local']) if j['parent'] is not None else j['local']
    return joints


IDENTITY = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]


def stage_mesh(files, header, layers, solid_ids):
    """Every layer's display lists into one Mesh, frame 0. A DObj whose animation moves it (and its subtree) is a part:
    `solid` parts (a moving yakumono of layer 1) keep world coordinates, the others (`anim`) their root's local space.
    Each vertex's `joint` is its part number (0 = the static stage)."""
    m = ex.Mesh(files)
    m.geom = 0x00000001 | 0x00000004 | ex.G_CULL_BACK | 0x00200000  # z-buffered, culled, unlit until the DLs say otherwise
    m.light1 = [1.0, 1.0, 1.0]
    parts = []
    for li, layer in enumerate(header['layers']):
        joints = layers[li]
        links = header['layer_mask'] >> li & 1
        rel = {}
        for ji, j in enumerate(joints):
            p = j['parent']
            j['root'] = None
            if p is not None and joints[p]['root'] is not None:
                j['root'] = joints[p]['root']
            elif j['moving']:
                j['root'] = ji
                parts.append(dict(layer=li, joint=ji, kind='solid' if li == 1 and ji in solid_ids else 'anim', parent=p))
                j['part'] = len(parts)
            if j['root'] is None:
                part, world = 0, j['world']
            else:
                root = joints[j['root']]
                part = root['part']
                if parts[part - 1]['kind'] == 'solid':
                    world = j['world']
                else:
                    world = IDENTITY if j['root'] == ji else ex.mat_mul(rel[p], j['local'])
                rel[ji] = world
            mlist = files.ptr(layer['mobjs'], 4 * ji) if layer['mobjs'] else None
            clist = files.ptr(layer['matanim'], 4 * ji) if layer['matanim'] else None
            m.mobjs = ex.read_mobjs(files, mlist)
            for k, mo in enumerate(m.mobjs):
                ex.costume(files, mo, files.ptr(clist, 4 * k) if clist else None)
            if not j['dl']:
                continue
            if links:  # DObjDLLink {s32 list_id; Gfx *dl} until list_id 4
                dl_data = files.data(j['dl'][0])
                k = 0
                while k < 8:
                    lid = struct.unpack_from('>i', dl_data, j['dl'][1] + 8 * k)[0]
                    if lid == 4:
                        break
                    sub = files.ptr(j['dl'], 8 * k + 4)
                    if sub:
                        m.run(sub, part, world)
                    k += 1
            else:
                m.run(j['dl'], part, world)
    return m, parts


def add_mesh(g, mesh, part, name, shift, scale, cache):
    """The triangles of one part (by their first vertex) as a glTF mesh: one primitive per material. Positions are
    (p - shift) * scale. `cache` keeps the glb's materials and textures. Returns the mesh index, or None if empty."""
    prims = []
    for mi, key in enumerate(mesh.tris):
        all_idx = mesh.tris[key]
        idx = [v for k in range(0, len(all_idx), 3) if mesh.verts[all_idx[k]][0] == part for v in all_idx[k:k + 3]]
        if not idx:
            continue
        mat = mesh.materials[key]
        vids = sorted(set(idx))
        remap = {v: i for i, v in enumerate(vids)}
        pos, nrm, uv, col = [], [], [], []
        for v in vids:
            _, p, n, t, c = mesh.verts[v]
            pos.append([ex.f32((p[k] - shift[k]) * scale) for k in range(3)])
            if n is None:
                n = [0.0, 1.0, 0.0]
            ln = math.sqrt(sum(x * x for x in n)) or 1.0
            nrm.append([ex.f32(x / ln) for x in n])
            uv.append([ex.f32(x) for x in t])
            col.append([ex.f32(x) for x in (c or [1.0, 1.0, 1.0, 1.0])])
        attrs = {'POSITION': g.accessor(pos, 'VEC3', target=34962, minmax=True), 'NORMAL': g.accessor(nrm, 'VEC3', target=34962)}
        if 'image' in mat:
            attrs['TEXCOORD_0'] = g.accessor(uv, 'VEC2', target=34962)
        if mat['vcol']:
            attrs['COLOR_0'] = g.accessor(col, 'VEC4', target=34962)
        indices = g.accessor([remap[i] for i in idx], 'SCALAR', 5123 if len(vids) < 65536 else 5125, 34963)
        if mi not in cache['materials']:
            m = {'name': f'mat{mi:03d}', 'pbrMetallicRoughness': {'baseColorFactor': [ex.f32(x) for x in mat['color']], 'metallicFactor': 0.0, 'roughnessFactor': 1.0}}
            if mat['double']:
                m['doubleSided'] = True
            if mat['alpha'] != 'OPAQUE':
                m['alphaMode'] = mat['alpha']
                if mat['alpha'] == 'MASK':
                    m['alphaCutoff'] = 0.5
            if 'image' in mat:
                w, h, px = mat['image']
                data = ex.png(w, h, px)
                ik = (data, mat['wrap'])
                if ik not in cache['images']:
                    g.j['images'].append({'bufferView': g.view(data), 'mimeType': 'image/png'})
                    sampler = {'magFilter': 9729, 'minFilter': 9729, 'wrapS': mat['wrap'][0], 'wrapT': mat['wrap'][1]}
                    if sampler not in g.j['samplers']:
                        g.j['samplers'].append(sampler)
                    g.j['textures'].append({'source': len(g.j['images']) - 1, 'sampler': g.j['samplers'].index(sampler)})
                    cache['images'][ik] = len(g.j['textures']) - 1
                m['pbrMetallicRoughness']['baseColorTexture'] = {'index': cache['images'][ik]}
            g.j['materials'].append(m)
            cache['materials'][mi] = len(g.j['materials']) - 1
        prims.append({'attributes': attrs, 'indices': indices, 'material': cache['materials'][mi]})
    if not prims:
        return None
    g.j['meshes'].append({'name': name, 'primitives': prims})
    return len(g.j['meshes']) - 1


def new_gltf():
    g = ex.Gltf()
    g.j['asset']['generator'] = 'jumpman export-ssb64-stages.py'
    return g, {'materials': {}, 'images': {}}


def write_stage_glb(mesh, parts, layers, path):
    """stage.glb: node 0 the static stage (metres), node 1 `parts` holding each `anim` part as a node with its parent's
    frame-0 world matrix (and the SSB64 -> metres scale) over a child posed and animated like the part's root DObj (one
    looping clip per part, 60 fps)."""
    g, cache = new_gltf()
    m0 = add_mesh(g, mesh, 0, 'stage', [0.0, 0.0, 0.0], GLB, cache)
    g.j['nodes'].append({'name': 'stage'} if m0 is None else {'name': 'stage', 'mesh': m0})
    g.j['nodes'].append({'name': 'parts', 'children': []})
    g.j['scenes'][0]['nodes'] = [0, 1]
    times = {}
    for pi, part in enumerate(parts):
        if part['kind'] != 'anim':
            continue
        joints = layers[part['layer']]
        root = joints[part['joint']]
        mi = add_mesh(g, mesh, pi + 1, f'part{pi + 1}', [0.0, 0.0, 0.0], 1.0, cache)
        if mi is None:
            continue
        pw = joints[part['parent']]['world'] if part['parent'] is not None else IDENTITY
        mat = [[pw[r][c] * GLB for c in range(4)] if r < 3 else pw[r] for r in range(4)]
        holder = {'name': f'part{pi + 1}_parent', 'matrix': [ex.f32(mat[r][c]) for c in range(4) for r in range(4)], 'children': [len(g.j['nodes']) + 1]}
        node = {'name': f'part{pi + 1}', 'mesh': mi, 'translation': [ex.f32(x) for x in root['t']], 'rotation': [ex.f32(x) for x in ex.quat(root['r'])],
                'scale': [ex.f32(x) for x in root['s']]}
        g.j['nodes'][1]['children'].append(len(g.j['nodes']))
        g.j['nodes'].append(holder)
        g.j['nodes'].append(node)
        target = len(g.j['nodes']) - 1
        fr = root['frames']
        n = len(fr)
        channels, samplers = [], []
        for path_name, kind, rows in (('translation', 'VEC3', [[ex.f32(x) for x in f[0]] for f in fr]),
                                      ('rotation', 'VEC4', [[ex.f32(x) for x in ex.quat(f[1])] for f in fr]),
                                      ('scale', 'VEC3', [[ex.f32(x) for x in f[2]] for f in fr])):
            if all(row == rows[0] for row in rows):
                rows, key = [rows[0], rows[0]], ('held', n)
                if key not in times:
                    times[key] = g.accessor([0.0, ex.f32((n - 1) / FPS)], 'SCALAR', minmax=True)
            else:
                key = ('all', n)
                if key not in times:
                    times[key] = g.accessor([ex.f32(i / FPS) for i in range(n)], 'SCALAR', minmax=True)
            samplers.append({'input': times[key], 'output': g.accessor(rows, kind), 'interpolation': 'LINEAR'})
            channels.append({'sampler': len(samplers) - 1, 'target': {'node': target, 'path': path_name}})
        g.j['animations'].append({'name': f'part{pi + 1}', 'channels': channels, 'samplers': samplers})
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(g.glb())


def write_part_glb(mesh, part_no, shift, path):
    """A moving yakumono's model, relative to its DObj's frame-0 translation (the surface object moves it)."""
    g, cache = new_gltf()
    mi = add_mesh(g, mesh, part_no, f'part{part_no}', shift, GLB, cache)
    g.j['nodes'].append({'name': f'part{part_no}'} if mi is None else {'name': f'part{part_no}', 'mesh': mi})
    g.j['scenes'][0]['nodes'] = [0]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(g.glb())


# ---------------------------------------------------------------------------------------------- collision

def read_geometry(files, header, layer1):
    """MPGeometryData: yakumono groups -> line sets -> polylines of (x, y, flags)."""
    geo = header['geometry']
    d = files.data(geo[0])
    count = struct.unpack_from('>H', d, geo[1])[0]
    vdata, vids, vlinks, linfo = (files.ptr(geo, 4), files.ptr(geo, 8), files.ptr(geo, 12), files.ptr(geo, 16))
    mcount = struct.unpack_from('>H', d, geo[1] + 20)[0]
    mobjs = files.ptr(geo, 24)
    vd, idd, ld, nd = files.data(vdata[0]), files.data(vids[0]), files.data(vlinks[0]), files.data(linfo[0])

    def vertex(k):
        vid = struct.unpack_from('>H', idd, vids[1] + 2 * k)[0]
        x, y, fl = struct.unpack_from('>hhH', vd, vdata[1] + 6 * vid)
        return x, y, fl

    groups = []
    for g in range(count):
        o = linfo[1] + 18 * g
        yid = struct.unpack_from('>H', nd, o)[0]
        sets = {}
        for k, kind in enumerate(LINE_KINDS):
            first, n = struct.unpack_from('>HH', nd, o + 2 + 4 * k)
            lines = []
            for line in range(first, first + n):
                v1, vn = struct.unpack_from('>HH', ld, vlinks[1] + 4 * line)
                lines.append([vertex(v) for v in range(v1, v1 + vn)])
            sets[kind] = lines
        offset = [0.0, 0.0]
        animated = moving = False
        if yid < len(layer1):
            j = layer1[yid]
            animated = j['anim'] is not None
            moving = j['moving']
            if animated:  # mpCollision*: a yakumono with an animation moves its lines by its DObj's translation
                offset = [j['t'][0], j['t'][1]]
        groups.append(dict(yakumono=yid, lines=sets, offset=offset, animated=animated, moving=moving))
    md = files.data(mobjs[0]) if mobjs else b''
    mapobjs = [struct.unpack_from('>Hhh', md, mobjs[1] + 6 * i) for i in range(mcount)] if mobjs else []
    return groups, mapobjs


def collision(groups, blast, depth=DEPTH, walls=True):
    """Extrude every segment over z in [-depth, depth]; returns (vertices, triangles, pass-through triangle ids,
    ledge triangle ids, line counts). `blast` adds the two depth walls."""
    verts, vindex, tris = [], {}, []
    passing, ledges = [], []

    def vtx(p):
        key = tuple(p)
        if key not in vindex:
            vindex[key] = len(verts)
            verts.append(list(p))
        return vindex[key]

    def quad(a, b, normal_sign_y, normal_sign_x, flags):
        """Segment a -> b (SM64 units) as two triangles facing the line kind's side."""
        (ax, ay), (bx, by) = a, b
        # SM64 normal = (v2 - v1) x (v3 - v2): with v1 = (a, -d), v2 = (b, -d), v3 = (b, +d) it is
        # (b - a) x (0, 0, 2d) = (dy, -dx, 0) * 2d. Flip the winding when that does not face the wanted side.
        nx, ny = (by - ay), -(bx - ax)
        want = nx * normal_sign_x + ny * normal_sign_y
        p = [vtx([ax, ay, -depth]), vtx([bx, by, -depth]), vtx([bx, by, depth]), vtx([ax, ay, depth])]
        if want < 0:
            p = [p[1], p[0], p[3], p[2]]
        start = len(tris)
        tris.append([p[0], p[1], p[2], SURF_DEFAULT, 0])
        tris.append([p[0], p[2], p[3], SURF_DEFAULT, 0])
        if flags & 0x4000:
            passing.extend([start, start + 1])
        if flags & 0x8000:
            ledges.extend([start, start + 1])

    side = {'floor': (1, 0), 'ceil': (-1, 0), 'rwall': (0, 1), 'lwall': (0, -1)}
    counts = {k: 0 for k in LINE_KINDS}
    for g in groups:
        ox, oy = g['offset']
        for kind in LINE_KINDS:
            for line in g['lines'][kind]:
                counts[kind] += 1
                for i in range(len(line) - 1):
                    (x1, y1, f1), (x2, y2, _) = line[i], line[i + 1]
                    a = (rnd((x1 + ox) * K), rnd((y1 + oy) * K))
                    b = (rnd((x2 + ox) * K), rnd((y2 + oy) * K))
                    if a == b:
                        continue
                    quad(a, b, side[kind][0], side[kind][1], f1)
    if walls:
        # the depth walls: one quad each side of the band, over the blast zone, facing inward
        l, r, bo, t = rnd(blast['left'] * K), rnd(blast['right'] * K), rnd(blast['bottom'] * K), rnd(blast['top'] * K)
        for z, sign in ((-depth, 1), (depth, -1)):
            c = [vtx([l, bo, z]), vtx([r, bo, z]), vtx([r, t, z]), vtx([l, t, z])]
            # normal (v2 - v1) x (v3 - v2) = (r - l, 0, 0) x (0, t - bo, 0) = +z: keep for the back wall, flip for the front
            if sign < 0:
                c = [c[1], c[0], c[3], c[2]]
            tris.append([c[0], c[1], c[2], SURF_NO_CAM, 0])
            tris.append([c[0], c[2], c[3], SURF_NO_CAM, 0])
    return verts, tris, passing, ledges, counts


# ---------------------------------------------------------------------------------------------- export

MAPOBJ_PLAYERS = (0, 1, 2, 3)
MAPOBJ_ITEM = 4
MAPOBJ_REBIRTH = 0x20


def floor_below(groups, x, y):
    """Highest floor line point at x (SSB64 units) at or below y + 1, or None."""
    best = None
    for g in groups:
        ox, oy = g['offset']
        for line in g['lines']['floor']:
            for i in range(len(line) - 1):
                (x1, y1, _), (x2, y2, _) = line[i], line[i + 1]
                x1, x2, y1, y2 = x1 + ox, x2 + ox, y1 + oy, y2 + oy
                lo, hi = min(x1, x2), max(x1, x2)
                if lo <= x <= hi and hi > lo:
                    fy = y1 + (y2 - y1) * (x - x1) / (x2 - x1)
                    if fy <= y + 1 and (best is None or fy > best):
                        best = fy
    return best


def export_stage(files, names, stage, out):
    sid, _, label, kind = stage
    fid = names[map_file_name(stage)]
    header = read_header(files, fid)
    layers = [layer_joints(files, layer, li) for li, layer in enumerate(header['layers'])]
    groups, mapobjs = read_geometry(files, header, layers[1])
    moving = [g for g in groups if g['moving']]
    mesh, parts = stage_mesh(files, header, layers, {g['yakumono'] for g in moving})
    course = f'ssb64_{sid}'
    write_stage_glb(mesh, parts, layers, out / 'levels' / course / 'stage.glb')
    blast = header['blast']
    verts, tris, passing, ledges, lines = collision([g for g in groups if not g['moving']], blast)
    # moving yakumono: their lines relative to the DObj, a model relative to its frame-0 translation, and the path
    extra, solids = {}, []
    for g in moving:
        j = layers[1][g['yakumono']]
        part_no = next((pi + 1 for pi, p in enumerate(parts) if p['layer'] == 1 and p['joint'] == g['yakumono']), None)
        key = f'ssb64_yakumono_{g["yakumono"]}'
        gv, gt, _, _, _ = collision([dict(g, offset=[0.0, 0.0])], blast, walls=False)
        extra[key] = {'vertices': gv, 'triangles': gt}
        model = ''
        if part_no is not None:
            model = f'levels/{course}/part_{g["yakumono"]}.glb'
            write_part_glb(mesh, part_no, [j['t'][0], j['t'][1], 0.0], out / model)
        track = [[float(i), rnd(f[0][0] * K), rnd(f[0][1] * K), 0.0] for i, f in enumerate(j['frames'][::2])]
        solids.append({'behavior': 'ssb64Yakumono', 'model': model, 'collision': key, 'pos': track[0][1:], 'track': track})
    pt = lambda x, y: [rnd(x * K), rnd(y * K), 0.0]
    players = [pt(x, y) for k, x, y in mapobjs if k in MAPOBJ_PLAYERS]
    items = [pt(x, y) for k, x, y in mapobjs if k == MAPOBJ_ITEM]
    rebirth = [pt(x, y) for k, x, y in mapobjs if k == MAPOBJ_REBIRTH]
    # spawn: player 1's start dropped onto the floor below it (or the highest floor of the stage)
    first = next(((x, y) for k, x, y in sorted(mapobjs, key=lambda m: m[0]) if k in MAPOBJ_PLAYERS), None)
    if first is None:
        floor_ys = [p[1] + g['offset'][1] for g in groups for line in g['lines']['floor'] for p in line]
        first = (0, max(floor_ys) if floor_ys else 0)
    fy = floor_below(groups, first[0], first[1])
    spawn = [rnd(first[0] * K), rnd((fy if fy is not None else first[1]) * K), 0.0]
    b4 = lambda b: {k: rnd(b[k] * K) for k in ('top', 'bottom', 'left', 'right')}
    doc = {
        'model': f'levels/{course}/stage.glb',
        'vertices': verts,
        'triangles': tris,
        'spawn': {'pos': spawn, 'faceYaw': 32768},
        'horizontalBoundary': rnd(min(abs(blast['left']), abs(blast['right'])) * K),
        'deathHeight': rnd(blast['bottom'] * K),
        'explorationOnly': True,
        'extraCollision': extra,
        'solids': solids,
        'ssb64': {
            'stage': sid, 'name': label, 'kind': kind, 'file': fid, 'unit': K, 'depth': DEPTH, 'bgm': header['bgm'],
            'camera': b4(header['camera']), 'blast': b4(blast), 'warning': rnd(header['alt_warning'] * K),
            'fog': header['fog'], 'players': players, 'items': items, 'rebirth': rebirth,
            'passTriangles': passing, 'ledgeTriangles': ledges, 'lines': lines, 'layers': [len(x) for x in layers],
            'parts': [{'layer': p['layer'], 'joint': p['joint'], 'kind': p['kind'], 'frames': len(layers[p['layer']][p['joint']]['frames'])} for p in parts],
            'yakumono': [{'id': g['yakumono'], 'animated': g['animated'], 'moving': g['moving'],
                          'offset': [rnd(g['offset'][0] * K), rnd(g['offset'][1] * K)]} for g in groups],
        },
    }
    (out / 'courses').mkdir(parents=True, exist_ok=True)
    (out / 'courses' / f'{course}.json').write_text(json.dumps(doc, separators=(',', ':'), sort_keys=True) + '\n')
    return {'id': course, 'name': label, 'kind': kind, 'triangles': sum(len(v) for v in mesh.tris.values()) // 3,
            'materials': len(mesh.materials), 'collision': len(tris), 'bgm': header['bgm'], 'parts': len(parts), 'solids': len(solids),
            'unknown': sorted(x if isinstance(x, str) else hex(x) for x in mesh.unknown)}


def export(rom, out, only=None, log=print):
    files = ex.Files(rom)
    names = ex.file_names()
    index = []
    for stage in STAGES:
        if only and stage[0] not in only:
            continue
        info = export_stage(files, names, stage, out)
        log(f'{info["id"]}: {info["triangles"]} triangles, {info["materials"]} materials, {info["collision"]} collision, '
            f'{info["parts"]} moving parts ({info["solids"]} solid), bgm {info["bgm"]}'
            + (f', unhandled {info["unknown"]}' if info['unknown'] else ''))
        index.append({k: info[k] for k in ('id', 'name', 'kind', 'bgm')})
    (out / 'courses').mkdir(parents=True, exist_ok=True)
    (out / 'courses' / 'ssb64_stages.json').write_text(json.dumps({'game': 'ssb64', 'stages': index}, indent=1, sort_keys=True) + '\n')
    return index


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rom', type=Path, default=ROM)
    ap.add_argument('--out', type=Path, default=OUT)
    ap.add_argument('--stages', default='')
    a = ap.parse_args()
    rom = a.rom.read_bytes()
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise SystemExit(f'{a.rom} is not Super Smash Bros. (US) (sha1 {SHA1})')
    export(rom, a.out, set(a.stages.split(',')) if a.stages else None)


if __name__ == '__main__':
    main()
