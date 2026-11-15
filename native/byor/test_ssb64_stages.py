#!/usr/bin/env python3
"""ROM-free checks of the Super Smash Bros. stage and music exporters (scripts/export-ssb64-stages.py,
scripts/export-ssb64-music.py): python3 -m unittest byor/test_ssb64_stages.py

Everything here is synthetic (hand-built AObjEvent32 scripts, collision lines, ALSeqFiles); the enum reader runs against
reference/ssb64 when that submodule is checked out. No ROM bytes are used or committed.
"""
import importlib.util, struct, unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1].parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


st = load('export_ssb64_stages', 'export-ssb64-stages.py')
mu = load('export_ssb64_music', 'export-ssb64-music.py')


class FakeFiles:
    def __init__(self, data):
        self.d = data

    def data(self, fid):
        return self.d


def cmd(op, flags, payload):
    return struct.pack('>I', (op << 25) | (flags << 15) | payload)


class AnimJoint32(unittest.TestCase):
    def test_linear_translation_then_end(self):
        # SetValBlock (3) on TraX (track bit 4) to 100 over 10 frames, then End
        data = cmd(3, 1 << 4, 10) + struct.pack('>f', 100.0) + cmd(0, 0, 0)
        frames = st.simulate32(FakeFiles(data), (0, 0), [0, 0, 0], [0, 0, 0], [1, 1, 1])
        xs = [f[0][0] for f in frames]
        self.assertEqual(len(xs), 11)
        self.assertAlmostEqual(xs[0], 0.0)
        self.assertAlmostEqual(xs[5], 50.0)
        self.assertAlmostEqual(xs[10], 100.0)

    def test_frame_zero_of_a_loop(self):
        # SetVal (4) RotZ (bit 2) to 1.0 over 30 frames, Wait 30, Jump back: one period of 31 frames
        data = cmd(4, 1 << 2, 30) + struct.pack('>f', 1.0) + cmd(2, 0, 30) + cmd(1, 0, 0) + b'\0' * 4
        frames = st.simulate32(FakeFiles(data), (0, 0), [0, 0, 0], [0, 0, 0], [1, 1, 1])
        self.assertEqual(len(frames), 31)
        self.assertAlmostEqual(frames[0][1][2], 0.0)
        self.assertAlmostEqual(frames[15][1][2], 0.5)
        self.assertAlmostEqual(frames[30][1][2], 1.0)


class Collision(unittest.TestCase):
    def normal(self, verts, tri):
        a, b, c = (verts[i] for i in tri[:3])
        u = [b[k] - a[k] for k in range(3)]
        v = [c[k] - b[k] for k in range(3)]
        return [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]

    def test_each_line_kind_faces_its_side(self):
        lines = {'floor': [[(-1000, 0, 0x4000), (1000, 0, 0)]], 'ceil': [[(1000, -100, 0), (-1000, -100, 0)]],
                 'rwall': [[(1000, 0, 0), (1000, -100, 0)]], 'lwall': [[(-1000, -100, 0), (-1000, 0, 0x8000)]]}
        g = dict(yakumono=0, lines=lines, offset=[0.0, 0.0], animated=False)
        verts, tris, passing, ledges, counts = st.collision([g], dict(top=5000, bottom=-5000, left=-5000, right=5000))
        self.assertEqual(len(tris), 12)
        self.assertGreater(self.normal(verts, tris[0])[1], 0)
        self.assertLess(self.normal(verts, tris[2])[1], 0)
        self.assertGreater(self.normal(verts, tris[4])[0], 0)
        self.assertLess(self.normal(verts, tris[6])[0], 0)
        self.assertGreater(self.normal(verts, tris[8])[2], 0)
        self.assertLess(self.normal(verts, tris[10])[2], 0)
        self.assertEqual(tris[8][3], st.SURF_NO_CAM)
        self.assertEqual(passing, [0, 1], 'the pass-through flag of the segment start')
        self.assertEqual(ledges, [], 'the ledge flag sits on the last vertex, which starts no segment')
        self.assertEqual(counts, {'floor': 1, 'ceil': 1, 'rwall': 1, 'lwall': 1})
        self.assertEqual(verts[0], [-348.0, 0.0, -st.DEPTH])

    def test_floor_below(self):
        g = dict(yakumono=0, lines={'floor': [[(-100, 0, 0), (100, 200, 0)]], 'ceil': [], 'rwall': [], 'lwall': []}, offset=[0.0, 0.0], animated=False)
        self.assertAlmostEqual(st.floor_below([g], 0, 500), 100.0)
        self.assertIsNone(st.floor_below([g], 0, 50))
        self.assertIsNone(st.floor_below([g], 300, 500))

    def test_stage_table(self):
        self.assertEqual(len(st.STAGES), 41)
        self.assertEqual(st.map_file_name(st.STAGES[19]), 'GRBonus1DonkeyMap')
        self.assertEqual(st.map_file_name(st.STAGES[39]), 'GRBonus2PurinMap')
        self.assertEqual(st.map_file_name(st.STAGES[6]), 'GRPupupuMap')


class Music(unittest.TestCase):
    def test_alseqfile(self):
        sbk = b'S1' + struct.pack('>h', 2) + struct.pack('>Ii', 20, 3) + struct.pack('>Ii', 23, 1) + bytes([1, 2, 3, 9])
        self.assertEqual(mu.sequences(sbk), [bytes([1, 2, 3]), bytes([9])])
        with self.assertRaises(ValueError):
            mu.sequences(b'\0' * 8)

    def test_bgm_names(self):
        if not (ROOT / 'reference/ssb64/src/gm/gmsound.h').is_file():
            self.skipTest('reference/ssb64 not checked out')
        names = mu.music_names()
        self.assertEqual(len(names), 47)
        self.assertEqual((names[0], names[6], names[24], names[44]), ('Pupupu', 'Castle', 'BossEntry', 'ModeSelect'))


if __name__ == '__main__':
    unittest.main()
