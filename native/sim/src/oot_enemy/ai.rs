//! The action functions of each overlay, simplified: states, trigger distances and speeds follow
//! the C (cited per function), the stems/bones and effect spawners are left to the view.
use super::*;
use crate::math::{atan2s, coss, sins, to_s16};

fn dist_xz(a: &Vec3, b: &Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn yaw_to(from: &Vec3, to: &Vec3) -> f64 {
    atan2s(to[2] - from[2], to[0] - from[0]) as f64
}

/// `Math_ApproachS` / `Math_SmoothStepToS`: turn `yaw` toward `target` by at most `step` (s16 units).
fn turn(yaw: f64, target: f64, step: f64) -> f64 {
    let d = to_s16(target - yaw) as f64;
    to_s16(yaw + d.clamp(-step, step)) as f64
}

fn floor_y(world: &CollisionWorld, x: f64, y: f64, z: f64) -> Option<f64> {
    let h = world.find_floor(x, y + 120.0, z);
    h.surface.as_ref().map(|_| h.height)
}

fn forward(yaw: f64) -> [f64; 2] {
    [sins(yaw), coss(yaw)]
}

pub fn init(e: &mut Enemy) {
    use Kind::*;
    e.visible = true;
    match e.kind {
        DekuBaba | BigDekuBaba | WitheredBaba => {
            // EnDekubaba_SetupWaitPlayerNear: buried, `actionState` 45 frames before it may rise.
            e.st = St::Hidden;
            e.aux[0] = frames(45.0);
            e.reach = 0.0;
            e.head = [e.home[0], e.home[1] + 14.0 * e.spec().scale * U, e.home[2]];
            e.play("Idle");
        }
        Octorok | MadScrub | Leever => {
            e.st = St::Hidden;
            e.visible = false;
        }
        Wolfos => e.st = St::Hidden,
        Skulltula => {
            e.st = St::Idle;
            e.aux[0] = e.home[1];
        }
        Peahat => {
            // PEAHAT_TYPE_GROUNDED (-1) waits on the ground, FLYING (0) hovers.
            e.st = if e.param == -1 { St::Idle } else { St::Chase };
            e.aux[0] = frames(280.0);
            e.aux[1] = 0.0;
            e.play(if e.param == -1 { "Landing" } else { "Idle" });
        }
        Guay | Keese => {
            e.st = St::Idle;
            e.aux[0] = 0.0;
        }
        _ => e.st = St::Idle,
    }
}

pub fn step(e: &mut Enemy, player: Vec3, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    e.age += 1.0;
    e.t += 1.0;
    e.grace = (e.grace - 1.0).max(0.0);
    e.flash = (e.flash - 1.0).max(0.0);
    e.aux[2] = (e.aux[2] - 1.0).max(0.0);
    if e.st == St::Gone {
        if e.respawn > 0.0 {
            e.respawn -= 1.0;
            if e.respawn <= 0.0 {
                e.pos = e.home;
                e.hp = e.spec().hp;
                e.visible = true;
                e.stun = 0.0;
                e.frozen = 0.0;
                e.burn = 0.0;
                init(e);
                e.set_state(e.st);
            }
        }
        return;
    }
    if e.st == St::Dying {
        dying(e, ev);
        return;
    }
    if e.frozen > 0.0 {
        e.frozen -= 1.0;
        return;
    }
    if e.burn > 0.0 {
        e.burn -= 1.0;
    }
    if e.stun > 0.0 && !e.kind.is_baba() {
        e.stun -= 1.0;
        return;
    }
    use Kind::*;
    match e.kind {
        DekuBaba | BigDekuBaba | WitheredBaba => baba(e, &player, ev),
        MadScrub => scrub(e, &player, world, rng, ev),
        Octorok => octorok(e, &player, rng, ev),
        Tektite | RedTektite => tektite(e, &player, world, rng, ev),
        Peahat | PeahatLarva => peahat(e, &player, world, rng, ev),
        Wolfos => wolfos(e, &player, world, rng, ev),
        Guay | Keese => guay(e, &player, world, rng, ev),
        Leever => leever(e, &player, world, ev),
        GomaLarva => goma(e, &player, world, rng, ev),
        Skulltula => skulltula(e, &player, world, ev),
        GoldSkulltula => gold_skulltula(e, &player, world, ev),
    }
}

/// Touch damage: the AT collider overlaps the player's cylinder (radius 37, height 160).
fn touch(e: &mut Enemy, player: &Vec3, ev: &mut Vec<Ev>) {
    if e.aux[2] > 0.0 {
        return;
    }
    let s = e.spec();
    let c = e.hit_center();
    let r = s.radius * U + 37.0;
    if dist_xz(&c, player) < r && c[1] + s.height * U > player[1] && c[1] < player[1] + 160.0 {
        e.aux[2] = frames(20.0);
        ev.push(Ev::HurtPlayer { units: s.touch, from: c, kind: e.kind });
    }
}

fn dying(e: &mut Enemy, _ev: &mut Vec<Ev>) {
    let dur = match e.kind {
        k if k.is_baba() => frames(20.0),
        Kind::Octorok => frames(20.0),
        _ => frames(24.0),
    };
    if e.kind.is_baba() {
        // EnDekubaba_Die: the head sinks into the ground while shrinking.
        e.reach = (1.0 - e.t / dur).max(0.0);
    }
    if e.t >= dur {
        e.st = St::Gone;
        e.visible = false;
        if matches!(e.kind, Kind::Guay | Kind::Keese) {
            e.respawn = frames(300.0);
        }
    }
}

// ----------------------------------------------------------------------------------------------
// Deku Baba (z_en_dekubaba.c), Withered Deku Baba (z_en_karebaba.c)

/// Length of the stem from the root to the head when fully lunged (three 20-unit stem parts + head).
fn baba_reach(e: &Enemy) -> f64 {
    e.spec().scale * 95.0 * U
}

fn baba_head(e: &mut Enemy, out: f64, lift: f64) {
    let s = e.spec().scale;
    let f = forward(e.yaw);
    e.head = [e.home[0] + f[0] * out, e.home[1] + lift * s * U, e.home[2] + f[1] * out];
    e.pos = e.home;
}

fn baba(e: &mut Enemy, p: &Vec3, ev: &mut Vec<Ev>) {
    let s = e.spec().scale;
    let d_home = dist_xz(&e.home, p);
    let dy = (p[1] - e.home[1]).abs();
    let near = dist_xz(&e.home, p);
    let _ = near;
    if e.stun > 0.0 {
        // EN_DEKUBABA_ATTACKED_TYPE_STUNNED: the head droops until the nut wears off.
        e.stun -= 1.0;
        baba_head(e, baba_reach(e) * 0.15, 25.0);
        return;
    }
    match e.st {
        St::Hidden => {
            e.aux[0] = (e.aux[0] - 1.0).max(0.0);
            e.reach = 0.0;
            baba_head(e, 0.0, 14.0);
            // EnDekubaba_WaitPlayerNear: xz < 200 and |dy| < 30 (x scale).
            if e.aux[0] == 0.0 && d_home < 200.0 * s * U && dy < 30.0 * s * U + 60.0 {
                e.set_state(St::Rising);
                e.play("Chomp");
            }
        }
        St::Rising => {
            // EnDekubaba_ExitGround: 15 frames, the head comes up facing the player.
            let dur = frames(15.0);
            let f = (e.t / dur).min(1.0);
            e.reach = f;
            e.yaw = turn(e.yaw, yaw_to(&e.home, p), 0xE38 as f64 * 1.5);
            baba_head(e, 0.0, 14.0 + 31.0 * f);
            if e.t >= dur {
                e.aux[0] = frames(30.0);
                e.set_state(St::Idle);
            }
        }
        St::Idle => {
            // EnDekubaba_ChompAir: sway above the root, face the player.
            e.yaw = turn(e.yaw, yaw_to(&e.home, p), 0x71C as f64 * 1.5);
            let sway = (e.age * 0.25).sin() * 0.12;
            baba_head(e, baba_reach(e) * (0.25 + sway), 45.0);
            e.aux[0] -= 1.0;
            if d_home > 240.0 * s * U {
                e.set_state(St::Retreat);
            } else if e.aux[0] <= 0.0 || dist_xz(&e.home, p) < 80.0 * s * U {
                e.set_state(St::Windup);
            }
        }
        St::Windup => {
            // EnDekubaba_PrepareAttack: 8 frames of rearing back.
            let dur = frames(8.0);
            e.yaw = turn(e.yaw, yaw_to(&e.home, p), 0xE38 as f64 * 1.5);
            baba_head(e, -baba_reach(e) * 0.2 * (e.t / dur).min(1.0), 55.0);
            if e.t >= dur {
                e.set_state(St::Attack);
                ev.push(Ev::Sfx { name: "deku_attack", pos: e.head });
            }
        }
        St::Attack => {
            // EnDekubaba_Attack: the head whips forward, then chomps in place for ~10 frames.
            let lunge = frames(4.0);
            let f = (e.t / lunge).min(1.0);
            baba_head(e, baba_reach(e) * (-0.2 + 1.2 * f), 55.0 - 38.0 * f);
            touch(e, p, ev);
            if e.t > frames(14.0) {
                e.set_state(St::Recover);
            }
        }
        St::Recover => {
            // EnDekubaba_RecoverFromAttackMiss: lying on the floor; hits here weaken it.
            let dur = frames(25.0);
            let f = (e.t / dur).min(1.0);
            baba_head(e, baba_reach(e) * (1.0 - 0.7 * f), 17.0 + 25.0 * f);
            if e.t >= dur {
                e.aux[0] = frames(12.0);
                e.set_state(St::Idle);
            }
        }
        St::Hurt => {
            // EnDekubaba_Attacked: the head flies back upright (about 12 frames).
            let dur = frames(12.0);
            baba_head(e, baba_reach(e) * 0.1, 35.0 + 20.0 * (e.t / dur).min(1.0));
            if e.t >= dur {
                if e.hp <= 0 {
                    kill(e, ev);
                } else if e.vulnerable_hit {
                    e.set_state(St::Vulnerable);
                } else {
                    e.aux[0] = frames(10.0);
                    e.set_state(St::Idle);
                }
            }
        }
        St::Vulnerable => {
            // EnDekubaba_Vulnerable: head on the floor for 60 frames; the next sword hit finishes it.
            baba_head(e, baba_reach(e) * 0.5, 14.0);
            if e.t >= frames(60.0) {
                e.vulnerable_hit = false;
                e.aux[0] = frames(12.0);
                e.set_state(St::Idle);
            }
        }
        St::Retreat => {
            // EnDekubaba_EnterGround.
            let dur = frames(15.0);
            let f = 1.0 - (e.t / dur).min(1.0);
            e.reach = f;
            baba_head(e, 0.0, 14.0 + 31.0 * f);
            if e.t >= dur {
                e.aux[0] = frames(45.0);
                e.set_state(St::Hidden);
            }
        }
        _ => {}
    }
}

fn kill(e: &mut Enemy, ev: &mut Vec<Ev>) {
    e.set_state(St::Dying);
    e.play("Death");
    ev.push(Ev::Sfx { name: "enemy_dead", pos: e.pos });
    ev.push(Ev::Died { id: e.index, kind: e.kind, pos: e.pos });
}

// ----------------------------------------------------------------------------------------------
// Mad Deku Scrub (z_en_dekunuts.c)

fn scrub(e: &mut Enemy, p: &Vec3, _world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    match e.st {
        St::Hidden => {
            // EnDekunuts_Wait: pop up when the player comes within range and is at a similar height.
            e.visible = false;
            if d < 350.0 * U && (p[1] - e.pos[1]).abs() < 200.0 {
                e.visible = true;
                e.set_state(St::Rising);
                e.play("Unburrow");
            }
        }
        St::Rising => {
            e.yaw = turn(e.yaw, toward, 0x1000 as f64);
            if e.t >= frames(10.0) {
                e.set_state(St::Idle);
                e.aux[0] = frames(12.0);
                e.play("Stand");
            }
        }
        St::Idle => {
            // EnDekunuts_Stand: look at the player, then spit; a close player makes it burrow and run.
            e.yaw = turn(e.yaw, toward, 0x1000 as f64);
            e.aux[0] -= 1.0;
            if d > 450.0 * U {
                e.set_state(St::Retreat);
                e.play("Burrow");
            } else if d < 80.0 * U {
                e.set_state(St::Retreat);
                e.play("Burrow");
            } else if e.aux[0] <= 0.0 {
                e.set_state(St::Windup);
                e.play("Spit");
            }
        }
        St::Windup => {
            // EnDekunuts_Shoot: the nut leaves on frame 6 of the animation.
            e.yaw = turn(e.yaw, toward, 0x800 as f64);
            if e.t >= frames(6.0) && e.aux[1] == 0.0 {
                e.aux[1] = 1.0;
                let f = forward(e.yaw);
                let from = [e.pos[0] + f[0] * 20.0 * U, e.pos[1] + 12.0 * U, e.pos[2] + f[1] * 20.0 * U];
                let v = speed(3.0);
                // EnNutsball is aimed at the player's chest.
                let dir = [p[0] - from[0], p[1] + 50.0 - from[1], p[2] - from[2]];
                let l = (dir[0].powi(2) + dir[1].powi(2) + dir[2].powi(2)).sqrt().max(1.0);
                ev.push(Ev::Spit { kind: ShotKind::Nut, pos: from, vel: [dir[0] / l * v, dir[1] / l * v, dir[2] / l * v] });
            }
            if e.t >= frames(14.0) {
                e.aux[1] = 0.0;
                e.aux[0] = frames(20.0 + rng.zero_one() * 20.0);
                e.set_state(St::Idle);
                e.play("Stand");
            }
        }
        St::Retreat => {
            if e.t >= frames(12.0) {
                e.visible = false;
                e.set_state(St::Hidden);
            }
        }
        _ => {}
    }
    if e.visible && matches!(e.st, St::Idle | St::Windup | St::Rising) {
        touch(e, p, ev);
    }
}

// ----------------------------------------------------------------------------------------------
// Octorok (z_en_okuta.c)

fn octorok(e: &mut Enemy, p: &Vec3, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    match e.st {
        St::Hidden => {
            e.visible = false;
            // EnOkuta_WaitHidden: appears when the player is within 650 and not too close.
            if d < 650.0 * U && d > 200.0 * U * 0.0 {
                e.visible = true;
                e.set_state(St::Rising);
                e.play("Appear");
            }
        }
        St::Rising => {
            if e.t >= frames(8.0) {
                e.aux[0] = frames(30.0);
                e.set_state(St::Idle);
                e.play("Idle");
            }
        }
        St::Idle => {
            e.yaw = turn(e.yaw, toward, 0x400 as f64);
            e.aux[0] -= 1.0;
            if d > 700.0 * U || d < 60.0 * U {
                e.set_state(St::Retreat);
                e.play("Hide");
            } else if e.aux[0] <= 0.0 && d < 600.0 * U {
                e.set_state(St::Windup);
                e.play("Shoot");
            }
        }
        St::Windup => {
            // EnOkuta_Shoot: the rock spawns 8 frames into the animation, in front of the mouth.
            e.yaw = turn(e.yaw, toward, 0x400 as f64);
            if e.t >= frames(8.0) && e.aux[1] == 0.0 {
                e.aux[1] = 1.0;
                let f = forward(e.yaw);
                let from = [e.pos[0] + f[0] * 25.0 * U, e.pos[1] + 30.0 * U, e.pos[2] + f[1] * 25.0 * U];
                let v = speed(10.0);
                ev.push(Ev::Spit { kind: ShotKind::Rock, pos: from, vel: [f[0] * v, 0.0, f[1] * v] });
            }
            if e.t >= frames(18.0) {
                e.aux[1] = 0.0;
                e.aux[0] = frames(40.0 + rng.zero_one() * 20.0);
                e.set_state(St::Idle);
                e.play("Idle");
            }
        }
        St::Retreat => {
            if e.t >= frames(8.0) {
                e.visible = false;
                e.set_state(St::Hidden);
            }
        }
        _ => {}
    }
    if e.visible && e.st != St::Retreat {
        touch(e, p, ev);
    }
}

// ----------------------------------------------------------------------------------------------
// Tektite (z_en_tite.c)

fn tektite(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    let floor = floor_y(world, e.pos[0], e.pos[1], e.pos[2]).unwrap_or(e.home[1]);
    let gravity = -1.0 * U / (T * T);
    match e.st {
        St::Idle => {
            // EnTite_Idle: squats, turns toward a nearby player, hops when within 400.
            e.pos[1] = floor;
            e.vel = [0.0; 3];
            e.yaw = turn(e.yaw, toward, 0x400 as f64);
            e.play("Idle");
            if e.t > frames(20.0) && d < 400.0 * U {
                e.set_state(St::Windup);
            }
        }
        St::Windup => {
            // The legs coil for ~8 frames, then EnTite_Attack: a hop of 4 forward / 6.5 up.
            e.pos[1] = floor;
            e.yaw = turn(e.yaw, toward, 0x800 as f64);
            if e.t >= frames(8.0) {
                let f = forward(e.yaw);
                let fwd = if d > 90.0 * U { speed(4.0) } else { speed(2.0) };
                e.vel = [f[0] * fwd, speed(6.0 + rng.zero_one()), f[1] * fwd];
                e.set_state(St::Attack);
                e.play("Walk");
            }
        }
        St::Attack => {
            e.vel[1] += gravity;
            e.pos = [e.pos[0] + e.vel[0], e.pos[1] + e.vel[1], e.pos[2] + e.vel[2]];
            touch(e, p, ev);
            if e.vel[1] < 0.0 && e.pos[1] <= floor {
                e.pos[1] = floor;
                e.vel = [0.0; 3];
                e.set_state(St::Idle);
                e.play("Land");
            }
        }
        St::Hurt => {
            e.vel[1] += gravity;
            e.pos = [e.pos[0] + e.vel[0], (e.pos[1] + e.vel[1]).max(floor), e.pos[2] + e.vel[2]];
            e.vel[0] *= 0.92;
            e.vel[2] *= 0.92;
            if e.t >= frames(10.0) && e.pos[1] <= floor + 1.0 {
                e.set_state(St::Idle);
            }
        }
        _ => e.set_state(St::Idle),
    }
}

// ----------------------------------------------------------------------------------------------
// Peahat (z_en_peehat.c): propeller body, only its roots take damage while it flies.

pub const PEAHAT_FLY_HEIGHT: f64 = 100.0;

fn peahat(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let larva = e.kind == Kind::PeahatLarva;
    let floor = floor_y(world, e.pos[0], e.pos[1], e.pos[2]).unwrap_or(e.home[1]);
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    match e.st {
        St::Idle => {
            // EnPeehat_Ground: sits on the floor until the player is within 740 (xzDistMax), then rises.
            e.pos[1] = floor;
            e.play("Landing");
            if d < if larva { 400.0 } else { 740.0 } * U && e.t > frames(20.0) {
                e.set_state(St::Rising);
                e.play("Rising");
            }
        }
        St::Rising => {
            // 30 frames of takeoff.
            let f = (e.t / frames(30.0)).min(1.0);
            e.pos[1] = floor + PEAHAT_FLY_HEIGHT * U * 0.6 * f;
            if e.t >= frames(30.0) {
                e.aux[0] = frames(if larva { 400.0 } else { 280.0 });
                e.set_state(St::Chase);
                e.play("Idle");
            }
        }
        St::Chase => {
            // EnPeehat_Flying: follows the player at 2.5 + noise per frame; lands after a while.
            e.aux[0] -= 1.0;
            let sp = if larva { speed(6.0) } else { speed(2.7) };
            e.yaw = turn(e.yaw, toward, 0x300 as f64);
            if d > 30.0 * U {
                let f = forward(e.yaw);
                e.pos[0] += f[0] * sp;
                e.pos[2] += f[1] * sp;
            }
            let want = floor + PEAHAT_FLY_HEIGHT * U * (0.6 + 0.1 * (e.age * 0.07).sin());
            e.pos[1] += (want - e.pos[1]).clamp(-speed(1.0), speed(1.0));
            e.aux[1] += 0.9;
            touch(e, p, ev);
            if e.aux[0] <= 0.0 && !larva {
                e.set_state(St::Retreat);
                e.play("Landing");
            }
            if e.param == -1 && d > 900.0 * U {
                e.set_state(St::Retreat);
            }
        }
        St::Retreat => {
            // EnPeehat_Land: sinks, rests (vulnerable everywhere) for 4 seconds, takes off again.
            e.pos[1] += (floor - e.pos[1]).clamp(-speed(2.0), 0.0);
            if e.pos[1] <= floor + 1.0 && e.t > frames(100.0) {
                e.set_state(St::Idle);
            }
        }
        St::Hurt => {
            // EnPeehat_HitWhenGrounded / recoil.
            if e.t >= frames(14.0) {
                e.set_state(if e.pos[1] > floor + 20.0 { St::Chase } else { St::Idle });
                e.aux[0] = frames(200.0);
            }
        }
        _ => e.set_state(St::Idle),
    }
    let _ = rng;
}

/// Is the Peahat in the air (its blades shield it from the top)?
pub fn peahat_airborne(e: &Enemy) -> bool {
    matches!(e.st, St::Chase | St::Rising)
}

// ----------------------------------------------------------------------------------------------
// Wolfos (z_en_wf.c)

fn wolfos(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    let floor = floor_y(world, e.pos[0], e.pos[1], e.pos[2]).unwrap_or(e.pos[1]);
    e.pos[1] = floor.max(e.pos[1] - speed(3.0));
    e.aux[1] = (e.aux[1] - 1.0).max(0.0);
    match e.st {
        St::Hidden => {
            // EnWf_WaitAppear: stays put (crouched) until the player is within 700.
            e.visible = true;
            e.play("Idle");
            if d < 700.0 * U {
                e.set_state(St::Idle);
                ev.push(Ev::Sfx { name: "wolfos_howl", pos: e.pos });
                e.play("Howl");
            }
        }
        St::Idle => {
            // EnWf_Wait/RunAtPlayer: circle the player at ~200 and wait for an opening.
            e.yaw = turn(e.yaw, toward, 0x500 as f64);
            if e.t < frames(30.0) {
                e.play("Howl");
            } else if d > 260.0 * U {
                let f = forward(e.yaw);
                e.pos[0] += f[0] * speed(6.5);
                e.pos[2] += f[1] * speed(6.5);
                e.play("Run");
            } else if d > 140.0 * U {
                // strafe sideways
                let side = toward + 0x4000 as f64;
                let f = forward(side);
                e.pos[0] += f[0] * speed(4.0);
                e.pos[2] += f[1] * speed(4.0);
                e.play("Side");
                if e.t > frames(40.0 + rng.zero_one() * 40.0) {
                    e.set_state(St::Windup);
                }
            } else {
                e.set_state(St::Windup);
            }
        }
        St::Windup => {
            // EnWf_PrepareAttack / snarl, 10 frames.
            e.yaw = turn(e.yaw, toward, 0x800 as f64);
            e.play("Idle");
            if e.t >= frames(10.0) {
                e.set_state(St::Attack);
                e.play("Attack");
            }
        }
        St::Attack => {
            // EnWf_Attack: a bite lunge at 6.5/frame for ~16 frames.
            let f = forward(e.yaw);
            e.pos[0] += f[0] * speed(6.5);
            e.pos[2] += f[1] * speed(6.5);
            touch(e, p, ev);
            if e.t >= frames(16.0) || (d < 50.0 * U && e.t > frames(6.0)) {
                e.set_state(St::Recover);
                e.play("Idle");
            }
        }
        St::Recover => {
            // EnWf_Recover: backs off.
            let f = forward(e.yaw);
            e.pos[0] -= f[0] * speed(4.0);
            e.pos[2] -= f[1] * speed(4.0);
            if e.t >= frames(20.0) {
                e.set_state(St::Idle);
            }
        }
        St::Hurt => {
            // EnWf_Damaged: knocked back at 6.
            let f = forward(e.yaw);
            e.pos[0] -= f[0] * speed(5.0) * (1.0 - e.t / frames(14.0)).max(0.0);
            e.pos[2] -= f[1] * speed(5.0) * (1.0 - e.t / frames(14.0)).max(0.0);
            e.play("Damaged");
            if e.t >= frames(14.0) {
                e.set_state(St::Idle);
                e.t = frames(30.0);
            }
        }
        _ => e.set_state(St::Idle),
    }
}

// ----------------------------------------------------------------------------------------------
// Guay (z_en_crow.c)

fn guay(e: &mut Enemy, p: &Vec3, _world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    match e.st {
        St::Idle => {
            // EnCrow_Wait / flight in circles over its home point.
            let a = e.age * 0.03;
            let target = [e.home[0] + a.cos() * 120.0 * U * 0.5, e.home[1], e.home[2] + a.sin() * 120.0 * U * 0.5];
            let yaw = yaw_to(&e.pos, &target);
            e.yaw = turn(e.yaw, yaw, 0x500 as f64);
            let f = forward(e.yaw);
            e.pos[0] += f[0] * speed(3.0);
            e.pos[2] += f[1] * speed(3.0);
            e.pos[1] += (target[1] - e.pos[1]).clamp(-speed(1.0), speed(1.0));
            // Notices the player within 300 and dives.
            if d < 300.0 * U && (e.pos[1] - p[1]).abs() < 400.0 * U {
                e.set_state(St::Attack);
            }
        }
        St::Attack => {
            // EnCrow_Dive: speed 4 straight at the player's chest.
            let tx = [p[0], p[1] + 40.0, p[2]];
            let dir = [tx[0] - e.pos[0], tx[1] - e.pos[1], tx[2] - e.pos[2]];
            let l = (dir[0].powi(2) + dir[1].powi(2) + dir[2].powi(2)).sqrt().max(1.0);
            let v = speed(4.0);
            e.yaw = turn(e.yaw, yaw_to(&e.pos, p), 0x900 as f64);
            e.pos = [e.pos[0] + dir[0] / l * v, e.pos[1] + dir[1] / l * v, e.pos[2] + dir[2] / l * v];
            touch(e, p, ev);
            if e.aux[2] > 0.0 || e.t > frames(60.0) {
                e.set_state(St::Recover);
            }
        }
        St::Recover => {
            // Flies up and away for ~2 seconds, then circles again.
            e.pos[1] += speed(2.0);
            let f = forward(e.yaw + 0x8000 as f64);
            e.pos[0] += f[0] * speed(2.5);
            e.pos[2] += f[1] * speed(2.5);
            if e.t > frames(40.0 + rng.zero_one() * 20.0) {
                e.set_state(St::Idle);
            }
        }
        St::Hurt => {
            // Falls and flutters.
            e.pos[1] -= speed(1.5);
            if e.t > frames(10.0) {
                e.set_state(St::Idle);
            }
        }
        _ => e.set_state(St::Idle),
    }
}

// ----------------------------------------------------------------------------------------------
// Leever (z_en_reeba.c)

fn leever(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    let floor = floor_y(world, e.pos[0], e.pos[1], e.pos[2]).unwrap_or(e.pos[1]);
    e.pos[1] = floor;
    match e.st {
        St::Idle | St::Hidden => {
            e.visible = true;
            e.set_state(St::Rising);
        }
        St::Rising => {
            // EnReeba_Rise: burrowing up out of the sand.
            if e.t >= frames(15.0) {
                e.aux[0] = frames(240.0);
                e.set_state(St::Chase);
            }
        }
        St::Chase => {
            // EnReeba_Move: spins toward the player at 6 + noise.
            e.aux[0] -= 1.0;
            e.yaw = turn(e.yaw, toward, 0x400 as f64);
            let f = forward(e.yaw);
            let sp = speed(if d < 80.0 * U { 3.0 } else { 6.0 });
            e.pos[0] += f[0] * sp;
            e.pos[2] += f[1] * sp;
            touch(e, p, ev);
            if e.aux[0] <= 0.0 {
                e.set_state(St::Retreat);
            }
        }
        St::Retreat => {
            if e.t >= frames(15.0) {
                e.st = St::Gone;
                e.visible = false;
            }
        }
        St::Hurt => {
            let f = forward(e.yaw);
            e.pos[0] -= f[0] * speed(8.0) * (1.0 - e.t / frames(10.0)).max(0.0);
            e.pos[2] -= f[1] * speed(8.0) * (1.0 - e.t / frames(10.0)).max(0.0);
            if e.t >= frames(10.0) {
                e.set_state(St::Chase);
            }
        }
        _ => {}
    }
}

// ----------------------------------------------------------------------------------------------
// Gohma larva (z_en_goma.c)

fn goma(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    let toward = yaw_to(&e.pos, p);
    let floor = floor_y(world, e.pos[0], e.pos[1], e.pos[2]).unwrap_or(e.home[1]);
    let gravity = -1.3 * U / (T * T);
    match e.st {
        St::Idle => {
            // EnGoma_Walk: skitters toward the player at 1.5..3.8.
            e.pos[1] = floor;
            e.play("Walk");
            if d < 600.0 * U {
                e.yaw = turn(e.yaw, toward, 0x400 as f64);
                let f = forward(e.yaw);
                e.pos[0] += f[0] * speed(2.0);
                e.pos[2] += f[1] * speed(2.0);
                e.play("Run");
                if d < 90.0 * U && e.t > frames(30.0) {
                    e.set_state(St::Windup);
                    e.play("Prepare");
                }
            }
        }
        St::Windup => {
            // EnGoma_PrepareJump: 10 frames.
            e.yaw = turn(e.yaw, toward, 0x800 as f64);
            if e.t >= frames(10.0) {
                let f = forward(e.yaw);
                e.vel = [f[0] * speed(6.0), speed(5.0 + rng.zero_one()), f[1] * speed(6.0)];
                e.set_state(St::Attack);
                e.play("Jump");
            }
        }
        St::Attack => {
            e.vel[1] += gravity;
            e.pos = [e.pos[0] + e.vel[0], e.pos[1] + e.vel[1], e.pos[2] + e.vel[2]];
            touch(e, p, ev);
            if e.vel[1] < 0.0 && e.pos[1] <= floor {
                e.pos[1] = floor;
                e.set_state(St::Recover);
                e.play("Land");
            }
        }
        St::Recover => {
            if e.t >= frames(20.0) {
                e.set_state(St::Idle);
            }
        }
        St::Hurt => {
            e.play("Damaged");
            if e.t >= frames(12.0) {
                e.set_state(St::Idle);
            }
        }
        _ => e.set_state(St::Idle),
    }
}

// ----------------------------------------------------------------------------------------------
// Skulltula (z_en_st.c) and Gold Skulltula (z_en_sw.c)

fn skulltula(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.home, p);
    let ceiling = e.home[1];
    match e.st {
        St::Idle => {
            // Hangs from its thread, turning slowly; lowers when the player passes beneath.
            e.yaw = turn(e.yaw, e.yaw + 0x200 as f64, 0x200 as f64 * 0.2);
            e.pos[1] += (ceiling - e.pos[1]).clamp(-speed(1.5), speed(1.5));
            e.play("Idle");
            if d < 300.0 * U && p[1] < ceiling {
                e.set_state(St::Chase);
            }
        }
        St::Chase => {
            // EnSt_MoveToGround-ish: drops to the player's height and sidles over.
            let want = (p[1] + 40.0).max(floor_y(world, e.pos[0], ceiling, e.pos[2]).unwrap_or(p[1]) + 20.0);
            e.pos[1] += (want - e.pos[1]).clamp(-speed(2.5), speed(2.5));
            let toward = yaw_to(&e.pos, p);
            e.yaw = turn(e.yaw, toward, 0x500 as f64);
            if dist_xz(&e.pos, &e.home) < 250.0 * U {
                let f = forward(e.yaw);
                e.pos[0] += f[0] * speed(1.5);
                e.pos[2] += f[1] * speed(1.5);
            }
            touch(e, p, ev);
            if d > 380.0 * U {
                e.set_state(St::Retreat);
            }
        }
        St::Retreat => {
            e.pos[1] += (ceiling - e.pos[1]).clamp(-speed(2.0), speed(2.0));
            let to_home = yaw_to(&e.pos, &e.home);
            if dist_xz(&e.pos, &e.home) > 5.0 {
                let f = forward(to_home);
                e.pos[0] += f[0] * speed(2.0);
                e.pos[2] += f[1] * speed(2.0);
            }
            if (e.pos[1] - ceiling).abs() < 2.0 {
                e.set_state(St::Idle);
            }
        }
        St::Hurt => {
            if e.t >= frames(10.0) {
                e.set_state(St::Chase);
            }
        }
        _ => e.set_state(St::Idle),
    }
}

fn gold_skulltula(e: &mut Enemy, p: &Vec3, world: &CollisionWorld, ev: &mut Vec<Ev>) {
    let d = dist_xz(&e.pos, p);
    match e.st {
        St::Idle => {
            // Clings to the wall; startles when the player is within 120 and drops to the floor.
            e.pos[1] = e.home[1] + (e.age * 0.12).sin() * 3.0;
            if d < 140.0 * U {
                e.set_state(St::Chase);
            }
        }
        St::Chase => {
            let floor = floor_y(world, e.pos[0], e.pos[1], e.pos[2]).unwrap_or(e.home[1]);
            e.pos[1] += (floor + 15.0 - e.pos[1]).clamp(-speed(3.0), speed(3.0));
            e.yaw = turn(e.yaw, yaw_to(&e.pos, p), 0x600 as f64);
            let f = forward(e.yaw);
            e.pos[0] += f[0] * speed(4.0);
            e.pos[2] += f[1] * speed(4.0);
            touch(e, p, ev);
        }
        _ => e.set_state(St::Idle),
    }
}

// ----------------------------------------------------------------------------------------------
// Shots

/// Advance rocks and nuts; they hurt the player on contact and die on the floor or after 4 seconds.
pub fn step_shots(shots: &mut Vec<Shot>, player: Vec3, world: &CollisionWorld, ev: &mut Vec<Ev>) {
    for s in shots.iter_mut() {
        s.age += 1.0;
        s.pos = [s.pos[0] + s.vel[0], s.pos[1] + s.vel[1], s.pos[2] + s.vel[2]];
        let floor = floor_y(world, s.pos[0], s.pos[1], s.pos[2]);
        if floor.is_some_and(|f| s.pos[1] < f - 5.0) || s.age > frames(80.0) {
            s.dead = true;
        }
        if !s.reflected && dist_xz(&s.pos, &player) < 37.0 + 12.0 * U && s.pos[1] > player[1] - 20.0 && s.pos[1] < player[1] + 170.0 {
            s.dead = true;
            ev.push(Ev::HurtPlayer { units: 8, from: s.pos, kind: if s.kind == ShotKind::Rock { Kind::Octorok } else { Kind::MadScrub } });
        }
    }
    shots.retain(|s| !s.dead);
}

// ----------------------------------------------------------------------------------------------
// Taking a hit

/// Resolve a weapon hit against the damage table and the enemy's state; returns what happened.
pub fn on_hit(e: &mut Enemy, dmg: Dmg, from: Vec3, rng: &mut Rng, ev: &mut Vec<Ev>) -> HitResult {
    let mut r = HitResult::default();
    if !e.targetable() || e.hp <= 0 && !e.kind.is_baba() {
        return r;
    }
    let (damage, react) = damage_of(e.kind, dmg);
    let spark = e.hit_center();
    // Untouchable states: buried Babas, the Peahat's blades, a Wolfos with its guard up.
    if e.kind.is_baba() && matches!(e.st, St::Hidden | St::Retreat) {
        r.clank = true;
        ev.push(Ev::Clank { pos: spark });
        return r;
    }
    if e.kind.is_baba() && e.st == St::Hurt {
        return r;
    }
    if matches!(e.kind, Kind::Peahat | Kind::PeahatLarva) && peahat_airborne(e) && e.kind == Kind::Peahat {
        // Only the roots underneath are soft: the hit must come from below the body.
        if from[1] > e.pos[1] + 20.0 * U && !matches!(dmg, Dmg::Explosive | Dmg::FireMagic | Dmg::IceMagic | Dmg::Boomerang | Dmg::Hookshot) {
            r.clank = true;
            ev.push(Ev::Clank { pos: spark });
            return r;
        }
    }
    if e.kind == Kind::Wolfos && e.st != St::Hurt && dmg.is_sword() && e.aux[1] <= 0.0 {
        // EnWf_Block: faces the attacker and raises its guard against frontal sword blows 40% of the time.
        let toward = yaw_to(&e.pos, &from);
        let front = to_s16(toward - e.yaw).abs() < 0x3000;
        if front && rng.zero_one() < 0.4 && matches!(e.st, St::Idle | St::Windup | St::Recover) {
            e.aux[1] = frames(20.0);
            e.play("Block");
            r.clank = true;
            ev.push(Ev::Clank { pos: spark });
            return r;
        }
    }
    if damage == 0 && react == 0 {
        return r;
    }
    r.applied = true;
    r.damage = damage;
    ev.push(Ev::Spark { pos: spark, hard: false });
    ev.push(Ev::Hurt { kind: e.kind, dmg, pos: spark });
    e.flash = frames(8.0);
    e.killed_by = Some(dmg);

    // Stun (nut, boomerang, hookshot rows with reaction 1).
    if react == 1 && damage == 0 {
        e.stun = frames(40.0);
        return r;
    }
    // Deku Baba reaction codes: 0xE boomerang, 0xF sword, 2 fire (EnDekubaba_CheckCollide).
    if e.kind.is_baba() {
        let vulnerable = e.st == St::Vulnerable;
        let new_hp = e.hp - damage;
        if react == 0xE && !vulnerable {
            // The boomerang stuns without hurting.
            e.stun = frames(40.0);
            return r;
        }
        if !vulnerable {
            let weakened = e.st == St::Recover;
            e.hp = if weakened && new_hp <= 0 { 1 } else { new_hp.max(0) };
            e.vulnerable_hit = weakened;
            if react == 2 {
                e.burn = frames(40.0);
            }
            e.set_state(St::Hurt);
            e.play("Chomp");
            ev.push(Ev::Sfx { name: "deku_damage", pos: spark });
        } else if react == 0xE || react == 0xF {
            if new_hp > 0 {
                e.hp = new_hp;
                e.set_state(St::Hurt);
                e.vulnerable_hit = true;
            } else {
                e.hp = 0;
                r.killed = true;
                kill(e, ev);
            }
        } else {
            e.hp = new_hp.max(0);
            e.set_state(St::Hurt);
        }
        // A Baba left at 0 hit points dies when its recoil finishes (EnDekubaba_Attacked -> SetupDie).
        return r;
    }

    e.hp -= damage;
    if react == 2 {
        e.burn = frames(40.0);
    }
    if (react == 0xF || react == 3) && !matches!(e.kind, Kind::DekuBaba | Kind::BigDekuBaba) {
        e.frozen = frames(60.0);
    }
    if react == 1 {
        e.stun = frames(30.0);
    }
    if e.hp <= 0 {
        r.killed = true;
        kill_generic(e, ev);
        return r;
    }
    // Flinch: knocked away from the blow.
    let away = yaw_to(&from, &e.pos);
    let f = forward(away);
    match e.kind {
        Kind::Tektite | Kind::RedTektite | Kind::GomaLarva => {
            e.vel = [f[0] * speed(3.0), speed(4.0), f[1] * speed(3.0)];
            e.set_state(St::Hurt);
        }
        Kind::Wolfos | Kind::Leever | Kind::Guay | Kind::Keese | Kind::Skulltula | Kind::Peahat | Kind::PeahatLarva => {
            e.pos[0] += f[0] * speed(3.0);
            e.pos[2] += f[1] * speed(3.0);
            e.set_state(St::Hurt);
        }
        _ => {}
    }
    r
}

fn kill_generic(e: &mut Enemy, ev: &mut Vec<Ev>) {
    kill(e, ev);
}

