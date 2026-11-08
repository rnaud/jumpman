//! The CAMERA_MODE_* update functions of `src/game/camera.c`: radial, outward radial, 8
//! directions, behind Mario / water surface, the default Lakitu camera (close / free roam /
//! Mario angle), C-up, fixed, boss fight, parallel tracking, slide / Hoot, spiral stairs and the
//! inside-cannon camera, plus their input handling (C buttons) and wall avoidance.
// Several locals mirror decomp assignments whose value is overwritten before use.
#![allow(unused_assignments)]
use super::triggers::areas::*;
use super::util::*;
use super::*;
use crate::math::{atan2s, coss, sins};

impl SmCam {
    pub fn play_camera_buzz_if_cdown(&mut self, e: &Env) {
        if e.pad.pressed & D_CBUTTONS != 0 { self.sound(CamSound::Buzz); }
    }
    pub fn play_camera_buzz_if_cbutton(&mut self, e: &Env) {
        if e.pad.pressed & CBUTTON_MASK != 0 { self.sound(CamSound::Buzz); }
    }
    pub fn play_camera_buzz_if_c_sideways(&mut self, e: &Env) {
        if e.pad.pressed & (L_CBUTTONS | R_CBUTTONS) != 0 { self.sound(CamSound::Buzz); }
    }

    /// `calc_y_to_curr_floor`: (posOff, focOff).
    pub fn calc_y_to_curr_floor(&self, e: &Env, pos_mul: f64, mut pos_bound: f64, foc_mul: f64, foc_bound: f64) -> (f64, f64) {
        let mut floor_height = self.geo.curr_floor_height;
        let m = self.mario;
        if m.action & act::FLAG_METAL_WATER == 0 {
            let wl = self.find_water_level(e, m.pos[0], m.pos[2]);
            if floor_height < wl { floor_height = wl; }
        }
        if m.action & act::FLAG_ON_POLE != 0 {
            if let Some(o) = m.used_obj {
                let (oy, h) = (o.pos[1], o.hitbox_height);
                if self.geo.curr_floor_height >= oy && m.pos[1] < 0.7 * h + oy {
                    pos_bound = 1200.0;
                }
            }
        }
        let pos_off = ((floor_height - m.pos[1]) * pos_mul).clamp(-pos_bound, pos_bound);
        let foc_off = ((floor_height - m.pos[1]) * foc_mul).clamp(-foc_bound, foc_bound);
        (pos_off, foc_off)
    }

    /// `focus_on_mario`
    pub fn focus_on_mario(&self, focus: &mut V3, pos: &mut V3, pos_y_off: f64, foc_y_off: f64, dist: f64, pitch: i32, yaw: i32) {
        let m = self.mario.pos;
        *pos = set_dist_and_angle([m[0], m[1] + pos_y_off, m[2]], dist, w(pitch + self.lakitu_pitch), yaw);
        *focus = [m[0], m[1] + foc_y_off, m[2]];
    }

    /// `approach_camera_height`
    pub fn approach_camera_height(&mut self, goal: f64, inc: f64) {
        if self.status_flags & CAM_FLAG_SMOOTH_MOVEMENT != 0 {
            if self.c.pos[1] < goal {
                self.c.pos[1] += inc;
                if self.c.pos[1] > goal { self.c.pos[1] = goal; }
            } else {
                self.c.pos[1] -= inc;
                if self.c.pos[1] < goal { self.c.pos[1] = goal; }
            }
        } else {
            self.c.pos[1] = goal;
        }
    }

    /// `set_camera_height`
    pub fn set_camera_height(&mut self, e: &Env, mut goal_height: f64) {
        let base_off = 125.0;
        let (mut cam_ceil, _) = self.find_ceil(e, self.c.pos[0], self.lakitu.goal_pos[1] - 50.0, self.c.pos[2]);
        let m = self.mario;
        if m.action & act::FLAG_HANGING != 0 {
            let mario_ceil = self.geo.curr_ceil_height;
            let mut mario_floor = self.geo.curr_floor_height;
            if mario_floor < mario_ceil - 400.0 { mario_floor = mario_ceil - 400.0; }
            goal_height = mario_floor + (mario_ceil - mario_floor) * 0.4;
            if m.pos[1] - 400.0 > goal_height { goal_height = m.pos[1] - 400.0; }
            self.approach_camera_height(goal_height, 5.0);
        } else {
            let mut cam_floor = self.find_floor(e, self.c.pos[0], self.c.pos[1] + 100.0, self.c.pos[2]).0 + base_off;
            let mario_floor = base_off + self.geo.curr_floor_height;
            if cam_floor < mario_floor { cam_floor = mario_floor; }
            if goal_height < cam_floor {
                goal_height = cam_floor;
                self.c.pos[1] = goal_height;
            }
            if m.action == act::BUTT_STUCK_IN_GROUND || m.action == act::HEAD_STUCK_IN_GROUND || m.action == act::FEET_STUCK_IN_GROUND {
                if (self.c.pos[1] - goal_height).abs() > 1000.0 { self.c.pos[1] = goal_height; }
            }
            self.approach_camera_height(goal_height, 20.0);
            if cam_ceil != CELL_HEIGHT_LIMIT {
                cam_ceil -= base_off;
                if (self.c.pos[1] > cam_ceil && self.geo.curr_floor_height + base_off < cam_ceil)
                    || (self.geo.curr_ceil_height != CELL_HEIGHT_LIMIT && self.geo.curr_ceil_height > cam_ceil && self.c.pos[1] > cam_ceil)
                {
                    self.c.pos[1] = cam_ceil;
                }
            }
        }
    }

    /// `look_down_slopes`
    pub fn look_down_slopes(&self, e: &Env, cam_yaw: i32) -> i32 {
        let mut pitch = 0x05B0;
        let m = self.mario.pos;
        let x = m[0] + sins(cam_yaw as f64) * 40.0;
        let z = m[2] + coss(cam_yaw as f64) * 40.0;
        let (h, floor) = self.find_floor(e, x, m[1], z);
        let dy = h - m[1];
        if let Some(f) = floor {
            if f.kind != SURFACE_WALL_MISC && dy > 0.0 {
                if f.normal[2] == 0.0 && dy < 100.0 {
                    pitch = 0x05B0;
                } else {
                    pitch += atan2s(40.0, dy);
                }
            }
        }
        pitch
    }

    /// `pan_ahead_of_player`
    pub fn pan_ahead_of_player(&mut self) {
        let m = self.mario;
        let (dist, _pitch, yaw) = dist_and_angle(self.c.pos, m.pos);
        let mut pan = [0.0, 0.0, sins(0xC00 as f64) * dist];
        pan = rotate_in_xz(pan, m.face_angle[1]);
        let yaw = w(-yaw);
        pan = rotate_in_xz(pan, yaw);
        pan[2] = 0.0;
        if m.action == act::LONG_JUMP || (m.action != act::TOP_OF_POLE && m.action & act::FLAG_ON_POLE != 0) {
            pan[0] = -pan[0];
        }
        if self.status_flags & CAM_FLAG_SLEEPING != 0 {
            approach_f32_asymptotic_bool(&mut self.pan_distance, 0.0, 0.025);
        } else {
            approach_f32_asymptotic_bool(&mut self.pan_distance, pan[0], 0.025);
        }
        pan[0] = self.pan_distance;
        let yaw = w(-yaw);
        pan = rotate_in_xz(pan, yaw);
        add(&mut self.c.focus, pan);
    }

    /// `clamp_positions_and_find_yaw`
    pub fn clamp_positions_and_find_yaw(&self, pos: &mut V3, origin: V3, x_max: f64, x_min: f64, z_max: f64, z_min: f64) -> i32 {
        if pos[0] >= x_max { pos[0] = x_max; }
        if pos[0] <= x_min { pos[0] = x_min; }
        if pos[2] >= z_max { pos[2] = z_max; }
        if pos[2] <= z_min { pos[2] = z_min; }
        calculate_yaw(origin, *pos)
    }

    /// `find_in_bounds_yaw_wdw_bob_thi`
    pub fn find_in_bounds_yaw_wdw_bob_thi(&self, pos: &mut V3, origin: V3, mut yaw: i32) -> i32 {
        match self.curr_level_area {
            AREA_WDW_MAIN => yaw = self.clamp_positions_and_find_yaw(pos, origin, 4508.0, -3739.0, 4508.0, -3739.0),
            AREA_BOB => yaw = self.clamp_positions_and_find_yaw(pos, origin, 8000.0, -8000.0, 7050.0, -8000.0),
            AREA_THI_HUGE => yaw = self.clamp_positions_and_find_yaw(pos, origin, 8192.0, -8192.0, 8192.0, -8192.0),
            AREA_THI_TINY => yaw = self.clamp_positions_and_find_yaw(pos, origin, 2458.0, -2458.0, 2458.0, -2458.0),
            _ => {}
        }
        yaw
    }

    // ---- wall avoidance ----

    fn is_behind_surface(pos: V3, s: &Surface) -> bool {
        let (v1, v2, v3) = (s.v1, s.v2, s.v3);
        let nx = (v2[1] - v1[1]) * (v3[2] - v2[2]) - (v3[1] - v2[1]) * (v2[2] - v1[2]);
        let ny = (v2[2] - v1[2]) * (v3[0] - v2[0]) - (v3[2] - v2[2]) * (v2[0] - v1[0]);
        let nz = (v2[0] - v1[0]) * (v3[1] - v2[1]) - (v3[0] - v2[0]) * (v2[1] - v1[1]);
        let (dx, dy, dz) = (v1[0] - pos[0], v1[1] - pos[1], v1[2] - pos[2]);
        dx * nx + dy * ny + dz * nz < 0.0
    }

    /// `is_range_behind_surface` (surface type -1 = any).
    pub fn is_range_behind_surface(from: V3, to: V3, surf: Option<&Surface>, range: i32, surf_type: i32) -> i32 {
        let mut behind = 1;
        if let Some(s) = surf {
            if surf_type == -1 || s.kind != surf_type {
                if range == 0 {
                    behind = Self::is_behind_surface(to, s) as i32;
                } else {
                    let (dist, pitch, yaw) = dist_and_angle(from, to);
                    let left = Self::is_behind_surface(set_dist_and_angle(from, dist, pitch, w(yaw + range)), s) as i32;
                    let right = Self::is_behind_surface(set_dist_and_angle(from, dist, pitch, w(yaw - range)), s) as i32;
                    behind = left * right;
                }
            }
        }
        behind
    }

    /// `is_surf_within_bounding_box`
    fn is_surf_within_bounding_box(s: &Surface, x_max: f64, y_max: f64, z_max: f64) -> bool {
        let (mut dxm, mut dym, mut dzm) = (0.0f64, 0.0f64, 0.0f64);
        let vs = [s.v1, s.v2, s.v3];
        for i in 0..3 {
            let j = (i + 1) % 3;
            dxm = dxm.max((vs[i][0] - vs[j][0]).abs());
            dym = dym.max((vs[i][1] - vs[j][1]).abs());
            dzm = dzm.max((vs[i][2] - vs[j][2]).abs());
        }
        let mut smaller = false;
        if y_max != -1.0 && dym < y_max { smaller = true; }
        if x_max != -1.0 && z_max != -1.0 && dxm < x_max && dzm < z_max { smaller = true; }
        smaller
    }

    /// `rotate_camera_around_walls`: returns the status (0, 1 or 3) and the avoid yaw.
    pub fn rotate_camera_around_walls(&mut self, e: &Env, c_pos: V3, avoid_yaw: &mut i32, yaw_range: i32) -> i32 {
        let m = self.mario.pos;
        let (_d, _p, yaw_from_mario) = dist_and_angle(m, c_pos);
        self.status_flags &= !CAM_FLAG_CAM_NEAR_WALL;
        let mut check_dist = 0.0;
        let mut coarse = 150.0;
        let mut fine = 100.0;
        let mut status = 0;
        let mut step = 0;
        while step < 8 {
            let mut d = crate::collision::WallData {
                x: m[0] + (c_pos[0] - m[0]) * check_dist,
                y: m[1] + (c_pos[1] - m[1]) * check_dist,
                z: m[2] + (c_pos[2] - m[2]) * check_dist,
                offset_y: 100.0,
                radius: coarse,
                walls: vec![],
            };
            approach_f32_symmetric_bool(&mut coarse, 250.0, 30.0);
            if e.world.find_wall_collisions(&mut d, true) != 0 {
                if step >= 5 {
                    self.status_flags |= CAM_FLAG_CAM_NEAR_WALL;
                    if status <= 0 {
                        status = 1;
                        let wall = d.walls.last().unwrap();
                        let wall_yaw = w(atan2s(wall.normal[2], wall.normal[0]) + degrees(90));
                        *avoid_yaw = w(calc_avoid_yaw(yaw_from_mario, wall_yaw) + degrees(180));
                    }
                }
                let mut d = crate::collision::WallData {
                    x: m[0] + (c_pos[0] - m[0]) * check_dist,
                    y: m[1] + (c_pos[1] - m[1]) * check_dist,
                    z: m[2] + (c_pos[2] - m[2]) * check_dist,
                    offset_y: 100.0,
                    radius: fine,
                    walls: vec![],
                };
                approach_f32_symmetric_bool(&mut fine, 200.0, 20.0);
                if e.world.find_wall_collisions(&mut d, true) != 0 {
                    let wall = d.walls.last().unwrap().clone();
                    let hor_wall_norm = atan2s(wall.normal[2], wall.normal[0]);
                    let wall_yaw = w(hor_wall_norm + degrees(90));
                    if Self::is_range_behind_surface(m, c_pos, Some(&wall), yaw_range, SURFACE_WALL_MISC) == 0
                        && Self::is_behind_surface(m, &wall)
                        && !Self::is_surf_within_bounding_box(&wall, -1.0, 150.0, -1.0)
                    {
                        *avoid_yaw = w(calc_avoid_yaw(yaw_from_mario, wall_yaw) + degrees(180));
                        approach_s16_symmetric_bool(avoid_yaw, hor_wall_norm, yaw_range);
                        status = 3;
                        step = 8;
                    }
                }
            }
            check_dist += 0.125;
            step += 1;
        }
        status
    }

    /// `resolve_geometry_collisions`
    pub fn resolve_geometry_collisions(&self, e: &Env, pos: &mut V3) {
        self.wall_push(e, pos, 0.0, 100.0);
        let mut floor_y = self.find_floor(e, pos[0], pos[1] + 50.0, pos[2]).0;
        let mut ceil_y = self.find_ceil(e, pos[0], pos[1] - 50.0, pos[2]).0;
        if FLOOR_LOWER_LIMIT != floor_y && CELL_HEIGHT_LIMIT == ceil_y {
            floor_y += 125.0;
            if pos[1] < floor_y { pos[1] = floor_y; }
        }
        if FLOOR_LOWER_LIMIT == floor_y && CELL_HEIGHT_LIMIT != ceil_y {
            ceil_y -= 125.0;
            if pos[1] > ceil_y { pos[1] = ceil_y; }
        }
        if FLOOR_LOWER_LIMIT != floor_y && CELL_HEIGHT_LIMIT != ceil_y {
            floor_y += 125.0;
            ceil_y -= 125.0;
            if pos[1] <= floor_y && pos[1] < ceil_y { pos[1] = floor_y; }
            if pos[1] > floor_y && pos[1] >= ceil_y { pos[1] = ceil_y; }
            if pos[1] <= floor_y && pos[1] >= ceil_y { pos[1] = (floor_y + ceil_y) * 0.5; }
        }
    }

    // ---- radial / outward radial / 8 directions ----

    /// `update_radial_camera`
    pub fn update_radial_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3) -> i32 {
        let m = self.mario.pos;
        let (cdx, cdz) = (m[0] - self.c.area_cen[0], m[2] - self.c.area_cen[2]);
        let mut cam_yaw = w(atan2s(cdz, cdx) + self.mode_offset_yaw);
        let pitch = self.look_down_slopes(e, cam_yaw);
        let (y_off, base_dist) = (125.0, 1000.0);
        self.area_yaw = w(cam_yaw - self.mode_offset_yaw);
        let (pos_y, focus_y) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        self.focus_on_mario(focus, pos, pos_y + y_off, focus_y + y_off, self.lakitu_dist as f64 + base_dist, pitch, cam_yaw);
        cam_yaw = self.find_in_bounds_yaw_wdw_bob_thi(pos, *focus, cam_yaw);
        cam_yaw
    }

    /// `update_8_directions_camera`
    pub fn update_8_directions_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3, alias: bool) -> i32 {
        let mut cam_yaw = w(self.dir8_base_yaw + self.dir8_yaw_offset);
        let pitch = self.look_down_slopes(e, cam_yaw);
        let (y_off, base_dist) = (125.0, 1000.0);
        self.area_yaw = cam_yaw;
        let (pos_y, focus_y) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        self.focus_on_mario(focus, pos, pos_y + y_off, focus_y + y_off, self.lakitu_dist as f64 + base_dist, pitch, cam_yaw);
        if alias { self.c.focus = *focus; }
        self.pan_ahead_of_player();
        if alias { *focus = self.c.focus; }
        if self.curr_level_area == AREA_DDD_SUB {
            cam_yaw = self.clamp_positions_and_find_yaw(pos, *focus, 6839.0, 995.0, 5994.0, -3945.0);
        }
        cam_yaw
    }

    /// `update_outward_radial_camera`
    pub fn update_outward_radial_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3) -> i32 {
        let m = self.mario.pos;
        let (dx, dz) = (m[0] - self.c.area_cen[0], m[2] - self.c.area_cen[2]);
        let cam_yaw = w(atan2s(dz, dx) + self.mode_offset_yaw + degrees(180));
        let pitch = self.look_down_slopes(e, cam_yaw);
        let (base_dist, y_off) = (1000.0, 125.0);
        self.area_yaw = w(cam_yaw - self.mode_offset_yaw - degrees(180));
        let (pos_y, focus_y) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        self.focus_on_mario(focus, pos, pos_y + y_off, focus_y + y_off, self.lakitu_dist as f64 + base_dist, pitch, cam_yaw);
        cam_yaw
    }

    /// `offset_yaw_outward_radial`
    pub fn offset_yaw_outward_radial(&mut self, area_yaw: i32) -> i32 {
        let mut yaw_goal = degrees(60);
        let mut yaw = self.mode_offset_yaw;
        match self.curr_level_area {
            AREA_TTC => {
                let center = [self.c.area_cen[0], self.mario.pos[1], self.c.area_cen[2]];
                if 800.0 > abs_dist(center, self.mario.pos) { yaw_goal = 0x3800; }
            }
            AREA_SSL_PYRAMID => {
                yaw_goal = (area_yaw & 0xC000) - area_yaw + degrees(45);
                if yaw_goal < 0 { yaw_goal = -yaw_goal; }
                yaw_goal = yaw_goal / 32 * 48;
            }
            AREA_LLL_OUTSIDE => yaw_goal = 0,
            _ => {}
        }
        let d_yaw = s16(self.mario.forward_vel / 32.0 * 128.0);
        if self.area_yaw_change < 0 { approach_s16_symmetric_bool(&mut yaw, -yaw_goal, d_yaw); }
        if self.area_yaw_change > 0 { approach_s16_symmetric_bool(&mut yaw, yaw_goal, d_yaw); }
        if yaw < -degrees(60) { approach_s16_symmetric_bool(&mut yaw, -yaw_goal, 0x200); }
        if yaw > degrees(60) { approach_s16_symmetric_bool(&mut yaw, yaw_goal, 0x200); }
        yaw
    }

    /// `radial_camera_move`
    pub fn radial_camera_move(&mut self, e: &Env) {
        let max_area_yaw = degrees(60);
        let min_area_yaw = degrees(-60);
        let mut rotate_speed = 0x1000;
        let m = self.mario.pos;
        let mut area_dx = m[0] - self.c.area_cen[0];
        let mut area_dz = m[2] - self.c.area_cen[2];
        let mut yaw_offset = w(calculate_yaw(m, self.c.pos) - atan2s(area_dz, area_dx));
        yaw_offset = yaw_offset.clamp(min_area_yaw, max_area_yaw);
        if self.movement_flags & CAM_MOVE_ROTATE == 0 {
            let (cur, prev) = (self.geo.curr_floor_type, self.geo.prev_floor_type);
            if cur == SURFACE_CAMERA_MIDDLE && prev != SURFACE_CAMERA_MIDDLE {
                self.movement_flags |= CAM_MOVE_RETURN_TO_MIDDLE | CAM_MOVE_ENTERED_ROTATE_SURFACE;
            }
            if cur == SURFACE_CAMERA_ROTATE_RIGHT && prev != SURFACE_CAMERA_ROTATE_RIGHT {
                self.movement_flags |= CAM_MOVE_ROTATE_RIGHT | CAM_MOVE_ENTERED_ROTATE_SURFACE;
            }
            if cur == SURFACE_CAMERA_ROTATE_LEFT && prev != SURFACE_CAMERA_ROTATE_LEFT {
                self.movement_flags |= CAM_MOVE_ROTATE_LEFT | CAM_MOVE_ENTERED_ROTATE_SURFACE;
            }
        }
        if self.movement_flags & CAM_MOVE_ENTERED_ROTATE_SURFACE != 0 {
            rotate_speed = 0x200;
        }
        if self.c.mode == CAMERA_MODE_OUTWARD_RADIAL {
            area_dx = -area_dx;
            area_dz = -area_dz;
        }
        let mut avoid_yaw = 0;
        let cpos = self.c.pos;
        let avoid_status = self.rotate_camera_around_walls(e, cpos, &mut avoid_yaw, 0x400);
        if avoid_status == 3 {
            if w(avoid_yaw - atan2s(area_dz, area_dx) + degrees(90)) < 0 {
                avoid_yaw = w(avoid_yaw + degrees(180));
            }
            avoid_yaw = w(avoid_yaw - atan2s(area_dz, area_dx));
            avoid_yaw = avoid_yaw.clamp(degrees(-105), degrees(105));
        }
        if self.movement_flags & CAM_MOVE_RETURN_TO_MIDDLE != 0 {
            if !approach_s16_symmetric_bool(&mut self.mode_offset_yaw, 0, rotate_speed) {
                self.movement_flags &= !CAM_MOVE_RETURN_TO_MIDDLE;
            }
        } else {
            if self.movement_flags & CAM_MOVE_ROTATE_RIGHT != 0 && avoid_status == 3 && avoid_yaw + 0x10 < self.mode_offset_yaw {
                self.mode_offset_yaw = avoid_yaw;
                self.movement_flags &= !(CAM_MOVE_ROTATE_RIGHT | CAM_MOVE_ENTERED_ROTATE_SURFACE);
            }
            if self.movement_flags & CAM_MOVE_ROTATE_LEFT != 0 && avoid_status == 3 && avoid_yaw - 0x10 > self.mode_offset_yaw {
                self.mode_offset_yaw = avoid_yaw;
                self.movement_flags &= !(CAM_MOVE_ROTATE_LEFT | CAM_MOVE_ENTERED_ROTATE_SURFACE);
            }
            if self.second_rotate_flags & CAM_MOVE_ROTATE_RIGHT == 0
                && self.movement_flags & CAM_MOVE_ROTATE_RIGHT != 0
                && !approach_s16_symmetric_bool(&mut self.mode_offset_yaw, max_area_yaw, rotate_speed)
            {
                self.movement_flags &= !(CAM_MOVE_ROTATE_RIGHT | CAM_MOVE_ENTERED_ROTATE_SURFACE);
            }
            if self.second_rotate_flags & CAM_MOVE_ROTATE_LEFT == 0
                && self.movement_flags & CAM_MOVE_ROTATE_LEFT != 0
                && !approach_s16_symmetric_bool(&mut self.mode_offset_yaw, min_area_yaw, rotate_speed)
            {
                self.movement_flags &= !(CAM_MOVE_ROTATE_LEFT | CAM_MOVE_ENTERED_ROTATE_SURFACE);
            }
            if self.second_rotate_flags & CAM_MOVE_ROTATE_RIGHT != 0
                && self.movement_flags & CAM_MOVE_ROTATE_RIGHT != 0
                && !approach_s16_symmetric_bool(&mut self.mode_offset_yaw, degrees(105), rotate_speed)
            {
                self.movement_flags &= !(CAM_MOVE_ROTATE_RIGHT | CAM_MOVE_ENTERED_ROTATE_SURFACE);
                self.second_rotate_flags &= !CAM_MOVE_ROTATE_RIGHT;
            }
            if self.second_rotate_flags & CAM_MOVE_ROTATE_LEFT != 0
                && self.movement_flags & CAM_MOVE_ROTATE_LEFT != 0
                && !approach_s16_symmetric_bool(&mut self.mode_offset_yaw, degrees(-105), rotate_speed)
            {
                self.movement_flags &= !(CAM_MOVE_ROTATE_LEFT | CAM_MOVE_ENTERED_ROTATE_SURFACE);
                self.second_rotate_flags &= !CAM_MOVE_ROTATE_LEFT;
            }
        }
        if self.movement_flags & CAM_MOVE_ROTATE == 0 {
            if avoid_status == 3 {
                approach_s16_asymptotic_bool(&mut self.mode_offset_yaw, avoid_yaw, 10);
            } else {
                if self.c.mode == CAMERA_MODE_RADIAL {
                    rotate_speed = s16(self.mario.forward_vel / 32.0 * 128.0);
                    approach_s16_symmetric_bool(&mut self.mode_offset_yaw, yaw_offset, rotate_speed);
                }
                if self.c.mode == CAMERA_MODE_OUTWARD_RADIAL {
                    self.mode_offset_yaw = self.offset_yaw_outward_radial(atan2s(area_dz, area_dx));
                }
            }
        }
        self.mode_offset_yaw = self.mode_offset_yaw.clamp(-0x5554, 0x5554);
    }

    /// `lakitu_zoom`
    pub fn lakitu_zoom(&mut self, range_dist: f64, mut range_pitch: i32) {
        let d = self.lakitu_dist as f64;
        if self.lakitu_dist < 0 {
            self.lakitu_dist += 30;
            if self.lakitu_dist > 0 { self.lakitu_dist = 0; }
        } else if range_dist < d {
            self.lakitu_dist -= 30;
            if (self.lakitu_dist as f64) < range_dist { self.lakitu_dist = range_dist as i32; }
        } else if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
            self.lakitu_dist += 30;
            if self.lakitu_dist as f64 > range_dist { self.lakitu_dist = range_dist as i32; }
        } else {
            self.lakitu_dist -= 30;
            if self.lakitu_dist < 0 { self.lakitu_dist = 0; }
        }
        if self.curr_level_area == AREA_SSL_PYRAMID && self.c.mode == CAMERA_MODE_OUTWARD_RADIAL {
            range_pitch /= 2;
        }
        if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
            self.lakitu_pitch += range_pitch / 13;
            if self.lakitu_pitch > range_pitch { self.lakitu_pitch = range_pitch; }
        } else {
            self.lakitu_pitch -= range_pitch / 13;
            if self.lakitu_pitch < 0 { self.lakitu_pitch = 0; }
        }
    }

    /// `update_yaw_and_dist_from_c_up`
    pub fn update_yaw_and_dist_from_c_up(&mut self) {
        let dist = 1000.0;
        self.mode_offset_yaw = w(self.mode_info.start.yaw - self.area_yaw);
        self.lakitu_dist = s16(self.mode_info.start.dist - dist);
        self.movement_flags &= !CAM_MOVING_INTO_MODE;
    }

    /// `radial_camera_input`
    pub fn radial_camera_input(&mut self, e: &Env) {
        let pressed = e.pad.pressed;
        if self.movement_flags & CAM_MOVE_ENTERED_ROTATE_SURFACE != 0 || self.movement_flags & CAM_MOVE_ROTATE == 0 {
            if pressed & (L_CBUTTONS | R_CBUTTONS) != 0 {
                self.movement_flags &= !CAM_MOVE_ENTERED_ROTATE_SURFACE;
            }
            if pressed & R_CBUTTONS != 0 {
                if self.mode_offset_yaw > -0x800 {
                    self.movement_flags |= CAM_MOVE_ROTATE_RIGHT;
                    if self.c.mode == CAMERA_MODE_RADIAL {
                        if self.mode_offset_yaw > 0x22AA { self.second_rotate_flags |= CAM_MOVE_ROTATE_RIGHT; }
                        self.sound(if self.mode_offset_yaw == degrees(105) { CamSound::ChangeBlocked } else { CamSound::CButtonSide });
                    } else {
                        self.sound(if self.mode_offset_yaw == degrees(60) { CamSound::ChangeBlocked } else { CamSound::CButtonSide });
                    }
                } else {
                    self.movement_flags |= CAM_MOVE_RETURN_TO_MIDDLE;
                    self.sound(CamSound::CButtonUp);
                }
            }
            if pressed & L_CBUTTONS != 0 {
                if self.mode_offset_yaw < 0x800 {
                    self.movement_flags |= CAM_MOVE_ROTATE_LEFT;
                    if self.c.mode == CAMERA_MODE_RADIAL {
                        if self.mode_offset_yaw < -0x22AA { self.second_rotate_flags |= CAM_MOVE_ROTATE_LEFT; }
                        self.sound(if self.mode_offset_yaw == degrees(-105) { CamSound::ChangeBlocked } else { CamSound::CButtonSide });
                    } else {
                        self.sound(if self.mode_offset_yaw == degrees(-60) { CamSound::ChangeBlocked } else { CamSound::CButtonSide });
                    }
                } else {
                    self.movement_flags |= CAM_MOVE_RETURN_TO_MIDDLE;
                    self.sound(CamSound::CButtonUp);
                }
            }
        }
        if pressed & U_CBUTTONS != 0 {
            if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
                self.movement_flags &= !CAM_MOVE_ZOOMED_OUT;
                self.sound(CamSound::CButtonUp);
            } else {
                self.set_mode_c_up();
            }
        }
        if pressed & D_CBUTTONS != 0 {
            if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
                self.movement_flags |= CAM_MOVE_ALREADY_ZOOMED_OUT;
                self.play_camera_buzz_if_cdown(e);
            } else {
                self.movement_flags |= CAM_MOVE_ZOOMED_OUT;
                self.sound(CamSound::CButtonDown);
            }
        }
    }

    /// `handle_c_button_movement`
    pub fn handle_c_button_movement(&mut self, e: &Env) {
        let pressed = e.pad.pressed;
        if pressed & U_CBUTTONS != 0 {
            if self.c.mode != CAMERA_MODE_FIXED && self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
                self.movement_flags &= !CAM_MOVE_ZOOMED_OUT;
                self.sound(CamSound::CButtonUp);
            } else {
                self.set_mode_c_up();
                self.zoom_amount = if self.zero_zoom_dist > self.camera_zoom_dist { -self.camera_zoom_dist } else { self.camera_zoom_dist };
            }
        }
        if self.c.mode != CAMERA_MODE_FIXED {
            if pressed & D_CBUTTONS != 0 {
                if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
                    self.movement_flags |= CAM_MOVE_ALREADY_ZOOMED_OUT;
                    self.zoom_amount = self.camera_zoom_dist + 400.0;
                    self.play_camera_buzz_if_cdown(e);
                } else {
                    self.movement_flags |= CAM_MOVE_ZOOMED_OUT;
                    self.zoom_amount = self.camera_zoom_dist + 400.0;
                    self.sound(CamSound::CButtonDown);
                }
            }
            let c_side_yaw = 0x1000;
            if pressed & R_CBUTTONS != 0 {
                if self.movement_flags & CAM_MOVE_ROTATE_LEFT != 0 {
                    self.movement_flags &= !CAM_MOVE_ROTATE_LEFT;
                } else {
                    self.movement_flags |= CAM_MOVE_ROTATE_RIGHT;
                    if self.c_side_button_yaw == 0 { self.sound(CamSound::CButtonSide); }
                    self.c_side_button_yaw = -c_side_yaw;
                }
            }
            if pressed & L_CBUTTONS != 0 {
                if self.movement_flags & CAM_MOVE_ROTATE_RIGHT != 0 {
                    self.movement_flags &= !CAM_MOVE_ROTATE_RIGHT;
                } else {
                    self.movement_flags |= CAM_MOVE_ROTATE_LEFT;
                    if self.c_side_button_yaw == 0 { self.sound(CamSound::CButtonSide); }
                    self.c_side_button_yaw = c_side_yaw;
                }
            }
        }
    }

    /// `mode_radial_camera`
    pub fn mode_radial_camera(&mut self, e: &Env) {
        let old_area_yaw = self.area_yaw;
        if self.movement_flags & CAM_MOVING_INTO_MODE != 0 {
            self.update_yaw_and_dist_from_c_up();
        }
        self.radial_camera_input(e);
        self.radial_camera_move(e);
        if self.c.mode == CAMERA_MODE_RADIAL {
            self.lakitu_zoom(400.0, 0x900);
        }
        let mut focus = self.c.focus;
        let mut pos = [0.0; 3];
        self.c.next_yaw = self.update_radial_camera(e, &mut focus, &mut pos);
        self.c.focus = focus;
        self.c.pos[0] = pos[0];
        self.c.pos[2] = pos[2];
        self.area_yaw_change = w(self.area_yaw - old_area_yaw);
        if self.mario.action == act::RIDING_HOOT { pos[1] += 500.0; }
        self.set_camera_height(e, pos[1]);
        self.pan_ahead_of_player();
    }

    /// `mode_8_directions_camera`
    pub fn mode_8_directions_camera(&mut self, e: &Env) {
        let old_area_yaw = self.area_yaw;
        self.radial_camera_input(e);
        if e.pad.pressed & R_CBUTTONS != 0 {
            self.dir8_yaw_offset = w(self.dir8_yaw_offset + degrees(45));
            self.sound(CamSound::CButtonSide);
        }
        if e.pad.pressed & L_CBUTTONS != 0 {
            self.dir8_yaw_offset = w(self.dir8_yaw_offset - degrees(45));
            self.sound(CamSound::CButtonSide);
        }
        self.lakitu_zoom(400.0, 0x900);
        let mut focus = self.c.focus;
        let mut pos = [0.0; 3];
        self.c.next_yaw = self.update_8_directions_camera(e, &mut focus, &mut pos, true);
        self.c.focus = focus;
        self.c.pos[0] = pos[0];
        self.c.pos[2] = pos[2];
        self.area_yaw_change = w(self.area_yaw - old_area_yaw);
        self.set_camera_height(e, pos[1]);
    }

    /// `mode_outward_radial_camera`
    pub fn mode_outward_radial_camera(&mut self, e: &Env) {
        let old_area_yaw = self.area_yaw;
        if self.movement_flags & CAM_MOVING_INTO_MODE != 0 {
            self.update_yaw_and_dist_from_c_up();
        }
        self.radial_camera_input(e);
        self.radial_camera_move(e);
        self.lakitu_zoom(400.0, 0x900);
        let mut focus = self.c.focus;
        let mut pos = [0.0; 3];
        self.c.next_yaw = self.update_outward_radial_camera(e, &mut focus, &mut pos);
        self.c.focus = focus;
        self.c.pos[0] = pos[0];
        self.c.pos[2] = pos[2];
        self.area_yaw_change = w(self.area_yaw - old_area_yaw);
        if self.mario.action == act::RIDING_HOOT { pos[1] += 500.0; }
        self.set_camera_height(e, pos[1]);
        self.pan_ahead_of_player();
    }

    // ---- parallel tracking ----

    /// `parallel_tracking_init`
    pub fn parallel_tracking_init(&mut self, path: Vec<ParPoint>) {
        if self.c.mode != CAMERA_MODE_PARALLEL_TRACKING {
            self.par_track_index = 0;
            self.par_track_trans_off = [0.0; 3];
            self.c.pos = [
                (path[0].pos[0] + path[1].pos[0]) / 2.0,
                (path[0].pos[1] + path[1].pos[1]) / 2.0,
                (path[0].pos[2] + path[1].pos[2]) / 2.0,
            ];
            self.par_track_path = path;
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            self.c.mode = CAMERA_MODE_PARALLEL_TRACKING;
        }
    }

    /// `update_parallel_tracking_camera`
    pub fn update_parallel_tracking_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3, alias: bool) -> i32 {
        let idx = self.par_track_index;
        let old_pos = *pos;
        let (p0, p1) = (self.par_track_path[idx].pos, self.par_track_path[idx + 1].pos);
        let (dist_thresh, zoom) = (self.par_track_path[idx].dist_thresh, self.par_track_path[idx].zoom);
        let (mario_floor_dist, _) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        let m = self.mario.pos;
        let mario_pos = [m[0], m[1] + 150.0 + mario_floor_dist, m[2]];
        let par_scale = 0.5;
        let par_mid = [p0[0] + (p1[0] - p0[0]) * par_scale, p0[1] + (p1[1] - p0[1]) * par_scale, p0[2] + (p1[2] - p0[2]) * par_scale];
        let (path_length, mut path_pitch, mut path_yaw) = dist_and_angle(p0, p1);
        let mut mario_offset = [mario_pos[0] - par_mid[0], mario_pos[1] - par_mid[1], mario_pos[2] - par_mid[2]];
        path_yaw = w(-path_yaw);
        mario_offset = rotate_in_xz(mario_offset, path_yaw);
        path_yaw = w(-path_yaw);
        path_pitch = w(-path_pitch);
        mario_offset = rotate_in_yz(mario_offset, path_pitch);
        path_pitch = w(-path_pitch);
        let mut foc_offset = mario_offset;
        foc_offset[0] = -foc_offset[0] * 0.0;
        foc_offset[1] *= 0.0;
        let mut cam_offset = [pos[0] - par_mid[0], pos[1] - par_mid[1], pos[2] - par_mid[2]];
        path_yaw = w(-path_yaw);
        cam_offset = rotate_in_xz(cam_offset, path_yaw);
        path_yaw = w(-path_yaw);
        path_pitch = w(-path_pitch);
        cam_offset = rotate_in_yz(cam_offset, path_pitch);
        path_pitch = w(-path_pitch);
        if mario_offset[2] > cam_offset[2] {
            if mario_offset[2] - cam_offset[2] > dist_thresh { cam_offset[2] = mario_offset[2] - dist_thresh; }
        } else if mario_offset[2] - cam_offset[2] < -dist_thresh {
            cam_offset[2] = mario_offset[2] + dist_thresh;
        }
        mario_offset[0] = -mario_offset[0] * zoom;
        mario_offset[1] *= zoom;
        mario_offset[2] = cam_offset[2];
        foc_offset[0] *= 0.3;
        foc_offset[1] *= 0.3;
        mario_offset[2] = path_length / 2.0 - mario_offset[2];
        let path_angle = [path_pitch, w(path_yaw + degrees(180)), 0];
        *pos = offset_rotated(p0, mario_offset, path_angle);
        if alias { self.c.pos = *pos; }
        foc_offset[2] = path_length / 2.0 - foc_offset[2];
        self.c.focus = offset_rotated(p0, foc_offset, path_angle);
        if alias { *focus = self.c.focus; }
        if self.status_flags & CAM_FLAG_CHANGED_PARTRACK_INDEX != 0 {
            self.status_flags &= !CAM_FLAG_CHANGED_PARTRACK_INDEX;
            for i in 0..3 { self.par_track_trans_off[i] = old_pos[i] - self.c.pos[i]; }
        }
        for i in 0..3 { approach_f32_asymptotic_bool(&mut self.par_track_trans_off[i], 0.0, 0.025); }
        let off = self.par_track_trans_off;
        add(&mut self.c.pos, off);
        let path = &self.par_track_path;
        if path[idx + 1].start_of_path != 0 {
            let (np, ny) = calculate_angles(path[idx + 1].pos, path[idx + 2].pos);
            let next_path_pos = set_dist_and_angle(path[idx + 1].pos, 400.0, np, ny);
            let dist_to_prev = abs_dist(mario_pos, next_path_pos);
            let (pp, py) = calculate_angles(path[idx + 1].pos, path[idx].pos);
            let prev_path_pos = set_dist_and_angle(path[idx + 1].pos, 400.0, pp, py);
            let dist_to_next = abs_dist(mario_pos, prev_path_pos);
            if dist_to_prev < dist_to_next {
                self.par_track_index += 1;
                self.status_flags |= CAM_FLAG_CHANGED_PARTRACK_INDEX;
            }
        }
        let idx = self.par_track_index;
        let path = &self.par_track_path;
        if idx != 0 {
            let (np, ny) = calculate_angles(path[idx].pos, path[idx + 1].pos);
            let next_path_pos = set_dist_and_angle(path[idx].pos, 700.0, np, ny);
            let dist_to_prev = abs_dist(mario_pos, next_path_pos);
            let (pp, py) = calculate_angles(path[idx].pos, path[idx - 1].pos);
            let prev_path_pos = set_dist_and_angle(path[idx].pos, 700.0, pp, py);
            let dist_to_next = abs_dist(mario_pos, prev_path_pos);
            if dist_to_prev > dist_to_next {
                self.par_track_index -= 1;
                self.status_flags |= CAM_FLAG_CHANGED_PARTRACK_INDEX;
            }
        }
        *focus = mario_pos;
        let (_d, _p, yaw) = dist_and_angle(*focus, *pos);
        yaw
    }

    /// `mode_parallel_tracking_camera`
    pub fn mode_parallel_tracking_camera(&mut self, e: &Env) {
        self.radial_camera_input(e);
        self.set_fov_function(CAM_FOV_DEFAULT);
        let mut focus = self.c.focus;
        let mut pos = self.c.pos;
        self.c.next_yaw = self.update_parallel_tracking_camera(e, &mut focus, &mut pos, true);
        // The decomp passes c->focus / c->pos as outputs: the trailing writes land in them.
        self.c.focus = focus;
        self.c.pos = pos;
    }

    // ---- fixed ----

    /// `set_camera_mode_fixed`
    pub fn set_camera_mode_fixed(&mut self, x: i32, y: i32, z: i32) -> bool {
        let (px, py, pz) = (x as f64, y as f64, z as f64);
        let mut base_pos_set = false;
        if self.fixed_mode_base_position != [px, py, pz] {
            base_pos_set = true;
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
        }
        self.fixed_mode_base_position = [px, py, pz];
        if self.c.mode != CAMERA_MODE_FIXED {
            self.status_flags &= !CAM_FLAG_SMOOTH_MOVEMENT;
            self.c.mode = CAMERA_MODE_FIXED;
            self.c.pos = [self.fixed_mode_base_position[0], self.mario.pos[1], self.fixed_mode_base_position[2]];
        }
        base_pos_set
    }

    /// `set_fixed_cam_axis_sa_lobby`
    pub fn set_fixed_cam_axis_sa_lobby(&mut self) {
        match self.curr_level_area {
            AREA_SA => self.fixed_mode_base_position = [646.0, 143.0, -1513.0],
            AREA_CASTLE_LOBBY => self.fixed_mode_base_position = [-577.0, 143.0, 1443.0],
            _ => {}
        }
    }

    /// `update_fixed_camera`
    pub fn update_fixed_camera(&mut self, e: &Env, focus: &mut V3) -> i32 {
        let mut scale_to_mario = 0.5;
        let mut height_offset = 0.0;
        self.play_camera_buzz_if_c_sideways(e);
        match self.curr_level_area {
            AREA_RR => { scale_to_mario = 0.0; height_offset = 0.0; }
            AREA_CASTLE_LOBBY => { scale_to_mario = 0.3; height_offset = 0.0; }
            AREA_BBH => { scale_to_mario = 0.0; height_offset = 0.0; }
            _ => {}
        }
        self.handle_c_button_movement(e);
        self.play_camera_buzz_if_cdown(e);
        let (focus_floor_off, _) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        *focus = self.mario.pos;
        focus[1] += focus_floor_off + 125.0;
        let (mut dist_cam_to_focus, face_pitch, face_yaw) = dist_and_angle(*focus, self.c.pos);
        let _ = face_pitch;
        let mut base_pos = self.fixed_mode_base_position;
        let ceo = self.castle_entrance_offset;
        add(&mut base_pos, ceo);
        let mut goal_height;
        if self.geo.curr_floor_type != SURFACE_DEATH_PLANE && self.geo.curr_floor_height != FLOOR_LOWER_LIMIT {
            goal_height = self.geo.curr_floor_height + base_pos[1] + height_offset;
        } else {
            goal_height = self.lakitu.goal_pos[1];
        }
        if 300.0 > dist_cam_to_focus { goal_height += 300.0 - dist_cam_to_focus; }
        let (mut ceil_height, _) = self.find_ceil(e, self.c.pos[0], goal_height - 100.0, self.c.pos[2]);
        if ceil_height != CELL_HEIGHT_LIMIT {
            ceil_height -= 125.0;
            if goal_height > ceil_height { goal_height = ceil_height; }
        }
        if self.status_flags & CAM_FLAG_SMOOTH_MOVEMENT != 0 {
            approach_f32_symmetric_bool(&mut self.c.pos[1], goal_height, 15.0);
        } else {
            if goal_height < self.mario.pos[1] - 500.0 { goal_height = self.mario.pos[1] - 500.0; }
            self.c.pos[1] = goal_height;
        }
        let m = self.mario.pos;
        self.c.pos[0] = base_pos[0] + (m[0] - base_pos[0]) * scale_to_mario;
        self.c.pos[2] = base_pos[2] + (m[2] - base_pos[2]) * scale_to_mario;
        if scale_to_mario != 0.0 {
            // c->focus (the live focus, not yet updated this frame when called through the mode)
            let (d, pitch, yaw) = dist_and_angle(self.c.focus, self.c.pos);
            dist_cam_to_focus = d;
            if dist_cam_to_focus > 1000.0 {
                dist_cam_to_focus = 1000.0;
                self.c.pos = set_dist_and_angle(self.c.focus, dist_cam_to_focus, pitch, yaw);
            }
        }
        face_yaw
    }

    /// `mode_fixed_camera`
    pub fn mode_fixed_camera(&mut self, e: &Env) {
        if e.level == LEVEL_BBH { self.set_fov_function(CAM_FOV_BBH); } else { self.set_fov_function(CAM_FOV_APP_45); }
        let mut focus = self.c.focus;
        self.c.next_yaw = self.update_fixed_camera(e, &mut focus);
        self.c.focus = focus;
        self.c.yaw = self.c.next_yaw;
        self.pan_ahead_of_player();
        self.castle_entrance_offset = [0.0; 3];
    }

    // ---- boss fight ----

    /// `update_boss_fight_camera`
    pub fn update_boss_fight_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3) -> i32 {
        self.handle_c_button_movement(e);
        if self.camera_event == CAM_EVENT_BOWSER_JUMP {
            self.set_environmental_camera_shake(SHAKE_ENV_BOWSER_JUMP);
            self.camera_event = 0;
        }
        if self.camera_event == CAM_EVENT_BOWSER_THROW_BOUNCE {
            self.set_environmental_camera_shake(SHAKE_ENV_BOWSER_THROW_BOUNCE);
            self.camera_event = 0;
        }
        let yaw = w(self.mode_offset_yaw + degrees(45));
        let m = self.mario;
        let (mut second_focus, held_state) = match e.second_focus {
            Some(s) => (s.pos, s.held_state),
            None => ([self.c.area_cen[0], m.pos[1], self.c.area_cen[2]], 0),
        };
        let mut focus_distance = abs_dist(m.pos, second_focus) * 1.6;
        focus_distance = focus_distance.clamp(800.0, 5000.0);
        if held_state == 1 {
            second_focus = offset_rotated(m.pos, [0.0, -150.0, -125.0], m.face_angle);
        }
        *focus = [
            (m.pos[0] + second_focus[0]) / 2.0,
            (m.pos[1] + second_focus[1]) / 2.0 + 125.0,
            (m.pos[2] + second_focus[2]) / 2.0,
        ];
        *pos = set_dist_and_angle(*focus, focus_distance, 0x1000, yaw);
        let (fh, floor) = self.find_floor(e, self.c.area_cen[0], CELL_HEIGHT_LIMIT, self.c.area_cen[2]);
        pos[1] = fh;
        if let Some(f) = floor {
            let [nx, ny, nz] = f.normal;
            pos[1] = 300.0 - (nx * pos[0] + nz * pos[2] + f.origin_offset) / ny;
            match self.curr_level_area {
                AREA_BOB => { pos[1] += 125.0; pos[1] += 125.0; }
                AREA_WF => pos[1] += 125.0,
                _ => {}
            }
        }
        if e.level == LEVEL_BBH { pos[1] = 2047.0; }
        if self.c_side_button_yaw < 0 {
            self.mode_offset_yaw = w(self.mode_offset_yaw + 0x200);
            self.c_side_button_yaw += 0x100;
            if self.c_side_button_yaw > 0 { self.c_side_button_yaw = 0; }
        }
        if self.c_side_button_yaw > 0 {
            self.mode_offset_yaw = w(self.mode_offset_yaw - 0x200);
            self.c_side_button_yaw -= 0x100;
            if self.c_side_button_yaw < 0 { self.c_side_button_yaw = 0; }
        }
        focus[1] = (m.pos[1] + second_focus[1]) / 2.0 + 100.0;
        if held_state == 1 {
            let av = if m.angle_vel_y > 0.0 { m.angle_vel_y } else { -m.angle_vel_y };
            focus[1] += 300.0 * sins(av);
        }
        if focus_distance < 400.0 { focus_distance = 400.0; }
        self.lakitu_zoom(focus_distance, 0x1800);
        *pos = set_dist_and_angle(*pos, self.lakitu_dist as f64, w(self.lakitu_pitch + 0x1000), yaw);
        yaw
    }

    /// `set_camera_mode_boss_fight`
    pub fn set_camera_mode_boss_fight(&mut self) {
        if self.c.mode != CAMERA_MODE_BOSS_FIGHT {
            self.transition_to_camera_mode(CAMERA_MODE_BOSS_FIGHT, 15);
            self.mode_offset_yaw = w(self.c.next_yaw - degrees(45));
        }
    }

    /// `mode_boss_fight_camera`
    pub fn mode_boss_fight_camera(&mut self, e: &Env) {
        let mut focus = self.c.focus;
        let mut pos = self.c.pos;
        self.c.next_yaw = self.update_boss_fight_camera(e, &mut focus, &mut pos);
        self.c.focus = focus;
        self.c.pos = pos;
    }

    // ---- behind Mario / water surface ----

    /// `update_behind_mario_camera`
    pub fn update_behind_mario_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3, alias: bool) -> i32 {
        let m = self.mario;
        let mut goal_pitch = w(-m.face_angle[0]);
        let mario_yaw = w(m.face_angle[1] + degrees(180));
        let mut goal_yaw_off = 0;
        let mut pitch_inc = 32;
        let mut max_dist = 800.0;
        let mut foc_y_off = 125.0;
        if self.selection_flags & CAM_MODE_MARIO_ACTIVE != 0 {
            max_dist = 350.0;
            foc_y_off = 120.0;
        }
        if m.action & (act::FLAG_SWIMMING | act::FLAG_METAL_WATER) == 0 {
            pitch_inc = 128;
        }
        *focus = m.pos;
        // `c->focus[1] += focYOff`: writes the live camera focus (== `focus` when called from the mode).
        if alias { focus[1] += foc_y_off; } else { self.c.focus[1] += foc_y_off; }
        let (mut dist, mut pitch, mut yaw) = dist_and_angle(*focus, *pos);
        if dist > max_dist { dist = max_dist; }
        let abs_pitch = pitch.abs();
        let mut yaw_speed = 32 - abs_pitch / 0x200;
        yaw_speed = yaw_speed.clamp(1, 32);
        if self.c_side_button_yaw != 0 {
            approach_s16_symmetric_bool(&mut self.c_side_button_yaw, 0, 1);
            yaw_speed = 8;
        }
        if self.behind_mario_sound_timer != 0 {
            goal_pitch = 0;
            approach_s16_symmetric_bool(&mut self.behind_mario_sound_timer, 0, 1);
            pitch_inc = 0x800;
        }
        if self.behind_mario_sound_timer == 28 && (self.c_side_button_yaw < 5 || self.c_side_button_yaw > 28) {
            self.sound(CamSound::CButtonUp);
        }
        if self.c_side_button_yaw == 28 && (self.behind_mario_sound_timer < 5 || self.behind_mario_sound_timer > 28) {
            self.sound(CamSound::CButtonUp);
        }
        let cb = self.c_buttons_pressed;
        if cb & L_CBUTTONS != 0 {
            if e.pad.pressed & L_CBUTTONS != 0 { self.sound(CamSound::CButtonSide); }
            if dist < max_dist { approach_f32_symmetric_bool(&mut dist, max_dist, 5.0); }
            goal_yaw_off = -0x3FF8;
            self.c_side_button_yaw = 30;
            yaw_speed = 2;
        }
        if cb & R_CBUTTONS != 0 {
            if e.pad.pressed & R_CBUTTONS != 0 { self.sound(CamSound::CButtonSide); }
            if dist < max_dist { approach_f32_symmetric_bool(&mut dist, max_dist, 5.0); }
            goal_yaw_off = 0x3FF8;
            self.c_side_button_yaw = 30;
            yaw_speed = 2;
        }
        if cb & D_CBUTTONS != 0 {
            if e.pad.pressed & (U_CBUTTONS | D_CBUTTONS) != 0 { self.sound(CamSound::CButtonSide); }
            if dist < max_dist { approach_f32_symmetric_bool(&mut dist, max_dist, 5.0); }
            goal_pitch = -0x3000;
            self.behind_mario_sound_timer = 30;
            pitch_inc = 0x800;
        }
        if cb & U_CBUTTONS != 0 {
            if e.pad.pressed & (U_CBUTTONS | D_CBUTTONS) != 0 { self.sound(CamSound::CButtonSide); }
            if dist < max_dist { approach_f32_symmetric_bool(&mut dist, max_dist, 5.0); }
            goal_pitch = 0x3000;
            self.behind_mario_sound_timer = 30;
            pitch_inc = 0x800;
        }
        approach_s16_asymptotic_bool(&mut yaw, w(mario_yaw + goal_yaw_off), yaw_speed);
        approach_s16_symmetric_bool(&mut pitch, goal_pitch, pitch_inc);
        if dist < 300.0 { dist = 300.0; }
        *pos = set_dist_and_angle(*focus, dist, pitch, yaw);
        match self.curr_level_area {
            AREA_WDW_MAIN => yaw = self.clamp_positions_and_find_yaw(pos, *focus, 4508.0, -3739.0, 4508.0, -3739.0),
            AREA_THI_HUGE => yaw = self.clamp_positions_and_find_yaw(pos, *focus, 8192.0, -8192.0, 8192.0, -8192.0),
            AREA_THI_TINY => yaw = self.clamp_positions_and_find_yaw(pos, *focus, 2458.0, -2458.0, 2458.0, -2458.0),
            _ => {}
        }
        yaw
    }

    /// `mode_behind_mario`
    pub fn mode_behind_mario(&mut self, e: &Env) -> i32 {
        self.movement_flags &= !CAM_MOVING_INTO_MODE;
        let mut new_pos = self.c.pos;
        let mut focus = self.c.focus;
        let yaw = self.update_behind_mario_camera(e, &mut focus, &mut new_pos, true);
        self.c.focus = focus;
        self.c.pos[0] = new_pos[0];
        self.c.pos[2] = new_pos[2];
        if self.c.mode == CAMERA_MODE_WATER_SURFACE {
            let mut floor_height = self.find_floor(e, self.c.pos[0], self.c.pos[1], self.c.pos[2]).0;
            new_pos[1] = e.m.water_level + 120.0;
            floor_height += 120.0;
            if new_pos[1] < floor_height { new_pos[1] = floor_height; }
        }
        self.approach_camera_height(new_pos[1], 50.0);
        let water_height = self.find_water_level(e, self.c.pos[0], self.c.pos[2]) + 100.0;
        if self.c.pos[1] <= water_height {
            self.movement_flags |= CAM_MOVE_SUBMERGED;
        } else {
            self.movement_flags &= !CAM_MOVE_SUBMERGED;
        }
        let mut p = self.c.pos;
        self.resolve_geometry_collisions(e, &mut p);
        self.c.pos = p;
        let (mut dist_cam_to_focus, cam_pitch, cam_yaw) = dist_and_angle(self.c.focus, self.c.pos);
        if dist_cam_to_focus > 800.0 {
            dist_cam_to_focus = 800.0;
            self.c.pos = set_dist_and_angle(self.c.focus, dist_cam_to_focus, cam_pitch, cam_yaw);
        }
        self.pan_ahead_of_player();
        yaw
    }

    pub fn mode_behind_mario_camera(&mut self, e: &Env) { self.c.next_yaw = self.mode_behind_mario(e); }
    pub fn mode_water_surface_camera(&mut self, e: &Env) { self.c.next_yaw = self.mode_behind_mario(e); }

    // ---- default (Lakitu / close / free roam / Mario angle) ----

    /// `update_mario_camera` (sModeTransitions for CLOSE / 7 / FREE_ROAM).
    pub fn update_mario_camera(&mut self, focus: &mut V3, pos: &mut V3) -> i32 {
        let yaw = w(self.mario.face_angle[1] + self.mode_offset_yaw + degrees(180));
        let zoom = self.camera_zoom_dist;
        self.focus_on_mario(focus, pos, 125.0, 125.0, zoom, 0x05B0, yaw);
        self.mario.face_angle[1]
    }

    /// `update_default_camera`
    pub fn update_default_camera(&mut self, e: &Env) -> i32 {
        let m = self.mario;
        let yaw_goal = w(m.face_angle[1] + degrees(180));
        let (ceil_height0, ceil) = self.find_ceil(e, self.lakitu.goal_pos[0], self.lakitu.goal_pos[1], self.lakitu.goal_pos[2]);
        let mut ceil_height = ceil_height0;
        self.handle_c_button_movement(e);
        let (mut dist, mut pitch, mut yaw) = dist_and_angle(m.pos, self.c.pos);
        let zoom_dist;
        let mut z;
        if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
            z = if self.set_cam_angle(0) == CAM_ANGLE_MARIO { self.camera_zoom_dist + 1050.0 } else { self.camera_zoom_dist + 400.0 };
        } else {
            z = self.camera_zoom_dist;
        }
        if m.action & act::FLAG_HANGING != 0 || m.action == act::RIDING_HOOT {
            z *= 0.8;
            self.set_handheld_shake(HAND_CAM_SHAKE_HANG_OWL);
        }
        zoom_dist = z;
        if self.zoom_amount == 0.0 {
            if dist > zoom_dist {
                dist -= 50.0;
                if dist < zoom_dist { dist = zoom_dist; }
            }
        } else {
            self.zoom_amount -= 30.0;
            if self.zoom_amount < 0.0 { self.zoom_amount = 0.0; }
            if dist > zoom_dist {
                dist -= 30.0;
                if dist < zoom_dist { dist = zoom_dist; }
            }
            if dist < zoom_dist {
                dist += 30.0;
                if dist > zoom_dist { dist = zoom_dist; }
            }
        }
        let mut next_yaw_vel;
        if self.c_side_button_yaw == 0 {
            next_yaw_vel = if self.c.mode == CAMERA_MODE_FREE_ROAM { 0xC0 } else { 0x100 };
            if e.pad.stick_x != 0.0 || e.pad.stick_y != 0.0 { next_yaw_vel = 0x20; }
        } else {
            if self.c_side_button_yaw < 0 { yaw = w(yaw + 0x200); }
            if self.c_side_button_yaw > 0 { yaw = w(yaw - 0x200); }
            approach_s16_symmetric_bool(&mut self.c_side_button_yaw, 0, 0x100);
            next_yaw_vel = 0;
        }
        self.yaw_speed = 0x400;
        let mut xz_dist = hor_dist(m.pos, self.c.pos);
        let mut close_to_mario = 0;
        if self.status_flags & CAM_FLAG_BEHIND_MARIO_POST_DOOR != 0 {
            if xz_dist >= 250.0 {
                self.status_flags &= !CAM_FLAG_BEHIND_MARIO_POST_DOOR;
            }
            if ((m.face_angle[1] - yaw) / 2).abs() < 0x1800 {
                self.status_flags &= !CAM_FLAG_BEHIND_MARIO_POST_DOOR;
                yaw = w(self.camera_yaw_after_door_cutscene + degrees(180));
                dist = 800.0;
                self.status_flags |= CAM_FLAG_BLOCK_SMOOTH_MOVEMENT;
            }
        } else if xz_dist < 250.0 {
            self.c.pos[0] += (250.0 - xz_dist) * sins(yaw as f64);
            self.c.pos[2] += (250.0 - xz_dist) * coss(yaw as f64);
            if self.c_side_button_yaw == 0 {
                next_yaw_vel = 0x1000;
                self.yaw_speed = 0;
                let (d, p, y) = dist_and_angle(m.pos, self.c.pos);
                dist = d;
                pitch = p;
                yaw = y;
            }
            close_to_mario |= 1;
        }
        if -16.0 < e.pad.stick_y {
            self.c.yaw = yaw;
        }
        let (mut pos_height, foc_height) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        let mut c_pos = self.c.pos;
        let mut avoid_yaw = 0;
        let mut avoid_status = self.rotate_camera_around_walls(e, c_pos, &mut avoid_yaw, 0x600);
        let mut yaw_vel = 0;
        if avoid_status == 3 {
            self.avoid_yaw_vel = yaw;
            self.status_flags |= CAM_FLAG_COLLIDED_WITH_WALL;
            let (xd, temp_pitch, temp_yaw) = dist_and_angle(m.pos, c_pos);
            xz_dist = xd;
            approach_s16_asymptotic_bool(&mut yaw, avoid_yaw, 10);
            c_pos = set_dist_and_angle(m.pos, xz_dist, temp_pitch, temp_yaw);
            self.avoid_yaw_vel = (self.avoid_yaw_vel - yaw) / 0x100;
        } else {
            if m.forward_vel == 0.0 {
                if self.status_flags & CAM_FLAG_COLLIDED_WITH_WALL != 0 {
                    let yaw_dir = if (yaw_goal - yaw) / 0x100 >= 0 { -1 } else { 1 };
                    if (self.avoid_yaw_vel > 0 && yaw_dir > 0) || (self.avoid_yaw_vel < 0 && yaw_dir < 0) {
                        yaw_vel = next_yaw_vel;
                    }
                } else {
                    yaw_vel = next_yaw_vel;
                }
            } else {
                if next_yaw_vel == 0x1000 { yaw_vel = next_yaw_vel; }
                self.status_flags &= !CAM_FLAG_COLLIDED_WITH_WALL;
            }
            if avoid_status != 0 { yaw_vel = w(yaw_vel + yaw_vel); }
            if close_to_mario & 1 != 0 && avoid_status != 0 { yaw_vel = 0; }
            if yaw_vel != 0 && !e.dialog_active {
                approach_s16_symmetric_bool(&mut yaw, yaw_goal, yaw_vel);
            }
        }
        if avoid_status == 0 && self.status_flags & CAM_FLAG_COLLIDED_WITH_WALL == 0 {
            approach_f32_asymptotic_bool(&mut dist, zoom_dist - 100.0, 0.05);
        }
        c_pos = set_dist_and_angle(m.pos, dist, pitch, yaw);
        c_pos[1] += pos_height + 125.0;
        if self.collide_with_walls(e, &mut c_pos, 10.0, 80.0) != 0 {
            self.status_flags |= CAM_FLAG_COLLIDED_WITH_WALL;
        }
        self.c.focus = [m.pos[0], m.pos[1] + 125.0 + foc_height, m.pos[2]];
        let mut mario_floor_height = 125.0 + self.geo.curr_floor_height;
        let mut mario_floor = self.geo.curr_floor.clone();
        let (cf, _) = self.find_floor(e, c_pos[0], c_pos[1] + 50.0, c_pos[2]);
        let mut cam_floor_height = cf + 125.0;
        let mut scale = 0.1;
        while scale < 1.0 {
            let temp = scale_along_line(c_pos, m.pos, scale);
            let (th, tf) = self.find_floor(e, temp[0], temp[1], temp[2]);
            let th = th + 125.0;
            if tf.is_some() && th > mario_floor_height {
                mario_floor_height = th;
                mario_floor = tf;
            }
            scale += 0.2;
        }
        if self.selection_flags & CAM_MODE_MARIO_ACTIVE != 0 {
            mario_floor_height -= 35.0;
            cam_floor_height -= 35.0;
            self.c.focus[1] -= 25.0;
        }
        let mut water_height = self.find_water_level(e, c_pos[0], c_pos[2]);
        if water_height != FLOOR_LOWER_LIMIT {
            water_height += 125.0;
            let dist_from_water = water_height - mario_floor_height;
            if self.movement_flags & CAM_MOVE_METAL_BELOW_WATER == 0 {
                if dist_from_water > 800.0 && m.action & act::FLAG_METAL_WATER != 0 {
                    self.movement_flags |= CAM_MOVE_METAL_BELOW_WATER;
                }
            } else if dist_from_water < 400.0 || m.action & act::FLAG_METAL_WATER == 0 {
                self.movement_flags &= !CAM_MOVE_METAL_BELOW_WATER;
            }
            if self.movement_flags & CAM_MOVE_METAL_BELOW_WATER == 0 && cam_floor_height < water_height {
                cam_floor_height = water_height;
            }
        } else {
            self.movement_flags &= !CAM_MOVE_METAL_BELOW_WATER;
        }
        c_pos[1] = cam_floor_height;
        let mut temp_pos = c_pos;
        temp_pos[1] -= 125.0;
        if mario_floor.is_some() && cam_floor_height <= mario_floor_height {
            avoid_status = Self::is_range_behind_surface(self.c.focus, temp_pos, mario_floor.as_deref(), 0, -1);
            if avoid_status != 1 && ceil_height > mario_floor_height {
                cam_floor_height = mario_floor_height;
            }
        }
        pos_height = 0.0;
        if self.c.mode == CAMERA_MODE_FREE_ROAM {
            if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 {
                pos_height = 375.0;
                if self.curr_level_area == AREA_SSL_PYRAMID { pos_height /= 2.0; }
            } else {
                pos_height = 100.0;
            }
        }
        if self.movement_flags & CAM_MOVE_ZOOMED_OUT != 0 && self.selection_flags & CAM_MODE_MARIO_ACTIVE != 0 {
            pos_height = 610.0;
            if self.curr_level_area == AREA_SSL_PYRAMID || e.level == LEVEL_CASTLE { pos_height /= 2.0; }
        }
        // find_poison_gas_level: no gas rooms are ported here (HMC / BBH gas is returned as FLOOR_LOWER_LIMIT).
        if m.action & act::FLAG_HANGING != 0 || m.action == act::RIDING_HOOT {
            cam_floor_height = m.pos[1] + 400.0;
            if self.c.mode == CAMERA_MODE_FREE_ROAM { cam_floor_height -= 100.0; }
            ceil_height = CELL_HEIGHT_LIMIT;
            self.c.focus = m.pos;
        }
        if m.action & act::FLAG_ON_POLE != 0 {
            if let Some(o) = m.used_obj {
                cam_floor_height = o.pos[1] + 125.0;
                if m.pos[1] - 100.0 > cam_floor_height { cam_floor_height = m.pos[1] - 100.0; }
            }
            ceil_height = CELL_HEIGHT_LIMIT;
            self.c.focus = m.pos;
        }
        if cam_floor_height != FLOOR_LOWER_LIMIT {
            cam_floor_height += pos_height;
            self.approach_camera_height(cam_floor_height, 20.0);
        }
        self.c.pos[0] = c_pos[0];
        self.c.pos[2] = c_pos[2];
        let c2 = [self.lakitu.goal_pos[0], self.c.pos[1], self.lakitu.goal_pos[2]];
        let (mut d, tp, ty) = dist_and_angle(c2, self.c.pos);
        if d > 50.0 {
            d = 50.0;
            self.c.pos = set_dist_and_angle(c2, d, tp, ty);
        }
        if self.geo.curr_floor_type != SURFACE_DEATH_PLANE {
            let (mut d, tp, ty) = dist_and_angle(self.c.focus, self.c.pos);
            if d > zoom_dist {
                d = zoom_dist;
                self.c.pos = set_dist_and_angle(self.c.focus, d, tp, ty);
            }
        }
        if ceil_height != CELL_HEIGHT_LIMIT {
            ceil_height -= 150.0;
            if self.c.pos[1] > ceil_height && Self::is_range_behind_surface(self.c.pos, m.pos, ceil.as_deref(), 0, -1) == 1 {
                self.c.pos[1] = ceil_height;
            }
        }
        if self.curr_level_area == AREA_WDW_TOWN {
            let mut p = self.c.pos;
            let f = self.c.focus;
            yaw = self.clamp_positions_and_find_yaw(&mut p, f, 2254.0, -3789.0, 3790.0, -2253.0);
            self.c.pos = p;
        }
        yaw
    }

    pub fn mode_default_camera(&mut self, e: &Env) {
        self.set_fov_function(CAM_FOV_DEFAULT);
        self.c.next_yaw = self.update_default_camera(e);
        self.pan_ahead_of_player();
    }
    pub fn mode_lakitu_camera(&mut self, e: &Env) {
        self.camera_zoom_dist = 800.0;
        self.mode_default_camera(e);
    }
    pub fn mode_mario_camera(&mut self, e: &Env) {
        self.camera_zoom_dist = 350.0;
        self.mode_default_camera(e);
    }

    // ---- spiral stairs ----

    /// `update_spiral_stairs_camera`
    pub fn update_spiral_stairs_camera(&mut self, e: &Env, focus: &mut V3, pos: &mut V3) -> i32 {
        self.handle_c_button_movement(e);
        self.fixed_mode_base_position = [-1280.0, 614.0, 1740.0];
        let (focus_height, _) = self.calc_y_to_curr_floor(e, 1.0, 200.0, 0.9, 200.0);
        let m = self.mario.pos;
        focus[0] = m[0];
        let foc_y = m[1] + 125.0 + focus_height;
        focus[2] = m[2];
        let mut c_pos = *pos;
        let base = self.fixed_mode_base_position;
        let (_d, _fp, mut foc_yaw) = dist_and_angle(base, *focus);
        let (_d, _pp, pos_yaw0) = dist_and_angle(base, c_pos);
        self.spiral_stairs_yaw_offset = w(pos_yaw0 - foc_yaw).clamp(degrees(-90), degrees(90));
        foc_yaw = w(foc_yaw + self.spiral_stairs_yaw_offset);
        let mut pos_yaw = foc_yaw;
        approach_s16_symmetric_bool(&mut pos_yaw, foc_yaw, 0x1000);
        c_pos = set_dist_and_angle(base, 300.0, 0, pos_yaw);
        let check_pos = [
            focus[0] + (c_pos[0] - focus[0]) * 0.7,
            focus[1] + (c_pos[1] - focus[1]) * 0.7 + 300.0,
            focus[2] + (c_pos[2] - focus[2]) * 0.7,
        ];
        let (mut floor_height, _) = self.find_floor(e, check_pos[0], check_pos[1] + 50.0, check_pos[2]);
        if floor_height != FLOOR_LOWER_LIMIT {
            if floor_height < self.geo.curr_floor_height { floor_height = self.geo.curr_floor_height; }
            floor_height += 125.0;
            pos[1] = approach_f32(pos[1], floor_height, 30.0, 30.0);
        }
        approach_f32_symmetric_bool(&mut focus[1], foc_y, 30.0);
        pos[0] = c_pos[0];
        pos[2] = c_pos[2];
        calculate_yaw(*focus, *pos)
    }

    pub fn mode_spiral_stairs_camera(&mut self, e: &Env) {
        let mut focus = self.c.focus;
        let mut pos = self.c.pos;
        self.c.next_yaw = self.update_spiral_stairs_camera(e, &mut focus, &mut pos);
        self.c.focus = focus;
        self.c.pos = pos;
    }

    // ---- slide / Hoot ----

    /// `update_slide_or_0f_camera`
    pub fn update_slide_or_0f_camera(&mut self, focus: &mut V3, pos: &mut V3) -> i32 {
        let yaw = w(self.mario.face_angle[1] + self.mode_offset_yaw + degrees(180));
        self.focus_on_mario(focus, pos, 125.0, 125.0, 800.0, 5461, yaw);
        self.mario.face_angle[1]
    }

    /// `update_slide_camera`
    pub fn update_slide_camera(&mut self, e: &Env) -> i32 {
        let m = self.mario;
        let mut goal_pitch = 0x1555;
        let goal_yaw = w(m.face_angle[1] + degrees(180));
        if self.status_flags & CAM_FLAG_CCM_SLIDE_SHORTCUT != 0 {
            self.lakitu_dist = approach_f32(self.lakitu_dist as f64, -600.0, 20.0, 20.0) as i32;
        } else {
            self.lakitu_dist = approach_f32(self.lakitu_dist as f64, 0.0, 20.0, 20.0) as i32;
        }
        self.play_camera_buzz_if_cbutton(e);
        self.c.focus = m.pos;
        self.c.focus[1] += 50.0;
        let (mut dist_cam_to_focus, mut cam_pitch, mut cam_yaw);
        (dist_cam_to_focus, cam_pitch, cam_yaw) = dist_and_angle(self.c.focus, self.c.pos);
        let mut max_cam_dist = 800.0;
        if m.action == act::RIDING_HOOT {
            max_cam_dist = 1000.0;
            goal_pitch = 0x2800;
            approach_s16_symmetric_bool(&mut cam_yaw, goal_yaw, 0x100);
        } else {
            approach_s16_symmetric_bool(&mut cam_yaw, goal_yaw, 0x80);
        }
        approach_s16_symmetric_bool(&mut cam_pitch, goal_pitch, 0x100);
        let ld = self.lakitu_dist as f64;
        if m.action != act::RIDING_HOOT && self.geo.curr_floor_type == SURFACE_DEATH_PLANE {
            let pos = set_dist_and_angle(self.c.focus, max_cam_dist + ld, cam_pitch, cam_yaw);
            self.c.pos[0] = pos[0];
            self.c.pos[2] = pos[2];
            let fy = self.c.focus[1];
            approach_f32_symmetric_bool(&mut self.c.pos[1], fy, 30.0);
            let (d, p, y) = dist_and_angle(self.c.pos, self.c.focus);
            dist_cam_to_focus = d;
            cam_pitch = p;
            cam_yaw = y;
            let mut pitch_scale = (dist_cam_to_focus - max_cam_dist + ld) / 10000.0;
            if pitch_scale > 1.0 { pitch_scale = 1.0; }
            cam_pitch = w(cam_pitch + s16(0x1000 as f64 * pitch_scale));
            self.c.focus = set_dist_and_angle(self.c.pos, dist_cam_to_focus, cam_pitch, cam_yaw);
        } else {
            self.c.pos = set_dist_and_angle(self.c.focus, max_cam_dist + ld, cam_pitch, cam_yaw);
            self.status_flags |= CAM_FLAG_BLOCK_SMOOTH_MOVEMENT;
            let floor_height = self.find_floor(e, self.c.pos[0], self.c.pos[1] + 200.0, self.c.pos[2]).0 + 125.0;
            if self.c.pos[1] < floor_height { self.c.pos[1] = floor_height; }
            let (d, p, y) = dist_and_angle(self.c.focus, self.c.pos);
            dist_cam_to_focus = d;
            cam_pitch = p;
            cam_yaw = y;
            if dist_cam_to_focus > max_cam_dist + ld {
                dist_cam_to_focus = max_cam_dist + ld;
                self.c.pos = set_dist_and_angle(self.c.focus, dist_cam_to_focus, cam_pitch, cam_yaw);
            }
        }
        let _ = (cam_pitch, cam_yaw);
        calculate_yaw(self.c.focus, self.c.pos)
    }

    /// `mode_slide_camera`
    pub fn mode_slide_camera(&mut self, e: &Env) {
        if self.geo.curr_floor_type == SURFACE_CLOSE_CAMERA || self.geo.curr_floor_type == SURFACE_NO_CAM_COL_SLIPPERY {
            self.mode_lakitu_camera(e);
        } else {
            if e.pad.pressed & U_CBUTTONS != 0 {
                self.movement_flags |= CAM_MOVE_C_UP_MODE;
            }
            self.c.next_yaw = self.update_slide_camera(e);
        }
    }

    // ---- C-up ----

    /// `store_lakitu_cam_info_for_c_up`
    pub fn store_lakitu_cam_info_for_c_up(&mut self) {
        let mp = self.mario.pos;
        let mut p = self.c.pos;
        sub(&mut p, mp);
        self.camera_store_c_up.pos = p;
        self.camera_store_c_up.focus = [0.0, self.c.focus[1] - mp[1], 0.0];
    }

    /// `set_mode_c_up`
    pub fn set_mode_c_up(&mut self) -> bool {
        if self.movement_flags & CAM_MOVE_C_UP_MODE == 0 {
            self.movement_flags |= CAM_MOVE_C_UP_MODE;
            self.store_lakitu_cam_info_for_c_up();
            self.sound_flags &= !1;
            true
        } else {
            false
        }
    }

    /// `exit_c_up`
    pub fn exit_c_up(&mut self, e: &Env) {
        if self.movement_flags & CAM_MOVE_C_UP_MODE != 0 && self.movement_flags & CAM_MOVE_STARTED_EXITING_C_UP == 0 {
            let mut check_foc = self.c.focus;
            check_foc[0] = self.mario.pos[0];
            check_foc[2] = self.mario.pos[2];
            let (mut cur_dist, cur_pitch, cur_yaw) = dist_and_angle(check_foc, self.c.pos);
            let mut cur_pos = self.c.pos;
            cur_dist = 80.0;
            let mut check_yaw = 0;
            let last = self.mode_info.last_mode;
            if last == CAMERA_MODE_SPIRAL_STAIRS || last == CAMERA_MODE_CLOSE || last == CAMERA_MODE_FREE_ROAM {
                let mut searching = true;
                let mut sector = 0;
                while sector < 16 && searching {
                    cur_pos = set_dist_and_angle(check_foc, cur_dist, 0, w(cur_yaw + check_yaw));
                    if self.wall_push(e, &mut cur_pos, 20.0, 50.0) == 0 {
                        let mut d = cur_dist;
                        while d < self.camera_zoom_dist {
                            cur_pos = set_dist_and_angle(check_foc, d, 0, w(cur_yaw + check_yaw));
                            let (ch, cs) = self.find_ceil(e, cur_pos[0], cur_pos[1] - 150.0, cur_pos[2]);
                            if cs.is_some() && ch - 10.0 < cur_pos[1] { break; }
                            let (fh, fs) = self.find_floor(e, cur_pos[0], cur_pos[1] + 150.0, cur_pos[2]);
                            if fs.is_some() && fh + 10.0 > cur_pos[1] { break; }
                            if self.wall_push(e, &mut cur_pos, 20.0, 50.0) == 1 { break; }
                            d += 20.0;
                        }
                        if d >= self.camera_zoom_dist { searching = false; }
                    }
                    if searching {
                        check_yaw = -check_yaw;
                        if check_yaw < 0 { check_yaw -= 0x1000; } else { check_yaw += 0x1000; }
                    }
                    sector += 1;
                }
                if !searching {
                    self.camera_store_c_up.pos = set_dist_and_angle(check_foc, self.camera_zoom_dist, 0, w(cur_yaw + check_yaw));
                    self.camera_store_c_up.focus = check_foc;
                    let mp = self.mario.pos;
                    sub(&mut self.camera_store_c_up.pos, mp);
                    sub(&mut self.camera_store_c_up.focus, mp);
                }
                self.movement_flags |= CAM_MOVE_STARTED_EXITING_C_UP;
                self.transition_next_state(15);
            } else {
                self.movement_flags &= !(CAM_MOVE_STARTED_EXITING_C_UP | CAM_MOVE_C_UP_MODE);
                self.c.pos = set_dist_and_angle(check_foc, cur_dist, cur_pitch, w(cur_yaw + check_yaw));
            }
            let _ = cur_pos;
            self.sound(CamSound::CButtonDown);
        }
    }

    /// `update_c_up`
    pub fn update_c_up(&mut self, focus: &mut V3, pos: &mut V3) -> i32 {
        let pitch = self.c_up_camera_pitch;
        let yaw = w(self.mario.face_angle[1] + self.mode_offset_yaw + degrees(180));
        self.focus_on_mario(focus, pos, 125.0, 125.0, 250.0, pitch, yaw);
        self.mario.face_angle[1]
    }

    /// `move_mario_head_c_up`
    pub fn move_mario_head_c_up(&mut self, e: &Env) {
        self.c_up_camera_pitch = w(self.c_up_camera_pitch + s16((e.pad.stick_y * 10.0).trunc()));
        self.mode_offset_yaw = w(self.mode_offset_yaw - s16((e.pad.stick_x * 10.0).trunc()));
        self.c_up_camera_pitch = self.c_up_camera_pitch.clamp(-0x2000, 0x38E3);
        self.mode_offset_yaw = self.mode_offset_yaw.clamp(-0x5555, 0x5555);
        self.head_rotation[0] = self.c_up_camera_pitch * 3 / 4;
        self.head_rotation[1] = self.mode_offset_yaw * 3 / 4;
    }

    /// `move_into_c_up`
    pub fn move_into_c_up(&mut self) {
        let (start, end) = (self.mode_info.start, self.mode_info.end);
        let (frame, max) = (self.mode_info.frame as f64, self.mode_info.max as f64);
        let dist = start.dist + (end.dist - start.dist) * frame / max;
        let pitch = start.pitch + s16(w(end.pitch - start.pitch) as f64 * frame / max);
        let yaw = start.yaw + s16(w(end.yaw - start.yaw) as f64 * frame / max);
        for i in 0..3 {
            self.c.focus[i] = start.focus[i] + (end.focus[i] - start.focus[i]) * frame / max;
        }
        let mp = self.mario.pos;
        add(&mut self.c.focus, mp);
        self.c.pos = set_dist_and_angle(self.c.focus, dist, pitch, yaw);
        self.head_rotation = [0, 0];
        self.mode_info.frame += 1;
        if self.mode_info.frame == self.mode_info.max {
            self.movement_flags &= !CAM_MOVING_INTO_MODE;
        }
    }

    /// `mode_c_up_camera`
    pub fn mode_c_up_camera(&mut self, e: &Env) -> bool {
        if self.sound_flags & 1 == 0 {
            self.sound(CamSound::CButtonUp);
            self.sound_flags |= 1;
        }
        if self.movement_flags & CAM_MOVING_INTO_MODE != 0 {
            self.movement_flags |= CAM_MOVE_C_UP_MODE;
            self.move_into_c_up();
            return true;
        }
        if self.movement_flags & CAM_MOVE_STARTED_EXITING_C_UP == 0 {
            self.move_mario_head_c_up(e);
            let mut f = self.c.focus;
            let mut p = self.c.pos;
            self.update_c_up(&mut f, &mut p);
            self.c.focus = f;
            self.c.pos = p;
        } else if self.status_flags & CAM_FLAG_TRANSITION_OUT_OF_C_UP != 0 {
            let mp = self.mario.pos;
            self.c.pos = self.camera_store_c_up.pos;
            add(&mut self.c.pos, mp);
            self.c.focus = self.camera_store_c_up.focus;
            add(&mut self.c.focus, mp);
            approach_s16_symmetric_bool(&mut self.head_rotation[0], 0, 1024);
            approach_s16_symmetric_bool(&mut self.head_rotation[1], 0, 1024);
        } else {
            self.movement_flags &= !(CAM_MOVE_STARTED_EXITING_C_UP | CAM_MOVE_C_UP_MODE);
        }
        self.pan_distance = 0.0;
        if e.pad.pressed & (A_BUTTON | B_BUTTON | D_CBUTTONS | L_CBUTTONS | R_CBUTTONS) != 0 {
            self.exit_c_up(e);
        }
        false
    }

    // ---- inside cannon ----

    /// `update_in_cannon` (note the swapped focus / pos arguments to `focus_on_mario` in the decomp).
    pub fn update_in_cannon(&mut self, focus: &mut V3, pos: &mut V3) -> i32 {
        let (pitch, yaw) = (self.mario.face_angle[0], self.mario.face_angle[1]);
        let off = self.cannon_y_offset;
        self.focus_on_mario(pos, focus, 125.0 + off, 125.0, 800.0, pitch, yaw);
        self.mario.face_angle[1]
    }

    /// `mode_cannon_camera`
    pub fn mode_cannon_camera(&mut self, e: &Env) {
        self.lakitu_pitch = 0;
        self.movement_flags &= !CAM_MOVING_INTO_MODE;
        let mut focus = self.c.focus;
        let mut pos = self.c.pos;
        self.c.next_yaw = self.update_in_cannon(&mut focus, &mut pos);
        self.c.focus = focus;
        self.c.pos = pos;
        if e.pad.pressed & A_BUTTON != 0 {
            self.set_camera_mode(e, CAMERA_MODE_BEHIND_MARIO, 1);
            self.pan_distance = 0.0;
            self.cannon_y_offset = 0.0;
            self.status_flags &= !CAM_FLAG_BLOCK_SMOOTH_MOVEMENT;
        } else {
            self.cannon_y_offset = approach_f32(self.cannon_y_offset, 0.0, 100.0, 100.0);
        }
    }

    /// `sModeTransitions[mode](c, focus, pos)`: used by [`SmCam::set_camera_mode`] to find the
    /// destination pose of a mode switch. Returns (yaw, focus, pos).
    pub fn mode_transition_fn(&mut self, e: &Env, mode: i32, mut focus: V3, mut pos: V3) -> (i32, V3, V3) {
        let yaw = match mode {
            CAMERA_MODE_RADIAL => self.update_radial_camera(e, &mut focus, &mut pos),
            CAMERA_MODE_OUTWARD_RADIAL => self.update_outward_radial_camera(e, &mut focus, &mut pos),
            CAMERA_MODE_BEHIND_MARIO => self.update_behind_mario_camera(e, &mut focus, &mut pos, false),
            CAMERA_MODE_CLOSE | 7 | CAMERA_MODE_FREE_ROAM => self.update_mario_camera(&mut focus, &mut pos),
            CAMERA_MODE_C_UP => self.update_c_up(&mut focus, &mut pos),
            CAMERA_MODE_SLIDE_HOOT | 15 => self.update_slide_or_0f_camera(&mut focus, &mut pos),
            CAMERA_MODE_INSIDE_CANNON => self.update_in_cannon(&mut focus, &mut pos),
            CAMERA_MODE_BOSS_FIGHT => self.update_boss_fight_camera(e, &mut focus, &mut pos),
            CAMERA_MODE_PARALLEL_TRACKING if !self.par_track_path.is_empty() => self.update_parallel_tracking_camera(e, &mut focus, &mut pos, false),
            CAMERA_MODE_FIXED => self.update_fixed_camera(e, &mut focus),
            CAMERA_MODE_8_DIRECTIONS => self.update_8_directions_camera(e, &mut focus, &mut pos, false),
            CAMERA_MODE_SPIRAL_STAIRS => self.update_spiral_stairs_camera(e, &mut focus, &mut pos),
            _ => 0,
        };
        (yaw, focus, pos)
    }
}
