"""Export Ocarina of Time item models (bomb, bombchu, boomerang, arrow, hookshot tip / chain, deku nut,
deku stick) from a local US 1.2 ROM into public/models/oot/items/*.json.

Each file is a one-bone Link-format asset (`limbs`, `textures`, `batches` with `[x, y, z, s, t, nx, ny, nz, bone]`
vertices, `clips` empty) so the Rust side can reuse the Link batch loader. The display lists come from
`gameplay_keep` (segment 4: gBombBodyDL, gBombCapDL, gBombchuDL, gBoomerangDL, gArrowNearDL), `object_link_boy`
(segment 6: gLinkAdultHookshotTipDL, gLinkAdultHookshotChainDL) and the `object_gi_*` pickups.
Usage: python3 scripts/export-oot-items.py [rom] [decomp-dir]
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

# name -> (object bank, display lists (xml name, mode) in draw order)
ITEMS = {
    'bomb_body': ('gameplay_keep', ['gBombBodyDL']),
    'bomb_cap': ('gameplay_keep', ['gBombCapDL']),
    'bombchu': ('gameplay_keep', ['gBombchuDL']),
    'boomerang': ('gameplay_keep', ['gBoomerangDL']),
    'arrow': ('gameplay_keep', ['gArrowNearDL']),
    'hookshot_tip': ('object_link_boy', ['gLinkAdultHookshotTipDL']),
    'hookshot_chain': ('object_link_boy', ['gLinkAdultHookshotChainDL']),
    'deku_nut': ('object_gi_nuts', ['gGiNutDL']),
    'deku_stick': ('object_gi_stick', ['gGiStickDL']),
}


def texture(data, spec):
    """Superset of the Link decoder: adds ia8 / i4 / ia4."""
    fmt = spec['Format']
    start = link.offset(spec)
    w, h = int(spec['Width']), int(spec['Height'])
    if fmt in ('ia8', 'i4', 'ia4'):
        pixels = bytearray()
        for i in range(w * h):
            if fmt == 'ia8':
                b = data[start + i]
                intensity, alpha = (b >> 4) * 17, (b & 15) * 17
            else:
                b = data[start + i // 2]
                n = (b >> 4) if i % 2 == 0 else (b & 15)
                if fmt == 'i4':
                    intensity, alpha = n * 17, n * 17
                else:
                    intensity, alpha = (n >> 1) * 255 // 7, 255 * (n & 1)
            pixels.extend([intensity] * 3 + [alpha])
        return dict(width=w, height=h, rgba=base64.b64encode(pixels).decode())
    return link.texture(data, spec)


def decode(segments, specs, textures, dl):
    """Walk a display list (and nested ones); returns `{(tex, color, wrap_s, wrap_t): [vertex...]}`."""
    batches = {}
    cache = {}
    state = dict(tex=None, color=0xffffff, ws=0, wt=0)

    def run(seg, p):
        while True:
            a, b = struct.unpack_from('>II', segments[seg], p)
            p += 8
            op = a >> 24
            if op == 0xdf:
                return
            if op == 0xde:
                nseg = b >> 24
                if nseg in segments:
                    run(nseg, b & 0xffffff)
                if a & 0x00010000:  # branch (no return)
                    return
            elif op == 0xfd:
                found = specs.get(b)
                if found and 'tlut' not in found[1]['Name'].lower():
                    state['tex'] = found[1]['Name']
                    if state['tex'] not in textures:
                        textures[state['tex']] = texture(found[0], found[1])
            elif op == 0xf5 and b >> 24 == 0:
                state['ws'], state['wt'] = (b >> 8) & 3, (b >> 18) & 3
            elif op == 0xfa:
                state['color'] = b >> 8
            elif op == 1:
                n = (a >> 12) & 255
                first = ((a & 255) >> 1) - n
                vseg = b >> 24
                for i in range(n):
                    v = struct.unpack_from('>hhh2xhhbbbB', segments[vseg], (b & 0xffffff) + i * 16)
                    cache[first + i] = [*v[:7], v[7], 0]
            elif op in (5, 6):
                key = (state['tex'], state['color'], state['ws'], state['wt'])
                batch = batches.setdefault(key, [])
                for word in ([a] if op == 5 else [a, b]):
                    batch.extend(cache[(word >> shift & 255) // 2] for shift in (16, 8, 0))

    run(dl >> 24, dl & 0xffffff)
    return batches


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    root = HERE.parent
    parser.add_argument('rom', nargs='?', type=Path, default=root / 'reference/roms/oot-us-1.2.z64')
    parser.add_argument('decomp', nargs='?', type=Path, default=root / 'reference/oot')
    args = parser.parse_args()
    rom = args.rom.read_bytes()
    if hashlib.md5(rom).hexdigest() != '57a9719ad547c516342e1a15d5c28c3d':
        raise ValueError('Expected a big-endian OoT US 1.2 ROM')
    with (args.decomp / 'baseroms/ntsc-1.2/segments.csv').open() as f:
        names = [row['Name'] for row in csv.DictReader(f)]
    xml = args.decomp / 'assets/xml/objects'
    banks = {}
    for bank in {b for b, _ in ITEMS.values()}:
        banks[bank] = (link.segment(rom, names, bank), link.definitions(xml / (bank + '.xml')))
    keep_data, keep_defs = banks['gameplay_keep']
    out = root / 'public/models/oot/items'
    out.mkdir(parents=True, exist_ok=True)
    for name, (bank, dls) in ITEMS.items():
        data, defs = banks[bank]
        segments = {4: keep_data, 6: data}
        specs = {}
        for seg, d, entries in [(0x04000000, keep_data, keep_defs), (0x06000000, data, defs)]:
            for v in entries.values():
                if 'Format' in v and 'Width' in v and v.get('Offset', '').startswith('0x'):
                    specs[seg + link.offset(v)] = (d, v)
        textures = {}
        merged = {}
        for dl in dls:
            for key, verts in decode(segments, specs, textures, (0x04000000 if bank == 'gameplay_keep' else 0x06000000) + link.offset(defs[dl])).items():
                merged.setdefault(key, []).extend(verts)
        asset = dict(limbs=[dict(pos=[0, 0, 0], child=255, sibling=255)], textures=textures, clips={}, batches=[
            dict(mode='base', texture=tex, color=color, wrap=[ws, wt], vertices=verts)
            for (tex, color, ws, wt), verts in merged.items()])
        (out / (name + '.json')).write_text(json.dumps(asset, separators=(',', ':')))
        count = sum(len(b['vertices']) for b in asset['batches'])
        print(f'{name}: {len(asset["batches"])} batches, {count // 3} triangles, {len(textures)} textures')


if __name__ == '__main__':
    main()
