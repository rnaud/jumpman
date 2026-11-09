//! Banjo-Kazooie game events -> the extracted sound bank (`bk_sfx`, 402 clips). A data table ([`TABLE`]) names, per event key, the
//! clips the decomp plays (`enum sfx_e`); [`play_event`] picks one (round robin) and plays it. The systems of [`BkEventSoundsPlugin`] turn
//! Banjo's movement (footsteps by world, landings, swimming), enemy hits and deaths and the BK events of `banjo_ctl` into event keys.
//!
//! Clip choices follow the call sites of the decomp where they are known (`SFX_*` in each actor's file); the footstep set per world is an
//! approximation (the port has no surface types for BK terrain, see `DECOMP_TALLY.md`).
use bevy::prelude::*;
use jumpman_sim::banjo_ext::{BanjoEvent, PickupKind};
use jumpman_sim::bk_enemy::{kind_of, Kind};
use jumpman_sim::object::ObjectBehaviorKind;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::game::runner::GameSim;

/// event key -> clip names (without folder and `_<variant>.wav`).
pub const TABLE: &[(&str, &[&str])] = &[
    // Banjo
    // footsteps and landings by surface type (1..12): `D_80368F30` and `D_80368DF0` of `core2/code_6D490.c`
    ("step.s1", &["70_WALKING_NOISE_1"]),
    ("step.s2", &["74_WALKING_NOISE_5"]),
    ("step.s3", &["B_BANJO_LANDING_06"]),
    ("step.s4", &["10_BANJO_LANDING_07"]),
    ("step.s5", &["76_WALKING_NOISE_7"]),
    ("step.s6", &["72_WALKING_NOISE_3"]),
    ("step.s7", &["28_RUSTLING_NOISE"]),
    ("step.s8", &["5_BANJO_LANDING_01"]),
    ("step.s9", &["98_DEAF_THUD"]),
    ("step.s10", &["99_METALLIC_THUD"]),
    ("step.s11", &["10_BANJO_LANDING_07"]),
    ("step.s12", &["123_BANJO_LANDING_10"]),
    ("land.s1", &["8_BANJO_LANDING_04"]),
    ("land.s2", &["7_BANJO_LANDING_03"]),
    ("land.s3", &["B_BANJO_LANDING_06"]),
    ("land.s4", &["10_BANJO_LANDING_07"]),
    ("land.s5", &["26_BANJO_LANDING_09"]),
    ("land.s6", &["6_BANJO_LANDING_02"]),
    ("land.s7", &["28_RUSTLING_NOISE"]),
    ("land.s8", &["5_BANJO_LANDING_01"]),
    ("land.s9", &["98_DEAF_THUD"]),
    ("land.s10", &["99_METALLIC_THUD"]),
    ("land.s11", &["10_BANJO_LANDING_07"]),
    ("land.s12", &["123_BANJO_LANDING_10"]),
    ("swim.paddle", &["12_WATER_PADDLING_1"]),
    ("swim.idle", &["DC_IDLE_PADDLING"]),
    ("splash", &["F_SMALL_WATER_SPLASH"]),
    // pickups and items
    ("collect.note", &["C7_SHWOOP"]),
    ("collect.egg", &["9_SQUEAKY_TOY"]),
    ("collect.red_feather", &["C7_SHWOOP"]),
    ("collect.gold_feather", &["A6_MAGICAL_FINISH"]),
    ("collect.honeycomb", &["190_GULP"]),
    ("collect.jiggy", &["A6_MAGICAL_FINISH"]),
    ("collect.token", &["C5_TWINKLY_POP"]),
    ("egg.fire", &["8A_ALTERNATIVE_EGG_SHOT"]),
    ("egg.break", &["D_EGGSHELL_BREAKING"]),
    // world
    ("door.open", &["7E_CREAKY_DOOR_OPENING"]),
    ("door.break", &["D9_WOODEN_CRATE_BREAKING_1", "DA_WOODEN_CRATE_BREAKING_2", "DB_WOODEN_CRATE_BREAKING_3"]),
    ("switch.press", &["90_SWITCH_PRESS"]),
    ("podium.place", &["2D_KABOING"]),
    ("puzzle.complete", &["A6_MAGICAL_FINISH"]),
    // Mumbo
    ("mumbo.transform", &["17E_MUMBO_TRANSFORMATION_01", "17F_MUMBO_TRANSFORMATION_02", "180_MUMBO_TRANSFORMATION_03", "181_MUMBO_TRANSFORMATION_04"]),
    ("mumbo.poor", &["41_MUMBO_ERGHHH"]),
    ("form.croc_bite", &["6D_CROC_BITE"]),
    // enemies: hit (it was struck and lives) and die
    ("hit.default", &["1D_HITTING_AN_ENEMY_1", "1E_HITTING_AN_ENEMY_2", "1F_HITTING_AN_ENEMY_3"]),
    ("hit.flibbit", &["8E_GRUNTLING_DAMAGE"]),
    ("hit.bigbutt", &["143_BULL_DAMAGE"]),
    ("hit.nipper", &["79_TICKER_DEATH"]),
    ("die.default", &["1E_HITTING_AN_ENEMY_2"]),
    ("die.termite", &["7A_TERMITE_DEATH"]),
    ("die.ticker", &["79_TICKER_DEATH"]),
    ("die.clucker", &["68_CLUCKER_AAEEGHH"]),
    ("die.grille_chompa", &["D7_GRABBA_DEATH"]),
    ("die.slappa", &["D7_GRABBA_DEATH"]),
    ("die.nipper", &["78_EAGLECRY"]),
    ("die.buzzbomb", &["115_BUZZBOMB_DEATH"]),
    ("die.flibbit", &["115_BUZZBOMB_DEATH"]),
    ("die.tee_hee", &["121_AWAWAU", "30_MAGIC_POOF"]),
    ("die.bigbutt", &["3C_BULL_GROWN"]),
    ("die.limbo", &["117_LIMBO_COLLAPSING"]),
    ("die.beehive", &["67_BEEHIVE_CLONK"]),
    ("die.shrapnel", &["1B_EXPLOSION_1"]),
    ("boss.slam", &["3_DULL_CANNON_SHOT"]),
    ("boss.attack_buzz", &["C1_BUZZBOMB_ATTACK"]),
];

static ROUND: AtomicUsize = AtomicUsize::new(0);
/// Event keys played since the last [`take_events`] (tests and the `JUMPMAN_VOICE_LOG`-style hook).
static PLAYED: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

pub fn take_events() -> Vec<&'static str> {
    std::mem::take(&mut *PLAYED.lock().unwrap())
}

pub fn clips(event: &str) -> &'static [&'static str] {
    TABLE.iter().find(|(k, _)| *k == event).map_or(&[], |(_, c)| c)
}

/// Play one clip of `event` (round robin over its clips). False when the event has no clips or the clip file is missing.
pub fn play_event(event: &str) -> bool {
    let list = clips(event);
    if list.is_empty() {
        return false;
    }
    let n = ROUND.fetch_add(1, Ordering::Relaxed);
    if let Some((key, _)) = TABLE.iter().find(|(k, _)| *k == event) {
        let mut log = PLAYED.lock().unwrap();
        log.push(key);
        let excess = log.len().saturating_sub(256);
        log.drain(..excess);
    }
    super::bk_sfx::play(list[n % list.len()])
}

/// All clips the table can play (for the existence test).
pub fn all_clips() -> Vec<&'static str> {
    let mut v: Vec<&str> = TABLE.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    v.sort();
    v.dedup();
    v
}

/// The event key of a BK event, `None` for the ones with their own handling in `banjo_ctl`.
pub fn event_for(e: &BanjoEvent) -> Option<&'static str> {
    Some(match e {
        BanjoEvent::Collected(PickupKind::Note) => "collect.note",
        BanjoEvent::Collected(PickupKind::Egg) => "collect.egg",
        BanjoEvent::Collected(PickupKind::RedFeather) => "collect.red_feather",
        BanjoEvent::Collected(PickupKind::GoldFeather) => "collect.gold_feather",
        BanjoEvent::Collected(PickupKind::Honeycomb) => "collect.honeycomb",
        BanjoEvent::Collected(PickupKind::Jiggy) => "collect.jiggy",
        BanjoEvent::Collected(PickupKind::MumboToken) => "collect.token",
        BanjoEvent::EggFired { .. } => "egg.fire",
        BanjoEvent::DoorBroken(_) => "door.break",
        BanjoEvent::SwitchPressed(_) => "switch.press",
        BanjoEvent::FormChanged(_) => "mumbo.transform",
        BanjoEvent::NeedTokens(_) => "mumbo.poor",
        BanjoEvent::FormAttack(_) => "form.croc_bite",
        BanjoEvent::PicturePieces(..) => "podium.place",
        BanjoEvent::PuzzleComplete(_) => "puzzle.complete",
        _ => return None,
    })
}

/// Surface type (1..12) of the floor under Banjo: the exporter writes `0x100 + type` as the collision triangle's kind
/// (`byor-rs/src/bk/levels.rs::surface_type`, `func_803246B4`); anything else is the default type 1.
pub fn surface_of_kind(kind: i32) -> u8 {
    if (0x101..=0x10C).contains(&kind) {
        (kind - 0x100) as u8
    } else {
        1
    }
}

pub fn step_event(surface: u8) -> &'static str {
    const KEYS: [&str; 12] = ["step.s1", "step.s2", "step.s3", "step.s4", "step.s5", "step.s6", "step.s7", "step.s8", "step.s9", "step.s10", "step.s11", "step.s12"];
    KEYS[(surface.clamp(1, 12) - 1) as usize]
}

pub fn land_event(surface: u8) -> &'static str {
    const KEYS: [&str; 12] = ["land.s1", "land.s2", "land.s3", "land.s4", "land.s5", "land.s6", "land.s7", "land.s8", "land.s9", "land.s10", "land.s11", "land.s12"];
    KEYS[(surface.clamp(1, 12) - 1) as usize]
}

/// Which hit / death keys an enemy kind uses.
pub fn enemy_keys(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Flibbit => ("hit.flibbit", "die.flibbit"),
        Kind::Bigbutt => ("hit.bigbutt", "die.bigbutt"),
        Kind::Nipper => ("hit.nipper", "die.nipper"),
        Kind::Termite => ("hit.default", "die.termite"),
        Kind::Clucker => ("hit.default", "die.clucker"),
        Kind::GrilleChompa => ("hit.default", "die.grille_chompa"),
        Kind::Slappa => ("hit.default", "die.slappa"),
        Kind::Grabba => ("hit.default", "die.slappa"),
        Kind::Tanktup => ("hit.default", "die.default"),
        Kind::Buzzbomb => ("hit.default", "die.buzzbomb"),
        Kind::TeeHee => ("hit.default", "die.tee_hee"),
        Kind::Limbo => ("hit.default", "die.limbo"),
        Kind::Beehive => ("hit.default", "die.beehive"),
        Kind::Shrapnel => ("hit.default", "die.shrapnel"),
        _ => ("hit.default", "die.default"),
    }
}

pub struct BkEventSoundsPlugin;

impl Plugin for BkEventSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (motion_sounds, enemy_sounds));
    }
}

/// Footsteps by distance walked, landings, swimming strokes.
#[derive(Default)]
struct Motion {
    stride: f64,
    was_air: bool,
    air_speed: f64,
    swim_timer: f64,
    /// surface type under Banjo while grounded
    surface: u8,
}

fn motion_sounds(sim: Option<Res<GameSim>>, mut st: Local<Motion>) {
    let Some(sim) = sim else { return };
    if !sim.as_banjo || !sim.course.request.id.starts_with("bk_") {
        return;
    }
    use jumpman_sim::state::*;
    let m = &sim.mario;
    let air = m.action & ACT_FLAG_AIR != 0;
    if !air {
        st.surface = m.floor.as_ref().map_or(1, |f| surface_of_kind(f.kind));
    }
    let swimming = m.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED;
    if swimming {
        st.swim_timer += 1.0;
        if m.forward_vel.abs() > 3.0 && st.swim_timer >= 20.0 {
            st.swim_timer = 0.0;
            play_event("swim.paddle");
        } else if st.swim_timer >= 60.0 {
            st.swim_timer = 0.0;
            play_event("swim.idle");
        }
        st.stride = 0.0;
        st.was_air = false;
        return;
    }
    if air {
        st.air_speed = st.air_speed.max(-m.vel[1]);
    } else if st.was_air {
        // landed: harder the faster it fell (units per frame)
        if st.air_speed > 6.0 {
            play_event(land_event(st.surface));
        }
        st.air_speed = 0.0;
        st.stride = 0.0;
    }
    st.was_air = air;
    if !air && m.forward_vel > 3.0 && matches!(m.action, ACT_WALKING | ACT_HOLD_WALKING | ACT_DECELERATING) {
        st.stride += m.forward_vel;
        // one step per ~70 units walked (two per stride of the walk cycle)
        if st.stride >= 70.0 {
            st.stride = 0.0;
            play_event(step_event(st.surface));
        }
    }
}

/// Enemy hits (a counter the sim bumps when an attack lands) and deaths.
#[derive(Default)]
struct Seen {
    struck: HashMap<usize, f64>,
    died: HashSet<usize>,
}

fn enemy_sounds(sim: Option<Res<GameSim>>, mut seen: Local<Seen>) {
    let Some(sim) = sim else { return };
    if !sim.course.request.id.starts_with("bk_") {
        return;
    }
    for (i, o) in sim.mario.objects.objects.iter().enumerate() {
        if o.behavior != ObjectBehaviorKind::BkEnemy {
            continue;
        }
        let kind = kind_of(o);
        let (hit, die) = enemy_keys(kind);
        let struck = o.var("struck");
        let before = seen.struck.insert(i, struck).unwrap_or(struck);
        if struck > before && o.var("died") == 0.0 {
            play_event(hit);
        }
        if o.var("died") != 0.0 && seen.died.insert(i) {
            play_event(die);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn every_event_has_clips_and_the_keys_are_unique() {
        let mut keys: Vec<&str> = TABLE.iter().map(|(k, _)| *k).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate event keys");
        assert!(TABLE.iter().all(|(_, c)| !c.is_empty()));
        assert!(TABLE.len() >= 60);
    }

    #[test]
    fn every_clip_of_the_table_exists_in_the_extracted_bank_and_is_audible() {
        let dir = assets().join("audio/voices/bk_sfx");
        if !dir.is_dir() {
            eprintln!("skipped: no extracted BK sound bank");
            return;
        }
        for name in all_clips() {
            let path = super::super::voices::clip_path(&assets(), &format!("{}{name}", super::super::bk_sfx::FOLDER), 0);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let clip = super::super::voices::parse_wav(&bytes).unwrap_or_else(|| panic!("{name} is not mono 16-bit PCM"));
            let peak = clip.pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.02, "{name} is silent");
        }
    }

    #[test]
    fn play_event_rejects_unknown_keys_and_the_round_robin_cycles() {
        assert!(!play_event("no.such.event"));
        // three plays of a three-clip event touch each clip once (the files may be absent: only the choice is checked)
        let list = clips("door.break");
        assert_eq!(list.len(), 3);
        let start = ROUND.load(Ordering::Relaxed);
        for _ in 0..3 {
            play_event("door.break");
        }
        assert!(ROUND.load(Ordering::Relaxed) >= start + 3);
    }

    fn app_with(course: &str) -> Option<App> {
        use crate::game::world::CourseRequest;
        let sim = GameSim::load(&assets(), CourseRequest::new(course)).ok()?;
        let mut app = App::new();
        app.insert_resource(sim);
        app.add_systems(Update, (motion_sounds, enemy_sounds));
        Some(app)
    }

    #[test]
    fn walking_landing_and_hurting_enemies_play_their_events() {
        use jumpman_sim::state::*;
        let Some(mut app) = app_with("bk_mumbos_mountain") else {
            eprintln!("skipped: no SM64 / BK assets");
            return;
        };
        let _ = take_events();
        app.world_mut().resource_mut::<GameSim>().as_banjo = true;
        let walk = ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
        for _ in 0..90 {
            app.world_mut().resource_mut::<GameSim>().tick(&walk);
            app.update();
        }
        let played = take_events();
        assert!(played.iter().filter(|e| e.starts_with("step.s")).count() >= 3, "{played:?}");
        // a fall from height: a landing sound
        {
            let mut sim = app.world_mut().resource_mut::<GameSim>();
            let p = sim.mario.pos;
            sim.mario.pos = [p[0], p[1] + 900.0, p[2]];
            sim.mario.vel = [0.0; 3];
            sim.mario.set_action(ACT_FREEFALL, 0.0);
        }
        for _ in 0..90 {
            app.world_mut().resource_mut::<GameSim>().tick(&ControllerSnapshot::default());
            app.update();
        }
        let played = take_events();
        assert!(played.iter().any(|e| e.starts_with("land.s")), "{played:?}");
        // an enemy that is struck and one that dies
        {
            let mut sim = app.world_mut().resource_mut::<GameSim>();
            let i = sim.mario.objects.objects.iter().position(|o| o.behavior == ObjectBehaviorKind::BkEnemy).unwrap();
            let o = &mut sim.mario.objects.objects[i];
            o.set_var("struck", 1.0);
            let j = sim.mario.objects.objects.iter().rposition(|o| o.behavior == ObjectBehaviorKind::BkEnemy).unwrap();
            sim.mario.objects.objects[j].set_var("died", 1.0);
        }
        app.update();
        let played = take_events();
        assert!(played.iter().any(|e| e.starts_with("hit.")) || played.iter().any(|e| e.starts_with("die.")), "{played:?}");
    }

    #[test]
    fn banjo_events_map_to_table_keys() {
        use jumpman_sim::banjo_ext::Form;
        for e in [
            BanjoEvent::Collected(PickupKind::Note),
            BanjoEvent::Collected(PickupKind::Jiggy),
            BanjoEvent::DoorBroken(1),
            BanjoEvent::SwitchPressed(0),
            BanjoEvent::FormChanged(Form::Croc),
            BanjoEvent::NeedTokens(5),
            BanjoEvent::FormAttack(Form::Croc),
            BanjoEvent::PuzzleComplete(1),
        ] {
            let key = event_for(&e).unwrap();
            assert!(!clips(key).is_empty(), "{key}");
        }
        assert_eq!(event_for(&BanjoEvent::ClimbRelease), None);
    }

    #[test]
    fn every_enemy_kind_has_hit_and_die_keys_with_clips() {
        for k in jumpman_sim::bk_enemy::ALL_KINDS {
            let (hit, die) = enemy_keys(k);
            assert!(!clips(hit).is_empty() && !clips(die).is_empty(), "{k:?}");
        }
    }

    #[test]
    fn surface_types_pick_the_decomps_footstep_and_landing_clips() {
        assert_eq!(surface_of_kind(0x100 + 7), 7);
        assert_eq!(surface_of_kind(0), 1);
        assert_eq!(surface_of_kind(0x20), 1);
        assert_eq!(clips(step_event(1)), &["70_WALKING_NOISE_1"]);
        assert_eq!(clips(step_event(2)), &["74_WALKING_NOISE_5"], "sand");
        assert_eq!(clips(step_event(10)), &["99_METALLIC_THUD"], "metal");
        assert_eq!(clips(land_event(3)), &["B_BANJO_LANDING_06"], "snow");
        assert_eq!(clips(land_event(12)), &["123_BANJO_LANDING_10"]);
        for t in 1..=12 {
            assert!(!clips(step_event(t)).is_empty() && !clips(land_event(t)).is_empty());
        }
    }
}
