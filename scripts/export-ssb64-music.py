#!/usr/bin/env python3
"""Export the Super Smash Bros. (US) music from a local ROM: every BGM sequence and the music instrument bank.

Formats follow VetriTheRetri/ssb-decomp-re (reference/ssb64, MUSIC_AND_SFX_DISCOVERIES.md, see native/SSB64.md "Music"):
  * S1_music.sbk (ROM 0xB277B0, smashbrothers.us.yaml): a libultra ALSeqFile (`s16 revision 'S1'; s16 seqCount;
    {u32 offset; s32 len} seqArray[]`, include/PR/libaudio.h). Each sequence is an ALCMidiHdr compact-MIDI file
    (16 track offsets + division), played by n_alCSPlay (src/libultra/n_audio) - the same format Banjo-Kazooie uses.
  * B1_sounds1.ctl / .tbl (0xB4E5C0 / 0xB54CE0): the ALBankFile of the music instruments (VADPCM wavetables).
The BGM id is the sequence index (syAudioPlayBGM, src/sys/audio.c); names come from the gmMusicID enum
(src/gm/gmsound.h). Standard library only, deterministic; native/byor-rs/src/ssb64_music.rs is its byte-identical port.

Writes <out>/seq_NN.bin (one per BGM), bank.ctl, bank.tbl and music.json (tracks plus the menu / victory themes).
"""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECOMP = ROOT / 'reference/ssb64'
ROM = ROOT / 'reference/roms/ssb64-us.z64'
OUT = ROOT / 'assets/audio/music/ssb64'
SHA1 = 'e2929e10fccc0aa84e5776227e798abc07cedabf'

SBK = (0xB277B0, 0xB4E5C0)   # S1_music.sbk
CTL = (0xB4E5C0, 0xB54CE0)   # B1_sounds1.ctl
TBL = (0xB54CE0, 0xC6B650)   # B1_sounds1.tbl (up to B1_sounds2.ctl)
RATE = 32000                 # dSYAudioPublicSettings output rate; the bank's sampleRate is the same
VOLUME = 0x7800              # syAudioSetBGMVolume's ceiling, the level the game plays the BGM at

# Themes of the menus the game's own menu music can use (gmMusicID names without `nSYAudioBGM`).
MENU = {'menu': 'ModeSelect', 'select': 'BattleSelect', 'results': 'Results', 'training': 'TrainingMode', 'data': 'Data',
        'opening': 'Opening', 'credits': 'Staffroll', 'stage_clear': '1PStageClear', 'game_over': '1PGameOver'}
# The victory fanfare of each fighter (scVSResults: the winner's series theme).
VICTORY = {'mario': 'WinMario', 'luigi': 'WinMario', 'fox': 'WinFox', 'donkey': 'WinDonkey', 'samus': 'WinMetroid',
           'link': 'WinZelda', 'yoshi': 'WinYoshi', 'captain': 'WinFZero', 'kirby': 'WinKirby', 'pikachu': 'WinPMonsters',
           'purin': 'WinPMonsters', 'ness': 'WinMother'}


def music_names(decomp=DECOMP):
    """gmMusicID (src/gm/gmsound.h) names in enum order, without `nSYAudioBGM`."""
    text = (decomp / 'src/gm/gmsound.h').read_text()
    body = text[text.index('typedef enum gmMusicID'):]
    body = body[body.index('{') + 1:body.index('}')]
    names = []
    for line in body.split('\n'):
        line = line.split('//')[0].strip()
        for part in line.split(','):
            part = part.split('=')[0].strip()
            if re.fullmatch(r'nSYAudioBGM\w+', part) and part != 'nSYAudioBGMEnd':
                names.append(part[len('nSYAudioBGM'):])
    return names


def sequences(sbk):
    """The ALSeqFile's sequences: [(bytes)]."""
    rev, count = struct.unpack_from('>hh', sbk, 0)
    if rev != 0x5331:
        raise ValueError('S1_music.sbk: not an ALSeqFile')
    out = []
    for i in range(count):
        off, length = struct.unpack_from('>Ii', sbk, 4 + 8 * i)
        out.append(sbk[off:off + length])
    return out


def export(rom, out, decomp=DECOMP):
    out.mkdir(parents=True, exist_ok=True)
    names = music_names(decomp)
    seqs = sequences(rom[SBK[0]:SBK[1]])
    tracks = []
    for i, data in enumerate(seqs):
        file = f'seq_{i:02d}.bin'
        (out / file).write_bytes(data)
        division = struct.unpack_from('>I', data, 0x40)[0]
        live = sum(1 for k in range(16) if struct.unpack_from('>I', data, 4 * k)[0])
        tracks.append({'id': i, 'file': file, 'name': names[i] if i < len(names) else f'Seq{i}', 'bytes': len(data),
                       'division': division, 'channels': live, 'volume': VOLUME})
    (out / 'bank.ctl').write_bytes(rom[CTL[0]:CTL[1]])
    (out / 'bank.tbl').write_bytes(rom[TBL[0]:TBL[1]])
    by_name = {t['name']: t['id'] for t in tracks}
    doc = {'game': 'ssb64', 'rate': RATE, 'tracks': tracks,
           'menu': {k: by_name[v] for k, v in MENU.items() if v in by_name},
           'victory': {k: by_name[v] for k, v in VICTORY.items() if v in by_name}}
    (out / 'music.json').write_text(json.dumps(doc, indent=1, sort_keys=True) + '\n')
    return doc


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rom', type=Path, default=ROM)
    ap.add_argument('--out', type=Path, default=OUT)
    a = ap.parse_args()
    rom = a.rom.read_bytes()
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise SystemExit(f'{a.rom} is not Super Smash Bros. (US) (sha1 {SHA1})')
    doc = export(rom, a.out)
    print(f'{len(doc["tracks"])} sequences', flush=True)


if __name__ == '__main__':
    main()
