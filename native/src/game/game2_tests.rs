//! Second app-layer differential test against `tests/rustFixtures.game2.gen.test.ts`: the King Bob-omb
//! dialog (ids 17, 116, 128), the Koopa the Quick and racing-penguin questions, the Wing Cap switch,
//! `CoinEffects`, fading warps and instant warps. The TypeScript runs the matching parts of
//! `Game.stepWorld`; this replays the same scripts through the Rust app-layer functions
//! (`step_king`, `GameSim::tick_with` / `koopa_question`, `WingSwitch`, `Progress::step_fading_warps`
//! / `post_tick`, the `CoinFx` core) and compares per-frame state hashes bit for bit.
//! `GAME2_ONLY=<name>` restricts the run.
use super::game_tests::WorldHasher;
use super::longrun_tests::{mario_fields, nums, object_fields, Hasher, F};
use super::runner::GameSim;
use super::signpost::{king_dialog_pages, DialogBox};
use super::world::CourseRequest;
use crate::objects::colliders::build_colliders;
use crate::objects::dialog::{step_king, step_tail, CameraOverride, KingDialog, ModalState};
use crate::objects::stage::StageHooks;
use crate::progress::coin_fx::CoinFx;
use crate::progress::data::ProgressData;
use crate::progress::flow::Progress;
use crate::progress::save::SaveFile;
use crate::progress::wing::WingSwitch;
use bevy::math::DVec3;
use jumpman_sim::behaviors::koopa_the_quick::{KOOPA_RACE_COUNTDOWN, KOOPA_RACE_INVITATION, KOOPA_RACE_RESULT_DIALOG};
use jumpman_sim::math::*;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::object::{mario_hitbox, ObjectBehaviorKind as K, INTERACT_BREAKABLE};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;
use serde_json::Value;
use std::path::PathBuf;

pub(super) fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

pub(super) fn lcg(seed: u32) -> impl FnMut() -> u32 {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        s >> 16
    }
}

pub(super) fn hash(list: &[F]) -> u32 {
    let mut h = Hasher::new();
    for f in list {
        match f {
            F::N(v) => h.num(*v),
            F::S(s) => h.str(s),
        }
    }
    h.0
}

pub(super) fn nn(v: f64) -> F {
    F::N(v)
}

pub(super) fn mario_hash(m: &MarioRuntime) -> u32 {
    let mut mf: Vec<F> = mario_fields(m).into_iter().map(F::N).collect();
    let (flag, home, cd) = match &m.cannon {
        Some(c) => (1.0, c.home[0], c.cooldown),
        None => (0.0, 0.0, 0.0),
    };
    for v in [flag, home, cd, m.floor.as_ref().map_or(-1.0, |s| s.kind as f64), m.floor.as_ref().map_or(-2.0, |s| s.room.map_or(-1.0, |r| r as f64))] {
        mf.push(F::N(v));
    }
    hash(&mf)
}

pub(super) fn all_objects_hash(m: &MarioRuntime) -> u32 {
    let mut h = Hasher::new();
    h.word(m.objects.len() as u32);
    for i in m.objects.ids() {
        for f in object_fields(&m.objects[i]) {
            match f {
                F::N(v) => h.num(v),
                F::S(s) => h.str(&s),
            }
        }
    }
    h.0
}

fn one_object_hash(m: &MarioRuntime, id: jumpman_sim::object::ObjId) -> u32 {
    hash(&object_fields(&m.objects[id]))
}

pub(super) fn ctl(x: f64, y: f64, down: u32, prev: u32) -> ControllerSnapshot {
    ControllerSnapshot { focus_yaw: None, stick_x: x, stick_y: y, stick_mag: (x * x + y * y).sqrt().min(64.0), button_down: down, button_pressed: down & !prev }
}

pub(super) fn load(id: &str, act: i32, area: i32, index: u32) -> GameSim {
    seed_random(0x2badbeefu32.wrapping_add(index.wrapping_mul(7919)));
    let mut req = CourseRequest::new(id);
    req.act = act;
    req.area = area;
    let mut sim = GameSim::load(&assets(), req).unwrap();
    sim.mario.request_death_warp = None;
    sim
}

fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

struct Golden {
    frames: Vec<u32>,
    groups: usize,
}

fn golden(fx: &Value, name: &str) -> Golden {
    let s = fx["scenarios"].as_array().unwrap().iter().find(|s| s["name"] == name).unwrap_or_else(|| panic!("no scenario {name}"));
    Golden { frames: nums(&s["frames"]).into_iter().map(|v| v as u32).collect(), groups: s["groups"].as_u64().unwrap() as usize }
}

struct Check<'a> {
    name: &'a str,
    want: Golden,
    got: Vec<u32>,
}

impl Check<'_> {
    fn push(&mut self, hashes: &[u32]) {
        assert_eq!(hashes.len(), self.want.groups, "{}: group count", self.name);
        self.got.extend_from_slice(hashes);
    }

    fn finish(self, group_names: &[&str]) -> Result<(), String> {
        let frames = self.got.len() / self.want.groups;
        if self.want.frames.len() != self.got.len() {
            return Err(format!("{}: {} frames vs {} expected", self.name, frames, self.want.frames.len() / self.want.groups));
        }
        for (i, (a, b)) in self.got.iter().zip(&self.want.frames).enumerate() {
            if a != b {
                return Err(format!("{}: frame {} group {} differs ({:08x} vs TS {:08x})", self.name, i / self.want.groups, group_names[i % self.want.groups], a, b));
            }
        }
        Ok(())
    }
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sim/tests/fixtures/game2.json");
    serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn wanted(only: &Option<String>, name: &str) -> bool {
    only.as_ref().map_or(true, |o| o == name)
}

// ---- King Bob-omb dialog ---------------------------------------------------------------------------
fn king_scenario(fx: &Value, dialog_id: i32, index: u32) -> Result<(), String> {
    let name = format!("king{dialog_id}");
    let mut check = Check { name: &name, want: golden(fx, &name), got: vec![] };
    let mut sim = load("bob", 1, 1, index);
    let king = sim.mario.objects.find_by_behavior(K::KingBobomb).unwrap();
    sim.mario.objects[king].set_var("dialog", dialog_id as f64);
    let kpos = sim.mario.objects[king].pos;
    let m = &mut sim.mario;
    m.pos = [kpos[0] + 300.0, kpos[1] + 40.0, kpos[2]];
    m.face_angle[1] = atan2s(kpos[2] - m.pos[2], kpos[0] - m.pos[0]) as f64;
    m.set_action(ACT_FREEFALL, 0.0);
    let eye = DVec3::new(js_round(m.pos[0]) + 500.0, js_round(m.pos[1]) + 400.0, js_round(m.pos[2]) + 500.0);
    let saved_focus = DVec3::new(js_round(m.pos[0]), js_round(m.pos[1]) + 100.0, js_round(m.pos[2]));
    let mut dialog: Option<KingDialog> = None;
    let mut state = ModalState::default();
    let mut ui = DialogBox::default();
    let mut cam = CameraOverride::default();
    let mut log: Vec<F> = vec![];
    let mut prev_down = 0u32;
    let mut next = lcg(77 + index);
    for f in 0..900u32 {
        let mut down = if f > 30 && f % 9 == 0 {
            A_BUTTON
        } else if f % 31 == 7 {
            B_BUTTON
        } else {
            0
        };
        if next() % 40 == 0 {
            down = A_BUTTON;
        }
        let c = ctl(0.0, 0.0, down, prev_down);
        prev_down = down;
        sim.camera_yaw = 0;
        if let Some(k) = dialog.as_mut() {
            let done = step_king(k, &mut sim, &c, (eye, saved_focus), &mut ui, &mut cam, &mut state);
            if let Some(e) = k.music.take() {
                log.push(F::S(if matches!(e, crate::audio::MusicEvent::KingDefeat) { "kingDefeat" } else { "kingFight" }.into()));
            }
            if done {
                dialog = None;
            }
            step_tail(&mut state, &sim.mario.world, &mut cam);
        } else {
            sim.tick(&c);
            step_tail(&mut state, &sim.mario.world, &mut cam);
            for e in std::mem::take(&mut sim.events) {
                match e {
                    super::runner::SimEvent::Object(ObjectEvent::KingDialog { king, id }) => {
                        sim.mario.used_obj = Some(king);
                        sim.mario.set_action(ACT_READING_NPC_DIALOG, 2.0);
                        dialog = Some(KingDialog::new(king, id as i32, eye));
                        log.push(F::S("begin".into()));
                        log.push(nn(id as f64));
                    }
                    super::runner::SimEvent::Object(ObjectEvent::Boss { .. }) => log.push(F::S("boss".into())),
                    super::runner::SimEvent::Object(ObjectEvent::KingLeave) => log.push(F::S("kingLeave".into())),
                    _ => {}
                }
            }
        }
        // The dialog's own page log: the pages shown are tracked through the DialogBox.
        let shown = ui.text.clone().unwrap_or_default();
        let camera = dialog.as_ref().and_then(|k| k.camera.as_ref()).or(state.tail.as_ref());
        let mut dh = vec![nn(dialog.is_some() as i32 as f64), nn(dialog.as_ref().map_or(-9.0, |k| k.page as f64))];
        dh.push(nn(camera.map_or(-1.0, |c| c.frame as f64)));
        dh.push(nn(camera.map_or(-1.0, |c| c.return_frames_opt())));
        if camera.is_some() {
            if let Some((e, fo)) = cam.cur {
                dh.extend([nn(e.x), nn(e.y), nn(e.z), nn(fo.x), nn(fo.y), nn(fo.z)]);
            }
        }
        dh.push(F::S(shown));
        dh.extend(log.iter().cloned());
        if std::env::var("GAME2_DUMP").ok().as_deref() == Some(name.as_str()) {
            let txt: Vec<String> = dh.iter().map(|f| match f { F::N(v) => format!("{v}"), F::S(s) => format!("{s:?}") }).collect();
            eprintln!("DUMP {f} [{}]", txt.join(","));
        }
        let king = sim.mario.objects.find_by_behavior(K::KingBobomb).unwrap_or(king);
        check.push(&[mario_hash(&sim.mario), one_object_hash(&sim.mario, king), hash(&dh)]);
    }
    check.finish(&["mario", "king", "dialog"])
}

/// The world factory's own colliders (moving platforms, the cannon lid): they are part of the TS world.
fn install_app_colliders(sim: &mut GameSim, progress: &Progress, id: &str) {
    let specs = sim.course.extras.platforms.clone();
    let mut platforms = super::platform::Platforms::new(&specs);
    platforms.install(&mut sim.mario);
    if !sim.course.extras.cannons.is_empty() {
        let mut c = super::cannons::Cannons::new(&sim.course.extras.cannons, sim.course.extras.extra_collision.get("cannonLid"));
        c.reset(&mut sim.mario.world, progress.save.cannon_open(id));
        let _ = c;
    }
}

// ---- Koopa the Quick and the racing penguin -----------------------------------------------------------
fn race_scenario(fx: &Value, kind: &str, index: u32) -> Result<(), String> {
    let koopa_kind = kind.starts_with("koopa");
    let mut check = Check { name: kind, want: golden(fx, kind), got: vec![] };
    let act = fx["scenarios"].as_array().unwrap().iter().find(|s| s["name"] == kind).unwrap()["act"].as_i64().unwrap() as i32;
    let mut sim = load(if koopa_kind { "bob" } else { "ccm" }, act, if koopa_kind { 1 } else { 2 }, index);
    let id = if koopa_kind { "bob" } else { "ccm" };
    let progress = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    sim.mario.request_death_warp = None;
    install_app_colliders(&mut sim, &progress, id);
    let save = SaveFile::in_memory(1);
    set_wing_boxes(&mut sim, false);
    let target = sim.mario.objects.find_by_behavior(if koopa_kind { K::KoopaTheQuick } else { K::RacingPenguin }).unwrap();
    let tpos = sim.mario.objects[target].pos;
    let m = &mut sim.mario;
    m.pos = [tpos[0] + if koopa_kind { 250.0 } else { 150.0 }, tpos[1] + if koopa_kind { 30.0 } else { 20.0 }, tpos[2]];
    m.face_angle[1] = atan2s(tpos[2] - m.pos[2], tpos[0] - m.pos[0]) as f64;
    m.set_action(ACT_FREEFALL, 0.0);
    let mut colliders = build_colliders(&mut sim, save.wing_switch_on());
    let mut world_hasher = WorldHasher { cache: Default::default(), keep: vec![] };
    let accept = kind.ends_with('A');
    let mut next = lcg(1234 + index);
    let (mut prev_down, mut held) = (0u32, 0u32);
    let tag = |e: &ObjectEvent| -> Option<String> {
        Some(match e {
            ObjectEvent::Race { state } => format!("race{}", state),
            ObjectEvent::Dialogue { text } => format!("dialogue{text}"),
            ObjectEvent::RacerSound { cue, .. } => format!("racerSound{cue}"),
            ObjectEvent::KoopaSound { cue, .. } => format!("koopaSound{cue}"),
            ObjectEvent::PenguinSound { .. } => "penguinSound".into(),
            ObjectEvent::PenguinReward { .. } => "penguinReward".into(),
            _ => return None,
        })
    };
    for frame in 0..1500u32 {
        let a = sim.mario.objects[target].action;
        let asking = if koopa_kind { a == KOOPA_RACE_INVITATION || a == KOOPA_RACE_COUNTDOWN || a == KOOPA_RACE_RESULT_DIALOG } else { a == 1 };
        let (mut down, mut sx, mut sy) = (0u32, 0.0, 0.0);
        if asking {
            held += 1;
            if held > 6 && held % 5 == 1 {
                down = if accept { A_BUTTON } else { B_BUTTON };
            }
        } else {
            held = 0;
            let mag = if next() % 3 == 0 { 0 } else { 64 };
            sx = if mag != 0 { (next() as i32 % 129 - 64) as f64 } else { 0.0 };
            sy = if mag != 0 { (next() as i32 % 129 - 64) as f64 } else { 0.0 };
            if next() % 5 == 0 {
                down |= A_BUTTON;
            }
            if next() % 9 == 0 {
                down |= B_BUTTON;
            }
        }
        let c = ctl(sx, sy, down, prev_down);
        prev_down = down;
        sim.camera_yaw = 0;
        if koopa_kind && sim.koopa_question(&c) {
        } else {
            let mut hooks = StageHooks { colliders: Some(&mut colliders), cannons: None, wing_unlocked: false, course_id: "x", cannon_unlocked: false, operator: None };
            sim.tick_with(&c, &mut hooks);
        }
        let mut evs: Vec<F> = vec![];
        for e in std::mem::take(&mut sim.events) {
            if let super::runner::SimEvent::Object(o) = &e {
                if let Some(t) = tag(o) {
                    evs.push(F::S(t));
                }
            }
        }
        evs.push(nn(sim.mario.objects[target].action as f64));
        evs.push(nn(sim.coins as f64));
        if std::env::var("GAME2_DUMP").ok().as_deref() == Some(kind) && frame == std::env::var("GAME2_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(0) {
            let objs: Vec<String> = sim.mario.objects.ids().map(|i| format!("[{:?},{}]", sim.mario.objects[i].behavior, one_object_hash(&sim.mario, i))).collect();
            eprintln!("DUMPTARGET {:?}", object_fields(&sim.mario.objects[target]).iter().map(|f| match f { F::N(v) => format!("{v}"), F::S(s) => s.clone() }).collect::<Vec<_>>());
            eprintln!("DUMPMARIO {:?}", mario_fields(&sim.mario));
            eprintln!("DUMPOBJ [{}] events {}", objs.join(","), evs.len());
        }
        check.push(&[mario_hash(&sim.mario), one_object_hash(&sim.mario, target), hash(&evs), all_objects_hash(&sim.mario), world_hasher.hash(&sim.mario.world)]);
    }
    check.finish(&["mario", "target", "events", "objects", "world"])
}

// ---- Wing Cap switch ----------------------------------------------------------------------------------------
fn wing_scenario(fx: &Value, index: u32) -> Result<(), String> {
    let mut check = Check { name: "wing", want: golden(fx, "wing"), got: vec![] };
    let mut sim = load("totwc", 1, 1, index);
    let mut save = SaveFile::in_memory(1);
    let data = ProgressData::load(&assets(), "totwc", 1);
    let mut colliders = build_colliders(&mut sim, false);
    let mut sw = WingSwitch::new(data.wing_switch.unwrap(), &data.extra_collision, std::sync::Arc::make_mut(&mut sim.mario.world), &save);
    let top = sw.top_pos;
    let mut hasher = WorldHasher { cache: Default::default(), keep: vec![] };
    let mut wing_on = false;
    let mut next = lcg(99);
    let mut prev_down = 0u32;
    for f in 0..420u32 {
        if f % 140 == 0 {
            let m = &mut sim.mario;
            m.pos = [top[0] + ((next() as i64 % 5) - 2) as f64 * 20.0, top[1] + 150.0 + (f % 3) as f64 * 40.0, top[2] + ((next() as i64 % 5) - 2) as f64 * 20.0];
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.set_action(ACT_FREEFALL, 0.0);
            if f == 280 {
                sw.reset(std::sync::Arc::make_mut(&mut sim.mario.world), false);
                wing_on = false;
                set_wing_boxes(&mut sim, false);
            }
        }
        let down = if f % 37 == 3 { A_BUTTON } else { 0 };
        let c = ctl(0.0, 0.0, down, prev_down);
        prev_down = down;
        sim.camera_yaw = 0;
        let mut hooks = StageHooks { colliders: Some(&mut colliders), cannons: None, wing_unlocked: wing_on, course_id: "totwc", cannon_unlocked: false, operator: None };
        sim.tick_with(&c, &mut hooks);
        let (floor, y, fh) = (sim.mario.floor.clone(), sim.mario.pos[1], sim.mario.floor_height);
        let pressed = sw.step(&floor, y, fh, std::sync::Arc::make_mut(&mut sim.mario.world));
        if pressed {
            save.press_wing_switch();
            wing_on = true;
            set_wing_boxes(&mut sim, true);
            crate::objects::colliders::sync(&mut sim, &mut colliders, true);
        }
        let sh = hash(&[nn(sw.timer as f64), nn(sw.height), nn(pressed as i32 as f64), nn(save.wing_switch_on() as i32 as f64)]);
        check.push(&[mario_hash(&sim.mario), sh, hasher.hash(&sim.mario.world), all_objects_hash(&sim.mario)]);
    }
    check.finish(&["mario", "switch", "world", "objects"])
}

fn set_wing_boxes(sim: &mut GameSim, on: bool) {
    for o in sim.mario.objects.objects.iter_mut().filter(|o| o.behavior == K::WingBox) {
        o.set_var("unlocked", if on { 1.0 } else { 0.0 });
        if o.action == 0 {
            o.hitbox.interact_type = if on { INTERACT_BREAKABLE } else { 0 };
        }
    }
}

// ---- CoinFx --------------------------------------------------------------------------------------------------
fn coin_fx_scenario(fx: &Value) -> Result<(), String> {
    let mut check = Check { name: "coinfx", want: golden(fx, "coinfx"), got: vec![] };
    let mut core = CoinFx::default();
    let mut next = lcg(4242);
    let snap = |core: &CoinFx| -> u32 {
        let (digits, bursts, sparks) = core.snapshot();
        let mut d: Vec<String> = digits.iter().map(|(y, age, vel)| format!("{},{},{}", js_num(*y), age, js_num(*vel))).collect();
        d.sort();
        let mut b: Vec<f64> = bursts.iter().map(|l| *l as f64).collect();
        b.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut s: Vec<String> = sparks.iter().map(|(age, scale, frame)| format!("{},{},{}", age, js_num(*scale), frame)).collect();
        s.sort();
        let mut f = vec![nn(d.len() as f64)];
        f.extend(d.into_iter().map(F::S));
        f.push(nn(b.len() as f64));
        f.extend(b.into_iter().map(nn));
        f.push(nn(s.len() as f64));
        f.extend(s.into_iter().map(F::S));
        hash(&f)
    };
    for _ in 0..400 {
        if next() % 9 == 0 {
            core.burst([1.0, 2.0, 3.0]);
        }
        if next() % 21 == 0 {
            let n = 1 + (next() % 7);
            core.number(n, [10.0, 20.0 + (next() % 5) as f64, 30.0]);
        }
        core.advance_for_test();
        check.push(&[snap(&core)]);
    }
    core.clear_for_test();
    check.push(&[snap(&core)]);
    check.finish(&["state"])
}

/// JS `String(number)` for the values that occur here (integers or short decimals).
fn js_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e21 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ---- Fading and instant warps ---------------------------------------------------------------------------------
fn fade_scenario(fx: &Value, index: u32) -> Result<(), String> {
    let mut check = Check { name: "fade", want: golden(fx, "fade"), got: vec![] };
    let mut sim = load("bob", 1, 1, index);
    let mut progress = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    sim.mario.request_death_warp = None;
    let pads = sim.course.fading_warps.clone();
    let mut prev_down = 0u32;
    let (mut cue_count, mut snap_yaw) = (0usize, 0.0f64);
    for f in 0..1040u32 {
        if f % 260 == 0 {
            let pad = &pads[((f / 260) as usize) % pads.len()];
            let m = &mut sim.mario;
            m.pos = [pad.pos[0] + if f % 520 != 0 { 30.0 } else { 0.0 }, pad.pos[1] + 120.0, pad.pos[2]];
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.set_action(ACT_FREEFALL, 0.0);
        }
        let down = (if f % 260 > 70 && f % 260 < 90 { Z_TRIG } else { 0 }) | if f % 61 == 0 { A_BUTTON } else { 0 };
        let c = ctl(0.0, 0.0, down, prev_down);
        prev_down = down;
        sim.camera_yaw = 0;
        progress.cues.clear();
        progress.step_fading_warps(&mut sim);
        cue_count += progress.cues.iter().filter(|c| **c == "teleport").count();
        if let Some(yaw) = progress.rig_snap.take() {
            snap_yaw = yaw;
        }
        sim.tick(&c);
        let (opacity, just, fading) = progress.fade_state();
        if std::env::var("GAME2_DUMP").ok().as_deref() == Some("fade") && (170..178).contains(&f) {
            eprintln!("DUMPF {f} {:?}", [opacity as f64, just as i32 as f64, fading as i32 as f64, snap_yaw, cue_count as f64, sim.mario.action as f64, sim.mario.action_timer]);
        }
        check.push(&[mario_hash(&sim.mario), hash(&[nn(opacity as f64), nn(just as i32 as f64), nn(fading as i32 as f64), nn(snap_yaw), nn(cue_count as f64)])]);
    }
    check.finish(&["mario", "fade"])
}

fn instant_scenario(fx: &Value, index: u32) -> Result<(), String> {
    let mut check = Check { name: "instant", want: golden(fx, "instant"), got: vec![] };
    let mut sim = load("castle_inside", 1, 2, index);
    let mut progress = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
    sim.mario.request_death_warp = None;
    let warps = progress.data.instant_warps.clone();
    let surfaces: Vec<_> = sim.mario.world.surfaces().iter().filter(|s| s.kind >= 0x1b && warps.contains_key(&(s.kind - 0x1b).to_string())).cloned().collect();
    let mut shifts = 0.0;
    for f in 0..360u32 {
        if f % 90 == 0 && !surfaces.is_empty() {
            let s = &surfaces[((f / 90) as usize * 7) % surfaces.len()];
            let m = &mut sim.mario;
            m.pos = [(s.v1[0] + s.v2[0] + s.v3[0]) / 3.0, s.v1[1].max(s.v2[1]).max(s.v3[1]) + 60.0, (s.v1[2] + s.v2[2] + s.v3[2]) / 3.0];
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.set_action(ACT_FREEFALL, 0.0);
        }
        sim.camera_yaw = 0;
        sim.tick(&ctl(0.0, 0.0, 0, 0));
        // Hash before the end-of-tick shift: the TS applies it at the start of the next tick.
        check.push(&[mario_hash(&sim.mario), hash(&[nn(shifts)])]);
        let before = sim.mario.pos;
        progress.post_tick(&mut sim, &ctl(0.0, 0.0, 0, 0));
        if sim.mario.pos != before {
            shifts += 1.0;
        }
    }
    check.finish(&["mario", "shifts"])
}

#[test]
fn app_layer_two_matches_typescript() {
    let fx = fixture();
    let only = std::env::var("GAME2_ONLY").ok();
    let mut failures = vec![];
    let mut run = |name: &str, r: &dyn Fn() -> Result<(), String>| {
        if wanted(&only, name) {
            if let Err(e) = r() {
                failures.push(e);
            }
        }
    };
    for (i, id) in [17, 116, 128].into_iter().enumerate() {
        run(&format!("king{id}"), &|| king_scenario(&fx, id, i as u32));
    }
    for (i, k) in ["koopaA", "koopaB", "penguinA", "penguinB"].into_iter().enumerate() {
        run(k, &|| race_scenario(&fx, k, 3 + i as u32));
    }
    run("wing", &|| wing_scenario(&fx, 7));
    run("coinfx", &|| coin_fx_scenario(&fx));
    run("fade", &|| fade_scenario(&fx, 8));
    run("instant", &|| instant_scenario(&fx, 9));
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    let _ = (mario_hitbox, king_dialog_pages);
}
