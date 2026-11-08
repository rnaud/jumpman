#!/usr/bin/env python3
"""Upgrade-path tests for byor/pipeline.py (no ROMs, no decomp trees): python3 -m unittest byor/test_pipeline.py

The game steps and staging are replaced by fakes; install(), stamps, hashes, remembered ROMs and the stale logic
are the real code.
"""
import json, os, sys, tempfile, unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pipeline  # noqa: E402

HASH = {'sm64': 'h-sm64-v2', 'mk64': 'h-mk64'}


class Upgrade(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix='byor-test-'))
        self.src, self.data = self.tmp / 'src', self.tmp / 'data'
        self.src.mkdir()
        (self.data / 'stamps').mkdir(parents=True)
        self.set_hashes(HASH)
        self.rom = self.tmp / 'game.z64'
        self.rom.write_bytes(b'rom')
        self.out = {}  # game -> files its fake step writes this run
        self.fail = False
        self._saved = (pipeline.stage, dict(pipeline.DO), pipeline.identify, sys.argv)
        pipeline.stage = self.fake_stage
        pipeline.identify = lambda p: ('sm64' if Path(p).name.startswith('game') else None, b'x')
        for g in pipeline.DO:
            pipeline.DO[g] = self.fake_step(g)

    def tearDown(self):
        pipeline.stage, pipeline.DO, pipeline.identify, sys.argv = self._saved[0], self._saved[1], self._saved[2], self._saved[3]
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def set_hashes(self, h):
        (self.src / 'exporters.json').write_text(json.dumps({'version': pipeline.VERSION, 'games': h}))

    def fake_stage(self, src, stg, games):
        (stg / 'assets').mkdir(parents=True)
        (stg / 'public').mkdir()
        (stg / 'assets/ui').mkdir()
        (stg / 'assets/ui/font.ttf').write_text('original')

    def fake_step(self, game):
        def step(stg, rom, game=game):
            for rel, body in self.out.get(game, {}).items():
                p = stg / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(body)
                if self.fail and rel == self.fail:
                    raise RuntimeError('extraction died midway')
        return step

    def run_main(self, *extra):
        sys.argv = ['pipeline.py', '--source-root', str(self.src), '--data-dir', str(self.data), *extra]
        from io import StringIO
        old, sys.stdout = sys.stdout, StringIO()
        try:
            rc = pipeline.main()
            return rc, sys.stdout.getvalue()
        finally:
            sys.stdout = old

    def stamp(self, game='sm64'):
        return json.loads((self.data / 'stamps' / f'{game}.json').read_text())

    def install_v2(self):
        """What an old app left behind: a v2 stamp (no hash, no file list) and its files."""
        (self.data / 'stamps/sm64.json').write_text(json.dumps({'version': 2, 'game': 'sm64', 'time': 1}))
        (self.data / 'assets/courses').mkdir(parents=True)
        (self.data / 'assets/courses/old.json').write_text('old')

    def test_v2_stamp_is_stale_and_reimport_reuses_the_remembered_rom(self):
        self.install_v2()
        self.assertEqual(pipeline.stamp_state(self.data, 'sm64', HASH['sm64']), 'stale')
        # no remembered ROM yet (v2 never recorded one): nothing to reuse
        self.assertIsNone(pipeline.remembered_rom(self.data, 'sm64'))
        self.out['sm64'] = {'assets/courses/bob.json': 'new'}
        rc, text = self.run_main('--sm64', str(self.rom))
        self.assertEqual(rc, 0, text)
        self.assertEqual(pipeline.stamp_state(self.data, 'sm64', HASH['sm64']), 'current')
        self.assertEqual(self.stamp()['hash'], HASH['sm64'])
        self.assertEqual(self.stamp()['files'], ['assets/courses/bob.json'])
        self.assertTrue((self.data / 'assets/ui/font.ttf').is_file())
        # the exporters change later -> only that game is stale, and its ROM is found again
        self.set_hashes({**HASH, 'sm64': 'h-sm64-v3'})
        exp = pipeline.expected_hashes(self.src)
        self.assertEqual(pipeline.stamp_state(self.data, 'sm64', exp['sm64']), 'stale')
        self.assertEqual(pipeline.stamp_state(self.data, 'mk64', exp['mk64']), 'missing')
        rc, text = self.run_main('--status')
        self.assertIn(f'@@STATUS sm64 stale {self.rom.resolve()}', text)
        self.assertIn('@@STATUS mk64 missing\n', text)

    def test_moved_or_replaced_rom_is_not_reused(self):
        self.out['sm64'] = {'assets/courses/bob.json': 'new'}
        self.assertEqual(self.run_main('--sm64', str(self.rom))[0], 0)
        self.set_hashes({**HASH, 'sm64': 'changed'})
        self.rom.rename(self.tmp / 'moved.z64')
        self.assertIn('@@STATUS sm64 stale\n', self.run_main('--status')[1])

    def test_swap_removes_files_the_new_exporters_no_longer_write(self):
        self.out['sm64'] = {'assets/courses/a.json': '1', 'assets/courses/b.json': '1'}
        self.run_main('--sm64', str(self.rom))
        self.assertTrue((self.data / 'assets/courses/b.json').is_file())
        self.out['sm64'] = {'assets/courses/a.json': '2', 'assets/extra/c.json': '2'}
        self.assertEqual(self.run_main('--sm64', str(self.rom))[0], 0)
        self.assertEqual((self.data / 'assets/courses/a.json').read_text(), '2')
        self.assertFalse((self.data / 'assets/courses/b.json').exists())
        self.assertTrue((self.data / 'assets/ui/font.ttf').is_file(), 'original assets are never removed')

    def test_failed_import_leaves_the_previous_install_intact(self):
        self.out['sm64'] = {'assets/courses/a.json': 'good', 'assets/courses/b.json': 'good'}
        self.run_main('--sm64', str(self.rom))
        before = self.stamp()
        self.set_hashes({**HASH, 'sm64': 'newer'})
        self.out['sm64'] = {'assets/courses/a.json': 'half-written', 'assets/courses/z.json': 'half'}
        self.fail = 'assets/courses/z.json'
        rc, text = self.run_main('--sm64', str(self.rom))
        self.assertEqual(rc, 1)
        self.assertIn('@@FAIL sm64', text)
        self.assertEqual((self.data / 'assets/courses/a.json').read_text(), 'good')
        self.assertTrue((self.data / 'assets/courses/b.json').is_file())
        self.assertFalse((self.data / 'assets/courses/z.json').exists())
        self.assertEqual(self.stamp(), before)
        self.assertFalse((self.data / '.stage').exists(), 'the stage dir is cleaned up')

    def test_files_shared_by_two_games_survive_one_games_reimport(self):
        shared = 'assets/audio/voices.json'
        self.out['sm64'] = {shared: 'v'}
        self.out['mk64'] = {shared: 'v', 'public/mk64/x.json': '1'}
        pipeline.identify = lambda p: (Path(p).stem, b'x')
        a, b = self.tmp / 'sm64.z64', self.tmp / 'mk64.z64'
        a.write_bytes(b'a'); b.write_bytes(b'b')
        self.assertEqual(self.run_main('--sm64', str(a), '--mk64', str(b))[0], 0)
        self.out['sm64'] = {}
        self.assertEqual(self.run_main('--sm64', str(a))[0], 0)
        self.assertTrue((self.data / shared).is_file(), 'mk64 still owns it')

    def test_game_hashes_follow_script_and_manifest_contents(self):
        root = self.tmp / 'root'
        (root / 'scripts/music').mkdir(parents=True)
        (root / 'reference/oot/src').mkdir(parents=True)
        for rel in sum(pipeline.GAME_SCRIPTS.values(), []):
            (root / 'scripts' / rel).write_text('x')
        (root / 'reference/oot/src/a.c').write_text('one')
        man = self.tmp / 'manifest.txt'
        man.write_text('oot/src/a.c\n')
        h1 = pipeline.game_hashes(root, man)
        (root / 'reference/oot/src/a.c').write_text('two')
        h2 = pipeline.game_hashes(root, man)
        self.assertNotEqual(h1['oot'], h2['oot'])
        self.assertEqual({g: h1[g] for g in ('sm64', 'mk64', 'bk')}, {g: h2[g] for g in ('sm64', 'mk64', 'bk')})
        (root / 'scripts/mk64_courses.py').write_text('changed')
        self.assertNotEqual(pipeline.game_hashes(root, man)['mk64'], h2['mk64'])
        # the gzipped audio template hashes like the plain file
        plain = pipeline.game_hashes(root, man)['sm64']
        import gzip
        (root / 'scripts/music/sm64-us-template.wasm').rename(root / 'scripts/music/t')
        (root / 'scripts/music/sm64-us-template.wasm.gz').write_bytes(gzip.compress(b'x'))
        self.assertEqual(pipeline.game_hashes(root, man)['sm64'], plain)


if __name__ == '__main__':
    unittest.main()
