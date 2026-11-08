//! SM64 cutscene and special actions that the earlier ports left out (DECOMP_TALLY gap #4):
//! `src/game/mario_actions_cutscene.c` (disappeared, electrocution, suffocation, eaten by Bubba,
//! shocked, squished, stuck in ground x3, putting on cap, warp-door spawn, emerge from pipe,
//! BBH enter jump / spin, intro, credits, end waving, reading automatic dialog, waiting for
//! dialog, debug free move, key / star door unlocking, entering star door, pulling / pushing
//! door), plus the `interaction.c` entry points that start them.
//!
//! Engine-free. Animation-driven timers use [`AnimClock`], which mirrors `set_mario_animation`,
//! `geo_update_animation_frame` and `is_anim_at_end` over the decomp animation headers in
//! `mario_anim_meta.rs`. Everything the presentation layer must do is reported through
//! [`CutExt::cues`] (sounds, dialog boxes, warps, camera events, visibility, time stop) so the
//! Bevy layer, the camera agent and the audio layer attach without touching the sim.
use crate::actions::cutscene::ready_for_npc_dialog;
use crate::mario_anim_meta::*;
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::{stop_and_set_height_to_floor, stationary_ground_step, GROUND_STEP_LEFT_GROUND_I};
use crate::math::*;
use crate::state::*;

// ---------------------------------------------------------------------------------------------
// Animation clock (mario.c: set_mario_animation / is_anim_at_end / is_anim_past_end,
// graph_node.c: geo_update_animation_frame with animAccel == 0)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct AnimClock {
    pub id: Option<usize>,
    pub frame: i32,
}

impl AnimClock {
    fn meta(&self) -> (i32, i32, i32, i32) {
        MARIO_ANIM_META[self.id.unwrap_or(0).min(MARIO_ANIM_META.len() - 1)]
    }

    /// `set_mario_animation`: returns the current `animFrame`. A new animation starts one frame
    /// before its start frame (so the first frame the action sees is `startFrame - 1`).
    pub fn set(&mut self, id: usize) -> i32 {
        if self.id != Some(id) {
            self.id = Some(id);
            let (flags, start, _, _) = MARIO_ANIM_META[id];
            self.frame = if flags & ANIM_FLAG_2 != 0 {
                start
            } else if flags & ANIM_FLAG_BACKWARD != 0 {
                start + 1
            } else {
                start - 1
            };
        }
        self.frame
    }

    /// `set_anim_to_frame`.
    pub fn set_frame(&mut self, frame: i32) {
        self.frame = frame;
    }

    /// `geo_update_animation_frame` once per rendered frame.
    pub fn advance(&mut self) {
        if self.id.is_none() {
            return;
        }
        let (flags, _, loop_start, loop_end) = self.meta();
        if flags & ANIM_FLAG_2 != 0 {
            return;
        }
        if flags & ANIM_FLAG_BACKWARD != 0 {
            self.frame -= 1;
            if self.frame < loop_start {
                self.frame = if flags & ANIM_FLAG_NOLOOP != 0 { loop_start } else { loop_end - 1 };
            }
        } else {
            self.frame += 1;
            if self.frame >= loop_end {
                self.frame = if flags & ANIM_FLAG_NOLOOP != 0 { loop_end - 1 } else { loop_start };
            }
        }
    }

    pub fn loop_end(&self) -> i32 {
        self.meta().3
    }

    /// `is_anim_at_end`.
    pub fn at_end(&self) -> bool {
        self.frame + 1 == self.loop_end()
    }

    /// `is_anim_past_end`.
    pub fn past_end(&self) -> bool {
        self.frame >= self.loop_end() - 2
    }
}

// ---------------------------------------------------------------------------------------------
// Cues for the presentation layer
// ---------------------------------------------------------------------------------------------

/// `level_update.h` WARP_OP_*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarpOp {
    Unknown02,
    WarpDoor,
    WarpObject,
    StarExit,
    Death,
    CreditsEnd,
    CreditsNext,
}

/// The `CAM_EVENT_START_*` values the cutscene actions leave in `statusForCamera->cameraEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraEvent {
    StartIntro,
    StartCredits,
    StartEndWaving,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CutCue {
    /// `level_trigger_warp`.
    Warp(WarpOp),
    /// `create_dialog_box(id)` (automatic dialogs).
    Dialog(u32),
    /// `create_dialog_inverted_box` for signs.
    SignDialog(u32),
    /// `enable_time_stop` / `disable_time_stop`.
    TimeStop(bool),
    /// A named sound effect (`SOUND_*` without the prefix, lower case).
    Sound(&'static str),
    /// Camera cutscene request (the camera agent's hook).
    Camera(CameraEvent),
    /// `gHudDisplay.flags` hidden / default.
    Hud(bool),
    /// The intro warp pipe appears (position) / is deleted.
    IntroPipe(Option<Vec3>),
    /// Door / key objects spawned by the unlocking actions.
    SpawnKeyDoorKey,
    SpawnUnlockDoorStar,
    /// `save_file_set_flags` for the key / star door the action opened (`door_flag`).
    UnlockDoor { star: bool, upstairs: bool },
}

pub const CUE_LIMIT: usize = 64;

/// Extra state of the cutscene actions. Lives on [`MarioRuntime::cut`].
#[derive(Clone, Debug)]
pub struct CutExt {
    pub clock: AnimClock,
    pub cues: Vec<CutCue>,
    /// `marioObj->header.gfx.node.flags & GRAPH_RENDER_ACTIVE` (false: hidden by an action).
    pub mario_visible: bool,
    /// `marioObj->header.gfx.scale` (squish, BBH shrink).
    pub scale: [f64; 3],
    /// `gTimeStopState`.
    pub time_stop: bool,
    /// An automatic dialog box is open (`get_dialog_id() >= 0`). The presentation layer clears this
    /// when the box closes.
    pub dialog_open: bool,
    /// `statusForCamera->cameraEvent`.
    pub camera_event: Option<CameraEvent>,
    /// `sIntroWarpPipeObj->oPosY` while the intro pipe exists.
    pub intro_pipe_y: Option<f64>,
    /// `gNeverEnteredCastle`.
    pub never_entered_castle: bool,
    /// Camera `cutscene == 0` (dialog / intro finished); set by the presentation layer, default true.
    pub camera_cutscene_done: bool,
    /// Credits viewport (`sEndCutsceneVp`): (vscale, vtrans) once active.
    pub credits_viewport: Option<([i32; 2], [i32; 2])>,
    /// Credits entry flags (`gCurrCreditsEntry->unk02`) for the running credits scene.
    pub credits_flags: i32,
    /// `sDispCreditsEntry` is set: the credits text should be displayed this frame.
    pub credits_text_visible: bool,
    /// Extra yaw applied to the model each frame by the credits (`(unk02 & 0xC0) << 8`).
    pub model_yaw_offset: f64,
    /// Peach / toad objects `act_end_waving_cutscene` spawns (positions).
    pub end_waving_actors: Option<[Vec3; 3]>,
    /// Debug free move toggled by the host (`JUMPMAN_` debug input); not reachable in normal play.
    pub debug_free_move_speed: f64,
    /// Set for the cap pickup cutscene (`interact_cap`): the host enables it.
    pub cap_cutscene: bool,
    /// Getting stuck in snow / sand after a long fall, dive or ground pound (`should_get_stuck_in_ground`).
    /// Off by default: the TypeScript golden fixtures (the ccm snow runs) never stick, so the host opts in.
    pub stuck_in_ground: bool,
    /// `INPUT_SQUISHED` (a dynamic floor / ceiling gap of at most 150 starts `ACT_SQUISHED`). Off by default for the
    /// same reason: the TS golden run of bob act 3 passes under a moving platform without being squished.
    pub squish: bool,
}

impl Default for CutExt {
    fn default() -> Self {
        Self {
            clock: AnimClock::default(),
            cues: vec![],
            mario_visible: true,
            scale: [1.0; 3],
            time_stop: false,
            dialog_open: false,
            camera_event: None,
            intro_pipe_y: None,
            never_entered_castle: false,
            camera_cutscene_done: true,
            credits_viewport: None,
            credits_flags: 0,
            credits_text_visible: false,
            model_yaw_offset: 0.0,
            end_waving_actors: None,
            debug_free_move_speed: 1.0,
            cap_cutscene: false,
            stuck_in_ground: false,
            squish: false,
        }
    }
}

impl CutExt {
    pub fn cue(&mut self, c: CutCue) {
        if self.cues.len() < CUE_LIMIT {
            self.cues.push(c);
        }
    }

    pub fn take_cues(&mut self) -> Vec<CutCue> {
        std::mem::take(&mut self.cues)
    }

    /// Called from `MarioRuntime::reset_to_spawn`.
    pub fn on_respawn(&mut self) {
        let keep = (self.cap_cutscene, self.never_entered_castle, self.stuck_in_ground, self.squish);
        *self = Self::default();
        self.cap_cutscene = keep.0;
        self.never_entered_castle = keep.1;
        self.stuck_in_ground = keep.2;
        self.squish = keep.3;
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// `level_trigger_warp(m, WARP_OP_DEATH)`: asks the host for the death warp. Returns false if the
/// host had no death hook and the runtime fell back to an immediate respawn (the action was
/// replaced by the respawn state).
fn level_trigger_warp(m: &mut MarioRuntime, op: WarpOp) -> bool {
    m.cut.cue(CutCue::Warp(op));
    if op == WarpOp::Death {
        let before = m.state.respawns;
        m.respawn(false);
        return m.state.respawns == before;
    }
    true
}

fn play_sound(m: &mut MarioRuntime, name: &'static str) {
    m.cut.cue(CutCue::Sound(name));
}

fn play_sound_if_no_flag(m: &mut MarioRuntime, name: &'static str, flag: u32) {
    if m.flags & flag == 0 {
        play_sound(m, name);
        m.state.flags |= flag;
    }
}

fn enable_time_stop(m: &mut MarioRuntime) {
    m.cut.time_stop = true;
    m.cut.cue(CutCue::TimeStop(true));
}

fn disable_time_stop(m: &mut MarioRuntime) {
    m.cut.time_stop = false;
    m.cut.cue(CutCue::TimeStop(false));
}

fn advance_cutscene_step(m: &mut MarioRuntime) {
    m.action_state = 0.0;
    m.action_timer = 0.0;
    m.action_arg += 1.0;
}

fn used_obj_pos(m: &MarioRuntime) -> Vec3 {
    m.used_obj.map(|id| m.objects[id].pos).unwrap_or(m.pos)
}

/// `common_death_handler` (mario_actions_cutscene.c). Returns the animation frame.
fn common_death_handler(m: &mut MarioRuntime, anim: usize, frame_to_death_warp: i32) -> i32 {
    let frame = m.cut.clock.set(anim);
    if frame == frame_to_death_warp {
        level_trigger_warp(m, WarpOp::Death);
    }
    stop_and_set_height_to_floor(m);
    frame
}

// ---------------------------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------------------------

/// `act_disappeared`: makes Mario vanish and triggers the warp stored in the high half of the arg.
fn act_disappeared(m: &mut MarioRuntime) -> bool {
    m.cut.clock.set(MARIO_ANIM_A_POSE);
    stop_and_set_height_to_floor(m);
    m.cut.mario_visible = false;
    if m.action_arg != 0.0 {
        m.action_arg -= 1.0;
        let arg = m.action_arg as u32;
        if arg & 0xffff == 0 {
            let op = match arg >> 16 {
                4 => WarpOp::WarpObject,
                3 => WarpOp::WarpDoor,
                0x12 => WarpOp::Death,
                2 => WarpOp::Unknown02,
                _ => WarpOp::WarpObject,
            };
            level_trigger_warp(m, op);
        }
    }
    false
}

/// `act_electrocution`.
fn act_electrocution(m: &mut MarioRuntime) -> bool {
    play_sound_if_no_flag(m, "mario_dying", MARIO_ACTION_SOUND_PLAYED);
    common_death_handler(m, MARIO_ANIM_ELECTROCUTION, 43);
    false
}

/// `act_suffocation`.
fn act_suffocation(m: &mut MarioRuntime) -> bool {
    play_sound_if_no_flag(m, "mario_dying", MARIO_ACTION_SOUND_PLAYED);
    common_death_handler(m, MARIO_ANIM_SUFFOCATING, 86);
    false
}

/// `act_eaten_by_bubba`.
fn act_eaten_by_bubba(m: &mut MarioRuntime) -> bool {
    play_sound_if_no_flag(m, "mario_dying", MARIO_ACTION_SOUND_PLAYED);
    m.cut.clock.set(MARIO_ANIM_A_POSE);
    m.cut.mario_visible = false;
    m.state.health = 0xff as f64;
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t == 60.0 {
        level_trigger_warp(m, WarpOp::Death);
    }
    false
}

/// `act_shocked`.
fn act_shocked(m: &mut MarioRuntime) -> bool {
    play_sound_if_no_flag(m, "mario_waaaooow", MARIO_ACTION_SOUND_PLAYED);
    play_sound(m, "moving_shocked");
    if m.cut.clock.set(MARIO_ANIM_SHOCKED) == 0 {
        m.action_timer += 1.0;
        m.state.flags |= MARIO_METAL_SHOCK;
    }
    if m.action_arg == 0.0 {
        mario_set_forward_vel(m, 0.0);
        if m.step_air(1) == AIR_STEP_LANDED as i32 {
            play_sound(m, "terrain_landing");
            m.action_arg = 1.0;
        }
    } else {
        if m.action_timer >= 6.0 {
            m.invinc_timer = 30.0;
            let next = if m.health < 0x100 as f64 { ACT_ELECTROCUTION } else { ACT_IDLE };
            m.set_action(next, 0.0);
        }
        stop_and_set_height_to_floor(m);
    }
    false
}

/// `act_squished`.
fn act_squished(m: &mut MarioRuntime) -> bool {
    let mut space_under_ceil = m.ceil_height - m.floor_height;
    if space_under_ceil < 0.0 {
        space_under_ceil = 0.0;
    }
    match m.action_state as i32 {
        0 => {
            if space_under_ceil > 160.0 {
                m.state.squish_timer = 0.0;
                return m.set_action(ACT_IDLE, 0.0);
            }
            m.state.squish_timer = 255.0;
            if space_under_ceil >= 10.1 {
                // Mario becomes a pancake.
                let squish = space_under_ceil / 160.0;
                m.cut.scale = [2.0 - squish, squish, 2.0 - squish];
            } else {
                if m.flags & MARIO_METAL_CAP == 0 && m.invinc_timer == 0.0 {
                    // cap on: 3 units; cap off: 4.5 units
                    m.state.hurt_counter += if m.flags & MARIO_CAP_ON_HEAD != 0 { 12.0 } else { 18.0 };
                    play_sound_if_no_flag(m, "mario_attacked", MARIO_MARIO_SOUND_PLAYED);
                }
                m.cut.scale = [1.8, 0.05, 1.8];
                m.action_state = 1.0;
            }
        }
        1 => {
            if space_under_ceil >= 30.0 {
                m.action_state = 2.0;
            }
        }
        2 => {
            m.action_timer += 1.0;
            if m.action_timer >= 15.0 {
                if m.health < 0x100 as f64 {
                    if level_trigger_warp(m, WarpOp::Death) {
                        m.set_action(ACT_DISAPPEARED, 0.0);
                    }
                    return false;
                } else if m.hurt_counter == 0.0 {
                    // un-squish animation
                    m.state.squish_timer = 30.0;
                    m.cut.scale = [1.0; 3];
                    return m.set_action(ACT_IDLE, 0.0);
                }
            }
        }
        _ => {}
    }

    // Steep floor / ceiling: slide out.
    let mut under_steep = false;
    let mut surf_angle = 0.0;
    if let Some(f) = &m.floor {
        if f.normal[1] < 0.5 {
            surf_angle = atan2s(f.normal[2], f.normal[0]) as f64;
            under_steep = true;
        }
    }
    if let Some(c) = &m.ceil {
        if -0.5 < c.normal[1] {
            surf_angle = atan2s(c.normal[2], c.normal[0]) as f64;
            under_steep = true;
        }
    }
    if under_steep {
        m.vel[0] = sins(surf_angle) * 10.0;
        m.vel[2] = coss(surf_angle) * 10.0;
        m.vel[1] = 0.0;
        if m.step_ground() == GROUND_STEP_LEFT_GROUND_I {
            // instant un-squish
            m.state.squish_timer = 0.0;
            m.cut.scale = [1.0; 3];
            return m.set_action(ACT_IDLE, 0.0);
        }
    }

    // Squished for more than 10 seconds: kill Mario.
    let arg = m.action_arg;
    m.action_arg += 1.0;
    if arg > 300.0 {
        m.state.health = 0xff as f64;
        m.state.hurt_counter = 0.0;
        if level_trigger_warp(m, WarpOp::Death) {
            m.set_action(ACT_DISAPPEARED, 0.0);
        }
        return false;
    }
    stop_and_set_height_to_floor(m);
    m.cut.clock.set(MARIO_ANIM_A_POSE);
    false
}

/// `act_putting_on_cap`.
fn act_putting_on_cap(m: &mut MarioRuntime) -> bool {
    let frame = m.cut.clock.set(MARIO_ANIM_PUT_CAP_ON);
    if frame == 0 {
        enable_time_stop(m);
    }
    if frame == 28 {
        // cutscene_put_cap_on
        m.state.flags = (m.flags & !MARIO_CAP_IN_HAND) | MARIO_CAP_ON_HEAD;
        play_sound(m, "action_unknown43e");
    }
    if m.cut.clock.at_end() {
        m.set_action(ACT_IDLE, 0.0);
        disable_time_stop(m);
    }
    stationary_ground_step(&mut m.state, &m.world);
    false
}

/// `stuck_in_ground_handler`.
fn stuck_in_ground_handler(m: &mut MarioRuntime, anim: usize, unstuck: i32, target2: i32, target3: i32, end_action: u32) {
    let mut frame = m.cut.clock.set(anim);
    if m.input & INPUT_A_PRESSED != 0 {
        m.action_timer += 1.0;
        if m.action_timer >= 5.0 && frame < unstuck - 1 {
            frame = unstuck - 1;
            m.cut.clock.set_frame(frame);
        }
    }
    stop_and_set_height_to_floor(m);
    if frame == -1 {
        play_sound(m, "action_terrain_stuck_in_ground");
    } else if frame == unstuck {
        play_sound(m, "action_unstuck_from_ground");
    } else if frame == target2 || frame == target3 {
        play_sound(m, "terrain_landing");
    }
    if m.cut.clock.at_end() {
        m.set_action(end_action, 0.0);
    }
}

/// `should_get_stuck_in_ground` (mario_actions_airborne.c): snow terrain only here (the course
/// exporter has no sand terrain flag), the floor must be soft and a long fall.
pub fn should_get_stuck_in_ground(m: &MarioRuntime) -> bool {
    let Some(floor) = &m.floor else { return false };
    if !m.cut.stuck_in_ground || !m.world.snow_terrain {
        return false;
    }
    let hard = (crate::surfaces::SURFACE_HARD..=crate::surfaces::SURFACE_HARD_NOT_SLIPPERY).contains(&floor.kind)
        || floor.kind == crate::surfaces::SURFACE_BURNING;
    !hard && floor.flags & 0x01 == 0 && m.peak_height - m.pos[1] > 1000.0 && floor.normal[1] >= 0.866_025_4
}

/// `ACT_BBH_ENTER_SPIN` (`act_bbh_enter_spin`).
fn act_bbh_enter_spin(m: &mut MarioRuntime) -> bool {
    let cage = used_obj_pos(m);
    let (dx, dz) = (cage[0] - m.pos[0], cage[2] - m.pos[2]);
    let dist = (dx * dx + dz * dz).sqrt();
    let mut forward_vel = if dist > 20.0 { 10.0 } else { dist / 2.0 };
    if forward_vel < 0.5 {
        forward_vel = 0.0;
    }

    let mut state = m.action_state as i32;
    if state == 0 {
        let floor_dist = 512.0 - (m.pos[1] - m.floor_height);
        m.vel[1] = if floor_dist > 0.0 { (4.0 * floor_dist + 1.0).sqrt() - 1.0 } else { 2.0 };
        m.action_state = 1.0;
        m.action_timer = 100.0;
        state = 1;
    }
    match state {
        1 => {
            m.face_angle[1] = atan2s(dz, dx) as f64;
            mario_set_forward_vel(m, forward_vel);
            if m.cut.clock.set(MARIO_ANIM_FORWARD_SPINNING) == 0 {
                play_sound(m, "action_spin");
            }
            m.state.flags &= !MARIO_UNKNOWN_08;
            m.step_air(0);
            if m.vel[1] <= 0.0 {
                m.action_state = 2.0;
            }
        }
        2 | 3 => {
            m.face_angle[1] = atan2s(dz, dx) as f64;
            mario_set_forward_vel(m, forward_vel);
            m.state.flags &= !MARIO_UNKNOWN_08;
            if m.step_air(0) == AIR_STEP_LANDED as i32 {
                level_trigger_warp(m, WarpOp::Unknown02);
                m.action_state = 4.0;
            }
            if m.action_state == 2.0 {
                if m.cut.clock.frame == 0 {
                    m.action_state = 3.0;
                }
            } else if m.action_state == 3.0 {
                play_sound_if_no_flag(m, "action_shrink_into_bbh", MARIO_ACTION_SOUND_PLAYED);
                m.cut.clock.set(MARIO_ANIM_DIVE);
                m.face_angle[0] = atan2s(m.forward_vel, -m.vel[1]) as f64;
            }
            m.state.squish_timer = 255.0;
            if m.action_timer >= 11.0 {
                m.action_timer -= 6.0;
                let s = m.action_timer / 100.0;
                m.cut.scale = [s, s, s];
            }
        }
        4 => {
            stop_and_set_height_to_floor(m);
            m.cut.mario_visible = false;
        }
        _ => {}
    }
    false
}

/// `act_bbh_enter_jump`.
fn act_bbh_enter_jump(m: &mut MarioRuntime) -> bool {
    play_sound(m, if m.flags & MARIO_METAL_CAP != 0 { "action_metal_jump" } else { "action_terrain_jump" });
    if m.action_state == 0.0 {
        let cage = used_obj_pos(m);
        let (dx, dz) = (cage[0] - m.pos[0], cage[2] - m.pos[2]);
        let dist = (dx * dx + dz * dz).sqrt();
        m.vel[1] = 60.0;
        m.face_angle[1] = atan2s(dz, dx) as f64;
        mario_set_forward_vel(m, dist / 20.0);
        m.state.flags &= !MARIO_UNKNOWN_08;
        m.action_state = 1.0;
    }
    m.cut.clock.set(MARIO_ANIM_DOUBLE_JUMP_RISE);
    m.step_air(0);
    if m.vel[1] <= 0.0 {
        m.set_action(ACT_BBH_ENTER_SPIN, 0.0);
    }
    false
}

/// `act_warp_door_spawn` (`m->usedObj->oAction == 0` is "the door finished opening": the host
/// reports it through `door_finished`, which the progress layer sets once the door leaf is shut).
fn act_warp_door_spawn(m: &mut MarioRuntime, door_finished: bool) -> bool {
    if m.action_state == 0.0 {
        m.action_state = 1.0;
    } else if door_finished {
        if m.cut.never_entered_castle {
            m.cut.cue(CutCue::Dialog(crate::sm64_dialogs::DIALOG_NEVER_ENTERED_CASTLE));
            m.set_action(ACT_READING_AUTOMATIC_DIALOG, crate::sm64_dialogs::DIALOG_NEVER_ENTERED_CASTLE as f64);
        } else {
            m.set_action(ACT_IDLE, 0.0);
        }
    }
    m.cut.clock.set(MARIO_ANIM_FIRST_PERSON);
    stop_and_set_height_to_floor(m);
    false
}

/// `act_emerge_from_pipe`.
fn act_emerge_from_pipe(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t < 11.0 {
        m.cut.mario_visible = false;
        return false;
    }
    m.cut.mario_visible = true;
    play_sound_if_no_flag(m, "mario_yahoo", MARIO_MARIO_SOUND_PLAYED);
    // launch_mario_until_land(m, ACT_JUMP_LAND_STOP, SINGLE_JUMP, 8.0)
    mario_set_forward_vel(m, 8.0);
    m.cut.clock.set(MARIO_ANIM_SINGLE_JUMP);
    if m.step_air(0) == AIR_STEP_LANDED as i32 {
        m.set_action(ACT_JUMP_LAND_STOP, 0.0);
        mario_set_forward_vel(m, 0.0);
        play_sound(m, "terrain_landing");
    }
    false
}

/// `act_reading_automatic_dialog`.
fn act_reading_automatic_dialog(m: &mut MarioRuntime) -> bool {
    m.action_state += 1.0;
    let state = m.action_state as i32;
    if state == 2 {
        enable_time_stop(m);
    }
    if state < 9 {
        let anim = if m.prev_action == ACT_STAR_DANCE_WATER { MARIO_ANIM_WATER_IDLE } else { MARIO_ANIM_FIRST_PERSON };
        m.cut.clock.set(anim);
        // always look up for automatic dialogs
        m.action_timer -= 1024.0;
    } else if state == 9 {
        let id = (m.action_arg as u32) & 0xffff;
        m.cut.dialog_open = true;
        m.cut.cue(CutCue::Dialog(id));
    } else if state == 10 {
        // wait until dialog is done
        if m.cut.dialog_open {
            m.action_state -= 1.0;
        }
    } else if state < 19 {
        // look back down
        m.action_timer += 1024.0;
    } else if state == 25 {
        disable_time_stop(m);
        if m.cut.never_entered_castle {
            m.cut.never_entered_castle = false;
        }
        if m.prev_action == ACT_STAR_DANCE_WATER {
            m.set_action(ACT_WATER_IDLE, 0.0);
        } else {
            // make Mario walk into the door after the star door dialog
            m.set_action(if m.prev_action == ACT_UNLOCKING_STAR_DOOR { ACT_WALKING } else { ACT_IDLE }, 0.0);
        }
    }
    false
}

/// `act_waiting_for_dialog`.
fn act_waiting_for_dialog(m: &mut MarioRuntime) -> bool {
    m.cut.clock.set(if m.held_obj.is_none() { MARIO_ANIM_FIRST_PERSON } else { MARIO_ANIM_IDLE_WITH_LIGHT_OBJ });
    false
}

/// `mario_ready_to_speak` (also true while Mario waits for a dialog).
pub fn mario_ready_to_speak(m: &MarioRuntime) -> bool {
    m.action == ACT_WAITING_FOR_DIALOG || ready_for_npc_dialog(m)
}

/// `act_debug_free_move`.
fn act_debug_free_move(m: &mut MarioRuntime) -> bool {
    let down = m.controller.button_down;
    let mut speed = if down & B_BUTTON != 0 { 4.0 } else { 1.0 };
    if down & L_TRIG != 0 {
        speed = 0.01;
    }
    m.cut.clock.set(MARIO_ANIM_A_POSE);
    let mut pos = m.pos;
    // U_JPAD / D_JPAD are mapped to C-up / C-down.
    if down & C_UP != 0 {
        pos[1] += 16.0 * speed;
    }
    if down & C_DOWN != 0 {
        pos[1] -= 16.0 * speed;
    }
    if m.intended_mag > 0.0 {
        pos[0] += 32.0 * speed * sins(m.intended_yaw);
        pos[2] += 32.0 * speed * coss(m.intended_yaw);
    }
    m.world.resolve_and_return_wall_collisions(&mut pos, 60.0, 50.0);
    let floor = m.world.find_floor(pos[0], pos[1], pos[2]);
    if floor.surface.is_some() {
        if pos[1] < floor.height {
            pos[1] = floor.height;
        }
        m.state.pos = pos;
    }
    m.face_angle[1] = m.intended_yaw;
    if m.controller.button_pressed == A_BUTTON {
        let water = m.world.water_level(m.pos[0], m.pos[2]);
        let action = if m.pos[1] <= water - 100.0 { ACT_WATER_IDLE } else { ACT_IDLE };
        m.set_action(action, 0.0);
    }
    false
}

// ----------------------------- doors ---------------------------------------------------------

/// `act_unlocking_key_door`. Root motion (`update_mario_pos_for_anim`) is applied by the host
/// (`progress/doors.rs`); the timers, flags and the final action are the decomp's.
fn act_unlocking_key_door(m: &mut MarioRuntime) -> bool {
    if let Some(id) = m.used_obj {
        let o = &m.objects[id];
        let yaw = o.move_yaw;
        m.state.face_angle[1] = yaw;
        let (ox, oz) = (o.pos[0], o.pos[2]);
        m.state.pos[0] = ox + coss(yaw) * 75.0;
        m.state.pos[2] = oz + sins(yaw) * 75.0;
    }
    if (m.action_arg as u32) & 2 != 0 {
        m.face_angle[1] = to_s16(m.face_angle[1] + 0x8000 as f64) as f64;
    }
    if m.action_timer == 0.0 {
        m.cut.cue(CutCue::SpawnKeyDoorKey);
        m.cut.clock.set(MARIO_ANIM_UNLOCK_DOOR);
    }
    match m.cut.clock.frame {
        79 => play_sound(m, "general_door_insert_key"),
        111 => play_sound(m, "general_door_turn_key"),
        _ => {}
    }
    stop_and_set_height_to_floor(m);
    if m.cut.clock.at_end() {
        let upstairs = m.used_obj.map(|id| (m.objects[id].param as i64 >> 24) == 1).unwrap_or(false);
        m.cut.cue(CutCue::UnlockDoor { star: false, upstairs });
        m.set_action(ACT_WALKING, 0.0);
    }
    m.action_timer += 1.0;
    false
}

/// `act_unlocking_star_door`.
fn act_unlocking_star_door(m: &mut MarioRuntime) -> bool {
    match m.action_state as i32 {
        0 => {
            if let Some(id) = m.used_obj {
                m.state.face_angle[1] = m.objects[id].move_yaw;
            }
            if (m.action_arg as u32) & 2 != 0 {
                m.face_angle[1] = to_s16(m.face_angle[1] + 0x8000 as f64) as f64;
            }
            m.sign_motion = [m.pos[0], 0.0, m.pos[2]];
            m.cut.clock.set(MARIO_ANIM_SUMMON_STAR);
            m.action_state += 1.0;
        }
        1 => {
            if m.cut.clock.at_end() {
                m.cut.cue(CutCue::SpawnUnlockDoorStar);
                m.action_state += 1.0;
            }
        }
        2 => {
            let t = m.action_timer;
            m.action_timer += 1.0;
            if t == 70.0 {
                m.cut.clock.set(MARIO_ANIM_RETURN_STAR_APPROACH_DOOR);
                m.action_state += 1.0;
            }
        }
        3 => {
            if m.cut.clock.at_end() {
                m.cut.cue(CutCue::UnlockDoor { star: true, upstairs: false });
                m.set_action(ACT_READING_AUTOMATIC_DIALOG, crate::sm64_dialogs::DIALOG_UNLOCKED_STAR_DOOR as f64);
                return false;
            }
        }
        _ => {}
    }
    m.state.pos[0] = m.sign_motion[0];
    m.state.pos[2] = m.sign_motion[2];
    stop_and_set_height_to_floor(m);
    false
}

/// `act_entering_star_door`.
fn act_entering_star_door(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    let door = m.used_obj.map(|id| (m.objects[id].pos, m.objects[id].move_yaw)).unwrap_or((m.pos, m.face_angle[1]));
    if t == 0.0 {
        // ~30 degrees / 1/12 rot
        let mut target = door.1 + 0x1555 as f64;
        if (m.action_arg as u32) & 2 != 0 {
            target += 0x5556 as f64; // ~120 degrees (total 150 degrees)
        }
        // Offsets that put Mario 150 units in front of the door.
        let dx = door.0[0] + 150.0 * sins(target) - m.pos[0];
        let dz = door.0[2] + 150.0 * coss(target) - m.pos[2];
        m.sign_motion = [dx / 20.0, 0.0, dz / 20.0];
        m.face_angle[1] = atan2s(dz, dx) as f64;
    }
    let timer = m.action_timer;
    if timer < 15.0 {
        m.cut.clock.set(MARIO_ANIM_FIRST_PERSON);
    } else if timer < 35.0 {
        // go through the door for 20 frames
        m.state.pos[0] += m.sign_motion[0];
        m.state.pos[2] += m.sign_motion[2];
        m.cut.clock.set(MARIO_ANIM_WALKING);
    } else {
        m.face_angle[1] = door.1;
        if (m.action_arg as u32) & 2 != 0 {
            m.face_angle[1] = to_s16(m.face_angle[1] + 0x8000 as f64) as f64;
        }
        m.state.pos[0] += 12.0 * sins(m.face_angle[1]);
        m.state.pos[2] += 12.0 * coss(m.face_angle[1]);
        m.cut.clock.set(MARIO_ANIM_WALKING);
    }
    stop_and_set_height_to_floor(m);
    if m.action_timer == 48.0 {
        m.set_action(ACT_IDLE, 0.0);
    }
    false
}

/// `act_going_through_door` (pulling / pushing). Root motion is applied by the host.
fn act_going_through_door(m: &mut MarioRuntime) -> bool {
    if m.action_timer == 0.0 {
        m.cut.clock.set(if (m.action_arg as u32) & 1 != 0 { MARIO_ANIM_PULL_DOOR_WALK_IN } else { MARIO_ANIM_PUSH_DOOR_WALK_IN });
    }
    if let Some(id) = m.used_obj {
        let o = &m.objects[id];
        m.state.face_angle[1] = o.move_yaw;
        m.state.pos[0] = o.pos[0];
        m.state.pos[2] = o.pos[2];
    }
    stop_and_set_height_to_floor(m);
    if (m.action_arg as u32) & 4 != 0 {
        if m.action_timer == 16.0 {
            level_trigger_warp(m, WarpOp::WarpDoor);
        }
    } else if m.cut.clock.at_end() {
        if (m.action_arg as u32) & 2 != 0 {
            m.face_angle[1] = to_s16(m.face_angle[1] + 0x8000 as f64) as f64;
        }
        m.set_action(ACT_IDLE, 0.0);
    }
    m.action_timer += 1.0;
    false
}

// ----------------------------- intro / credits / end waving ----------------------------------

/// Intro warp pipe position (`spawn_object_abs_with_rot(... -1328, 60, 4664 ...)`).
pub const INTRO_PIPE_POS: Vec3 = [-1328.0, 60.0, 4664.0];

/// `act_intro_cutscene` (US timers: 37 / 38 / 118).
fn act_intro_cutscene(m: &mut MarioRuntime) -> bool {
    match m.action_arg as i32 {
        // INTRO_CUTSCENE_HIDE_HUD_AND_MARIO
        0 => {
            m.cut.cue(CutCue::Hud(false));
            m.cut.camera_event = Some(CameraEvent::StartIntro);
            m.cut.cue(CutCue::Camera(CameraEvent::StartIntro));
            m.cut.mario_visible = false;
            advance_cutscene_step(m);
        }
        // INTRO_CUTSCENE_PEACH_LAKITU_SCENE (TIMER_SPAWN_PIPE 37)
        1 => {
            if m.cut.camera_event != Some(CameraEvent::StartIntro) {
                let t = m.action_timer;
                m.action_timer += 1.0;
                if t == 37.0 {
                    m.cut.intro_pipe_y = Some(INTRO_PIPE_POS[1]);
                    m.cut.cue(CutCue::IntroPipe(Some(INTRO_PIPE_POS)));
                    advance_cutscene_step(m);
                }
            }
        }
        // INTRO_CUTSCENE_RAISE_PIPE (TIMER_RAISE_PIPE 38)
        2 => {
            let y = m.cut.intro_pipe_y.unwrap_or(INTRO_PIPE_POS[1]);
            m.cut.intro_pipe_y = Some(approach_f32(y, 260.0, 10.0, 10.0));
            if m.action_timer == 0.0 {
                play_sound(m, "menu_exit_pipe");
            }
            let t = m.action_timer;
            m.action_timer += 1.0;
            if t == 38.0 {
                m.vel[1] = 60.0;
                advance_cutscene_step(m);
            }
        }
        // INTRO_CUTSCENE_JUMP_OUT_OF_PIPE
        3 => {
            if m.action_timer == 25.0 {
                m.cut.cue(CutCue::Hud(true));
            }
            let t = m.action_timer;
            m.action_timer += 1.0;
            if t >= 118.0 {
                m.cut.mario_visible = true;
                play_sound_if_no_flag(m, "mario_yahoo", MARIO_MARIO_SOUND_PLAYED);
                play_sound_if_no_flag(m, "action_hit_3", MARIO_ACTION_SOUND_PLAYED);
                m.cut.clock.set(MARIO_ANIM_SINGLE_JUMP);
                mario_set_forward_vel(m, 10.0);
                if m.step_air(0) == AIR_STEP_LANDED as i32 {
                    play_sound(m, "terrain_landing");
                    play_sound(m, "mario_haha");
                    advance_cutscene_step(m);
                }
            }
        }
        // INTRO_CUTSCENE_LAND_OUTSIDE_PIPE
        4 => {
            m.cut.clock.set(MARIO_ANIM_LAND_FROM_SINGLE_JUMP);
            if m.cut.clock.at_end() {
                advance_cutscene_step(m);
            }
            stop_and_set_height_to_floor(m);
        }
        // INTRO_CUTSCENE_LOWER_PIPE
        5 => {
            let t = m.action_timer;
            m.action_timer += 1.0;
            if t == 0.0 {
                play_sound(m, "menu_enter_pipe");
                m.cut.clock.set(MARIO_ANIM_FIRST_PERSON);
            }
            let y = m.cut.intro_pipe_y.unwrap_or(260.0) - 5.0;
            m.cut.intro_pipe_y = Some(y);
            if y <= 50.0 {
                m.cut.intro_pipe_y = None;
                m.cut.cue(CutCue::IntroPipe(None));
                advance_cutscene_step(m);
            }
            stop_and_set_height_to_floor(m);
        }
        // INTRO_CUTSCENE_SET_MARIO_TO_IDLE
        6 => {
            if m.cut.camera_cutscene_done {
                m.set_action(ACT_IDLE, 0.0);
            }
            stop_and_set_height_to_floor(m);
        }
        _ => {}
    }
    false
}

/// Credits timers (US): `TIMER_CREDITS_SHOW` / `PROGRESS` / `WARP`.
pub const TIMER_CREDITS_SHOW: f64 = 61.0;
pub const TIMER_CREDITS_PROGRESS: f64 = 90.0;
pub const TIMER_CREDITS_WARP: f64 = 200.0;

/// `sEndCutsceneVp` for a given `actionState` and credits entry flags (`unk02`):
/// returns (vscale, vtrans).
pub fn credits_viewport(action_state: i32, entry_flags: i32) -> ([i32; 2], [i32; 2]) {
    let width = action_state * 640 / 100;
    let height = action_state * 480 / 100;
    let vscale = [640 - width, 480 - height];
    let vtrans = [
        (if entry_flags & 0x10 != 0 { width } else { -width }) * 56 / 100 + 640,
        (if entry_flags & 0x20 != 0 { height } else { -height }) * 66 / 100 + 480,
    ];
    (vscale, vtrans)
}

/// `act_credits_cutscene`.
fn act_credits_cutscene(m: &mut MarioRuntime) -> bool {
    m.cut.camera_event = Some(CameraEvent::StartCredits);
    let water = m.world.water_level(m.pos[0], m.pos[2]);
    if m.pos[1] < water - 100.0 {
        m.cut.clock.set(MARIO_ANIM_WATER_IDLE);
    } else {
        m.cut.clock.set(MARIO_ANIM_FIRST_PERSON);
        if m.action_timer > 0.0 {
            stop_and_set_height_to_floor(m);
        }
    }

    if m.action_timer >= TIMER_CREDITS_SHOW {
        if m.action_state < 40.0 {
            m.action_state += 2.0;
        }
        m.cut.credits_viewport = Some(credits_viewport(m.action_state as i32, m.cut.credits_flags));
    }
    if m.action_timer == TIMER_CREDITS_PROGRESS {
        // reset_cutscene_msg_fade
    }
    m.cut.credits_text_visible = m.action_timer >= TIMER_CREDITS_PROGRESS;
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t == TIMER_CREDITS_WARP {
        level_trigger_warp(m, WarpOp::CreditsNext);
    }
    m.cut.model_yaw_offset = ((m.cut.credits_flags & 0xc0) << 8) as f64;
    false
}

/// `act_end_waving_cutscene`: Peach at (60, 906, -1180), toads at (+-180, 906, -1170).
pub const END_WAVING_ACTORS: [Vec3; 3] = [[60.0, 906.0, -1180.0], [180.0, 906.0, -1170.0], [-180.0, 906.0, -1170.0]];

fn act_end_waving_cutscene(m: &mut MarioRuntime) -> bool {
    if m.action_state == 0.0 {
        m.cut.camera_event = Some(CameraEvent::StartEndWaving);
        m.cut.cue(CutCue::Camera(CameraEvent::StartEndWaving));
        m.cut.end_waving_actors = Some(END_WAVING_ACTORS);
        m.action_state = 1.0;
    }
    m.cut.clock.set(MARIO_ANIM_CREDITS_WAVING);
    stop_and_set_height_to_floor(m);
    // gfx.angle[1] += 0x8000; gfx.pos[0] -= 60: presentation-only.
    m.cut.model_yaw_offset = 0x8000 as f64;
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t == 300.0 {
        level_trigger_warp(m, WarpOp::CreditsEnd);
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------------------------

/// Handler table for the actions of this module (chained after `cutscene_action` and
/// `ending_action` by `actions::execute_mario_action`).
pub fn cutscene_ext_action(m: &mut MarioRuntime) -> Option<bool> {
    let handled = matches!(
        m.action,
        ACT_DISAPPEARED | ACT_INTRO_CUTSCENE | ACT_READING_AUTOMATIC_DIALOG | ACT_WAITING_FOR_DIALOG | ACT_DEBUG_FREE_MOVE
            | ACT_ELECTROCUTION | ACT_SUFFOCATION | ACT_EATEN_BY_BUBBA | ACT_CREDITS_CUTSCENE | ACT_END_WAVING_CUTSCENE
            | ACT_WARP_DOOR_SPAWN | ACT_EMERGE_FROM_PIPE | ACT_BBH_ENTER_JUMP | ACT_BBH_ENTER_SPIN | ACT_SHOCKED
            | ACT_SQUISHED | ACT_HEAD_STUCK_IN_GROUND | ACT_BUTT_STUCK_IN_GROUND | ACT_FEET_STUCK_IN_GROUND
            | ACT_PUTTING_ON_CAP | ACT_UNLOCKING_KEY_DOOR | ACT_UNLOCKING_STAR_DOOR | ACT_ENTERING_STAR_DOOR
            | ACT_PULLING_DOOR | ACT_PUSHING_DOOR
    );
    if !handled {
        return None;
    }
    // One animation frame per rendered frame (the first pass of the same-frame action loop).
    if m.action_loops <= 1 {
        m.cut.clock.advance();
    }
    Some(match m.action {
        ACT_DISAPPEARED => act_disappeared(m),
        ACT_INTRO_CUTSCENE => act_intro_cutscene(m),
        ACT_READING_AUTOMATIC_DIALOG => act_reading_automatic_dialog(m),
        ACT_WAITING_FOR_DIALOG => act_waiting_for_dialog(m),
        ACT_DEBUG_FREE_MOVE => act_debug_free_move(m),
        ACT_ELECTROCUTION => act_electrocution(m),
        ACT_SUFFOCATION => act_suffocation(m),
        ACT_EATEN_BY_BUBBA => act_eaten_by_bubba(m),
        ACT_CREDITS_CUTSCENE => act_credits_cutscene(m),
        ACT_END_WAVING_CUTSCENE => act_end_waving_cutscene(m),
        ACT_WARP_DOOR_SPAWN => {
            let finished = m.used_obj.map_or(true, |id| m.objects[id].action == 0);
            act_warp_door_spawn(m, finished)
        }
        ACT_EMERGE_FROM_PIPE => act_emerge_from_pipe(m),
        ACT_BBH_ENTER_JUMP => act_bbh_enter_jump(m),
        ACT_BBH_ENTER_SPIN => act_bbh_enter_spin(m),
        ACT_SHOCKED => act_shocked(m),
        ACT_SQUISHED => act_squished(m),
        ACT_HEAD_STUCK_IN_GROUND => {
            stuck_in_ground_handler(m, MARIO_ANIM_HEAD_STUCK_IN_GROUND, 96, 105, 135, ACT_IDLE);
            false
        }
        ACT_BUTT_STUCK_IN_GROUND => {
            stuck_in_ground_handler(m, MARIO_ANIM_BOTTOM_STUCK_IN_GROUND, 127, 136, -2, ACT_GROUND_POUND_LAND);
            false
        }
        ACT_FEET_STUCK_IN_GROUND => {
            stuck_in_ground_handler(m, MARIO_ANIM_LEGS_STUCK_IN_GROUND, 116, 129, -2, ACT_IDLE);
            false
        }
        ACT_PUTTING_ON_CAP => act_putting_on_cap(m),
        ACT_UNLOCKING_KEY_DOOR => act_unlocking_key_door(m),
        ACT_UNLOCKING_STAR_DOOR => act_unlocking_star_door(m),
        ACT_ENTERING_STAR_DOOR => act_entering_star_door(m),
        ACT_PULLING_DOOR | ACT_PUSHING_DOOR => act_going_through_door(m),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Triggers (interaction.c / level_update.c / mario.c)
// ---------------------------------------------------------------------------------------------

/// `INPUT_SQUISHED` (mario.c `update_mario_geometry_inputs`): the floor or ceiling belongs to a
/// dynamic object and the gap is at most 150 units.
pub fn squish_input(m: &MarioRuntime) -> bool {
    if !m.cut.squish {
        return false;
    }
    let dynamic = m.floor.as_ref().is_some_and(|f| f.object.is_some()) || m.ceil.as_ref().is_some_and(|c| c.object.is_some());
    if !dynamic {
        return false;
    }
    let gap = m.ceil_height - m.floor_height;
    (0.0..=150.0).contains(&gap)
}

/// The `INPUT_SQUISHED` branch of the stationary / moving / airborne / object common cancels
/// (`drop_and_set_mario_action(m, ACT_SQUISHED, 0)`).
pub fn check_squished(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_SQUISHED != 0 {
        return m.set_action(ACT_SQUISHED, 0.0);
    }
    false
}

/// `interact_shock`: electric shock (Amp, lightning). The caller has already decided that Mario
/// can be hurt; this applies `take_damage_from_interact_object` (damage in wedges) and the action.
pub fn interact_shock(m: &mut MarioRuntime, damage: f64) -> bool {
    if m.flags & MARIO_VANISH_CAP != 0 || m.invinc_timer > 0.0 || m.action & (ACT_FLAG_INVULNERABLE | ACT_FLAG_INTANGIBLE) != 0 {
        return false;
    }
    let arg = if m.action & (ACT_FLAG_AIR | ACT_FLAG_ON_POLE | ACT_FLAG_HANGING) == 0 { 1.0 } else { 0.0 };
    m.state.hurt_counter += 4.0 * damage;
    m.events.push(MarioEvent::Hurt { damage });
    m.set_action(ACT_SHOCKED, arg)
}

/// `interact_bbh_entrance`.
pub fn interact_bbh_entrance(m: &mut MarioRuntime, entrance: crate::object::ObjId) -> bool {
    if m.action == ACT_BBH_ENTER_SPIN || m.action == ACT_BBH_ENTER_JUMP {
        return false;
    }
    m.used_obj = Some(entrance);
    m.interact_obj = Some(entrance);
    if m.action & ACT_FLAG_AIR != 0 {
        m.set_action(ACT_BBH_ENTER_SPIN, 0.0)
    } else {
        m.set_action(ACT_BBH_ENTER_JUMP, 0.0)
    }
}

/// `interact_clam_or_bubba` with `INT_SUBTYPE_EATS_MARIO`.
pub fn eaten_by_bubba(m: &mut MarioRuntime) -> bool {
    m.set_action(ACT_EATEN_BY_BUBBA, 0.0)
}

/// `interact_cap`: the idle / walking branch (`MARIO_CAP_IN_HAND` + `ACT_PUTTING_ON_CAP`). Only when
/// the host enabled [`CutExt::cap_cutscene`]; otherwise the cap goes straight on, as before.
pub fn cap_pickup_cutscene(m: &mut MarioRuntime) -> bool {
    if m.cut.cap_cutscene && (m.action & ACT_FLAG_IDLE != 0 || m.action == ACT_WALKING) {
        m.state.flags |= MARIO_CAP_IN_HAND;
        m.state.flags &= !MARIO_CAP_ON_HEAD;
        return m.set_action(ACT_PUTTING_ON_CAP, 0.0);
    }
    m.state.flags |= MARIO_CAP_ON_HEAD;
    false
}

/// `check_fall_damage_or_get_stuck` stuck branch (feet).
pub fn check_stuck_feet(m: &mut MarioRuntime) -> bool {
    if should_get_stuck_in_ground(m) {
        return m.set_action(ACT_FEET_STUCK_IN_GROUND, 0.0);
    }
    false
}

/// `set_mario_initial_action` for `MARIO_SPAWN_DOOR_WARP` / `UNKNOWN_03` (pipe).
pub fn spawn_from_door(m: &mut MarioRuntime, arg: f64) -> bool {
    m.set_action(ACT_WARP_DOOR_SPAWN, arg)
}
pub fn spawn_from_pipe(m: &mut MarioRuntime) -> bool {
    m.set_action(ACT_EMERGE_FROM_PIPE, 0.0)
}

/// `level_update.c` credits: `ACT_CREDITS_CUTSCENE` is started by `warp_credits` with the entry's
/// Mario position and angle (`marioAngle << 8`).
pub fn start_credits_scene(m: &mut MarioRuntime, entry: &CreditsEntry) {
    m.pos = [entry.mario_pos[0] as f64, entry.mario_pos[1] as f64, entry.mario_pos[2] as f64];
    m.face_angle = [0.0, ((entry.mario_angle as i32) << 8) as f64, 0.0];
    m.cut.credits_flags = entry.flags;
    m.cut.credits_viewport = None;
    m.set_action(ACT_CREDITS_CUTSCENE, 0.0);
}

/// `struct CreditsEntry` (level_update.h) without the text pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditsEntry {
    pub level: &'static str,
    pub area: i32,
    /// `unk02` (u8): bit 0x10/0x20 pick the viewport corner, 0xC0 the model spin.
    pub flags: i32,
    /// `marioAngle` (s8): the yaw is `marioAngle << 8`.
    pub mario_angle: i8,
    pub mario_pos: [i16; 3],
    /// `credits01..credits20`: the raw strings; the first starts with the line-count digit that
    /// `print_displaying_credits_entry` strips.
    pub text: &'static [&'static str],
}

/// `sCreditsSequence` (level_update.c, `VERSION_US`). The first and last entries have no text.
pub const CREDITS_SEQUENCE: [CreditsEntry; 22] = [
    CreditsEntry { level: "castle_grounds", area: 1, flags: 1, mario_angle: -128, mario_pos: [0, 8000, 0], text: &[] },
    CreditsEntry { level: "bob", area: 1, flags: 1, mario_angle: 117, mario_pos: [713, 3918, -3889], text: &["1GAME DIRECTOR", "SHIGERU MIYAMOTO"] },
    CreditsEntry { level: "wf", area: 1, flags: 50, mario_angle: 46, mario_pos: [347, 5376, 326], text: &["2ASSISTANT DIRECTORS", "YOSHIAKI KOIZUMI", "TAKASHI TEZUKA"] },
    CreditsEntry { level: "jrb", area: 1, flags: 18, mario_angle: 22, mario_pos: [3800, -4840, 2727], text: &["2SYSTEM PROGRAMMERS", "YASUNARI NISHIDA", "YOSHINORI TANIMOTO"] },
    CreditsEntry { level: "ccm", area: 2, flags: 34, mario_angle: 25, mario_pos: [-5464, 6656, -6575], text: &["3PROGRAMMERS", "HAJIME YAJIMA", "DAIKI IWAMOTO", "TOSHIO IWAWAKI"] },
    CreditsEntry { level: "bbh", area: 1, flags: 1, mario_angle: 60, mario_pos: [257, 1922, 2580], text: &["4CAMERA PROGRAMMER", "MARIO FACE PROGRAMMER", "TAKUMI KAWAGOE", "GILES GODDARD"] },
    CreditsEntry { level: "hmc", area: 1, flags: 241, mario_angle: 123, mario_pos: [-6469, 1616, -6054], text: &["2COURSE DIRECTORS", "YOICHI YAMADA", "YASUHISA YAMAMURA"] },
    CreditsEntry { level: "thi", area: 3, flags: 17, mario_angle: -32, mario_pos: [508, 1024, 1942], text: &["2COURSE DESIGNERS", "KENTA USUI", "NAOKI MORI"] },
    CreditsEntry { level: "lll", area: 2, flags: 33, mario_angle: 124, mario_pos: [-73, 82, -1467], text: &["3COURSE DESIGNERS", "YOSHIKI HARUHANA", "MAKOTO MIYANAGA", "KATSUHIKO KANNO"] },
    CreditsEntry { level: "ssl", area: 1, flags: 65, mario_angle: 98, mario_pos: [-5906, 1024, -2576], text: &["1SOUND COMPOSER", "KOJI KONDO"] },
    CreditsEntry { level: "ddd", area: 1, flags: 50, mario_angle: 47, mario_pos: [-4884, -4607, -272], text: &["4SOUND EFFECTS", "SOUND PROGRAMMER", "YOJI INAGAKI", "HIDEAKI SHIMIZU"] },
    CreditsEntry { level: "sl", area: 1, flags: 17, mario_angle: -34, mario_pos: [1925, 3328, 563], text: &["23-D ANIMATORS", "YOSHIAKI KOIZUMI", "SATORU TAKIZAWA"] },
    CreditsEntry { level: "wdw", area: 1, flags: 33, mario_angle: 105, mario_pos: [-537, 1850, 1818], text: &["1ADDITIONAL GRAPHICS", "MASANAO ARIMOTO"] },
    CreditsEntry { level: "ttm", area: 1, flags: 2, mario_angle: -33, mario_pos: [2613, 313, 1074], text: &["3TECHNICAL SUPPORT", "TAKAO SAWANO", "HIROHITO YOSHIMOTO", "HIROTO YADA"] },
    CreditsEntry { level: "thi", area: 1, flags: 51, mario_angle: 54, mario_pos: [-2609, 512, 856], text: &["1TECHNICAL SUPPORT", "SGI N64 PROJECT STAFF"] },
    CreditsEntry { level: "ttc", area: 1, flags: 17, mario_angle: -72, mario_pos: [-1304, -71, -967], text: &["2PROGRESS MANAGEMENT", "KIMIYOSHI FUKUI", "KEIZO KATO"] },
    CreditsEntry { level: "rr", area: 1, flags: 33, mario_angle: 64, mario_pos: [1565, 1024, -148], text: &["5SCREEN TEXT WRITER", "TRANSLATION", "LESLIE SWAN", "MINA AKINO", "HIRO YAMADA"] },
    CreditsEntry { level: "sa", area: 1, flags: 1, mario_angle: 24, mario_pos: [-1050, -1330, -1559], text: &["4MARIO VOICE", "PEACH VOICE", "CHARLES MARTINET", "LESLIE SWAN"] },
    CreditsEntry { level: "cotmc", area: 1, flags: 49, mario_angle: -16, mario_pos: [-254, 415, -6045], text: &["3SPECIAL THANKS TO", "EAD STAFF", "ALL NINTENDO PERSONNEL", "MARIO CLUB STAFF"] },
    CreditsEntry { level: "ddd", area: 2, flags: 145, mario_angle: -64, mario_pos: [3948, 1185, -104], text: &["1PRODUCER", "SHIGERU MIYAMOTO"] },
    CreditsEntry { level: "ccm", area: 1, flags: 33, mario_angle: 31, mario_pos: [3169, -4607, 5240], text: &["1EXECUTIVE PRODUCER", "HIROSHI YAMAUCHI"] },
    CreditsEntry { level: "castle_grounds", area: 1, flags: 1, mario_angle: -128, mario_pos: [0, 906, -1200], text: &[] },
];

/// Actions that handle `health < 0x100` themselves (the decomp's death sequences).
pub fn runs_own_death(action: u32) -> bool {
    matches!(
        action,
        ACT_SHOCKED | ACT_ELECTROCUTION | ACT_SUFFOCATION | ACT_EATEN_BY_BUBBA | ACT_SQUISHED | ACT_DISAPPEARED | ACT_QUICKSAND_DEATH
    )
}
