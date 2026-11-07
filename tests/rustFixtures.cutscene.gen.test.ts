// Goldens for the cutscene/object/holding/ending actions and the Link/Banjo controllers:
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.cutscene.gen.test.ts`
// then `cargo test -p jumpman-sim --test cutscene_chars` in native/.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import * as S from '../src/sim/state';

function rng(seed: number) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

type Row = (number | null)[];
interface Init { action: number; arg: number; dy: number; fv: number; sign?: number[]; }

const BK = (arg: number) => arg >= 0x60 && arg <= 0x66;

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust cutscene fixtures', () => {
  it('writes cutscene / link / banjo goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
    const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
    const r = rng(4242);
    const quiet = (n: number): Row[] => Array.from({ length: n }, () => [0, 0, 0, 0, 0, null]);
    const noise = (n: number, focus: boolean): Row[] => Array.from({ length: n }, (_, f) => {
      const sx = Math.round((r() - 0.5) * 128), sy = Math.round((r() - 0.5) * 128);
      let down = 0, pressed = 0;
      if (r() < 0.12) pressed |= S.A_BUTTON; if (r() < 0.12) pressed |= S.B_BUTTON;
      if (r() < 0.08) { down |= S.Z_TRIG; if (r() < 0.5) pressed |= S.Z_TRIG; }
      down |= pressed & (S.A_BUTTON | S.B_BUTTON); if (r() < 0.3) down |= S.A_BUTTON;
      return [sx, sy, Math.min(64, Math.hypot(sx, sy)), down, pressed, focus ? Math.floor(r() * 65536) - 32768 : null];
    });
    const press = (n: number, at: number, pressed: number, down: number): Row[] =>
      Array.from({ length: n }, (_, f) => [0, 0, 0, f >= at && f < at + 20 ? down : 0, f === at ? pressed : 0, null]);

    const scenarios: Record<string, { kind: 'act' | 'link' | 'banjo'; init: Init; script: Row[]; mine: number[] }> = {};
    const mineAct = [
      S.ACT_DEATH_ON_BACK, S.ACT_DEATH_ON_STOMACH, S.ACT_STANDING_DEATH, S.ACT_QUICKSAND_DEATH, S.ACT_SPAWN_SPIN_AIRBORNE,
      S.ACT_SPAWN_NO_SPIN_AIRBORNE, S.ACT_SPAWN_SPIN_LANDING, S.ACT_SPAWN_NO_SPIN_LANDING, S.ACT_EXIT_AIRBORNE,
      S.ACT_SPECIAL_EXIT_AIRBORNE, S.ACT_FALLING_EXIT_AIRBORNE, S.ACT_EXIT_LAND_SAVE_DIALOG, S.ACT_DEATH_EXIT,
      S.ACT_SPECIAL_DEATH_EXIT, S.ACT_FALLING_DEATH_EXIT, S.ACT_UNUSED_DEATH_EXIT, S.ACT_FALL_AFTER_STAR_GRAB,
      S.ACT_STAR_DANCE_EXIT, S.ACT_STAR_DANCE_NO_EXIT, S.ACT_READING_SIGN, S.ACT_TELEPORT_FADE_IN, S.ACT_TELEPORT_FADE_OUT,
      S.ACT_JUMBO_STAR_CUTSCENE, S.ACT_END_PEACH_CUTSCENE, S.ACT_PUNCHING,
    ];
    const a = (action: number, arg: number, dy: number, n: number, script?: Row[], extra: Partial<Init> = {}) =>
      ({ kind: 'act' as const, init: { action, arg, dy, fv: 0, ...extra }, script: script ?? quiet(n), mine: mineAct });
    scenarios.death_back = a(S.ACT_DEATH_ON_BACK, 0, 0, 60);
    scenarios.death_stomach = a(S.ACT_DEATH_ON_STOMACH, 0, 0, 40);
    scenarios.standing_death = a(S.ACT_STANDING_DEATH, 0, 0, 85);
    scenarios.quicksand = a(S.ACT_QUICKSAND_DEATH, 0, 0, 45);
    scenarios.spawn_spin = a(S.ACT_SPAWN_SPIN_AIRBORNE, 0, 900, 80, undefined, { fv: 10 });
    scenarios.spawn_nospin = a(S.ACT_SPAWN_NO_SPIN_AIRBORNE, 0, 600, 60);
    scenarios.exit_painting = a(S.ACT_EXIT_AIRBORNE, 0, 700, 80);
    scenarios.exit_launch = a(S.ACT_SPECIAL_EXIT_AIRBORNE, 0, 700, 80);
    scenarios.exit_falling = a(S.ACT_FALLING_EXIT_AIRBORNE, 0, 700, 80);
    scenarios.death_exit = a(S.ACT_DEATH_EXIT, 0, 700, 90);
    scenarios.death_exit_launch = a(S.ACT_SPECIAL_DEATH_EXIT, 0, 700, 90);
    scenarios.death_exit_unused = a(S.ACT_UNUSED_DEATH_EXIT, 0, 500, 60);
    scenarios.star_grab = a(S.ACT_FALL_AFTER_STAR_GRAB, 0, 500, 60);
    scenarios.star_grab_noexit = a(S.ACT_FALL_AFTER_STAR_GRAB, 1, 500, 60);
    scenarios.star_dance = a(S.ACT_STAR_DANCE_EXIT, 0, 0, 20);
    scenarios.sign = a(S.ACT_READING_SIGN, 0, 0, 20, undefined, { sign: [3, 40, -2] });
    scenarios.teleport_in = a(S.ACT_TELEPORT_FADE_IN, 0, 0, 40);
    scenarios.teleport_out = a(S.ACT_TELEPORT_FADE_OUT, 0, 0, 10);
    scenarios.punch = a(S.ACT_PUNCHING, 0, 0, 60, press(60, 5, S.B_BUTTON, S.B_BUTTON).map((row, f) => f === 12 || f === 30 ? [0, 0, 0, S.B_BUTTON, S.B_BUTTON, null] : row));
    scenarios.jumbo_flight = a(S.ACT_JUMBO_STAR_CUTSCENE, 2, 0, 560);
    scenarios.jumbo_start = a(S.ACT_JUMBO_STAR_CUTSCENE, 0, 400, 320);
    scenarios.end_peach = a(S.ACT_END_PEACH_CUTSCENE, 0, 400, 1900);
    // Link: IDLE/WALKING/PUNCHING/FREEFALL are fully handled by stepLink.
    const linkMine = [S.ACT_IDLE, S.ACT_WALKING, S.ACT_PUNCHING, S.ACT_FREEFALL];
    scenarios.link_free = { kind: 'link', init: { action: S.ACT_IDLE, arg: 0, dy: 0, fv: 0 }, script: noise(400, false), mine: linkMine };
    scenarios.link_focus = { kind: 'link', init: { action: S.ACT_IDLE, arg: 0, dy: 0, fv: 0 }, script: noise(400, true), mine: linkMine };
    // Banjo: compare a frame when the move was already BK (checked below) or on frame 0.
    const bk = (name: string, action: number, arg: number, dy: number, fv: number, script: Row[]) =>
      (scenarios[name] = { kind: 'banjo', init: { action, arg, dy, fv }, script, mine: [] });
    bk('banjo_jump', S.ACT_IDLE, 0, 0, 0, press(70, 0, S.A_BUTTON, S.A_BUTTON));
    bk('banjo_jump_walk', S.ACT_WALKING, 0, 0, 10, press(70, 0, S.A_BUTTON, S.A_BUTTON).map(row => [0, 64, 64, row[3], row[4], null]));
    bk('banjo_flap', S.ACT_FREEFALL, 0, 800, 8, press(120, 0, S.A_BUTTON, S.A_BUTTON).map((row, f) => [0, 40, 40, f < 90 ? S.A_BUTTON : 0, row[4], null]));
    bk('banjo_claw', S.ACT_IDLE, 0, 0, 0, press(50, 0, S.B_BUTTON, S.B_BUTTON));
    bk('banjo_roll', S.ACT_WALKING, 0, 0, 12, press(50, 0, S.B_BUTTON, S.B_BUTTON).map(row => [0, 64, 64, row[3], row[4], null]));
    bk('banjo_barge', S.ACT_CROUCHING, 0, 0, 0, press(60, 0, S.B_BUTTON, S.B_BUTTON).map(row => [0, 64, 64, row[3], row[4], null]));
    bk('banjo_flip', S.ACT_CROUCHING, 0, 0, 0, press(70, 0, S.A_BUTTON, S.A_BUTTON));
    bk('banjo_peck', S.ACT_JUMP, 0, 600, 6, press(60, 0, S.B_BUTTON, S.B_BUTTON).map(row => [0, 30, 30, row[3], row[4], null]));

    const fixtures: Record<string, unknown> = {};
    for (const [name, sc] of Object.entries(scenarios)) {
      const m = new MarioRuntime(world, { pos: data.spawn.pos, faceYaw: data.spawn.faceYaw });
      m.setAction(sc.init.action, sc.init.arg);
      m.pos[1] += sc.init.dy;
      m.forwardVel = sc.init.fv;
      if (sc.init.sign) m.signMotion = sc.init.sign as [number, number, number];
      const frames: number[][] = [], cmp: boolean[] = [];
      let prevAction = m.action, prevArg = m.actionArg, mineNow = true;
      sc.script.forEach(([stickX, stickY, stickMag, buttonDown, buttonPressed, focus], i) => {
        const c: any = { stickX, stickY, stickMag, buttonDown, buttonPressed };
        if (focus !== null && focus !== undefined) c.focusYaw = focus;
        const doCompare = sc.kind === 'banjo' ? (i === 0 || BK(prevArg)) : sc.mine.includes(prevAction);
        m.step(c, 0, sc.kind === 'link', sc.kind === 'banjo');
        frames.push([...m.pos, ...m.vel, ...m.faceAngle, m.action, m.forwardVel, m.health, m.actionArg, m.actionState, m.actionTimer, m.flags]);
        // The frame must also end in an action this area owns: otherwise the loop ran a not-yet-ported foreign handler.
        const postMine = sc.kind === 'banjo' ? BK(m.actionArg) : sc.mine.includes(m.action);
        cmp.push(doCompare && mineNow && postMine);
        if (!doCompare) mineNow = false;
        prevAction = m.action; prevArg = m.actionArg;
      });
      fixtures[name] = { kind: sc.kind, init: sc.init, script: sc.script, frames, cmp };
    }
    writeFileSync(`${out}/cutscene_chars_bob.json`, JSON.stringify(fixtures));
  });
});
