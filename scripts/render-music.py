"""Render the extracted SM64 level sequences into compact looping music stems.

This is an adaptation, not an emulator: it reads the decomp sequence disassembly,
takes the richest layer for each instrument program, and pitch-shifts the extracted
instrument samples to the written notes.
"""
from __future__ import annotations

import json
import math
import re
import subprocess
import wave
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
PY = ROOT / 'reference/sm64/tools/seq_decoder.py'
OUT = ROOT / 'assets/audio/music'
SAMPLES = ROOT / 'assets/audio/instruments'
BANK = json.loads((ROOT / 'reference/sm64/sound/sound_banks/0D.json').read_text())
TRACKS = {
    'grass': '03_level_grass',
    'inside': '04_level_inside_castle',
    'slide': '09_level_slide',
}
RATE = 32000
SECONDS = 32.0
TICK = 1 / 60


def midi_from_name(name: str) -> int | None:
    m = re.search(r'_([A-G])([#b]?)(-?\d+)', name)
    if not m:
        return None
    pitch = {'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11}[m.group(1)]
    pitch += {'': 0, '#': 1, 'b': -1}[m.group(2)]
    return 12 * (int(m.group(3)) + 1) + pitch


def load_sample(name: str):
    path = SAMPLES / f'{name}.wav'
    if not path.exists():
        return None
    with wave.open(str(path), 'rb') as wav:
        channels = wav.getnchannels()
        rate = wav.getframerate()
        raw = wav.readframes(wav.getnframes())
    data = np.frombuffer(raw, dtype='<i2').astype(np.float32) / 32768
    if channels > 1:
        data = data.reshape(-1, channels).mean(axis=1)
    return rate, data, midi_from_name(name)


def parse_sequence(seq: str):
    text = subprocess.check_output(['python3', str(PY), str(ROOT / 'assets/extracted/sound/sequences/us' / f'{seq}.m64')], text=True)
    chans = {}
    current = None
    for line in text.splitlines():
        m = re.match(r'\.(chan_[0-9A-Fa-f]+):', line)
        if m:
            current = m.group(1)
            chans[current] = {'program': 0, 'delay': 0, 'layers': []}
            continue
        if not current:
            continue
        if line.startswith('chan_setinstr'):
            chans[current]['program'] = int(line.split()[1], 0)
        elif line.startswith('chan_delay'):
            chans[current]['delay'] = int(line.split()[1], 0)
        m = re.match(r'chan_setlayer \d+, \.(\w+)', line)
        if m:
            chans[current]['layers'].append(m.group(1))

    layers = {}
    current_layer = None
    for line in text.splitlines():
        m = re.match(r'\.(layer_[0-9A-Fa-f]+):', line)
        if m:
            current_layer = m.group(1)
            layers[current_layer] = []
            continue
        if current_layer and line.startswith('layer_note'):
            layers[current_layer].append(line)

    best = {}
    for channel in chans.values():
        for layer in channel['layers']:
            notes = layers.get(layer, [])
            if len(notes) > best.get(channel['program'], (0, 0, None))[0]:
                best[channel['program']] = (len(notes), channel['delay'], layer)
    return best, layers


def events_for(layer_lines, start_ticks):
    t = start_ticks
    last_delay = 64
    events = []
    for line in layer_lines:
        m = re.match(r'layer_note([012]) (.*)', line)
        if not m:
            continue
        kind = int(m.group(1))
        a = [int(x, 0) for x in m.group(2).split(',')]
        if kind == 0:
            note, delay, velocity, duration = a
        elif kind == 1:
            note, delay, velocity = a
            duration = delay
        else:
            note, velocity, duration = a
            delay = last_delay
        last_delay = delay
        events.append((t, note, velocity, max(1, duration)))
        t += delay
    return events


def sample_for(program: int, note: int):
    inst = BANK['instruments'].get(f'inst{program}')
    if not inst:
        inst = BANK['instruments']['inst0']
    normal_lo = inst.get('normal_range_lo', 0)
    name = inst.get('sound_lo') if note < normal_lo and 'sound_lo' in inst else inst.get('sound')
    if isinstance(name, dict):
        name = name.get('sample')
    return load_sample(str(name)) if name else None


def render(seq: str, path: Path):
    best, layers = parse_sequence(seq)
    chosen = []
    for program, (count, delay, layer) in sorted(best.items(), key=lambda item: -item[1][0]):
        if count == 0 or len(chosen) >= 4:
            continue
        chosen.append((program, delay, layer, count))
    total = int(RATE * SECONDS)
    mix = np.zeros(total, dtype=np.float32)
    print(seq, [(p, l, c) for p, _, l, c in chosen])
    for program, delay, layer, _ in chosen:
        sample = sample_for(program, 60)
        if not sample:
            print('  missing sample', program)
            continue
        rate, data, base = sample
        if base is None:
            base = 60
        for start_tick, note, velocity, duration in events_for(layers[layer], delay):
            start = start_tick * TICK
            if start >= SECONDS:
                break
            ratio = 2 ** ((note - base) / 12)
            count = min(len(data), int(rate / ratio * min(2.0, duration * TICK)))
            if count < 8:
                continue
            source = np.interp(
                np.arange(count) * ratio,
                np.arange(len(data)),
                data,
            ).astype(np.float32)
            fade = min(len(source), int(RATE * 0.02))
            if fade > 0:
                source[:fade] *= np.linspace(0, 1, fade)
                source[-fade:] *= np.linspace(1, 0, fade)
            at = int(start * RATE)
            end = min(total, at + len(source))
            if end <= at:
                continue
            gain = (velocity / 127) * (0.14 if program == 0 else 0.09)
            mix[at:end] += source[:end - at] * gain
    peak = float(np.max(np.abs(mix))) or 1.0
    mix *= 0.85 / peak
    pcm = (mix * 32767).astype('<i2')
    with wave.open(str(path), 'wb') as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(pcm.tobytes())
    print('wrote', path, round(len(pcm) / RATE, 1), 's peak', round(peak, 3))


if __name__ == '__main__':
    OUT.mkdir(parents=True, exist_ok=True)
    for name, seq in TRACKS.items():
        render(seq, OUT / f'{name}.wav')
