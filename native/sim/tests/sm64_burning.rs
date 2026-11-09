//! Burning actions (`ACT_BURNING_JUMP` / `_FALL` / `_GROUND`, interaction.c `interact_flame`).
mod r2a_common;
use jumpman_sim::behaviors::g2::interact::{interact_flame, INTERACT_FLAME};
use jumpman_sim::object::*;
use jumpman_sim::state::*;
use r2a_common::*;

fn rig() -> (jumpman_sim::mario_runtime::MarioRuntime, ObjId) {
    let mut m = runtime(ground(0.0, 5000.0), vec![spec(ObjectBehaviorKind::Flame, [4000.0, 0.0, 4000.0], 0.0, 0)]);
    let id = ObjId(0);
    m.objects[id].hitbox.interact_type = INTERACT_FLAME;
    stand_at(&mut m, 0.0, 0.0);
    m.invinc_timer = 0.0;
    (m, id)
}

#[test]
fn flame_sets_mario_on_fire_and_the_run_burns_out() {
    let (mut m, id) = rig();
    assert!(interact_flame(id, &mut m));
    assert_eq!(m.action, ACT_BURNING_JUMP);
    assert_eq!(m.vel[1], 31.5);
    assert_eq!(m.forward_vel, 8.0);
    assert_eq!(m.burn_timer, 0.0);
    let h0 = m.health;
    let mut ground_frames = 0;
    let mut saw_ground = false;
    for _ in 0..200 {
        tick(&mut m);
        if m.action == ACT_BURNING_GROUND {
            saw_ground = true;
            ground_frames += 1;
            assert!(m.forward_vel >= 8.0 && m.forward_vel <= 48.0);
        } else if saw_ground {
            break;
        }
    }
    assert!(saw_ground, "landed into the burning run");
    assert!(m.action != ACT_BURNING_GROUND && m.action & ACT_FLAG_MOVING != 0, "the burn timer (> 160) ends the run into walking: {:#x}", m.action);
    // 2 per frame on the ground after the jump's 3 per frame.
    assert!((40..=81).contains(&ground_frames), "{ground_frames} burning frames");
    assert!(m.health < h0 - 10.0 * 40.0, "10 health per burning frame ({} -> {})", h0, m.health);
    assert!(m.health >= 0x100 as f64, "burning in the air never kills");
}

#[test]
fn falling_mario_burns_with_the_fall_action() {
    let (mut m, id) = rig();
    m.state.pos[1] = 800.0;
    m.state.vel[1] = -10.0;
    m.set_action(ACT_FREEFALL, 0.0);
    interact_flame(id, &mut m);
    assert_eq!(m.action, ACT_BURNING_FALL);
    for _ in 0..80 {
        tick(&mut m);
        if m.action == ACT_BURNING_GROUND {
            return;
        }
    }
    panic!("never landed: {:#x}", m.action);
}

#[test]
fn metal_cap_and_invulnerable_ignore_fire() {
    let (mut m, id) = rig();
    m.state.flags |= MARIO_METAL_CAP;
    interact_flame(id, &mut m);
    assert_eq!(m.action, ACT_IDLE);
    let (mut m, id) = rig();
    m.invinc_timer = 20.0;
    interact_flame(id, &mut m);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn burning_ground_health_runs_out_into_standing_death() {
    let (mut m, id) = rig();
    interact_flame(id, &mut m);
    m.state.health = 0x100 as f64 + 25.0;
    m.set_action(ACT_BURNING_GROUND, 0.0);
    for _ in 0..4 {
        tick(&mut m);
    }
    assert_eq!(m.action, ACT_STANDING_DEATH);
}
