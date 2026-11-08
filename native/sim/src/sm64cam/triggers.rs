//! Area / surface camera triggers of `camera.c`: `camera_course_processing`, the per-level
//! `sCam*` trigger tables (data in `data.rs`, generated from the decomp) with their `cam_*`
//! callbacks, the surface-type mode switches, and the Mario-side camera hooks (the
//! `set_camera_mode` calls the decomp makes from `mario.c` / `mario_actions_*.c`).
use super::cutscenes::*;
use super::data::*;
use super::util::*;
use super::*;

/// `AREA_*` of camera.h: `LEVEL_AREA_INDEX(level, area)`.
pub mod areas {
    use super::super::data::*;
    const fn a(level: i32, area: i32) -> i32 { (level << 4) + area }
    pub const AREA_BBH: i32 = a(LEVEL_BBH, 1);
    pub const AREA_CCM_OUTSIDE: i32 = a(LEVEL_CCM, 1);
    pub const AREA_CCM_SLIDE: i32 = a(LEVEL_CCM, 2);
    pub const AREA_CASTLE_LOBBY: i32 = a(LEVEL_CASTLE, 1);
    pub const AREA_CASTLE_TIPPY: i32 = a(LEVEL_CASTLE, 2);
    pub const AREA_CASTLE_BASEMENT: i32 = a(LEVEL_CASTLE, 3);
    pub const AREA_HMC: i32 = a(LEVEL_HMC, 1);
    pub const AREA_SSL_OUTSIDE: i32 = a(LEVEL_SSL, 1);
    pub const AREA_SSL_PYRAMID: i32 = a(LEVEL_SSL, 2);
    pub const AREA_SSL_EYEROK: i32 = a(LEVEL_SSL, 3);
    pub const AREA_BOB: i32 = a(LEVEL_BOB, 1);
    pub const AREA_SL_OUTSIDE: i32 = a(LEVEL_SL, 1);
    pub const AREA_SL_IGLOO: i32 = a(LEVEL_SL, 2);
    pub const AREA_WDW_MAIN: i32 = a(LEVEL_WDW, 1);
    pub const AREA_WDW_TOWN: i32 = a(LEVEL_WDW, 2);
    pub const AREA_JRB_MAIN: i32 = a(LEVEL_JRB, 1);
    pub const AREA_JRB_SHIP: i32 = a(LEVEL_JRB, 2);
    pub const AREA_THI_HUGE: i32 = a(LEVEL_THI, 1);
    pub const AREA_THI_TINY: i32 = a(LEVEL_THI, 2);
    pub const AREA_THI_WIGGLER: i32 = a(LEVEL_THI, 3);
    pub const AREA_TTC: i32 = a(LEVEL_TTC, 1);
    pub const AREA_RR: i32 = a(LEVEL_RR, 1);
    pub const AREA_CASTLE_GROUNDS: i32 = a(LEVEL_CASTLE_GROUNDS, 1);
    pub const AREA_SA: i32 = a(LEVEL_SA, 1);
    pub const AREA_LLL_OUTSIDE: i32 = a(LEVEL_LLL, 1);
    pub const AREA_LLL_VOLCANO: i32 = a(LEVEL_LLL, 2);
    pub const AREA_DDD_WHIRLPOOL: i32 = a(LEVEL_DDD, 1);
    pub const AREA_DDD_SUB: i32 = a(LEVEL_DDD, 2);
    pub const AREA_WF: i32 = a(LEVEL_WF, 1);
    pub const AREA_TTM_OUTSIDE: i32 = a(LEVEL_TTM, 1);
}
use areas::*;

impl SmCam {
    /// `surface_type_modes`: returns whether the floor type chose a mode.
    fn surface_type_modes(&mut self) -> u32 {
        let mut changed = 0;
        match self.geo.curr_floor_type {
            SURFACE_CLOSE_CAMERA => { self.transition_to_camera_mode(CAMERA_MODE_CLOSE, 90); changed += 1; }
            SURFACE_CAMERA_FREE_ROAM => { self.transition_to_camera_mode(CAMERA_MODE_FREE_ROAM, 90); changed += 1; }
            SURFACE_NO_CAM_COL_SLIPPERY => { self.transition_to_camera_mode(CAMERA_MODE_CLOSE, 90); changed += 1; }
            _ => {}
        }
        changed
    }

    /// `set_mode_if_not_set_by_surface`
    fn set_mode_if_not_set_by_surface(&mut self, mode: i32) -> u32 {
        let changed = self.surface_type_modes();
        if changed == 0 && mode != 0 {
            self.transition_to_camera_mode(mode, 90);
        }
        changed
    }

    /// `surface_type_modes_thi`
    fn surface_type_modes_thi(&mut self) {
        match self.geo.curr_floor_type {
            SURFACE_CLOSE_CAMERA | SURFACE_CAMERA_FREE_ROAM | SURFACE_NO_CAM_COL_SLIPPERY => {
                if self.c.mode != CAMERA_MODE_CLOSE {
                    self.transition_to_camera_mode(CAMERA_MODE_FREE_ROAM, 90);
                }
            }
            SURFACE_CAMERA_8_DIR => self.transition_to_camera_mode(CAMERA_MODE_8_DIRECTIONS, 90),
            _ => self.transition_to_camera_mode(CAMERA_MODE_RADIAL, 90),
        }
    }

    /// `check_blocking_area_processing`
    fn check_blocking_area_processing(&mut self, e: &Env) {
        let mode = self.c.mode;
        let a = self.mario.action;
        if a & act::FLAG_METAL_WATER != 0 || mode == CAMERA_MODE_BEHIND_MARIO || mode == CAMERA_MODE_WATER_SURFACE {
            self.status_flags |= CAM_FLAG_BLOCK_AREA_PROCESSING;
        }
        if e.level == LEVEL_DDD || e.level == LEVEL_WDW || e.level == LEVEL_COTMC {
            self.status_flags &= !CAM_FLAG_BLOCK_AREA_PROCESSING;
        }
        if (mode == CAMERA_MODE_BEHIND_MARIO && a & (act::FLAG_SWIMMING | act::FLAG_METAL_WATER) == 0) || mode == CAMERA_MODE_INSIDE_CANNON {
            self.status_flags |= CAM_FLAG_BLOCK_AREA_PROCESSING;
        }
    }

    /// `camera_course_processing`: evaluates the area's triggers and the per-area mode rules.
    pub fn camera_course_processing(&mut self, e: &Env) -> i32 {
        let old_mode = self.c.mode;
        if self.c.mode == CAMERA_MODE_C_UP {
            self.c.mode = self.mode_info.last_mode;
        }
        self.check_blocking_area_processing(e);
        let mut inside_bounds = false;
        let area = e.area as i8;
        for t in level_triggers(e.level) {
            if t.area == area {
                let center = [t.center[0] as f64, t.center[1] as f64, t.center[2] as f64];
                let bounds = [t.bounds[0] as f64, t.bounds[1] as f64, t.bounds[2] as f64];
                if is_pos_in_bounds(self.mario.pos, center, bounds, t.yaw) && self.status_flags & CAM_FLAG_BLOCK_AREA_PROCESSING == 0 {
                    self.run_event(t.ev, e);
                    inside_bounds = true;
                }
            }
            if t.area == -1 && !inside_bounds && self.status_flags & CAM_FLAG_BLOCK_AREA_PROCESSING == 0 {
                self.run_event(t.ev, e);
            }
        }
        if self.status_flags & CAM_FLAG_BLOCK_AREA_PROCESSING == 0 {
            match self.curr_level_area {
                AREA_WF => {
                    if self.mario.action == act::RIDING_HOOT {
                        self.transition_to_camera_mode(CAMERA_MODE_SLIDE_HOOT, 60);
                    } else {
                        match self.geo.curr_floor_type {
                            SURFACE_CAMERA_8_DIR => {
                                self.transition_to_camera_mode(CAMERA_MODE_8_DIRECTIONS, 90);
                                self.dir8_base_yaw = degrees(90);
                            }
                            SURFACE_BOSS_FIGHT_CAMERA => {
                                if e.act == 1 { self.set_camera_mode_boss_fight(); } else { self.set_camera_mode_radial(60); }
                            }
                            _ => self.set_camera_mode_radial(60),
                        }
                    }
                }
                AREA_BBH => {
                    if self.fixed_mode_base_position == [210.0, 420.0, 3109.0] && self.mario.pos[1] < 1800.0 {
                        self.transition_to_camera_mode(CAMERA_MODE_CLOSE, 30);
                    }
                }
                AREA_SSL_PYRAMID => { self.set_mode_if_not_set_by_surface(CAMERA_MODE_OUTWARD_RADIAL); }
                AREA_SSL_OUTSIDE => { self.set_mode_if_not_set_by_surface(CAMERA_MODE_RADIAL); }
                AREA_THI_HUGE => {}
                AREA_THI_TINY => self.surface_type_modes_thi(),
                AREA_TTC => { self.set_mode_if_not_set_by_surface(CAMERA_MODE_OUTWARD_RADIAL); }
                AREA_BOB => {
                    if self.set_mode_if_not_set_by_surface(CAMERA_MODE_NONE) == 0 {
                        if self.geo.curr_floor_type == SURFACE_BOSS_FIGHT_CAMERA {
                            self.set_camera_mode_boss_fight();
                        } else if self.c.mode == CAMERA_MODE_CLOSE {
                            self.transition_to_camera_mode(CAMERA_MODE_RADIAL, 60);
                        } else {
                            self.set_camera_mode_radial(60);
                        }
                    }
                }
                AREA_WDW_MAIN => {
                    if self.geo.curr_floor_type == SURFACE_INSTANT_WARP_1B { self.c.def_mode = CAMERA_MODE_RADIAL; }
                }
                AREA_WDW_TOWN => {
                    if self.geo.curr_floor_type == SURFACE_INSTANT_WARP_1C { self.c.def_mode = CAMERA_MODE_CLOSE; }
                }
                AREA_DDD_WHIRLPOOL => self.lakitu.def_mode = CAMERA_MODE_OUTWARD_RADIAL,
                AREA_DDD_SUB => {
                    if self.c.mode != CAMERA_MODE_BEHIND_MARIO && self.c.mode != CAMERA_MODE_WATER_SURFACE {
                        if self.mario.action & act::FLAG_ON_POLE != 0 || self.geo.curr_floor_height > 800.0 {
                            self.transition_to_camera_mode(CAMERA_MODE_8_DIRECTIONS, 60);
                        } else if self.mario.pos[1] < 800.0 {
                            self.transition_to_camera_mode(CAMERA_MODE_FREE_ROAM, 60);
                        }
                    }
                    self.lakitu.def_mode = CAMERA_MODE_FREE_ROAM;
                }
                _ => {}
            }
        }
        self.status_flags &= !CAM_FLAG_BLOCK_AREA_PROCESSING;
        if old_mode == CAMERA_MODE_C_UP {
            self.mode_info.last_mode = self.c.mode;
            self.c.mode = old_mode;
        }
        self.c.mode
    }

    /// `set_camera_mode_radial`
    pub fn set_camera_mode_radial(&mut self, transition_time: i32) {
        let focus = [self.c.area_cen[0], self.mario.pos[1], self.c.area_cen[2]];
        if self.c.mode != CAMERA_MODE_RADIAL {
            let yaw = w(calculate_yaw(focus, self.mario.pos) - calculate_yaw(self.c.focus, self.c.pos) + degrees(90));
            if yaw > 0 {
                self.transition_to_camera_mode(CAMERA_MODE_RADIAL, transition_time);
            } else {
                self.c.mode = CAMERA_MODE_RADIAL;
                self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            }
            self.mode_offset_yaw = 0;
        }
    }

    /// `set_camera_mode_8_directions`
    pub fn set_camera_mode_8_directions(&mut self) {
        if self.c.mode != CAMERA_MODE_8_DIRECTIONS {
            self.c.mode = CAMERA_MODE_8_DIRECTIONS;
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            self.dir8_base_yaw = 0;
            self.dir8_yaw_offset = 0;
        }
    }

    /// `set_camera_mode_close_cam`
    pub fn set_camera_mode_close_cam(&mut self) {
        if self.c.mode != CAMERA_MODE_CLOSE {
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            self.c.mode = CAMERA_MODE_CLOSE;
        }
    }

    /// `move_camera_through_floor_while_descending`
    fn move_camera_through_floor_while_descending(&mut self, height: f64) {
        if self.geo.curr_floor_height < height - 100.0 && self.geo.prev_floor_height > self.geo.curr_floor_height {
            self.c.pos[1] = height - 400.0;
            self.lakitu.cur_pos[1] = height - 400.0;
            self.lakitu.goal_pos[1] = height - 400.0;
        }
    }

    /// The `cam_*` callbacks of the trigger tables.
    pub fn run_event(&mut self, ev: Ev, e: &Env) {
        use Ev::*;
        match ev {
            SlSnowmanHead8dir => {
                self.status_flags |= CAM_FLAG_BLOCK_AREA_PROCESSING;
                self.transition_to_camera_mode(CAMERA_MODE_8_DIRECTIONS, 60);
                self.dir8_base_yaw = 0x1D27;
            }
            SlFreeRoam => self.transition_to_camera_mode(CAMERA_MODE_FREE_ROAM, 60),
            ThiMoveCamThroughTunnel => { if self.mode_offset_yaw < degrees(60) { self.mode_offset_yaw = degrees(60); } }
            ThiLookThroughTunnel => { if self.mode_offset_yaw > 0x3AAA { self.mode_offset_yaw = 0x3AAA; } }
            HmcEnterMaze => {
                if self.c.pos[1] > -102.0 {
                    let (_d, pitch, yaw) = dist_and_angle(self.c.focus, self.lakitu.goal_pos);
                    self.lakitu.goal_pos = set_dist_and_angle(self.c.focus, 300.0, pitch, yaw);
                    self.lakitu.goal_pos[1] = -800.0;
                    self.c.pos[1] = self.lakitu.goal_pos[1];
                    self.lakitu.cur_pos[1] = self.lakitu.goal_pos[1];
                    self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
                }
            }
            CastleHmcStartPoolCutscene => {
                if self.mario.action != act_consts::SPECIAL_DEATH_EXIT && self.mario.action != act::SPECIAL_EXIT_AIRBORNE {
                    self.start_cutscene(e, CUTSCENE_ENTER_POOL);
                }
            }
            HmcElevatorBlackHole => self.move_camera_through_floor_while_descending(1536.0),
            HmcElevatorMazeEmergencyExit => self.move_camera_through_floor_while_descending(2355.0),
            HmcElevatorLake | HmcElevatorMaze => self.move_camera_through_floor_while_descending(1843.0),
            SslEnterPyramidTop => { self.start_object_cutscene_without_focus(CUTSCENE_ENTER_PYRAMID_TOP); }
            SslPyramidCenter => {
                self.status_flags |= CAM_FLAG_BLOCK_AREA_PROCESSING;
                self.transition_to_camera_mode(CAMERA_MODE_CLOSE, 90);
            }
            SslBossRoom => {
                self.status_flags |= CAM_FLAG_BLOCK_AREA_PROCESSING;
                self.transition_to_camera_mode(CAMERA_MODE_OUTWARD_RADIAL, 90);
            }
            RrExitBuildingSide => {
                self.set_camera_mode_8_directions();
                self.dir8_base_yaw = degrees(90);
            }
            RrExitBuildingTop => {
                self.set_camera_mode_8_directions();
                if self.c.pos[1] < 6343.0 {
                    self.c.pos[1] = 7543.0;
                    self.lakitu.goal_pos[1] = self.c.pos[1];
                    self.lakitu.cur_pos[1] = self.c.pos[1];
                    self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
                }
            }
            RrEnterBuildingWindow => { if self.c.mode != CAMERA_MODE_FIXED { self.set_camera_mode_fixed(-2974, 478, -3975); } }
            RrEnterBuilding => {
                if self.c.mode != CAMERA_MODE_FIXED { self.set_camera_mode_fixed(-2953, 798, -3943); }
                if self.c.pos[1] > 6043.0 { self.c.pos[1] = 6043.0; }
            }
            RrEnterBuildingSide => {
                if self.c.mode != CAMERA_MODE_FIXED {
                    self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
                    self.c.mode = CAMERA_MODE_FIXED;
                }
            }
            BobTower => {
                self.status_flags |= CAM_FLAG_BLOCK_AREA_PROCESSING;
                self.transition_to_camera_mode(CAMERA_MODE_RADIAL, 90);
            }
            BobDefaultFreeRoam => self.transition_to_camera_mode(CAMERA_MODE_FREE_ROAM, 90),
            CotmcExitWaterfall => self.movement_flags |= CAM_MOVE_FIX_IN_PLACE,
            CcmEnterSlideShortcut => self.status_flags |= CAM_FLAG_CCM_SLIDE_SHORTCUT,
            CcmLeaveSlideShortcut => self.status_flags &= !CAM_FLAG_CCM_SLIDE_SHORTCUT,
            CastleCloseMode => self.set_camera_mode_close_cam(),
            CastleEnterLobby | CastleEnterLobbySlidingDoor => {
                if self.c.mode != CAMERA_MODE_FIXED {
                    self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
                    self.set_fixed_cam_axis_sa_lobby();
                    self.c.mode = CAMERA_MODE_FIXED;
                }
            }
            CastleLeaveLobbySlidingDoor => {
                self.set_camera_mode_close_cam();
                self.c.door_status = DOOR_ENTER_LOBBY;
            }
            CastleEnterSpiralStairs => self.transition_to_camera_mode(CAMERA_MODE_SPIRAL_STAIRS, 20),
            CastleLookUpstairs => {
                let (fh, _) = self.find_floor(e, self.c.pos[0], self.c.pos[1], self.c.pos[2]);
                if self.geo.curr_floor_height > 1229.0 && fh < 1229.0 && self.c_side_button_yaw == 0 {
                    self.c.pos = [-227.0, 1425.0, 1533.0];
                }
            }
            CastleBasementLookDownstairs => {
                let (fh, _) = self.find_floor(e, self.c.pos[0], self.c.pos[1], self.c.pos[2]);
                if fh > -110.0 && self.c_side_button_yaw == 0 {
                    self.c.pos = [-980.0, 249.0, -1398.0];
                }
            }
            CastleLobbyEntrance => {
                let b = self.fixed_mode_base_position;
                self.castle_entrance_offset = [-813.0 - b[0], 378.0 - b[1], 1103.0 - b[2]];
            }
            BbhRoom6 => {
                let path = BBH_LIBRARY_PAR_TRACK
                    .iter()
                    .map(|p| ParPoint { start_of_path: p.0, pos: p.1, dist_thresh: p.2, zoom: p.3 })
                    .collect();
                self.parallel_tracking_init(path);
            }
            BbhFallOffRoof | BbhEnterFrontDoor | BbhRoom0BackEntrance => self.set_camera_mode_close_cam(),
            BbhFallIntoPool => {
                self.set_camera_mode_close_cam();
                self.lakitu.goal_pos = offset_rotated(self.mario.pos, [0.0, 0.0, 300.0], self.mario.face_angle);
                self.lakitu.goal_pos[1] = -2300.0;
                self.c.pos = self.lakitu.goal_pos;
                self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            }
            BbhRoom1 => { self.set_camera_mode_fixed(956, 440, 1994); }
            BbhLeaveFrontDoor => {
                self.c.door_status = DOOR_LEAVING_SPECIAL;
                self.set_camera_mode_fixed(956, 440, 1994);
            }
            BbhRoom2Lower => { self.set_camera_mode_fixed(2591, 400, 1284); }
            BbhRoom4 => { self.set_camera_mode_fixed(3529, 340, -1384); }
            BbhRoom8 => { self.set_camera_mode_fixed(-500, 740, -1306); }
            BbhRoom5Library => { self.set_camera_mode_fixed(-2172, 200, 675); }
            BbhRoom5LibraryToHiddenTransition => { if self.set_camera_mode_fixed(-2172, 200, 675) { self.transition_next_state(20); } }
            BbhRoom5HiddenToLibraryTransition => { if self.set_camera_mode_fixed(-1542, 320, -307) { self.transition_next_state(20); } }
            BbhRoom5Hidden => {
                self.c.door_status = DOOR_LEAVING_SPECIAL;
                self.set_camera_mode_fixed(-1542, 320, -307);
            }
            BbhRoom3 => { self.set_camera_mode_fixed(-1893, 320, 2327); }
            BbhRoom7MrI => { self.set_camera_mode_fixed(1371, 360, -1302); }
            BbhRoom7MrIToCoffinsTransition => { if self.set_camera_mode_fixed(1371, 360, -1302) { self.transition_next_state(20); } }
            BbhRoom7CoffinsToMrITransition => { if self.set_camera_mode_fixed(2115, 260, -772) { self.transition_next_state(20); } }
            BbhElevatorRoomLower => {
                self.c.door_status = DOOR_LEAVING_SPECIAL;
                self.set_camera_mode_close_cam();
            }
            BbhElevator => {
                if self.c.mode == CAMERA_MODE_FIXED {
                    self.set_camera_mode_close_cam();
                    self.c.pos[1] = -405.0;
                    self.lakitu.goal_pos[1] = -405.0;
                }
            }
            BbhRoom12Upper => {
                self.c.door_status = DOOR_LEAVING_SPECIAL;
                self.set_camera_mode_fixed(-2932, 296, 4429);
            }
            BbhRoom2Library => { self.set_camera_mode_fixed(3493, 440, 617); }
            BbhRoom2LibraryToTrapdoorTransition => { if self.set_camera_mode_fixed(3493, 440, 617) { self.transition_next_state(20); } }
            BbhRoom2Trapdoor => { self.set_camera_mode_fixed(3502, 440, 1217); }
            BbhRoom2TrapdoorTransition => { if self.set_camera_mode_fixed(3502, 440, 1217) { self.transition_next_state(20); } }
            BbhRoom9Attic => { self.set_camera_mode_fixed(-670, 460, 372); }
            BbhRoom9AtticTransition => { if self.set_camera_mode_fixed(-670, 460, 372) { self.transition_next_state(20); } }
            BbhRoom9MrITransition => { if self.set_camera_mode_fixed(131, 380, -263) { self.transition_next_state(20); } }
            BbhRoom13Balcony => { self.set_camera_mode_fixed(210, 420, 3109); }
            BbhRoom0 => {
                self.c.door_status = DOOR_LEAVING_SPECIAL;
                self.set_camera_mode_fixed(-204, 807, 204);
            }
        }
    }

    // ---- Mario-side camera hooks ----

    /// The `set_camera_mode` / `cameraEvent` calls Mario's code makes in the decomp
    /// (`update_mario_sound_and_camera`, `set_submerged_cam_preset_and_spawn_bubbles`,
    /// `set_water_plunge_action`, `act_flying`, `act_shot_from_cannon`, `act_in_cannon`,
    /// `act_first_person`, ...), derived from Mario's action each tick instead of editing the
    /// Mario action files. `prev_action` is the action of the previous tick.
    pub fn mario_camera_hooks(&mut self, e: &Env, prev_action: u32, first_person: bool) {
        let a = e.m.action;
        let swimming = a & (act::FLAG_SWIMMING | act::FLAG_METAL_WATER) != 0;
        let prev_swimming = prev_action & (act::FLAG_SWIMMING | act::FLAG_METAL_WATER) != 0;
        let mode = self.c.mode;
        // act_first_person / update_mario_sound_and_camera: C-up look mode
        if first_person {
            if self.mode_info.last_mode != CAMERA_MODE_C_UP && self.c.mode != CAMERA_MODE_C_UP {
                self.set_camera_mode(e, CAMERA_MODE_C_UP, 0x10);
            }
        } else if self.c.mode == CAMERA_MODE_C_UP {
            self.movement_flags &= !CAM_MOVE_C_UP_MODE;
            self.set_camera_mode(e, -1, 1);
        }
        // act_in_cannon (state 0): CAM_EVENT_CANNON
        if a == act::IN_CANNON && prev_action != act::IN_CANNON {
            self.camera_event = CAM_EVENT_CANNON;
        }
        // act_shot_from_cannon
        if a == act::SHOT_FROM_CANNON {
            if mode != CAMERA_MODE_BEHIND_MARIO {
                self.camera_event = CAM_EVENT_SHOT_FROM_CANNON;
            }
            if self.cs.shot_timer == 10 && mode != CAMERA_MODE_BEHIND_MARIO {
                self.set_camera_mode(e, CAMERA_MODE_BEHIND_MARIO, 1);
            }
            self.cs.shot_timer += 1;
        } else {
            self.cs.shot_timer = 0;
        }
        // act_flying: behind-Mario camera while flying
        if a == act_consts::FLYING {
            if self.c.mode != CAMERA_MODE_BEHIND_MARIO {
                self.set_camera_mode(e, CAMERA_MODE_BEHIND_MARIO, 1);
            }
        } else if prev_action == act_consts::FLYING && self.c.mode == CAMERA_MODE_BEHIND_MARIO {
            let d = self.c.def_mode;
            self.set_camera_mode(e, d, 1);
        }
        // set_water_plunge_action
        if swimming && !prev_swimming && self.c.mode != CAMERA_MODE_WATER_SURFACE {
            self.set_camera_mode(e, CAMERA_MODE_WATER_SURFACE, 1);
        }
        // set_submerged_cam_preset_and_spawn_bubbles
        if swimming {
            let height_below_water = (e.m.water_level - 80.0) - e.m.pos[1];
            let preset = self.c.mode;
            if a & act::FLAG_METAL_WATER != 0 {
                if preset != CAMERA_MODE_CLOSE { self.set_camera_mode(e, CAMERA_MODE_CLOSE, 1); }
            } else {
                if height_below_water > 800.0 && preset != CAMERA_MODE_BEHIND_MARIO {
                    self.set_camera_mode(e, CAMERA_MODE_BEHIND_MARIO, 1);
                }
                if height_below_water < 400.0 && self.c.mode != CAMERA_MODE_WATER_SURFACE {
                    self.set_camera_mode(e, CAMERA_MODE_WATER_SURFACE, 1);
                }
            }
        } else if (self.c.mode == CAMERA_MODE_BEHIND_MARIO || self.c.mode == CAMERA_MODE_WATER_SURFACE)
            && a != act_consts::FLYING
            && a != act::SHOT_FROM_CANNON
            && a != act::IN_CANNON
        {
            // transition_submerged_to_walking / update_mario_sound_and_camera
            let d = self.c.def_mode;
            self.set_camera_mode(e, d, 1);
        }
    }
}

/// A few action constants not in [`act`](super::act) (kept out of the public list on purpose).
pub mod act_consts {
    pub const FLYING: u32 = 0x10880899;
    pub const SPECIAL_DEATH_EXIT: u32 = 0x0000192C;
}
