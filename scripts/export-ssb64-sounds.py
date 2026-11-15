#!/usr/bin/env python3
"""Export the Super Smash Bros. (US) fighter voices and special-move sounds from a local ROM as mono 16-bit WAVs.

Formats follow VetriTheRetri/ssb-decomp-re (reference/ssb64, see native/SSB64.md "Sounds"):
  * B1_sounds2.ctl / .tbl (ROM 0xC6B650 / 0xC7B1F0, smashbrothers.us.yaml): a libultra ALBankFile with one bank and one
    instrument whose soundArray holds every sound-effect sample, VADPCM (include/PR/libaudio.h).
  * fgm.ucd (0xF5A9C0) and fgm.tbl (0xF57BF0): `{s32 count; u32 offsets[count]; data}` packages. A sound id
    (gmFGMVoiceID, src/gm/gmsound.h) indexes a fgm.ucd voice script; its notes spawn fgm.tbl articulation programs that
    trigger a sample, set volume and pitch per frame and attach LFOs from fgm.unk (0xF573D0). The interpreters are
    func_80026B90_27790 (scripts) and func_80027460_28060 (articulations) in src/libultra/n_audio/n_env.c.
  * The engine runs both once per 60 Hz audio frame and plays voices at alCents2Ratio(cents) x the 32000 Hz output rate
    (src/sys/audio.c); this script runs the same interpreters offline and mixes the result at 32000 Hz.
Names come from the gmFGMVoiceID enum (src/gm/gmsound.h, US branch of its REGION_US conditionals). EXPORT below is the
list of sounds the game's Smash fighters use (native/src/characters/smash_audio.rs maps them to moves). Standard library
only, deterministic; native/byor-rs/src/ssb64_sounds.rs is its byte-identical port.

Writes assets/audio/voices/ssb64/<Name>_0.wav (Name = the enum name without `nSYAudio`) and manifest-ssb64.json.
"""
import argparse
import hashlib
import json
import math
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECOMP = ROOT / 'reference/ssb64'
ROM = ROOT / 'reference/roms/ssb64-us.z64'
OUT = ROOT / 'assets/audio/voices/ssb64'
SHA1 = 'e2929e10fccc0aa84e5776227e798abc07cedabf'

CTL = (0xC6B650, 0xC7B1F0)       # B1_sounds2.ctl
TBL = (0xC7B1F0, 0xF573D0)       # B1_sounds2.tbl (up to fgm.unk)
FGM_UNK = (0xF573D0, 0xF57BF0)
FGM_TBL = (0xF57BF0, 0xF5A9C0)
FGM_UCD = (0xF5A9C0, 0xF5F4E0)
RATE = 32000                     # dSYAudioPublicSettings output rate
MAX_FRAMES = 210                 # sounds still playing after 3.5 s (charge loops) are cut there with a fade
FADE = 320                       # samples of fade-out on a cut sound
MAX_RATIO = 1.99996              # the N64 resampler's pitch limit

FIGHTER_NAMES = ['Mario', 'Luigi', 'Donkey', 'Link', 'Samus', 'Yoshi', 'Kirby', 'Fox', 'Pikachu', 'Purin', 'Captain', 'Ness']
# Every fighter voice (nSYAudioVoice<Fighter>*) plus these effects (jumps, special moves, the KO star).
EXPORT_FGM = [
    'FGMDeadUpStar', 'FGMSamusJump1', 'FGMSamusJumpAerial', 'FGMKirbyPurinJump', 'FGMInflateJump1', 'FGMNessJump',
    'FGMMarioSpecialN', 'FGMMarioSpecialHiJump', 'FGMMarioSpecialHiCoin',
    'FGMDonkeyCharge', 'FGMDonkeySpin', 'FGMDonkeySlap1',
    'FGMLinkSpecialNShoot', 'FGMLinkSpecialNReturn', 'FGMLinkSpecialLwGet', 'FGMLinkSpecialHi',
    'FGMSamusSpecialNCharge0', 'FGMSamusSpecialNShootL', 'FGMSamusSpecialLw', 'FGMSamusSpecialHi', 'FGMExplodeS',
    'FGMYoshiSpecialNTongue', 'FGMYoshiEggLayShatter', 'FGMYoshiSpecialHiThrow',
    'FGMKirbySpecialNStart', 'FGMKirbySpecialNThrow', 'FGMKirbySpecialLwStart',
    'FGMFoxBlaster', 'FGMFoxSpecialHiStart', 'FGMFoxSpecialHiFly', 'FGMFoxSpecialLwStart', 'FGMFoxSpecialLwHit',
    'FGMPikachuElectric1', 'FGMPikachuSpecialHiStart', 'FGMPikachuSpecialLwThunder',
    'FGMCaptainSpecialNStart', 'FGMCaptainSpecialNPunch', 'FGMCaptainSpecialHi',
    'FGMNessPSI', 'FGMNessPKThunderLoop', 'FGMNessSpecialLwStart',
]


# ---------------------------------------------------------------------------------------------- names

def voice_names(decomp=DECOMP):
    """gmFGMVoiceID (src/gm/gmsound.h) names in enum order, US branch."""
    text = (decomp / 'src/gm/gmsound.h').read_text()
    body = text[text.index('typedef enum gmFGMVoiceID'):]
    body = body[body.index('{') + 1:body.index('} gmFGMVoiceID')]
    names, keep = [], [True]
    for line in body.split('\n'):
        line = line.split('//')[0].strip()
        if line.startswith('#if'):
            keep.append(keep[-1] and line == '#if defined(REGION_US)')
            continue
        if line.startswith('#else'):
            keep[-1] = keep[-2] and not keep[-1]
            continue
        if line.startswith('#endif'):
            keep.pop()
            continue
        if not keep[-1]:
            continue
        for part in line.split(','):
            part = part.split('=')[0].strip()
            if re.fullmatch(r'nSYAudio\w+', part):
                names.append(part[len('nSYAudio'):])
    return names


def export_list(names):
    fighters = '|'.join(FIGHTER_NAMES)
    voices = [n for n in names if re.fullmatch(rf'Voice({fighters})\w*', n)]
    return voices + EXPORT_FGM


# ---------------------------------------------------------------------------------------------- data

def package(data):
    count = struct.unpack_from('>i', data, 0)[0]
    offs = list(struct.unpack_from(f'>{count}I', data, 4))
    return [data[o:(offs[i + 1] if i + 1 < count else len(data))] for i, o in enumerate(offs)]


def lfos(data):
    count = struct.unpack_from('>I', data, 0)[0]
    return [struct.unpack_from('>BBBBfff', data, 4 + 16 * i) for i in range(count)]


def vadpcm(book, data):
    """Decode N64 VADPCM; `book` is the ALADPCMBook (order, npredictors, s16 table)."""
    order, npred = struct.unpack_from('>ii', book, 0)
    if order != 2:
        raise ValueError(f'unsupported VADPCM order {order}')
    table = struct.unpack_from(f'>{8 * order * npred}h', book, 8)
    books = [[table[p * 16 + o * 8:p * 16 + o * 8 + 8] for o in range(2)] for p in range(npred)]
    out = []
    h2 = h1 = 0
    for off in range(0, len(data) - 8, 9):
        scale, pred = data[off] >> 4, data[off] & 15
        t0, t1 = books[pred]
        nib = []
        for b in data[off + 1:off + 9]:
            nib += (b >> 4, b & 15)
        res = [(n - 16 if n >= 8 else n) << scale for n in nib]
        for half in (0, 8):
            e = res[half:half + 8]
            y = []
            for i in range(8):
                acc = (e[i] << 11) + h2 * t0[i] + h1 * t1[i]
                for j in range(i):
                    acc += e[j] * t1[i - 1 - j]
                v = acc >> 11
                y.append(-32768 if v < -32768 else 32767 if v > 32767 else v)
            out += y
            h2, h1 = y[6], y[7]
    return out


class Bank:
    """B1_sounds2: instrument 0's sounds as (pcm, loop) with loop = (start, end, count) or None."""

    def __init__(self, rom):
        self.ctl, self.tbl = rom[CTL[0]:CTL[1]], rom[TBL[0]:TBL[1]]
        bank = struct.unpack_from('>I', self.ctl, 4)[0]
        inst = struct.unpack_from('>I', self.ctl, bank + 12)[0]
        count = struct.unpack_from('>h', self.ctl, inst + 14)[0]
        self.sounds = list(struct.unpack_from(f'>{count}I', self.ctl, inst + 16))
        self.cache = {}

    def sound(self, i):
        if i not in self.cache:
            wave_ptr = struct.unpack_from('>I', self.ctl, self.sounds[i] + 8)[0]
            base, length, kind, _flags, loop, book = struct.unpack_from('>iiBBxxII', self.ctl, wave_ptr)
            if kind != 0:
                raise ValueError(f'sound {i}: not ADPCM')
            order, npred = struct.unpack_from('>ii', self.ctl, book)
            pcm = vadpcm(self.ctl[book:book + 8 + 16 * order * npred], self.tbl[base:base + length])
            lp = None
            if loop:
                start, end, count = struct.unpack_from('>III', self.ctl, loop)
                if count and start < end <= len(pcm):
                    lp = (start, end, count)
            self.cache[i] = (pcm, lp)
        return self.cache[i]


# ---------------------------------------------------------------------------------------------- the FGM engine, offline

def cents_ratio(cents):
    """alCents2Ratio (src/libultra/audio/cents2ratio.c), in doubles."""
    x = 1.00057779 if cents >= 0 else 0.9994225441
    cents = abs(cents)
    ratio = 1.0
    while cents:
        if cents & 1:
            ratio *= x
        x *= x
        cents >>= 1
    return ratio


def varint(code, p):
    v = code[p]
    p += 1
    if v & 0x80:
        v = ((v & 0x7F) << 8) + code[p]
        p += 1
    return v, p


def s16(v):
    v = int(v) & 0xFFFF
    return v - 0x10000 if v & 0x8000 else v


class Rng:
    def __init__(self):
        self.seed = 1

    def next(self):
        self.seed = (self.seed * 0x343FD + 0x269EC3) & 0xFFFFFFFF
        return ((self.seed >> 16) & 0xFFFF) / 65536.0


class Mod:
    """An LFO attached to a voice (ALWhatever8009EDD0_siz24)."""
    __slots__ = ('id', 'shape', 'target', 'post', 'period', 'amp', 'offset', 'phase', 't18', 'prev', 'cur')


class Voice:
    """An articulation instance (ALWhatever8009EE0C)."""

    def __init__(self, code):
        self.code, self.pc, self.loop_pc, self.timer = code, 0, 0, 1
        self.state = 3                         # 3 start, 1 playing, 2 silence, 0 free
        self.vol, self.pan, self.fx, self.pitch, self.note, self.chan_vol = 127, 64, 0, 0, 0, 255
        self.wave = None                       # sound index (a trigger may change it)
        self.mods = []
        self.started = False                   # the synth voice exists
        self.pos, self.ratio, self.gain = 0.0, 1.0, 0.0
        self.done = False                      # the sample ran out


class Script:
    """A voice script (ALWhatever8009EDD0_siz34)."""

    def __init__(self, code):
        self.code, self.pc, self.loop_pc, self.timer = code, 0, 0, 1
        self.artic, self.dur = 0, [0] * 6
        self.flag20 = self.flag21 = 0
        self.prio, self.group, self.vol, self.voice, self.ended = 0, 0, 255, None, False

    def fork(self, code):
        s = Script(code)
        s.artic, s.dur, s.prio, s.group, s.vol = self.artic, list(self.dur), self.prio, self.group, self.vol
        s.flag20, s.flag21 = self.flag20, self.flag21
        return s


class Engine:
    def __init__(self, ucd, tbl, unk, bank):
        self.ucd, self.tbl, self.unk, self.bank = ucd, tbl, unk, bank

    def render(self, sound_id):
        self.rng1, self.rng2 = Rng(), Rng()
        self.scripts = [Script(self.ucd[sound_id])]
        self.voices = []
        out, frame, cut = [], 0, False
        while True:
            for s in self.scripts:             # a voice freed last frame lets the script's next note start a new one
                if s.voice is not None and s.voice.state == 0:
                    s.voice = None
            i = 0                              # forks append to the list and run this frame (timer 1)
            while i < len(self.scripts):
                self.run_script(self.scripts[i])
                i += 1
            self.scripts = [s for s in self.scripts if not s.ended]
            for v in self.voices:
                self.run_voice(v)
            start, end = frame * RATE // 60, (frame + 1) * RATE // 60
            buf = [0.0] * (end - start)
            for v in self.voices:
                self.mix(v, buf)
            out += buf
            self.voices = [v for v in self.voices if v.state != 0]
            frame += 1
            if not self.scripts and not self.voices:
                break
            if frame >= MAX_FRAMES:
                cut = any(v.started and not v.done and v.gain > 0.0 for v in self.voices)
                break
        while out and out[-1] == 0.0:
            out.pop()
        if cut:
            n = min(FADE, len(out))
            for k in range(n):
                out[len(out) - n + k] *= (n - k) / n
        return [max(-32768, min(32767, math.floor(x + 0.5))) for x in out], cut

    # -- func_80026B90_27790
    def run_script(self, s):
        if s.ended or s.timer == 0:
            return
        s.timer -= 1
        if s.timer != 0:
            return
        if s.flag21:
            s.flag21 = 0
            self.release(s)
            s.timer = 1
            return
        code, p, t5 = s.code, s.pc, 0
        for _ in range(4096):
            op = code[p]
            p += 1
            timer = 0
            if op & 0xF8 >= 0xD0:
                if op == 0xD0:
                    self.release(s)
                    s.ended = True
                    s.pc = p
                    return
                elif op == 0xD1:
                    s.artic, p = varint(code, p)
                elif op == 0xD2:
                    s.flag20 = 1 if code[p] & 0x80 else 0
                    p += 1
                elif op == 0xD3:
                    s.prio = code[p]
                    p += 1
                elif op == 0xD4:
                    for k in range(6):
                        s.dur[k], p = varint(code, p)
                elif op == 0xD5:
                    s.vol = code[p]
                    p += 1
                elif op == 0xD6:
                    s.vol = max(0, min(255, s.vol + struct.unpack('b', code[p:p + 1])[0]))
                    p += 1
                elif op in (0xD7, 0xDC):
                    p += 1                     # pan, fx level (mono export)
                elif op in (0xD8, 0xDD):
                    p += 1
                elif op == 0xD9:
                    idx, p = varint(code, p)
                    if idx < len(self.ucd):
                        self.scripts.append(s.fork(self.ucd[idx]))
                elif op == 0xDA:
                    s.loop_pc = p
                elif op == 0xDB:
                    p = s.loop_pc
                elif op == 0xDE:
                    s.group = code[p]
                    p += 1
                    if s.group:
                        for o in self.scripts:
                            if o is not s and not o.ended and o.group == s.group:
                                if (s.prio & 0x7F) >= (o.prio & 0x7F):
                                    o.ended = True
                                    self.release(o)
                                else:
                                    self.release(s)
                                    s.ended = True
                                    s.pc = p
                                    return
                elif op == 0xDF:
                    t5 = -2400
                elif op == 0xE0:
                    t5 = -4800
                else:
                    raise ValueError(f'fgm.ucd opcode {op:#x}')
            else:
                timer = op & 7
                if 1 <= timer <= 6:
                    timer = s.dur[timer - 1]
                elif timer == 7:
                    timer, p = varint(code, p)
                if not op & 0xF8:
                    self.release(s)
                else:
                    cents = (op >> 3) * 100 - 1300 + t5
                    t5 = 0
                    if s.voice is not None:
                        s.voice.note = cents
                    elif s.artic < len(self.tbl):
                        v = Voice(self.tbl[s.artic])
                        v.note = cents
                        v.chan_vol = s.vol * 127 >> 7
                        self.voices.append(v)
                        s.voice = v
                    if timer > 1 and s.flag20:
                        timer -= 1
                        s.flag21 = 1
            if timer != 0:
                break
        else:
            raise ValueError('fgm.ucd script does not yield')
        s.pc, s.timer = p, timer

    def release(self, s):
        if s.voice is not None:
            s.voice.timer, s.voice.state = 0, 2
            s.voice = None

    # -- func_80027460_28060
    def run_voice(self, v):
        if v.timer != 0:
            v.timer -= 1
            if v.timer == 0:
                code, p = v.code, v.pc
                for _ in range(4096):
                    op = code[p]
                    p += 1
                    timer = op & 0xF
                    if timer & 8:
                        b = code[p]
                        p += 1
                        timer = ((timer & 7) << 7) + (b & 0x7F)
                        if b & 0x80:
                            timer = (timer << 8) + code[p]
                            p += 1
                    kind = op & 0xF0
                    if kind in (0x00, 0x10, 0x30):
                        a = code[p]
                        p += 1
                        cur = {0x00: v.vol, 0x10: v.pan, 0x30: v.fx}[kind]
                        val = a if a <= 127 else max(0, min(127, a - 192 + cur))
                        if kind == 0x00:
                            v.vol = val
                        elif kind == 0x10:
                            v.pan = val
                        else:
                            v.fx = val
                    elif kind == 0x20:
                        a = s16((code[p] << 8) | code[p + 1])
                        p += 2
                        if a <= 1200:
                            v.pitch = max(-1200, a)
                        else:
                            v.pitch = max(-1200, min(1200, a - 2400 + v.pitch))
                    elif kind == 0x40:
                        mid = code[p]
                        p += 1
                        idx, p = varint(code, p)
                        if idx < len(self.unk):
                            self.spawn_mod(v, mid, self.unk[idx])
                    elif kind == 0x50:
                        mid = code[p]
                        p += 1
                        v.mods = [m for m in v.mods if m.id != mid]
                    elif kind == 0x60:
                        idx, p = varint(code, p)
                        if idx < len(self.bank.sounds):
                            v.wave, v.state = idx, 3
                    elif kind == 0x70:
                        v.state, timer = 2, 10000
                    elif kind == 0x80:
                        v.loop_pc = p
                    elif kind == 0x90:
                        p = v.loop_pc
                    else:
                        raise ValueError(f'fgm.tbl opcode {op:#x}')
                    if timer != 0:
                        break
                else:
                    raise ValueError('fgm.tbl program does not yield')
                v.pc, v.timer = p, timer
        self.run_mods(v)
        if v.state == 1:
            v.ratio = min(MAX_RATIO, cents_ratio(v.pitch + v.note))
            v.gain = self.gain(v)
        elif v.state == 2:
            v.gain, v.state, v.started = 0.0, 0, False
        elif v.state == 3:
            if v.wave is None:
                v.state = 0
            else:
                v.started, v.done, v.pos = True, False, 0.0
                v.ratio = min(MAX_RATIO, cents_ratio(v.pitch + v.note))
                v.gain = self.gain(v)
                v.state = 1

    @staticmethod
    def gain(v):
        return (v.vol * v.chan_vol * 127 >> 7) / 32768.0

    def spawn_mod(self, v, mid, entry):
        shape, target, post, phase0, period, amp, offset = entry
        m = next((m for m in v.mods if m.id == mid), None)
        if m is None:
            m = Mod()
            m.id, m.t18, m.prev, m.cur = mid, 0.0, 0.0, 0.0
            v.mods.append(m)
            v.mods.sort(key=lambda x: x.id)
        m.id, m.shape, m.target, m.post, m.period, m.amp, m.offset = mid, shape, target, post, period, amp, offset
        m.phase = period * phase0 / 256.0
        if shape in (4, 8):
            rng = self.rng1 if shape == 4 else self.rng2
            m.cur = rng.next() * amp + offset
            m.t18 = rng.next() * period
            m.phase = m.t18 * phase0 / 256.0
        elif shape == 5:
            m.prev = 0.0
            m.cur = self.rng1.next() * amp + offset
            m.t18 = self.rng1.next() * period + 0.5
            m.phase = m.t18 * phase0 / 256.0

    def run_mods(self, v):
        regs = [0.0] * 10
        for m in v.mods:
            if m.shape < 4:
                m.phase += 1.0
                if m.period < m.phase:
                    m.phase -= m.period
            if m.shape == 0:
                k = int(m.phase / m.period * 4096.0) & 0xFFF if m.period else 0
                s = math.sin(math.pi * (k & 0x7FF) / 2048.0)
                y = (-s if k & 0x800 else s) * m.amp + m.offset
            elif m.shape == 1:
                y = m.amp if m.period / 2 < m.phase else m.offset
            elif m.shape == 2:
                y = m.amp * m.phase / m.period + m.offset if m.period else m.offset
            elif m.shape == 3:
                y = m.amp * (m.period - m.phase) / m.period + m.offset if m.period else m.offset
            elif m.shape in (4, 8):
                m.phase += 1.0
                if m.t18 < m.phase:
                    rng = self.rng1 if m.shape == 4 else self.rng2
                    m.cur = rng.next() * m.amp + m.offset
                    m.t18 = rng.next() * m.period
                    m.phase = 0.0
                y = m.cur
            elif m.shape == 5:
                m.phase += 1.0
                if m.t18 < m.phase:
                    m.prev = m.cur
                    m.cur = self.rng1.next() * m.amp + m.offset
                    m.t18 = self.rng1.next() * m.period + 0.5
                    m.phase = 0.0
                y = (m.cur - m.prev) * m.phase / m.t18 + m.prev if m.t18 else m.prev
            elif m.shape in (6, 7):
                m.phase += 1.0
                if m.period < m.phase:
                    m.phase = m.period
                if not m.period:
                    y = m.offset
                elif m.shape == 6:
                    y = m.amp * m.phase / m.period + m.offset
                else:
                    y = m.amp * (m.period - m.phase) / m.period + m.offset
            else:
                y = 0.0
            if m.post >> 4 == 1:
                y += regs[m.post & 0xF] if (m.post & 0xF) < 10 else 0.0
            elif m.post >> 4 == 2:
                y *= regs[m.post & 0xF] if (m.post & 0xF) < 10 else 0.0
            t = m.target
            if t in (10, 11):
                v.vol = int(max(0.0, min(127.0, y + (v.vol if t == 11 else 0))))
            elif t in (12, 13):
                v.pitch = int(max(-1200.0, min(1200.0, y + (v.pitch if t == 13 else 0))))
            elif t in (14, 15):
                v.pan = int(max(0.0, min(127.0, y + (v.pan if t == 15 else 0))))
            elif t < 10:
                regs[t] = y
            else:
                if t < 24:
                    dst, field = m, t
                else:
                    dst = next((o for o in v.mods if o.id == (t - 24) // 8), None)
                    field = (t - 24) % 8 + 16
                if dst is None:
                    continue
                attr = {16: 'period', 17: 'period', 18: 'amp', 19: 'amp', 20: 'offset', 21: 'offset', 22: 'phase', 23: 'phase'}.get(field)
                if attr is None:
                    continue
                setattr(dst, attr, y + (getattr(dst, attr) if field & 1 else 0.0))

    def mix(self, v, buf):
        if not v.started or v.done:
            return
        pcm, loop = self.bank.sound(v.wave)
        n = len(pcm)
        for k in range(len(buf)):
            i = int(v.pos)
            if i >= n:
                v.done = True
                return
            a = pcm[i]
            b = pcm[i + 1] if i + 1 < n else 0
            buf[k] += (a + (b - a) * (v.pos - i)) * v.gain
            v.pos += v.ratio
            if loop is not None and v.pos >= loop[1]:
                v.pos -= loop[1] - loop[0]


# ---------------------------------------------------------------------------------------------- output

def wav_bytes(pcm, rate):
    data = struct.pack(f'<{len(pcm)}h', *pcm)
    return (b'RIFF' + struct.pack('<I', 36 + len(data)) + b'WAVEfmt ' + struct.pack('<IHHIIHH', 16, 1, 1, rate, rate * 2, 2, 16)
            + b'data' + struct.pack('<I', len(data)) + data)


def export(rom, out, decomp=DECOMP, log=print):
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise ValueError('not Super Smash Bros. (US)')
    names = voice_names(decomp)
    ids = {n: i for i, n in enumerate(names)}
    engine = Engine(package(rom[FGM_UCD[0]:FGM_UCD[1]]), package(rom[FGM_TBL[0]:FGM_TBL[1]]), lfos(rom[FGM_UNK[0]:FGM_UNK[1]]), Bank(rom))
    out.mkdir(parents=True, exist_ok=True)
    manifest = {}
    for name in export_list(names):
        pcm, cut = engine.render(ids[name])
        if not pcm:
            continue                           # VoiceLinkNull: a script that plays nothing
        (out / f'{name}_0.wav').write_bytes(wav_bytes(pcm, RATE))
        peak = max(abs(x) for x in pcm)
        manifest[name] = {'id': ids[name], 'frames': len(pcm), 'duration': round(len(pcm) / RATE, 4), 'peak': peak, 'cut': cut}
    (out / 'manifest-ssb64.json').write_text(json.dumps({'sampleRate': RATE, 'clips': manifest}, indent=1, sort_keys=True) + '\n')
    log(f'ssb64: exported {len(manifest)} sounds')
    return manifest


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rom', type=Path, default=ROM)
    ap.add_argument('--out', type=Path, default=OUT)
    a = ap.parse_args()
    rom = a.rom.read_bytes()
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise SystemExit(f'{a.rom} is not Super Smash Bros. (US) (sha1 {SHA1})')
    export(rom, a.out)


if __name__ == '__main__':
    main()
