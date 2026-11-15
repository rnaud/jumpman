#!/usr/bin/env python3
"""ROM-free checks of the Super Smash Bros. weapon / effect / item exporter (scripts/export-ssb64-weapons.py):
python3 -m unittest byor/test_ssb64_weapons.py

Synthetic relocData files only (hand-built DObjDesc trees and DObjDLLink arrays); no ROM bytes are used or committed.
"""
import importlib.util, struct, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

spec = importlib.util.spec_from_file_location('export_ssb64_weapons', ROOT / 'scripts/export-ssb64-weapons.py')
wp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wp)


class FakeFiles:
    """A Files stand-in: file id -> (bytes, {slot offset: (file, offset)})."""

    def __init__(self, files):
        self.files = files

    def get(self, fid):
        return self.files[fid]

    def data(self, fid):
        return self.files[fid][0]

    def ptr(self, ref, off=0):
        if ref is None:
            return None
        return self.files[ref[0]][1].get(ref[1] + off)


class Trees(unittest.TestCase):
    def test_dobjdesc_tree_parents_by_depth(self):
        """Root at depth 0 (translate x 10), a child at depth 1 (translate y 5, its DL at 0x100), the end id 18."""
        desc = struct.pack('>iI9f', 0, 0, 10.0, 0, 0, 0, 0, 0, 1, 1, 1)
        desc += struct.pack('>iI9f', 1, 0, 0, 5.0, 0, 0, 0, 0, 1, 1, 1)
        desc += struct.pack('>iI9f', 18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        files = FakeFiles({7: (desc, {44 + 4: (7, 0x100)})})
        joints = wp.tree(files, (7, 0), True)
        self.assertEqual(len(joints), 2)
        self.assertIsNone(joints[0]['dl'])
        self.assertEqual(joints[1]['dl'], (7, 0x100))
        # the child's world translation is the parent's plus its own
        self.assertEqual([joints[1]['world'][k][3] for k in range(3)], [10.0, 5.0, 0.0])

    def test_single_display_list(self):
        joints = wp.tree(FakeFiles({}), (3, 0x40), False)
        self.assertEqual(joints[0]['dl'], (3, 0x40))
        self.assertEqual(joints[0]['world'][0][0], 1.0)

    def test_dl_links_and_plain_lists(self):
        """A DObjDLLink array {1, dl}, {4, NULL} gives its lists; a G_ENDDL-first list is itself."""
        links = struct.pack('>IIII', 1, 0, 4, 0)
        plain = struct.pack('>II', 0xDF000000, 0)
        files = FakeFiles({1: (links, {4: (2, 0)}), 2: (plain, {})})
        self.assertEqual(wp.dl_parts(files, (1, 0)), [(2, 0)])
        self.assertEqual(wp.dl_parts(files, (2, 0)), [(2, 0)])
        self.assertEqual(wp.dl_parts(files, None), [])


class Materials(unittest.TestCase):
    def test_shade_alpha(self):
        # G_CC_MODULATEIA in cycle 1: alpha = TEXEL0 * SHADE reads the vertex alpha
        self.assertTrue(wp.shade_alpha((0x00121824, 0xFF33FFFF), False))
        # alpha = 0 0 0 TEXEL0: the vertices' alpha is never read
        self.assertFalse(wp.shade_alpha((0x00FFFE00 | (7 << 12) | (7 << 9), (7 << 12) | (1 << 9)), False))

    def test_plane_of_flat_models(self):
        self.assertEqual(wp.plane([-1, -1, 0], [1, 1, 0]), 'z')
        self.assertEqual(wp.plane([0, -1, -1], [0, 1, 1]), 'x')
        self.assertIsNone(wp.plane([-1, -1, -1], [1, 1, 1]))


class Tables(unittest.TestCase):
    def test_objects_are_unique_and_cover_the_classic_items(self):
        names = [o[0] for o in wp.OBJECTS]
        self.assertEqual(len(names), len(set(names)))
        for item in ['beam_sword', 'bat', 'fan', 'star_rod', 'ray_gun', 'fire_flower', 'hammer', 'maxim_tomato', 'heart', 'star',
                     'bob_omb', 'green_shell', 'red_shell', 'motion_bomb', 'bumper', 'poke_ball', 'capsule', 'crate', 'barrel']:
            self.assertIn(item, names)
        kinds = {o[0]: o[1] for o in wp.OBJECTS}
        self.assertEqual(kinds['fireball'], 'weapon')
        self.assertEqual(kinds['falcon_punch'], 'effect')

    def test_item_sounds_are_fgm_names(self):
        names = wp.snd.voice_names()
        for n in wp.ITEM_FGM:
            self.assertIn(n, names)


if __name__ == '__main__':
    unittest.main()
