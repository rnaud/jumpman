//! App-layer differential test against `tests/rustFixtures.game.gen.test.ts`: the TypeScript drives
//! the real world factory (moving platforms, door colliders, goombas, cannons, room doors, coins,
//! signs, the cannon operator, painting floors) with `Game.stepWorld`'s order for 2400 frames per
//! course; this file replays the same script through the Rust app-layer types and compares seven
//! per-frame state hashes bit for bit.
//!
//! `GAME_ONLY=bob.2.1` restricts the run; `GAME_DUMP=<file>` (with `GAME_ONLY`) writes Rust's
//! per-frame state, to be diffed against the generator's `GAME_DETAIL`/`GAME_OUT` dump.
use super::cannons::{CannonOperator, Cannons, OperatorOutput};
use super::goomba::{interact, spawn_goombas, GoombaEvent, WalkingGoomba};
use super::longrun_tests::{mario_fields, nums, object_fields, Hasher, F};
use super::platform::Platforms;
use super::runner::{GameSim, PickupKind};
use super::signpost::{readable_sign, DialogBox, SignReading};
use super::world::CourseRequest;
use crate::objects::colliders::{build_colliders, sync as sync_colliders};
use crate::objects::stage::StageHooks;
use crate::progress::flow::Progress;
use crate::progress::paintings::painting_destination;
use crate::progress::save::SaveFile;
use bevy::math::DVec3;
use jumpman_sim::math::*;
use jumpman_sim::object::ObjectBehaviorKind as K;
use jumpman_sim::object_manager;
use jumpman_sim::state::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

type Field = F;
fn n(v: f64) -> Field {
    F::N(v)
}
fn b(v: bool) -> Field {
    F::N(v as i32 as f64)
}

fn hash_fields(list: &[Field], h: &mut Hasher) {
    for f in list {
        match f {
            F::N(v) => h.num(*v),
            F::S(s) => h.str(s),
        }
    }
}

fn nf(list: &[f64]) -> Vec<Field> {
    list.iter().map(|v| F::N(*v)).collect()
}

struct Feature {
    kind: &'static str,
    pos: [f64; 3],
    y_offset: f64,
    spread: i64,
    front_yaw: Option<f64>,
    front_dist: Option<f64>,
}

fn lcg(seed: u32) -> impl FnMut() -> u32 {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        s >> 16
    }
}

fn surface_hash(s: &jumpman_sim::collision::Surface, owner: f64) -> u32 {
    let mut h = Hasher::new();
    for v in [s.v1, s.v2, s.v3] {
        for c in v {
            h.num(c);
        }
    }
    h.num(s.kind as f64);
    h.num(s.force as f64);
    h.num(s.flags as f64);
    h.num(s.room.map_or(-1.0, |r| r as f64));
    h.num(owner);
    h.0
}

pub(super) struct WorldHasher {
    pub(super) cache: HashMap<usize, u32>,
    pub(super) keep: Vec<Arc<jumpman_sim::collision::Surface>>,
}

impl WorldHasher {
    pub(super) fn hash(&mut self, world: &jumpman_sim::collision::CollisionWorld) -> u32 {
        let mut sum = 0u32;
        for s in world.surfaces() {
            let owner = match s.object {
                Some(t) if t < 0x4000_0000 => t as f64,
                _ => -1.0,
            };
            let h = if owner == -1.0 {
                let key = Arc::as_ptr(s) as usize;
                match self.cache.get(&key) {
                    Some(h) => *h,
                    None => {
                        let h = surface_hash(s, owner);
                        self.cache.insert(key, h);
                        self.keep.push(s.clone());
                        h
                    }
                }
            } else {
                surface_hash(s, owner)
            };
            sum = sum.wrapping_add(h);
        }
        let mut out = Hasher::new();
        out.word(world.surfaces().len() as u32);
        out.word(sum);
        out.0
    }
}

struct Stats {
    sign_frames: u32,
    operator_frames: u32,
    door_frames: u32,
}

fn replay(fx: &Value, index: usize, frames: usize, window: usize) -> Result<Stats, String> {
    let id = fx["id"].as_str().unwrap().to_string();
    let act = fx["act"].as_i64().unwrap() as i32;
    let area = fx["area"].as_i64().unwrap() as i32;
    let stars = fx["stars"].as_u64().unwrap() as u32;
    let tag = format!("{id}.{act}.{area}");
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    seed_random((0x51ed270bu32).wrapping_add((index as u32).wrapping_mul(9973)));
    let mut req = CourseRequest::new(&id);
    req.act = act;
    req.area = area;
    let mut sim = GameSim::load(&assets, req).map_err(|e| format!("{tag}: load {e}"))?;
    let mut save = SaveFile::in_memory(1);
    let star_courses: Vec<(&str, i32)> = vec![
        ("bob", 1), ("bob", 2), ("bob", 3), ("bob", 4), ("bob", 5), ("bob", 6), ("wf", 1), ("wf", 2), ("wf", 3), ("wf", 4), ("wf", 5), ("wf", 6),
        ("jrb", 1), ("jrb", 2), ("jrb", 3), ("jrb", 4), ("jrb", 5), ("jrb", 6),
    ];
    for (c, a) in star_courses {
        if save.stars() >= stars {
            break;
        }
        save.collect_star(c, a, 0);
    }
    let mut i = 1;
    while save.stars() < stars {
        save.collect_star("ccm", i % 7 + 100 * (i / 7), 0);
        i += 1;
    }
    let mut progress = Progress::new(&mut sim, &assets, save);
    // The TS generator has no death-warp handler: Mario respawns at the spawn point by himself.
    sim.mario.request_death_warp = None;
    let platforms_specs = sim.course.extras.platforms.clone();
    let mut platforms = Platforms::new(&platforms_specs);
    platforms.install(&mut sim.mario);
    let mut goombas = spawn_goombas(&sim.course.extras.goombas, &sim.mario.world);
    let mut cannons = (!sim.course.extras.cannons.is_empty()).then(|| {
        let mut c = Cannons::new(&sim.course.extras.cannons, sim.course.extras.extra_collision.get("cannonLid"));
        c.reset(&mut sim.mario.world, progress.save.cannon_open(&id));
        c
    });
    let mut colliders = build_colliders(&mut sim, progress.save.wing_switch_on());

    // Loader parity.
    let counts = &fx["counts"];
    let static_coins = sim.pickups.iter().filter(|p| p.kind == PickupKind::Coin).count();
    let red = sim.pickups.iter().filter(|p| p.kind == PickupKind::Red).count();
    let have = [
        ("platforms", platforms.list.len()),
        ("goombas", goombas.len()),
        ("coins", static_coins),
        ("redCoins", red),
        ("cannons", cannons.as_ref().map_or(0, |c| c.entries.len())),
        ("doors", progress.doors.as_ref().map_or(0, |d| d.leaves.len())),
        ("signs", sim.course.extras.signs.len()),
        ("objects", sim.mario.objects.len()),
    ];
    for (name, count) in have {
        if counts[name].as_u64() != Some(count as u64) {
            return Err(format!("{tag}: {name} count {count} vs {}", counts[name]));
        }
    }

    let paintings = progress.data.painting_table();
    let mut paint_surfaces: Vec<(i32, Arc<jumpman_sim::collision::Surface>)> = vec![];
    for s in sim.mario.world.surfaces() {
        if s.kind >= 0xd3 && s.kind < 0xd3 + 0x2d && paintings.contains_key(&(s.kind - 0xd3)) && !paint_surfaces.iter().any(|(k, _)| *k == s.kind) {
            paint_surfaces.push((s.kind, s.clone()));
        }
    }
    paint_surfaces.sort_by_key(|(k, _)| *k);

    let categories = ["platform", "goomba", "buddy", "cannon", "door", "sign", "painting", "redcoin", "coin"];
    let list_features = |kind: &'static str, sim: &GameSim, platforms: &Platforms, goombas: &[WalkingGoomba], cannons: &Option<Cannons>, progress: &Progress| -> Vec<Feature> {
        let f = |pos: [f64; 3], y_offset: f64, spread: i64| Feature { kind, pos, y_offset, spread, front_yaw: None, front_dist: None };
        match kind {
            "platform" => platforms.list.iter().map(|p| f(p.pos.to_array(), 200.0, 2)).collect(),
            "goomba" => goombas.iter().map(|g| f(g.pos.to_array(), 120.0, 3)).collect(),
            "buddy" => sim
                .mario
                .objects
                .objects
                .iter()
                .filter(|o| matches!(o.behavior, K::BobombBuddy | K::BobombBuddyAdvice))
                .map(|o| Feature { front_dist: Some(55.0), ..f(o.pos, 60.0, 0) })
                .collect(),
            "cannon" => cannons.as_ref().map_or(vec![], |c| c.entries.iter().map(|e| f(e.seat.home, -240.0, 1)).collect()),
            "door" => {
                let mut v: Vec<Feature> = progress.doors.as_ref().map_or(vec![], |d| d.leaves.iter().map(|l| f(l.door.pos, 100.0, 3)).collect());
                v.sort_by(|a, b| a.pos[0].partial_cmp(&b.pos[0]).unwrap().then(a.pos[1].partial_cmp(&b.pos[1]).unwrap()).then(a.pos[2].partial_cmp(&b.pos[2]).unwrap()));
                v
            }
            "sign" => sim.course.extras.signs.iter().map(|s| Feature { front_yaw: Some(s.yaw), front_dist: Some(110.0), ..f(s.pos, 100.0, 0) }).collect(),
            "painting" => paint_surfaces
                .iter()
                .map(|(_, s)| {
                    f(
                        [(s.v1[0] + s.v2[0] + s.v3[0]) / 3.0, s.v1[1].max(s.v2[1]).max(s.v3[1]), (s.v1[2] + s.v2[2] + s.v3[2]) / 3.0],
                        200.0,
                        0,
                    )
                })
                .collect(),
            "redcoin" => sim.pickups.iter().filter(|p| p.kind == PickupKind::Red).map(|p| f(p.pos, 150.0, 2)).collect(),
            _ => sim.pickups.iter().filter(|p| p.kind != PickupKind::Red).enumerate().filter(|(i, _)| i % 3 == 0).map(|(_, p)| f(p.pos, 150.0, 3)).collect(),
        }
    };
    let non_empty: Vec<&'static str> = categories
        .iter()
        .copied()
        .filter(|k| !list_features(k, &sim, &platforms, &goombas, &cannons, &progress).is_empty())
        .collect();

    let want = nums(&fx["frames"]);
    let mut next = lcg((0xabc12345u32).wrapping_add((index as u32).wrapping_mul(7717)));
    let base = atan2s(-64.0, 0.0) as f64;
    let mut prev_down = 0u32;
    struct Seg {
        x: f64,
        y: f64,
        mag: f64,
        down: u32,
        pulse: u32,
        yaw: f64,
        left: u32,
        i: u32,
    }
    let mut seg: Option<Seg> = None;
    let mut target: Option<Feature> = None;
    let mut sign: Option<SignReading> = None;
    let mut dialog = DialogBox::default();
    let mut operator: Option<CannonOperator> = None;
    let mut world_hasher = WorldHasher { cache: HashMap::new(), keep: vec![] };
    let (mut window_start, mut w) = (0usize, -1i64);
    let mut stats = Stats { sign_frames: 0, operator_frames: 0, door_frames: 0 };
    let mut dump: Vec<Value> = vec![];
    let dump_path = std::env::var("GAME_DUMP").ok().filter(|_| std::env::var("GAME_ONLY").is_ok());
    let camera = (DVec3::new(0.0, 1000.0, 1000.0), DVec3::ZERO);
    let names = ["mario", "objects", "platforms", "goombas", "coins", "world", "misc"];
    let mut prev_pos;

    for f in 0..frames {
        let mut k = f as i64 - window_start as i64;
        if f == 0 || (k >= window as i64 && sign.is_none() && operator.is_none()) || k >= 4 * window as i64 {
            window_start = f;
            k = 0;
            w += 1;
        }
        if k == 0 && !non_empty.is_empty() {
            let kind = non_empty[(w as usize) % non_empty.len()];
            let list = list_features(kind, &sim, &platforms, &goombas, &cannons, &progress);
            let len = list.len();
            let feature = list.into_iter().nth((w as usize / non_empty.len()) % len).unwrap();
            let spread = feature.spread;
            let (rx, rz, rd) = (next() as i64, next() as i64, next() as i64);
            let mut dx = (((rx % (2 * spread + 1)) - spread) * 70) as f64;
            let mut dz = (((rz % (2 * spread + 1)) - spread) * 70) as f64;
            if let Some(front) = feature.front_dist {
                let yaw = feature.front_yaw.unwrap_or(((rx % 8) * 0x2000 - 0x8000) as f64);
                let dist = front + ((rd % 3) * 10) as f64;
                dx = dist * sins(yaw);
                dz = dist * coss(yaw);
            }
            let m = &mut sim.mario;
            m.pos = [feature.pos[0] + dx, feature.pos[1] + feature.y_offset, feature.pos[2] + dz];
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.face_angle[1] = atan2s(feature.pos[2] - m.pos[2], feature.pos[0] - m.pos[0]) as f64;
            m.set_action(ACT_FREEFALL, 0.0);
            sign = None;
            operator = None;
            seg = None;
            target = Some(feature);
        }
        // Controller.
        let (cx, cy, cmag, cdown, cyaw);
        let tk = target.as_ref().map(|t| t.kind);
        if sign.is_some() || operator.is_some() {
            cx = 0.0;
            cy = 0.0;
            cmag = 0.0;
            cdown = if f % 5 == 0 { A_BUTTON } else { 0 };
            cyaw = 0.0;
        } else if matches!(tk, Some("sign") | Some("buddy")) && (24..110).contains(&k) {
            cx = 0.0;
            cy = 0.0;
            cmag = 0.0;
            cdown = if k % 8 == 0 { A_BUTTON } else { 0 };
            cyaw = 0.0;
        } else if k < 45 && target.is_some() {
            let t = target.as_ref().unwrap().pos;
            let m = &sim.mario;
            let wanted = atan2s(t[2] - m.pos[2], t[0] - m.pos[0]) as f64;
            let d = if k % 20 == 5 || k % 20 == 6 || k % 20 == 10 {
                A_BUTTON
            } else if k % 20 == 15 {
                B_BUTTON
            } else {
                0
            };
            if tk == Some("sign") {
                cx = 0.0;
                cy = 0.0;
                cmag = 0.0;
                cdown = 0;
                cyaw = 0.0;
            } else {
                cx = 0.0;
                cy = 64.0;
                cmag = 64.0;
                cdown = d;
                cyaw = to_s16(wanted - base) as f64;
            }
        } else {
            if seg.as_ref().map_or(true, |g| g.left == 0) {
                let len = 6 + next() % 30;
                let still = next() % 6 == 0;
                let (x, y) = if still { (0, 0) } else { (next() as i32 % 129 - 64, next() as i32 % 129 - 64) };
                let (x, y) = (x as f64, y as f64);
                let mut down = 0u32;
                if next() % 100 < 45 {
                    down |= A_BUTTON;
                }
                if next() % 100 < 40 {
                    down |= B_BUTTON;
                }
                if next() % 100 < 15 {
                    down |= Z_TRIG;
                }
                let pulse = 3 + next() % 10;
                let yaw = (next() % 64) as f64 * 1024.0 - 32768.0;
                seg = Some(Seg { x, y, mag: (x * x + y * y).sqrt().min(64.0), down, pulse, yaw, left: len, i: 0 });
            }
            let g = seg.as_mut().unwrap();
            let mut d = g.down;
            if (g.i / g.pulse) % 2 == 1 {
                d &= Z_TRIG;
            }
            if k % 14 == 0 {
                d |= B_BUTTON;
            }
            g.i += 1;
            g.left -= 1;
            cx = g.x;
            cy = g.y;
            cmag = g.mag;
            cdown = d;
            cyaw = g.yaw;
        }
        let ctl = ControllerSnapshot { focus_yaw: None, stick_x: cx, stick_y: cy, stick_mag: cmag, button_down: cdown, button_pressed: cdown & !prev_down };
        prev_down = cdown;
        sim.camera_yaw = cyaw as i32;

        // ---- Game.stepWorld ----
        prev_pos = sim.mario.pos;
        let mut finished = false;
        if sign.is_none() && ctl.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
            if let Some(s) = readable_sign(&sim.course.extras.signs, &sim.mario, None).cloned() {
                sign = Some(SignReading::new(&s, &mut sim.mario, camera.0, camera.1).0);
            }
        }
        if let Some(reading) = sign.as_mut() {
            stats.sign_frames += 1;
            sim.tick_mario_only(&ctl);
            let (done, _) = reading.step(&mut sim.mario, &ctl, &mut dialog);
            if done {
                sign = None;
            }
            finished = true;
        } else if operator.is_some() && cannons.is_some() {
            stats.operator_frames += 1;
            let op = operator.as_mut().unwrap();
            let mut res = OperatorOutput::default();
            op.step(&mut sim.mario, &ctl, camera, cannons.as_mut().unwrap(), &mut res);
            if res.save_unlock {
                progress.save.open_cannon(&id);
            }
            if sim.mario.action == ACT_READING_NPC_DIALOG {
                sim.tick_mario_only(&ctl);
            }
            object_manager::after_mario_step(&mut sim.mario);
            sync_colliders(&mut sim, &mut colliders, progress.save.wing_switch_on());
            if res.finished {
                operator = None;
            }
            finished = true;
        } else if let Some(doors) = progress.doors.as_mut() {
            if doors.step(&mut sim.mario, &mut progress.save, &mut |_| {}) {
                stats.door_frames += 1;
                finished = true;
            }
        }
        if !finished {
            let _ = platforms.step(&mut sim.mario);
            for g in &mut goombas {
                if g.step(&sim.mario.pos, &sim.mario.world) {
                    sim.spawn_coin(g.pos.to_array(), true, 1.0);
                }
            }
            let mut hooks = StageHooks {
                colliders: Some(&mut colliders),
                cannons: cannons.as_mut(),
                wing_unlocked: progress.save.wing_switch_on(),
                course_id: &id,
                cannon_unlocked: progress.save.cannon_open(&id),
                operator: None,
            };
            sim.tick_with(&ctl, &mut hooks);
            let stopped_op = hooks.operator.take();
            let stopped = stopped_op.is_some();
            if stopped {
                operator = stopped_op;
            } else {
                for e in interact(&mut goombas, &mut sim.mario, &prev_pos) {
                    if let GoombaEvent::Coin(pos) = e {
                        sim.spawn_coin(pos, true, 1.0);
                    }
                }
            }
        }

        if let (Ok(path), true) = (std::env::var("GAME_SURF_RS"), f == 0) {
        let all: Vec<Vec<f64>> = sim
                .mario
            .world
            .surfaces()
            .iter()
            .map(|s| {
                let owner = match s.object {
                    Some(t) if t < 0x4000_0000 => t as f64,
                    _ => -1.0,
                };
                vec![s.v1[0], s.v1[1], s.v1[2], s.v2[0], s.v2[1], s.v2[2], s.v3[0], s.v3[1], s.v3[2], s.kind as f64, s.force as f64, s.flags as f64, s.room.map_or(-1.0, |r| r as f64), owner]
            })
            .collect();
        std::fs::write(path, serde_json::to_vec(&all).unwrap()).unwrap();
    }
        // ---- hashes ----
        let m = &sim.mario;
        let mut mf = nf(&mario_fields(m));
        let (cannon_flag, cannon_home, cannon_cd) = match &m.cannon {
            Some(c) => (1.0, c.home[0], c.cooldown),
            None => (0.0, 0.0, 0.0),
        };
        mf.extend(nf(&[cannon_flag, cannon_home, cannon_cd, m.floor.as_ref().map_or(-1.0, |s| s.kind as f64), m.floor.as_ref().map_or(-2.0, |s| s.room.map_or(-1.0, |r| r as f64))]));
        let mut h_mario = Hasher::new();
        hash_fields(&mf, &mut h_mario);

        let mut h_objects = Hasher::new();
        h_objects.word(m.objects.len() as u32);
        let object_dump: Vec<Vec<Field>> = m.objects.ids().map(|i| object_fields(&m.objects[i])).collect();
        for fields in &object_dump {
            hash_fields(fields, &mut h_objects);
        }
        let mut h_plat = Hasher::new();
        let plat_dump: Vec<Vec<f64>> = platforms.list.iter().map(|p| p.debug_fields()).collect();
        for p in &plat_dump {
            hash_fields(&nf(p), &mut h_plat);
        }
        let mut h_goomba = Hasher::new();
        let goomba_dump: Vec<Vec<f64>> = goombas.iter().map(|g| g.debug_fields()).collect();
        for g in &goomba_dump {
            hash_fields(&nf(g), &mut h_goomba);
        }
        let mut h_coin = Hasher::new();
        let coin_fields = |p: &super::runner::Pickup| -> Vec<f64> {
            let l = p.loot.as_ref();
            let ld = l.map(|l| l.debug_fields());
            vec![
                p.pos[0], p.pos[1], p.pos[2], p.collected as i32 as f64, if p.kind == PickupKind::Red { 1.0 } else { p.value as f64 },
                (!p.collected && l.map_or(true, |l| l.visible)) as i32 as f64, 1.0, l.is_some() as i32 as f64,
                l.map_or(0.0, |l| l.tangible as i32 as f64), l.map_or(0.0, |l| l.visible as i32 as f64),
                ld.map_or(0.0, |d| d[5]), ld.map_or(0.0, |d| d[0]), ld.map_or(0.0, |d| d[1]), ld.map_or(0.0, |d| d[2]), ld.map_or(0.0, |d| d[3]),
                ld.map_or(0.0, |d| d[4]),
            ]
        };
        let plain: Vec<&super::runner::Pickup> = sim.pickups.iter().filter(|p| p.kind != PickupKind::Red).collect();
        let reds: Vec<&super::runner::Pickup> = sim.pickups.iter().filter(|p| p.kind == PickupKind::Red).collect();
        h_coin.word(plain.len() as u32);
        let coin_dump: Vec<Vec<f64>> = plain.iter().chain(reds.iter()).map(|p| coin_fields(p)).collect();
        for c in &coin_dump {
            hash_fields(&nf(c), &mut h_coin);
        }
        let h_world = world_hasher.hash(&m.world);
        let mut misc: Vec<Field> = vec![n(sim.coins as f64), n(sim.red_coins as f64), n(m.heal_counter)];
        if let Some(c) = &cannons {
            misc.push(b(c.open));
            misc.push(n(c.opening_tick() as f64));
            for e in &c.entries {
                // The TS shares one seat object between Mario and the cannon: the final state of the
                // action (aiming, or the fire frame with cooldown 6) is the truth.
                let seat = match (&m.cannon, &m.fired_cannon) {
                    (Some(s), _) if s.home == e.seat.home => s,
                    (_, Some(s)) if s.home == e.seat.home => s,
                    _ => &e.seat,
                };
                misc.extend([
                    n(e.lid_pos.x), n(e.lid_pos.y), n(e.lid_pos.z), b(e.lid_visible), b(e.parts_visible), n(e.base_pos.x), n(e.base_pos.y), n(e.base_pos.z),
                    n(angle_to_radians(e.base_yaw)), n(e.barrel_rot_x), n(seat.cooldown), n(seat.rise), n(seat.shake), n(seat.pitch), n(seat.yaw),
                    n(seat.input_yaw),
                ]);
            }
        }
        if let Some(d) = &progress.doors {
            misc.push(b(d.passage_active()));
            misc.push(b(d.unlocking_active()));
            for l in &d.leaves {
                misc.extend([n(l.timer as f64), n(l.pos[0]), n(l.pos[1]), n(l.pos[2])]);
            }
        }
        misc.extend([n(progress.save.stars() as f64), b(sign.is_some()), b(operator.is_some())]);
        if let Some(s) = &sign {
            let (phase, page, clock) = s.debug_state();
            let _ = phase;
            misc.extend([F::S(phase.to_string()), n(page as f64), n(clock as f64)]);
        }
        if let Some(o) = &operator {
            let (stage, page, timer) = o.debug_state();
            misc.extend([F::S(stage.to_string()), n(page as f64), n(timer as f64), F::S(o.text().unwrap_or("").to_string())]);
        }
        misc.extend([n(m.sign_motion[0]), n(m.sign_motion[1]), n(m.sign_motion[2])]);
        let dest = painting_destination(m.floor.as_ref().map_or(-1, |s| s.kind), m.pos[1] - m.floor_height, &paintings);
        misc.push(F::S(dest.unwrap_or_else(|| "-".into())));
        let mut h_misc = Hasher::new();
        hash_fields(&misc, &mut h_misc);

        let got = [h_mario.0, h_objects.0, h_plat.0, h_goomba.0, h_coin.0, h_world, h_misc.0];
        if dump_path.is_some() {
            let js = |fields: &Vec<Field>| -> Vec<Value> {
                fields.iter().map(|f| match f {
                    F::N(v) => serde_json::json!(if v.is_finite() { *v } else { f64::NAN }),
                    F::S(s) => Value::String(s.clone()),
                }).collect()
            };
            dump.push(serde_json::json!({
                "mario": js(&mf), "objects": object_dump.iter().map(js).collect::<Vec<_>>(), "platforms": plat_dump, "goombas": goomba_dump,
                "coins": coin_dump, "misc": js(&misc),
            }));
        }
        let bad: Vec<&str> = (0..7).filter(|&j| got[j] as f64 != want[f * 7 + j]).map(|j| names[j]).collect();
        if !bad.is_empty() {
            if let Some(p) = &dump_path {
                let _ = std::fs::write(p, serde_json::to_vec(&dump).unwrap());
            }
            return Err(format!(
                "{tag} frame {f} (window {w}, k {k}, {}): {} differ (action {:#x} pos {:?})",
                target.as_ref().map_or("-", |t| t.kind),
                bad.join("+"),
                sim.mario.action,
                sim.mario.pos
            ));
        }
    }
    Ok(stats)
}

#[test]
fn app_layer_matches_typescript() {
    super::world::legacy_fixture_mode(true); // goldens predate the group 1 actors
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sim/tests/fixtures/game.json");
    let fx: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let frames = fx["frames"].as_u64().unwrap() as usize;
    let window = fx["window"].as_u64().unwrap() as usize;
    assert_eq!(fx["hashes"].as_u64(), Some(7));
    let only = std::env::var("GAME_ONLY").ok();
    let mut failures = vec![];
    let (mut signs, mut operators, mut doors) = (0, 0, 0);
    for (i, c) in fx["courses"].as_array().unwrap().iter().enumerate() {
        let tag = format!("{}.{}.{}", c["id"].as_str().unwrap(), c["act"], c["area"]);
        if only.as_deref().is_some_and(|o| o != tag) {
            continue;
        }
        // rr's terrain was exported with both `#ifdef VERSION_JP` vertex lists (ghost triangles across the
        // level); the TS fixture recorded that data, the corrected export cannot match it.
        if matches!(c["id"].as_str(), Some("rr" | "lll")) {
            continue;
        }
        match replay(c, i, frames, window) {
            Ok(s) => {
                signs += s.sign_frames;
                operators += s.operator_frames;
                doors += s.door_frames;
            }
            Err(e) => failures.push(e),
        }
    }
    assert!(failures.is_empty(), "{} course(s) diverged:\n{}", failures.len(), failures.join("\n"));
    if only.is_none() {
        assert!(signs > 100 && operators > 100 && doors > 100, "vacuous run: sign {signs} operator {operators} door {doors}");
    }
}
