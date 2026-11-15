#!/usr/bin/env python3
"""ROM-free checks of the Super Smash Bros. motion event script exporter (scripts/export-ssb64-motions.py):
python3 -m unittest byor/test_ssb64_motions.py

Scripts here are hand-assembled with the decomp's ftMotionCommand* layouts (src/ft/ftdef.h); the decomp table readers run
against reference/ssb64 when that submodule is checked out. No ROM bytes are used or committed.
"""
import importlib.util, struct, unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('export_ssb64_motions', ROOT / 'scripts/export-ssb64-motions.py')
mx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mx)
HAVE_DECOMP = (ROOT / 'reference/ssb64/src/ft/ftdef.h').is_file()

OPS = ['End', 'SyncWait', 'AsyncWait', 'MakeAttackColl', 'MakeAttackCollScaled', 'ClearAttackCollID', 'ClearAttackCollAll',
       'SetAttackCollOffset', 'SetAttackCollDamage', 'SetAttackCollSize', 'SetAttackCollSoundLevel', 'RefreshAttackCollID',
       'SetThrow', 'SetDamageThrown', 'PlayFGM', 'PlayLoopSFXStoreInfo', 'StopLoopSFX', 'PlayVoiceStoreInfo',
       'PlayLoopVoiceStoreInfo', 'PlayFGMStoreInfo', 'PlaySmashVoice', 'SetFlag0', 'SetFlag1', 'SetFlag2', 'SetFlag3',
       'SetAirJumpAdd', 'SetAirJumpMax', 'SetHitStatusPartAll', 'SetHitStatusPartID', 'SetHitStatusAll',
       'ResetDamageCollPartAll', 'SetDamageCollPartID', 'LoopBegin', 'LoopEnd', 'Subroutine', 'Return', 'Goto',
       'PauseScript', 'Effect', 'EffectItemHold', 'SetModelPartID', 'ResetModelPartAll', 'HideModelPartAll',
       'SetTexturePartID', 'SetColAnim', 'ResetColAnim', 'SetParallelScript', 'SetSlopeContour', 'HideItem', 'MakeRumble',
       'StopRumble', 'SetAfterImage']


def op(name, value=0):
    return (OPS.index(name) << 26) | (value & 0x3FFFFFF)


def attack(aid, gid, jid, dmg, reb, elem, sz, ox, oy, oz, ang, kbs, kbw, ga, sd, fl, fk, kbb):
    """ftMotionCommandMakeAttackColl (src/ft/ftdef.h), five words."""
    m = lambda v, n: v & ((1 << n) - 1)
    return [(3 << 26) | (aid << 23) | (gid << 20) | (m(jid, 7) << 13) | (dmg << 5) | (reb << 4) | elem,
            (sz << 16) | m(ox, 16), (m(oy, 16) << 16) | m(oz, 16), (m(ang, 10) << 22) | (kbs << 12) | (kbw << 2) | ga,
            (m(sd, 8) << 24) | (fl << 21) | (fk << 17) | (kbb << 7)]


class FakeFiles:
    """One relocData file of script words; `ptrs` maps a slot's byte offset to the (file, offset) it points at."""

    def __init__(self, words, ptrs=None):
        self.d = struct.pack('>%dI' % len(words), *words)
        self.p = ptrs or {}

    def data(self, fid):
        return self.d

    def ptr(self, ref, off=0):
        return self.p.get(ref[1] + off)


class Decode(unittest.TestCase):
    def test_mario_jab_and_signed_fields(self):
        # dMarioMainMotion_Jab1: ftMotionCommandMakeAttackColl(0, 0, 10, 2, 1, 0, 160, 0, 0, 0, 361, 50, 0, 3, 0, 0, 0, 8)
        e = mx.decode('MakeAttackColl', attack(0, 0, 10, 2, 1, 0, 160, 0, 0, 0, 361, 50, 0, 3, 0, 0, 0, 8), [])
        self.assertEqual((e['joint'], e['damage'], e['size'], e['angle'], e['knockback_scale'], e['knockback_weight'],
                          e['knockback_base'], e['hit_air'], e['hit_ground'], e['can_rebound']),
                         (10, 2, 160, 361, 50, 0, 8, True, True, True))
        # Falcon Punch's hand hitbox: (0, 0, 16, 24, 1, 0, 250, 100, 0, 0, 361, 120, 0, 3, 0, 2, 1, 30)
        e = mx.decode('MakeAttackColl', attack(0, 0, 16, 24, 1, 0, 250, 100, 0, 0, 361, 120, 0, 3, 0, 2, 1, 30), [])
        self.assertEqual((e['damage'], e['offset'], e['knockback_base'], e['fgm_level'], e['fgm_kind']), (24, [100, 0, 0], 30, 2, 1))
        # negative offsets / angle / item joint (-2)
        e = mx.decode('MakeAttackColl', attack(1, 0, -2, 5, 0, 1, 100, -40, 45, -7, -70, 100, 30, 1, -3, 0, 0, 0), [])
        self.assertEqual((e['joint'], e['offset'], e['angle'], e['shield_damage'], e['hit_air'], e['hit_ground'], e['element']),
                         (-2, [-40, 45, -7], -70, -3, True, False, 1))

    def test_sound_and_small_events(self):
        names = ['FGMA', 'VoiceMarioJump']
        self.assertEqual(mx.decode('PlayVoiceStoreInfo', [op('PlayVoiceStoreInfo', 1)], names), {'fgm': 1, 'name': 'VoiceMarioJump'})
        w = (OPS.index('SetAttackCollDamage') << 26) | (2 << 23) | (17 << 15)
        self.assertEqual(mx.decode('SetAttackCollDamage', [w], names), {'id': 2, 'damage': 17})
        self.assertEqual(mx.decode('SetHitStatusAll', [op('SetHitStatusAll', 3)], names), {'value': 3})


class Interpreter(unittest.TestCase):
    def run_words(self, words, ptrs=None):
        return mx.run_script(FakeFiles(words, ptrs), (0, 0), OPS, ['FGM0', 'FGM1'])

    def test_waits_put_events_on_frames(self):
        # WaitAsync(2); hitbox; Wait(2); ClearAll; End -> hitbox on frame 2, cleared on frame 4
        words = [op('AsyncWait', 2)] + attack(0, 0, 10, 2, 1, 0, 160, 0, 0, 0, 361, 50, 0, 3, 0, 0, 0, 8) + \
                [op('SyncWait', 2), op('ClearAttackCollAll'), op('End')]
        events, end, loops = self.run_words(words)
        self.assertEqual([(e['frame'], e['op']) for e in events], [(2, 'MakeAttackColl'), (4, 'ClearAttackCollAll'), (4, 'End')])
        self.assertEqual((end, loops), (4, []))
        w = mx.windows(events, None)
        self.assertEqual([(x['id'], x['from'], x['to'], x['set']) for x in w], [(0, 2, 4, 0)])

    def test_loops_subroutines_and_goto_back(self):
        # LoopBegin(3) { PlayFGM(1); Wait(2) } LoopEnd; Subroutine(sub); Goto(start)   sub: PlayFGM(0); Return
        words = [op('LoopBegin', 3), op('PlayFGM', 1), op('SyncWait', 2), op('LoopEnd'),
                 op('Subroutine'), 0, op('Goto'), 0,
                 op('PlayFGM', 0), op('Return')]
        ptrs = {20: (0, 32), 28: (0, 0)}
        events, end, loops = self.run_words(words, ptrs)
        self.assertEqual([(e['frame'], e['name']) for e in events], [(0, 'FGM1'), (2, 'FGM1'), (4, 'FGM1'), (6, 'FGM0')])
        self.assertIsNone(end)
        self.assertEqual(loops, [{'thread': 0, 'from': 0, 'period': 6}])

    def test_refresh_starts_new_hit_records(self):
        ev = lambda f, o, **k: dict(k, frame=f, op=o)
        a0 = mx.decode('MakeAttackColl', attack(0, 0, 10, 2, 1, 0, 160, 0, 0, 0, 361, 50, 0, 3, 0, 0, 0, 8), [])
        a1 = mx.decode('MakeAttackColl', attack(1, 0, 9, 2, 1, 0, 160, 0, 0, 0, 361, 50, 0, 3, 0, 0, 0, 8), [])
        events = [dict(a0, frame=1, op='MakeAttackColl'), dict(a1, frame=1, op='MakeAttackColl'), ev(3, 'RefreshAttackCollID', id=1),
                  ev(4, 'SetAttackCollDamage', id=0, damage=5), ev(6, 'ClearAttackCollAll')]
        w = mx.windows(events, None)
        self.assertEqual([(x['id'], x['from'], x['to'], x['set'], x['damage']) for x in w],
                         [(0, 1, 4, 0, 2), (1, 1, 3, 0, 2), (1, 3, 6, 1, 2), (0, 4, 6, 0, 5)])


@unittest.skipUnless(HAVE_DECOMP, 'reference/ssb64 not checked out')
class DecompTables(unittest.TestCase):
    def test_event_enum_and_script_offsets(self):
        self.assertEqual(mx.event_names(), OPS)
        names = mx.ex.file_names()
        tables = mx.motion_scripts(names)
        main, sub, rows = tables['Captain']
        self.assertEqual(main, names['CaptainMainMotion'])
        offs = dict((mid, off) for mid, off, _ in rows)
        self.assertEqual(offs[203], mx.offsets()['dCaptainMainMotion_FalconPunchGround'])
        main, _, rows = tables['Mario']
        self.assertEqual(dict((m, o) for m, o, _ in rows)[165], mx.offsets()['dMarioMainMotion_Jab1'])


if __name__ == '__main__':
    unittest.main()
