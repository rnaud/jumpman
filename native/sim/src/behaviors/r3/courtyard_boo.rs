//! Castle courtyard Boos (`bhvCourtyardBooTriplet`): port of `bhv_courtyard_boo_triplet_init` (boo.inc.c).
//!
//! The invisible spawner deletes itself unless the save has at least 12 stars (`SPAWN_CASTLE_BOO_STAR_REQUIREMENT`);
//! otherwise it releases three Boos at (0, 50, 0), (210, 110, 210) and (-210, 70, -210) from itself, each facing a random
//! yaw, and is gone. The app publishes the star count as the `stars` var (`progress::castle_npcs`); until then it waits.
//! The Boos are the sim's simplified `Boo` (the ghost-hunt Boo of `boo.inc.c` is not ported).
use crate::behaviors::g2::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

const BOO_MODEL: &str = "actors/boo/boo_geo.glb";
/// `sCourtyardBooTripletPositions`.
const POSITIONS: [[f64; 3]; 3] = [[0.0, 50.0, 0.0], [210.0, 110.0, 210.0], [-210.0, 70.0, -210.0]];
/// `SPAWN_CASTLE_BOO_STAR_REQUIREMENT`.
pub const STAR_REQUIREMENT: f64 = 12.0;

pub fn init(o: &mut SimObject) {
    o.visible = false;
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    if !m.objects[id].has_var("stars") {
        return;
    }
    let (stars, pos) = (m.objects[id].var("stars"), m.objects[id].pos);
    if stars >= STAR_REQUIREMENT {
        for p in POSITIONS {
            let b = spawn_child(m, id, K::Boo, Some(BOO_MODEL), 0.0);
            let boo = &mut m.objects[b];
            boo.pos = [pos[0] + p[0], pos[1] + p[1], pos[2] + p[2]];
            boo.home = boo.pos;
            boo.move_yaw = random_u16();
            boo.face_yaw = boo.move_yaw;
        }
    }
    m.objects[id].deactivate(-1.0);
}
