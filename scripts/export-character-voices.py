"""Export Link (Ocarina of Time US 1.2) and Banjo/Kazooie (Banjo-Kazooie US 1.0) voice clips from local ROMs.

Writes mono 16-bit WAVs to assets/audio/voices/<set>/<NAME>_<n>.wav plus a manifest-oot.json / manifest-bk.json per game (rate, duration, peak, RMS, source sample).
`assets/audio/character-sounds.json` refers to the clips as ["voices/<set>/<NAME>", variants]; the game
picks one of <NAME>_0 .. <NAME>_<variants-1> at random.

Both games store voice effects as N64 VADPCM (4-bit, 16 samples per 9 byte frame):

* OoT: sound font 0 (Audiobank) lists the sound effects as (sample header, tuning) pairs; the samples
  live in sample bank 0 of the Audiotable. `include/tables/sfx/voicebank_table.h` plus the channel scripts in
  `assets/audio/sequences/seq_0.prg.seq` (zeldaret/oot) name which entries NA_SE_VO_LI_* play; VO_LI_ARRAYS below
  is that mapping (the game picks one entry of the array at random). Rate = tuning * 32000.
* BK: the libultra ALBankFile at ROM 0xD846C0 (soundfont1ctl/tbl in n64decomp/banjo-kazooie's yaml) holds one
  instrument whose sound N is `enum sfx_e` value N (include/enums.h). Samples are 22050 Hz (bank rate; the engine
  runs at 22000 Hz and `sample_rate` 0x7FF8 means unity pitch).

The clips are not normalised: they keep the ROM's own level, like the SM64 voice clips. Output is deterministic.
A missing ROM leaves that game's sets out; a ROM with the wrong hash is an error.
"""
import argparse
import hashlib
import json
import struct
import wave
from pathlib import Path

OOT_MD5 = '57a9719ad547c516342e1a15d5c28c3d'
BK_MD5 = 'b29599651a13f681c9923d69354bf4a3'

# OoT: NA_SE_VO_LI_<name> -> Soundfont_0 sound-effect indices (SF0_EFFECT_n) it plays, one picked at random.
# Read from the ARRAY_* tables of the sfx channel scripts in seq_0.prg.seq. The child set is the *_KID entries.
VO_LI_ARRAYS = {
    'link': {
        'AUTO_JUMP': [26, 27],
        'SWORD_N': [0, 1, 2, 3],
        'SWORD_L': [4, 5],
        'CLIMB_END': [7, 8],
        'DAMAGE_S': [9, 10, 11],
        'FALL_S': [17, 18],
        'FALL_L': [15, 16],
        'LAND_DAMAGE_S': [24],
    },
    'young_link': {
        'AUTO_JUMP': [53, 54],      # NA_SE_VO_LI_AUTO_JUMP_KID
        'SWORD_N': [28, 29, 30, 31],
        'SWORD_L': [32, 33],        # NA_SE_VO_LI_ROLLING_CUT_KID, the child slot of SWORD_L
        'CLIMB_END': [35, 36],
        'DAMAGE_S': [37, 38, 39],
        'FALL_S': [45, 46],
        'FALL_L': [43, 44],
        'LAND_DAMAGE_S': [49],
    },
}
OOT_SFX_COUNT = 136

# BK: output name -> `enum sfx_e` values (names are the decomp's). 'OW' is basfx_owSfxList, 'JUMP' basfx_jumpSfxList.
BK_VOICES = {
    'JUMP': [0x54, 0x55, 0x56],                                    # SFX_54_BANJO_HOO_1, HOO_2, 56_HUI
    'OW': [0x32, 0x34, 0x35, 0x37, 0x38, 0x39, 0x3A, 0x3B],       # EGHEE AGHOAA WOAH OHWW AYE_1 AYE_2 HOUW GAAH
    'AHOO': [0x33],                                                # SFX_33_BANJO_AHOO
    'DOH': [0x36],                                                 # SFX_36_BANJO_DOH (death inducing damage)
    'HUIII': [0x53],                                               # SFX_53_BANJO_HUIII (shockspring liftoff)
    'YAH_OH': [0x52],                                              # SFX_52_BANJO_YAH_OH (starting a Beakbomb)
    'WAHOO': [0x4F],                                               # SFX_4F_BANJO_WAHOO
    'UWAAAAOOH': [0x63],                                           # SFX_63_BANJO_UWAAAAOOH (huge fall starts)
    'KAZOOIE_HEUGH': [0x47],                                       # SFX_47_KAZOOIE_HEUGH (FeatherFlap)
    'KAZOOIE_RUUH': [0x48],                                        # SFX_48_KAZOOIE_RUUH (TalonTrot jump)
    'KAZOOIE_RUH': [0x43],                                         # SFX_43_KAZOOIE_RUH (Beakbarge)
    'KAZOOIE_HUGHH': [0x45],                                       # SFX_45_KAZOOIE_HUGHH (Beakbuster)
    'KAZOOIE_BRUIII': [0x4E],                                      # SFX_4E_KAZOOIE_BRUIII
}


def vadpcm(book, data):
    """Decode N64 VADPCM; `book` is the AdpcmBook/ALADPCMBook struct (order, npredictors, s16 table)."""
    order, npred = struct.unpack_from('>ii', book, 0)
    if order != 2:
        raise ValueError(f'unsupported VADPCM order {order}')
    table = struct.unpack_from(f'>{8 * order * npred}h', book, 8)
    books = [[table[p * 16 + o * 8:p * 16 + o * 8 + 8] for o in range(2)] for p in range(npred)]
    out = []
    h2 = h1 = 0
    for off in range(0, len(data) - 8, 9):
        scale, pred = data[off] >> 4, data[off] & 15
        t0, t1 = books[pred]
        nib = []
        for b in data[off + 1:off + 9]:
            nib += (b >> 4, b & 15)
        res = [(n - 16 if n >= 8 else n) << scale for n in nib]
        for half in (0, 8):
            e = res[half:half + 8]
            y = []
            for i in range(8):
                acc = (e[i] << 11) + h2 * t0[i] + h1 * t1[i]
                for j in range(i):
                    acc += e[j] * t1[i - 1 - j]
                v = acc >> 11
                y.append(-32768 if v < -32768 else 32767 if v > 32767 else v)
            out += y
            h2, h1 = y[6], y[7]
    return out


def write_wav(path, pcm, rate):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as w:
        w.setparams((1, 2, rate, len(pcm), 'NONE', 'not compressed'))
        w.writeframes(struct.pack(f'<{len(pcm)}h', *pcm))


def stats(pcm):
    peak = max(map(abs, pcm))
    rms = (sum(x * x for x in pcm) / len(pcm)) ** 0.5
    return peak, round(rms, 1)


def emit(out, group, name, variants, manifest):
    """variants: list of (pcm, rate, source description)."""
    for n, (pcm, rate, source) in enumerate(variants):
        rel = f'voices/{group}/{name}_{n}.wav'
        write_wav(out.parent / rel, pcm, rate)
        peak, rms = stats(pcm)
        manifest[rel] = {'sampleRate': rate, 'frames': len(pcm), 'duration': round(len(pcm) / rate, 4),
                         'peak': peak, 'rms': rms, 'source': source}


def export_oot(rom, out, manifest):
    # dmadata (0x7960 in US 1.2): entry 3 = Audiobank, 5 = Audiotable.
    def seg(i, size):
        vstart, vend, pstart, pend = struct.unpack_from('>4I', rom, 0x7960 + 16 * i)
        if pend or vend - vstart != size:
            raise ValueError(f'unexpected dmadata entry {i}')
        return rom[pstart:pstart + size]
    bank, table = seg(3, 179648), seg(5, 4526992)
    font = bank[0:0x3AA0]                      # Soundfont_0 (gSoundFontTable[0]), sample bank 0 at Audiotable 0
    sfx_list = struct.unpack_from('>I', font, 4)[0]

    def effect(i):
        if not 0 <= i < OOT_SFX_COUNT:
            raise ValueError(f'sfx index {i}')
        sample, tuning = struct.unpack_from('>If', font, sfx_list + 8 * i)
        bits, addr, loop, book = struct.unpack_from('>IIII', font, sample)
        codec, size = bits >> 28, bits & 0xFFFFFF
        if codec != 0 or struct.unpack_from('>I', font, loop + 8)[0] != 0:
            raise ValueError(f'effect {i}: not a one-shot ADPCM sample')
        order, npred = struct.unpack_from('>ii', font, book)
        pcm = vadpcm(font[book:book + 8 + 16 * order * npred], table[addr:addr + size])
        return pcm, round(tuning * 32000), f'OoT Soundfont_0 SF0_EFFECT_{i}'
    for group, names in VO_LI_ARRAYS.items():
        for name, idx in names.items():
            emit(out, group, name, [effect(i) for i in idx], manifest)


def export_bk(rom, out, manifest):
    ctl, tbl = rom[0xD846C0:0xD954B0], rom[0xD954B0:0xEA3EB0]
    bank = struct.unpack_from('>I', ctl, 4)[0]
    rate, instrument = struct.unpack_from('>iI', ctl, bank + 4)[0], struct.unpack_from('>I', ctl, bank + 12)[0]
    count = struct.unpack_from('>h', ctl, instrument + 14)[0]
    if rate != 22050 or count != 402:
        raise ValueError('unexpected Banjo-Kazooie sound bank')

    def sound(i):
        p = struct.unpack_from('>I', ctl, instrument + 16 + 4 * i)[0]
        wave_ptr = struct.unpack_from('>I', ctl, p + 8)[0]
        base, length, kind, _flags, loop, book = struct.unpack_from('>iiBBxxII', ctl, wave_ptr)
        if kind != 0 or loop:
            raise ValueError(f'sound {i}: not a one-shot ADPCM sample')
        order, npred = struct.unpack_from('>ii', ctl, book)
        pcm = vadpcm(ctl[book:book + 8 + 16 * order * npred], tbl[base:base + length])
        return pcm, rate, f'BK sfx_e 0x{i:X}'
    for name, ids in BK_VOICES.items():
        emit(out, 'banjo', name, [sound(i) for i in ids], manifest)


def load(path, md5):
    data = path.read_bytes()
    if hashlib.md5(data).hexdigest() != md5:
        raise ValueError(f'{path}: unexpected ROM (need a big-endian z64 of the right release)')
    return data


def main():
    root = Path(__file__).resolve().parents[1]
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--oot', type=Path, default=root / 'reference/roms/oot-us-1.2.z64')
    ap.add_argument('--bk', type=Path, default=root / 'reference/roms/bk-us-1.0.z64')
    ap.add_argument('--out', type=Path, default=root / 'assets/audio/voices')
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for label, rom, md5, fn, name in (('OoT', args.oot, OOT_MD5, export_oot, 'manifest-oot.json'),
                                      ('BK', args.bk, BK_MD5, export_bk, 'manifest-bk.json')):
        if not rom.is_file():
            print(f'{label}: no ROM at {rom}; its voice sets stay silent')
            continue
        manifest = {}
        fn(load(rom, md5), args.out, manifest)
        (args.out / name).write_text(json.dumps(dict(sorted(manifest.items())), indent=1) + '\n')
        print(f'{label}: exported {len(manifest)} clips')


if __name__ == '__main__':
    main()
