"""ROM-free checks of the in-process layer (byor/inproc.py) that iOS relies on:  python3 -m unittest byor/test_inproc.py

 - the mini C preprocessor evaluates sounds.h exactly like the C compiler does (needs `cc`, i.e. a Mac);
 - the ELF stub that stands in for the assembled sound player round-trips through render-music's `elf_section`;
 - byor/seq_asm.py (the in-process stand-in for clang on 00_sound_player.s) handles its subset on small snippets, and
   assembles the real sound player to the same bytes as clang (needs clang, i.e. a Mac), also through the `clang` stub.
"""
import importlib.util, os, shutil, subprocess, sys, tempfile, unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import inproc  # noqa: E402
import seq_asm  # noqa: E402

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

    def test_sound_player_matches_clang(self):
        src, inc = REF / 'sound/sequences/00_sound_player.s', REF / 'include'
        mine = seq_asm.sound_player(REF)
        with tempfile.TemporaryDirectory() as t:
            stub = Path(t) / 'stub.o'
            rc, _, err = inproc._compile(['clang', '-x', 'assembler-with-cpp', '-DVERSION_US', '-I', str(inc), '-c', str(src),
                                          '-o', str(stub)], os.getcwd())
            self.assertEqual(rc, 0, err)
            self.assertEqual(render_music().elf_section(stub, '.rodata'), mine)
            if not shutil.which('clang'):
                self.skipTest('no clang')
            o = Path(t) / 'sfx.o'
            subprocess.run(['clang', '-target', 'x86_64-unknown-linux-gnu', '-x', 'assembler-with-cpp', '-DVERSION_US',
                            '-I', str(inc), '-c', str(src), '-o', str(o)], check=True)
            self.assertEqual(render_music().elf_section(o, '.rodata'), mine)


class SeqAsmTests(unittest.TestCase):
    def asm(self, s):
        return seq_asm.assemble_str('.section .rodata\n' + s + '\n', {}, ['VERSION_US'])

    def fails(self, s, msg, raw=False):
        with self.assertRaises(seq_asm.AsmError) as e:
            seq_asm.assemble_str(s, {}, ['VERSION_US']) if raw else self.asm(s)
        self.assertIn(msg, str(e.exception))

    def test_bytes_and_expressions(self):
        self.assertEqual(self.asm('.byte 1, 0x10, 0b11, 010, -1, 255'), bytes([1, 16, 3, 8, 0xff, 255]))
        self.assertEqual(self.asm('.byte 0x1234 >> 8, 0x1234 & 0xff, (0x80 | (0x1234 & 0x7f00) >> 8)'), bytes([0x12, 0x34, 0x92]))
        # GNU precedence: | and & bind tighter than + and -
        self.assertEqual(self.asm('.byte 1 + 2 | 4, 2 * 3 + 1, ~0 & 7, !0, (3 >= 2) + (1 == 2)'), bytes([7, 7, 7, 1, 1]))
        self.fails('.byte 256', 'out of range')
        self.fails('.byte -1 >> 8', 'negative')

    def test_labels_macros_and_conditionals(self):
        self.assertEqual(self.asm('start:\n.byte (fwd - start) >> 8, (fwd - start) & 0xff\nback: .byte back - start\n.byte 9\nfwd:\n.byte fwd'),
                         bytes([0, 4, 2, 9, 4]))
        src = ('s:\n.macro var x\n .if (\\x >= 0x80)\n  .byte (0x80 | (\\x & 0x7f00) >> 8), (\\x & 0xff)\n .else\n  .byte \\x\n .endif\n.endm\n'
               '.macro op a=3, b\n .byte 0x50 + \\a\n var \\b\n.endm\n'
               '.macro next v\n .byte \\v, (here\\@ - s)\n here\\@:\n.endm\n'
               'op , 5\nop 1, 0x123\nnext 7\nnext 8\n.macro e a, b\n.byte \\a, \\b\n.endm\ne 7 2000 - 1999\n.align 4, 0xee')
        self.assertEqual(self.asm(src), bytes([0x53, 5, 0x51, 0x81, 0x23, 7, 7, 8, 9, 7, 1, 0xee]))
        self.assertEqual(seq_asm.split_macro_args('46, 0xc, (75 - V), 20'), ['46', '0xc', '(75 - V)', '20'])
        self.assertEqual(self.asm('.set V, 10\n.byte (75 - V)\nW = 3\n.set V, 0\n.byte V + W'), bytes([65, 3]))
        self.fails('nope 1', 'unknown macro')
        self.fails('.word 1', 'unsupported')
        self.fails('.byte nowhere', 'undefined symbol nowhere')

    def test_preprocessor(self):
        inc = '.macro m\n#ifdef VERSION_EU\n.byte 1\n#else\n.byte 2\n#endif\n.endm\n'
        src = ('#include "m.inc"\n.section .rodata\n.align 0\nm // two\n/* skip\n.byte 9 */\n'
               '#if defined(VERSION_SH) || defined(VERSION_CN)\n.byte 3\n#else\n  #ifndef VERSION_JP\n.byte 4\n  #endif\n#endif\n'
               '#if defined(VERSION_US) && !defined(VERSION_EU)\n.byte 5\n#endif\n')
        self.assertEqual(seq_asm.assemble_str(src, {'m.inc': inc}, ['VERSION_US']), bytes([2, 4, 5]))
        self.fails('#define X 1\n', '#define', raw=True)
        self.fails('#if 1\n', 'unterminated', raw=True)
        self.fails('.byte 1\n', 'outside .section', raw=True)

if __name__ == '__main__':
    unittest.main()
