//! The gameplay `CUTSCENE_*` sequences of `camera.c`: `play_cutscene`, the shot tables
//! (`sCutscene*`) and the shot functions. Ported: door pull / push (and the `_MODE` variants),
//! door warp, sliding star doors, enter cannon, prepare cannon, enter / exit painting (death
//! exit, exit success), the death cameras (standing, stomach, on back, quicksand, suffocation,
//! non-painting, Bowser), star spawn, red-coin star spawn, enter pool, enter pyramid top, exit
//! waterfall / fall to castle grounds and the star dances (default, rotate, fly away, close-up,
//! key dance). Not ported: intro / ending / credits / grand star / end waving, the Bowser
//! arena entrance, the dialog / read-message / race-dialog family (the app's own dialog camera
//! keeps those), cap-switch press, key-door unlock and the pyramid-top explosion.
use super::data::*;
use super::triggers::areas::*;
use super::util::*;
use super::*;

pub const CUTSCENE_DOOR_PULL: i32 = 130;
pub const CUTSCENE_DOOR_PUSH: i32 = 131;
pub const CUTSCENE_ENTER_CANNON: i32 = 133;
pub const CUTSCENE_ENTER_PAINTING: i32 = 134;
pub const CUTSCENE_DEATH_EXIT: i32 = 135;
pub const CUTSCENE_DOOR_WARP: i32 = 139;
pub const CUTSCENE_DOOR_PULL_MODE: i32 = 140;
pub const CUTSCENE_DOOR_PUSH_MODE: i32 = 141;
pub const CUTSCENE_INTRO_PEACH: i32 = 142;
pub const CUTSCENE_DANCE_ROTATE: i32 = 143;
pub const CUTSCENE_ENTER_BOWSER_ARENA: i32 = 144;
pub const CUTSCENE_SLIDING_DOORS_OPEN: i32 = 149;
pub const CUTSCENE_PREPARE_CANNON: i32 = 150;
pub const CUTSCENE_UNLOCK_KEY_DOOR: i32 = 151;
pub const CUTSCENE_STANDING_DEATH: i32 = 152;
pub const CUTSCENE_DEATH_ON_STOMACH: i32 = 153;
pub const CUTSCENE_DEATH_ON_BACK: i32 = 154;
pub const CUTSCENE_QUICKSAND_DEATH: i32 = 155;
pub const CUTSCENE_SUFFOCATION_DEATH: i32 = 156;
pub const CUTSCENE_EXIT_BOWSER_SUCC: i32 = 157;
pub const CUTSCENE_EXIT_BOWSER_DEATH: i32 = 158;
pub const CUTSCENE_WATER_DEATH: i32 = 159;
pub const CUTSCENE_EXIT_PAINTING_SUCC: i32 = 160;
pub const CUTSCENE_CAP_SWITCH_PRESS: i32 = 161;
pub const CUTSCENE_DIALOG: i32 = 162;
pub const CUTSCENE_RACE_DIALOG: i32 = 163;
pub const CUTSCENE_ENTER_PYRAMID_TOP: i32 = 164;
pub const CUTSCENE_DANCE_FLY_AWAY: i32 = 165;
pub const CUTSCENE_DANCE_CLOSEUP: i32 = 166;
pub const CUTSCENE_KEY_DANCE: i32 = 167;
pub const CUTSCENE_EXIT_SPECIAL_SUCC: i32 = 169;
pub const CUTSCENE_NONPAINTING_DEATH: i32 = 170;
pub const CUTSCENE_READ_MESSAGE: i32 = 171;
pub const CUTSCENE_ENDING: i32 = 172;
pub const CUTSCENE_STAR_SPAWN: i32 = 173;
pub const CUTSCENE_GRAND_STAR: i32 = 174;
pub const CUTSCENE_DANCE_DEFAULT: i32 = 175;
pub const CUTSCENE_RED_COIN_STAR_SPAWN: i32 = 176;
pub const CUTSCENE_END_WAVING: i32 = 177;
pub const CUTSCENE_CREDITS: i32 = 178;
pub const CUTSCENE_EXIT_WATERFALL: i32 = 179;
pub const CUTSCENE_EXIT_FALL_WMOTR: i32 = 180;
pub const CUTSCENE_ENTER_POOL: i32 = 181;
/// `CUTSCENE_STOP` / `CUTSCENE_LOOP` (`gCutsceneTimer` is an s16: STOP reads back as -0x8000).
pub const CUTSCENE_STOP: i32 = -0x8000;
pub const CUTSCENE_LOOP: i32 = 0x7FFF;

/// `struct CutsceneVariable`
#[derive(Clone, Copy, Debug, Default)]
pub struct CsVar {
    pub point: V3,
    pub angle: [i32; 3],
}

/// An object the cutscene looks at (`gCutsceneFocus`): its position, home and hitbox height.
#[derive(Clone, Copy, Debug, Default)]
pub struct CutFocus {
    pub pos: V3,
    pub home: V3,
    pub hitbox_height: f64,
}

/// The rippling painting Mario enters (`gRipplingPainting`): position, yaw in degrees, size.
#[derive(Clone, Copy, Debug, Default)]
pub struct PaintingInfo {
    pub pos: V3,
    pub yaw_deg: f64,
    pub size: f64,
}

/// `sCameraStoreCutscene`
#[derive(Clone, Copy, Debug, Default)]
pub struct CsStore {
    pub pos: V3,
    pub focus: V3,
    pub pan_dist: f64,
    pub cannon_y_offset: f64,
}

/// Cutscene state (`gCutsceneTimer`, `sCutsceneShot`, `sCutsceneVars`, ...).
#[derive(Clone, Debug, Default)]
pub struct Cs {
    pub shot: usize,
    pub timer: i32,
    pub vars: [CsVar; 10],
    pub store: CsStore,
    pub focus: Option<CutFocus>,
    /// `gObjCutsceneDone`: the object that started the cutscene is finished (star spawn, red coin star).
    pub obj_done: bool,
    pub player2_focus_offset: V3,
    pub shot_timer: i32,
}

type Shot = fn(&mut SmCam, &Env);

impl SmCam {
    // ---- infrastructure ----

    /// `clear_cutscene_vars` + `start_cutscene`
    pub fn start_cutscene(&mut self, _e: &Env, cutscene: i32) {
        if self.c.cutscene != cutscene {
            self.c.cutscene = cutscene;
            self.cs.vars = [CsVar::default(); 10];
        }
    }

    /// `start_object_cutscene_without_focus`
    pub fn start_object_cutscene_without_focus(&mut self, cutscene: i32) {
        self.object_cutscene = cutscene;
    }

    /// `start_object_cutscene`: an object (star, cannon, ...) asks for a cutscene.
    pub fn start_object_cutscene(&mut self, cutscene: i32, focus: Option<CutFocus>) {
        self.object_cutscene = cutscene;
        self.recent_cutscene = 0;
        self.cs.focus = focus;
        self.cs.obj_done = false;
    }

    /// `cutscene_object`: returns 1 when started, -1 when it just ran, 0 otherwise.
    pub fn cutscene_object(&mut self, cutscene: i32, focus: Option<CutFocus>) -> i32 {
        if self.c.cutscene == 0 && self.object_cutscene == 0 {
            if self.recent_cutscene != cutscene {
                self.start_object_cutscene(cutscene, focus);
                1
            } else {
                -1
            }
        } else {
            0
        }
    }

    /// `cutscene_event` predicate: is the shot timer within [start, end] (end -1 = open).
    fn ev(&self, start: i32, end: i32) -> bool {
        start <= self.cs.timer && (end == -1 || end >= self.cs.timer)
    }

    fn table(cutscene: i32) -> &'static [(Shot, i32)] {
        match cutscene {
            CUTSCENE_STAR_SPAWN => &[(SmCam::cutscene_star_spawn, CUTSCENE_LOOP), (SmCam::cutscene_star_spawn_back, 15), (SmCam::cutscene_star_spawn_end, 0)],
            CUTSCENE_RED_COIN_STAR_SPAWN => &[(SmCam::cutscene_red_coin_star, CUTSCENE_LOOP), (SmCam::cutscene_red_coin_star_end, 0)],
            CUTSCENE_DOOR_WARP => &[(SmCam::cutscene_door_start, 1), (SmCam::cutscene_door_loop, CUTSCENE_LOOP)],
            CUTSCENE_DOOR_PULL => &[
                (SmCam::cutscene_door_start, 1),
                (SmCam::cutscene_door_fix_cam, 30),
                (SmCam::cutscene_door_move_behind_mario, 1),
                (SmCam::cutscene_door_follow_mario, 50),
                (SmCam::cutscene_door_end, 0),
            ],
            CUTSCENE_DOOR_PUSH => &[
                (SmCam::cutscene_door_start, 1),
                (SmCam::cutscene_door_fix_cam, 20),
                (SmCam::cutscene_door_move_behind_mario, 1),
                (SmCam::cutscene_door_follow_mario, 50),
                (SmCam::cutscene_door_end, 0),
            ],
            CUTSCENE_DOOR_PULL_MODE => &[(SmCam::cutscene_door_start, 1), (SmCam::cutscene_door_fix_cam, 30), (SmCam::cutscene_door_mode, CUTSCENE_LOOP)],
            CUTSCENE_DOOR_PUSH_MODE => &[(SmCam::cutscene_door_start, 1), (SmCam::cutscene_door_fix_cam, 20), (SmCam::cutscene_door_mode, CUTSCENE_LOOP)],
            CUTSCENE_ENTER_CANNON => &[(SmCam::cutscene_enter_cannon_start, 1), (SmCam::cutscene_enter_cannon_raise, 121), (SmCam::cutscene_enter_cannon_end, 0)],
            CUTSCENE_ENTER_PAINTING => &[(SmCam::cutscene_enter_painting, CUTSCENE_LOOP)],
            CUTSCENE_DEATH_EXIT => &[(SmCam::cutscene_exit_painting, 118), (SmCam::cutscene_exit_painting_end, 0)],
            CUTSCENE_EXIT_PAINTING_SUCC => &[(SmCam::cutscene_exit_painting, 180), (SmCam::cutscene_exit_painting_end, 0)],
            CUTSCENE_PREPARE_CANNON => &[(SmCam::cutscene_prepare_cannon, 170), (SmCam::cutscene_prepare_cannon_end, 0)],
            CUTSCENE_EXIT_WATERFALL => &[(SmCam::cutscene_exit_waterfall, 52), (SmCam::cutscene_exit_to_castle_grounds_end, 0)],
            CUTSCENE_EXIT_FALL_WMOTR => &[(SmCam::cutscene_exit_fall_to_castle_grounds, 73), (SmCam::cutscene_exit_to_castle_grounds_end, 0)],
            CUTSCENE_ENTER_PYRAMID_TOP => &[(SmCam::cutscene_enter_pyramid_top, 90), (SmCam::cutscene_exit_to_castle_grounds_end, 0)],
            CUTSCENE_STANDING_DEATH => &[(SmCam::cutscene_death_standing, CUTSCENE_LOOP)],
            CUTSCENE_ENTER_POOL => &[(SmCam::cutscene_enter_pool, 100), (SmCam::cutscene_exit_to_castle_grounds_end, 0)],
            CUTSCENE_DEATH_ON_STOMACH => &[(SmCam::cutscene_death_stomach, CUTSCENE_LOOP)],
            CUTSCENE_DEATH_ON_BACK => &[(SmCam::cutscene_bbh_death, CUTSCENE_LOOP)],
            CUTSCENE_QUICKSAND_DEATH => &[(SmCam::cutscene_quicksand_death, CUTSCENE_LOOP)],
            CUTSCENE_SUFFOCATION_DEATH => &[(SmCam::cutscene_suffocation, CUTSCENE_LOOP)],
            CUTSCENE_DANCE_ROTATE | CUTSCENE_DANCE_DEFAULT => &[(SmCam::cutscene_dance_default_rotate, CUTSCENE_LOOP)],
            CUTSCENE_DANCE_FLY_AWAY => &[(SmCam::cutscene_dance_fly_away, CUTSCENE_LOOP)],
            CUTSCENE_DANCE_CLOSEUP => &[(SmCam::cutscene_dance_closeup, CUTSCENE_LOOP)],
            CUTSCENE_KEY_DANCE => &[(SmCam::cutscene_key_dance, CUTSCENE_LOOP)],
            CUTSCENE_UNLOCK_KEY_DOOR => &[(SmCam::cutscene_unlock_key_door, 200), (SmCam::cutscene_double_doors_end, 0)],
            CUTSCENE_SLIDING_DOORS_OPEN => &[(SmCam::cutscene_sliding_doors_open, 50), (SmCam::cutscene_double_doors_end, 0)],
            CUTSCENE_EXIT_BOWSER_SUCC => &[(SmCam::cutscene_exit_bowser_succ, 190), (SmCam::cutscene_non_painting_end, 0)],
            CUTSCENE_EXIT_BOWSER_DEATH => &[(SmCam::cutscene_exit_bowser_death, 120), (SmCam::cutscene_non_painting_end, 0)],
            CUTSCENE_EXIT_SPECIAL_SUCC => &[(SmCam::cutscene_exit_non_painting_succ, 163), (SmCam::cutscene_non_painting_end, 0)],
            CUTSCENE_NONPAINTING_DEATH => &[(SmCam::cutscene_non_painting_death, 120), (SmCam::cutscene_non_painting_end, 0)],
            _ => &[],
        }
    }

    /// Is `cutscene` one of the sequences this port plays?
    pub fn cutscene_supported(cutscene: i32) -> bool { !Self::table(cutscene).is_empty() }

    /// `play_cutscene`
    pub fn play_cutscene(&mut self, e: &Env) {
        let old_cutscene = self.c.cutscene;
        self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
        self.movement_flags &= !CAM_MOVING_INTO_MODE;
        let table = Self::table(self.c.cutscene);
        let duration = match table.get(self.cs.shot) {
            Some(&(shot, dur)) => {
                shot(self, e);
                dur
            }
            None => 0,
        };
        if duration != 0 && self.cs.timer & 0x8000 == 0 {
            if self.cs.timer < 0x3FFF { self.cs.timer += 1; }
            if self.cs.timer == duration {
                self.cs.shot += 1;
                self.cs.timer = 0;
            }
        } else {
            self.camera_event = 0;
            self.cs.shot = 0;
            self.cs.timer = 0;
            // Sequences this port does not play end at once (the decomp would read garbage).
            if table.is_empty() { self.c.cutscene = 0; }
        }
        self.area_yaw_change = 0;
        if self.c.cutscene == 0 && old_cutscene != 0 {
            self.recent_cutscene = old_cutscene;
        }
    }

    /// `determine_dance_cutscene`
    pub fn determine_dance_cutscene(&self, e: &Env) -> i32 {
        let mut star_index = (e.last_star_num - 1) / 2;
        if !(0..=3).contains(&star_index) { star_index = 0; }
        let mut course = e.course_num;
        if course < 0 || course > 25 { course = 0; }
        let mut idx = DANCE_CUTSCENE_INDEX[course as usize][star_index as usize];
        if e.last_star_num & 1 != 0 { idx &= 0xF; } else { idx >>= 4; }
        match idx {
            0 => CUTSCENE_DANCE_FLY_AWAY,
            1 => CUTSCENE_DANCE_ROTATE,
            2 => CUTSCENE_DANCE_CLOSEUP,
            3 => CUTSCENE_KEY_DANCE,
            _ => CUTSCENE_DANCE_DEFAULT,
        }
    }

    fn open_door_cutscene(&self, pull: i32, push: i32) -> i32 {
        if self.mario.action == act::PULLING_DOOR { pull } else if self.mario.action == act::PUSHING_DOOR { push } else { 0 }
    }

    /// `get_cutscene_from_mario_status`
    pub fn get_cutscene_from_mario_status(&mut self, e: &Env) -> i32 {
        let mut cutscene = self.c.cutscene;
        if cutscene == 0 {
            cutscene = self.object_cutscene;
            self.object_cutscene = 0;
            let m = self.mario;
            if self.camera_event == CAM_EVENT_DOOR {
                cutscene = match self.curr_level_area {
                    AREA_CASTLE_LOBBY => {
                        if self.c.mode == CAMERA_MODE_SPIRAL_STAIRS || self.c.mode == CAMERA_MODE_CLOSE || self.c.door_status == DOOR_ENTER_LOBBY {
                            self.open_door_cutscene(CUTSCENE_DOOR_PULL_MODE, CUTSCENE_DOOR_PUSH_MODE)
                        } else {
                            self.open_door_cutscene(CUTSCENE_DOOR_PULL, CUTSCENE_DOOR_PUSH)
                        }
                    }
                    AREA_BBH => {
                        if self.c.door_status == DOOR_LEAVING_SPECIAL {
                            self.open_door_cutscene(CUTSCENE_DOOR_PULL, CUTSCENE_DOOR_PUSH)
                        } else {
                            self.open_door_cutscene(CUTSCENE_DOOR_PULL_MODE, CUTSCENE_DOOR_PUSH_MODE)
                        }
                    }
                    _ => self.open_door_cutscene(CUTSCENE_DOOR_PULL, CUTSCENE_DOOR_PUSH),
                };
            }
            if self.camera_event == CAM_EVENT_DOOR_WARP { cutscene = CUTSCENE_DOOR_WARP; }
            if self.camera_event == CAM_EVENT_CANNON { cutscene = CUTSCENE_ENTER_CANNON; }
            if surface_is_painting_warp(self.geo.curr_floor_type) { cutscene = CUTSCENE_ENTER_PAINTING; }
            let bowser_prev = matches!(self.prev_level, LEVEL_BOWSER_1 | LEVEL_BOWSER_2 | LEVEL_BOWSER_3);
            match m.action {
                act::DEATH_EXIT => cutscene = CUTSCENE_DEATH_EXIT,
                act::EXIT_AIRBORNE => cutscene = CUTSCENE_EXIT_PAINTING_SUCC,
                act::SPECIAL_EXIT_AIRBORNE => cutscene = if bowser_prev { CUTSCENE_EXIT_BOWSER_SUCC } else { CUTSCENE_EXIT_SPECIAL_SUCC },
                act::SPECIAL_DEATH_EXIT => cutscene = if bowser_prev { CUTSCENE_EXIT_BOWSER_DEATH } else { CUTSCENE_NONPAINTING_DEATH },
                act::ENTERING_STAR_DOOR => cutscene = if self.c.door_status == DOOR_DEFAULT { CUTSCENE_SLIDING_DOORS_OPEN } else { CUTSCENE_DOOR_PULL_MODE },
                act::UNLOCKING_KEY_DOOR => cutscene = CUTSCENE_UNLOCK_KEY_DOOR,
                act::WATER_DEATH => cutscene = CUTSCENE_WATER_DEATH,
                act::DEATH_ON_BACK => cutscene = CUTSCENE_DEATH_ON_BACK,
                act::DEATH_ON_STOMACH => cutscene = CUTSCENE_DEATH_ON_STOMACH,
                act::STANDING_DEATH => cutscene = CUTSCENE_STANDING_DEATH,
                act::SUFFOCATION => cutscene = CUTSCENE_SUFFOCATION_DEATH,
                act::QUICKSAND_DEATH => cutscene = CUTSCENE_QUICKSAND_DEATH,
                act::ELECTROCUTION => cutscene = CUTSCENE_STANDING_DEATH,
                act::STAR_DANCE_EXIT | act::STAR_DANCE_WATER => cutscene = self.determine_dance_cutscene(e),
                act::STAR_DANCE_NO_EXIT => cutscene = CUTSCENE_DANCE_DEFAULT,
                _ => {}
            }
            match self.camera_event {
                CAM_EVENT_START_INTRO => cutscene = CUTSCENE_INTRO_PEACH,
                CAM_EVENT_START_GRAND_STAR => cutscene = CUTSCENE_GRAND_STAR,
                CAM_EVENT_START_ENDING => cutscene = CUTSCENE_ENDING,
                CAM_EVENT_START_END_WAVING => cutscene = CUTSCENE_END_WAVING,
                CAM_EVENT_START_CREDITS => cutscene = CUTSCENE_CREDITS,
                _ => {}
            }
        }
        self.c.door_status = DOOR_DEFAULT;
        cutscene
    }

    // ---- helpers shared by shots ----

    fn reset_pan_distance(&mut self) { self.pan_distance = 0.0; }
    fn update_camera_yaw(&mut self) {
        self.c.next_yaw = calculate_yaw(self.c.focus, self.c.pos);
        self.c.yaw = self.c.next_yaw;
    }
    fn set_flag_post_door(&mut self) {
        self.status_flags |= CAM_FLAG_BEHIND_MARIO_POST_DOOR;
        self.camera_yaw_after_door_cutscene = calculate_yaw(self.c.focus, self.c.pos);
    }
    fn stop(&mut self) {
        self.cs.timer = CUTSCENE_STOP;
        self.c.cutscene = 0;
    }
    fn approach_vec3f(cur: &mut V3, target: V3, x: f64, y: f64, z: f64) {
        approach_f32_asymptotic_bool(&mut cur[0], target[0], x);
        approach_f32_asymptotic_bool(&mut cur[1], target[1], y);
        approach_f32_asymptotic_bool(&mut cur[2], target[2], z);
    }
    /// `set_focus_rel_mario`
    fn set_focus_rel_mario(&mut self, e: &Env, left_right: f64, y_off: f64, forw_back: f64, yaw_off: i32) {
        let (foc_floor_y_off, _) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        let yaw = w(self.mario.face_angle[1] + yaw_off);
        let m = self.mario.pos;
        use crate::math::{coss, sins};
        self.c.focus[2] = m[2] + forw_back * coss(yaw as f64) - left_right * sins(yaw as f64);
        self.c.focus[0] = m[0] + forw_back * sins(yaw as f64) + left_right * coss(yaw as f64);
        self.c.focus[1] = m[1] + y_off + foc_floor_y_off;
    }
    fn pan_camera(&mut self, inc_pitch: i32, inc_yaw: i32) {
        let (d, pitch, yaw) = dist_and_angle(self.c.pos, self.c.focus);
        self.c.focus = set_dist_and_angle(self.c.pos, d, w(pitch + inc_pitch), w(yaw + inc_yaw));
    }
    fn rotate_and_move_vec3f(to: &mut V3, from: V3, inc_dist: f64, inc_pitch: i32, inc_yaw: i32) {
        let (dist, pitch, yaw) = dist_and_angle(from, *to);
        *to = set_dist_and_angle(from, dist + inc_dist, w(pitch + inc_pitch), w(yaw + inc_yaw));
    }
    fn focus_in_front_of_mario(&mut self, dist: f64, speed: f64) {
        let goal = offset_rotated(self.mario.pos, [0.0, 100.0, dist], self.mario.face_angle);
        Self::approach_vec3f(&mut self.c.focus, goal, speed, speed, speed);
    }
    fn store_info_cannon(&mut self) {
        self.cs.store = CsStore { pos: self.c.pos, focus: self.c.focus, pan_dist: self.pan_distance, cannon_y_offset: self.cannon_y_offset };
    }
    fn retrieve_info_cannon(&mut self) {
        self.c.pos = self.cs.store.pos;
        self.c.focus = self.cs.store.focus;
        self.pan_distance = self.cs.store.pan_dist;
        self.cannon_y_offset = self.cs.store.cannon_y_offset;
    }
    fn store_info_star(&mut self) {
        self.reset_pan_distance();
        self.cs.store.pos = self.c.pos;
        self.cs.store.focus = [self.mario.pos[0], self.c.focus[1], self.mario.pos[2]];
    }
    fn retrieve_info_star(&mut self) {
        self.c.pos = self.cs.store.pos;
        self.c.focus = self.cs.store.focus;
    }

    /// `cutscene_goto_cvar_pos`
    fn cutscene_goto_cvar_pos(&mut self, goal_dist: f64, goal_pitch: i32, rot_pitch: i32, rot_yaw: i32) {
        let target = self.cs.vars[3].point;
        let (mut next_dist, mut next_pitch, mut next_yaw) = dist_and_angle(target, self.c.pos);
        if next_dist > 8000.0 && self.c.cutscene == CUTSCENE_PREPARE_CANNON {
            next_dist = goal_dist * 4.0;
            next_pitch = goal_pitch;
            self.cs.vars[0].point = target;
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            // (TTM only) nextYaw aimed away from the area centre; applies when the cannon is in TTM
            if self.curr_level_area / 16 == LEVEL_TTM {
                next_yaw = crate::math::atan2s(target[2] - self.c.area_cen[2], target[0] - self.c.area_cen[0]);
            }
        } else if self.c.cutscene == CUTSCENE_PREPARE_CANNON {
            let (mut cur_dist, mut cur_pitch, mut cur_yaw) = dist_and_angle(self.c.pos, self.cs.vars[0].point);
            let (cannon_dist, cannon_pitch, cannon_yaw) = dist_and_angle(self.c.pos, target);
            approach_f32_asymptotic_bool(&mut cur_dist, cannon_dist, 0.1);
            approach_s16_asymptotic_bool(&mut cur_pitch, cannon_pitch, 15);
            approach_s16_asymptotic_bool(&mut cur_yaw, cannon_yaw, 15);
            self.cs.vars[0].point = set_dist_and_angle(self.c.pos, cur_dist, cur_pitch, cur_yaw);
        } else {
            let mut v0 = self.cs.vars[0].point;
            Self::approach_vec3f(&mut v0, target, 0.1, 0.1, 0.1);
            self.cs.vars[0].point = v0;
        }
        approach_f32_asymptotic_bool(&mut next_dist, goal_dist, 0.05);
        approach_s16_asymptotic_bool(&mut next_pitch, goal_pitch, 0x20);
        self.c.pos = set_dist_and_angle(target, next_dist, next_pitch, next_yaw);
        self.c.focus = self.cs.vars[0].point;
        self.pan_camera(rot_pitch, rot_yaw);
        let (d, mut pitch, yaw) = dist_and_angle(self.c.pos, self.c.focus);
        pitch = pitch.clamp(-0x3000, 0x3000);
        self.c.focus = set_dist_and_angle(self.c.pos, d, pitch, yaw);
    }

    // ---- doors ----

    fn cutscene_door_start(&mut self, _e: &Env) {
        self.cs.vars[0].point = self.c.pos;
        self.cs.vars[1].point = self.c.focus;
    }
    fn cutscene_door_fix_cam(&mut self, _e: &Env) {
        self.c.pos = self.cs.vars[0].point;
        self.c.focus = self.cs.vars[1].point;
    }
    fn cutscene_door_loop(&mut self, _e: &Env) {
        if self.mario.action != act::PULLING_DOOR && self.mario.action != act::PUSHING_DOOR {
            self.stop();
        }
    }
    fn cutscene_door_move_behind_mario(&mut self, e: &Env) {
        self.reset_pan_distance();
        let door_rotation = if self.mario.action == act::PULLING_DOOR { 0 } else { degrees(-180) };
        self.set_focus_rel_mario(e, 0.0, 125.0, 0.0, 0);
        self.cs.vars[0].angle = [0, w(self.mario.face_angle[1] + door_rotation), 0];
        let cam_offset = [0.0, 125.0, 250.0];
        self.c.pos = offset_rotated(self.mario.pos, cam_offset, self.cs.vars[0].angle);
    }
    fn cutscene_door_follow_mario(&mut self, e: &Env) {
        self.set_focus_rel_mario(e, 0.0, 125.0, 0.0, 0);
        let (mut dist, pitch, yaw) = dist_and_angle(self.c.focus, self.c.pos);
        approach_f32_symmetric_bool(&mut dist, 150.0, 7.0);
        self.c.pos = set_dist_and_angle(self.c.focus, dist, pitch, yaw);
        self.update_camera_yaw();
    }
    fn cutscene_door_end(&mut self, _e: &Env) {
        self.c.mode = if self.c.def_mode == CAMERA_MODE_FREE_ROAM { CAMERA_MODE_FREE_ROAM } else { CAMERA_MODE_CLOSE };
        self.c.cutscene = 0;
        self.cs.timer = CUTSCENE_STOP;
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.status_flags &= !CAM_FLAG_BLOCK_SMOOTH_MOVEMENT;
        self.set_flag_post_door();
        self.update_camera_yaw();
    }
    fn cutscene_door_mode(&mut self, e: &Env) {
        self.reset_pan_distance();
        self.camera_course_processing(e);
        if self.c.mode == CAMERA_MODE_FIXED {
            let mut f = self.c.focus;
            self.c.next_yaw = self.update_fixed_camera(e, &mut f);
            self.c.focus = f;
        }
        if self.c.mode == CAMERA_MODE_PARALLEL_TRACKING && !self.par_track_path.is_empty() {
            let mut f = self.c.focus;
            let mut p = self.c.pos;
            self.c.next_yaw = self.update_parallel_tracking_camera(e, &mut f, &mut p, true);
            self.c.focus = f;
            self.c.pos = p;
        }
        self.c.yaw = self.c.next_yaw;
        let a = self.mario.action;
        if a != act::ENTERING_STAR_DOOR && a != act::PULLING_DOOR && a != act::PUSHING_DOOR {
            self.stop();
        }
    }

    fn cutscene_sliding_doors_open(&mut self, e: &Env) {
        self.reset_pan_distance();
        if self.ev(0, 8) {
            let (mut dist, mut pitch, mut yaw) = dist_and_angle(self.mario.pos, self.c.pos);
            if dist < 500.0 {
                dist = 500.0;
                yaw = w(self.mario.face_angle[1] + 0x8800);
                pitch = 0x800;
            }
            self.c.pos = set_dist_and_angle(self.mario.pos, dist, pitch, yaw);
        }
        if self.ev(8, 8) {
            self.cs.vars[1].point = self.mario.pos;
            self.cs.vars[0].angle = self.mario.face_angle;
            self.cs.vars[0].point = [80.0, 325.0, 200.0];
        }
        if self.ev(8, 28) { approach_f32_symmetric_bool(&mut self.cs.vars[0].point[1], 75.0, 10.0); }
        if self.ev(29, -1) { approach_f32_symmetric_bool(&mut self.cs.vars[0].point[1], 125.0, 10.0); }
        if self.ev(8, -1) {
            self.cs.vars[1].point[0] = self.mario.pos[0];
            self.cs.vars[1].point[2] = self.mario.pos[2];
            approach_f32_asymptotic_bool(&mut self.cs.vars[0].point[0], 0.0, 0.1);
            approach_f32_symmetric_bool(&mut self.cs.vars[0].point[2], 125.0, 50.0);
            let fa = self.mario.face_angle;
            for i in 0..3 { approach_s16_asymptotic_bool(&mut self.cs.vars[0].angle[i], fa[i], 16); }
            let pos = offset_rotated(self.cs.vars[1].point, self.cs.vars[0].point, self.cs.vars[0].angle);
            Self::approach_vec3f(&mut self.c.pos, pos, 0.15, 0.05, 0.15);
            self.set_focus_rel_mario(e, 0.0, 125.0, 0.0, 0);
        }
    }
    /// `cutscene_unlock_key_door` (the basement / upstairs key doors): the camera moves in on Mario and the lock (cvar2 / cvar3 are
    /// the goal pos and focus relative to Mario), the focus rises to the key from frame 70, the fov shakes when the key turns
    /// (frame 79, preset 1) and from frame 124 it flies back to the stored pos and focus (cvar0 / cvar1).
    fn cutscene_unlock_key_door(&mut self, _e: &Env) {
        if self.ev(0, 0) {
            self.cs.vars[0].point = self.c.pos;
            self.cs.vars[1].point = self.c.focus;
            self.cs.vars[2].point = offset_rotated(self.mario.pos, [-206.0, 108.0, 234.0], self.mario.face_angle);
            self.cs.vars[3].point = offset_rotated(self.mario.pos, [48.0, 104.0, -193.0], self.mario.face_angle);
        }
        if self.ev(0, 123) {
            let (pos, focus) = (self.cs.vars[2].point, self.cs.vars[3].point);
            Self::approach_vec3f(&mut self.c.pos, pos, 0.1, 0.1, 0.1);
            Self::approach_vec3f(&mut self.c.focus, focus, 0.1, 0.1, 0.1);
        }
        if self.ev(124, -1) {
            let (pos, focus) = (self.cs.vars[0].point, self.cs.vars[1].point);
            Self::approach_vec3f(&mut self.c.pos, pos, 0.1, 0.1, 0.1);
            Self::approach_vec3f(&mut self.c.focus, focus, 0.1, 0.1, 0.1);
        }
        // cutscene_set_fov_shake_preset(1)
        if self.ev(79, 79) { self.set_fov_shake(0x100, 0x30, 0x8000); }
        if self.ev(70, 110) {
            let y = self.mario.pos[1] + 140.0;
            approach_f32_asymptotic_bool(&mut self.cs.vars[3].point[1], y, 0.07);
        }
    }
    fn cutscene_double_doors_end(&mut self, _e: &Env) {
        self.set_flag_post_door();
        self.c.cutscene = 0;
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
    }

    // ---- cannon ----

    fn cutscene_enter_cannon_start(&mut self, _e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.camera_event = 0;
        if let Some(o) = self.mario.used_obj {
            self.cs.vars[0].point = o.pos;
            self.cs.vars[0].angle = o.move_angle;
        }
        let (d, p, y) = dist_and_angle(self.cs.vars[0].point, self.c.pos);
        self.cs.vars[1].point[2] = d;
        self.cs.vars[1].angle[0] = p;
        self.cs.vars[1].angle[1] = y;
        self.cs.vars[3].point[1] = 0.0;
        self.cs.vars[4].point[1] = 0.0;
    }
    fn cutscene_enter_cannon_raise(&mut self, e: &Env) {
        if self.ev(70, 70) {
            self.set_environmental_camera_shake(SHAKE_ENV_EXPLOSION);
            self.set_fov_shake(0x100, 0x30, 0x8000);
        }
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        approach_s16_symmetric_bool(&mut self.cs.vars[1].angle[0], 0, 0x80);
        approach_s16_symmetric_bool(&mut self.cs.vars[2].angle[0], 0, 0x80);
        self.c.pos = set_dist_and_angle(self.cs.vars[0].point, self.cs.vars[1].point[2], self.cs.vars[1].angle[0], self.cs.vars[1].angle[1]);
        self.cs.vars[1].point[2] = approach_f32(self.cs.vars[1].point[2], 400.0, 5.0, 5.0);
        self.cs.vars[1].angle[1] = w(self.cs.vars[1].angle[1] + 0x40);
        self.cs.vars[3].point[1] += 2.0;
        self.c.pos[1] += self.cs.vars[3].point[1];
        if let Some(o) = self.mario.used_obj {
            self.cs.vars[0].point[1] = o.pos[1];
            self.c.focus = offset_rotated(o.pos, [0.0, 100.0, 0.0], o.move_angle);
        }
        let floor_height = self.find_floor(e, self.c.pos[0], self.c.pos[1] + 500.0, self.c.pos[2]).0 + 100.0;
        if self.c.pos[1] < floor_height { self.c.pos[1] = floor_height; }
    }
    fn cutscene_enter_cannon_end(&mut self, _e: &Env) {
        self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
        self.status_flags |= CAM_FLAG_BLOCK_SMOOTH_MOVEMENT;
        self.c.mode = CAMERA_MODE_INSIDE_CANNON;
        self.c.cutscene = 0;
        self.cannon_y_offset = 800.0;
    }

    fn cutscene_prepare_cannon(&mut self, _e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        if self.ev(0, 0) {
            self.store_info_cannon();
            self.cs.vars[0].point = self.c.focus;
            self.cs.vars[2].point[0] = 30.0;
            self.cs.vars[3].point = self.cs.focus.map_or(self.cs.vars[3].point, |f| f.pos);
            self.cs.vars[5].angle = [0; 3];
        }
        if self.ev(0, 140) {
            let ry = self.cs.vars[5].angle[1];
            self.cutscene_goto_cvar_pos(300.0, 0x2000, 0, ry);
            approach_s16_symmetric_bool(&mut self.cs.vars[5].angle[1], 0x400, 17);
            self.set_handheld_shake(HAND_CAM_SHAKE_CUTSCENE);
        }
        if self.ev(141, -1) {
            let prev = self.cs.store;
            if abs_dist(self.c.pos, prev.pos) < 8000.0 {
                let n = self.cs.vars[2].point[0];
                let appr = |value: &mut f64, target: f64| {
                    let inc = (target - *value).abs() / n;
                    approach_f32_symmetric_bool(value, target, inc);
                };
                for i in 0..3 { appr(&mut self.c.pos[i], prev.pos[i]); }
                for i in 0..3 { appr(&mut self.c.focus[i], prev.focus[i]); }
            } else {
                self.c.focus = prev.focus;
                self.c.pos = prev.pos;
                self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            }
            if self.cs.vars[2].point[0] > 1.0 { self.cs.vars[2].point[0] -= 1.0; }
        }
    }
    fn cutscene_prepare_cannon_end(&mut self, _e: &Env) {
        self.cs.timer = CUTSCENE_STOP;
        self.c.cutscene = 0;
        self.retrieve_info_cannon();
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
    }

    // ---- paintings ----

    fn cutscene_enter_painting(&mut self, e: &Env) {
        self.set_fov_function(CAM_FOV_APP_20);
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        if let Some(p) = e.painting {
            let angle = [0, s16(p.yaw_deg / 360.0 * 65536.0), 0];
            let mut focus_offset = [p.size / 2.0, p.size / 2.0, 0.0];
            let focus = offset_rotated(p.pos, focus_offset, angle);
            Self::approach_vec3f(&mut self.c.focus, focus, 0.1, 0.1, 0.1);
            focus_offset[2] = -(((p.size * 1000.0) / 2.0) / 307.0);
            let mut focus = offset_rotated(p.pos, focus_offset, angle);
            let floor_height = self.find_floor(e, focus[0], focus[1] + 500.0, focus[2]).0 + 125.0;
            if focus[1] < floor_height { focus[1] = floor_height; }
            if self.c.cutscene == CUTSCENE_ENTER_PAINTING {
                Self::approach_vec3f(&mut self.c.pos, focus, 0.2, 0.1, 0.2);
            } else {
                Self::approach_vec3f(&mut self.c.pos, focus, 0.9, 0.9, 0.9);
            }
            let m = self.mario.pos;
            let floor = self.find_floor(e, m[0], m[1] + 50.0, m[2]).1;
            if floor.map_or(true, |f| !(0xA6..=0xF9).contains(&f.kind)) {
                self.c.cutscene = 0;
                self.cs.timer = CUTSCENE_STOP;
                self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
            }
        }
        self.c.mode = CAMERA_MODE_CLOSE;
    }
    fn cutscene_exit_painting(&mut self, e: &Env) {
        if self.ev(0, 0) {
            self.cs.vars[2].point = [258.0, -352.0, 1189.0];
            self.cs.vars[1].point = [65.0, -155.0, 444.0];
            if self.prev_level == LEVEL_TTM {
                self.cs.vars[1].point[1] = 0.0;
                self.cs.vars[1].point[2] = 0.0;
            }
            self.cs.vars[0].point = self.mario.pos;
            self.cs.vars[0].angle = [0, self.mario.face_angle[1], 0];
            self.c.focus = offset_rotated(self.cs.vars[0].point, self.cs.vars[1].point, self.cs.vars[0].angle);
            self.c.pos = offset_rotated(self.cs.vars[0].point, self.cs.vars[2].point, self.cs.vars[0].angle);
            let mut fh = self.find_floor(e, self.c.pos[0], self.c.pos[1] + 10.0, self.c.pos[2]).0;
            if fh != FLOOR_LOWER_LIMIT {
                fh += 60.0;
                if self.c.pos[1] < fh { self.c.pos[1] = fh; }
            }
        }
        if self.ev(5, -1) {
            approach_f32_asymptotic_bool(&mut self.cs.vars[2].point[0], 178.0, 0.05);
            approach_f32_asymptotic_bool(&mut self.cs.vars[2].point[2], 889.0, 0.05);
            let pos = offset_rotated(self.cs.vars[0].point, self.cs.vars[2].point, self.cs.vars[0].angle);
            self.c.pos[0] = pos[0];
            self.c.pos[2] = pos[2];
            let m = self.mario.pos;
            let (fh, floor) = self.find_floor(e, m[0], m[1] + 10.0, m[2]);
            if floor.is_some() {
                let target = [m[0], fh + (m[1] - fh) * 0.7 + 125.0, m[2]];
                Self::approach_vec3f(&mut self.c.focus, target, 0.2, 0.2, 0.2);
                if target[1] < self.c.pos[1] {
                    approach_f32_asymptotic_bool(&mut self.c.pos[1], target[1], 0.05);
                }
            }
        }
        if self.prev_level == LEVEL_TTM {
            self.c.pos = [-296.0, 1261.0, 3521.0];
        }
        self.update_camera_yaw();
    }
    fn cutscene_exit_painting_end(&mut self, _e: &Env) {
        self.c.mode = CAMERA_MODE_CLOSE;
        self.c.cutscene = 0;
        self.cs.timer = CUTSCENE_STOP;
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.status_flags &= !CAM_FLAG_BLOCK_SMOOTH_MOVEMENT;
        self.update_camera_yaw();
    }

    // ---- exits to the castle grounds / pool / pyramid ----

    fn focus_mario_castle_grounds(&mut self) {
        self.c.focus = self.mario.pos;
        self.c.focus[1] = self.c.pos[1] + (self.mario.pos[1] + 125.0 - self.c.pos[1]) * 0.5;
        let m = self.mario.pos;
        Self::approach_vec3f(&mut self.c.focus, m, 0.05, 0.4, 0.05);
    }
    fn cutscene_exit_waterfall(&mut self, _e: &Env) {
        if self.ev(0, 0) { self.c.pos = [-3899.0, 39.0, -5671.0]; }
        self.focus_mario_castle_grounds();
        self.update_camera_yaw();
    }
    fn cutscene_exit_to_castle_grounds_end(&mut self, _e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.cs.timer = CUTSCENE_STOP;
        self.c.cutscene = 0;
        self.update_camera_yaw();
    }
    fn cutscene_exit_fall_to_castle_grounds(&mut self, _e: &Env) {
        if self.ev(0, 0) { self.c.pos = [5830.0, 32.0, 3985.0]; }
        self.focus_mario_castle_grounds();
        self.update_camera_yaw();
    }
    fn cutscene_enter_pool(&mut self, _e: &Env) {
        if self.ev(0, 0) {
            self.cs.vars[3].point = self.mario.pos;
            if self.curr_level_area / 16 == LEVEL_CASTLE { self.cs.vars[3].point = [2485.0, -1589.0, -2659.0]; }
            if self.curr_level_area / 16 == LEVEL_HMC { self.cs.vars[3].point = [3350.0, -4589.0, 4800.0]; }
            self.cs.vars[0].point = self.c.focus;
        }
        self.cutscene_goto_cvar_pos(1200.0, 0x2000, 0x200, 0);
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
    }
    fn cutscene_enter_pyramid_top(&mut self, e: &Env) {
        if self.ev(0, 0) {
            self.cs.vars[0].point = self.c.focus;
            self.cs.vars[3].point = [self.c.area_cen[0], 1280.0, self.c.area_cen[2]];
        }
        self.cutscene_goto_cvar_pos(200.0, 0x3000, 0, 0);
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.set_handheld_shake(HAND_CAM_SHAKE_CUTSCENE);
        if self.mario.pos[1] > 1250.0 {
            self.cutscene_exit_to_castle_grounds_end(e);
        }
    }

    // ---- deaths ----

    fn cutscene_death_standing(&mut self, _e: &Env) {
        if self.ev(0, 0) {
            self.cs.vars[0].point = self.c.focus;
            self.cs.vars[3].point = self.mario.pos;
            self.cs.vars[3].point[1] += 70.0;
        }
        self.cutscene_goto_cvar_pos(400.0, 0x1000, 0x300, 0);
        self.set_handheld_shake(HAND_CAM_SHAKE_HIGH);
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
    }
    fn death_stomach_start(&mut self) {
        self.cs.vars[3].point = offset_rotated(self.mario.pos, [0.0, 40.0, -60.0], self.mario.face_angle);
        self.cs.vars[0].point = self.c.focus;
    }
    fn cutscene_death_stomach(&mut self, _e: &Env) {
        if self.ev(0, 0) { self.death_stomach_start(); }
        self.cutscene_goto_cvar_pos(400.0, 0x1800, 0, -0x400);
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.set_handheld_shake(HAND_CAM_SHAKE_CUTSCENE);
    }
    fn cutscene_bbh_death(&mut self, _e: &Env) {
        if self.ev(0, 0) {
            self.cs.vars[3].point = offset_rotated(self.mario.pos, [0.0, 40.0, 60.0], self.mario.face_angle);
            self.cs.vars[0].point = self.c.focus;
        }
        self.cutscene_goto_cvar_pos(400.0, 0x1800, 0, 0x400);
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.set_handheld_shake(HAND_CAM_SHAKE_CUTSCENE);
    }
    fn cutscene_quicksand_death(&mut self, _e: &Env) {
        let m = self.mario.pos;
        self.cs.vars[3].point = [m[0], m[1] + 20.0, m[2]];
        if self.ev(0, 0) { self.cs.vars[0].point = self.c.focus; }
        self.cutscene_goto_cvar_pos(400.0, 0x2800, 0x200, 0);
        if self.c.cutscene == CUTSCENE_WATER_DEATH {
            let (dist, pitch, mut yaw) = dist_and_angle(m, self.c.pos);
            approach_s16_asymptotic_bool(&mut yaw, w(self.mario.face_angle[1] - 0x3000), 8);
            self.c.pos = set_dist_and_angle(m, dist, pitch, yaw);
        }
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.set_handheld_shake(HAND_CAM_SHAKE_HIGH);
    }
    fn cutscene_suffocation(&mut self, _e: &Env) {
        if self.ev(0, 0) { self.death_stomach_start(); }
        let m = self.mario.pos;
        let (dist, pitch, yaw) = dist_and_angle(m, self.c.pos);
        self.c.pos = set_dist_and_angle(m, dist, pitch, w(yaw + 0x100));
        self.cutscene_goto_cvar_pos(400.0, 0x2800, 0x200, 0);
        if self.ev(50, -1) {
            let target = offset_rotated(m, [0.0, 20.0, 120.0], self.mario.face_angle);
            let mut v3 = self.cs.vars[3].point;
            Self::approach_vec3f(&mut v3, target, 0.1, 0.1, 0.1);
            self.cs.vars[3].point = v3;
        }
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.set_handheld_shake(HAND_CAM_SHAKE_HIGH);
    }

    // ---- exits from a level through a non-painting warp ----

    fn exit_succ_start(&mut self) {
        self.cs.vars[7].point = self.mario.pos;
        self.cs.vars[7].angle = self.mario.face_angle;
        self.cs.vars[6].point = [6.0, 363.0, 543.0];
        self.cs.vars[5].point = [137.0, 226.0, 995.0];
    }
    fn non_painting_set_cam_pos(&mut self, e: &Env) {
        match self.prev_level {
            LEVEL_HMC | LEVEL_COTMC => self.c.pos = [3465.0, -1008.0, -2961.0],
            LEVEL_RR => self.c.pos = [-3741.0, 3151.0, 6065.0],
            LEVEL_WMOTR => self.c.pos = [1972.0, 3230.0, 5891.0],
            _ => {
                self.c.pos = offset_rotated(self.cs.vars[7].point, self.cs.vars[5].point, self.cs.vars[7].angle);
                self.c.pos[1] = self.find_floor(e, self.c.pos[0], self.c.pos[1] + 1000.0, self.c.pos[2]).0 + 125.0;
            }
        }
    }
    fn non_painting_set_cam_focus(&mut self) {
        self.c.focus = offset_rotated(self.cs.vars[7].point, self.cs.vars[6].point, self.cs.vars[7].angle);
        let (p, m) = (self.c.pos, self.mario.pos);
        if matches!(self.prev_level, LEVEL_COTMC | LEVEL_HMC | LEVEL_RR | LEVEL_WMOTR) {
            self.c.focus = [p[0] + (m[0] - p[0]) * 0.7, p[1] + (m[1] - p[1]) * 0.4, p[2] + (m[2] - p[2]) * 0.7];
        } else {
            self.c.focus[1] = p[1] + (m[1] - p[1]) * 0.2;
        }
    }
    fn cutscene_exit_bowser_succ(&mut self, e: &Env) {
        if self.ev(0, 0) { self.exit_succ_start(); }
        self.non_painting_set_cam_pos(e);
        if self.ev(18, -1) { approach_f32_asymptotic_bool(&mut self.cs.vars[6].point[0], -24.0, 0.05); }
        self.non_painting_set_cam_focus();
        if self.ev(125, 125) && self.c.cutscene == CUTSCENE_EXIT_BOWSER_SUCC { self.set_camera_pitch_shake(0x800, 0x40, 0x800); }
        if self.ev(41, 41) { self.set_environmental_camera_shake(SHAKE_ENV_EXPLOSION); }
    }
    fn cutscene_exit_non_painting_succ(&mut self, e: &Env) {
        if self.ev(0, 0) { self.exit_succ_start(); }
        if self.ev(0, 0) { self.cs.vars[5].point = [137.0, 246.0, 1115.0]; }
        self.non_painting_set_cam_pos(e);
        if self.ev(18, -1) { approach_f32_asymptotic_bool(&mut self.cs.vars[6].point[0], -24.0, 0.05); }
        self.non_painting_set_cam_focus();
        if self.ev(41, 41) { self.set_environmental_camera_shake(SHAKE_ENV_EXPLOSION); }
        self.update_camera_yaw();
    }
    fn non_painting_death_start(&mut self) {
        self.cs.vars[7].point = self.mario.pos;
        self.cs.vars[7].angle = self.mario.face_angle;
        self.cs.vars[6].point = [-42.0, 350.0, 727.0];
        self.cs.vars[5].point = [107.0, 226.0, 1187.0];
    }
    fn cutscene_exit_bowser_death(&mut self, e: &Env) {
        if self.ev(0, 0) { self.non_painting_death_start(); }
        self.non_painting_set_cam_pos(e);
        self.non_painting_set_cam_focus();
    }
    fn cutscene_non_painting_death(&mut self, e: &Env) {
        if self.ev(0, 0) {
            self.non_painting_death_start();
            self.cs.vars[5].point = match self.prev_level {
                LEVEL_HMC | LEVEL_COTMC => [187.0, 369.0, -197.0],
                _ => [107.0, 246.0, 1307.0],
            };
        }
        self.non_painting_set_cam_pos(e);
        self.non_painting_set_cam_focus();
        self.status_flags |= CAM_FLAG_UNUSED_CUTSCENE_ACTIVE;
    }
    fn cutscene_non_painting_end(&mut self, _e: &Env) {
        self.c.cutscene = 0;
        self.c.mode = if self.c.def_mode == CAMERA_MODE_CLOSE { CAMERA_MODE_CLOSE } else { CAMERA_MODE_FREE_ROAM };
        self.status_flags |= CAM_FLAG_UNUSED_CUTSCENE_ACTIVE;
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.transition_next_state(60);
        self.update_camera_yaw();
    }

    // ---- star spawn ----

    fn cutscene_star_spawn(&mut self, _e: &Env) {
        if self.ev(0, 0) { self.store_info_star(); }
        if let Some(f) = self.cs.focus {
            let star = [f.pos[0], f.pos[1] + f.hitbox_height, f.pos[2]];
            Self::approach_vec3f(&mut self.c.focus, star, 0.1, 0.1, 0.1);
        }
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        if self.cs.obj_done { self.cs.timer = CUTSCENE_LOOP; }
    }
    fn cutscene_star_spawn_back(&mut self, e: &Env) {
        if self.c.mode == CAMERA_MODE_BOSS_FIGHT && self.set_cam_angle(0) == CAM_ANGLE_LAKITU {
            let mut f = [0.0; 3];
            let mut p = [0.0; 3];
            self.update_boss_fight_camera(e, &mut f, &mut p);
            Self::approach_vec3f(&mut self.c.focus, f, 0.2, 0.2, 0.2);
            Self::approach_vec3f(&mut self.c.pos, p, 0.2, 0.2, 0.2);
        } else if self.ev(0, 0) {
            self.retrieve_info_star();
            self.transition_next_state(15);
        }
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.status_flags |= CAM_FLAG_UNUSED_CUTSCENE_ACTIVE;
    }
    fn cutscene_star_spawn_end(&mut self, _e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.cs.timer = CUTSCENE_STOP;
        self.c.cutscene = 0;
    }
    fn cutscene_red_coin_star(&mut self, _e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        let f = self.cs.focus.unwrap_or_default();
        if self.ev(0, 0) {
            self.cs.vars[1].point = f.pos;
            self.store_info_star();
            self.cs.vars[2].point[2] = self.fov.fov;
        }
        if self.ev(30, 30) {
            let home = f.home;
            self.cs.vars[1].point = home;
            let pos_yaw = calculate_yaw(home, self.c.pos);
            let mut yaw = calculate_yaw(home, self.mario.pos);
            if w(yaw - pos_yaw + degrees(90)).abs() < w(yaw - pos_yaw - degrees(90)).abs() {
                yaw = w(yaw + degrees(90));
            } else {
                yaw = w(yaw - degrees(90));
            }
            self.c.pos = set_dist_and_angle(home, 400.0, 0x1000, yaw);
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
        }
        if self.ev(0, -1) {
            approach_f32_asymptotic_bool(&mut self.c.focus[0], f.pos[0], 0.15);
            approach_f32_asymptotic_bool(&mut self.c.focus[2], f.pos[2], 0.15);
        }
        if self.ev(0, 29) { approach_f32_asymptotic_bool(&mut self.c.focus[1], f.pos[1], 0.1); }
        if self.ev(30, -1) {
            let v1 = self.cs.vars[1].point[1];
            self.c.focus[1] = v1 + (f.pos[1] - v1) * 0.8;
            self.fov.fov = 60.0;
        }
        if self.cs.obj_done { self.cs.timer = CUTSCENE_LOOP; }
    }
    fn cutscene_red_coin_star_end(&mut self, _e: &Env) {
        self.retrieve_info_star();
        self.cs.timer = CUTSCENE_STOP;
        self.c.cutscene = 0;
        self.fov.fov = self.cs.vars[2].point[2];
    }

    // ---- star dances ----

    fn star_dance_bound_yaw(&mut self, abs_yaw: i32, yaw_max: i32) {
        let (_d, _p, yaw) = dist_and_angle(self.mario.pos, self.c.pos);
        let mut yaw_from_abs = w(yaw - abs_yaw);
        if yaw_from_abs & 0x8000 != 0 { yaw_from_abs = -yaw_from_abs; }
        if yaw_from_abs > yaw_max {
            self.c.next_yaw = abs_yaw;
            self.c.yaw = abs_yaw;
        }
    }
    fn dance_move_to_mario(&mut self) {
        let (mut dist, mut pitch, yaw) = dist_and_angle(self.mario.pos, self.c.pos);
        approach_f32_asymptotic_bool(&mut dist, 600.0, 0.3);
        approach_s16_asymptotic_bool(&mut pitch, 0x1000, 0x10);
        self.c.pos = set_dist_and_angle(self.mario.pos, dist, pitch, yaw);
    }
    fn cutscene_dance_default_rotate(&mut self, _e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        self.yaw_speed = 0;
        self.set_fov_function(CAM_FOV_DEFAULT);
        if self.ev(0, 20) { self.focus_in_front_of_mario(-100.0, 0.2); }
        if self.ev(0, 39) { self.dance_move_to_mario(); }
        if self.ev(40, 40) { self.set_fov_shake(0x200, 0x28, 0x8000); }
        if self.c.cutscene != CUTSCENE_DANCE_DEFAULT {
            let m = self.mario.pos;
            if self.ev(75, 102) { self.focus_in_front_of_mario(-200.0, 0.03); }
            if self.ev(50, -1) { Self::rotate_and_move_vec3f(&mut self.c.pos, m, 0.0, 0, 0x200); }
            if self.ev(50, 80) { Self::rotate_and_move_vec3f(&mut self.c.pos, m, -15.0, 0, 0); }
            if self.ev(70, 90) { Self::rotate_and_move_vec3f(&mut self.c.pos, m, 20.0, 0, 0); }
        } else {
            let a = self.mario.action;
            if a != act::STAR_DANCE_NO_EXIT && a != act::STAR_DANCE_WATER && a != act::STAR_DANCE_EXIT {
                self.cs.timer = CUTSCENE_STOP;
                self.c.cutscene = 0;
                self.transition_next_state(20);
                self.status_flags |= CAM_FLAG_UNUSED_CUTSCENE_ACTIVE;
            }
        }
    }
    fn cutscene_dance_closeup(&mut self, e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        let water = self.mario.action == act::STAR_DANCE_WATER;
        let (fly_above_end, closer_start, shake_at) = if water { (62, 63, 70) } else { (32, 33, 40) };
        if self.ev(0, 0) {
            if e.last_star_num == 4 && e.course_num == 3 { self.star_dance_bound_yaw(0x0, 0x4000); }
            if e.last_star_num == 1 && e.course_num == 9 { self.star_dance_bound_yaw(0x8000u32 as i32 - 0x10000, 0x5000); }
            if e.last_star_num == 5 && e.course_num == 11 { self.star_dance_bound_yaw(0x8000u32 as i32 - 0x10000, 0x800); }
            self.cs.vars[9].point = self.c.focus;
            self.cs.vars[8].angle[0] = 0x2000;
        }
        if self.ev(0, -1) {
            let m = self.mario.pos;
            let mut p = self.cs.vars[9].point;
            Self::approach_vec3f(&mut p, [m[0], m[1] + 125.0, m[2]], 0.2, 0.2, 0.2);
            self.cs.vars[9].point = p;
            self.c.focus = p;
        }
        if self.ev(0, fly_above_end) {
            let mut goal_pitch = 0x1800;
            if (e.last_star_num == 6 && e.course_num == 10) || (e.last_star_num == 4 && e.course_num == 14) { goal_pitch = 0x800; }
            let (mut dist, mut pitch, mut yaw) = dist_and_angle(self.mario.pos, self.c.pos);
            approach_f32_asymptotic_bool(&mut dist, 800.0, 0.05);
            approach_s16_asymptotic_bool(&mut pitch, goal_pitch, 16);
            approach_s16_asymptotic_bool(&mut yaw, self.c.yaw, 8);
            self.c.pos = set_dist_and_angle(self.mario.pos, dist, pitch, yaw);
        }
        if self.ev(closer_start, -1) {
            let (mut dist, mut pitch, mut yaw) = dist_and_angle(self.mario.pos, self.c.pos);
            approach_f32_asymptotic_bool(&mut dist, 240.0, 0.4);
            approach_s16_asymptotic_bool(&mut yaw, self.c.yaw, 8);
            approach_s16_asymptotic_bool(&mut pitch, 0x1000, 5);
            self.c.pos = set_dist_and_angle(self.mario.pos, dist, pitch, yaw);
        }
        if self.ev(closer_start, closer_start) { self.set_fov_function(CAM_FOV_APP_80); }
        if self.ev(shake_at, shake_at) { self.set_fov_shake(0x300, 0x30, 0x8000); }
        self.set_handheld_shake(HAND_CAM_SHAKE_CUTSCENE);
    }
    fn cutscene_dance_fly_away(&mut self, e: &Env) {
        self.status_flags |= CAM_FLAG_SMOOTH_MOVEMENT;
        if self.ev(0, 0) {
            self.cs.vars[9].point = self.c.focus;
            self.cs.vars[8].point[2] = 65.0;
            if self.c.mode == CAMERA_MODE_RADIAL {
                self.c.yaw = calculate_yaw(self.c.area_cen, self.c.pos);
                self.c.next_yaw = self.c.yaw;
            }
            if e.last_star_num == 6 && e.course_num == 4 { self.star_dance_bound_yaw(0x5600, 0x800); }
            if e.last_star_num == 2 && e.course_num == 12 { self.star_dance_bound_yaw(0x0, 0x800); }
            if e.last_star_num == 1 && e.course_num == 10 { self.star_dance_bound_yaw(0x2000, 0x800); }
            if e.last_star_num == 3 && e.course_num == 15 { self.star_dance_bound_yaw(0x0, 0x800); }
        }
        if self.ev(0, 30) {
            let m = self.mario.pos;
            let mut p = self.cs.vars[9].point;
            Self::approach_vec3f(&mut p, [m[0], m[1] + 125.0, m[2]], 0.2, 0.2, 0.2);
            self.cs.vars[9].point = p;
            self.c.focus = p;
            let (mut dist, mut pitch, mut yaw) = dist_and_angle(m, self.c.pos);
            approach_f32_asymptotic_bool(&mut dist, 600.0, 0.3);
            approach_s16_asymptotic_bool(&mut pitch, 0x1000, 16);
            approach_s16_asymptotic_bool(&mut yaw, self.c.yaw, 8);
            self.c.pos = set_dist_and_angle(m, dist, pitch, yaw);
        }
        let m = self.mario.pos;
        if self.ev(55, 124) {
            // cutscene_dance_fly_rotate_around_mario: cutscene_pan_cvar9 + rotate_and_move
            self.c.focus = self.cs.vars[9].point;
            self.cs.vars[9].angle[0] = w(self.cs.vars[9].angle[0] - 29);
            self.cs.vars[9].angle[1] = w(self.cs.vars[9].angle[1] + 29);
            let (pa, ya) = (self.cs.vars[9].angle[0], self.cs.vars[9].angle[1]);
            self.pan_camera(pa, ya);
            let d = self.cs.vars[8].point[2];
            Self::rotate_and_move_vec3f(&mut self.c.pos, m, d, 0, 0);
            Self::rotate_and_move_vec3f(&mut self.c.pos, m, 0.0, 0, 0x80);
        }
        if self.ev(40, 40) { self.set_fov_shake(0x400, 0x30, 0x8000); }
        self.set_fov_function(CAM_FOV_DEFAULT);
        self.set_handheld_shake(HAND_CAM_SHAKE_STAR_DANCE);
    }
    fn cutscene_key_dance(&mut self, _e: &Env) {
        if self.ev(0, 10) {
            self.dance_move_to_mario();
            self.focus_in_front_of_mario(0.0, 0.2);
        }
        if self.ev(0, 0) {
            self.cs.vars[8].point = [38.0, 171.0, -248.0];
            self.cs.vars[7].point = [-57.0, 51.0, 187.0];
        }
        if self.ev(20, 20) {
            self.cs.vars[8].point = [-178.0, 62.0, -132.0];
            self.cs.vars[7].point = [299.0, 91.0, 58.0];
        }
        if self.ev(35, 35) {
            self.lakitu.key_dance_roll = 0x2800;
            self.cs.vars[8].point = [89.0, 373.0, -304.0];
            self.cs.vars[7].point = [0.0, 127.0, 0.0];
        }
        if self.ev(52, 52) {
            self.lakitu.key_dance_roll = 0;
            self.cs.vars[8].point = [135.0, 158.0, -673.0];
            self.cs.vars[7].point = [-20.0, 135.0, -198.0];
        }
        if self.ev(11, -1) {
            self.c.pos = offset_rotated(self.mario.pos, self.cs.vars[8].point, self.mario.face_angle);
            self.c.focus = offset_rotated(self.mario.pos, self.cs.vars[7].point, self.mario.face_angle);
        }
        if self.ev(54, 54) { self.set_fov_shake(0x180, 0x30, 0x8000); }
        if self.ev(52, -1) { self.set_handheld_shake(HAND_CAM_SHAKE_CUTSCENE); }
    }
}
