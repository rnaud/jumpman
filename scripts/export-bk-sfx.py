"""Export every Banjo-Kazooie (US 1.0) sound effect from the local ROM.

BK keeps all its effects in one libultra ALBankFile (ROM 0xD846C0, soundfont1ctl / tbl): one instrument whose sound N is
`enum sfx_e` value N (reference/banjo-kazooie `include/enums.h`), 402 one-shot (or looped) VADPCM samples at 22050 Hz.
`export-character-voices.py` already decodes 13 of them as Banjo's and Kazooie's voices; this writes all of them:

* `assets/audio/voices/bk_sfx/<id>_<NAME>_0.wav` - mono 16-bit, `<id>` the sfx_e value in hex, `<NAME>` the enum name without
  `SFX_` and its hex prefix (e.g. `2_CLAW_SWIPE_0.wav`, `D_EGGSHELL_BREAKING_0.wav`); looped samples are written once through
  (the loop region is not repeated).
* `assets/audio/voices/manifest-bk-sfx.json` - rate, duration, peak, rms and the id of each clip, and the ids that
  were skipped with the reason (samples that are not VADPCM).

Not modelled: the per-sound envelope, pan and volume of the sound entries, the sound-source pitch / volume randomisation
of the game code (`func_8030E760` and friends). Clips keep the ROM's own level. The output is deterministic. A missing ROM
leaves the set out; a ROM with the wrong hash is an error.

    python3 scripts/export-bk-sfx.py [--rom reference/roms/bk-us-1.0.z64] [--out assets/audio]
"""
import argparse
import hashlib
import json
import re
import runpy
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BK_MD5 = 'b29599651a13f681c9923d69354bf4a3'
voices = runpy.run_path(str(ROOT / 'scripts/export-character-voices.py'))


def sfx_names(enums_h):
    """`enum sfx_e` -> {value: name without the `SFX_<hex>_` prefix}."""
    text = enums_h.read_text()
    body = text.split('enum sfx_e', 1)[1].split('};', 1)[0]
    body = re.sub(r'//[^\n]*', '', body)
    body = re.sub(r'/\*.*?\*/', '', body, flags=re.S)
    names, value = {}, 0
    for entry in body.split('{', 1)[1].split(','):
        entry = entry.strip()
        if not entry:
            continue
        m = re.match(r'(\w+)(?:\s*=\s*(0x[0-9A-Fa-f]+|\d+))?', entry)
        if not m:
            continue
        if m.group(2):
            value = int(m.group(2), 0)
        name = re.sub(r'^SFX_[0-9A-Fa-f]+_', '', m.group(1))
        names[value] = name
        value += 1
    return names


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--rom', type=Path, default=ROOT / 'reference/roms/bk-us-1.0.z64')
    ap.add_argument('--out', type=Path, default=ROOT / 'assets/audio')
    args = ap.parse_args()
    if not args.rom.is_file():
        print('no Banjo-Kazooie ROM: skipping')
        return
    rom = args.rom.read_bytes()
    if hashlib.md5(rom).hexdigest() != BK_MD5:
        raise SystemExit('Expected a big-endian Banjo-Kazooie US 1.0 ROM')
    enums = ROOT / 'reference/banjo-kazooie/include/enums.h'
    names = sfx_names(enums) if enums.is_file() else {}
    ctl, tbl = rom[0xD846C0:0xD954B0], rom[0xD954B0:0xEA3EB0]
    bank = struct.unpack_from('>I', ctl, 4)[0]
    rate, instrument = struct.unpack_from('>iI', ctl, bank + 4)[0], struct.unpack_from('>I', ctl, bank + 12)[0]
    count = struct.unpack_from('>h', ctl, instrument + 14)[0]
    if rate != 22050 or count != 402:
        raise SystemExit('unexpected Banjo-Kazooie sound bank')
    out = args.out / 'voices/bk_sfx'
    manifest, skipped = {}, {}
    for i in range(count):
        p = struct.unpack_from('>I', ctl, instrument + 16 + 4 * i)[0]
        if p == 0:
            skipped[i] = 'empty slot'
            continue
        wave_ptr = struct.unpack_from('>I', ctl, p + 8)[0]
        base, length, kind, _flags, loop, book = struct.unpack_from('>iiBBxxII', ctl, wave_ptr)
        if kind != 0 or length <= 0:
            skipped[i] = f'sample type {kind}, length {length}'
            continue
        order, npred = struct.unpack_from('>ii', ctl, book)
        pcm = voices['vadpcm'](ctl[book:book + 8 + 16 * order * npred], tbl[base:base + length])
        if not pcm:
            skipped[i] = 'empty sample'
            continue
        name = f'{i:X}_{names.get(i, "SOUND")}'
        rel = f'voices/bk_sfx/{name}_0.wav'
        voices['write_wav'](args.out / rel, pcm, rate)
        peak, rms = voices['stats'](pcm)
        manifest[rel] = {'id': i, 'sampleRate': rate, 'frames': len(pcm), 'duration': round(len(pcm) / rate, 4),
                         'peak': peak, 'rms': rms, 'looped': bool(loop)}
    (args.out / 'voices/manifest-bk-sfx.json').write_text(json.dumps(
        {'clips': manifest, 'skipped': {f'{k:X}': v for k, v in skipped.items()}}, indent=1, sort_keys=True))
    print(f'{len(manifest)} Banjo-Kazooie sound effects, {len(skipped)} skipped -> {out}')


if __name__ == '__main__':
    main()
