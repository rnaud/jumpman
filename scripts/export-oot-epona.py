"""Export Epona (object_horse) and Link's riding animations from a local OoT US 1.2 ROM using zeldaret/oot asset definitions.

Writes public/models/oot/epona.json:

* ``limbs``     47 skeleton limbs (`gEponaSkel`, `pos` / `child` / `sibling`, same layout as link.json).
* ``textures``  RGBA8 textures, base64, keyed by the decomp's name (the eye is `gEponaEyeOpenTex`, TLUT resolved).
* ``batches``   draw batches (texture / prim colour / wrap) of skinned vertices.  A vertex is
                `[s, t, nx, ny, nz, normalLimb, n, (limb, x, y, z, weight) * n]`: the body is a `SKIN_LIMB_TYPE_ANIMATED`
                limb whose vertices are the weighted sum of up to a few limb matrices applied to per-limb local positions
                (`Skin_ApplyLimbModifications` in z_skin.c); every other limb is a rigid display list
                (`SKIN_LIMB_TYPE_NORMAL`, n = 1, weight = 100).
* ``clips``     Epona's nine animations (`gEponaIdleAnim` ...) as frames of `[tx, ty, tz, rot * 3 * 47]` (s16).
* ``riderClips`` Link's `gPlayerAnim_link_uma_*` clips in the layout of link.json clips (22 Vec3s per frame), played on
                the adult Link rig while he sits on the saddle.
"""
import argparse
import base64
import csv
import hashlib
import importlib.util
import json
import struct
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('export_oot_link', HERE / 'export-oot-link.py')
link = importlib.util.module_from_spec(spec)
spec.loader.exec_module(link)

LIMB_COUNT = 47
EYE_SEGMENT_8 = 'gEponaEyeOpenTex'   # EnHorse_Draw sets segment 8 to the eye texture (open / half / closed)

CLIPS = dict(idle='gEponaIdleAnim', whinny='gEponaWhinnyAnim', refuse='gEponaRefuseAnim', rearing='gEponaRearingAnim',
             walk='gEponaWalkingAnim', trot='gEponaTrottingAnim', gallop='gEponaGallopingAnim',
             jump='gEponaJumpingAnim', jumpHigh='gEponaJumpingHighAnim')

# Link on horseback (z_player.c D_80854944 / D_80854968 / D_8085498C and the mount / dismount animations).
RIDER = dict(stop='uma_anim_stop', stand='uma_anim_stand', walk='uma_anim_walk', slowrun='uma_anim_slowrun',
             fastrun='uma_anim_fastrun', jump100='uma_anim_jump100', jump200='uma_anim_jump200',
             walkMuti='uma_anim_walk_muti', slowrunMuti='uma_anim_slowrun_muti', fastrunMuti='uma_anim_fastrun_muti',
             stopMuti='uma_stop_muti', walk2='uma_anim_walk2', wait1='uma_wait_1', wait2='uma_wait_2', wait3='uma_wait_3',
             mountLeft='uma_left_up', mountRight='uma_right_up', dismountLeft='uma_left_down',
             dismountRight='uma_right_down')


def u16(m, p):
    return struct.unpack_from('>H', m, p)[0]


def s16(m, p):
    return struct.unpack_from('>h', m, p)[0]


def skeleton(m, objects):
    base = struct.unpack_from('>I', m, link.offset(objects['gEponaSkel']))[0] & 0xffffff
    limbs = []
    for i in range(LIMB_COUNT):
        p = struct.unpack_from('>I', m, base + 4 * i)[0] & 0xffffff
        x, y, z, child, sibling, kind, seg = struct.unpack_from('>hhhBBiI', m, p)
        limbs.append(dict(pos=[x, y, z], child=child, sibling=sibling, kind=kind, seg=seg & 0xffffff))
    return limbs


def animation(m, objects, name):
    p = link.offset(objects[name])
    frames = s16(m, p)
    data = struct.unpack_from('>I', m, p + 4)[0] & 0xffffff
    joints = struct.unpack_from('>I', m, p + 8)[0] & 0xffffff
    static_max = u16(m, p + 12)
    out = []
    for f in range(frames):
        row = []
        for j in range(LIMB_COUNT + 1):
            for axis in range(3):
                idx = u16(m, joints + 6 * j + 2 * axis)
                row.append(s16(m, data + 2 * (idx if idx < static_max else idx + f)))
        out.append(row)
    return out


def skin_vertices(m, limb):
    """Per buffer index of the animated limb: (s, t, normal, normal limb, [(limb, x, y, z, weight)])."""
    total, count, modifs = struct.unpack_from('>HHI', m, limb['seg'])
    modifs &= 0xffffff
    out = {}
    for i in range(count):
        vtx_count, trans_count, hub, sv, lt = struct.unpack_from('>HHHxxII', m, modifs + 16 * i)
        sv &= 0xffffff
        lt &= 0xffffff
        trans = [struct.unpack_from('>BxhhhB', m, lt + 10 * k) for k in range(trans_count)]
        # `Skin_ApplyLimbModifications`: one transform -> that point at full weight; otherwise a weighted sum.
        if trans_count == 1:
            pts = [(trans[0][0], trans[0][1], trans[0][2], trans[0][3], 100)]
        else:
            pts = [(t[0], t[1], t[2], t[3], t[4]) for t in trans]
        for k in range(vtx_count):
            index, s, t, nx, ny, nz, alpha = struct.unpack_from('>HhhbbbB', m, sv + 10 * k)
            out[index] = (s, t, (nx, ny, nz), trans[hub][0], pts)
    return total, out


def build(m, objects, rom_motion, animations):
    limbs = skeleton(m, objects)
    specs = {0x06000000 + link.offset(v): v for v in objects.values() if 'Format' in v and 'Width' in v}
    by_name = {v['Name']: v for v in objects.values() if v.get('Name')}
    specs[0x08000000] = by_name[EYE_SEGMENT_8]
    textures = {}
    batches = {}
    animated = [i for i, l in enumerate(limbs) if l['kind'] == 4]
    assert len(animated) == 1, animated
    skin_total, skin = skin_vertices(m, limbs[animated[0]])
    skin_limb = animated[0]
    dl_addrs = []
    for i, l in enumerate(limbs):
        if l['kind'] == 11:
            dl_addrs.append((i, l['seg'], False))
        elif l['kind'] == 4:
            dl_addrs.append((i, struct.unpack_from('>I', m, l['seg'] + 8)[0] & 0xffffff, True))

    def tex_of(addr):
        s = specs.get(addr)
        if s is None:
            raise KeyError(hex(addr))
        return s

    for limb, dl, is_skin in dl_addrs:
        state = dict(tex=None, color=0xffffff, ws=0, wt=0, cache={})

        def run(p, depth=0):
            while True:
                a, b = struct.unpack_from('>II', m, p)
                p += 8
                op = a >> 24
                if op == 0xdf:
                    return
                if op == 0xde:
                    run(b & 0xffffff, depth + 1)
                    if (a >> 16) & 0xff:
                        return
                elif op == 0xfd:
                    s = tex_of(b)
                    if 'tlut' not in s['Name'].lower():
                        state['tex'] = s['Name']
                        if s['Name'] not in textures:
                            textures[s['Name']] = link.texture(m, s)
                elif op == 0xf5 and b >> 24 == 0:
                    state['ws'], state['wt'] = (b >> 8) & 3, (b >> 18) & 3
                elif op == 0xfa:
                    state['color'] = b >> 8
                elif op == 1:
                    n = (a >> 12) & 255
                    first = ((a & 255) >> 1) - n
                    seg = b >> 24
                    addr = b & 0xffffff
                    for k in range(n):
                        if seg == 8:
                            idx = addr // 16 + k
                            s, t, nrm, nlimb, pts = skin[idx]
                            rec = [s, t, nrm[0], nrm[1], nrm[2], nlimb, len(pts)]
                            for pt in pts:
                                rec.extend(pt)
                        else:
                            x, y, z, _f, s, t, nx, ny, nz, _a = struct.unpack_from('>hhhHhhbbbB', m, addr + 16 * k)
                            rec = [s, t, nx, ny, nz, limb, 1, limb, x, y, z, 100]
                        state['cache'][first + k] = rec
                elif op in (5, 6, 7):
                    batch = batches.setdefault((state['tex'], state['color'], state['ws'], state['wt']), [])
                    words = [a] if op == 5 else [a, b]
                    for word in words:
                        batch.extend(state['cache'][(word >> sh & 255) // 2] for sh in (16, 8, 0))

        run(dl)

    clips = {k: animation(m, objects, v) for k, v in CLIPS.items()}
    rider = {}
    for key, suffix in RIDER.items():
        entry = animations['gPlayerAnim_link_' + suffix + '_Data']
        rider[key] = [struct.unpack_from('>66h', rom_motion, link.offset(entry) + i * 134)
                      for i in range(int(entry['FrameCount']))]
    return dict(
        limbs=[dict(pos=l['pos'], child=l['child'], sibling=l['sibling']) for l in limbs],
        skinLimb=skin_limb,
        textures=textures,
        clips=clips,
        riderClips=rider,
        batches=[dict(texture=tex, color=color, wrap=[ws, wt], vertices=verts)
                 for (tex, color, ws, wt), verts in batches.items()])


def extract(rom_path, source):
    rom = rom_path.read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError("Expected a big-endian OoT US 1.2 ROM")
    with (source / 'baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    model = link.segment(rom, names, 'object_horse')
    motion = link.segment(rom, names, 'link_animetion')
    xml = source / 'assets/xml'
    objects = link.definitions(xml / 'objects/object_horse.xml')
    animations = link.definitions(xml / 'misc/link_animetion.xml')
    return build(model, objects, motion, animations)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    root = HERE.parents[0]
    parser.add_argument('rom', nargs='?', type=Path, default=root / 'reference/roms/oot-us-1.2.z64')
    parser.add_argument('decomp', nargs='?', type=Path, default=root / 'reference/oot')
    parser.add_argument('--out', type=Path, default=root / 'public/models/oot/epona.json')
    args = parser.parse_args()
    asset = extract(args.rom, args.decomp)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(asset, separators=(',', ':')))
    tris = sum(len(b['vertices']) // 3 for b in asset['batches'])
    print(f'Exported Epona: {len(asset["textures"])} textures, {len(asset["clips"])} clips, '
          f'{len(asset["riderClips"])} rider clips, {tris} triangles to {args.out}')
