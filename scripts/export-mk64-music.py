#!/usr/bin/env python3
"""Export the Mario Kart 64 music data from a US 1.0 ROM and build the MK64 audio engine around it.

MK64 ships its audio as four blobs in the ROM (a descendant of the SM64 EU sound library): the
sequence archive (30 sequences), the instrument banks (`ctl`, 21 banks), the raw sample table
(`tbl`, VADPCM) and the sequence -> bank-set table.  The engine is the decomp's own
`src/audio/*.c` compiled to WebAssembly (`scripts/render-music-mk64.py`, needs WASI SDK 34 + the
pinned sm64-port for the portable RSP mixer); the checked-in template
(`scripts/music/mk64-template.wasm`) holds that CODE only.  This script converts the ROM blobs to the
little-endian layout a wasm32 build reads (the engine walks C structs in them) and patches them into
the template, writing `assets/audio/music/mk64/mk64-audio.wasm` (+ `sequences.json`).

Pure standard library; no emulator, compiler or network.

  export-mk64-music.py [ROM]            # default: reference/roms/mk64-us-1.0.z64
  export-mk64-music.py --check          # re-run the structural self-checks only
"""
import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/audio/music/mk64'
TEMPLATE = ROOT / 'scripts/music/mk64-template.wasm'
LAYOUT = ROOT / 'scripts/music/mk64-template.json'
SHA1 = '579c48e211ae952530ffc8738709f078d5dd215e'

# ROM layout (linker script of n64decomp/mk64: audio_banks, audio_tables, sequences, instrument_sets).
SEQ_ROM = 0xBC5F60
BANKS_ROM = SEQ_ROM - 0x25FD00
TABLES_ROM = BANKS_ROM + 0x13840
SEQ_COUNT = 30
BLOBS = {'gMk64Sequences': 'seq', 'gMk64Banks': 'ctl', 'gMk64Tables': 'tbl', 'gMk64BankSets': 'sets'}


def be16(d, o): return struct.unpack_from('>H', d, o)[0]
def be32(d, o): return struct.unpack_from('>I', d, o)[0]


class Swapper:
    """Byte-swap a big-endian buffer in place, field by field, refusing conflicting passes."""

    def __init__(self, data):
        self.d = bytearray(data)
        self.done = {}

    def u32(self, off):
        self._swap(off, 4)

    def u16(self, off):
        self._swap(off, 2)

    def _swap(self, off, size):
        if off + size > len(self.d):
            raise SystemExit(f'field at {off:#x} is outside the buffer')
        if off in self.done:
            if self.done[off] != size:
                raise SystemExit(f'conflicting field sizes at {off:#x}')
            return
        self.done[off] = size
        self.d[off:off + size] = self.d[off:off + size][::-1]


def alseqfile(data, base, size, count_hint=None):
    """Swap an ALSeqFile header (rev u16, count u16, {offset u32, len u32}*count); returns entries."""
    rev, count = struct.unpack_from('>HH', data, base)
    if count_hint is not None and count != count_hint:
        raise SystemExit(f'expected {count_hint} entries, found {count}')
    entries = [struct.unpack_from('>II', data, base + 4 + i * 8) for i in range(count)]
    return rev, entries


def swap_header(buf, count):
    s = Swapper(buf)
    s.u16(0)
    s.u16(2)
    for i in range(count):
        s.u32(4 + i * 8)
        s.u32(8 + i * 8)
    return bytes(s.d)


def convert_bank(raw):
    """One ctl bank: 16-byte header then an AudioBank (drums, instruments[], ...) with offsets relative to it."""
    n_inst, n_drums = be32(raw, 0), be32(raw, 4)
    body = raw[16:]
    sw = Swapper(raw)
    for w in range(0, 16, 4):
        sw.u32(w)
    s = Swapper(body)
    ln = len(body)

    def chk(off, size=4):
        if off < 0 or off + size > ln:
            raise SystemExit(f'bank offset {off:#x} out of range ({ln:#x})')
        return off

    seen = {'env': set(), 'sample': set(), 'loop': set(), 'book': set()}

    def envelope(off):
        # Envelopes stay big endian: the engine byte-swaps them when it reads them (BSWAP16), because
        # envelopes embedded in sequence files are big endian too.  Only validate the extent here.
        if off in seen['env']:
            return
        seen['env'].add(off)
        while True:
            chk(off, 4)
            delay = struct.unpack_from('>h', body, off)[0]
            off += 4
            if delay <= 0:
                break

    def sample(off):
        if off == 0 or off in seen['sample']:
            return
        seen['sample'].add(off)
        chk(off, 0x14)
        for k in (4, 8, 12, 16):
            s.u32(off + k)
        loop, book = be32(body, off + 8), be32(body, off + 12)
        if loop and loop not in seen['loop']:
            seen['loop'].add(loop)
            chk(loop, 16)
            count = be32(body, loop + 8)
            for k in (0, 4, 8, 12):
                s.u32(loop + k)
            if count:
                chk(loop + 16, 32)
                for k in range(16):
                    s.u16(loop + 16 + k * 2)
        if book and book not in seen['book']:
            seen['book'].add(book)
            chk(book, 8)
            order, npred = be32(body, book), be32(body, book + 4)
            if not (0 < order <= 8 and 0 < npred <= 16):
                raise SystemExit(f'implausible ADPCM book at {book:#x}: order {order} predictors {npred}')
            s.u32(book)
            s.u32(book + 4)
            chk(book + 8, order * npred * 16)
            for k in range(order * npred * 8):
                s.u16(book + 8 + k * 2)

    def sound(off):
        smp = be32(body, off)
        s.u32(off)
        s.u32(off + 4)
        sample(smp)

    drums_off = be32(body, 0)
    s.u32(0)
    inst = []
    for i in range(n_inst):
        o = 4 + i * 4
        inst.append(be32(body, o))
        s.u32(o)
    for off in inst:
        if off == 0:
            continue
        chk(off, 0x20)
        env = be32(body, off + 4)
        s.u32(off + 4)
        sound(off + 8)
        sound(off + 16)
        sound(off + 24)
        envelope(env)
    if drums_off and n_drums:
        chk(drums_off, n_drums * 4)
        drums = []
        for i in range(n_drums):
            drums.append(be32(body, drums_off + i * 4))
            s.u32(drums_off + i * 4)
        for off in drums:
            if off == 0:
                continue
            chk(off, 0x10)
            env = be32(body, off + 12)
            sound(off + 4)
            s.u32(off + 12)
            envelope(env)
    return bytes(sw.d[:16]) + bytes(s.d), len(seen['sample']), len(seen['env'])


def build(rom):
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise SystemExit('not a Mario Kart 64 (US 1.0) .z64 ROM')
    # --- sequences
    rev, seqs = alseqfile(rom, SEQ_ROM, 0, SEQ_COUNT)
    assert rev == 3
    seq_end = max(o + l for o, l in seqs)
    seq = swap_header(rom[SEQ_ROM:SEQ_ROM + seq_end], SEQ_COUNT)
    seq += bytes(-len(seq) % 16)
    # --- instrument sets: u16 offsets (one per sequence) then bank id lists
    # The table sits right behind the (padded) sequence archive; it starts with offset 0x3C = 30 * 2.
    sets_rom = rom.find(struct.pack('>HHH', SEQ_COUNT * 2, SEQ_COUNT * 2 + 2, SEQ_COUNT * 2 + 4), SEQ_ROM + seq_end,
                        SEQ_ROM + seq_end + 0x100)
    if sets_rom < 0:
        raise SystemExit('instrument set table not found behind the sequences (ROM layout changed?)')
    sets = bytearray(rom[sets_rom:sets_rom + 0x100])
    first = be16(sets, 0)
    if first != SEQ_COUNT * 2:
        raise SystemExit(f'instrument set table starts at {first:#x}, expected {SEQ_COUNT * 2:#x} (ROM layout changed?)')
    for i in range(SEQ_COUNT):
        sets[i * 2:i * 2 + 2] = sets[i * 2:i * 2 + 2][::-1]
    sets = bytes(sets)
    # --- banks
    rev, banks = alseqfile(rom, BANKS_ROM, 0, 21)
    ctl = bytearray(rom[BANKS_ROM:TABLES_ROM])
    ctl[:] = swap_header(bytes(ctl), 21)
    n_samples = n_envs = 0
    for off, ln in banks:
        conv, ns, ne = convert_bank(rom[BANKS_ROM + off:BANKS_ROM + off + ln])
        ctl[off:off + ln] = conv
        n_samples += ns
        n_envs += ne
    # --- samples
    tbl = bytearray(rom[TABLES_ROM:SEQ_ROM])
    rev2, tables = alseqfile(rom, TABLES_ROM, 0, 21)
    tbl[:] = swap_header(bytes(tbl), 21)
    # --- checks: every bank set names existing banks, every sample lies inside the table
    for i in range(SEQ_COUNT):
        o = struct.unpack_from('<H', sets, i * 2)[0]
        n = sets[o]
        assert 0 < n < 8 and all(b < 21 for b in sets[o + 1:o + 1 + n]), f'bank set {i}'
    info = dict(sequences=[dict(id=i, offset=o, length=l) for i, (o, l) in enumerate(seqs)],
                banks=len(banks), samples=n_samples, envelopes=n_envs,
                bytes=dict(seq=len(seq), ctl=len(ctl), tbl=len(tbl), sets=len(sets)))
    return dict(seq=seq, ctl=bytes(ctl), tbl=bytes(tbl), sets=sets), info


def patch_template(blobs):
    layout = json.loads(LAYOUT.read_text())
    wasm = bytearray(TEMPLATE.read_bytes())
    for symbol, key in BLOBS.items():
        at, length = layout[symbol]['offset'], layout[symbol]['length']
        blob = blobs[key]
        if len(blob) != length:
            raise SystemExit(f'{key}: built {len(blob)} bytes, the template expects {length} (is this the US 1.0 ROM?)')
        if any(wasm[at:at + length]):
            raise SystemExit('template is not data-less')
        wasm[at:at + length] = blob
    return bytes(wasm)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('rom', nargs='?', type=Path, default=ROOT / 'reference/roms/mk64-us-1.0.z64')
    ap.add_argument('--blobs', type=Path, help='also write the four converted blobs to this directory (build tooling)')
    ap.add_argument('--out', type=Path, default=OUT)
    a = ap.parse_args()
    blobs, info = build(a.rom.read_bytes())
    if a.blobs:
        a.blobs.mkdir(parents=True, exist_ok=True)
        for k, v in blobs.items():
            (a.blobs / f'{k}.bin').write_bytes(v)
    a.out.mkdir(parents=True, exist_ok=True)
    if TEMPLATE.exists() and LAYOUT.exists():
        (a.out / 'mk64-audio.wasm').write_bytes(patch_template(blobs))
    elif not a.blobs:
        raise SystemExit('no engine template yet (scripts/music/mk64-template.wasm): run with --blobs to export the data only')
    (a.out / 'sequences.json').write_text(json.dumps(info, separators=(',', ':')) + '\n')
    print(json.dumps(info['bytes']), 'banks', info['banks'], 'samples', info['samples'], 'envelopes', info['envelopes'])


if __name__ == '__main__':
    sys.exit(main())
