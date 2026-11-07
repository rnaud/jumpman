//! Port of `src/game/cannons.ts` (the cannon lids/barrels, entering, unlock animation and the
//! cannon camera) and `cannonOperator.ts` (the Bob-omb buddy who opens them). Engine-free.
use super::collider::Collider;
use super::course_data::{CannonPlacement, CollisionData};
use crate::camera::transition::{place, polar, Polar};
use bevy::math::DVec3;
use jumpman_sim::actions::cannon::CannonSeat;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::math::*;
use jumpman_sim::object::ObjId;
use jumpman_sim::state::*;
use std::sync::Arc;

pub const CANNON_BASE: &str = "actors/cannon_base/cannon_base_geo.glb";
pub const CANNON_BARREL: &str = "actors/cannon_barrel/cannon_barrel_geo.glb";
/// `Surface::object` tag base for cannon lid covers.
pub const COVER_TAG: u32 = 0x5000_0000;

pub struct Cannon {
    pub seat: CannonSeat,
    pub lid_pos: DVec3,
    pub lid_yaw_degrees: f64,
    pub lid_visible: bool,
    pub base_pos: DVec3,
    pub base_yaw: f64,
    pub barrel_pitch: f64,
    pub parts_visible: bool,
    cover: Collider,
    occupied: bool,
}

pub struct Cannons {
    pub entries: Vec<Cannon>,
    pub open: bool,
    opening: i32,
}

impl Cannons {
    pub fn new(placements: &[CannonPlacement], lid_collision: Option<&CollisionData>) -> Self {
        let entries = placements
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let base_yaw = to_s16(((spec.param as i64) << 8) as f64) as f64;
                Cannon {
                    seat: CannonSeat { home: spec.pos, base_yaw, yaw: base_yaw, pitch: 0.0, rise: 0.0, shake: 0.0, input_yaw: 0.0, cooldown: 0.0 },
                    lid_pos: DVec3::from(spec.pos),
                    lid_yaw_degrees: spec.yaw,
                    lid_visible: true,
                    base_pos: DVec3::from(spec.pos),
                    base_yaw,
                    barrel_pitch: 0.0,
                    parts_visible: false,
                    cover: Collider::new(lid_collision.cloned().unwrap_or_default(), spec.yaw, COVER_TAG + i as u32),
                    occupied: false,
                }
            })
            .collect();
        Self { entries, open: false, opening: -1 }
    }

    /// `reset(unlocked)`.
    pub fn reset(&mut self, world: &mut Arc<jumpman_sim::collision::CollisionWorld>, unlocked: bool) {
        self.open = unlocked;
        self.opening = -1;
        for c in &mut self.entries {
            c.lid_pos = DVec3::from(c.seat.home);
            c.cover.place(world, if unlocked { None } else { Some(c.seat.home) });
            c.lid_visible = !unlocked;
            c.parts_visible = false;
            c.occupied = false;
            c.seat.cooldown = 0.0;
            c.seat.pitch = 0.0;
            c.seat.shake = 0.0;
            c.seat.rise = 0.0;
            c.seat.yaw = c.seat.base_yaw;
        }
    }

    /// `unlock(from)`: start the lid animation; returns the nearest cannon's position.
    pub fn unlock(&mut self, from: [f64; 3]) -> Option<[f64; 3]> {
        if self.open || self.opening >= 0 || self.entries.is_empty() {
            return None;
        }
        self.opening = 0;
        let distance = |c: &Cannon| js_hypot3(c.seat.home[0] - from[0], c.seat.home[1] - from[1], c.seat.home[2] - from[2]);
        let mut nearest = &self.entries[0];
        for c in &self.entries {
            if distance(c) < distance(nearest) {
                nearest = c;
            }
        }
        Some(nearest.seat.home)
    }

    pub fn opening(&self) -> bool {
        self.opening >= 0
    }

    /// Frames since the lids started to move, for the door sound (`t === 61`).
    pub fn opening_tick(&self) -> i32 {
        self.opening
    }

    pub fn step(&mut self, m: &mut MarioRuntime) {
        if self.opening >= 0 {
            let t = self.opening - 61;
            self.opening += 1;
            if t == 80 {
                let world = &mut m.world;
                self.reset(world, true);
            } else if t >= 0 {
                for c in &mut self.entries {
                    if t < 30 {
                        c.lid_pos.y -= 0.5;
                    } else {
                        c.lid_pos.x += 4.0;
                    }
                    let pos = c.lid_pos.to_array();
                    c.cover.place(&mut m.world, Some(pos));
                }
            }
        }
        if !self.open {
            return;
        }
        let fired = m.events.iter().any(|e| matches!(e, MarioEvent::CannonFire));
        for c in &mut self.entries {
            let [x, y, z] = c.seat.home;
            let base_y = y - 340.0;
            let dist = js_hypot(m.pos[0] - x, m.pos[2] - z);
            let mine = m.cannon.as_ref().map_or(false, |s| s.home == c.seat.home);
            if mine {
                // While Mario is inside, the action owns the seat; mirror it for drawing.
                c.seat = m.cannon.clone().unwrap();
                c.occupied = true;
            } else if c.occupied {
                c.occupied = false;
                if fired {
                    c.seat.cooldown = 6.0;
                }
            }
            if c.seat.cooldown > 0.0 {
                c.seat.cooldown -= 1.0;
            }
            if !mine && c.seat.cooldown == 0.0 {
                c.seat.rise = 0.0;
                c.seat.shake = 0.0;
                c.seat.pitch = 0.0;
                c.seat.yaw = c.seat.base_yaw;
                if m.action & ACT_FLAG_INTANGIBLE == 0 && dist < 187.0 && m.pos[1] < base_y + 150.0 && m.pos[1] + 160.0 > base_y {
                    m.drop_held_object();
                    m.cannon = Some(c.seat.clone());
                    m.set_action(ACT_IN_CANNON, 0.0);
                    c.occupied = true;
                }
            }
            let inside = m.cannon.as_ref().map_or(false, |s| s.home == c.seat.home);
            c.base_pos = DVec3::new(x + c.seat.shake, base_y + c.seat.rise, z + c.seat.shake);
            c.base_yaw = c.seat.yaw;
            c.barrel_pitch = c.seat.pitch;
            c.parts_visible = c.seat.cooldown == 0.0 && js_hypot(dist, m.pos[1] - base_y) < 500.0 && !(inside && m.action_state == 2.0);
        }
    }
}

/// `CannonCamera`: cutscene_enter_cannon_raise and update_in_cannon on the 30 Hz clock.
pub struct CannonCamera {
    pub fov_offset: f64,
    distance: f64,
    yaw: f64,
    pitch: f64,
}

impl CannonCamera {
    pub fn new(eye: DVec3, seat: &CannonSeat) -> Self {
        let [x, y, z] = seat.home;
        let (dx, dy, dz) = (eye.x - x, eye.y - (y - 340.0), eye.z - z);
        Self { fov_offset: 0.0, distance: js_hypot3(dx, dy, dz), yaw: atan2s(dz, dx) as f64, pitch: atan2s(js_hypot(dx, dz), dy) as f64 }
    }

    /// Returns the (eye, focus) override for this tick, or `None` to leave the camera alone.
    pub fn update(&mut self, m: &MarioRuntime) -> Option<(DVec3, DVec3)> {
        let seat = m.cannon.as_ref()?;
        let t = m.action_timer - 1.0;
        let shake_tick = t - 71.0;
        self.fov_offset = if shake_tick < 0.0 { 0.0 } else { coss(shake_tick * 0x8000 as f64) * (0.0f64).max(0x100 as f64 - 0x30 as f64 * shake_tick) / 0x100 as f64 };
        if m.action_state == 2.0 {
            let (pitch, yaw) = (m.face_angle[0], m.face_angle[1]);
            let mut eye = DVec3::from(m.pos);
            eye.y += 125.0;
            let offset = (0.0f64).max(800.0 - (t - 122.0) * 100.0);
            let look = DVec3::new(eye.x + 800.0 * coss(pitch) * sins(yaw), eye.y + offset + 800.0 * sins(pitch), eye.z + 800.0 * coss(pitch) * coss(yaw));
            return Some((eye, look));
        }
        if t <= 0.0 {
            return None;
        }
        let [x, y, z] = seat.home;
        let distance = self.distance + clamp(400.0 - self.distance, -5.0 * (t - 1.0), 5.0 * (t - 1.0));
        let yaw = self.yaw + (t - 1.0) * 0x40 as f64;
        let pitch = self.pitch + clamp(-self.pitch, -t * 0x80 as f64, t * 0x80 as f64);
        let base_y = y - 340.0;
        let mut eye = DVec3::new(
            x + distance * coss(pitch) * sins(yaw),
            base_y + t.min(69.0) * 5.0 + distance * sins(pitch) + 2.0 * t,
            z + distance * coss(pitch) * coss(yaw),
        );
        let floor = m.world.find_floor(eye.x, eye.y + 500.0, eye.z);
        if floor.surface.is_some() {
            eye.y = eye.y.max(floor.height + 100.0);
        }
        let focus = DVec3::new(
            x + seat.shake + 100.0 * sins(seat.pitch) * sins(seat.yaw),
            base_y + seat.rise + 100.0 * coss(seat.pitch),
            z + seat.shake + 100.0 * sins(seat.pitch) * coss(seat.yaw),
        );
        Some((eye, focus))
    }
}

// US dialog 004, 105, 047 and 106, split at their native page boundaries.
const BATTLEFIELD: [&[&str]; 2] = [
    &[
        "We're peace-loving Bob-ombs, so we don't use cannons.",
        "But if you'd like to blast off, we don't mind. Help yourself.",
        "We'll prepare all of the cannons in this course for you to use. Bon Voyage!",
    ],
    &[
        "Ready for blastoff! Come on, hop into the cannon!",
        "You can reach the Star on the floating island by using the four cannons.",
        "Use the Control Stick to aim, then press [A] to fire.",
        "If you're handy, you can grab on to trees or poles to land.",
    ],
];
const OTHER_COURSES: [&[&str]; 2] = [&["Hi! I'll prepare the cannon for you!"], &["Ready for blastoff! Come on, hop into the cannon!"]];

pub fn advice(param: i32) -> &'static [&'static str] {
    match param {
        1 => &[
            "Watch out! If you wander around here, you're liable to be plastered by a water bomb!",
            "Those enemy Bob-ombs love to fight, and they're always finding ways to attack.",
            "This meadow has become a battlefield ever since the Big Bob-omb got his paws on the Power Star.",
            "Can you recover the Star for us? Cross the bridge and go left up the path to find the Big Bob-omb.",
            "Please come back to see me after you've retrieved the Power Star!",
        ],
        2 => &[
            "Hey, you! It's dangerous ahead, so listen up! Take my advice.",
            "Cross the two bridges ahead, then watch for falling water bombs.",
            "The Big Bob-omb at the top of the mountain is very powerful--don't let him grab you!",
            "We're Bob-omb Buddies, and we're on your side. You can talk to us whenever you'd like to!",
        ],
        3 => &[
            "Thank you, Mario! The Big Bob-omb is nothing but a big dud now! But the battle for the castle has just begun.",
            "Other enemies are holding the other Power Stars. If you recover more Stars, you can open new doors that lead to new worlds!",
            "My Bob-omb Buddies are waiting for you. Be sure to talk to them--they'll set up cannons for you.",
        ],
        _ => &[],
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OperatorStage {
    Turn,
    Advice,
    Intro,
    Opening,
    Ready,
}

pub struct CannonOperator {
    pub buddy: ObjId,
    pub advice_buddy: bool,
    advice_param: i32,
    battlefield: bool,
    unlocked: bool,
    page: usize,
    pub stage: OperatorStage,
    timer: i32,
    lid: DVec3,
    focus: DVec3,
    original_eye: DVec3,
    original_focus: DVec3,
    /// Camera (eye, look-at) while the lid cutscene runs.
    pub eye: DVec3,
}

/// What a step asks of its host.
#[derive(Default, Debug)]
pub struct OperatorOutput {
    pub finished: bool,
    pub save_unlock: bool,
    pub play_door: Option<DVec3>,
}

impl CannonOperator {
    pub fn new(m: &MarioRuntime, buddy: ObjId, battlefield: bool, unlocked: bool) -> Self {
        let o = &m.objects[buddy];
        Self {
            buddy,
            advice_buddy: o.behavior == jumpman_sim::object::ObjectBehaviorKind::BobombBuddyAdvice,
            advice_param: o.param as i32,
            battlefield,
            unlocked,
            page: 0,
            stage: OperatorStage::Turn,
            timer: 0,
            lid: DVec3::ZERO,
            focus: DVec3::ZERO,
            original_eye: DVec3::ZERO,
            original_focus: DVec3::ZERO,
            eye: DVec3::ZERO,
        }
    }

    pub fn opening(&self) -> bool {
        self.stage == OperatorStage::Opening
    }

    fn pages(&self) -> &'static [&'static str] {
        let set = if self.battlefield { &BATTLEFIELD } else { &OTHER_COURSES };
        match self.stage {
            OperatorStage::Advice => advice(self.advice_param),
            OperatorStage::Intro => set[0],
            OperatorStage::Ready => set[1],
            _ => &[],
        }
    }

    pub fn text(&self) -> Option<&'static str> {
        self.pages().get(self.page).copied()
    }

    /// One tick of `CannonOperator.step`. `camera` is the current (eye, focus); the returned
    /// override (if any) is the camera for this tick.
    pub fn step(&mut self, m: &mut MarioRuntime, controls: &ControllerSnapshot, camera: (DVec3, DVec3), cannons: &mut Cannons, out: &mut OperatorOutput) -> Option<(DVec3, DVec3)> {
        let mut cam = None;
        if self.stage == OperatorStage::Turn {
            let id = self.buddy;
            let angle = m.objects[id].angle_to_mario;
            m.objects[id].turn_toward_angle(angle, 0x1000 as f64);
            m.objects[id].face_yaw = m.objects[id].move_yaw;
            if to_s16(m.objects[id].move_yaw - angle) == 0 {
                if m.action != ACT_READING_NPC_DIALOG {
                    m.used_obj = Some(id);
                    m.set_action(ACT_READING_NPC_DIALOG, 1.0);
                } else if m.action_state == 8.0 {
                    self.stage = if self.advice_buddy {
                        OperatorStage::Advice
                    } else if self.unlocked {
                        OperatorStage::Ready
                    } else {
                        OperatorStage::Intro
                    };
                }
            }
        } else if self.opening() {
            cannons.step(m);
            if self.timer == 61 {
                out.play_door = Some(self.lid);
            }
            cam = Some(self.update_camera(camera.0));
            self.timer += 1;
            if self.timer == 171 {
                self.stage = OperatorStage::Ready;
                cam = None;
            }
        } else if controls.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
            let len = self.pages().len();
            self.page += 1;
            if self.page == len {
                self.page = 0;
                if self.stage != OperatorStage::Intro {
                    m.action_state = 9.0;
                    out.finished = true;
                    return None;
                }
                out.save_unlock = true;
                self.unlocked = true;
                match cannons.unlock(m.objects[self.buddy].pos) {
                    Some(nearest) => {
                        self.stage = OperatorStage::Opening;
                        self.lid = DVec3::from(nearest);
                        self.original_eye = camera.0;
                        self.original_focus = camera.1;
                        self.focus = camera.1;
                        self.eye = camera.0;
                    }
                    None => self.stage = OperatorStage::Ready,
                }
            }
        }
        cam
    }

    fn update_camera(&mut self, current: DVec3) -> (DVec3, DVec3) {
        let mut eye = if self.timer == 0 { self.original_eye } else { self.eye };
        let _ = current;
        let t = self.timer as f64;
        if self.timer >= 141 {
            let factor = if eye.distance(self.original_eye) >= 8000.0 { 1.0 } else { 1.0 / (1.0f64).max(170.0 - t) };
            eye = eye.lerp(self.original_eye, factor);
            self.focus = self.focus.lerp(self.original_focus, factor);
            self.eye = eye;
            return (eye, self.focus);
        }
        let mut orbit: Polar = polar(self.lid, eye);
        if orbit.dist > 8000.0 {
            orbit.dist = 1200.0;
            orbit.pitch = 0x2000 as f64;
            self.focus = self.lid;
        } else {
            let current = polar(eye, self.focus);
            let target = polar(eye, self.lid);
            self.focus = place(
                eye,
                Polar {
                    dist: current.dist + (target.dist - current.dist) * 0.1,
                    pitch: current.pitch - (to_s16(current.pitch - target.pitch) as f64 / 15.0).trunc(),
                    yaw: current.yaw - (to_s16(current.yaw - target.yaw) as f64 / 15.0).trunc(),
                },
            );
        }
        eye = place(
            self.lid,
            Polar {
                dist: orbit.dist + (300.0 - orbit.dist) * 0.05,
                pitch: orbit.pitch - (to_s16(orbit.pitch - 0x2000 as f64) as f64 / 32.0).trunc(),
                yaw: orbit.yaw,
            },
        );
        self.eye = eye;
        let aim = polar(eye, self.focus);
        let target = place(eye, Polar { dist: aim.dist, pitch: clamp(aim.pitch, -(0x3000 as f64), 0x3000 as f64), yaw: aim.yaw + (0x400 as f64).min(17.0 * t) });
        (eye, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
    use jumpman_sim::mario_runtime::SpawnPoint;

    fn world() -> Arc<CollisionWorld> {
        let mut w = CollisionWorld::default();
        w.add_triangle([-3000.0, 0.0, -3000.0], [3000.0, 0.0, 3000.0], [3000.0, 0.0, -3000.0], &SurfaceOptions::default());
        w.add_triangle([-3000.0, 0.0, -3000.0], [-3000.0, 0.0, 3000.0], [3000.0, 0.0, 3000.0], &SurfaceOptions::default());
        Arc::new(w)
    }

    fn placement() -> CannonPlacement {
        CannonPlacement { pos: [0.0, 340.0, 0.0], yaw: 0.0, param: 64.0, acts: None }
    }

    #[test]
    fn open_cannon_swallows_mario_standing_in_it() {
        let mut m = MarioRuntime::new(world(), SpawnPoint { action: None, pos: [10.0, 0.0, 10.0], face_yaw: 0.0 }, -9000.0);
        let mut cannons = Cannons::new(&[placement()], None);
        cannons.reset(&mut m.world, true);
        cannons.step(&mut m);
        assert_eq!(m.action, ACT_IN_CANNON);
        assert!(m.cannon.is_some());
        assert_eq!(cannons.entries[0].seat.base_yaw, 0x4000 as f64);
    }

    #[test]
    fn closed_cannon_ignores_mario_and_unlock_finds_nearest() {
        let mut m = MarioRuntime::new(world(), SpawnPoint { action: None, pos: [10.0, 0.0, 10.0], face_yaw: 0.0 }, -9000.0);
        let mut far = placement();
        far.pos = [2000.0, 340.0, 0.0];
        let mut cannons = Cannons::new(&[far, placement()], None);
        cannons.reset(&mut m.world, false);
        cannons.step(&mut m);
        assert_ne!(m.action, ACT_IN_CANNON);
        assert_eq!(cannons.unlock([100.0, 0.0, 0.0]), Some([0.0, 340.0, 0.0]));
        assert!(cannons.opening());
        // The lid slides away and the cannon opens after 61 + 80 frames.
        for _ in 0..145 {
            cannons.step(&mut m);
        }
        assert!(cannons.open);
    }

    #[test]
    fn advice_pages_exist_for_each_buddy() {
        for p in 1..=3 {
            assert!(!advice(p).is_empty());
        }
    }
}
