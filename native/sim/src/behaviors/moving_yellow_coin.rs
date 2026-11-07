//! Port of `src/sim/behaviors/movingYellowCoin.ts`.
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::object_manager::ObjectManager;
use crate::surfaces::{SURFACE_BURNING, SURFACE_DEATH_PLANE};

/// `spawnMovingYellowCoins(source, ctx, count)`; `pos` is `source.pos`.
pub fn spawn_moving_yellow_coins(objects: &mut ObjectManager, pos: Vec3, count: i32) {
    for _ in 0..count {
        let mut spec = ObjectSpec::new(ObjectBehaviorKind::MovingYellowCoin, pos, 0.0);
        spec.model = Some("actors/coin/yellow_coin_geo.glb".to_string());
        objects.spawn(&spec);
    }
}

// obj_spawn_yellow_coins and bhv_moving_yellow_coin_loop.
pub fn init_moving_yellow_coin(o: &mut SimObject) {
    o.forward_vel = random() * 20.0;
    o.vel[1] = random() * 40.0 + 20.0;
    o.move_yaw = (random() * 65536.0).floor();
    o.gravity = 3.0;
    o.friction = 1.0;
    o.buoyancy = 1.5;
    o.billboard = true;
    o.hitbox.radius = 100.0;
    o.hitbox.height = 64.0;
}

pub fn moving_yellow_coin(id: ObjId, m: &mut MarioRuntime) {
    let flags = m.objects[id].step_physics(&m.world, None);
    if m.objects[id].grounded {
        let pos = m.objects[id].pos;
        if flags & OBJ_COL_FLAG_NO_Y_VEL == 0 { m.objects.coin_bounce(pos); }
        let kind = m.world.find_floor(pos[0], pos[1], pos[2]).surface.map(|s| s.kind);
        if kind == Some(SURFACE_BURNING) || kind == Some(SURFACE_DEATH_PLANE) {
            m.objects[id].deactivate(-1.0);
            return;
        }
    }
    let o = &mut m.objects[id];
    if o.timer >= 342.0 { o.deactivate(-1.0); return; }
    o.visible = o.timer < 302.0 || o.timer % 2.0 == 0.0;
}
