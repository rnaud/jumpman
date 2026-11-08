"""Regenerate assets/audio/music from a user's extracted SM64 data, without WASI SDK or sm64-port.

The sm64-port audio engine is compiled once (scripts/render-music.py, needs WASI SDK 34 and the pinned
port) into a wasm whose four sound blobs (sequences, bank sets, instrument bank, sample data) are
ROM-derived.  `--make-template` blanks those blobs out of such a wasm, leaving engine CODE only
(scripts/music/sm64-us-template.wasm + .json, safe to ship).  The default mode rebuilds the blobs from
the user's extracted samples/sequences with the decomp's own tools (cc, python3, Apple clang's assembler)
and patches them into the template: the result is byte-identical to the original wasm.

  render-music-template.py                        # pipeline use: writes assets/audio/music/*
  render-music-template.py --make-template WASM   # maintainers: wasm -> scripts/music template
"""
import argparse
import importlib.util
import json
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('render_music', HERE / 'render-music.py')
rm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rm)

TEMPLATE_DIR = HERE / 'music'
BLOBS = {'gMusicData': 'sequences.bin', 'gBankSetsData': 'sets.bin',
         'gSoundDataADSR': 'native.ctl', 'gSoundDataRaw': 'native.tbl'}


def make_template(wasm_path):
    tmp = Path(tempfile.mkdtemp())
    rm.build_banks(tmp)
    wasm = bytearray(Path(wasm_path).read_bytes())
    layout = {}
    for symbol, name in BLOBS.items():
        blob = (tmp / name).read_bytes()
        at = bytes(wasm).find(blob)
        if at < 0 or bytes(wasm).count(blob) != 1:
            raise SystemExit(f'{name} not found exactly once in {wasm_path}')
        layout[symbol] = {'offset': at, 'length': len(blob)}
        wasm[at:at + len(blob)] = bytes(len(blob))
    (TEMPLATE_DIR / 'sm64-us-template.wasm').write_bytes(wasm)
    (TEMPLATE_DIR / 'sm64-us-template.json').write_text(json.dumps(layout, indent=2) + '\n')
    print('template written;', {k: v['length'] for k, v in layout.items()})


def patch_template(tmp):
    layout = json.loads((TEMPLATE_DIR / 'sm64-us-template.json').read_text())
    wasm = bytearray((TEMPLATE_DIR / 'sm64-us-template.wasm').read_bytes())
    for symbol, name in BLOBS.items():
        blob = (tmp / name).read_bytes()
        at, length = layout[symbol]['offset'], layout[symbol]['length']
        if len(blob) != length:
            raise SystemExit(f'{name}: built {len(blob)} bytes, template expects {length} '
                             '(is this the US Super Mario 64 ROM?)')
        if any(wasm[at:at + length]):
            raise SystemExit('template is not data-less')
        wasm[at:at + length] = blob
    return bytes(wasm)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--make-template', type=Path)
    a = ap.parse_args()
    if a.make_template:
        return make_template(a.make_template)
    rm.OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as scratch:
        tmp = Path(scratch)
        (rm.OUT / 'sounds.json').write_text(json.dumps(rm.sound_identifiers(tmp), indent=2) + '\n')
        rm.build_banks(tmp)
        (rm.OUT / 'sm64-us.wasm').write_bytes(patch_template(tmp))
    (rm.OUT / 'courses.json').write_text(json.dumps(rm.course_settings(), separators=(',', ':')) + '\n')


if __name__ == '__main__':
    main()
