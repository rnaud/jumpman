#!/usr/bin/env python3
"""Export the Super Smash Bros. (US) weapon, effect and item models from a local ROM as static glTF (.glb), plus the
item sounds.

Formats follow VetriTheRetri/ssb-decomp-re (reference/ssb64, see native/SSB64.md "Weapons, effects and items"):
  * weapons: WPAttributes (src/wp/wptypes.h) `{data, p_mobjsubs, anim_joints, p_matanim_joints, ...}` in the fighter's
    Main / Special1 file; `data` is a DObjDesc tree or one display list (wpManagerMakeWeapon, src/wp/wpmanager.c).
  * items: ITAttributes (src/it/ittypes.h), same four pointers first, in ITCommonData (file 251) for the common items
    (the `@ 0x....` offsets of src/relocData/251_ITCommonData.c), LinkMain / NessSpecial1 for Link's bomb and PK Fire;
    built by itManagerMakeItem (src/it/itmanager.c) with gcSetupCustomDObjsWithMObj (src/sys/objanim.c).
  * effects: EFDesc (src/ef/eftypes.h) file + offsets of the DObjDesc tree (or display list), MObjSub ***,
    AnimJoint and MatAnimJoint tables (efManagerMakeEffect, src/ef/efmanager.c); the offsets below are the ll* symbols
    of the src/relocData/<file>.c layouts.
  * display lists, materials and textures are drawn with export-ssb64.py's F3DEX2 / MObj / combiner model (`Mesh`); a
    joint's MObjSub list and its material script at frame 0 (AObjEvent32, as the fighters' costume 0) set the textures.
Joint animations (AnimJoint scripts) are not exported: each model is its bind pose, baked into one mesh (no skin).
The item sounds (a short list of gmFGMVoiceID names) are rendered with export-ssb64-sounds.py's FGM engine.
Standard library only, deterministic; native/byor-rs/src/ssb64/weapons.rs is its byte-identical port.

Writes public/models/ssb64/objects/<name>.glb, public/models/ssb64/objects.json and
assets/audio/voices/ssb64/<Name>_0.wav + manifest-ssb64-items.json.
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
OUT = ROOT / 'public/models/ssb64'
SOUND_OUT = ROOT / 'assets/audio/voices/ssb64'


def _load(stem):
    spec = importlib.util.spec_from_file_location(stem.replace('-', '_'), Path(__file__).resolve().parent / f'{stem}.py')
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


ex = _load('export-ssb64')
snd = _load('export-ssb64-sounds')

UNIT = ex.UNIT
DOBJ_END = 18  # DOBJ_ARRAY_MAX: the DObjDesc terminator id

# What a model is made of: 'attr' = a WPAttributes / ITAttributes struct (file, offset, tree): the four pointers are read
# from it; 'ef' = an EFDesc: file, root (DObjDesc tree when `tree`, else a display list), MObjSub ***, MatAnimJoint ***.
# `palette`: the root MObj's palette_id (wpMarioFireballMakeWeapon gives Luigi's fireball palette 1, the green shell
# uses 1 and the red one 0). `billboard`: the game draws it facing the camera (matrix kinds 0x47 / 0x4F / 0x50 in its
# WPDesc / EFDesc transform types). `env0`: the renderer clears the environment colour first (the items' colour-animation
# renderer itDisplayColAnimOPA, wpYoshiEggThrowProcDisplay).
OBJECTS = [
    # weapons (src/wp/wp<fighter>/*.c WPDesc, flags & 1 = DObjDesc tree)
    ('fireball', 'weapon', ('attr', 'MarioSpecial1', 0x00, False), {'billboard': True}),
    ('fireball_luigi', 'weapon', ('attr', 'LuigiSpecial1', 0x00, False), {'billboard': True, 'palette': 1}),
    ('blaster', 'weapon', ('attr', 'FoxSpecial1', 0x00, False), {}),
    ('charge_shot', 'weapon', ('attr', 'SamusSpecial1', 0x00, False), {'billboard': True}),
    ('samus_bomb', 'weapon', ('attr', 'SamusMain', 0x0C, False), {}),
    ('boomerang', 'weapon', ('attr', 'LinkSpecial1', 0x00, True), {}),
    ('spin_attack', 'weapon', ('attr', 'LinkMain', 0x0C, True), {'solid': True}),
    ('final_cutter', 'weapon', ('attr', 'KirbyMain', 0x08, True), {}),
    ('pk_fire', 'weapon', ('attr', 'NessSpecial1', 0x00, False), {'billboard': True}),
    ('pk_thunder', 'weapon', ('attr', 'NessMain', 0x0C, True), {'billboard': True}),
    ('pk_thunder_trail', 'weapon', ('attr', 'NessMain', 0x40, False), {'billboard': True}),
    ('thunder_head', 'weapon', ('attr', 'PikachuMain', 0x0C, False), {'billboard': True}),
    ('thunder_jolt_air', 'weapon', ('attr', 'PikachuSpecial1', 0x00, False), {'billboard': True}),
    ('thunder_jolt', 'weapon', ('attr', 'PikachuSpecial1', 0x34, True), {}),
    ('yoshi_egg', 'weapon', ('attr', 'YoshiMain', 0x0C, False), {'env0': True}),
    ('yoshi_star', 'weapon', ('attr', 'YoshiMain', 0x40, False), {}),
    ('ray_gun_shot', 'weapon', ('attr', 'ITCommonData', 0x2B0, False), {}),
    # items (ITAttributes; ITCommonData offsets from src/relocData/251_ITCommonData.c)
    ('capsule', 'item', ('attr', 'ITCommonData', 0x050, True), {}),
    ('maxim_tomato', 'item', ('attr', 'ITCommonData', 0x0B8, True), {}),
    ('heart', 'item', ('attr', 'ITCommonData', 0x100, True), {}),
    ('star', 'item', ('attr', 'ITCommonData', 0x148, True), {}),
    ('beam_sword', 'item', ('attr', 'ITCommonData', 0x190, True), {}),
    ('bat', 'item', ('attr', 'ITCommonData', 0x1D8, True), {}),
    ('fan', 'item', ('attr', 'ITCommonData', 0x220, True), {}),
    ('ray_gun', 'item', ('attr', 'ITCommonData', 0x268, True), {}),
    ('fire_flower', 'item', ('attr', 'ITCommonData', 0x2E4, True), {}),
    ('hammer', 'item', ('attr', 'ITCommonData', 0x374, True), {'env0': True}),
    ('motion_bomb', 'item', ('attr', 'ITCommonData', 0x3BC, True), {}),
    ('bob_omb', 'item', ('attr', 'ITCommonData', 0x424, True), {'env0': True}),
    ('star_rod', 'item', ('attr', 'ITCommonData', 0x48C, True), {}),
    ('green_shell', 'item', ('attr', 'ITCommonData', 0x53C, True), {'palette': 1}),
    ('red_shell', 'item', ('attr', 'ITCommonData', 0x584, True), {'palette': 0}),
    ('crate', 'item', ('attr', 'ITCommonData', 0x5CC, True), {}),
    ('barrel', 'item', ('attr', 'ITCommonData', 0x634, True), {}),
    ('bumper', 'item', ('attr', 'ITCommonData', 0x69C, True), {}),
    ('poke_ball', 'item', ('attr', 'ITCommonData', 0x6E4, True), {}),
    ('egg', 'item', ('attr', 'ITCommonData', 0xACC, True), {}),
    ('link_bomb', 'item', ('attr', 'LinkMain', 0x40, True), {'env0': True}),
    ('pk_fire_pillar', 'item', ('attr', 'NessSpecial1', 0x34, True), {}),
    # effects (EFDesc, src/ef/efmanager.c)
    ('falcon_punch', 'effect', ('ef', 'CaptainSpecial3', 0x760, False, 0x690, 0x830), {}),
    ('falcon_kick', 'effect', ('ef', 'CaptainSpecial2', 0xB08, True, 0x960, 0xC00), {}),
    ('reflector', 'effect', ('ef', 'FoxSpecial2', 0x2B0, True, 0, 0), {'billboard': True}),
    ('psi_magnet', 'effect', ('ef', 'NessSpecial2', 0x9A8, True, 0x810, 0xAD0), {'billboard': True}),
    ('sing', 'effect', ('ef', 'PurinSpecial2', 0x2130, True, 0x1C20, 0x2D70), {'billboard': True, 'solid': True}),
    ('spin_attack_trail', 'effect', ('ef', 'LinkSpecial2', 0x11C0, True, 0x1038, 0x12F0), {'solid': True}),
    ('thunder_shock', 'effect', ('ef', 'PikachuSpecial2', 0x1640, True, 0x13A0, 0x1A80), {'billboard': True}),
    ('egg_lay', 'effect', ('ef', 'YoshiSpecial3', 0x960, True, 0, 0), {}),
    ('hit_slash', 'effect', ('ef', 'EFCommonEffects1', 0x7750, True, 0x73E0, 0x7860), {'billboard': True}),
]

# Item sounds (gmFGMVoiceID names, src/gm/gmsound.h; which item plays which: src/it/itcommon/*.c).
ITEM_FGM = [
    'FGMItemGet', 'FGMItemThrow', 'FGMItemMapCollide', 'FGMItemSpawn1', 'FGMContainerSmash', 'FGMLightSwingS',
    'FGMLightSwingM', 'FGMLightSwingL', 'FGMHeavySwing1', 'FGMSwordSwing1', 'FGMSwordSwing3', 'FGMSwordSwing4',
    'FGMBatHit', 'FGMHarisenHit', 'FGMHammerSwing', 'FGMLGunShoot', 'FGMLGunEmpty', 'FGMFireFlowerShoot', 'FGMBurnE',
    'FGMStarRodSwing1', 'FGMStarRodSwing3', 'FGMStarRodSwing4', 'FGMStarRodEmpty', 'FGMStarGet', 'FGMStarMapCollide',
    'FGMBombHeiFuse', 'FGMBombHeiWalkStart', 'FGMExplodeL', 'FGMShellHit', 'FGMMSBombAttach', 'FGMBumperHit',
    'FGMMBallOpen', 'FGMPlayerHeal', 'FGMPunchL',
]


def dl_parts(files, dl):
    """A DObj's `dl`: one display list, or a DObjDLLink array `{s32 list_id; Gfx *dl}` ending with list id 4 (the
    XLU / DLLinks renderers): the display lists to draw, in order."""
    if dl is None:
        return []
    d = files.data(dl[0])
    first = struct.unpack_from('>I', d, dl[1])[0]
    if first < 4 and files.ptr(dl, 4) is not None:
        out, k = [], 0
        while struct.unpack_from('>I', d, dl[1] + 8 * k)[0] < 4:
            p = files.ptr(dl, 8 * k + 4)
            if p:
                out.append(p)
            k += 1
        return out
    return [dl]


def tree(files, root, is_tree):
    """gcSetupCustomDObjs: the DObjDesc list (`id & 0xFFF` = depth, parent = the last DObj one level up) or one DObj."""
    if not is_tree:
        return [dict(dl=root, world=ex.mat_trs([0.0] * 3, [0.0] * 3, [1.0] * 3))]
    d = files.data(root[0])
    joints, stack = [], {}
    i = 0
    while True:
        o = root[1] + 44 * i
        jid = struct.unpack_from('>i', d, o)[0]
        if jid == DOBJ_END:
            break
        depth = jid & 0xFFF
        vals = struct.unpack_from('>9f', d, o + 8)
        local = ex.mat_trs(list(vals[0:3]), list(vals[3:6]), list(vals[6:9]))
        parent = stack.get(depth - 1) if depth else None
        world = ex.mat_mul(joints[parent]['world'], local) if parent is not None else local
        stack[depth] = len(joints)
        joints.append(dict(dl=files.ptr(root, 44 * i + 4), world=world))
        i += 1
    return joints


def model(files, names, spec, opts):
    """The Mesh of one object: every DObj's MObjs (frame 0 of its material script) and display lists."""
    if spec[0] == 'attr':
        _, fname, off, is_tree = spec
        ref = (names[fname], off)
        root, mobs, mats = files.ptr(ref, 0), files.ptr(ref, 4), files.ptr(ref, 12)
    else:
        _, fname, root_off, is_tree, mob_off, mat_off = spec
        fid = names[fname]
        root = (fid, root_off)
        mobs = (fid, mob_off) if mob_off else None
        mats = (fid, mat_off) if mat_off else None
    joints = tree(files, root, is_tree) if root else []
    m = ex.Mesh(files)
    if opts.get('env0'):
        m.env = [0.0, 0.0, 0.0, 0.0]
    palette = opts.get('palette')
    for ji, j in enumerate(joints):
        mlist = files.ptr(mobs, 4 * ji) if mobs else None
        clist = files.ptr(mats, 4 * ji) if mats else None
        m.mobjs = ex.read_mobjs(files, mlist)
        for k, mo in enumerate(m.mobjs):
            ex.costume(files, mo, files.ptr(clist, 4 * k) if clist else None)
            if opts.get('solid'):
                # the material script fades it in from alpha 0: export it at full alpha
                mo['prim'] = mo['prim'][:3] + [1.0]
                mo['env'] = mo['env'][:3] + [1.0]
            if palette is not None and mo['palettes']:
                # the first MObj with a palette list (the code sets DObjGetStruct(gobj)->mobj->palette_id)
                mo['palette_id'] = palette
                mo['palette'] = files.ptr(mo['palettes'], 4 * palette)
                palette = None
        for part in dl_parts(files, j['dl']):
            m.run(part, ji, j['world'])
    return m, len(joints)


def shade_alpha(mode, two_cycle):
    """True if the combiner's alpha reads the shade (vertex) alpha. Otherwise the exported vertex colours keep alpha 1:
    the sprites' vertices often carry alpha 0 that the RDP never reads."""
    w0, w1 = mode
    cycles = [((w0 >> 12) & 7, (w1 >> 12) & 7, (w0 >> 9) & 7, (w1 >> 9) & 7), ((w1 >> 21) & 7, (w1 >> 3) & 7, (w1 >> 18) & 7, w1 & 7)]
    return any(4 in c for c in cycles[:2 if two_cycle else 1])


def glb(mesh, name):
    """One node, one mesh (a primitive per material), positions in exported units (UNIT per SSB64 unit)."""
    g = ex.Gltf()
    g.j['asset']['generator'] = 'jumpman export-ssb64-weapons.py'
    g.j['nodes'].append({'name': name, 'mesh': 0})
    g.j['scenes'][0]['nodes'] = [0]
    prims = []
    image_index = {}
    lo, hi = [1e30] * 3, [-1e30] * 3
    for mi, key in enumerate(mesh.tris):
        idx = mesh.tris[key]
        mat = mesh.materials[key]
        vids = sorted(set(idx))
        remap = {v: i for i, v in enumerate(vids)}
        keep_alpha = shade_alpha(key[0], key[1])
        pos, nrm, uv, col = [], [], [], []
        for v in vids:
            _, p, n, t, c = mesh.verts[v]
            pp = [ex.f32(x * UNIT) for x in p]
            for k in range(3):
                lo[k] = min(lo[k], pp[k])
                hi[k] = max(hi[k], pp[k])
            pos.append(pp)
            if n is None:
                n = [0.0, 1.0, 0.0]
            ln = math.sqrt(sum(x * x for x in n)) or 1.0
            nrm.append([ex.f32(x / ln) for x in n])
            uv.append([ex.f32(x) for x in t])
            c = c or [1.0, 1.0, 1.0, 1.0]
            col.append([ex.f32(x) for x in (c if keep_alpha else c[:3] + [1.0])])
        attrs = {'POSITION': g.accessor(pos, 'VEC3', target=34962, minmax=True), 'NORMAL': g.accessor(nrm, 'VEC3', target=34962)}
        if 'image' in mat:
            attrs['TEXCOORD_0'] = g.accessor(uv, 'VEC2', target=34962)
        if mat['vcol']:
            attrs['COLOR_0'] = g.accessor(col, 'VEC4', target=34962)
        indices = g.accessor([remap[i] for i in idx], 'SCALAR', 5123 if len(vids) < 65536 else 5125, 34963)
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
    if not prims:
        lo, hi = [0.0] * 3, [0.0] * 3
    return g.glb(), lo, hi


def plane(lo, hi):
    """The axis a flat model (a sprite quad) has no extent along: 'x', 'y', 'z', or None for a solid."""
    ext = [hi[k] - lo[k] for k in range(3)]
    big = max(ext)
    for k in range(3):
        if big > 0.0 and ext[k] <= 0.01 * big:
            return 'xyz'[k]
    return None


def export_models(files, names, out, log=print):
    objs = out / 'objects'
    objs.mkdir(parents=True, exist_ok=True)
    index = {'game': 'ssb64', 'unit': UNIT, 'objects': {}}
    for name, kind, spec, opts in OBJECTS:
        mesh, joints = model(files, names, spec, opts)
        data, lo, hi = glb(mesh, name)
        (objs / f'{name}.glb').write_bytes(data)
        tris = sum(len(v) for v in mesh.tris.values()) // 3
        index['objects'][name] = {'kind': kind, 'file': f'objects/{name}.glb', 'source': spec[1], 'joints': joints,
                                  'triangles': tris, 'materials': len(mesh.materials), 'billboard': bool(opts.get('billboard')),
                                  'plane': plane(lo, hi), 'min': lo, 'max': hi}
        if mesh.unknown:
            log(f'{name}: unhandled display list opcodes {sorted(x if isinstance(x, str) else hex(x) for x in mesh.unknown)}')
    # FTAttributes.joint_itemlight_id / joint_itemheavy_id (src/ft/fttypes.h, +0x33C / +0x334): the joints that hold items
    tables = ex.fighter_tables(names)
    index['hands'] = {}
    for f in ex.FIGHTERS[:12]:
        d, a = files.data(tables[f]['main']), tables[f]['attr']
        index['hands'][f.lower()] = {'light': struct.unpack_from('>i', d, a + 0x33C)[0], 'heavy': struct.unpack_from('>i', d, a + 0x334)[0]}
    (out / 'objects.json').write_text(json.dumps(index, indent=1, sort_keys=True) + '\n')
    log(f'ssb64: exported {len(OBJECTS)} weapon / effect / item models')
    return index


def export_sounds(rom, out, log=print):
    names = snd.voice_names()
    ids = {n: i for i, n in enumerate(names)}
    engine = snd.Engine(snd.package(rom[snd.FGM_UCD[0]:snd.FGM_UCD[1]]), snd.package(rom[snd.FGM_TBL[0]:snd.FGM_TBL[1]]),
                        snd.lfos(rom[snd.FGM_UNK[0]:snd.FGM_UNK[1]]), snd.Bank(rom))
    out.mkdir(parents=True, exist_ok=True)
    manifest = {}
    for name in ITEM_FGM:
        pcm, cut = engine.render(ids[name])
        if not pcm:
            continue
        (out / f'{name}_0.wav').write_bytes(snd.wav_bytes(pcm, snd.RATE))
        manifest[name] = {'id': ids[name], 'frames': len(pcm), 'duration': round(len(pcm) / snd.RATE, 4), 'peak': max(abs(x) for x in pcm), 'cut': cut}
    (out / 'manifest-ssb64-items.json').write_text(json.dumps({'sampleRate': snd.RATE, 'clips': manifest}, indent=1, sort_keys=True) + '\n')
    log(f'ssb64: exported {len(manifest)} item sounds')
    return manifest


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rom', type=Path, default=ROM)
    ap.add_argument('--out', type=Path, default=OUT)
    ap.add_argument('--sound-out', type=Path, default=SOUND_OUT)
    a = ap.parse_args()
    rom = a.rom.read_bytes()
    if hashlib.sha1(rom).hexdigest() != ex.SHA1:
        raise SystemExit(f'{a.rom} is not Super Smash Bros. (US) (sha1 {ex.SHA1})')
    export_models(ex.Files(rom), ex.file_names(), a.out)
    export_sounds(rom, a.sound_out)


if __name__ == '__main__':
    main()
