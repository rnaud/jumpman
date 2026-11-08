"""Export Ocarina of Time (US 1.2) item / combat / system / enemy / environment sound effects from the local ROM.

Writes mono 16-bit WAVs to assets/audio/voices/oot_sfx/<NAME>_0.wav (NAME = the decomp's NA_SE_* name without the
prefix, e.g. IT_SWORD_STRIKE) plus assets/audio/voices/manifest-oot-sfx.json (rate, duration, peak, rms, sample sources,
and the sfx ids that were *not* decoded with the reason).

How a sound is made (same Audiobank / Audiotable VADPCM decode as `export-character-voices.py`):

* `include/tables/sfx/*_table.h` (zeldaret/oot, `reference/oot`) names the channel script (CHAN_xxxx) of every NA_SE_*.
* `assets/audio/sequences/seq_0.prg.seq` holds the channel / layer scripts. A small interpreter follows `instr`
  (SF0_INST_n of Soundfont 0), `transpose`, `env`, `notedv / notedvg / noteldv / notevg`, delays, `ldlayer`, `jump / rjump / call`
  and renders each note: sample (low / normal / high split by key range) at tuning * 32000 * 2^((key - 39) / 12) Hz,
  looped samples repeated until the note ends, the ADSR envelope (instrument default or the script's `env`) applied with
  1 envelope tick = 1/180 s and an approximate release time, mixed on a timeline at 16000 Hz.
* Not modelled: vibrato, portamento (pitch slide), pan, reverb, channel volume, random-choice (`rand` / `dyntbl`) scripts.
  A sound whose script needs one of those but still has a note is decoded from its first notes; a sound with no decodable
  note is listed under `skipped`.

Clips are peak-normalised to 26000 (the ROM's relative loudness is not kept). The output is deterministic.
A missing ROM (or an OoT checkout without the sequence file) leaves the set out.
"""
import argparse
import hashlib
import importlib.util
import json
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT_RATE = 16000
ENV_TICK = 1 / 180
TICKS_PER_SECOND = 96          # tempo 120, 48 tatums per beat
MAX_SECONDS = 1.8
CLIP_SECONDS = 2.0
PEAK = 26000

# NA_SE_<name> -> exported as <name>. Chosen for items, combat, pickups, webs / switches / pots and enemy hit / death.
WANTED = '''
IT_SWORD_IMPACT IT_SWORD_SWING IT_SWORD_PUTAWAY IT_SWORD_PICKOUT IT_ARROW_SHOT IT_BOOMERANG_THROW IT_SHIELD_BOUND
IT_BOW_DRAW IT_SHIELD_REFLECT_SW IT_ARROW_STICK_HRAD IT_HAMMER_HIT IT_HOOKSHOT_CHAIN IT_SHIELD_REFLECT_MG IT_BOMB_IGNIT
IT_BOMB_EXPLOSION IT_BOMB_UNEXPLOSION IT_BOOMERANG_FLY IT_SWORD_STRIKE IT_HAMMER_SWING IT_HOOKSHOT_REFLECT
IT_ARROW_STICK_CRE IT_ARROW_STICK_OBJ IT_SWORD_SWING_HARD IT_WALL_HIT_HARD IT_WALL_HIT_SOFT IT_STONE_HIT IT_WOODSTICK_BROKEN
IT_SHIELD_POSTURE IT_SLING_SHOT IT_SLING_DRAW IT_SWORD_CHARGE IT_ROLLING_CUT IT_SWORD_STRIKE_HARD IT_SLING_REFLECT
IT_HOOKSHOT_READY IT_HOOKSHOT_RECEIVE IT_HOOKSHOT_STICK_OBJ IT_SWORD_REFLECT_MG IT_DEKU IT_ROLLING_CUT_LV1
IT_ROLLING_CUT_LV2 IT_BOW_FLICK IT_BOMBCHU_MOVE IT_SLING_FLICK IT_SWORD_STICK_STN IT_REFLECTION_WOOD IT_MAGIC_ARROW_SHOT
IT_EXPLOSION_FRAME IT_EXPLOSION_ICE IT_EXPLOSION_LIGHT IT_FLAME IT_MASTER_SWORD_SWING
SY_GET_RUPY SY_GET_ITEM SY_HP_RECOVER SY_ERROR SY_DECIDE SY_CURSOR SY_CANCEL SY_LOCK_ON SY_LOCK_ON_HUMAN SY_FOUND
SY_HIT_SOUND SY_CORRECT_CHIME SY_TRE_BOX_APPEAR SY_PIECE_OF_HEART SY_GET_BOXITEM SY_SET_FIRE_ARROW SY_SET_ICE_ARROW
SY_SET_LIGHT_ARROW SY_RUPY_COUNT SY_HITPOINT_ALARM SY_GLASSMODE_ON SY_GLASSMODE_OFF SY_ATTENTION_ON
PL_DAMAGE PL_BODY_HIT PL_CATCH_BOOMERANG PL_CHANGE_ARMS PL_TAKE_OUT_SHIELD PL_PULL_UP_POT PL_PULL_UP_WOODBOX
PL_PULL_UP_PLANT PL_PULL_UP_ROCK PL_THROW PL_MAGIC_FIRE PL_MAGIC_WIND_NORMAL PL_MAGIC_WIND_WARP PL_MAGIC_SOUL_NORMAL
PL_ARROW_CHARGE_FIRE PL_ARROW_CHARGE_ICE PL_ARROW_CHARGE_LIGHT PL_ICE_BROKEN PL_FREEZE PL_SPARK PL_KNOCK PL_ROLL
PL_PUT_OUT_ITEM PL_SWORD_CHARGE PL_BODY_BOUND PL_CLIMB_CLIFF
EV_EXPLOSION EV_DOOR_OPEN EV_DOOR_CLOSE EV_SLIDE_DOOR_OPEN EV_SLIDE_DOOR_CLOSE EV_FOOT_SWITCH EV_TBOX_UNLOCK EV_TBOX_OPEN
EV_FLAME_IGNITION EV_TORCH EV_BOMB_BOUND EV_BOX_BREAK EV_HAMMER_SWITCH EV_BLOCK_BOUND EV_PLANT_BROKEN EV_ROCK_BROKEN
EV_WEB_VIBRATION EV_WEB_BROKEN EV_WOODBOX_BREAK EV_POT_BROKEN EV_WALL_BROKEN EV_DIAMOND_SWITCH EV_BURNING EV_ICE_FREEZE
EV_ICE_BROKEN EV_STONE_BOUND EV_TREE_CUT EV_WOOD_HIT EV_BLOCK_RISING EV_BURN_OUT EV_FLAME_OF_FIRE EV_RED_EYE
EN_DEKU_DAMAGE EN_DEKU_DEAD EN_DEKU_ATTACK EN_DEKU_MOUTH EN_DEKU_JR_DEAD EN_NUTS_DAMAGE EN_NUTS_DEAD EN_NUTS_THROW EN_NUTS_UP
EN_NUTS_DOWN EN_NUTS_FAINT EN_EXTINCT EN_TEKU_DAMAGE EN_TEKU_DEAD EN_STAL_DAMAGE EN_STAL_DEAD EN_STAL_WALK EN_STALKID_DAMAGE
EN_STALKID_DEAD EN_STALKID_ATTACK EN_RIZA_DAMAGE EN_RIZA_DEAD EN_RIZA_ATTACK EN_WOLFOS_DAMAGE EN_WOLFOS_DEAD EN_WOLFOS_ATTACK
EN_REDEAD_DAMAGE EN_REDEAD_DEAD EN_REDEAD_ATTACK EN_OCTAROCK_ROCK EN_OCTAROCK_DEAD1 EN_OCTAROCK_DEAD2 EN_OCTAROCK_JUMP
EN_OCTAROCK_LAND EN_OCTAROCK_BUBLE EN_BUBLE_WING EN_BUBLE_BITE EN_BUBLE_DEAD EN_BUBLE_DAMAGE EN_MORIBLIN_DEAD
EN_MORIBLIN_ATTACK EN_PO_DAMAGE EN_PO_DEAD EN_BARI_DAMAGE EN_BARI_DEAD EN_FFLY_DEAD EN_FFLY_ATTACK EN_BIRI_DEAD
EN_BIRI_SPARK EN_LIKE_DAMAGE EN_LIKE_DEAD EN_GOMA_JR_DAM1 EN_GOMA_JR_DEAD EN_GOMA_DAM1 EN_GOMA_DEAD EN_GOMA_BJR_DAM1
EN_GOMA_BJR_DEAD EN_FLAME_DAMAGE EN_ANUBIS_DEAD EN_FREEZAD_DAMAGE EN_FREEZAD_DEAD EN_DODO_J_DAMAGE EN_DODO_J_DEAD
EN_VALVAISA_DAMAGE1 EN_VALVAISA_DEAD EN_AMOS_DAMAGE EN_AMOS_DEAD EN_SHELL_DEAD EN_BUBLEWALK_DAMAGE EN_BUBLEWALK_DEAD
EN_GERUDOFT_DAMAGE EN_GERUDOFT_DEAD EN_DEADHAND_DAMAGE EN_DEADHAND_DEAD EN_KDOOR_BREAK EN_KDOOR_HIT
EN_PIHAT_DAMAGE EN_PIHAT_SM_DEAD EN_PIHAT_FLY EN_PIHAT_LAND EN_TEKU_WALK EN_STALTU_DAMAGE EN_STALWALL_DEAD EN_KAICHO_DEAD
EN_KAICHO_CRY EN_KAICHO_ATTACK EN_RIVA_DAMAGE EN_RIVA_DEAD EN_RIVA_APPEAR EN_GOMA_JR_DAM2 EN_NUTS_CUTBODY EN_WOLFOS_CRY
EN_GOMA_JR_FREEZE EN_DEKU_SCRAPE EN_STALTU_DOWN EN_OCTAROCK_SINK EN_NUTS_WALK
'''.split()


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def parse_pitches(oot):
    return {m[1]: int(m[2]) for m in re.finditer(r'#define (PITCH_\w+)\s+(\d+)', (oot / 'include/audio/aseq.h').read_text())}


def parse_tables(oot):
    chans = {}
    for table in (oot / 'include/tables/sfx').glob('*_table.h'):
        for m in re.finditer(r'DEFINE_SFX\((\w+), (NA_SE_\w+),', table.read_text()):
            chans.setdefault(m[2], m[1])
    return chans


class Seq:
    """Blocks of seq_0.prg.seq: name -> ('channel' | 'layer' | 'envelope', [(op, [args])]) and label -> (block, index)."""

    def __init__(self, text):
        self.blocks, self.labels = {}, {}
        cur = None
        for line in text.splitlines():
            m = re.match(r'\.(channel|layer|envelope)\s+(\w+)', line)
            if m:
                cur = self.blocks[m[2]] = (m[1], [])
                self.labels[m[2]] = (m[2], 0)
                continue
            if cur is None:
                continue
            m = re.match(r'(\w+):\s*$', line)
            if m:
                self.labels[m[1]] = (next(k for k, v in self.blocks.items() if v is cur), len(cur[1]))
                continue
            m = re.match(r'/\*.*?\*/\s+([.\w]+)\s*(.*?)\s*(?://.*)?$', line)
            if m:
                args = re.sub(r'/\*.*?\*/', '', m[2])
                args = re.sub(r'FRAMERATE_CONST\((\w+),\s*\w+\)', r'\1', args)
                cur[1].append((m[1], [a.strip() for a in args.split(',') if a.strip()]))
            elif line.startswith('\t') or line.startswith('    '):
                parts = line.split()
                if parts and cur[0] == 'envelope':
                    cur[1].append((parts[0], [a.strip() for a in ' '.join(parts[1:]).split(',') if a.strip()]))


def small_adpcm(book, data):
    """CODEC_SMALL_ADPCM: 16 samples in 5 bytes (header scale / predictor + sixteen 2-bit residuals, MSB first)."""
    order, npred = struct.unpack_from('>ii', book, 0)
    table = struct.unpack_from(f'>{8 * order * npred}h', book, 8)
    books = [[table[p * 16 + o * 8:p * 16 + o * 8 + 8] for o in range(2)] for p in range(npred)]
    out, h2, h1 = [], 0, 0
    for off in range(0, len(data) - 4, 5):
        scale, pred = data[off] >> 4, data[off] & 15
        t0, t1 = books[pred % npred]
        res = []
        for b in data[off + 1:off + 5]:
            res += [(b >> 6) & 3, (b >> 4) & 3, (b >> 2) & 3, b & 3]
        res = [(n - 4 if n >= 2 else n) << scale for n in res]
        for half in (0, 8):
            e, y = res[half:half + 8], []
            for i in range(8):
                acc = (e[i] << 11) + h2 * t0[i] + h1 * t1[i]
                for j in range(i):
                    acc += e[j] * t1[i - 1 - j]
                y.append(max(-32768, min(32767, acc >> 11)))
            out += y
            h2, h1 = y[6], y[7]
    return out


class Font:
    def __init__(self, bank, table, offset, size):
        self.font, self.table = bank[offset:offset + size], table
        self.cache = {}

    def sample(self, ptr, tuning, voc):
        key = (ptr, tuning)
        if key not in self.cache:
            font = self.font
            bits, addr, loop, book = struct.unpack_from('>IIII', font, ptr)
            codec, size = bits >> 28, bits & 0xFFFFFF
            if codec not in (0, 3):
                self.cache[key] = None
                return None
            order, npred = struct.unpack_from('>ii', font, book)
            decode = voc.vadpcm if codec == 0 else small_adpcm
            pcm = decode(font[book:book + 8 + 16 * order * npred], self.table[addr:addr + size])
            lstart, lend, lcount = struct.unpack_from('>III', font, loop)
            self.cache[key] = (pcm, (lstart, min(lend, len(pcm))) if lcount else None, tuning)
        return self.cache[key]

    def instrument(self, i, key, voc):
        inst = struct.unpack_from('>I', self.font, 8 + 4 * i)[0]
        if not inst:
            return None, None
        lo, hi = self.font[inst + 1], self.font[inst + 2]
        slot = 8 if key < lo else 24 if key > hi else 16
        ptr, tuning = struct.unpack_from('>If', self.font, inst + slot)
        if not ptr:
            return None, None
        env = struct.unpack_from('>I', self.font, inst + 4)[0]
        return self.sample(ptr, tuning, voc), (env, self.font[inst + 3])

    def default_envelope(self, env):
        pts = []
        for k in range(64):
            delay, arg = struct.unpack_from('>hh', self.font, env + 4 * k)
            pts.append((delay, arg))
            if delay in (0, -1, -3):
                break
        return pts


def seq_envelope(seq, name):
    pts = []
    blk = seq.blocks.get(name)
    if not blk:
        return None
    for op, a in blk[1]:
        if op == 'point':
            pts.append((int(a[0], 0), int(a[1], 0)))
        elif op == 'hang':
            pts.append((-1, 0))
        elif op == 'disable':
            pts.append((0, 0))
        elif op == 'goto':
            pts.append((-2, int(a[0], 0)))
        elif op == 'restart':
            pts.append((-3, 0))
    return pts


def release_seconds(index):
    if index is None:
        return 0.1
    return 0.02 + ((255 - index) / 255) ** 1.5 * 1.5


def envelope_gain(points, t_on, t_total, release):
    """Gain curve sampled per output sample position -> function(t) giving 0..1. points: [(delay, arg)]."""
    # Expand the points into (time, level) breakpoints until a HANG; GOTO jumps at most 4 times.
    knots, t, level, i, hops = [(0.0, 0.0)], 0.0, 0.0, 0, 0
    hang = None
    while i < len(points) and hops < 4 and t < 30:
        delay, arg = points[i]
        if delay == 0:
            break
        if delay == -1:
            hang = t
            break
        if delay == -2:
            i, hops = arg, hops + 1
            continue
        if delay == -3:
            break
        t += delay * ENV_TICK
        level = arg / 32767
        knots.append((t, level))
        i += 1
    sustain = level
    if hang is None and i >= len(points) or (hang is None and i < len(points) and points[i][0] in (0, -3)):
        sustain = 0.0

    def level_at(x):
        if x >= knots[-1][0]:
            return sustain if hang is not None else knots[-1][1]
        for (t0, l0), (t1, l1) in zip(knots, knots[1:]):
            if x <= t1:
                return l0 + (l1 - l0) * ((x - t0) / (t1 - t0) if t1 > t0 else 1)
        return 0.0

    def gain(x):
        g = level_at(min(x, t_on))
        if x > t_on:
            g = level_at(t_on) * max(0.0, 1 - (x - t_on) / max(release, 1e-3))
        return g
    return gain


WAVES = {'FONTANY_INSTR_SAWTOOTH': ('wave', 'saw'), 'FONTANY_INSTR_TRIANGLE': ('wave', 'tri'),
         'FONTANY_INSTR_SINE': ('wave', 'sine'), 'FONTANY_INSTR_SQUARE': ('wave', 'square')}


def wave_table(kind):
    """One 64-sample period of the soundfont-less waveforms (FONTANY_INSTR_*); looped by the renderer."""
    import math
    n = 64
    if kind == 'saw':
        return [int(-12000 + 24000 * k / n) for k in range(n + 1)]
    if kind == 'tri':
        return [int(12000 * (1 - 4 * abs(k / n - .5) * 1 if False else (4 * abs(k / n - .5) - 1))) for k in range(n + 1)]
    if kind == 'square':
        return [12000 if k < n // 2 else -12000 for k in range(n + 1)]
    return [int(12000 * math.sin(2 * math.pi * k / n)) for k in range(n + 1)]


def render_note(font, voc, inst, key, vel, length_s, env_points, release, mix, start_s):
    if start_s > MAX_SECONDS:
        return True
    if inst[0] == 'wave':
        pcm, loop, src_rate, idata = wave_table(inst[1]), (0, 64), 27.5 * 2 ** (key / 12) * 64, (0, 0)
    else:
        sample, idata = font[inst[0]].instrument(inst[1], key, voc)
        if sample is None:
            return False
        pcm, loop, tuning = sample
        src_rate = tuning * 32000 * 2 ** ((key - 39) / 12)
    if env_points is None:
        env_points = font[inst[0]].default_envelope(idata[0]) if inst[0] != 'wave' and idata[0] else [(1, 32000), (-1, 0)]
    gain = envelope_gain(env_points, length_s, length_s + release, release) if env_points and env_points[0][0] != 0 else None
    total = min(length_s + release, MAX_SECONDS) if (loop or gain) else MAX_SECONDS
    step = src_rate / OUT_RATE
    n = int(total * OUT_RATE)
    pos, base = 0.0, int(start_s * OUT_RATE)
    if len(mix) < base + n:
        mix.extend([0.0] * (base + n - len(mix)))
    v = vel / 127
    for k in range(n):
        i = int(pos)
        if loop:
            if i >= loop[1]:
                pos = i = loop[0] + (i - loop[0]) % max(1, loop[1] - loop[0])
                pos = float(i)
        if i + 1 >= len(pcm):
            break
        frac = pos - i
        x = pcm[i] + (pcm[i + 1] - pcm[i]) * frac
        t = k / OUT_RATE
        g = gain(t) if gain else (1.0 if t < length_s or loop else max(0.0, 1 - (t - length_s) / max(release, 1e-3)))
        mix[base + k] += x * g * v
        pos += step
    return True


def run_channel(seq, name, pitches, font, voc, report):
    mix, notes = [], 0
    kind, ops = seq.blocks[name]

    def run(block, idx, inst, transpose, env, rel, start, depth):
        nonlocal notes
        t, last_len, steps = start, 48, 0
        while steps < 200:
            steps += 1
            if idx >= len(ops_of(block)):
                return
            op, a = ops_of(block)[idx]
            idx += 1
            if op == 'end':
                return
            if op == 'instr':
                m = re.match(r'SF(\d)_INST_(\d+)', a[0])
                inst = (int(m[1]), int(m[2])) if m else WAVES.get(a[0])
            elif op == 'transpose':
                transpose = int(a[0], 0)
            elif op == 'env':
                env = seq_envelope(seq, a[0]) if a else None
                if len(a) > 1:
                    rel = release_seconds(int(a[1], 0))
            elif op == 'releaserate':
                rel = release_seconds(int(a[0], 0))
            elif op in ('ldlayer',):
                run(a[1], 0, inst, transpose, env, rel, t, depth + 1)
            elif op in ('notedv', 'notedvg', 'noteldv', 'notevg'):
                p = a[0]
                key = pitches.get(p)
                if key is None:
                    try:
                        key = int(p, 0)
                    except ValueError:
                        continue
                if op in ('notedv', 'notedvg'):
                    last_len = int(a[1], 0)
                    vel = int(a[2], 0)
                else:
                    vel = int(a[1], 0) if len(a) > 1 else 100
                if inst is not None:
                    if render_note(font, voc, inst, key + transpose, vel, (last_len or 150) / TICKS_PER_SECOND, env, rel, mix, t / TICKS_PER_SECOND):
                        notes += 1
                    else:
                        report.add(f'instrument {inst} unavailable')
                if op != 'noteldv':
                    t += last_len
            elif op in ('ldelay', 'delay1', 'delay', 'lldelay'):
                try:
                    t += int(a[0], 0)
                except (ValueError, IndexError):
                    pass
            elif op in ('jump', 'rjump', 'call'):
                target = seq.labels.get(a[-1])
                if target and depth < 6:
                    if op == 'call':
                        run(target[0], target[1], inst, transpose, env, rel, t, depth + 1)
                    else:
                        block, idx = target
                        steps += 20
            elif op == 'loop':
                pass

    def ops_of(block):
        return seq.blocks[block][1]
    run(name, 0, None, 0, None, 0.1, 0, 0)
    return mix, notes


def write_clip(path, mix):
    import wave
    peak = max(map(abs, mix)) or 1
    scale = PEAK / peak
    mix = mix[:int(CLIP_SECONDS * OUT_RATE)]
    fade = int(0.05 * OUT_RATE)
    for k in range(min(fade, len(mix))):
        mix[-1 - k] *= k / fade
    pcm = [max(-32768, min(32767, int(round(x * scale)))) for x in mix]
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as w:
        w.setparams((1, 2, OUT_RATE, len(pcm), 'NONE', 'not compressed'))
        w.writeframes(struct.pack(f'<{len(pcm)}h', *pcm))
    rms = (sum(x * x for x in pcm) / len(pcm)) ** .5
    return len(pcm), max(map(abs, pcm)), round(rms, 1)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--oot', type=Path, default=ROOT / 'reference/roms/oot-us-1.2.z64')
    ap.add_argument('--decomp', type=Path, default=ROOT / 'reference/oot')
    ap.add_argument('--out', type=Path, default=ROOT / 'assets/audio/voices')
    args = ap.parse_args()
    seq_path = args.decomp / 'assets/audio/sequences/seq_0.prg.seq'
    if not args.oot.is_file() or not seq_path.is_file():
        print(f'OoT sfx: need {args.oot} and {seq_path}; the set stays silent')
        return
    voc = load_module('voices', ROOT / 'scripts/export-character-voices.py')
    rom = voc.load(args.oot, voc.OOT_MD5)

    def seg(i, size):
        vstart, vend, pstart, pend = struct.unpack_from('>4I', rom, 0x7960 + 16 * i)
        return rom[pstart:pstart + size]
    bank = seg(3, 179648)
    font = {0: Font(bank, seg(5, 4526992), 0, 15008)}
    font[1] = Font(bank, font[0].table, 15008, 6064)     # gSoundFontTable[1]: both fonts use sample bank 0
    pitches, chans = parse_pitches(args.decomp), parse_tables(args.decomp)
    seq = Seq(seq_path.read_text())
    manifest, skipped = {}, {}
    for name in WANTED:
        full = 'NA_SE_' + name
        chan = chans.get(full)
        if chan is None or chan not in seq.blocks:
            skipped[name] = 'no channel script in the sfx tables' if chan is None else f'{chan} not in seq_0'
            continue
        report = set()
        try:
            mix, notes = run_channel(seq, chan, pitches, font, voc, report)
        except Exception as e:      # noqa: BLE001 - a malformed script must not stop the export
            skipped[name] = f'{chan}: {type(e).__name__} {e}'
            continue
        if not notes or not mix or max(map(abs, mix)) < 1:
            skipped[name] = f'{chan}: no decodable note ({", ".join(sorted(report)) or "none rendered"})'
            continue
        rel = f'voices/oot_sfx/{name}_0.wav'
        frames, peak, rms = write_clip(args.out.parent / rel, mix)
        manifest[rel] = {'sampleRate': OUT_RATE, 'frames': frames, 'duration': round(frames / OUT_RATE, 4), 'peak': peak,
                         'rms': rms, 'source': f'{full} {chan}, {notes} note(s)'}
    (args.out / 'manifest-oot-sfx.json').write_text(json.dumps({'clips': dict(sorted(manifest.items())), 'skipped': dict(sorted(skipped.items()))}, indent=1) + '\n')
    print(f'OoT sfx: exported {len(manifest)} clips, skipped {len(skipped)}')
    for k, v in sorted(skipped.items()):
        print(f'  skipped {k}: {v}')


if __name__ == '__main__':
    main()
