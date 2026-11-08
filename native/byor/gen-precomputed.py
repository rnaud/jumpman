#!/usr/bin/env python3
"""Regenerate byor/sm64-us-sound-player.bin (+ .json): the assembled bytes of the decomp's
sound/sequences/00_sound_player.s (a decomp *source* file, not ROM data).  iOS has no assembler, so inproc.py
substitutes these bytes for the `clang -x assembler-with-cpp` step of scripts/render-music.py, after checking
that the .s and seq_macros.inc still hash to the recorded value.  Needs clang (Xcode CLT).

  gen-precomputed.py [--repo-root DIR]
"""
import importlib.util, json, subprocess, sys, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
root = Path(sys.argv[sys.argv.index('--repo-root') + 1]) if '--repo-root' in sys.argv else HERE.parents[1]
ref = root / 'reference/sm64'
sys.path.insert(0, str(HERE))
import inproc
spec = importlib.util.spec_from_file_location('render_music', root / 'scripts/render-music.py')
rm = importlib.util.module_from_spec(spec); spec.loader.exec_module(rm)
with tempfile.TemporaryDirectory() as t:
    o = Path(t) / 'sfx.o'
    subprocess.run(['clang', '-target', 'x86_64-unknown-linux-gnu', '-x', 'assembler-with-cpp', '-DVERSION_US', '-I', ref / 'include',
                    '-c', ref / 'sound/sequences/00_sound_player.s', '-o', o], check=True)
    blob = rm.elf_section(o, '.rodata')
(HERE / 'sm64-us-sound-player.bin').write_bytes(blob)
meta = {'source_sha256': inproc.sound_player_digest(ref / 'sound/sequences/00_sound_player.s', [ref / 'include']), 'length': len(blob)}
(HERE / 'sm64-us-sound-player.json').write_text(json.dumps(meta, indent=1) + '\n')
print('wrote sm64-us-sound-player.bin', len(blob), 'bytes', meta['source_sha256'][:12])
