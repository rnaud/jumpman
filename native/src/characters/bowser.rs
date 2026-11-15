//! Bowser as a playable character: a skin on Mario's controller and physics. The sim keeps playing Mario
//! (`GameSim::{as_link, as_banjo}` stay off, so every course, enemy, star and mode rule sees Mario's moves); only the
//! model changes. The model is SM64's own Bowser (`actors/bowser/bowser_geo.glb`, from the user's ROM import, never
//! bundled). It is an unskinned node hierarchy with the 27 clips of the decomp's `bowser_seg6_anims_06057690` table
//! (`BOWSER_ANIM_*`, in address order: [`glb_clip`] maps a table index to the glb's), so it is driven by the object views'
//! [`AnimRig`] in seek mode: this module picks the clip from Mario's action (the table below) and samples it on its own clock.
//!
//! Mario action -> Bowser clip (decomp `BOWSER_ANIM_*` table index; the glb index is [`glb_clip`] of it):
//!
//! | Mario actions                                                   | Bowser clip                  | playback               |
//! |-----------------------------------------------------------------|------------------------------|------------------------|
//! | idle, `*_LAND_STOP`, panting, reading, waiting, holding idle    | 12 `IDLE`                    | loop                   |
//! | walking / decelerating / holding walk, slower than [`RUN_FROM`] | 13 `SLOW_GAIT`               | loop, rate from speed  |
//! | walking / decelerating / holding walk, faster                   | 19 `RUN`                     | loop, rate from speed  |
//! | turning around, braking, butt / stomach slides                  | 21 `RUN_SLIP`                | loop                   |
//! | crouching, crawling, crouch slide                               | 14 `LOOK_DOWN_STOP_WALK`     | once, holds            |
//! | jumps and every airborne move (rise and fall), ground pound     | 9 `JUMP_START`               | once, holds the air pose |
//! | landings (`*_LAND`, ground-pound land)                          | 8 `JUMP_STOP`                | once                   |
//! | punch, second punch, kick, jump kick, throws (B)                | 22 `BREATH_QUICK` (+ flames) | once per hit           |
//! | dive, slide kick, dive / stomach slide                          | 16 `FLIP_DOWN`               | once, holds            |
//! | knockback, bonk, shock, burn, squish, blown away                | 2 `SHAKING`                  | loop                   |
//! | deaths, sleeping                                                | 17 `LAY_DOWN`                | once, holds            |
//! | waking up                                                       | 0 `STAND_UP`                 | once                   |
//! | star dances, grand star                                         | 10 `DANCE`                   | loop                   |
//! | swimming                                                        | 13 `SLOW_GAIT`               | loop, slow             |
//! | ledge grab / hang / pole                                        | 23 `EDGE_MOVE`               | loop                   |
//! | ledge climbs                                                    | 24 `EDGE_STOP`               | once                   |
//! | flying, shot from cannon                                        | 25 `FLIP`                    | once, holds            |
//! | anything else                                                   | 12 `IDLE`                    | loop                   |
//!
//! Size: drawn [`MODEL_SCALE`] of the boss (about 1.7 x Mario's height); his collision and hitbox stay Mario's, so every
//! course still fits him. The follow camera stands [`CAMERA_DISTANCE`] times further back. Voice: the `bowser` set of
//! `character-sounds.json` (SM64 sound-bank effects: laugh, fire, defeat). Missing glb: he is left out of the lineup and the
//! V cycle, and selecting him anyway (env) toasts and draws nothing.
use super::Character;
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::render::object_view::{spawn_model, AnimRig};
use crate::render::{to_bevy, AssetRoot};
use bevy::prelude::*;
use jumpman_sim::behaviors::bowser_sm64::ANIM_LOOP_END;
use jumpman_sim::math::to_s16;
use jumpman_sim::state::*;
use std::path::Path;

/// Bowser's model in the SM64 export (the one the boss fights use).
pub const MODEL: &str = "actors/bowser/bowser_geo.glb";
/// The playable Bowser is drawn at this fraction of the boss's size.
pub const MODEL_SCALE: f32 = 0.52;
/// Follow-camera distance factor while he plays.
pub const CAMERA_DISTANCE: f64 = 1.3;
/// Forward speed (SM64 units per frame) from which he runs instead of lumbering.
pub const RUN_FROM: f64 = 9.0;

/// `BOWSER_ANIM_*` (decomp `bowser.h`), the clip order of `bowser_seg6_anims_06057690` and of the glb.
pub mod anim {
    pub const STAND_UP: usize = 0;
    pub const SHAKING: usize = 2;
    pub const JUMP_STOP: usize = 8;
    pub const JUMP_START: usize = 9;
    pub const DANCE: usize = 10;
    pub const IDLE: usize = 12;
    pub const SLOW_GAIT: usize = 13;
    pub const LOOK_DOWN_STOP_WALK: usize = 14;
    pub const FLIP_DOWN: usize = 16;
    pub const LAY_DOWN: usize = 17;
    pub const RUN: usize = 19;
    pub const RUN_SLIP: usize = 21;
    pub const BREATH_QUICK: usize = 22;
    pub const EDGE_MOVE: usize = 23;
    pub const EDGE_STOP: usize = 24;
    pub const FLIP: usize = 25;
    /// Every clip the playable Bowser uses (the tests check each one exists in the glb).
    #[cfg(test)]
    pub const USED: [usize; 16] = [STAND_UP, SHAKING, JUMP_STOP, JUMP_START, DANCE, IDLE, SLOW_GAIT, LOOK_DOWN_STOP_WALK, FLIP_DOWN, LAY_DOWN, RUN, RUN_SLIP, BREATH_QUICK, EDGE_MOVE, EDGE_STOP, FLIP];
}

/// The glb's clip for a `BOWSER_ANIM_*` index. The exporter writes the clips sorted by address, which is the decomp table's
/// order except for the run clips: the table lists `RUN_START` (`..._06054290`) before `RUN`, `RUN_STOP` and `RUN_SLIP`
/// (`..._06052D94`, `..._060534F4`, `..._06053B8C`), the glb after them.
pub fn glb_clip(anim: usize) -> usize {
    match anim {
        18 => 21,
        19 => 18,
        20 => 19,
        21 => 20,
        other => other,
    }
}

/// Whether the installed data has Bowser's model.
pub fn installed(root: &Path) -> bool {
    root.join(MODEL).is_file()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Play {
    Loop,
    /// Plays once and holds the last frame.
    Once,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BowserClip {
    pub anim: usize,
    pub play: Play,
    /// Playback rate (1 = the decomp's 30 frames a second).
    pub speed: f32,
    /// Restart the clip when this changes within the same action (Mario's punch sequence stages).
    pub stage: u8,
}

const fn lp(anim: usize, speed: f32) -> BowserClip {
    BowserClip { anim, play: Play::Loop, speed, stage: 0 }
}
const fn once(anim: usize, speed: f32) -> BowserClip {
    BowserClip { anim, play: Play::Once, speed, stage: 0 }
}

/// Length of a clip in seconds (its `loopEnd`: the glb holds frames 0..loopEnd-1).
pub fn clip_seconds(anim: usize) -> f32 {
    (ANIM_LOOP_END.get(anim).copied().unwrap_or(2).max(2) - 1) as f32 / 30.0
}

/// The sample time of a clip `t` seconds after it started.
pub fn sample_time(clip: &BowserClip, t: f32) -> f32 {
    let len = clip_seconds(clip.anim);
    let t = t.max(0.0) * clip.speed;
    match clip.play {
        Play::Loop => t.rem_euclid(len + 1.0 / 30.0).min(len),
        Play::Once => t.min(len),
    }
}

/// Mario's punch sequence stage from `actionArg` (`mario_update_punch_sequence`: 0-2 first punch, 3-5 second punch, 6+ kick).
fn punch_stage(arg: f64) -> u8 {
    if arg < 3.0 {
        0
    } else if arg < 6.0 {
        1
    } else {
        2
    }
}

/// The clip for one frame of Mario's state (the module table).
pub fn clip_for(action: u32, action_arg: f64, forward_vel: f64) -> BowserClip {
    use anim::*;
    let speed = forward_vel.abs();
    match action {
        ACT_IDLE | ACT_BRAKING_STOP | ACT_BUTT_SLIDE_STOP | ACT_SLIDE_KICK_SLIDE_STOP | ACT_JUMP_LAND_STOP | ACT_DOUBLE_JUMP_LAND_STOP | ACT_TRIPLE_JUMP_LAND_STOP
        | ACT_FREEFALL_LAND_STOP | ACT_SIDE_FLIP_LAND_STOP | ACT_LONG_JUMP_LAND_STOP | ACT_BACKFLIP_LAND_STOP | ACT_PANTING | ACT_STANDING_AGAINST_WALL | ACT_COUGHING
        | ACT_SHIVERING | ACT_READING_SIGN | ACT_READING_NPC_DIALOG | ACT_READING_AUTOMATIC_DIALOG | ACT_WAITING_FOR_DIALOG | ACT_HOLD_IDLE | ACT_HOLD_JUMP_LAND_STOP
        | ACT_HOLD_FREEFALL_LAND_STOP | ACT_HOLD_BUTT_SLIDE_STOP | ACT_METAL_WATER_STANDING | ACT_PUTTING_ON_CAP | ACT_IN_QUICKSAND | ACT_TOP_OF_POLE => lp(IDLE, 1.0),
        ACT_WALKING | ACT_HOLD_WALKING | ACT_DECELERATING | ACT_HOLD_DECELERATING | ACT_METAL_WATER_WALKING | ACT_RIDING_SHELL_GROUND => {
            if speed >= RUN_FROM {
                // The boss charges at 50 units a frame with this clip; drawn at MODEL_SCALE his stride covers proportionally less.
                lp(RUN, (speed / (50.0 * MODEL_SCALE as f64)).clamp(0.55, 1.7) as f32)
            } else {
                lp(SLOW_GAIT, (0.6 + speed / 5.0).clamp(0.6, 2.2) as f32)
            }
        }
        ACT_TURNING_AROUND | ACT_FINISH_TURNING_AROUND | ACT_BRAKING | ACT_BEGIN_SLIDING | ACT_BUTT_SLIDE | ACT_HOLD_BEGIN_SLIDING | ACT_HOLD_BUTT_SLIDE | ACT_BUTT_SLIDE_AIR
        | ACT_HOLD_BUTT_SLIDE_AIR | ACT_CROUCH_SLIDE => lp(RUN_SLIP, 1.0),
        ACT_CROUCHING | ACT_START_CROUCHING | ACT_STOP_CROUCHING | ACT_CRAWLING | ACT_START_CRAWLING | ACT_STOP_CRAWLING => once(LOOK_DOWN_STOP_WALK, 1.6),
        ACT_JUMP_LAND | ACT_FREEFALL_LAND | ACT_DOUBLE_JUMP_LAND | ACT_SIDE_FLIP_LAND | ACT_TRIPLE_JUMP_LAND | ACT_LONG_JUMP_LAND | ACT_BACKFLIP_LAND | ACT_GROUND_POUND_LAND
        | ACT_HOLD_JUMP_LAND | ACT_HOLD_FREEFALL_LAND | ACT_AIR_THROW_LAND | ACT_LAVA_BOOST_LAND | ACT_DEATH_EXIT_LAND | ACT_SPAWN_NO_SPIN_LANDING | ACT_SPAWN_SPIN_LANDING => once(JUMP_STOP, 1.4),
        ACT_PUNCHING | ACT_MOVE_PUNCHING => BowserClip { stage: punch_stage(action_arg), ..once(BREATH_QUICK, 1.5) },
        ACT_JUMP_KICK | ACT_THROWING | ACT_AIR_THROW | ACT_PICKING_UP | ACT_DIVE_PICKING_UP | ACT_PLACING_DOWN | ACT_PICKING_UP_BOWSER | ACT_RELEASING_BOWSER => once(BREATH_QUICK, 1.5),
        ACT_DIVE | ACT_DIVE_SLIDE | ACT_STOMACH_SLIDE | ACT_HOLD_STOMACH_SLIDE | ACT_SLIDE_KICK | ACT_SLIDE_KICK_SLIDE => once(FLIP_DOWN, 1.2),
        ACT_HARD_BACKWARD_GROUND_KB | ACT_HARD_FORWARD_GROUND_KB | ACT_SOFT_BACKWARD_GROUND_KB | ACT_SOFT_FORWARD_GROUND_KB | ACT_BACKWARD_GROUND_KB | ACT_FORWARD_GROUND_KB
        | ACT_BACKWARD_AIR_KB | ACT_FORWARD_AIR_KB | ACT_HARD_FORWARD_AIR_KB | ACT_HARD_BACKWARD_AIR_KB | ACT_SHOCKED | ACT_ELECTROCUTION | ACT_BURNING_GROUND | ACT_BURNING_JUMP
        | ACT_BURNING_FALL | ACT_LAVA_BOOST | ACT_SQUISHED | ACT_GETTING_BLOWN | ACT_GRABBED | ACT_HEAD_STUCK_IN_GROUND | ACT_BUTT_STUCK_IN_GROUND | ACT_FEET_STUCK_IN_GROUND => {
            lp(SHAKING, 1.0)
        }
        ACT_STANDING_DEATH | ACT_DEATH_ON_STOMACH | ACT_DEATH_ON_BACK | ACT_QUICKSAND_DEATH | ACT_DROWNING | ACT_WATER_DEATH | ACT_SUFFOCATION | ACT_EATEN_BY_BUBBA
        | ACT_START_SLEEPING | ACT_SLEEPING => once(LAY_DOWN, 1.0),
        ACT_WAKING_UP => once(STAND_UP, 1.4),
        ACT_STAR_DANCE_EXIT | ACT_STAR_DANCE_NO_EXIT | ACT_STAR_DANCE_WATER | ACT_JUMBO_STAR_CUTSCENE | ACT_FALL_AFTER_STAR_GRAB => lp(DANCE, 1.0),
        ACT_WATER_IDLE | ACT_BREASTSTROKE | ACT_FLUTTER_KICK | ACT_SWIMMING_END | ACT_WATER_ACTION_END | ACT_WATER_PLUNGE | ACT_METAL_WATER_FALLING | ACT_METAL_WATER_JUMP => {
            lp(SLOW_GAIT, 0.7)
        }
        ACT_LEDGE_GRAB | ACT_START_HANGING | ACT_HANGING | ACT_HANG_MOVING | ACT_HOLDING_POLE | ACT_GRAB_POLE_SLOW | ACT_GRAB_POLE_FAST | ACT_CLIMBING_POLE
        | ACT_TOP_OF_POLE_TRANSITION => lp(EDGE_MOVE, 1.0),
        ACT_LEDGE_CLIMB_SLOW_1 | ACT_LEDGE_CLIMB_SLOW_2 | ACT_LEDGE_CLIMB_DOWN | ACT_LEDGE_CLIMB_FAST => once(EDGE_STOP, 1.3),
        ACT_FLYING | ACT_SHOT_FROM_CANNON => once(FLIP, 1.0),
        a if a & ACT_FLAG_AIR != 0 => once(JUMP_START, 1.6),
        _ => lp(IDLE, 1.0),
    }
}

/// `JUMPMAN_BOWSER_CLIP=<index>`: hold one clip (looped) whatever Mario does, to look at the clips headless.
fn forced_clip() -> Option<usize> {
    std::env::var("JUMPMAN_BOWSER_CLIP").ok().and_then(|v| v.parse().ok()).filter(|i| *i < ANIM_LOOP_END.len())
}

// ---------------------------------------------------------------------------------------------
// Bevy glue
// ---------------------------------------------------------------------------------------------

pub struct BowserPlugin;

impl Plugin for BowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BowserView>()
            .add_systems(Update, (spawn_bowser, drive_bowser).chain().after(super::update_rigs).before(crate::CameraSet));
    }
}

#[derive(Component)]
pub struct BowserRoot;
#[derive(Component)]
struct BowserBody;

/// The playing clock of the clip on screen.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ClipClock {
    key: Option<(usize, u32, u8)>,
    pub t: f32,
}

impl ClipClock {
    /// Advance by `dt`; a new clip, or a one-shot clip whose action or stage changed, starts over. Returns the sample time.
    pub fn step(&mut self, clip: &BowserClip, action: u32, dt: f32) -> f32 {
        let key = (clip.anim, if clip.play == Play::Once { action } else { 0 }, clip.stage);
        if self.key != Some(key) {
            self.key = Some(key);
            self.t = 0.0;
        } else {
            self.t += dt;
        }
        sample_time(clip, self.t)
    }
    /// Whether the clip just (re)started this frame.
    pub fn started(&self) -> bool {
        self.t == 0.0
    }
}

#[derive(Resource, Default)]
pub struct BowserView {
    root: Option<Entity>,
    failed: bool,
    clock: ClipClock,
    /// Seconds of fire breath left (a jet of flame particles from his mouth on each punch).
    flame: f32,
}

fn spawn_bowser(mut commands: Commands, character: Res<Character>, root: Option<Res<AssetRoot>>, assets: Option<Res<AssetServer>>, mut view: ResMut<BowserView>, mut toast: ResMut<Toast>) {
    if *character != Character::Bowser || view.root.is_some() || view.failed {
        return;
    }
    let (Some(root), Some(assets)) = (root, assets) else { return };
    if !installed(&root.0) {
        warn!("Bowser's model is missing: {}", root.0.join(MODEL).display());
        toast.show("BOWSER IS NOT INSTALLED - IMPORT SUPER MARIO 64", 3.2);
        view.failed = true;
        return;
    }
    let root_e = commands.spawn((Transform::default(), Visibility::Hidden, super::CharacterRoot(Character::Bowser), BowserRoot)).id();
    let body = spawn_model(&mut commands, &assets, &root, MODEL, false).insert(BowserBody).id();
    commands.entity(root_e).add_child(body);
    view.root = Some(root_e);
}

#[allow(clippy::too_many_arguments)]
fn drive_bowser(
    character: Res<Character>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut view: ResMut<BowserView>,
    mut roots: Query<(&mut Transform, &mut Visibility), With<BowserRoot>>,
    mut rigs: Query<&mut AnimRig, With<BowserBody>>,
    particles: Option<ResMut<crate::fx::particles::Particles>>,
    audio: Option<ResMut<crate::audio::GameAudio>>,
) {
    let Some(root_e) = view.root else { return };
    let Ok((mut tf, mut vis)) = roots.get_mut(root_e) else { return };
    let show = *character == Character::Bowser;
    vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
    if !show {
        return;
    }
    let m = &sim.mario;
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let mut render_yaw = yaw;
    if matches!(m.action, ACT_SPECIAL_EXIT_AIRBORNE | ACT_EXIT_AIRBORNE | ACT_FALLING_EXIT_AIRBORNE | ACT_EXIT_LAND_SAVE_DIALOG) {
        render_yaw = to_s16(render_yaw + 32768.0) as f64;
    }
    tf.translation = to_bevy(pos);
    tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(render_yaw) as f32);
    // Hidden like Mario's model: cutscenes that clear GRAPH_RENDER_ACTIVE, and the blink after a hit.
    let hidden = !m.cut.mario_visible || crate::render::mario_view::blinks_out(m.invinc_timer, sim.ticks as u64);
    tf.scale = if hidden {
        Vec3::splat(0.0001)
    } else if m.squish_timer > 0.0 {
        Vec3::new(1.2, 0.65, 1.2) * MODEL_SCALE
    } else {
        Vec3::splat(MODEL_SCALE)
    };
    let dt = time.delta_secs().min(0.1);
    let mut clip = clip_for(m.action, m.action_arg, m.forward_vel);
    if let Some(forced) = forced_clip() {
        clip = lp(forced, 1.0);
    }
    let sample = view.clock.step(&clip, m.action, dt);
    if clip.anim == anim::BREATH_QUICK && view.clock.started() {
        view.flame = 0.32;
        // Mario's punch grunts are Mario-only (`mario_sounds`); Bowser spits fire (SM64 sound bank).
        if let Some(mut audio) = audio {
            audio.sfx.named("AIR_BOWSER_SPIT_FIRE", None);
        }
    }
    for mut rig in &mut rigs {
        rig.want = glb_clip(clip.anim);
        rig.seek = Some(sample);
    }
    // Fire breath: a short jet of flame particles out of his mouth (visual only: the hit is Mario's punch).
    if view.flame > 0.0 {
        view.flame -= dt;
        if let Some(mut p) = particles {
            let fwd = Vec3::new(tf.rotation.mul_vec3(Vec3::Z).x, 0.0, tf.rotation.mul_vec3(Vec3::Z).z).normalize_or_zero();
            let mouth = [pos[0] + fwd.x as f64 * 120.0, pos[1] + 150.0, pos[2] + fwd.z as f64 * 120.0];
            p.flame_jet(mouth, [fwd.x, 0.0, fwd.z]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// One representative action of every Mario action group, with the clip it must play.
    const GROUPS: [(u32, usize); 19] = [
        (ACT_IDLE, anim::IDLE),
        (ACT_JUMP_LAND_STOP, anim::IDLE),
        (ACT_TURNING_AROUND, anim::RUN_SLIP),
        (ACT_CROUCHING, anim::LOOK_DOWN_STOP_WALK),
        (ACT_JUMP, anim::JUMP_START),
        (ACT_TRIPLE_JUMP, anim::JUMP_START),
        (ACT_FREEFALL, anim::JUMP_START),
        (ACT_GROUND_POUND, anim::JUMP_START),
        (ACT_JUMP_LAND, anim::JUMP_STOP),
        (ACT_PUNCHING, anim::BREATH_QUICK),
        (ACT_JUMP_KICK, anim::BREATH_QUICK),
        (ACT_DIVE, anim::FLIP_DOWN),
        (ACT_HARD_BACKWARD_GROUND_KB, anim::SHAKING),
        (ACT_DEATH_ON_BACK, anim::LAY_DOWN),
        (ACT_STAR_DANCE_EXIT, anim::DANCE),
        (ACT_BREASTSTROKE, anim::SLOW_GAIT),
        (ACT_LEDGE_GRAB, anim::EDGE_MOVE),
        (ACT_LEDGE_CLIMB_FAST, anim::EDGE_STOP),
        (ACT_FLYING, anim::FLIP),
    ];

    #[test]
    fn every_action_group_maps_to_its_clip() {
        for (action, want) in GROUPS {
            assert_eq!(clip_for(action, 0.0, 0.0).anim, want, "action {action:#x}");
        }
        assert_eq!(clip_for(ACT_WALKING, 0.0, 4.0).anim, anim::SLOW_GAIT);
        assert_eq!(clip_for(ACT_WALKING, 0.0, 30.0).anim, anim::RUN);
        assert!(clip_for(ACT_WALKING, 0.0, 32.0).speed > clip_for(ACT_WALKING, 0.0, 12.0).speed, "runs faster with speed");
        // Every airborne action, mapped or not, has an air pose; nothing ends up on an unknown clip.
        assert_eq!(clip_for(ACT_WALL_KICK_AIR, 0.0, 0.0).anim, anim::JUMP_START);
        assert_eq!(clip_for(0xdead_beef & !ACT_FLAG_AIR, 0.0, 0.0).anim, anim::IDLE);
        for a in 0..0x400u32 {
            for action in [a, a | ACT_FLAG_AIR, a | ACT_FLAG_MOVING] {
                assert!(anim::USED.contains(&clip_for(action, 0.0, 10.0).anim));
            }
        }
    }

    #[test]
    fn punches_restart_per_stage_and_loops_wrap() {
        let p = |arg| clip_for(ACT_PUNCHING, arg, 0.0);
        assert_eq!((p(0.0).stage, p(2.0).stage, p(4.0).stage, p(7.0).stage), (0, 0, 1, 2));
        let mut clock = ClipClock::default();
        clock.step(&p(1.0), ACT_PUNCHING, 0.0);
        clock.step(&p(1.0), ACT_PUNCHING, 0.2);
        assert!(clock.t > 0.1);
        clock.step(&p(4.0), ACT_PUNCHING, 0.016);
        assert!(clock.started(), "the second punch replays the breath");
        // A loop wraps, a one-shot holds its last frame.
        let idle = clip_for(ACT_IDLE, 0.0, 0.0);
        let len = clip_seconds(anim::IDLE);
        assert!(sample_time(&idle, len + 0.5) < 0.6);
        let land = clip_for(ACT_JUMP_LAND, 0.0, 0.0);
        assert_eq!(sample_time(&land, 100.0), clip_seconds(anim::JUMP_STOP));
    }

    /// The clip map against the real model: the glb's clips are the decomp table in order (lengths match `loopEnd`), and
    /// every clip the map uses exists. Skipped without an SM64 import in the repo's asset folder.
    #[test]
    fn clip_map_matches_the_imported_model() {
        let path = assets().join(MODEL);
        if !path.is_file() {
            eprintln!("skipped: no {}", path.display());
            return;
        }
        let names = crate::render::g1_view::glb_animation_names(&path);
        assert_eq!(names.len(), ANIM_LOOP_END.len(), "{names:?}");
        assert!(names.iter().all(|n| n.starts_with("bowser_seg6_anim_")), "{names:?}");
        let glb = std::fs::read(&path).unwrap();
        let durations = glb_clip_durations(&glb);
        assert_eq!(durations.len(), ANIM_LOOP_END.len());
        // Through `glb_clip` every table entry's length matches its `loopEnd`, and the run clips are the expected addresses.
        for i in 0..ANIM_LOOP_END.len() {
            let d = durations[glb_clip(i)];
            assert!((d - clip_seconds(i)).abs() < 0.02, "anim {i} (glb {}) lasts {d} s, loopEnd says {}", glb_clip(i), clip_seconds(i));
        }
        assert_eq!(names[glb_clip(18)], "bowser_seg6_anim_06054290");
        assert_eq!(names[glb_clip(anim::RUN)], "bowser_seg6_anim_06052D94");
        assert_eq!(names[glb_clip(anim::RUN_SLIP)], "bowser_seg6_anim_06053B8C");
        for (action, want) in GROUPS {
            assert!(clip_for(action, 0.0, 0.0).anim < names.len(), "{action:#x} -> {want}");
        }
        assert!(anim::USED.iter().all(|a| *a < names.len()));
    }

    /// Longest keyframe time of each animation (reads the glb's JSON accessors).
    fn glb_clip_durations(glb: &[u8]) -> Vec<f32> {
        let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&glb[20..20 + len]).unwrap();
        let acc = &json["accessors"];
        json["animations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["samplers"].as_array().unwrap().iter().map(|s| acc[s["input"].as_u64().unwrap() as usize]["max"][0].as_f64().unwrap_or(0.0) as f32).fold(0.0, f32::max))
            .collect()
    }

    #[test]
    fn missing_model_is_not_installed_and_the_cycle_skips_him() {
        let empty = std::env::temp_dir().join("jumpman-no-bowser");
        assert!(!installed(&empty));
        assert_eq!(Character::Banjo.next_available(|_| false), Character::Mario);
        assert_eq!(Character::Banjo.next_available(|c| c == Character::Bowser), Character::Bowser);
        assert_eq!(Character::Bowser.next_available(|c| c == Character::Bowser), Character::Mario);
    }

    /// Selecting him without the model: a toast, no entity, and it is not retried every frame.
    #[test]
    fn missing_model_spawns_nothing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .insert_resource(Character::Bowser)
            .insert_resource(AssetRoot::new(std::env::temp_dir().join("jumpman-no-bowser")))
            .init_resource::<Toast>()
            .init_resource::<BowserView>()
            .add_systems(Update, spawn_bowser);
        app.update();
        app.update();
        let view = app.world().resource::<BowserView>();
        assert!(view.failed && view.root.is_none());
        assert_eq!(app.world_mut().query::<&BowserRoot>().iter(app.world()).count(), 0);
    }

    #[test]
    fn spawns_from_the_imported_model() {
        if !installed(&assets()) {
            eprintln!("skipped: no Bowser model");
            return;
        }
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<AnimationClip>()
            .init_asset::<bevy::gltf::Gltf>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .insert_resource(Character::Bowser)
            .insert_resource(AssetRoot::new(assets()))
            .init_resource::<Toast>()
            .init_resource::<BowserView>()
            .add_systems(Update, spawn_bowser);
        app.update();
        assert_eq!(app.world_mut().query::<(&BowserRoot, &super::super::CharacterRoot)>().iter(app.world()).count(), 1);
        let rig = app.world_mut().query::<&AnimRig>().single(app.world()).map(|r| r.clips.len()).unwrap();
        assert_eq!(rig, ANIM_LOOP_END.len());
    }
}
