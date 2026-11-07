//! Port of `src/sim/behaviors/waterBombCannon.ts` (water_bomb_cannon.inc.c and
//! water_bomb_act_shot_from_cannon in water_bomb.inc.c).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub fn init_water_bomb_cannon(o: &mut SimObject) {
    o.visible = false;
    o.set_var("fireTimer", 0.0);
    o.set_var("aimDelay", 0.0);
    o.set_var("targetYaw", 0.0);
}

pub fn init_cannon_barrel(o: &mut SimObject) {
    o.set_var("recoil", 0.0);
}

pub fn init_water_bomb(o: &mut SimObject) {
    o.forward_vel = -100.0;
    o.set_var("height", 1.7);
}

fn move_along_barrel(o: &mut SimObject, pitch: f64) {
    let horizontal = o.forward_vel * coss(pitch);
    o.pos[0] += horizontal * sins(o.move_yaw);
    o.pos[1] -= o.forward_vel * sins(pitch);
    o.pos[2] += horizontal * coss(o.move_yaw);
}

pub fn water_bomb_cannon_behavior(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let opos = m.objects[id].pos;
    let dx = mpos[0] - opos[0];
    let dz = mpos[2] - opos[2];
    let dist = js_hypot(dx, dz);
    if (mpos[1] - opos[1]).abs() < 300.0 && dist < 220.0 {
        m.pos[0] += (220.0 - dist) / 220.0 * dx;
        m.pos[2] += (220.0 - dist) / 220.0 * dz;
    }

    if m.objects[id].action == 0 {
        if m.objects[id].distance_to_mario >= 2000.0 { return; }
        let (link, move_yaw) = { let o = &m.objects[id]; (o.link, o.move_yaw) };
        let barrel = match link {
            Some(b) => b,
            None => {
                let mut spec = ObjectSpec::new(ObjectBehaviorKind::CannonBarrel, opos, move_yaw);
                spec.model = Some("actors/cannon_barrel/cannon_barrel_geo.glb".to_string());
                m.objects.spawn(&spec)
            }
        };
        m.objects[id].link = Some(barrel);
        let b = &mut m.objects[barrel];
        b.link = Some(id);
        b.active = true;
        b.visible = true;
        b.forward_vel = 0.0;
        b.set_var("recoil", 0.0);
        let o = &mut m.objects[id];
        o.visible = true;
        o.set_var("pitch", 0x1c00 as f64);
        o.set_var("targetPitch", 0x1c00 as f64);
        o.set_action(1);
        return;
    }
    if m.objects[id].distance_to_mario > 2500.0 {
        let link = m.objects[id].link;
        m.objects[id].visible = false;
        if let Some(l) = link { m.objects[l].deactivate(-1.0); }
        m.objects[id].set_action(0);
        return;
    }
    // The act-1 cannon at the start has BPARAM2(1) and never fires.
    let o = &mut m.objects[id];
    if o.param != 0.0 { return; }
    if o.var("fireTimer") > 0.0 {
        let t = o.var("fireTimer") - 1.0;
        o.set_var("fireTimer", t);
        return;
    }
    let pitch = approach_s32(o.var("pitch"), o.var("targetPitch"), 0x80 as f64, 0x80 as f64);
    o.set_var("pitch", pitch);
    let delta = to_s16(o.var("targetYaw") - o.face_yaw) as f64;
    o.face_yaw = to_s16(o.var("targetYaw") - approach_s32(delta, 0.0, 0x100 as f64, 0x100 as f64)) as f64;
    if o.face_yaw != o.var("targetYaw") { return; }
    if o.var("aimDelay") > 0.0 {
        let t = o.var("aimDelay") - 1.0;
        o.set_var("aimDelay", t);
        return;
    }
    o.set_var("fireTimer", 70.0);
    o.set_var("aimDelay", 60.0);
    o.set_var("targetPitch", 0x1000 as f64 + 0x400 as f64 * (random() * 4.0).floor());
    o.set_var("targetYaw", to_s16(o.move_yaw - 0x2000 as f64 + 0x1000 as f64 * (random() * 5.0).floor()) as f64);
    let pos = o.pos;
    m.objects.water_cannon_fired(pos);
}

pub fn cannon_barrel_behavior(id: ObjId, m: &mut MarioRuntime) {
    let Some(base_id) = m.objects[id].link else { return };
    let (base_face, base_pitch, base_pos, base_fire, base_link0) = {
        let base = &m.objects[base_id];
        (base.face_yaw, base.var("pitch"), base.pos, base.var("fireTimer"), base.links.as_ref().and_then(|l| l.first().copied()))
    };
    let o = &mut m.objects[id];
    o.move_yaw = base_face;
    o.face_yaw = base_face;
    o.set_var("pitch", base_pitch);
    let recoil = o.var("recoil") + o.forward_vel;
    o.set_var("recoil", recoil);
    if recoil > 0.0 {
        move_along_barrel(o, base_pitch + 0x4000 as f64);
        o.forward_vel = approach_s32(o.forward_vel, -5.0, 18.0, 18.0);
        return;
    }
    o.set_var("recoil", 0.0);
    o.pos = base_pos;
    if base_fire == 0.0 { o.forward_vel = 0.0; return; }
    if o.forward_vel != 0.0 { return; }
    o.forward_vel = 35.0;
    // Its 101-frame lifetime is shorter than the minimum interval between shots.
    let (opos, oface, opitch) = (o.pos, o.face_yaw, o.var("pitch"));
    let bomb = match base_link0 {
        Some(b) => b,
        None => {
            let mut spec = ObjectSpec::new(ObjectBehaviorKind::WaterBomb, opos, oface);
            spec.model = Some("actors/water_bubble/water_bomb_geo.glb".to_string());
            m.objects.spawn(&spec)
        }
    };
    m.objects[base_id].links = Some(vec![bomb]);
    let b = &mut m.objects[bomb];
    b.pos = opos;
    b.move_yaw = oface;
    b.face_yaw = oface;
    b.set_var("pitch", opitch + 0x4000 as f64);
    b.set_var("height", 1.7);
    b.active = true;
    b.visible = true;
    b.action_timer = 0.0;
}

pub fn water_bomb_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].action != 0 { falling_bomb(id, m); return; }
    let o = &mut m.objects[id];
    let t = o.action_timer;
    o.action_timer += 1.0;
    if t > 100.0 { o.deactivate(-1.0); return; }
    if o.var("height") > 1.2 {
        let h = o.var("height") - 0.1;
        o.set_var("height", h);
    }
    o.set_var("width", 2.0 - o.var("height"));
    let pitch = o.var("pitch");
    move_along_barrel(o, pitch);
}

pub fn water_bomb_spawner(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].var("onFloor") == 0.0 {
        let home = m.objects[id].home;
        let floor = m.world.find_floor(home[0], home[1], home[2]);
        let o = &mut m.objects[id];
        if floor.surface.is_some() { o.pos[1] = floor.height; }
        o.set_var("onFloor", 1.0);
    }
    let (mpos, mfwd, mface) = (m.pos, m.forward_vel, m.face_angle[1]);
    let (opos, oparam, olink) = { let o = &m.objects[id]; (o.pos, o.param, o.link) };
    let link_active = olink.map(|l| m.objects[l].active).unwrap_or(false);
    if link_active || js_hypot(mpos[0] - opos[0], mpos[2] - opos[2]) >= 200.0 + 50.0 * oparam ||
        mpos[1] - opos[1] >= 1000.0 { return; }
    if m.objects[id].var("delay") > 0.0 {
        let d = m.objects[id].var("delay") - 1.0;
        m.objects[id].set_var("delay", d);
        return;
    }

    let ahead = 100.0 + 28.0 * mfwd;
    let bomb = match olink {
        Some(b) => b,
        None => {
            let mut spec = ObjectSpec::new(ObjectBehaviorKind::WaterBomb, opos, 0.0);
            spec.model = Some("actors/water_bubble/water_bomb_geo.glb".to_string());
            m.objects.spawn(&spec)
        }
    };
    m.objects[id].link = Some(bomb);
    let b = &mut m.objects[bomb];
    b.reactivate();
    b.pos = [mpos[0] + ahead * sins(mface), opos[1] + 2000.0, mpos[2] + ahead * coss(mface)];
    b.set_action(1);
    b.hitbox.interact_type = 0;
    b.graph_y_offset = 0.0;
    b.set_var("height", 1.0);
    b.set_var("width", 1.0);
    b.set_var("stretch", 0.0);
    b.set_var("bounces", 0.0);
    let (bpos, blink) = (b.pos, b.link);
    let shadow = match blink {
        Some(s) => s,
        None => {
            let mut spec = ObjectSpec::new(ObjectBehaviorKind::WaterBombShadow, bpos, 0.0);
            spec.model = Some("actors/water_bubble/water_bomb_shadow_geo.glb".to_string());
            m.objects.spawn(&spec)
        }
    };
    m.objects[shadow].link = Some(bomb);
    m.objects[bomb].link = Some(shadow);
    let s = &mut m.objects[shadow];
    s.active = true;
    s.visible = true;
    m.objects[id].set_var("delay", (random() * 50.0).floor());
}

pub fn water_bomb_shadow(id: ObjId, m: &mut MarioRuntime) {
    let bomb = m.objects[id].link;
    let bomb = match bomb {
        Some(b) if m.objects[b].active && m.objects[b].action != 3 => b,
        _ => { m.objects[id].deactivate(-1.0); return; }
    };
    let [x, y, z] = m.objects[bomb].pos;
    let width = m.objects[bomb].var("width");
    let floor = m.world.find_floor(x, y, z);
    let o = &mut m.objects[id];
    o.pos = [x, y.min(floor.height + 500.0), z];
    o.scale = width;
}

fn falling_bomb(id: ObjId, m: &mut MarioRuntime) {
    let action = m.objects[id].action;
    if action == 1 {
        let pos = m.objects[id].pos;
        m.objects.water_bomb_event(pos, "drop");
        let o = &mut m.objects[id];
        o.vel[1] = -40.0;
        o.set_action(2);
        return;
    }
    if action == 3 {
        let pos = m.objects[id].pos;
        m.objects.water_bomb_event(pos, "burst");
        m.objects[id].deactivate(-1.0);
        return;
    }
    {
        let o = &mut m.objects[id];
        o.set_hitbox(ObjectHitbox {
            interact_type: INTERACT_MR_BLIZZARD, down_offset: 25.0, damage_or_coin_value: 1.0,
            health: 99.0, num_loot_coins: 0.0, radius: 80.0, height: 50.0, hurtbox_radius: 60.0, hurtbox_height: 50.0,
        });
    }
    {
        let o = &m.objects[id];
        if o.interact_status & INT_STATUS_INTERACTED != 0 || o.var("enteredWater") != 0.0 {
            let pos = o.pos;
            m.objects.water_bomb_event(pos, "splash");
            m.objects[id].set_action(3);
            return;
        }
    }
    let angle_to_mario;
    let grounded = m.objects[id].grounded;
    if grounded {
        angle_to_mario = m.objects[id].angle_to_mario;
        if m.objects[id].var("squashing") == 0.0 {
            let pos = m.objects[id].pos;
            let o = &mut m.objects[id];
            o.set_var("squashing", 1.0);
            let b = o.var("bounces") + 1.0;
            o.set_var("bounces", b);
            m.objects.water_bomb_event(pos, if b < 3.0 { "bounce" } else { "splash" });
            let o = &mut m.objects[id];
            o.move_yaw = angle_to_mario;
            o.forward_vel = 10.0;
            o.set_var("stretchSpeed", -40.0);
        }
        let o = &mut m.objects[id];
        let ss = o.var("stretchSpeed") + (15.0 - o.var("bounces") * 2.8);
        o.set_var("stretchSpeed", ss);
        let st = o.var("stretch") + ss * 0.01;
        o.set_var("stretch", st);
        if st < -0.8 {
            o.set_action(3);
        } else if st > 0.1 {
            o.vel[1] = 1.8 * ss;
        }
    } else {
        let o = &mut m.objects[id];
        let st = approach_s32(o.var("stretch"), 0.0, 0.008, 0.008);
        o.set_var("stretch", st);
        o.set_var("squashing", 0.0);
    }
    let o = &mut m.objects[id];
    let stretch = o.var("stretch");
    o.set_var("height", 1.0 + stretch);
    o.set_var("width", 1.0 - stretch * if o.var("bounces") == 3.0 { 4.0 } else { 1.0 });
    o.graph_y_offset = 40.0 * o.var("height");

    // cur_obj_move_standard with drag strength 10, gravity -4 and zero restitution.
    let drag = |v: f64| v * (1.0 - v.abs() * 0.001);
    let vx = drag(o.forward_vel * sins(o.move_yaw));
    let vz = drag(o.forward_vel * coss(o.move_yaw));
    let mut next = [o.pos[0] + vx, o.pos[1], o.pos[2] + vz];
    m.world.resolve_and_return_wall_collisions(&mut next, 25.0, 120.0);
    let floor = m.world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
    let intended = m.world.find_floor(next[0], next[1], next[2]);
    if let Some(s) = &intended.surface {
        if intended.height - floor.height < 5.0 || s.normal[1] > (78.0 * std::f64::consts::PI / 180.0).cos()
            || o.pos[1] > intended.height {
            o.pos[0] = next[0];
            o.pos[2] = next[2];
        }
    }
    o.forward_vel = js_hypot(vx, vz);
    o.vel[1] = (-78.0f64).max(o.vel[1] - 4.0);
    o.pos[1] += o.vel[1];
    o.set_var("enteredWater", if o.pos[1] <= m.world.water_level(o.pos[0], o.pos[2]) { 1.0 } else { 0.0 });
    o.grounded = floor.surface.is_some() && o.pos[1] < floor.height;
    if o.grounded {
        o.pos[1] = floor.height;
        o.vel[1] = 0.0;
    }
}
