//! Headless test hooks for object features (`JUMPMAN_TELEPORT`, `JUMPMAN_TAP`).
//!
//! * `JUMPMAN_STARS=n` (progress/save.rs) fakes the star count. `JUMPMAN_TELEPORT=x,y,z[,yaw_degrees]` places Mario (SM64 units) at startup.
//! * `JUMPMAN_TAP=a@3.0,b@5.5` latches an A / B press once the app clock passes the time.
use crate::camera::CameraRig;
use crate::game::runner::GameSim;
use crate::input::{self, PlayerInput};
use bevy::prelude::*;

pub struct DebugHooksPlugin;

impl Plugin for DebugHooksPlugin {
    fn build(&self, app: &mut App) {
        if let Some(taps) = std::env::var("JUMPMAN_TAP").ok().map(|v| parse_taps(&v)) {
            app.insert_resource(Taps(taps)).add_systems(PreUpdate, tap.after(input::read_input));
        }
        if let Ok(v) = std::env::var("JUMPMAN_SIDECAM") {
            let n: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if n.len() == 3 {
                app.insert_resource(SideCam([n[0], n[1], n[2]])).add_systems(Update, side_cam.after(crate::CameraSet));
            }
        }
        if let Ok(v) = std::env::var("JUMPMAN_COLLECT_DEMO") {
            app.insert_resource(CollectDemo(v.split(',').filter_map(|p| p.trim().parse().ok()).collect())).add_systems(Update, collect_demo.before(crate::CameraSet));
        }
        if let Ok(v) = std::env::var("JUMPMAN_TELEPORT") {
            app.insert_resource(Teleport(v)).add_systems(Update, teleport.before(crate::CameraSet));
        }
    }
}

#[derive(Resource)]
struct Taps(Vec<(f32, bool, bool)>);

#[derive(Resource)]
struct Teleport(String);

/// `JUMPMAN_COLLECT_DEMO=<t_before>,<t_after>[,<dist>]`: for the collectible-theme contact sheets. At `t_before` seconds Mario is placed `dist`
/// (default 260) units in front of the nearest currency pickup of the world (an SM64 coin, a village rupee of the Zelda scene or a
/// Banjo note), facing it; at `t_after` he stands in it, so a still before and one after show the counter going up.
#[derive(Resource)]
struct CollectDemo(Vec<f64>);

#[allow(clippy::too_many_arguments)]
fn collect_demo(
    spec: Res<CollectDemo>,
    time: Res<Time>,
    mut sim: ResMut<GameSim>,
    mut rig: ResMut<CameraRig>,
    zelda: Option<Res<crate::zelda::world::ZeldaWorld>>,
    field: Option<Res<crate::bk_enemies::pickups::PickupField>>,
    mut target: Local<Option<[f64; 3]>>,
    mut stage: Local<u8>,
) {
    let now = time.elapsed_secs() as f64;
    if *stage >= 2 {
        // a line per half second after the pickup, for the sheet's captions
        if spec.0.len() >= 2 && now >= spec.0[1] + (*stage as f64 - 2.0) * 0.5 && *stage < 8 {
            *stage += 1;
            eprintln!("collect_demo t={now:.1}: coins {} wallet {} notes {} pickups_left {} theme {:?}", sim.coins, sim.mario.link.inv.rupees, sim.mario.banjo.notes, sim.pickups.iter().filter(|p| !p.collected).count(), sim.theme());
        }
        return;
    }
    if spec.0.len() < 2 {
        return;
    }
    let dist = spec.0.get(2).copied().unwrap_or(260.0);
    let m = sim.mario.pos;
    let d2 = |p: &[f64; 3]| (p[0] - m[0]).powi(2) + (p[2] - m[2]).powi(2);
    if *stage == 0 && now >= spec.0[0] {
        let mut best: Option<[f64; 3]> = None;
        let mut consider = |p: [f64; 3]| {
            if best.map_or(true, |b| d2(&p) < d2(&b)) {
                best = Some(p);
            }
        };
        for p in sim.pickups.iter().filter(|p| !p.collected && p.kind != crate::game::runner::PickupKind::Dynamic) {
            consider(p.pos);
        }
        // (Banjo's sandbox scatter in the other worlds is not the world's currency)
        if sim.course.request.id.starts_with("bk_") {
            // the notes of the map's setup file (the live list only holds those near Banjo)
            for it in field.iter().flat_map(|f| f.items.iter()).filter(|i| i.kind == jumpman_sim::banjo_ext::PickupKind::Note) {
                consider(it.pos);
            }
            for p in sim.mario.banjo.pickups.iter().filter(|p| !p.collected && p.kind == jumpman_sim::banjo_ext::PickupKind::Note) {
                consider(p.pos);
            }
        }
        if let Some(z) = &zelda {
            for p in z.scene.kokiri.rupees.iter().filter(|p| !z.taken.contains(&p.key)) {
                consider(p.pos);
            }
        }
        let Some(t) = best else {
            if (now * 2.0) as i64 % 4 == 0 {
                eprintln!("collect_demo: no target yet (t={now:.1}, banjo pickups {}, course {})", sim.mario.banjo.pickups.len(), sim.course.request.id);
            }
            return;
        };
        *target = Some(t);
        let (dx, dz) = (t[0] - m[0], t[2] - m[2]);
        let len = dx.hypot(dz).max(1.0);
        sim.mario.pos = [t[0] - dx / len * dist, t[1], t[2] - dz / len * dist];
        sim.mario.vel = [0.0; 3];
        sim.mario.link.phys.fall_start = t[1];
        sim.mario.face_angle[1] = jumpman_sim::math::deg_to_angle(dx.atan2(dz).to_degrees()) as f64;
        // Link's wallet starts full in this build: empty it so the sheet can show it going up
        if sim.mario.link.enabled {
            sim.mario.link.inv.rupees = 20;
        }
        sim.curr = sim.snapshot();
        sim.prev = sim.curr.clone();
        rig.snap_behind(sim.mario.face_angle[1]);
        *stage = 1;
    } else if *stage == 1 && now >= spec.0[1] {
        if let Some(t) = *target {
            sim.mario.pos = t;
            sim.mario.vel = [0.0; 3];
        }
        *stage = 2;
    }
}

/// `JUMPMAN_SIDECAM=dist,height,yaw_degrees`: the main camera at a fixed bearing around Mario (yaw from +z towards +x),
/// looking at his chest. The contact sheets of the Link parity pass use it for side views of jumps, rolls and slides.
#[derive(Resource)]
struct SideCam([f32; 3]);

fn side_cam(spec: Res<SideCam>, sim: Res<GameSim>, mut cams: Query<&mut Transform, With<crate::camera::MainCamera>>) {
    let [dist, height, yaw] = spec.0;
    let target = crate::render::to_bevy([sim.mario.pos[0], sim.mario.pos[1] + 90.0, sim.mario.pos[2]]);
    let a = yaw.to_radians();
    let at = target + Vec3::new(a.sin() * dist, height, a.cos() * dist) * crate::render::SCALE;
    for mut t in &mut cams {
        *t = Transform::from_translation(at).looking_at(target, Vec3::Y);
    }
}

pub fn parse_taps(s: &str) -> Vec<(f32, bool, bool)> {
    s.split(',')
        .filter_map(|t| {
            let (button, time) = t.split_once('@')?;
            Some((time.trim().parse().ok()?, button.trim().eq_ignore_ascii_case("a"), button.trim().eq_ignore_ascii_case("b")))
        })
        .collect()
}

fn tap(mut taps: ResMut<Taps>, mut input: ResMut<PlayerInput>, time: Res<Time>) {
    let now = time.elapsed_secs();
    taps.0.retain(|&(at, a, b)| {
        if now >= at {
            input.tap(a, b);
            false
        } else {
            true
        }
    });
}

/// Runs once the requested course (`JUMPMAN_COURSE`, else the first one) is the loaded one: at `PostStartup` the shell
/// had not switched to it yet and the position was thrown away with the old sim.
fn teleport(spec: Res<Teleport>, mut sim: ResMut<GameSim>, mut rig: ResMut<CameraRig>, mut done: Local<bool>, mut frames: Local<u32>) {
    if *done {
        return;
    }
    *frames += 1;
    let want = std::env::var("JUMPMAN_COURSE").ok();
    let ready = match &want {
        Some(id) => sim.course.request.id == *id && *frames > 3,
        None => *frames > 3,
    };
    if !ready {
        return;
    }
    *done = true;
    let v: Vec<f64> = spec.0.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if v.len() < 3 {
        warn!("JUMPMAN_TELEPORT needs x,y,z");
        return;
    }
    sim.mario.pos = [v[0], v[1], v[2]];
    // a teleport starts a new fall (OoT resets `fallStartHeight` on warps)
    sim.mario.link.phys.fall_start = v[1];
    if let Some(yaw) = v.get(3) {
        sim.mario.face_angle[1] = jumpman_sim::math::deg_to_angle(*yaw) as f64;
    }
    // A spawn animation would swallow the move: drop into the free fall instead.
    {
        use jumpman_sim::state::*;
        if sim.mario.action & ACT_GROUP_MASK == ACT_GROUP_CUTSCENE {
            sim.mario.vel = [0.0; 3];
            sim.mario.set_action(ACT_FREEFALL, 0.0);
        }
    }
    sim.curr = sim.snapshot();
    sim.prev = sim.curr.clone();
    rig.snap_behind(sim.mario.face_angle[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tap_script() {
        assert_eq!(parse_taps("a@3.0, b@5.5,x"), vec![(3.0, true, false), (5.5, false, true)]);
    }
}
