"""Export the Banjo-Kazooie enemies from a local US 1.0 ROM as rigged models + clips.

Same pipeline as export-bk-banjo.py (rarezip asset table, F3DEX display lists behind the geo layout,
bone-tagged vertices, Rare animation files) for each actor in SPECIES. The animation table of every
enemy is read straight from the decomp: each `ActorAnimationInfo name[] = { {ASSET_..., seconds}, ... }`
array is indexed by the actor's state number, so the JSON carries `states[state] = [clip, seconds]` and the
engine (native/sim/src/bk_enemy.rs, native/src/bk_enemies/) can ask for "the clip of state 3" exactly like the
original `subaddie_set_state`.

Output: public/models/bk/enemies/<key>.json
    {transScale, bones, textures, batches, clips{name: clip}, states[[clip, seconds]|null]}
which `native/src/bk_enemies/` skins on the CPU with the same Catmull-Rom sampler as Banjo.
"""
import argparse
import base64
import hashlib
import json
import re
import runpy
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
bk_banjo = runpy.run_path(str(ROOT / 'scripts/export-bk-banjo.py'))
bk_levels = runpy.run_path(str(ROOT / 'scripts/export-bk-levels.py'))
assets, build_clip, enum_ids = bk_banjo['assets'], bk_banjo['build_clip'], bk_banjo['enum_ids']
decode_texture = bk_levels['decode_texture']

# key -> model asset enum, source file holding the ActorAnimationInfo array, array name, appendage selectors.
# `selectors` maps a geo "selector" index (0x0C command) to the branch shown (1-based, 0 = hidden); the
# decomp sets these with modelRender_setAppendageVisibility. Everything unlisted shows its first branch.
SPECIES = {
    'grublin': dict(model='ASSET_3C5_MODEL_GRUBLIN', src='src/MM/ch/grublin.c', table='chGrublinAnimations'),
    'grublin_hood': dict(model='ASSET_52C_MODEL_GRUBLIN_HOOD', src='src/CCW/ch/grublinhood.c', table='chGrublinHood_animations'),
    'limbo': dict(model='ASSET_4CC_MODEL_LIMBO', src='src/core2/ch/skeleton.c', table='chSkeletonAnimations'),
    'termite': dict(model='ASSET_350_MODEL_TERMITE', src='src/core2/ch/termite.c', table='chTermiteAnimations'),
    'bigbutt': dict(model='ASSET_353_MODEL_BIGBUTT', src='src/core2/ch/bigbutt.c', table='chBullAnimations'),
    'snippet': dict(model='ASSET_358_SNIPPET', src='src/core2/ch/crab.c', table='chCrabAnimations'),
    'mutie_snippet': dict(model='ASSET_38F_MODEL_MUTIE_SNIPPET', src='src/core2/ch/crab.c', table='chCrabMutantAnimations'),
    'sir_slush': dict(model='ASSET_377_MODEL_SIR_SLUSH', src='src/core2/ch/snowman.c', table='chSnowmanAnimations',
                      selectors={1: 0, 2: 1}),
    'bat': dict(model='ASSET_3CA_MODEL_BAT', src='src/core2/ch/bat.c', table='sChBatAnimations'),
    'shrapnel': dict(model='ASSET_3EC_MODEL_SHRAPNEL', src='src/core2/ch/shrapnel.c', table='chShrapnelAnimations'),
    'snarebear': dict(model='ASSET_440_MODEL_SNAREBEAR', src='src/CCW/ch/snarebear.c', table='chSnarebearAnimations'),
    'beehive': dict(model='ASSET_364_MODEL_BEEHIVE', src='src/core2/ch/beehive.c', table='chBeehiveAnimations'),
}


def read_table(decomp, spec, ids):
    """[(asset id, seconds) | None] indexed by state, from the decomp's ActorAnimationInfo array."""
    text = (decomp / spec['src']).read_text()
    m = re.search(r'ActorAnimationInfo\s+' + spec['table'] + r'\s*(?:\[[^\]]*\])?\s*=\s*\{(.*?)\};', text, re.S)
    if not m:
        raise ValueError(f"{spec['src']}: {spec['table']} not found")
    rows = []
    for asset, seconds in re.findall(r'\{\s*([A-Za-z0-9_]+)\s*,\s*([0-9.eE+\-]+)f?\s*\}', m.group(1)):
        if asset in ids:
            rows.append((ids[asset], float(seconds), asset))
        else:
            rows.append(None)
    return rows


def build_model(m, selectors):
    """export-bk-banjo.build_model with wider texture formats and per-species appendage selectors."""
    (_, geo_off, tex_list, _, gfx, vtx, _, anim_list, *_rest) = struct.unpack_from('>IiHHiiiiiiiiiHHf', m, 0)
    bones = []
    trans_scale = 1.0
    if anim_list:
        trans_scale, bone_count = struct.unpack_from('>fh', m, anim_list)
        for i in range(bone_count):
            x, y, z, bone_id, parent = struct.unpack_from('>fffhh', m, anim_list + 8 + 16 * i)
            bones.append(dict(pivot=[round(x, 3), round(y, 3), round(z, 3)], id=bone_id, parent=parent))
    if not bones:
        bones = [dict(pivot=[0.0, 0.0, 0.0], id=0, parent=-1)]
    _, tex_count = struct.unpack_from('>ih', m, tex_list)
    tex_data = tex_list + 8 + 16 * tex_count
    vertices = vtx + 0x18
    commands = gfx + 8
    textures = {}

    def texture(image, tlut, fmt, siz, width, height):
        key = f'{image:x}_{fmt}{siz}_{width}x{height}'
        if key in textures:
            return key
        try:
            rgba = decode_texture(m, tex_data, image, tlut, fmt, siz, width, height)
        except Exception:
            return None
        textures[key] = dict(width=width, height=height, rgba=base64.b64encode(rgba).decode())
        return key

    batches = {}
    cache = [None] * 64
    state = dict(tex_on=False, scale=(1.0, 1.0), timg=0, tlut=0, fmt=0, siz=0, wrap=(0, 0), width=32, height=32, texgen=False)

    def run_dl(index, bone):
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
                tex = None
                if state['tex_on'] and not state['texgen']:
                    tex = texture(state['timg'], state['tlut'], state['fmt'], state['siz'], state['width'], state['height'])
                key = (tex, state['wrap'])
                out = batches.setdefault(key, [])
                for word in words:
                    for shift in (16, 8, 0):
                        c = cache[(word >> shift & 0xFF) // 2]
                        if c is None:
                            continue
                        x, y, z, s, t, r, g, bl, al, bn = c
                        u = v = 0
                        if tex:
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

    seen = set()

    def geo(p, bone):
        while True:
            cmd, nxt = struct.unpack_from('>Ii', m, p)
            if (p, bone) in seen and cmd in (0x02, 0x05, 0x06, 0x0C):
                pass
            if cmd == 0x02:
                branch, mtx = struct.unpack_from('>Bb', m, p + 8)
                if branch:
                    geo(p + branch, mtx)
            elif cmd == 0x03:
                run_dl(struct.unpack_from('>h', m, p + 8)[0], bone)
            elif cmd == 0x07:
                run_dl(struct.unpack_from('>h', m, p + 10)[0], bone)
            elif cmd == 0x05:
                q = p + 8
                first = True
                while True:
                    index, = struct.unpack_from('>h', m, q)
                    q += 2
                    if index == 0 and not first:
                        break
                    first = False
                    run_dl(index, bone)
            elif cmd == 0x06:
                geo(p + struct.unpack_from('>i', m, p + 8)[0], bone)
            elif cmd == 0x08:
                lod_max, lod_min = struct.unpack_from('>ff', m, p + 8)
                branch, = struct.unpack_from('>i', m, p + 28)
                if lod_min == 0 and branch:
                    geo(p + branch, bone)
            elif cmd == 0x0C:
                count, index = struct.unpack_from('>hh', m, p + 8)
                choice = selectors.get(index, 1)
                if 0 < choice <= count:
                    geo(p + struct.unpack_from('>i', m, p + 12 + 4 * (choice - 1))[0], bone)
            elif cmd == 0x0D:
                branch, = struct.unpack_from('>h', m, p + 20)
                if branch:
                    geo(p + branch, bone)
            if nxt == 0:
                return
            p += nxt

    geo(geo_off, -1)
    return dict(transScale=trans_scale, bones=bones, textures=textures, batches=[
        dict(part='body', texture=tex, wrap=list(wrap), vertices=verts) for (tex, wrap), verts in batches.items()])


def export_species(read, ids, decomp, key, spec):
    asset = build_model(read(ids[spec['model']]), spec.get('selectors', {}))
    table = read_table(decomp, spec, ids)
    clips, states = {}, []
    for row in table:
        if row is None or row[0] == 0:
            states.append(None)
            continue
        index, seconds, name = row
        clip_name = re.sub(r'^ASSET_[0-9A-Fa-f]+_(ANIM_)?', '', name).lower()
        if clip_name not in clips:
            clips[clip_name] = build_clip(read(index), seconds)
        states.append([clip_name, seconds])
    asset['clips'] = clips
    asset['states'] = states
    return asset


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', nargs='?', type=Path, default=ROOT / 'reference/roms/bk-us-1.0.z64')
    parser.add_argument('decomp', nargs='?', type=Path, default=ROOT / 'reference/banjo-kazooie')
    parser.add_argument('--only', nargs='*', default=[])
    args = parser.parse_args()
    rom = args.rom.read_bytes()
    if hashlib.md5(rom).hexdigest() != 'b29599651a13f681c9923d69354bf4a3':
        raise SystemExit('Expected a big-endian Banjo-Kazooie US 1.0 ROM')
    read, ids = assets(rom), enum_ids(args.decomp)
    out = ROOT / 'public/models/bk/enemies'
    out.mkdir(parents=True, exist_ok=True)
    for key, spec in SPECIES.items():
        if args.only and key not in args.only:
            continue
        asset = export_species(read, ids, args.decomp, key, spec)
        path = out / f'{key}.json'
        path.write_text(json.dumps(asset, separators=(',', ':')))
        tris = sum(len(b['vertices']) for b in asset['batches']) // 3
        print(f'{key}: {tris} triangles, {len(asset["bones"])} bones, {len(asset["textures"])} textures, '
              f'{len(asset["clips"])} clips, {path.stat().st_size // 1024} KB')


if __name__ == '__main__':
    main()
