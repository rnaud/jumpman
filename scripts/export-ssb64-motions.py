#!/usr/bin/env python3
"""Export the Super Smash Bros. (US) fighters' motion event scripts from a local ROM: hitboxes, sounds, effects, flags.

Formats follow VetriTheRetri/ssb-decomp-re (reference/ssb64, see native/SSB64.md "Motion event scripts"):
  * every fighter motion (FTMotionDesc in src/ft/ftdata.c) names a script offset in the fighter's MainMotion file
    (SubMotion file when FTANIM_FLAG_SUBMOTION_SCRIPT is set); the symbolic offsets are the #defines of
    include/ft/motiondesc_offsets.h. 0x80000000 means "no script".
  * a script is a list of 32-bit words: `opcode:6` in the top bits (enum FTMotionEvent, src/ft/ftdef.h), the layouts of
    FTMotionEvent* in src/ft/fttypes.h (MSB-first bit fields), pointers relocated by the file's pointer chains.
  * ftMainUpdateMotionEventsAll / ftMainParseMotionEvent (src/ft/ftmain.c) run it once per 60 Hz frame; ftMainSetStatus
    starts it with script_wait = 1 and runs it in the same frame, so an event after waits totalling N fires on frame N of
    the motion (0 = the frame the motion starts). SyncWait adds to the wait, AsyncWait waits for animation frame N.
This exporter runs that interpreter offline (subroutines, loops, gotos, the parallel script) and writes each motion's
events in firing order with their frame. A goto back into code that already ran is a repeating loop: it is recorded as
`loop` (the events from frame `from` repeat every `period` frames) and the simulation stops there.

Standard library only, deterministic; native/byor-rs/src/ssb64_motions.rs is its byte-identical port.
Writes public/models/ssb64/motion-events.json.
"""
import argparse
import hashlib
import importlib.util
import json
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECOMP = ROOT / 'reference/ssb64'
ROM = ROOT / 'reference/roms/ssb64-us.z64'
OUT = ROOT / 'public/models/ssb64'
SHA1 = 'e2929e10fccc0aa84e5776227e798abc07cedabf'
NO_SCRIPT = 0x80000000
SUBMOTION = 0x10
MAX_FRAMES = 1200   # a script still running after 20 s is cut there (none is)
MAX_EVENTS = 4000

_here = Path(__file__).resolve().parent


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, _here / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


ex = _load('export_ssb64', 'export-ssb64.py')
snd = _load('export_ssb64_sounds', 'export-ssb64-sounds.py')

# words per event (FTMotionEvent* struct sizes / 4); anything else is one word
SIZES = {'MakeAttackColl': 5, 'MakeAttackCollScaled': 5, 'SetAttackCollOffset': 2, 'SetThrow': 2, 'SetDamageThrown': 2,
         'SetDamageCollPartID': 4, 'Subroutine': 2, 'Goto': 2, 'Effect': 4, 'EffectItemHold': 4, 'SetParallelScript': 2}


def event_names(decomp=DECOMP):
    """src/ft/ftdef.h enum FTMotionEvent: opcode -> name without the nFTMotionEvent prefix."""
    text = (decomp / 'src/ft/ftdef.h').read_text()
    body = text[text.index('typedef enum FTMotionEvent'):]
    body = body[body.index('{') + 1:body.index('}')]
    out = []
    for raw in body.split(','):
        m = re.match(r'nFTMotionEvent(\w+)$', re.sub(r'//[^\n]*', '', raw).strip())
        if m:
            out.append(m.group(1))
    return out


def offsets(decomp=DECOMP):
    """include/ft/motiondesc_offsets.h: script symbol -> byte offset in its file."""
    out = {}
    for m in re.finditer(r'#define (\w+) (0x[0-9A-Fa-f]+)', (decomp / 'include/ft/motiondesc_offsets.h').read_text()):
        out[m.group(1)] = int(m.group(2), 16)
    return out


def motion_scripts(names, decomp=DECOMP):
    """src/ft/ftdata.c: per fighter (main motion file, sub motion file, [(motion id, script offset, flags)])."""
    text = (decomp / 'src/ft/ftdata.c').read_text()
    syms = offsets(decomp)

    def value(expr):
        total = 0
        for term in expr.split('+'):
            term = term.strip()
            total += int(term, 16) if term.startswith('0x') else int(term) if term.isdigit() else syms[term]
        return total

    def file_id(s):
        m = re.match(r'&ll(\w+)FileID', s)
        return names[m.group(1)] if m else None

    out = {}
    for f in ex.FIGHTERS:
        fields = [x.strip() for x in re.search(r'FTData dFT%sData =\s*\{(.*?)\};' % f, text, re.S).group(1).split(',')]
        table = re.search(r'FTMotionDesc %s\[\]\s*=\s*\{(.*?)\n\};' % re.escape(fields[25].lstrip('&')), text, re.S).group(1)
        items = [x.strip() for x in re.sub(r'//[^\n]*|/\*.*?\*/|[{}]', ' ', table, flags=re.S).split(',') if x.strip()]
        rows = []
        for k in range(0, len(items) - 2, 3):
            off = value(items[k + 1])
            if off == NO_SCRIPT:
                continue
            flags = 0
            for term in items[k + 2].split('|'):
                term = term.strip()
                flags |= int(term, 16) if term.startswith('0x') else FLAGS[term]
            rows.append((k // 3, off, flags))
        out[f] = (file_id(fields[1]), file_id(fields[2]), rows)
    return out


FLAGS = {'FTANIM_FLAG_TRANSN_JOINT': 0x80000000, 'FTANIM_FLAG_XROTN_JOINT': 0x40000000, 'FTANIM_FLAG_YROTN_JOINT': 0x20000000,
         'FTANIM_FLAG_SUBMOTION_SCRIPT': 0x10, 'FTANIM_FLAG_ANIMJOINT': 0x08, 'FTANIM_FLAG_TRANSLATE_SCALES': 0x04,
         'FTANIM_FLAG_SHIELDPOSE': 0x02, 'FTANIM_FLAG_ANIMLOCKS': 0x01, 'FTANIM_FLAG_NONE': 0}


def bits(w, lo, n, signed=False):
    v = (w >> lo) & ((1 << n) - 1)
    return v - (1 << n) if signed and v >> (n - 1) else v


def s16(w, hi):
    return bits(w, 16 if hi else 0, 16, True)


def decode(op, ws, fgm_names):
    """One event's fields from its words (`ws[0]` holds the opcode). Pointer events are handled by the interpreter."""
    w = ws[0]
    value = bits(w, 0, 26)
    if op in ('MakeAttackColl', 'MakeAttackCollScaled'):
        return {'id': bits(w, 23, 3), 'group': bits(w, 20, 3), 'joint': bits(w, 13, 7, True), 'damage': bits(w, 5, 8),
                'can_rebound': bits(w, 4, 1) == 1, 'element': bits(w, 0, 4), 'size': bits(ws[1], 16, 16),
                'offset': [s16(ws[1], False), s16(ws[2], True), s16(ws[2], False)],
                'angle': bits(ws[3], 22, 10, True), 'knockback_scale': bits(ws[3], 12, 10), 'knockback_weight': bits(ws[3], 2, 10),
                'hit_air': bits(ws[3], 0, 1) == 1, 'hit_ground': bits(ws[3], 1, 1) == 1,
                'shield_damage': bits(ws[4], 24, 8, True), 'fgm_level': bits(ws[4], 21, 3), 'fgm_kind': bits(ws[4], 17, 4),
                'knockback_base': bits(ws[4], 7, 10)}
    if op == 'SetAttackCollOffset':
        return {'id': bits(w, 23, 3), 'offset': [bits(w, 7, 16, True), s16(ws[1], True), s16(ws[1], False)]}
    if op == 'SetAttackCollDamage':
        return {'id': bits(w, 23, 3), 'damage': bits(w, 15, 8)}
    if op == 'SetAttackCollSize':
        return {'id': bits(w, 23, 3), 'size': bits(w, 7, 16)}
    if op == 'SetAttackCollSoundLevel':
        return {'id': bits(w, 23, 3), 'fgm_level': bits(w, 20, 3)}
    if op in ('ClearAttackCollID', 'RefreshAttackCollID'):
        return {'id': value}
    if op in ('PlayFGM', 'PlayLoopSFXStoreInfo', 'PlayVoiceStoreInfo', 'PlayLoopVoiceStoreInfo', 'PlayFGMStoreInfo'):
        return {'fgm': value, 'name': fgm_names[value] if value < len(fgm_names) else ''}
    if op in ('Effect', 'EffectItemHold'):
        return {'joint': bits(w, 19, 7, True), 'effect': bits(w, 10, 9), 'flag': bits(w, 0, 10),
                'offset': [s16(ws[1], True), s16(ws[1], False), s16(ws[2], True)],
                'scatter': [s16(ws[2], False), s16(ws[3], True), s16(ws[3], False)]}
    if op == 'SetHitStatusPartID':
        return {'joint': bits(w, 19, 7, True), 'hitstatus': bits(w, 0, 19)}
    if op == 'SetDamageCollPartID':
        return {'joint': bits(w, 19, 7, True), 'offset': [s16(ws[1], True), s16(ws[1], False), s16(ws[2], True)],
                'size': [s16(ws[2], False), s16(ws[3], True), s16(ws[3], False)]}
    if op == 'SetModelPartID':
        return {'joint': bits(w, 19, 7, True), 'modelpart': bits(w, 0, 19, True)}
    if op == 'SetTexturePartID':
        return {'texturepart': bits(w, 20, 6), 'frame': bits(w, 0, 20)}
    if op == 'SetColAnim':
        return {'colanim': bits(w, 18, 8), 'length': bits(w, 0, 18)}
    if op == 'SetSlopeContour':
        return {'flags': bits(w, 0, 3)}
    if op == 'SetAfterImage':
        return {'is_itemswing': bits(w, 18, 8), 'drawstatus': bits(w, 0, 18, True)}
    if op == 'MakeRumble':
        return {'length': bits(w, 13, 13), 'rumble': bits(w, 0, 13)}
    if op in ('End', 'ClearAttackCollAll', 'StopLoopSFX', 'PlaySmashVoice', 'ResetDamageCollPartAll', 'ResetModelPartAll',
              'HideModelPartAll', 'ResetColAnim', 'PauseScript', 'SetAirJumpAdd', 'SetAirJumpMax'):
        return {}
    return {'value': value}


def run_script(files, start, ops, fgm_names):
    """ftMainUpdateMotionEventsAll over one motion: (events, end frame or None, loops)."""
    events, loops = [], []
    threads = [{'pc': start, 'wait': 1, 'stack': [], 'seen': {}}]
    end = None
    for frame in range(MAX_FRAMES):
        if all(t['pc'] is None for t in threads):
            break
        ti = 0
        while ti < len(threads):  # a parallel script started this frame also runs this frame
            t = threads[ti]
            if t['pc'] is not None and t['wait'] is not None:
                t['wait'] -= 1
            while t['pc'] is not None and t['wait'] is not None and t['wait'] <= 0:
                if len(events) >= MAX_EVENTS:
                    raise ValueError(f'script at {start}: more than {MAX_EVENTS} events')
                fid, off = t['pc']
                data = files.data(fid)
                w = struct.unpack_from('>I', data, off)[0]
                op = ops[w >> 26]
                n = SIZES.get(op, 1)
                ws = list(struct.unpack_from('>%dI' % n, data, off))
                t['seen'].setdefault(t['pc'], frame)
                nxt = (fid, off + 4 * n)
                ptr = files.ptr(t['pc'], 4) if n == 2 else None
                ev = None
                if op == 'End':
                    t['pc'] = None
                    ev = {}
                elif op == 'SyncWait':
                    t['wait'] += ws[0] & 0x3FFFFFF
                    t['pc'] = nxt
                elif op == 'AsyncWait':
                    t['wait'] = (ws[0] & 0x3FFFFFF) - frame
                    t['pc'] = nxt
                elif op == 'LoopBegin':
                    t['stack'].append(('loop', nxt, ws[0] & 0x3FFFFFF))
                    t['pc'] = nxt
                elif op == 'LoopEnd':
                    kind, back, count = t['stack'][-1]
                    count -= 1
                    if count != 0:
                        t['stack'][-1] = (kind, back, count)
                        t['pc'] = back
                    else:
                        t['stack'].pop()
                        t['pc'] = nxt
                elif op == 'Subroutine':
                    t['stack'].append(('ret', nxt, 0))
                    t['pc'] = ptr
                elif op == 'Return':
                    t['pc'] = t['stack'].pop()[1]
                elif op == 'Goto':
                    if ptr in t['seen']:
                        loops.append({'thread': ti, 'from': t['seen'][ptr], 'period': frame - t['seen'][ptr]})
                        t['pc'] = None
                    else:
                        t['pc'] = ptr
                elif op == 'SetParallelScript':
                    if len(threads) == 1:
                        threads.append({'pc': ptr, 'wait': 1, 'stack': [], 'seen': {}})
                    t['pc'] = nxt
                elif op == 'PauseScript':
                    t['wait'] = None
                    t['pc'] = nxt
                    ev = {}
                elif op == 'SetThrow':
                    d = files.data(ptr[0])
                    vals = struct.unpack_from('>7i', d, ptr[1])
                    ev = dict(zip(('status', 'damage', 'angle', 'knockback_scale', 'knockback_weight', 'knockback_base', 'element'), vals))
                    t['pc'] = nxt
                elif op == 'SetDamageThrown':
                    ev = {}
                    t['pc'] = nxt
                else:
                    ev = decode(op, ws, fgm_names)
                    t['pc'] = nxt
                if ev is not None:
                    ev['frame'] = frame
                    ev['op'] = op
                    if ti:
                        ev['thread'] = ti
                    events.append(ev)
                if op == 'End' and ti == 0:
                    end = frame
            ti += 1
    return events, end, loops


ATTACK = ('MakeAttackColl', 'MakeAttackCollScaled')
PARAMS = ('group', 'joint', 'damage', 'can_rebound', 'element', 'size', 'offset', 'angle', 'knockback_scale', 'knockback_weight',
          'knockback_base', 'hit_air', 'hit_ground', 'shield_damage', 'fgm_level', 'fgm_kind')


def windows(events, loop_stop):
    """The attack collisions (fp->attack_colls[4]) the events leave on after each frame, as windows of constant values:
    [from, to) frames (`to` None: on until the motion ends). `set` numbers the hit records: a target is hit once per set
    (a new collision shares the records of an active one of its group; RefreshAttackCollID starts new records)."""
    frames = {}
    for ev in events:
        frames.setdefault(ev['frame'], []).append(ev)
    state, live, out = {}, {}, []
    sets = 0
    last = max(frames) + 1 if frames else 0
    for f in range(last):
        for ev in frames.get(f, []):
            op = ev['op']
            if op in ATTACK:
                i = ev['id']
                cur = state.get(i)
                p = {k: ev[k] for k in PARAMS}
                p['scaled'] = op == 'MakeAttackCollScaled'
                if cur is None or cur['group'] != ev['group']:
                    share = [s['set'] for j, s in sorted(state.items()) if j != i and s['group'] == ev['group']]
                    if share:
                        p['set'] = share[0]
                    else:
                        p['set'] = sets
                        sets += 1
                else:
                    p['set'] = cur['set']
                state[i] = p
            elif op in ('SetAttackCollOffset', 'SetAttackCollDamage', 'SetAttackCollSize', 'SetAttackCollSoundLevel'):
                if ev['id'] in state:
                    for k in ('offset', 'damage', 'size', 'fgm_level'):
                        if k in ev:
                            state[ev['id']] = dict(state[ev['id']], **{k: ev[k]})
            elif op == 'RefreshAttackCollID':
                if ev['id'] in state:
                    state[ev['id']] = dict(state[ev['id']], set=sets)
                    sets += 1
            elif op == 'ClearAttackCollID':
                state.pop(ev['id'], None)
            elif op == 'ClearAttackCollAll':
                state.clear()
        for i in sorted(set(state) | set(live)):
            cur, w = state.get(i), live.get(i)
            if w is not None and (cur is None or any(w[k] != cur[k] for k in cur)):
                w['to'] = f
                out.append(w)
                del live[i]
            if cur is not None and i not in live:
                live[i] = dict(cur, id=i, **{'from': f})
    for i in sorted(live):
        live[i]['to'] = loop_stop
        out.append(live[i])
    out.sort(key=lambda w: (w['from'], w['id']))
    return out


def r1(x):
    return round(x, 1) + 0.0


def place(fighter, tracks, length, transn, windows_, attr_size):
    """Each window's centre per frame, in fighter space (SSB64 units from the feet, model axes: +Z forward, +Y up): the
    joint's world matrix at that frame (the motion's figatree pose, TopN scaled by FTAttributes.size) applied to the
    offset (divided by the size for MakeAttackCollScaled). Joint ids: 0 = TopN, 4 + n = DObjDesc n (`joint_NN`)."""
    by_index = {j['index']: k for k, j in enumerate(fighter.joints)}
    cache = {}

    def world(k, frame):
        key = (k, frame)
        if key not in cache:
            j = fighter.joints[k]
            tr = tracks[k] if tracks else None
            t, r, s = tr[min(frame, len(tr) - 1)] if tr else (j['t'], j['r'], j['s'])
            if transn and j['index'] == 0:
                t = [0.0, 0.0, 0.0]
            local = ex.mat_trs(t, r, s)
            cache[key] = ex.mat_mul(world(j['parent'], frame), local) if j['parent'] is not None else local
        return cache[key]

    for w in windows_:
        end = w['to'] if w['to'] is not None else max(length, w['from'] + 1)
        off = w['offset']
        if w['scaled']:
            off = [off[0] / attr_size, off[1] / attr_size, off[2] / attr_size]
        k = by_index.get(w['joint'] - 4) if w['joint'] >= 4 else None
        pos = []
        for frame in range(w['from'], end):
            p = ex.apply(world(k, frame), off) if k is not None else list(off)
            pos.append([r1(p[0] * attr_size), r1(p[1] * attr_size), r1(p[2] * attr_size)])
        w['positions'] = pos
        w['on_joint'] = k is not None
    return windows_


def export(rom, out, decomp=DECOMP):
    files = ex.Files(rom)
    names = ex.file_names()
    cmotions = ex.common_motions()
    special = next(k for k, v in cmotions.items() if v == 'SpecialStart')
    ops = event_names(decomp)
    fgm_names = snd.voice_names(decomp)
    tables = motion_scripts(names, decomp)
    ftables = ex.fighter_tables(names)
    index = {'game': 'ssb64', 'fps': 60, 'fighters': {}}
    for f in ex.FIGHTERS:
        main, sub, rows = tables[f]
        specials = ex.special_motions(f)
        fighter = ex.Fighter(files, names, f, ftables[f])
        anims = {}
        motions = {}
        for mid, off, flags in rows:
            fid = sub if flags & SUBMOTION else main
            events, end, loops = run_script(files, (fid, off), ops, fgm_names)
            name = cmotions.get(mid, f'Motion{mid}') if mid < special else (
                specials[mid - special] if mid - special < len(specials) else f'Special{mid - special}')
            anim = ftables[f]['motions'][mid] if mid < len(ftables[f]['motions']) else None
            stop = next((lp['from'] + lp['period'] for lp in loops if lp['thread'] == 0), None)
            wins = windows(events, stop)
            root = False
            if wins:  # the figatree is decoded only for motions that attack
                tracks, length = None, 0
                if anim is not None and anim in names:
                    if anim not in anims:
                        anims[anim] = fighter.animation(names[anim])
                    tracks, length = anims[anim]
                # root motion (the travel the sim does itself): a clip whose joint_00 starts at the origin, the rule of the
                # view's `root_motion_clips` (in the game TransN is unlinked from the body and its translation becomes the
                # fighter's velocity, ftMainSetStatus / ftMainPlayAnim); joint_00 is held at the origin for them
                k0 = next((k for k, j in enumerate(fighter.joints) if j['index'] == 0), None)
                if tracks and k0 is not None and tracks[k0] and abs(tracks[k0][0][0][1]) < 20.0:
                    root = True
                place(fighter, tracks, length, root, wins, fighter.size)
            motions[str(mid)] = {'motion': name, 'script': off, 'end': end, 'loops': loops, 'events': events, 'hitboxes': wins,
                                 'root_motion': root}
        index['fighters'][f.lower()] = {'size': fighter.size, 'motions': motions}
    out.mkdir(parents=True, exist_ok=True)
    (out / 'motion-events.json').write_text(json.dumps(index, sort_keys=True, separators=(',', ':')) + '\n')
    return index


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rom', type=Path, default=ROM)
    ap.add_argument('--out', type=Path, default=OUT)
    a = ap.parse_args()
    rom = a.rom.read_bytes()
    if hashlib.sha1(rom).hexdigest() != SHA1:
        raise SystemExit(f'{a.rom} is not Super Smash Bros. (US) (sha1 {SHA1})')
    index = export(rom, a.out)
    for f, v in index['fighters'].items():
        hits = sum(1 for m in v['motions'].values() for e in m['events'] if e['op'].startswith('MakeAttackColl'))
        print(f'{f}: {len(v["motions"])} scripts, {hits} hitboxes', flush=True)


if __name__ == '__main__':
    main()
