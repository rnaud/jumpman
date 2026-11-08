//! `bhvExclamationBox` contents other than caps and stars (`src/game/behaviors/exclamation_box.inc.c`,
//! `sExclamationBoxContents`): Koopa shell (3), one / three / ten coins (4 / 5 / 6), walking 1-up (7)
//! and the 1-up that runs away (9). The cap boxes (0-2) are `wingBox` (behaviors/star_box.rs) and
//! the star boxes `starBox`. Actions follow the original: 0/2 idle (`exclamation_box_act_2`, solid and
//! breakable), 1 recoil (`act_3`: 8 frames, then `act_4` spawns the contents and deletes the box).
//! The recoil reuses the star box's animation constants so the app's collider / view code treat all
//! boxes alike (action 0 = idle, 1 = recoil).
use super::*;
use crate::behaviors::moving_yellow_coin::spawn_moving_yellow_coins;
use crate::state::*;

pub const EXCLAMATION_BOX_BP_KOOPA_SHELL: i32 = 3;
pub const EXCLAMATION_BOX_BP_ONE_COIN: i32 = 4;
pub const EXCLAMATION_BOX_BP_THREE_COINS: i32 = 5;
pub const EXCLAMATION_BOX_BP_TEN_COINS: i32 = 6;
pub const EXCLAMATION_BOX_BP_1UP_WALKING: i32 = 7;
pub const EXCLAMATION_BOX_BP_1UP_RUNNING_AWAY: i32 = 9;

/// `sExclamationBoxHitbox` at `cur_obj_scale(2.0)`.
pub fn init(o: &mut SimObject) {
    o.scale = 2.0;
    o.set_hitbox(ObjectHitbox {
        interact_type: INTERACT_BREAKABLE, down_offset: 10.0, radius: 80.0, height: 60.0,
        health: 1.0, damage_or_coin_value: 0.0, num_loot_coins: 0.0, hurtbox_radius: 80.0, hurtbox_height: 60.0,
    });
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].action == 0 {
        // cur_obj_was_attacked_or_ground_pounded
        let (mpos, maction) = (m.pos, m.action);
        let o = &mut m.objects[id];
        let pound = maction == ACT_GROUND_POUND_LAND && (mpos[1] - o.home[1] - 104.0).abs() < 5.0
            && (mpos[0] - o.home[0]).abs() < 52.0 && (mpos[2] - o.home[2]).abs() < 52.0;
        if o.interact_status & INT_STATUS_WAS_ATTACKED == 0 && !pound {
            return;
        }
        o.interact_status = 0;
        o.hitbox.interact_type = 0;
        o.vel[1] = 30.0;
        o.set_action(1);
        return;
    }
    // exclamation_box_act_3: the squash-and-stretch recoil.
    let o = &mut m.objects[id];
    let t = o.action_timer;
    o.action_timer += 1.0;
    o.vel[1] = (0.0f64).max(o.vel[1] - 8.0);
    o.pos[1] += o.vel[1];
    let wave = sins(0x4000 as f64 + t * 0x1000 as f64);
    o.set_var("width", (1.0 - wave) * 0.5 + 1.0);
    o.set_var("height", (1.0 + wave) * 0.3);
    o.graph_y_offset = (1.0 - wave) * 26.0;
    if t == 8.0 {
        m.objects.break_object(id);
        spawn_contents(id, m);
        m.objects[id].deactivate(-1.0);
    }
}

/// `exclamation_box_spawn_contents` (velocity 20 up, forward 3, Mario's yaw).
fn spawn_contents(id: ObjId, m: &mut MarioRuntime) {
    let (pos, kind) = { let o = &m.objects[id]; (o.pos, o.param as i32) };
    let yaw = m.face_angle[1];
    let launch = |m: &mut MarioRuntime, k: ObjectBehaviorKind, model: &str| {
        let mut spec = ObjectSpec::new(k, pos, yaw);
        spec.model = Some(model.to_string());
        let c = m.objects.spawn(&spec);
        let o = &mut m.objects[c];
        o.vel[1] = 20.0;
        o.forward_vel = 3.0;
        o.move_yaw = yaw;
    };
    match kind {
        EXCLAMATION_BOX_BP_KOOPA_SHELL => launch(m, K::KoopaShell, "actors/koopa_shell/koopa_shell_geo.glb"),
        // bhvSingleCoinGetsSpawned / bhvThreeCoinsSpawn / bhvTenCoinsSpawn (obj_spawn_yellow_coins).
        EXCLAMATION_BOX_BP_ONE_COIN => spawn_moving_yellow_coins(&mut m.objects, pos, 1),
        EXCLAMATION_BOX_BP_THREE_COINS => spawn_moving_yellow_coins(&mut m.objects, pos, 3),
        EXCLAMATION_BOX_BP_TEN_COINS => spawn_moving_yellow_coins(&mut m.objects, pos, 10),
        EXCLAMATION_BOX_BP_1UP_WALKING => launch(m, K::OneUpWalking, "actors/mushroom_1up/mushroom_1up_geo.glb"),
        EXCLAMATION_BOX_BP_1UP_RUNNING_AWAY => launch(m, K::OneUpRunning, "actors/mushroom_1up/mushroom_1up_geo.glb"),
        _ => {}
    }
}
