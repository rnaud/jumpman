//! Throwaway diagnostics (ignored): `cargo test --release --lib bk_debug -- --ignored --nocapture`
use super::runner::GameSim;
use super::world::CourseRequest;
use jumpman_sim::state::*;
use std::path::PathBuf;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

#[test]
#[ignore]
fn bk_debug_spawn() {
    for id in std::env::var("IDS").unwrap_or_default().split(',') {
        let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
        println!("{id}: spawn {:?} yaw {} water boxes {}", sim.course.spawn.pos, sim.course.spawn.face_yaw, sim.course.water_boxes().len());
        for t in 0..90 {
            sim.tick(&ControllerSnapshot::default());
            if t % 30 == 29 {
                println!("  t{t}: pos {:?} action {:#x} floor {:?} fh {}", sim.mario.pos, sim.mario.action, sim.mario.floor.is_some(), sim.mario.floor_height);
            }
        }
        let p = sim.course.spawn.pos;
        for r in [40.0, 100.0, 200.0] {
            let ring: Vec<String> = (0..8)
                .map(|k| {
                    let a = k as f64 * std::f64::consts::FRAC_PI_4;
                    let f = sim.mario.world.find_floor(p[0] + r * a.sin(), p[1] + 100.0, p[2] + r * a.cos());
                    if f.surface.is_some() { format!("{:.0}", f.height) } else { "-".into() }
                })
                .collect();
            println!("  ring {r}: {}", ring.join(" "));
        }
        let c = ControllerSnapshot { stick_x: 0.0, stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
        for t in 0..45 {
            sim.tick(&c);
            if t % 15 == 14 {
                println!("  walk t{t}: pos {:?} action {:#x} vel {}", sim.mario.pos, sim.mario.action, sim.mario.forward_vel);
            }
        }
    }
}
