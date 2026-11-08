"""ROM-free checks of the in-process layer (byor/inproc.py) that iOS relies on:  python3 -m unittest byor/test_inproc.py

 - the mini C preprocessor evaluates sounds.h exactly like the C compiler does (needs `cc`, i.e. a Mac);
 - the ELF stub that stands in for the assembled sound player round-trips through render-music's `elf_section`;
 - the precomputed sound-player bytes still match the decomp source they were made from.
"""
import importlib.util, shutil, subprocess, sys, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import inproc  # noqa: E402

REF = ROOT / 'reference/sm64'


def render_music():
    spec = importlib.util.spec_from_file_location('render_music', ROOT / 'scripts/render-music.py')
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
    return m


@unittest.skipUnless((REF / 'include/sounds.h').is_file(), 'reference/sm64 not checked out')
class InprocTests(unittest.TestCase):
    def test_sound_ids_match_the_c_compiler(self):
        names = 'ACTION_TERRAIN_JUMP GENERAL_COIN MENU_STAR_SOUND MARIO_ATTACKED PEACH_MARIO MARIO_HERE_WE_GO'.split()
        src = '#include <stdio.h>\ntypedef unsigned int u32;\n#include "sounds.h"\nint main(void){\n' + \
              '\n'.join(f'printf("%u\\n", (unsigned)SOUND_{n});' for n in names) + '\n}\n'
        mine = inproc.sound_ids(src, ['VERSION_US'], [REF / 'include']).decode().split()
        if not shutil.which('cc'):
            self.skipTest('no C compiler')
        with tempfile.TemporaryDirectory() as t:
            (Path(t) / 'ids.c').write_text(src)
            subprocess.run(['cc', '-DVERSION_US', '-I', str(REF / 'include'), str(Path(t) / 'ids.c'), '-o', str(Path(t) / 'ids')], check=True)
            real = subprocess.check_output([str(Path(t) / 'ids')], text=True).split()
        self.assertEqual(mine, real)

    def test_elf_stub_roundtrip(self):
        blob = bytes(range(251)) * 3
        with tempfile.TemporaryDirectory() as t:
            f = Path(t) / 'x.o'
            f.write_bytes(inproc.make_elf_rodata(blob))
            self.assertEqual(render_music().elf_section(f, '.rodata'), blob)

    def test_precomputed_sound_player_is_current(self):
        meta = __import__('json').loads((HERE / 'sm64-us-sound-player.json').read_text())
        digest = inproc.sound_player_digest(REF / 'sound/sequences/00_sound_player.s', [REF / 'include'])
        self.assertEqual(digest, meta['source_sha256'], 'run byor/gen-precomputed.py')
        self.assertEqual((HERE / 'sm64-us-sound-player.bin').stat().st_size, meta['length'])


if __name__ == '__main__':
    unittest.main()
