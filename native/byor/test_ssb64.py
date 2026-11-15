#!/usr/bin/env python3
"""ROM-free checks of the Super Smash Bros. exporter (scripts/export-ssb64.py) and its pipeline entry:
python3 -m unittest byor/test_ssb64.py

Everything here is synthetic (hand-built vpk0 streams, relocData tables, figatree scripts, combiner words); the decomp
table readers run against reference/ssb64 when that submodule is checked out. No ROM bytes are used or committed.
"""
import hashlib, importlib.util, struct, sys, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import pipeline  # noqa: E402

spec = importlib.util.spec_from_file_location('export_ssb64', ROOT / 'scripts/export-ssb64.py')
ex = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ex)


def bits_to_bytes(bits):
    bits += '0' * (-len(bits) % 16)
    return bytes(int(bits[i:i + 8], 2) for i in range(0, len(bits), 8))


class Vpk0(unittest.TestCase):
    def test_literals_and_back_reference(self):
        # magic, size 6, method 0; offsets tree = leaf(2 bits), lengths tree = leaf(3 bits); 'a', 'b', copy 4 from 2 back
        bits = format(int.from_bytes(b'vpk0', 'big'), '032b') + format(6, '032b') + format(0, '08b')
        bits += '0' + format(2, '08b') + '1' + '0' + format(3, '08b') + '1'
        bits += '0' + format(ord('a'), '08b') + '0' + format(ord('b'), '08b') + '1' + format(2, '02b') + format(4, '03b')
        self.assertEqual(ex.vpk0(bits_to_bytes(bits)), b'ababab')

    def test_rejects_other_data(self):
        with self.assertRaises(ValueError):
            ex.vpk0(b'\0' * 16)


class RelocFiles(unittest.TestCase):
    def test_intern_and_extern_pointer_chains(self):
        """Two uncompressed files: file 0 has an intern pointer (word 1 -> byte 8) and an extern one (word 2 -> file 1
        byte 4, the file id read from the list after file 0's data), the way lbRelocLoadAndRelocFile walks them."""
        f0 = struct.pack('>IIII', 0xAAAAAAAA, (0xFFFF << 16) | 2, (0xFFFF << 16) | 1, 0xBBBBBBBB)
        f1 = struct.pack('>II', 0x11111111, 0x22222222)
        data_start = ex.TABLE + (ex.FILE_COUNT + 1) * 12
        rom = bytearray(data_start + 64)
        entries = [(0, 1, len(f0) // 4, 2, len(f0) // 4), (len(f0) + 4, 0xFFFF, len(f1) // 4, 0xFFFF, len(f1) // 4)]
        for i, (off, intern, cs, extern, ds) in enumerate(entries):
            struct.pack_into('>IHHHH', rom, ex.TABLE + 12 * i, off, intern, cs, extern, ds)
        rom[data_start:data_start + len(f0)] = f0
        struct.pack_into('>H', rom, data_start + len(f0), 1)  # extern file id list after file 0
        rom[data_start + len(f0) + 4:data_start + len(f0) + 4 + len(f1)] = f1
        files = ex.Files(bytes(rom))
        data, ptrs = files.get(0)
        self.assertEqual(data, f0)
        self.assertEqual(ptrs, {4: (0, 8), 8: (1, 4)})
        self.assertEqual(files.ptr((0, 0), 8), (1, 4))


class Motion(unittest.TestCase):
    def joint(self):
        return dict(t=[0.0, 0.0, 0.0], r=[0.0, 0.0, 0.0], s=[1.0, 1.0, 1.0])

    def test_linear_rotation_track(self):
        """Block 4 frames, SetValBlock RotX = 512 (1 rad at 1/512) over 4 frames, End: 0 until frame 4, then a ramp."""
        cmd = lambda op, flags, toggle: (op << 11) | (flags << 1) | toggle
        script = struct.pack('>6H', cmd(2, 0, 1), 4, cmd(2, 1, 1), 4, 512, cmd(0, 0, 0))
        frames = ex.simulate(script, 0, self.joint())
        rx = [f[1][0] for f in frames]
        self.assertEqual(len(rx), 9)
        self.assertAlmostEqual(rx[4], 0.0)
        self.assertAlmostEqual(rx[6], 0.5)
        self.assertAlmostEqual(rx[8], 1.0)

    def test_translation_and_hold(self):
        """SetValAfter (step) on TraY: value 400 * 1/4 = 100 after 2 frames."""
        cmd = lambda op, flags, toggle: (op << 11) | (flags << 1) | toggle
        script = struct.pack('>5H', cmd(9, 1 << 5, 1), 2, 400, cmd(1, 0, 1), 2) + struct.pack('>H', cmd(0, 0, 0))
        ty = [f[0][1] for f in ex.simulate(script, 0, self.joint())]
        self.assertEqual(ty[0], 0.0)
        self.assertEqual(ty[-1], 100.0)


class Rdp(unittest.TestCase):
    def test_tile_axes(self):
        self.assertEqual(ex.tile_axis(8, 2, 3)[0], [0, 1, 2, 3, 3, 2, 1, 0])  # mirror + clamp keeps the whole tile
        self.assertEqual(ex.tile_axis(8, 2, 0), ([0, 1, 2, 3], 10497))      # wrap: one period, REPEAT
        self.assertEqual(ex.tile_axis(8, 2, 1)[1], 33648)                   # MIRRORED_REPEAT

    def test_combiner_prim_times_shade(self):
        a, b, c, d = 3, 8, 4, 7  # (PRIM - 0) * SHADE + 0, alpha = PRIM
        w0 = (a << 20) | (c << 15) | (7 << 12) | (7 << 9) | (a << 5) | c
        w1 = (b << 28) | (b << 24) | (7 << 21) | (7 << 18) | (d << 15) | (7 << 12) | (3 << 9) | (d << 6) | (7 << 3) | 3
        out = ex.combine((w0, w1), False, [0] * 4, [0] * 4, [1.0, 0.5, 0.0, 1.0], [0.5, 0.5, 0.5, 1.0], [0] * 4, 0.0)
        self.assertEqual(out, [0.5, 0.25, 0.0, 1.0])
        self.assertTrue(ex.uses((w0, w1), False, {4}))
        self.assertFalse(ex.uses((w0, w1), False, {1, 2}))

    def test_rotation_order_matches_the_quaternion(self):
        r = [0.3, -0.7, 1.1]
        m = ex.mat_trs([0, 0, 0], r, [1, 1, 1])
        x, y, z, w = ex.quat(r)
        q = [[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
             [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
             [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]]
        for i in range(3):
            for k in range(3):
                self.assertAlmostEqual(m[i][k], q[i][k])


@unittest.skipUnless((ex.DECOMP / 'src/ft/ftdata.c').is_file(), 'reference/ssb64 not checked out')
class DecompTables(unittest.TestCase):
    def test_fighter_and_motion_tables(self):
        names = ex.file_names()
        self.assertEqual(names['MarioMain'], 203)
        self.assertEqual(names['FTMarioAnimWait'], 499)
        cm = ex.common_motions()
        self.assertEqual((cm[4], cm[10], cm[165], cm[184]), ('Wait', 'Run', 'Attack11', 'AttackAirN'))
        tables = ex.fighter_tables(names)
        self.assertEqual(set(tables), set(ex.FIGHTERS))
        for f in ex.FIGHTERS[:-1]:  # every playable fighter: Wait at 4, Appeal (taunt) at 164, the jab at 165
            t = tables[f]
            self.assertEqual(t['motions'][164].split('Anim')[1], 'Taunt', f)
            self.assertTrue(t['motions'][165].endswith('AnimJab1'), f)
        self.assertEqual(tables['Mario']['attr'], 0x428)
        self.assertEqual(tables['Mario']['specials'][3], 'SpecialN')
        self.assertEqual(len(tables['Pikachu']['specials']), 16)


spec = importlib.util.spec_from_file_location('export_ssb64_sounds', ROOT / 'scripts/export-ssb64-sounds.py')
snd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(snd)


class Sounds(unittest.TestCase):
    """scripts/export-ssb64-sounds.py: the FGM interpreters on hand-built programs."""

    def test_cents_ratio_and_packages(self):
        self.assertEqual(snd.cents_ratio(0), 1.0)
        self.assertAlmostEqual(snd.cents_ratio(-1200), 0.5, places=3)
        self.assertEqual(snd.package(bytes([0, 0, 0, 2, 0, 0, 0, 12, 0, 0, 0, 14, 1, 2, 3])), [b'\x01\x02', b'\x03'])
        self.assertEqual(snd.varint(bytes([0x81, 0x02]), 0), (0x102, 2))

    def test_one_note_through_one_articulation(self):
        # a fake bank: sound 0 is 64 samples of 1000
        class FakeBank:
            sounds = [0]

            def sound(self, i):
                return [1000] * 64, None
        # ucd: set_articulation 0, set_volume 255, note pitch_code 13 (0 cents) for 2 frames, stop
        ucd = [bytes([0xD1, 0, 0xD5, 255, (13 << 3) | 7, 2, 0xD0])]
        # tbl: trigger 0, vol 127 then wait 10 frames (extended timer byte before the argument), end
        tbl = [bytes([0x60, 0, 0x08, 10, 127, 0x70])]
        pcm, cut = snd.Engine(ucd, tbl, [], FakeBank()).render(0)
        self.assertFalse(cut)
        self.assertEqual(len(pcm), 64)       # the sample runs out before the 2-frame note ends
        gain = (127 * (255 * 127 >> 7) * 127 >> 7) / 32768.0
        self.assertEqual(pcm[0], int(1000 * gain + 0.5))

    @unittest.skipUnless((ex.DECOMP / 'src/gm/gmsound.h').is_file(), 'reference/ssb64 not checked out')
    def test_enum_names(self):
        names = snd.voice_names()
        self.assertEqual(names[0], 'FGMExplodeS')
        self.assertEqual(names.index('VoiceMarioJump'), 435)
        self.assertEqual(names.index('FGMMarioSpecialN'), 215)
        self.assertEqual(names.index('VoiceCaptainSpecialNPunch'), 347)
        exported = snd.export_list(names)
        self.assertTrue(set(snd.EXPORT_FGM) <= set(names))
        self.assertEqual(sum(n.startswith('Voice') for n in exported), 197)


class PipelineEntry(unittest.TestCase):
    def test_registered_with_the_decomp_checksum(self):
        label, algo, digests = pipeline.ROMS['ssb64']
        self.assertEqual((algo, digests), ('sha1', {ex.SHA1}))
        self.assertIn('ssb64', pipeline.DO)
        self.assertEqual(pipeline.GAME_SCRIPTS['ssb64'], ['export-ssb64.py', 'export-ssb64-motions.py', 'export-ssb64-sounds.py', 'export-ssb64-weapons.py', 'export-ssb64-stages.py', 'export-ssb64-music.py'])
        self.assertEqual(pipeline.LINKED['ssb64'], 'reference/ssb64')

    def test_identified_in_any_byte_order(self):
        """A synthetic image standing in for the ROM (its sha1 swapped into the table): .z64, .v64 and .n64 all identify."""
        z64 = b'\x80\x37\x12\x40' + bytes(range(256)) * 16
        saved = pipeline.ROMS['ssb64']
        pipeline.ROMS['ssb64'] = (saved[0], 'sha1', {hashlib.sha1(z64).hexdigest()})
        try:
            with tempfile.TemporaryDirectory() as t:
                v64 = bytearray(z64); v64[0::2], v64[1::2] = z64[1::2], z64[0::2]
                n64 = bytearray(len(z64))
                for i in range(4):
                    n64[i::4] = z64[3 - i::4]
                for name, data in (('a.z64', z64), ('b.v64', bytes(v64)), ('c.n64', bytes(n64))):
                    (Path(t) / name).write_bytes(data)
                    self.assertEqual(pipeline.identify(Path(t) / name), ('ssb64', z64), name)
        finally:
            pipeline.ROMS['ssb64'] = saved


if __name__ == '__main__':
    unittest.main()
