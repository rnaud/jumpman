//! `cur_obj_move_standard` and its helpers (src/game/object_helpers.c): the floor / wall /
//! water movement most SM64 actors use. Move flags live in `vars["mf"]`; the physics
//! parameters set by `SET_OBJ_PHYSICS` live in `vars` under `wall`, `grav`, `bounce`, `drag`,
//! `fric`, `buoy` (already divided by 100 like the macro does).
use crate::collision::{CollisionWorld, WallData, FLOOR_LOWER_LIMIT};
use crate::math::*;
use crate::object::SimObject;
use crate::surfaces::*;

pub const MOVE_LANDED: u32 = 1 << 0;
pub const MOVE_ON_GROUND: u32 = 1 << 1;
pub const MOVE_LEFT_GROUND: u32 = 1 << 2;
pub const MOVE_ENTERED_WATER: u32 = 1 << 3;
pub const MOVE_AT_WATER_SURFACE: u32 = 1 << 4;
pub const MOVE_UNDERWATER_OFF_GROUND: u32 = 1 << 5;
pub const MOVE_UNDERWATER_ON_GROUND: u32 = 1 << 6;
pub const MOVE_IN_AIR: u32 = 1 << 7;
pub const MOVE_HIT_WALL: u32 = 1 << 9;
pub const MOVE_HIT_EDGE: u32 = 1 << 10;
pub const MOVE_ABOVE_LAVA: u32 = 1 << 11;
pub const MOVE_LEAVING_WATER: u32 = 1 << 12;
pub const MOVE_BOUNCE: u32 = 1 << 13;
pub const MOVE_ABOVE_DEATH_BARRIER: u32 = 1 << 14;
pub const MASK_ON_GROUND: u32 = MOVE_LANDED | MOVE_ON_GROUND;
pub const MASK_IN_WATER: u32 = MOVE_ENTERED_WATER | MOVE_AT_WATER_SURFACE | MOVE_UNDERWATER_OFF_GROUND | MOVE_UNDERWATER_ON_GROUND;

const FLOOR_LOWER_LIMIT_MISC: f64 = FLOOR_LOWER_LIMIT + 1000.0;

pub fn flags(o: &SimObject) -> u32 {
    o.var("mf") as u32
}
pub fn set_flags(o: &mut SimObject, f: u32) {
    o.set_var("mf", f as f64);
}
pub fn floor_height(o: &SimObject) -> f64 {
    o.var("floorH")
}

/// `SET_OBJ_PHYSICS(wall, gravity, bounciness, drag, friction, buoyancy, 0, 0)`.
pub fn set_physics(o: &mut SimObject, wall: f64, gravity: f64, bounciness: f64, drag: f64, friction: f64, buoyancy: f64) {
    o.set_var("wall", wall);
    o.set_var("grav", gravity / 100.0);
    o.set_var("bounce", bounciness / 100.0);
    o.set_var("drag", drag / 100.0);
    o.set_var("fric", friction / 100.0);
    o.set_var("buoy", buoyancy / 100.0);
}

fn abs_angle_diff(x0: f64, x1: f64) -> i32 {
    let mut diff = to_s16(x1 - x0);
    if diff == -0x8000 {
        diff = -0x7FFF;
    }
    diff.abs()
}

/// `cur_obj_update_floor_height`.
pub fn update_floor_height(o: &mut SimObject, world: &CollisionWorld) {
    let h = world.find_floor(o.pos[0], o.pos[1], o.pos[2]).height;
    o.set_var("floorH", h);
}

fn update_floor(o: &mut SimObject, world: &CollisionWorld) {
    let hit = world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
    o.set_var("floorH", hit.height);
    let mut f = flags(o);
    match hit.surface {
        Some(s) => {
            if s.kind == SURFACE_BURNING {
                f |= MOVE_ABOVE_LAVA;
            } else if s.kind == SURFACE_DEATH_PLANE {
                f |= MOVE_ABOVE_DEATH_BARRIER;
            }
            o.set_var("floorType", s.kind as f64);
        }
        None => o.set_var("floorType", 0.0),
    }
    set_flags(o, f);
}

fn detect_steep_floor(o: &mut SimObject, world: &CollisionWorld, steep_deg: f64) -> i32 {
    let steep_normal_y = coss(to_s16(steep_deg * (0x10000 as f64 / 360.0)) as f64);
    if o.forward_vel != 0.0 {
        let ix = o.pos[0] + o.vel[0];
        let iz = o.pos[2] + o.vel[2];
        let hit = world.find_floor(ix, o.pos[1], iz);
        let delta = hit.height - o.var("floorH");
        if hit.height < FLOOR_LOWER_LIMIT_MISC {
            o.set_var("wallAngle", to_s16(o.move_yaw + 0x8000 as f64) as f64);
            return 2;
        }
        if let Some(s) = &hit.surface {
            if s.normal[1] < steep_normal_y && delta > 0.0 && hit.height > o.pos[1] {
                o.set_var("wallAngle", atan2s(s.normal[2], s.normal[0]) as f64);
                return 1;
            }
        }
    }
    0
}

/// `cur_obj_resolve_wall_collisions`: true when the wall faces against the movement.
pub fn resolve_wall_collisions(o: &mut SimObject, world: &CollisionWorld) -> bool {
    let radius = o.var("wall");
    if radius > 0.1 {
        let mut d = WallData { x: o.pos[0].trunc(), y: o.pos[1].trunc(), z: o.pos[2].trunc(), offset_y: 10.0, radius, walls: vec![] };
        if world.find_wall_collisions(&mut d, false) != 0 {
            o.pos = [d.x, d.y, d.z];
            if let Some(w) = d.walls.last() {
                let angle = atan2s(w.normal[2], w.normal[0]) as f64;
                o.set_var("wallAngle", angle);
                return abs_angle_diff(angle, o.move_yaw) > 0x4000;
            }
        }
    }
    false
}

/// `cur_obj_update_floor_and_resolve_wall_collisions`.
pub fn update_floor_and_resolve_walls(o: &mut SimObject, world: &CollisionWorld, steep_deg: f64) {
    let mut f = flags(o) & !(MOVE_ABOVE_LAVA | MOVE_ABOVE_DEATH_BARRIER | MOVE_HIT_WALL);
    set_flags(o, f);
    if resolve_wall_collisions(o, world) {
        f |= MOVE_HIT_WALL;
    }
    set_flags(o, f);
    update_floor(o, world);
    f = flags(o);
    if o.pos[1] > o.var("floorH") {
        f |= MOVE_IN_AIR;
    }
    set_flags(o, f);
    if detect_steep_floor(o, world, steep_deg) != 0 {
        let f = flags(o) | MOVE_HIT_WALL;
        set_flags(o, f);
    }
}

/// `cur_obj_update_floor_and_walls`.
pub fn update_floor_and_walls(o: &mut SimObject, world: &CollisionWorld) {
    update_floor_and_resolve_walls(o, world, 60.0);
}

fn apply_drag_to_value(value: &mut f64, drag: f64) {
    if *value != 0.0 {
        let decel = *value * *value * (drag * 0.0001);
        if *value > 0.0 {
            *value -= decel;
            if *value < 0.001 {
                *value = 0.0;
            }
        } else {
            *value += decel;
            if *value > -0.001 {
                *value = 0.0;
            }
        }
    }
}

fn move_xz(o: &mut SimObject, world: &CollisionWorld, steep_normal_y: f64, care: bool) -> bool {
    let ix = o.pos[0] + o.vel[0];
    let iz = o.pos[2] + o.vel[2];
    let hit = world.find_floor(ix, o.pos[1], iz);
    let delta = hit.height - o.var("floorH");
    let mut f = flags(o) & !MOVE_HIT_EDGE;
    let ok;
    if hit.height < FLOOR_LOWER_LIMIT_MISC {
        f |= MOVE_HIT_EDGE;
        ok = false;
    } else if delta < 5.0 {
        if !care {
            o.pos[0] = ix;
            o.pos[2] = iz;
            ok = true;
        } else if delta < -50.0 && f & MOVE_ON_GROUND != 0 {
            f |= MOVE_HIT_EDGE;
            ok = false;
        } else if hit.surface.as_ref().is_some_and(|s| s.normal[1] > steep_normal_y) {
            o.pos[0] = ix;
            o.pos[2] = iz;
            ok = true;
        } else {
            f |= MOVE_HIT_EDGE;
            ok = false;
        }
    } else {
        if hit.surface.as_ref().is_some_and(|s| s.normal[1] > steep_normal_y) || o.pos[1] > hit.height {
            o.pos[0] = ix;
            o.pos[2] = iz;
        }
        ok = false;
    }
    set_flags(o, f);
    ok
}

fn move_y_and_get_water_level(o: &mut SimObject, world: &CollisionWorld, gravity: f64, buoyancy: f64) -> f64 {
    o.vel[1] += gravity + buoyancy;
    if o.vel[1] < -78.0 {
        o.vel[1] = -78.0;
    }
    o.pos[1] += o.vel[1];
    world.water_level(o.pos[0], o.pos[2])
}

fn move_update_underwater_flags(o: &mut SimObject) {
    let decel = (o.vel[1] * o.vel[1]).sqrt() * (o.var("drag") * 7.0) / 100.0;
    if o.vel[1] > 0.0 {
        o.vel[1] -= decel;
    } else {
        o.vel[1] += decel;
    }
    let mut f = flags(o);
    if o.pos[1] < o.var("floorH") {
        o.pos[1] = o.var("floorH");
        f |= MOVE_UNDERWATER_ON_GROUND;
    } else {
        f |= MOVE_UNDERWATER_OFF_GROUND;
    }
    set_flags(o, f);
}

fn move_update_ground_air_flags(o: &mut SimObject, bounciness: f64) {
    let mut f = flags(o) & !MOVE_BOUNCE;
    if o.pos[1] < o.var("floorH") {
        if f & MOVE_ON_GROUND == 0 {
            if f & MOVE_LANDED != 0 {
                f &= !MOVE_LANDED;
                f |= MOVE_ON_GROUND;
            } else {
                f |= MOVE_LANDED;
            }
        }
        o.pos[1] = o.var("floorH");
        if o.vel[1] < 0.0 {
            o.vel[1] *= bounciness;
        }
        if o.vel[1] > 5.0 {
            f |= MOVE_BOUNCE;
        }
    } else {
        f &= !MOVE_LANDED;
        if f & MOVE_ON_GROUND != 0 {
            f &= !MOVE_ON_GROUND;
            f |= MOVE_LEFT_GROUND;
        }
    }
    f &= !MASK_IN_WATER;
    set_flags(o, f);
}

/// `cur_obj_move_y`.
pub fn move_y(o: &mut SimObject, world: &CollisionWorld, gravity: f64, bounciness: f64, buoyancy: f64) {
    let mut f = flags(o) & !MOVE_LEFT_GROUND;
    if f & MOVE_AT_WATER_SURFACE != 0 && o.vel[1] > 5.0 {
        f &= !MASK_IN_WATER;
        f |= MOVE_LEAVING_WATER;
    }
    set_flags(o, f);
    if f & MASK_IN_WATER == 0 {
        let water = move_y_and_get_water_level(o, world, gravity, 0.0);
        f = flags(o);
        if o.pos[1] > water {
            set_flags(o, f);
            move_update_ground_air_flags(o, bounciness);
        } else {
            f |= MOVE_ENTERED_WATER;
            f &= !MASK_ON_GROUND;
            set_flags(o, f);
        }
    } else {
        f &= !MOVE_ENTERED_WATER;
        set_flags(o, f);
        let water = move_y_and_get_water_level(o, world, gravity, buoyancy);
        if o.pos[1] < water {
            move_update_underwater_flags(o);
        } else {
            let mut f = flags(o);
            if o.pos[1] < o.var("floorH") {
                o.pos[1] = o.var("floorH");
                f &= !MASK_IN_WATER;
            } else {
                o.pos[1] = water;
                o.vel[1] = 0.0;
                f &= !(MOVE_UNDERWATER_OFF_GROUND | MOVE_UNDERWATER_ON_GROUND);
                f |= MOVE_AT_WATER_SURFACE;
            }
            set_flags(o, f);
        }
    }
    let mut f = flags(o);
    if f & (MASK_ON_GROUND | MOVE_AT_WATER_SURFACE | MOVE_UNDERWATER_OFF_GROUND) != 0 {
        f &= !MOVE_IN_AIR;
    } else {
        f |= MOVE_IN_AIR;
    }
    set_flags(o, f);
}

/// `cur_obj_move_standard`.
pub fn move_standard(o: &mut SimObject, world: &CollisionWorld, steep_slope_deg: f64) {
    let gravity = o.var("grav");
    let bounciness = o.var("bounce");
    let buoyancy = o.var("buoy");
    let drag = o.var("drag");
    let mut care = false;
    let mut deg = steep_slope_deg;
    if deg < 0.0 {
        care = true;
        deg = -deg;
    }
    let steep_normal_y = coss(deg * (0x10000 as f64 / 360.0));
    super::obj::cur_obj_compute_vel_xz(o);
    apply_drag_to_value(&mut o.vel[0], drag);
    apply_drag_to_value(&mut o.vel[2], drag);
    move_xz(o, world, steep_normal_y, care);
    move_y(o, world, gravity, bounciness, buoyancy);
    let negative = o.forward_vel < 0.0;
    o.forward_vel = (o.vel[0] * o.vel[0] + o.vel[2] * o.vel[2]).sqrt();
    if negative {
        o.forward_vel = -o.forward_vel;
    }
}
