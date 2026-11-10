//! Moving dungeon platforms (the `movers` list of an OoT course file): `jumpman_sim::oot_movers` runs the actors' state
//! machines, each platform is a collision body in the world (`dungeon::Collider`, re-placed whenever it moves) plus a model, and
//! a platform Link stands on carries him (and turns him with it when it rotates, `DYNA_TRANSFORM_ROT_Y`).
use super::dungeon::Collider;
use crate::game::course_data::CollisionData;
use crate::game::runner::GameSim;
use crate::progress::CourseScoped;
use crate::render::{scene, to_bevy, AssetRoot};
use bevy::prelude::*;
use jumpman_sim::link::items::Age;
use jumpman_sim::link::moves::GrabFace;
use jumpman_sim::math::Vec3 as V3;
use jumpman_sim::oot_movers::{Event, Kind, Mover, Movers, World, U};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MoverEntry {
    pub kind: String,
    pub model: Option<String>,
    pub collision: Option<String>,
    pub pos: V3,
    pub yaw: f64,
    pub param: i32,
    pub room: i32,
    /// `Bg_Mori_Hineri`: the paired hall (shown when its switch flips), and the door that turns with the hall.
    pub alt_model: Option<String>,
    pub alt_collision: Option<String>,
    pub door_model: Option<String>,
    pub door_a: Option<V3>,
    pub door_b: Option<V3>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Raw {
    #[serde(default)]
    movers: Vec<MoverEntry>,
    #[serde(default)]
    extra_collision: HashMap<String, CollisionData>,
}

/// The `movers` of a course file with their collision shapes.
pub fn load_entries(root: &std::path::Path, id: &str, area: i32) -> (Vec<MoverEntry>, HashMap<String, CollisionData>) {
    let path = crate::game::course_data::CourseData::path(root, id, area);
    let Ok(bytes) = std::fs::read(path) else { return Default::default() };
    let raw: Raw = serde_json::from_slice(&bytes).unwrap_or_default();
    (raw.movers, raw.extra_collision)
}

#[derive(Resource, Default)]
pub struct MoverWorld {
    pub movers: Movers,
    /// Entry index of each live mover (the sim keeps only one of the single-instance kinds).
    entries: Vec<MoverEntry>,
    colliders: Vec<Option<Collider>>,
    /// The paired hall's collision (swapped in when the variant changes).
    alt_colliders: Vec<Option<Collider>>,
    placed: Vec<Option<(V3, f64)>>,
    /// Local bounds of each body's collision (the slab's face Link can hold on to).
    bounds: Vec<Option<(V3, V3)>>,
    key: String,
    clock: u32,
    last_sfx: HashMap<&'static str, u32>,
}

/// A platform's model; the second field is the variant (parity) it shows.
#[derive(Component)]
pub struct MoverView(pub usize, pub u8);

/// The door of the twisted hallway.
#[derive(Component)]
pub struct MoverDoor(pub usize);

/// Ids of the faces movers offer to `LinkExt::grab_face` start here (the blocks use their index).
pub const GRAVE_BASE: u32 = 1000;

fn yaw_quat(deg: f64) -> Quat {
    Quat::from_rotation_y(deg.to_radians() as f32)
}

impl MoverWorld {
    /// Build the movers of a course. `flag` reads the scene's switch flags.
    pub fn build(entries: Vec<MoverEntry>, shapes: &HashMap<String, CollisionData>, flag: &dyn Fn(i32) -> bool, adult: bool, water_y: Option<f64>) -> Self {
        let mut list = vec![];
        let mut kept = vec![];
        for e in entries {
            let Some(kind) = Kind::parse(&e.kind) else { continue };
            list.push(Mover::new(kind, e.pos, e.yaw, e.param, e.room, flag, adult, water_y));
            kept.push(e);
        }
        // `Movers::new` drops the repeated single-instance kinds: keep the entries in step with it
        let movers = Movers::new(list);
        let mut seen: Vec<(Kind, i32, i32)> = movers.list.iter().map(|m| (m.kind, m.room, m.param)).collect();
        let entries: Vec<MoverEntry> = kept
            .into_iter()
            .filter(|e| {
                let k = Kind::parse(&e.kind);
                match seen.iter().position(|s| Some(s.0) == k && s.1 == e.room && s.2 == e.param) {
                    Some(i) => {
                        seen.remove(i);
                        true
                    }
                    None => false,
                }
            })
            .collect();
        let colliders = entries.iter().map(|e| e.collision.as_ref().and_then(|k| shapes.get(k)).map(|d| Collider::new(d.clone(), e.yaw))).collect();
        let alt_colliders = entries.iter().map(|e| e.alt_collision.as_ref().and_then(|k| shapes.get(k)).map(|d| Collider::new(d.clone(), e.yaw))).collect();
        let placed = vec![None; entries.len()];
        let bounds = entries
            .iter()
            .map(|e| {
                let v = &shapes.get(e.collision.as_ref()?)?.vertices;
                let lo = (0..3).map(|k| v.iter().map(|p| p[k]).fold(f64::MAX, f64::min)).collect::<Vec<_>>();
                let hi = (0..3).map(|k| v.iter().map(|p| p[k]).fold(f64::MIN, f64::max)).collect::<Vec<_>>();
                (!v.is_empty()).then(|| ([lo[0], lo[1], lo[2]], [hi[0], hi[1], hi[2]]))
            })
            .collect();
        Self { movers, entries, colliders, alt_colliders, placed, bounds, key: String::new(), clock: 0, last_sfx: HashMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.movers.list.is_empty()
    }

    /// The slab face Link could take hold of (`Bg_Haka`: Link stands on its back side, at the foot of the grave, and pulls).
    fn grave_face(&self, link: V3, facing: f64) -> Option<GrabFace> {
        let mut best: Option<(f64, GrabFace)> = None;
        for (i, m) in self.movers.list.iter().enumerate() {
            let (Kind::Grave, Some(p), Some((lo, hi))) = (m.kind, self.movers.pos(i), self.bounds.get(i).copied().flatten()) else { continue };
            let a = self.movers.yaw(i).to_radians();
            let (s, c) = (a.sin(), a.cos());
            let (dx, dz) = (link[0] - p[0], link[2] - p[2]);
            // local coordinates: the inverse of the collision's `x' = c x + s z, z' = -s x + c z`
            let (lx, lz) = (c * dx - s * dz, s * dx + c * dz);
            let ly = link[1] - p[1];
            if lx < lo[0] - 10.0 * U || lx > hi[0] + 10.0 * U || lz < lo[2] - 40.0 * U || lz > lo[2] + 10.0 * U || ly < -20.0 * U || ly > 60.0 * U {
                continue;
            }
            let yaw = self.movers.yaw(i) * 65536.0 / 360.0;
            if (jumpman_sim::math::to_s16(facing - yaw) as f64).abs() >= 0x3000 as f64 {
                continue;
            }
            let gap = (lz - lo[2]).abs();
            if best.is_none_or(|(g, _)| gap < g) {
                best = Some((gap, GrabFace { id: GRAVE_BASE + i as u32, yaw }));
            }
        }
        best.map(|b| b.1)
    }

    /// Put the colliders where the movers are; returns the per-mover displacement `(delta position, delta yaw)` since the last
    /// placement (zero for a body that was just placed).
    fn sync(&mut self, world: &mut jumpman_sim::collision::CollisionWorld) -> Vec<(V3, f64)> {
        let mut moved = vec![([0.0; 3], 0.0); self.entries.len()];
        for i in 0..self.entries.len() {
            let now = self.movers.pos(i).map(|p| (p, self.movers.yaw(i)));
            let before = self.placed[i];
            if before == now {
                continue;
            }
            if let (Some((p0, y0)), Some((p1, y1))) = (before, now) {
                moved[i] = ([p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]], y1 - y0);
            }
            if let Some(c) = self.colliders[i].as_mut() {
                if let Some((_, y)) = now {
                    c.set_yaw(y);
                }
                c.move_to(world, now.map(|n| n.0));
            }
            self.placed[i] = now;
        }
        moved
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_movers(
    mut commands: Commands,
    mut movers: ResMut<MoverWorld>,
    mut sim: ResMut<GameSim>,
    root: Res<AssetRoot>,
    assets: Res<AssetServer>,
    rooms: Res<super::rooms::SceneRooms>,
    blocks: Option<Res<super::blocks::BlockWorld>>,
    time: Option<Res<super::kankyo::OotTime>>,
    props: Option<ResMut<crate::link::prop_world::PropWorld>>,
    mut views: Query<(Entity, &mut MoverView, &mut Transform, &mut Visibility), Without<MoverDoor>>,
    mut doors: Query<(Entity, &MoverDoor, &mut Transform, &mut Visibility), Without<MoverView>>,
) {
    let req = sim.course.request.clone();
    let key = format!("{}:{}", req.id, req.area);
    let mut props = props;
    let switch = |p: &Option<ResMut<crate::link::prop_world::PropWorld>>, f: i32| p.as_ref().is_some_and(|p| p.props.flag(f));
    if movers.key != key {
        for (e, _, _, _) in &views {
            commands.entity(e).despawn();
        }
        for (e, _, _, _) in &doors {
            commands.entity(e).despawn();
        }
        let (entries, shapes) = load_entries(&root.0, &req.id, req.area);
        let water = sim.mario.world.water_boxes.get(2).map(|w| w.5 / U);
        let adult = sim.mario.link.age == Age::Adult;
        *movers = MoverWorld::build(entries, &shapes, &|f| switch(&props, f), adult, water);
        movers.key = key;
        for (i, e) in movers.entries.iter().enumerate() {
            let Some(model) = e.model.as_ref().filter(|m| root.exists(m)) else { continue };
            let base = super::glb::bevy_scale(&root.0.join(model));
            commands.spawn((
                MoverView(i, movers.movers.variant(i) & 1),
                CourseScoped,
                scene(&assets, model),
                Transform::from_translation(to_bevy(e.pos)).with_rotation(yaw_quat(e.yaw)).with_scale(Vec3::splat(base)),
                Visibility::default(),
            ));
            if let Some(door) = e.door_model.as_ref().filter(|d| root.exists(d)) {
                let base = super::glb::bevy_scale(&root.0.join(door));
                commands.spawn((MoverDoor(i), CourseScoped, scene(&assets, door), Transform::from_scale(Vec3::splat(base)), Visibility::Hidden));
            }
        }
        // a hall that starts in its second state shows that model and collision
        for i in 0..movers.entries.len() {
            if movers.movers.variant(i) & 1 == 1 && movers.entries[i].alt_model.is_some() {
                let m = &mut *movers;
                std::mem::swap(&mut m.colliders[i], &mut m.alt_colliders[i]);
            }
        }
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        let m = &mut *movers;
        m.sync(world);
        return;
    }
    if movers.is_empty() {
        return;
    }
    // A slab Link stands against can be taken hold of as well (the blocks fill `grab_face` first).
    if sim.mario.link.enabled && sim.mario.link.grab_face.is_none() {
        sim.mario.link.grab_face = movers.grave_face(sim.mario.pos, sim.mario.face_angle[1]);
    }
    // His hold on one: leaning away pulls it, but only with room behind him.
    let grip = sim.mario.link.grip.filter(|g| g.id >= GRAVE_BASE).map(|g| {
        let toward = [(g.yaw * std::f64::consts::PI / 32768.0).sin(), (g.yaw * std::f64::consts::PI / 32768.0).cos()];
        let force = if g.force < 0 && !super::blocks::back_clear(&sim.mario.world, sim.mario.pos, toward) { 0 } else { g.force };
        ((g.id - GRAVE_BASE) as usize, force)
    });
    let night = time.as_ref().is_some_and(|t| t.clock.is_night());
    let mario = &sim.mario;
    let on_top: Vec<bool> = movers
        .colliders
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let pos = movers.movers.pos(i);
            c.as_ref().zip(pos).is_some_and(|(c, p)| c.owns(&mario.floor) && (mario.pos[1] - mario.floor_height).abs() < 35.0 && (mario.floor_height - p[1]).abs() < 600.0 * U)
        })
        .collect();
    // a block on a switch presses it as well
    let pressed: Vec<bool> = on_top
        .iter()
        .enumerate()
        .map(|(i, top)| {
            *top || movers.movers.pos(i).zip(blocks.as_ref()).is_some_and(|(p, b)| {
                movers.movers.list[i].kind == Kind::JabuSwitch
                    && b.blocks.list.iter().any(|k| (k.pos[0] - p[0]).hypot(k.pos[2] - p[2]) < 40.0 * U && k.pos[1] - p[1] > -20.0 * U && k.pos[1] - p[1] < 40.0 * U)
            })
        })
        .collect();
    let events = {
        let flag = |f: i32| switch(&props, f);
        let water_y = sim.mario.world.water_boxes.get(2).map(|w| w.5 / U);
        let w = World { player: sim.mario.pos, room: rooms.current, on_top: &on_top, pressed: &pressed, flag: &flag, adult: sim.mario.link.age == Age::Adult, water_y, grip, night, scene: &req.id };
        movers.movers.tick(&w)
    };
    movers.clock = movers.clock.wrapping_add(1);
    if movers.clock % 30 == 0 && std::env::var("JUMPMAN_MOVERS_LOG").is_ok() {
        eprintln!("MOVERS room={:?} link={:?} top={:?} y={:?}", rooms.current, sim.mario.pos, on_top, movers.movers.list.iter().map(|m| m.y).collect::<Vec<_>>());
    }
    for e in &events {
        match *e {
            Event::Set(f) => {
                if let Some(p) = props.as_mut() {
                    p.props.set_switch(f, true);
                }
            }
            Event::Unset(f) => {
                if let Some(p) = props.as_mut() {
                    p.props.set_switch(f, false);
                }
            }
            Event::Message(id) => {
                // (no message box for the Graveyard Boy's warning yet)
                debug!("OoT message {id:#x}");
            }
            Event::Sfx(name) => {
                // the actors replay their looping sounds every frame: one trigger per half second is enough
                let clock = movers.clock;
                let last = &mut movers.last_sfx;
                if last.get(name).is_none_or(|t| clock.wrapping_sub(*t) >= 15) {
                    last.insert(name, clock);
                    crate::audio::oot_sfx::play(name);
                }
            }
        }
    }
    // Move the bodies; whatever Link stands on takes him along.
    let jabu_water = movers.movers.jabu_water();
    let moved = {
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        if let Some(y) = jabu_water {
            if let Some(w) = world.water_boxes.get_mut(7) {
                w.5 = y;
            }
        }
        let m = &mut *movers;
        m.sync(world)
    };
    for (i, (d, dyaw)) in moved.iter().enumerate() {
        // Link walks along with the slab he pulls
        if grip.is_some_and(|g| g.0 == i && g.1 != 0) && !on_top[i] {
            sim.mario.pos[0] += d[0];
            sim.mario.pos[2] += d[2];
        }
        if !on_top[i] {
            continue;
        }
        let m = &mut sim.mario;
        if *dyaw != 0.0 {
            let pivot = movers.movers.pos(i).unwrap_or(m.pos);
            let a = dyaw.to_radians();
            let (s, c) = (a.sin(), a.cos());
            let (dx, dz) = (m.pos[0] - pivot[0], m.pos[2] - pivot[2]);
            m.pos[0] = pivot[0] + c * dx + s * dz;
            m.pos[2] = pivot[2] - s * dx + c * dz;
            m.face_angle[1] += dyaw * 65536.0 / 360.0;
        }
        m.pos[0] += d[0];
        m.pos[1] += d[1];
        m.pos[2] += d[2];
    }
    // The twisted hallway changes hall when its switch flips and Link is away.
    for i in 0..movers.entries.len() {
        let want = movers.movers.variant(i) & 1;
        let shown = views.iter().find(|(_, v, _, _)| v.0 == i).map(|(_, v, _, _)| v.1);
        if movers.entries[i].alt_model.is_some() && shown.is_some_and(|s| s != want) {
            let world = std::sync::Arc::make_mut(&mut sim.mario.world);
            let m = &mut *movers;
            if let Some(c) = m.colliders[i].as_mut() {
                c.move_to(world, None);
            }
            std::mem::swap(&mut m.colliders[i], &mut m.alt_colliders[i]);
            m.placed[i] = None;
            m.sync(world);
            let model = if want == 1 { m.entries[i].alt_model.clone() } else { m.entries[i].model.clone() };
            for (e, mut v, _, _) in views.iter_mut() {
                if v.0 == i {
                    v.1 = want;
                    if let Some(model) = model.as_ref().filter(|m| root.exists(m)) {
                        commands.entity(e).insert(scene(&assets, model));
                    }
                }
            }
        }
    }
    for (_, v, mut tf, mut vis) in views.iter_mut() {
        match movers.movers.pos(v.0) {
            Some(p) => {
                tf.translation = to_bevy(p);
                tf.rotation = yaw_quat(movers.movers.yaw(v.0)) * Quat::from_rotation_z(movers.movers.roll(v.0).to_radians() as f32);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    // The door at the end of the hall turns about its own place (`Matrix_RotateZYX(0, -0x8000, rot.z)` after `Matrix_Translate`).
    for (_, d, mut tf, mut vis) in doors.iter_mut() {
        let (variant, roll) = (movers.movers.variant(d.0), movers.movers.roll(d.0));
        let e = &movers.entries[d.0];
        let at = if variant == 0 { e.door_a } else { e.door_b };
        match at {
            Some(p) if (variant == 0 || variant == 3) && roll != 0.0 => {
                let q = Quat::from_rotation_z(roll.to_radians() as f32);
                tf.translation = to_bevy(p) + q * Vec3::new(0.0, -50.0 * U as f32 * crate::render::SCALE, 0.0);
                tf.rotation = q * Quat::from_rotation_y(std::f32::consts::PI);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, room: i32, param: i32, collision: Option<&str>) -> MoverEntry {
        MoverEntry { kind: kind.into(), model: None, collision: collision.map(Into::into), pos: [0.0, 100.0, 0.0], yaw: 0.0, param, room, ..Default::default() }
    }

    #[test]
    fn repeated_single_instance_movers_keep_their_first_entry_in_step_with_the_sim() {
        let shape = CollisionData { rooms: vec![], vertices: vec![[0.0; 3], [100.0, 0.0, 0.0], [0.0, 0.0, 100.0]], triangles: vec![[0, 2, 1, 0, 0]], hookable: None };
        let shapes = HashMap::from([("lift".to_string(), shape)]);
        let w = MoverWorld::build(vec![entry("forest_elevator", 2, 27, Some("lift")), entry("forest_gate", 17, 0x1001, Some("lift")), entry("forest_elevator", 17, 27, Some("lift"))], &shapes, &|_| false, true, None);
        assert_eq!(w.movers.list.len(), 2);
        assert_eq!(w.entries.len(), 2);
        assert_eq!((w.entries[0].room, w.entries[1].room), (2, 17));
        assert_eq!(w.entries[1].kind, "forest_gate");
        assert!(w.colliders.iter().all(Option::is_some));
    }

    #[test]
    fn the_exported_forest_temple_has_an_elevator_three_gates_and_pillars() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let (entries, shapes) = load_entries(&root, "forest_temple", 1);
        if entries.is_empty() {
            return eprintln!("no OoT data imported; skipped");
        }
        let count = |k: &str| entries.iter().filter(|e| e.kind == k).count();
        assert_eq!((count("forest_gate"), count("forest_pillars")), (3, 1));
        for e in &entries {
            let col = e.collision.as_ref().unwrap();
            assert!(shapes.get(col).is_some_and(|s| s.triangles.len() > 4), "{col}");
            assert!(root.join(e.model.as_ref().unwrap()).exists(), "{:?}", e.model);
        }
        let w = MoverWorld::build(entries, &shapes, &|_| false, true, None);
        // the elevator starts at the top of the shaft (233 units) in the main hall
        let lift = w.movers.list.iter().find(|m| m.kind == Kind::ForestElevator).unwrap();
        assert!((lift.y - 233.0).abs() < 0.5, "{}", lift.y);
    }

    #[test]
    fn a_platform_under_link_is_owned_by_its_collider_and_moves_with_the_world() {
        use jumpman_sim::collision::CollisionWorld;
        let shape = CollisionData {
            rooms: vec![],
            vertices: vec![[-500.0, 0.0, -500.0], [500.0, 0.0, -500.0], [500.0, 0.0, 500.0], [-500.0, 0.0, 500.0]],
            triangles: vec![[0, 2, 1, 0, 0], [0, 3, 2, 0, 0]],
            hookable: None,
        };
        let shapes = HashMap::from([("lift".to_string(), shape)]);
        let mut w = MoverWorld::build(vec![MoverEntry { pos: [0.0, 400.0, 0.0], ..entry("jabu_elevator", 1, 0xFF01, Some("lift")) }], &shapes, &|_| false, true, None);
        let mut world = CollisionWorld::new(1024.0, 1e9);
        w.sync(&mut world);
        let floor = world.find_floor(0.0, 600.0, 0.0);
        assert!(w.colliders[0].as_ref().unwrap().owns(&floor.surface));
        assert!((floor.height - 400.0).abs() < 1.0, "{}", floor.height);
        // after some ticks the platform has moved and so has the floor
        let none = |_: i32| false;
        for _ in 0..60 {
            w.movers.tick(&World { player: [9e5; 3], room: Some(1), on_top: &[false], pressed: &[false], flag: &none, adult: true, water_y: None, grip: None, night: false, scene: "" });
        }
        let moved = w.sync(&mut world);
        assert!(moved[0].0[1].abs() > 1.0, "the displacement is reported for the carry");
        let floor = world.find_floor(0.0, 4000.0, 0.0);
        assert!((floor.height - w.movers.pos(0).unwrap()[1]).abs() < 1.0);
    }

    #[test]
    fn link_can_hold_a_gravestone_from_its_foot_side_only() {
        // a slab 100 wide (x) and 40 deep (z) facing +z, as the grave is placed with yaw 0
        let slab = CollisionData {
            rooms: vec![],
            vertices: vec![[-50.0, 0.0, 0.0], [50.0, 0.0, 0.0], [50.0, 100.0, 0.0], [-50.0, 100.0, 0.0], [-50.0, 0.0, 40.0], [50.0, 0.0, 40.0]],
            triangles: vec![[0, 1, 2, 0, 0]],
            hookable: None,
        };
        let shapes = HashMap::from([("grave".to_string(), slab)]);
        let w = MoverWorld::build(vec![MoverEntry { pos: [1000.0, 50.0, 2000.0], ..entry("grave", 1, 0, Some("grave")) }], &shapes, &|_| false, true, None);
        let at = |dx: f64, dz: f64| [1000.0 + dx, 50.0, 2000.0 + dz];
        // at the foot (-z side), looking along +z (yaw 0): a hold
        let f = w.grave_face(at(0.0, -2.0 * U), 0.0).expect("at the foot of the grave");
        assert_eq!(f.id, GRAVE_BASE);
        assert!(f.yaw.abs() < 1e-9);
        // facing away, to the side, or behind the slab: none
        assert!(w.grave_face(at(0.0, -2.0 * U), 32768.0).is_none());
        assert!(w.grave_face(at(300.0, -2.0 * U), 0.0).is_none());
        assert!(w.grave_face(at(0.0, 300.0), 0.0).is_none());
        assert!(w.grave_face(at(0.0, -100.0 * U), 0.0).is_none());
        // a slab turned a quarter (yaw 90 degrees: its front looks along +x): the foot side is -x
        let turned = MoverWorld::build(vec![MoverEntry { pos: [1000.0, 50.0, 2000.0], yaw: 90.0, ..entry("grave", 1, 0, Some("grave")) }], &shapes, &|_| false, true, None);
        let f = turned.grave_face(at(-2.0 * U, 0.0), 16384.0).expect("foot of the turned slab");
        assert!((f.yaw - 16384.0).abs() < 1e-6);
        assert!(turned.grave_face(at(0.0, -2.0 * U), 0.0).is_none());
    }
}
