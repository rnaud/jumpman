//! Port of `roomDoors.ts` (star-gated interior doors and their collider) plus the castle
//! entrance doors of `Game.ts` (`enteringDoor`: key doors that load another course/area).
//!
//! The door leaf's visual pose is exposed through [`Leaf::offset`]/[`Leaf::clip`] so a view can
//! follow it; the collision half lives here.
use super::data::{CastleDoor, DoorMotion, RoomDoorSpec, V3};
use super::save::{CastleKey, SaveFile};
use crate::game::course_data::CollisionData;
use crate::game::native_collision::add_native_collision;
use jumpman_sim::collision::{CollisionWorld, Surface};
use jumpman_sim::math::{atan2s, coss, deg_to_angle, sins, to_s16};
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;
use std::sync::Arc;

/// `get_door_save_file_flag` in interaction.c.
pub fn unlock_flag(door: &RoomDoorSpec) -> u32 {
    match door.stars {
        1 => {
            if door.pos[1] > 500.0 { 1 << 10 } else { 1 << 11 }
        }
        3 => {
            if door.pos[0] < 0.0 { 1 << 12 } else { 1 << 13 }
        }
        8 => 1 << 14,
        30 => 1 << 15,
        50 => 1 << 20,
        _ => 0,
    }
}

/// A movable collision shape (the TS `DoorCollider`): closed door faces block every movement
/// action, including jumps and dives.
#[derive(Clone)]
pub struct DoorCollider {
    pub data: CollisionData,
    pub yaw: f64,
    faces: Vec<Arc<Surface>>,
}

impl DoorCollider {
    pub fn new(data: CollisionData, yaw: f64) -> Self {
        Self { data, yaw, faces: vec![] }
    }

    pub fn owns(&self, surface: &Option<Arc<Surface>>) -> bool {
        surface.as_ref().map_or(false, |s| self.faces.iter().any(|f| Arc::ptr_eq(f, s)))
    }

    pub fn is_placed(&self) -> bool {
        !self.faces.is_empty()
    }

    /// `move(pos)`: remove the current faces and (when `pos` is given) re-add them there.
    pub fn move_to(&mut self, world: &mut CollisionWorld, pos: Option<V3>) {
        if !self.faces.is_empty() {
            let ptrs: Vec<*const Surface> = self.faces.iter().map(Arc::as_ptr).collect();
            world.remove_surfaces(&|s: &Surface| ptrs.contains(&(s as *const Surface)));
            self.faces.clear();
        }
        let Some(pos) = pos else { return };
        let start = world.surfaces().len();
        add_native_collision(world, &self.data, pos, self.yaw);
        self.faces = world.surfaces()[start..].to_vec();
    }
}

pub struct Leaf {
    pub door: RoomDoorSpec,
    pub collider: DoorCollider,
    /// -1 while closed, else frames since opening started.
    pub timer: i32,
    /// Current position of a sliding leaf (hinged doors animate through `clip`).
    pub pos: V3,
    /// Animation clip request for a view (0 = closed, 1 = pull, 2 = push), set when it changes.
    pub clip: Option<usize>,
}

pub struct RoomDoors {
    pub leaves: Vec<Leaf>,
    motion: DoorMotion,
    unlocking: Option<usize>,
    passage: Option<Passage>,
}

#[derive(Clone, Copy)]
struct Passage {
    leaf: usize,
    pull: bool,
    dx: f64,
    dz: f64,
}

impl RoomDoors {
    pub fn new(specs: &[RoomDoorSpec], motion: DoorMotion, door: &CollisionData, star_door: &CollisionData, world: &mut CollisionWorld) -> Self {
        let leaves = specs
            .iter()
            .map(|spec| {
                let mut collider = DoorCollider::new(if spec.sliding { star_door.clone() } else { door.clone() }, spec.yaw);
                collider.move_to(world, Some(spec.pos));
                Leaf { door: spec.clone(), collider, timer: -1, pos: spec.pos, clip: None }
            })
            .collect();
        Self { leaves, motion, unlocking: None, passage: None }
    }

    pub fn reset(&mut self, world: &mut CollisionWorld) {
        self.passage = None;
        self.unlocking = None;
        for leaf in &mut self.leaves {
            leaf.timer = -1;
            leaf.pos = leaf.door.pos;
            leaf.collider.move_to(world, Some(leaf.door.pos));
            leaf.clip = Some(0);
        }
    }

    pub fn active(&self) -> bool {
        self.unlocking.is_some() || self.passage.is_some()
    }

    /// One tick before Mario's step. Returns true when the doors consumed the step (Mario is in a
    /// door cutscene driven from here). `notify` receives HUD messages.
    pub fn step(&mut self, m: &mut MarioRuntime, save: &mut SaveFile, notify: &mut dyn FnMut(String)) -> bool {
        // Door leaves animate independently of Mario.
        for i in 0..self.leaves.len() {
            if self.leaves[i].timer < 0 {
                continue;
            }
            self.leaves[i].timer += 1;
            let frame = self.leaves[i].timer;
            let door = self.leaves[i].door.clone();
            if door.sliding {
                let offset = -8.0 * f64::from(17.min(frame).min((66 - frame).max(0)));
                let angle = deg_to_angle(door.yaw) as f64;
                let pos = [door.pos[0] + offset * coss(angle), door.pos[1], door.pos[2] - offset * sins(angle)];
                self.leaves[i].pos = pos;
                if frame <= 17 || frame >= 49 {
                    let world = Arc::make_mut(&mut m.world);
                    self.leaves[i].collider.move_to(world, Some(pos));
                }
            }
            if frame >= if door.sliding { 66 } else { 80 } {
                self.leaves[i].timer = -1;
                self.leaves[i].pos = door.pos;
                let world = Arc::make_mut(&mut m.world);
                self.leaves[i].collider.move_to(world, Some(door.pos));
                self.leaves[i].clip = Some(0);
            }
        }

        if let Some(i) = self.unlocking {
            m.action_timer += 1.0;
            let frame = m.action_timer as i32;
            if frame == 64 {
                m.action_state = 2.0;
            }
            if frame == 135 {
                m.action_state = 3.0;
            }
            if frame >= 161 {
                save.set_flag(unlock_flag(&self.leaves[i].door));
                notify("THE DOOR HAS OPENED!".into());
                m.set_action(ACT_WALKING, 0.0);
                self.unlocking = None;
            }
            return true;
        }

        if let Some(Passage { leaf, pull, dx, dz }) = self.passage {
            let door = self.leaves[leaf].door.clone();
            let yaw = deg_to_angle(door.yaw) as f64;
            m.action_timer += 1.0;
            let frame = m.action_timer as usize;
            let motion = if pull { &self.motion.pull } else { &self.motion.push };
            if door.sliding {
                if (15..35).contains(&frame) {
                    m.pos[0] += dx;
                    m.pos[2] += dz;
                }
                if frame >= 35 {
                    m.face_angle[1] = to_s16(yaw + if pull { 0.0 } else { 32768.0 }) as f64;
                    m.pos[0] += 12.0 * sins(m.face_angle[1]);
                    m.pos[2] += 12.0 * coss(m.face_angle[1]);
                }
            } else if !motion.is_empty() {
                let [x, z] = motion[(frame - 1).min(motion.len() - 1)];
                m.pos[0] = door.pos[0] + (x * coss(yaw) + z * sins(yaw)).trunc();
                m.pos[2] = door.pos[2] + (z * coss(yaw) - x * sins(yaw)).trunc();
            }
            let floor = m.world.find_floor(m.pos[0], m.pos[1] + 78.0, m.pos[2]);
            if floor.surface.is_some() {
                m.floor = floor.surface;
                m.pos[1] = floor.height;
                m.floor_height = floor.height;
            }
            if frame >= if door.sliding { 48 } else { motion.len() } {
                m.face_angle[1] = to_s16(yaw + if pull { 0.0 } else { 32768.0 }) as f64;
                m.set_action(ACT_IDLE, 0.0);
                self.passage = None;
            }
            return true;
        }

        if m.action != ACT_WALKING && m.action != ACT_DECELERATING {
            return false;
        }
        for i in 0..self.leaves.len() {
            let door = self.leaves[i].door.clone();
            let [x, y, z] = door.pos;
            if self.leaves[i].timer >= 0 || (x - m.pos[0]).hypot(z - m.pos[2]) >= 117.0 || m.pos[1] > y + 100.0 || m.pos[1] + 160.0 < y {
                continue;
            }
            // The 70-star door admits Mario to the endless stairs after its warning.
            if save.stars() < door.stars {
                notify(format!("YOU NEED {} MORE STARS", door.stars - save.stars()));
                if door.stars != 70 {
                    return false;
                }
            }
            let yaw = deg_to_angle(door.yaw) as f64;
            let pull = (to_s16(yaw - atan2s(z - m.pos[2], x - m.pos[0]) as f64)).abs() <= 0x4000;
            let flag = unlock_flag(&door);
            let arg = if pull { 1.0 } else { 2.0 };
            let facing = to_s16(yaw + if pull { 0.0 } else { 32768.0 }) as f64;
            if flag != 0 && !save.has_flag(flag) {
                self.unlocking = Some(i);
                m.set_action(ACT_UNLOCKING_STAR_DOOR, arg);
                m.action_state = 1.0;
                m.face_angle[1] = facing;
                m.vel = [0.0; 3];
                m.forward_vel = 0.0;
                return true;
            }
            let target = yaw + 0x1555 as f64 + if pull { 0.0 } else { 0x5556 as f64 };
            let dx = x + 150.0 * sins(target) - m.pos[0];
            let dz = z + 150.0 * coss(target) - m.pos[2];
            self.passage = Some(Passage { leaf: i, pull, dx: dx / 20.0, dz: dz / 20.0 });
            let act = if door.sliding { ACT_ENTERING_STAR_DOOR } else if pull { ACT_PULLING_DOOR } else { ACT_PUSHING_DOOR };
            m.set_action(act, arg);
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.face_angle[1] = if door.sliding { atan2s(dz, dx) as f64 } else { yaw };
            self.leaves[i].timer = 0;
            if door.sliding {
                let mate = (0..self.leaves.len()).find(|&j| {
                    j != i && {
                        let o = &self.leaves[j].door;
                        o.sliding && o.stars == door.stars && (o.pos[0] - x).hypot(o.pos[2] - z) < 200.0
                    }
                });
                if let Some(j) = mate {
                    self.leaves[j].timer = 0;
                }
            } else {
                let world = Arc::make_mut(&mut m.world);
                self.leaves[i].collider.move_to(world, None);
                self.leaves[i].clip = Some(if pull { 1 } else { 2 });
            }
            return true;
        }
        false
    }
}

/// Outcome of probing the castle entrance doors (`Game.ts` `enteringDoor`).
#[derive(Clone, Debug, PartialEq)]
pub enum EntranceHit {
    /// Locked and Mario has no key.
    NeedsKey,
    Enter { index: usize, locked: bool, pull: bool },
}

/// Find the entrance door Mario is walking into (castle grounds <-> inside, key doors).
pub fn probe_entrance(doors: &[CastleDoor], m: &MarioRuntime, save: &SaveFile) -> Option<EntranceHit> {
    if m.action != ACT_WALKING && m.action != ACT_DECELERATING {
        return None;
    }
    for (index, door) in doors.iter().enumerate() {
        if door.destination.is_empty() {
            continue;
        }
        let (dx, dz) = (door.pos[0] - m.pos[0], door.pos[2] - m.pos[2]);
        if dx.hypot(dz) >= 117.0 || m.pos[1] >= door.pos[1] + 100.0 || m.pos[1] + 160.0 <= door.pos[1] {
            continue;
        }
        let key = door.required_key.and_then(CastleKey::from_number);
        let locked = key.map_or(false, |k| !save.door_unlocked(k));
        if locked && !key.map_or(false, |k| save.has_key(k)) {
            return Some(EntranceHit::NeedsKey);
        }
        let yaw = (door.yaw * 65536.0 / 360.0).round();
        let pull = (to_s16(yaw - atan2s(dz, dx) as f64)).abs() <= 0x4000;
        return Some(EntranceHit::Enter { index, locked, pull });
    }
    None
}

/// Put Mario into the door cutscene for an entrance (`Game.ts`).
pub fn begin_entrance(m: &mut MarioRuntime, door: &CastleDoor, locked: bool, pull: bool) {
    let yaw = (door.yaw * 65536.0 / 360.0).round();
    let act = if locked { ACT_UNLOCKING_KEY_DOOR } else if pull { ACT_PULLING_DOOR } else { ACT_PUSHING_DOOR };
    m.set_action(act, if pull { 5.0 } else { 6.0 });
    m.face_angle[1] = yaw;
    m.pos = door.pos;
    m.forward_vel = 0.0;
    m.vel = [0.0; 3];
    if locked {
        let angle = door.yaw.to_radians();
        m.pos[0] += angle.cos() * 75.0;
        m.pos[2] += angle.sin() * 75.0;
        if !pull {
            m.face_angle[1] = to_s16(yaw + 32768.0) as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use crate::progress::data::ProgressData;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn castle() -> (GameSim, ProgressData) {
        let sim = GameSim::load(&assets(), CourseRequest::new("castle_inside")).unwrap();
        (sim, ProgressData::load(&assets(), "castle_inside", 1))
    }

    #[test]
    fn unlock_flags_match_interaction_c() {
        let d = |stars, x: f64, y: f64| RoomDoorSpec { pos: [x, y, 0.0], stars, ..Default::default() };
        assert_eq!(unlock_flag(&d(1, 0.0, 600.0)), 1 << 10);
        assert_eq!(unlock_flag(&d(1, 0.0, 0.0)), 1 << 11);
        assert_eq!(unlock_flag(&d(3, -5.0, 0.0)), 1 << 12);
        assert_eq!(unlock_flag(&d(3, 5.0, 0.0)), 1 << 13);
        assert_eq!(unlock_flag(&d(8, 0.0, 0.0)), 1 << 14);
        assert_eq!(unlock_flag(&d(0, 0.0, 0.0)), 0);
    }

    #[test]
    fn closed_door_blocks_and_moving_it_opens_the_gap() {
        let (mut sim, data) = castle();
        let motion = data.door_motion.clone().unwrap();
        let door = data.extra_collision["door"].clone();
        let star = data.extra_collision["star_door"].clone();
        let world = Arc::make_mut(&mut sim.mario.world);
        let mut doors = RoomDoors::new(&data.room_doors, motion, &door, &star, world);
        assert!(doors.leaves.iter().all(|l| l.collider.is_placed()));
        let before = world.surfaces().len();
        let first = doors.leaves.iter().position(|l| !l.door.sliding).unwrap();
        let spec = doors.leaves[first].door.clone();
        // A wall surface owned by the door exists at the closed door's position...
        let hit = world.find_floor(spec.pos[0], spec.pos[1] + 500.0, spec.pos[2]);
        assert!(hit.surface.is_some());
        doors.leaves[first].collider.move_to(world, None);
        assert!(world.surfaces().len() < before, "opening removed the door faces");
        doors.reset(world);
        assert_eq!(world.surfaces().len(), before, "reset restores them");
    }

    #[test]
    fn star_door_warns_and_wooden_door_opens() {
        let (mut sim, data) = castle();
        let motion = data.door_motion.clone().unwrap();
        let world = Arc::make_mut(&mut sim.mario.world);
        let mut doors = RoomDoors::new(&data.room_doors, motion, &data.extra_collision["door"], &data.extra_collision["star_door"], world);
        let mut save = SaveFile::in_memory(1);
        let mut notes = vec![];
        // Walk into a 0-star wooden door: passage starts and finishes, moving Mario through.
        let leaf = doors.leaves.iter().position(|l| l.door.stars == 0 && !l.door.sliding).unwrap();
        let door = doors.leaves[leaf].door.clone();
        let m = &mut sim.mario;
        let angle = door.yaw.to_radians();
        m.pos = [door.pos[0] - 80.0 * angle.sin(), door.pos[1], door.pos[2] - 80.0 * angle.cos()];
        m.set_action(ACT_WALKING, 0.0);
        let start = m.pos;
        assert!(doors.step(m, &mut save, &mut |n| notes.push(n)));
        assert!(doors.active() && m.action == ACT_PULLING_DOOR || m.action == ACT_PUSHING_DOOR);
        for _ in 0..200 {
            if !doors.step(m, &mut save, &mut |n| notes.push(n)) {
                break;
            }
        }
        assert_eq!(m.action, ACT_IDLE, "door cutscene ended");
        assert!((m.pos[0] - start[0]).hypot(m.pos[2] - start[2]) > 100.0, "Mario was carried through the door");
        // A star-gated door refuses with a message.
        let gated = doors.leaves.iter().position(|l| l.door.stars >= 8).unwrap();
        let g = doors.leaves[gated].door.clone();
        m.pos = [g.pos[0], g.pos[1], g.pos[2]];
        m.set_action(ACT_WALKING, 0.0);
        assert!(!doors.step(m, &mut save, &mut |n| notes.push(n)));
        assert!(notes.iter().any(|n| n.starts_with("YOU NEED")), "{notes:?}");
    }

    #[test]
    fn entrance_doors_need_keys() {
        let data = ProgressData::load(&assets(), "castle_inside", 1);
        let keyed = data.doors.iter().find(|d| d.required_key == Some(1)).expect("a key door");
        let (mut sim, _) = castle();
        let mut save = SaveFile::in_memory(1);
        sim.mario.pos = [keyed.pos[0], keyed.pos[1], keyed.pos[2] + 10.0];
        sim.mario.set_action(ACT_WALKING, 0.0);
        let doors = [keyed.clone()];
        assert_eq!(probe_entrance(&doors, &sim.mario, &save), Some(EntranceHit::NeedsKey));
        save.collect_key(CastleKey::Basement);
        assert!(matches!(probe_entrance(&doors, &sim.mario, &save), Some(EntranceHit::Enter { locked: true, .. })));
        save.unlock_door(CastleKey::Basement);
        assert!(matches!(probe_entrance(&doors, &sim.mario, &save), Some(EntranceHit::Enter { locked: false, .. })));
    }
}
