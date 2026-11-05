"""Convert every extracted PCM AIFF sample to browser-playable WAV, losslessly.

extract_assets.py already decodes the ROM's VADPCM. Keep source tuning and loop
metadata in the manifest for future sequence playback; don't loop voice effects.
"""
import json
import math
import struct
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def convert(source, destination):
    data = source.read_bytes()
    if data[:4] != b'FORM' or data[8:12] != b'AIFF':
        raise ValueError(f'{source}: run the decomp extractor to decode VADPCM first')
    chunks = {}
    offset = 12
    while offset + 8 <= len(data):
        tag, size = struct.unpack_from('>4sI', data, offset)
        chunks[tag] = data[offset + 8:offset + 8 + size]
        offset += 8 + size + (size & 1)
    comm = chunks[b'COMM']
    channels, frames, bits, exponent, mantissa = struct.unpack('>hIhHQ', comm)
    rate = round(math.ldexp(mantissa, (exponent & 0x7fff) - 16383 - 63))
    if channels != 1 or bits != 16 or not 1000 <= rate <= 192000:
        raise ValueError(f'{source}: unexpected audio format')
    skip, _ = struct.unpack_from('>II', chunks[b'SSND'])
    pcm = chunks[b'SSND'][8 + skip:8 + skip + frames * 2]
    if len(pcm) != frames * 2:
        raise ValueError(f'{source}: truncated sample')
    little = bytearray(len(pcm))
    little[0::2], little[1::2] = pcm[1::2], pcm[0::2]
    destination.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(destination), 'wb') as out:
        out.setparams((channels, 2, rate, frames, 'NONE', 'not compressed'))
        out.writeframes(little)
    loop = None
    if b'MARK' in chunks and b'INST' in chunks:
        marks, pos = {}, 2
        for _ in range(struct.unpack_from('>H', chunks[b'MARK'])[0]):
            marker, frame, length = struct.unpack_from('>HIB', chunks[b'MARK'], pos)
            marks[marker] = frame
            pos += 6 + ((length + 2) & ~1)
        mode, begin, end = struct.unpack_from('>hhh', chunks[b'INST'], 8)
        if mode == 1 and begin in marks and end in marks:
            loop = [marks[begin] / rate, marks[end] / rate]
    return {'loop': loop, 'frames': frames, 'sampleRate': rate, 'duration': frames / rate,
            'instrumentHex': chunks.get(b'INST', b'').hex(), 'markersHex': chunks.get(b'MARK', b'').hex()}


def main():
    samples = ROOT / 'reference/sm64/sound/samples'
    manifest = {}
    for source in sorted(samples.rglob('*.aiff')):
        relative = Path(str(source.relative_to(samples).with_suffix('.wav')).replace('#', 'sharp'))
        manifest[str(relative)] = convert(source, ROOT / 'assets/audio' / relative)
    if not manifest:
        raise SystemExit('No extracted sound samples found')
    (ROOT / 'assets/audio/manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Converted all {len(manifest)} samples to PCM WAV')


if __name__ == '__main__':
    main()
