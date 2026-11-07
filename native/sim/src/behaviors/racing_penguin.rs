//! Racing penguin on the CCM slide (src/sim/behaviors/racingPenguin.ts).
use crate::math::js_cos;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub fn init_racer(o: &mut SimObject) {
    o.scale = 4.0;
    o.set_var("animation", 3.0);
    o.set_var("waypoint", 0.0);
    o.set_var("airFrames", 0.0);
    o.set_var("weightedSpeed", 0.0);
    o.set_var("cooldown", 0.0);
}

fn turn(o: &mut SimObject, target: f64, speed: f64) -> bool {
    let delta = to_s16(target - o.move_yaw) as f64;
    o.move_yaw = to_s16(o.move_yaw + clamp(delta, -speed, speed)) as f64;
    o.face_yaw = o.move_yaw;
    o.move_yaw == to_s16(target) as f64
}

/// cur_obj_move_standard(78): the racer uses zero drag, gravity -8 and 5% bounce.
fn do_move(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let o = &mut m.objects[id];
    let mut next = [o.pos[0] + o.forward_vel * sins(o.move_yaw), o.pos[1], o.pos[2] + o.forward_vel * coss(o.move_yaw)];
    o.hit_wall = world.resolve_and_return_wall_collisions(&mut next, 10.0, 300.0).is_some();
    let floor = world.find_floor(next[0], next[1], next[2]);
    if let Some(s) = &floor.surface {
        if floor.height < o.pos[1] + 5.0 || s.normal[1] > js_cos(78.0 * std::f64::consts::PI / 180.0) {
            o.pos[0] = next[0];
            o.pos[2] = next[2];
        }
    }
    let ground = world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
    o.vel[1] = (-78f64).max(o.vel[1] - 8.0);
    o.pos[1] += o.vel[1];
    o.grounded = ground.surface.is_some() && o.pos[1] < ground.height;
    if o.grounded {
        o.pos[1] = ground.height;
        o.vel[1] *= -0.05;
    }
}

fn path_at(m: &MarioRuntime, i: usize) -> [f64; 4] { m.objects.penguin_race.as_ref().unwrap().path[i] }

pub fn race_penguin(id: ObjId, m: &mut MarioRuntime) {
    let Some(race) = m.objects.penguin_race.as_ref() else { return };
    let (path_len, finish, shortcut) = (race.path.len(), race.finish, race.shortcut);
    let t = m.objects[id].action_timer;
    m.objects[id].action_timer += 1.0;
    let opos = m.objects[id].pos;
    let mpos = m.pos;
    let near = js_hypot(mpos[0] - opos[0], mpos[2] - opos[2]) < 400.0
        && opos[1] < mpos[1] + 160.0 && mpos[1] < opos[1] + 400.0 && m.action & (ACT_FLAG_AIR | ACT_FLAG_INTANGIBLE) == 0;
    match m.objects[id].action {
        0 => {
            let o = &m.objects[id];
            if t > o.var("cooldown") && near && o.pos[1] <= mpos[1] + 1.0 {
                m.objects.dialogue("Race me down the slide? A to race, B to decline.");
                m.objects[id].set_action(1);
            }
        }
        1 => {
            let o = &mut m.objects[id];
            let answer = o.var("answer");
            if answer == 1.0 {
                o.vel[1] = 60.0;
                o.set_action(2);
            } else if answer == -1.0 {
                o.set_var("cooldown", 60.0);
                o.set_action(0);
            }
        }
        2 => {
            turn(&mut m.objects[id], 0x4000 as f64, 2500.0);
            if t == 50.0 { m.objects.racer_sound(opos, "gun"); }
            if t > 50.0 {
                let o = &mut m.objects[id];
                o.forward_vel = 20.0;
                o.set_action(3);
            }
        }
        3 => {
            let wp = m.objects[id].var("waypoint") as usize;
            let [pid, x, y, z] = path_at(m, wp);
            let [_, nx, ny, nz] = path_at(m, wp + 1);
            let dx = nx - opos[0];
            let dy = ny - opos[1];
            let dz = nz - opos[2];
            let mut reached = false;
            if (nx - x) * dx + (ny - y) * dy + (nz - z) * dz <= 0.0 {
                let w = m.objects[id].var("waypoint") + 1.0;
                m.objects[id].set_var("waypoint", w);
                reached = w == (path_len - 1) as f64;
            }
            if reached {
                m.objects[id].set_action(4);
            } else {
                let pos = m.objects[id].pos;
                m.objects.racer_sound(pos, "slide");
                let lead = pos[1] - mpos[1];
                let slow = lead < 100.0 || pid >= 35.0;
                let o = &mut m.objects[id];
                let ws = approach_f32(o.var("weightedSpeed"), if slow { -500.0 } else { 1000.0 }, 30.0, 100.0);
                o.set_var("weightedSpeed", ws);
                o.forward_vel = approach_f32(o.forward_vel, clamp((ws + lead) * 0.1, if pid >= 35.0 { 60.0 } else { 70.0 }, 150.0), 0.4, 0.4);
                let speed = (15.0 * o.forward_vel).trunc();
                turn(o, atan2s(dz, dx) as f64, speed);
                o.set_var("animation", 1.0);
                if dist_3d(&mpos, &finish) < 1000.0 && mpos[2] < finish[2] { o.set_var("won", 1.0); }
            }
            let o = &mut m.objects[id];
            let air = if m.state.action & ACT_FLAG_AIR != 0 { o.var("airFrames") + 1.0 } else { 0.0 };
            o.set_var("airFrames", air);
            if air > 60.0 || dist_3d(&mpos, &shortcut) < 500.0 { o.set_var("cheated", 1.0); }
        }
        4 => {
            let o = &mut m.objects[id];
            if o.forward_vel != 0.0 && t > 5.0 && o.hit_wall {
                let pos = o.pos;
                o.forward_vel = 0.0;
                o.set_var("animation", 2.0);
                o.set_var("stopped", t);
                m.objects.racer_sound(pos, "impact");
            }
            let o = &mut m.objects[id];
            // `t - o.vars.stopped` is NaN in TS when `stopped` was never set.
            let since = if o.has_var("stopped") { t - o.var("stopped") } else { f64::NAN };
            if o.forward_vel == 0.0 && since >= 25.0 { o.set_action(5); }
        }
        5 => {
            if turn(&mut m.objects[id], 0.0, 200.0) {
                let o = &mut m.objects[id];
                o.forward_vel = 0.0;
                o.set_var("animation", 3.0);
                if near {
                    let (won, cheated, pos) = (o.var("won") != 0.0, o.var("cheated") != 0.0, o.pos);
                    if won && !cheated {
                        m.objects.dialogue("You broke my record! I award you the gold medal!");
                        m.objects.penguin_reward([pos[0], pos[1] + 200.0, pos[2]]);
                    } else {
                        m.objects.dialogue(if won { "No shortcuts in this race!" } else { "You lost! Try again next time." });
                    }
                    m.objects[id].set_action(6);
                }
            } else {
                let o = &mut m.objects[id];
                o.forward_vel = 4.0;
                o.set_var("animation", 0.0);
            }
        }
        _ => {}
    }
    do_move(id, m);
    let o = &m.objects[id];
    let big = o.scale == 8.0;
    let radius = if big { 350.0 } else { 200.0 };
    let dx = m.pos[0] - o.pos[0];
    let dz = m.pos[2] - o.pos[2];
    let d = js_hypot(dx, dz);
    if d < radius && (m.pos[1] - o.pos[1]).abs() < if big { 250.0 } else { 200.0 } {
        m.pos[0] += dx * (radius - d) / radius;
        m.pos[2] += dz * (radius - d) / radius;
    }
}
