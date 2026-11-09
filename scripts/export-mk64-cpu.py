#!/usr/bin/env python3
"""Export the Mario Kart 64 CPU driver and racer data: public/mk64/cpu-ai.json.

Per course (the 16 race tracks, keyed by the course directory name):
  maxSeparation / minSeparation / steering   cpu_CourseMaximumSeparation (half track width the CPU may use),
                                             cpu_CourseMinimumSeparation, cpu_SteeringSensitivity
                                             (yamls/courses/<course>_metadata.yml)
  normalSpeed / curveSpeed / offTrackSpeed / slowSpeed
                                             cpu_NormalTargetSpeed, cpu_CurveTargetSpeed, cpu_OffTrackTargetSpeed and
                                             D_0D0096B8, four values each: 50cc, 100cc, 150cc, Extra
  behaviour                                  the CPUBehaviour list `cpu_behaviour_ptr` points at (common data segment 0x0D,
                                             MIO0 at ROM 0x132B50): [pathPointStart, pathPointEnd, type] rows up to the
                                             (-1, -1) terminator.  Types are CPU_BEHAVIOURS in include/defines.h
                                             (1 = drift through the curve, 2 = hop, 6 = normal speed, ...)
kart: the per-character tables of src/data/kart_attributes.c, in character order (Mario, Luigi, Yoshi, Toad,
D.K., Wario, Peach, Bowser) and per CC class (50cc, 100cc, 150cc, Extra, Battle).

Needs the MK64 decomp (reference/mk64) and the US 1.0 ROM.  Standard library only.

  export-mk64-cpu.py ROM [--output DIR]
"""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'reference/mk64'
COMMON_DATA_ROM = 0x132B50
SHA1 = '579c48e211ae952530ffc8738709f078d5dd215e'

# Race tracks: our directory name -> the decomp's yamls/courses stem.
COURSES = {
    'luigi_raceway': 'luigi_raceway', 'moo_moo_farm': 'moo_moo_farm', 'koopa_troopa_beach': 'koopa_beach',
    'kalimari_desert': 'kalimari_desert', 'toads_turnpike': 'toads_turnpike', 'frappe_snowland': 'frappe_snowland',
    'choco_mountain': 'choco_mountain', 'mario_raceway': 'mario_raceway', 'wario_stadium': 'wario_stadium',
    'sherbet_land': 'sherbet_land', 'royal_raceway': 'royal_raceway', 'bowsers_castle': 'bowsers_castle',
    'dks_jungle_parkway': 'dks_jungle_parkway', 'yoshi_valley': 'yoshi_valley', 'banshee_boardwalk': 'banshee_boardwalk',
    'rainbow_road': 'rainbow_road',
}
CHARACTERS = ['mario', 'luigi', 'yoshi', 'toad', 'dk', 'wario', 'peach', 'bowser']


def mio0(data):
    size, compressed, raw = struct.unpack_from('>III', data, 4)
    bits, mask, cursor = 0, 0, 16
    out = bytearray()
    while len(out) < size:
        if not mask:
            bits = data[cursor]
            cursor += 1
            mask = 128
        if bits & mask:
            out.append(data[raw])
            raw += 1
        else:
            pair = int.from_bytes(data[compressed:compressed + 2], 'big')
            compressed += 2
            for _ in range((pair >> 12) + 3):
                out.append(out[-((pair & 4095) + 1)])
        mask >>= 1
    return bytes(out[:size])


def number(text):
    return float(text.strip().rstrip('f'))


def course_metadata(stem):
    text = (SOURCE / f'yamls/courses/{stem}_metadata.yml').read_text()

    def scalar(key):
        return number(re.search(rf'^\s*{key}:\s*(-?[\d.]+f?)', text, re.M).group(1))

    def four(key):
        row = re.search(rf'^\s*{key}:\s*\[([^\]]*)\]', text, re.M).group(1)
        values = [number(v) for v in row.split(',')]
        assert len(values) == 4, (stem, key)
        return values

    pointer = re.search(r'^\s*cpu_behaviour_ptr:\s*(D_0D[0-9A-F]+)', text, re.M).group(1)
    return dict(
        maxSeparation=scalar('cpu_maximum_separation'), minSeparation=scalar('cpu_minimum_separation'),
        steering=int(scalar('cpu_steering_sensitivity')),
        normalSpeed=four('cpu_NormalTargetSpeed'), curveSpeed=four('cpu_CurveTargetSpeed'),
        offTrackSpeed=four('cpu_OffTrackTargetSpeed'), slowSpeed=four('D_0D0096B8'),
    ), pointer


def behaviour_offsets():
    """Symbol -> offset in the common data segment, from yamls/us/common_data.yml."""
    text = (SOURCE / 'yamls/us/common_data.yml').read_text()
    return {m.group(1): int(m.group(2), 16)
            for m in re.finditer(r'symbol:\s*(D_0D[0-9A-F]+)\n\s*type:\s*mk64:driving_behaviour\n\s*offset:\s*(0x[0-9A-Fa-f]+)', text)}


def behaviours(common, offset):
    rows = []
    for i in range(64):
        start, end, kind = struct.unpack_from('>hhi', common, offset + i * 8)
        if start == -1 and end == -1:
            break
        rows.append([start, end, kind])
    return rows


def float_arrays(text):
    """Name -> list of numbers for every `f32 name[] = { ... };` of kart_attributes.c."""
    arrays = {}
    for m in re.finditer(r'f32\s+(\w+)\[\]\s*=\s*\{(.*?)\};', text, re.S):
        body = re.sub(r'//[^\n]*', '', m.group(2))
        arrays[m.group(1)] = [number(v) for v in body.replace('\n', ' ').split(',') if v.strip()]
    return arrays


def pointer_tables(text):
    tables = {}
    for m in re.finditer(r'f32\*\s+(\w+)\[\d*\]\s*=\s*\{(.*?)\};', text, re.S):
        tables[m.group(1)] = [v.strip() for v in m.group(2).replace('\n', ' ').split(',') if v.strip()]
    return tables


def kart_tables():
    text = (SOURCE / 'src/data/kart_attributes.c').read_text()
    arrays, pointers = float_arrays(text), pointer_tables(text)

    def per_cc(table):  # 5 rows: 50cc, 100cc, 150cc, Extra, Battle
        return [arrays[n] for n in pointers[table]]

    return dict(
        characters=CHARACTERS,
        topSpeed=per_cc('gTopSpeedTable'),  # gTopSpeed50cc / 100cc / 150cc / Extra / Battle
        unk084=per_cc('D_800E2400'), unk088=per_cc('D_800E24B4'), unk210=per_cc('D_800E2568'),
        friction=arrays['gKartFrictionTable'], gravity=arrays['gKartGravityTable'],
        speedCap=arrays['gKartTopSpeedTable'], boundingBox=arrays['gKartBoundingBoxSizeTable'],
        acceleration=[arrays[n] for n in pointers['gKartAccelerationTables']],
        handling=arrays['gKartHandlingTable'], turnSpeedReduction0=arrays['gKartTurnSpeedReductionTable0'],
        turnSpeedReduction1=arrays['gKartTurnSpeedReductionTable1'], tripleABoost=arrays['gKartTripleABoost'],
    )


def export(rom_path, output):
    rom = rom_path.read_bytes()
    if rom[:4] == bytes.fromhex('37804012'):
        rom = bytes(b for i in range(0, len(rom), 2) for b in rom[i:i + 2][::-1])
    elif rom[:4] == bytes.fromhex('40123780'):
        rom = bytes(b for i in range(0, len(rom), 4) for b in rom[i:i + 4][::-1])
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise ValueError(f'Expected the US Mario Kart 64 ROM (SHA-1 {SHA1})')
    common = mio0(rom[COMMON_DATA_ROM:])
    offsets = behaviour_offsets()
    courses = {}
    for directory, stem in COURSES.items():
        meta, pointer = course_metadata(stem)
        meta['behaviour'] = behaviours(common, offsets[pointer])
        courses[directory] = meta
    output.mkdir(parents=True, exist_ok=True)
    data = dict(courses=courses, kart=kart_tables())
    (output / 'cpu-ai.json').write_text(json.dumps(data, separators=(',', ':')) + '\n')
    print(f'Exported CPU driver data for {len(courses)} courses and {len(CHARACTERS)} racers to {output / "cpu-ai.json"}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / 'public/mk64')
    args = parser.parse_args()
    export(args.rom.expanduser(), args.output)
