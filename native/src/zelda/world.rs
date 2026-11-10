//! `ZeldaWorld`: the Zelda counterpart of the `Game.ts` block that runs the village visit, the
//! dungeon, Gohma, the slingshot and the sword quest around the sim step. Engine-free apart
//! from the `Resource` derive, so it is tested headless. The Bevy views (`view.rs`, `ui.rs`,
//! `camera.rs`) read its public state.
use super::dungeon::{Collider, DekuDungeon};
use super::gohma::Gohma;
use super::save::ZeldaSave;
use super::scene::{quest_part, ZeldaScene};
use super::slingshot::{FairySlingshot, SeedTarget};
use super::sword_quest::{KokiriSwordQuest, QuestData};
use super::visit::{KokiriVisit, VisitEvent};
use crate::camera::rig::camera_ray;
use crate::game::native_collision::add_native_collision;
use crate::game::runner::GameSim;
use bevy::math::DVec3;
use bevy::prelude::Resource;
use jumpman_sim::collision::{CollisionWorld, Surface};
use jumpman_sim::math::Vec3;
use jumpman_sim::state::*;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

/// Ask the shell for another course (`loadCourse(course, entry, ..)`).
#[derive(Clone, Debug, PartialEq)]
pub struct Travel {
    pub course: String,
    pub entry: String,
    pub area: i32,
    pub push: bool,
}

pub struct Heart {
    pub pos: Vec3,
    pub visible: bool,
}

/// The "neck" wall in front of Mido, as two windings of one quad, plus the tree mouth collision.
struct QuestCollision {
    walls: Vec<Arc<Surface>>,
    wall_on: Option<bool>,
    mouth: Option<Collider>,
    mouth_phase: f64,
}

#[derive(Resource)]
pub struct ZeldaWorld {
    pub scene: ZeldaScene,
    pub visit: KokiriVisit,
    pub dungeon: Option<DekuDungeon>,
    pub gohma: Option<Gohma>,
    /// Gohma boss-room parts: (boss, heart container) objects.
    pub boss_parts: Option<(Vec3, Vec3)>,
    pub slingshot: FairySlingshot,
    pub quest: Option<KokiriSwordQuest>,
    quest_collision: Option<QuestCollision>,
    /// Rupee keys already picked up in this visit.
    pub taken: BTreeSet<String>,
    pub hearts: Vec<Heart>,
    /// Chest lid animations the view should (re)start: (key, speed).
    pub lid_requests: Vec<String>,
    pub toasts: Vec<String>,
    pub prompt: Option<String>,
    pub coin_sfx: bool,
    pub travel: Option<Travel>,
    pub aiming: bool,
    last_aim: bool,
}

#[derive(Default, Debug)]
pub struct WorldStep {
    /// Skip the player's simulation this tick.
    pub skip: bool,
}

fn dist2(a: Vec3, b: Vec3) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

impl ZeldaWorld {
    pub fn new(root: &Path, scene: ZeldaScene, sim: &mut GameSim, save: &ZeldaSave) -> Self {
        let mut scene = scene;
        // Static collision of scene objects (chests, rupee boxes...); dungeon parts are movable.
        {
            let world = Arc::make_mut(&mut sim.mario.world);
            let ground = |world: &CollisionWorld, p: Vec3, up: f64| {
                let hit = world.find_floor(p[0], p[1] + up, p[2]);
                hit.surface.is_some().then_some(hit.height)
            };
            for o in scene.objects.iter_mut() {
                if o.treasure_key.is_some() {
                    if let Some(h) = ground(world, o.pos, 100.0) {
                        o.pos[1] = h;
                        if let Some(c) = scene.kokiri.treasure_chests.iter_mut().find(|c| Some(&c.key) == o.treasure_key.as_ref()) {
                            c.pos[1] = h;
                        }
                    }
                } else if o.grounded_prop {
                    if let Some(h) = ground(world, o.pos, 150.0) {
                        o.pos[1] = h;
                    }
                }
                if o.dungeon_role.is_none() {
                    if let Some(extra) = o.collision.as_ref().and_then(|k| scene.extra_collision.get(k)) {
                        add_native_collision(world, extra, o.pos, o.yaw);
                    }
                }
            }
        }
        // Portrait framing and talk range from the speakers' model bounds.
        for o in scene.objects.iter().filter(|o| o.speaker.is_some()) {
            let Some((lo, hi)) = super::glb::bounds(&root.join(&o.model)) else { continue };
            let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
            let mut center = [(lo[0] + hi[0]) / 2.0 + o.pos[0], (lo[1] + hi[1]) / 2.0 + o.pos[1], (lo[2] + hi[2]) / 2.0 + o.pos[2]];
            center[1] += size[1] * 0.12;
            if let Some(spot) = scene.kokiri.talks.iter_mut().find(|t| t.id == o.speaker) {
                spot.portrait = Some(super::scene::Portrait {
                    center,
                    distance: 280.0f64.max(size[1] * 1.4).max(size[0] * 1.25).max(size[2] * 1.25),
                    yaw: o.yaw.to_radians() + 0.33,
                });
                spot.range = Some(spot.range.unwrap_or(240.0).max(size[0].max(size[2]) * 0.55 + 120.0));
            }
        }
        let hearts = scene.objects.iter().filter(|o| o.recovery_heart).map(|o| Heart { pos: o.pos, visible: true }).collect();
        let dungeon = if scene.course == "deku_tree" && scene.area == 1 { DekuDungeon::new(&scene, &sim.mario.world) } else { None };
        let boss_parts = {
            let boss = scene.objects.iter().find(|o| o.dungeon_role.as_deref() == Some("gohma")).map(|o| o.pos);
            let heart = scene.objects.iter().find(|o| o.dungeon_role.as_deref() == Some("container")).map(|o| o.pos);
            boss.zip(heart)
        };
        let gohma = boss_parts.map(|(b, h)| {
            let floor = sim.mario.world.find_floor(b[0], b[1], b[2]).height;
            let mut g = Gohma::new(b, h, floor);
            g.restore(save);
            g
        });
        let quest = scene.kokiri.quest.as_ref().map(|q| {
            let mut quest = KokiriSwordQuest::new(QuestData {
                path: q.path.clone(),
                chest: q.chest,
                mouth_closed: q.mouth.closed,
                mouth_opened: q.mouth.opened,
                mouth_eye: q.mouth.eye,
                mouth_look_from: q.mouth.look_from,
                mouth_look_to: q.mouth.look_to,
            });
            quest.restore(save);
            quest
        });
        let quest_collision = quest.as_ref().map(|_| QuestCollision {
            walls: vec![],
            wall_on: None,
            mouth: scene.extra_collision.get("tree_mouth").map(|d| Collider::new(d.clone(), 0.0)),
            mouth_phase: f64::NAN,
        });
        let mut w = Self {
            scene,
            visit: KokiriVisit::default(),
            dungeon,
            gohma,
            boss_parts,
            slingshot: FairySlingshot::default(),
            quest,
            quest_collision,
            taken: BTreeSet::new(),
            hearts,
            lid_requests: vec![],
            toasts: vec![],
            prompt: None,
            coin_sfx: false,
            travel: None,
            aiming: false,
            last_aim: false,
        };
        w.restore(sim, save);
        w
    }

    /// `updateSaveGeometry`: bring scene state in line with the save.
    pub fn restore(&mut self, sim: &mut GameSim, save: &ZeldaSave) {
        self.taken = self.scene.kokiri.rupees.iter().filter(|p| save.has_village_pickup(&p.key)).map(|p| p.key.clone()).collect();
        if let Some(d) = &mut self.dungeon {
            d.restore(&mut sim.mario, save);
        }
        if let Some(g) = &mut self.gohma {
            g.restore(save);
        }
        if let Some(q) = &mut self.quest {
            q.restore(save);
        }
        self.sync_quest_collision(sim);
        self.update_max_health(&mut sim.mario, save);
    }

    fn update_max_health(&self, m: &mut jumpman_sim::mario_runtime::MarioRuntime, save: &ZeldaSave) {
        m.max_health = if save.deku_cleared("heart") { 0x980 as f64 } else { 0x880 as f64 };
    }

    fn sync_quest_collision(&mut self, sim: &mut GameSim) {
        let (Some(q), Some(qc)) = (&self.quest, &mut self.quest_collision) else { return };
        let blocked = q.passage_blocked;
        let phase = q.mouth_phase;
        if qc.wall_on == Some(blocked) && (qc.mouth_phase == phase) {
            return;
        }
        let world = Arc::make_mut(&mut sim.mario.world);
        if qc.wall_on != Some(blocked) {
            if !qc.walls.is_empty() {
                let owned: Vec<*const Surface> = qc.walls.iter().map(Arc::as_ptr).collect();
                world.remove_surfaces(&|s| owned.contains(&(s as *const Surface)));
                qc.walls.clear();
            }
            if blocked {
                for quad in q.passage_quads() {
                    qc.walls.extend(world.add_quad(quad[0], quad[1], quad[2], quad[3], &Default::default()));
                }
            }
            qc.wall_on = Some(blocked);
        }
        if qc.mouth_phase != phase || qc.mouth_phase.is_nan() {
            if let Some(c) = &mut qc.mouth {
                c.move_to(world, Some(q.mouth_position()));
            }
            qc.mouth_phase = phase;
        }
    }

    pub fn seed_targets(&self, save: &ZeldaSave) -> Vec<SeedTarget> {
        if let Some(g) = &self.gohma {
            return g.targets();
        }
        self.dungeon.as_ref().map(|d| d.targets(save)).unwrap_or_default()
    }

    /// The Game.ts "village" block. Returns whether the player's own step is skipped.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, sim: &mut GameSim, controller: &ControllerSnapshot, save: &mut ZeldaSave, name: &str, slingshot_held: bool, link: bool, clock_ms: f64) -> WorldStep {
        let mut out = WorldStep::default();
        self.prompt = None;
        self.coin_sfx = false;
        self.visit.clock_ms = clock_ms;
        self.update_max_health(&mut sim.mario, save);
        // Exits (the floor's room number selects the destination).
        let m = &sim.mario;
        let grounded = m.floor.is_some() && m.pos[1] <= m.floor_height + 20.0;
        if grounded && self.travel.is_none() {
            let room = m.floor.as_ref().and_then(|f| f.room).unwrap_or(0);
            if let Some(exit) = self.scene.kokiri.exits.get(&room.to_string()) {
                if exit.course != "deku_tree" || self.quest.as_ref().is_some_and(|q| q.mouth_open()) {
                    self.travel = Some(Travel { course: exit.course.clone(), entry: exit.entry.clone(), area: 1, push: false });
                    out.skip = true;
                    return out;
                }
            }
        }
        let talking_mido = self.visit.speaker_id() == Some("mido");
        if let Some(q) = &mut self.quest {
            q.step(save, sim.mario.pos, talking_mido);
            if let Some(spot) = self.scene.kokiri.talks.iter_mut().find(|t| t.id.as_deref() == Some("mido")) {
                spot.pos = q.talk;
            }
            if q.start_opening {
                self.lid_requests.push("sword_chest".into());
            }
            if q.mouth_opening() {
                self.sync_quest_collision(sim);
                out.skip = true;
                return out;
            }
        }
        self.sync_quest_collision(sim);
        // Recovery hearts and rupees.
        let m = &mut sim.mario;
        if m.health < m.max_health {
            let pos = m.pos;
            if let Some(h) = self.hearts.iter_mut().find(|h| h.visible && (h.pos[1] - pos[1]).abs() < 135.0 && dist2(h.pos, pos) < 95.0) {
                h.visible = false;
                m.health = m.max_health.min(m.health + 0x100 as f64);
                self.coin_sfx = true;
            }
        }
        for p in &self.scene.kokiri.rupees {
            if self.taken.contains(&p.key) {
                continue;
            }
            let radius = if p.hidden { 220.0 } else { 132.0 };
            let pos = m.pos;
            if dist2(pos, p.pos) >= radius || (pos[1] - p.pos[1]).abs() >= 132.0 {
                continue;
            }
            let before = save.rupees;
            if !save.collect_village_rupees(&p.key, p.value, p.permanent) {
                continue;
            }
            if save.rupees > before {
                self.coin_sfx = true;
            }
            self.taken.insert(p.key.clone());
        }
        // Slingshot, Gohma, dungeon.
        let mut toasts: Vec<String> = vec![];
        let targets = self.seed_targets(save);
        let busy = self.visit.busy();
        let aim = slingshot_held && !busy;
        let world = sim.mario.world.clone();
        let terrain = |o: Vec3, d: Vec3, far: f64| camera_ray(&world, DVec3::from(o), DVec3::from(d), far);
        let step = self.slingshot.step(&mut sim.mario, controller, aim, save, &targets, &terrain, &mut toasts);
        drop(world);
        let aiming = self.slingshot.aiming;
        self.aiming = aiming;
        if let Some(id) = step.hit {
            if self.gohma.is_some() {
                self.gohma.as_mut().unwrap().seed_hit();
            } else if let Some(d) = &mut self.dungeon {
                d.seed_hit(id, save);
            }
        }
        if let Some(g) = &mut self.gohma {
            let o = g.step(&mut sim.mario, controller, save, aiming, link);
            self.prompt = o.prompt.map(str::to_string);
            if o.notify.iter().any(|n| n.contains("defeated")) {
                super::music::queue_jingle(crate::audio::oot_music::JINGLE_BOSS_CLEAR);
            }
            toasts.extend(o.notify.iter().map(|s| s.to_string()));
            if o.return_to_forest {
                self.travel = Some(Travel { course: "kokiri_forest".into(), entry: "tree".into(), area: 1, push: false });
            }
        }
        let mut handled = aiming;
        if let Some(d) = &mut self.dungeon {
            let r = d.step(&mut sim.mario, controller, save, !aiming && !busy);
            toasts.append(&mut d.toasts);
            if self.prompt.is_none() {
                self.prompt = d.prompt.clone();
            }
            if r.enter_boss {
                self.travel = Some(Travel { course: "deku_tree".into(), entry: "boss".into(), area: 2, push: true });
            }
            handled |= r.handled;
        }
        self.toasts.extend(toasts);
        if aiming {
            crate::hints::note("WASD / STICK - AIM    SPACE / A - FIRE (slingshot)");
            self.prompt = Some(if crate::hints::show() { format!("WASD / STICK - AIM    SPACE / A - FIRE    {} SEEDS", save.deku_seeds()) } else { format!("{} SEEDS", save.deku_seeds()) });
        }
        self.last_aim = aiming;
        if handled {
            out.skip = true;
            return out;
        }
        let stick_y = controller.stick_y;
        let consumed = self.visit.step(&self.scene, &mut sim.mario, controller.button_pressed, name, save, stick_y);
        for e in std::mem::take(&mut self.visit.events) {
            match e {
                VisitEvent::OpenLid(key) => self.lid_requests.push(key),
                VisitEvent::Travel(course) => self.travel = Some(Travel { course, entry: "warp-pad".into(), area: 1, push: false }),
            }
        }
        if consumed {
            self.sync_quest_collision(sim);
            out.skip = true;
            return out;
        }
        if self.prompt.is_none() {
            self.prompt = self.visit.prompt(&self.scene.kokiri, sim.mario.pos, save).or_else(|| (save.slingshot_equipped()).then(|| "H / RB - AIM SLINGSHOT".to_string()));
        }
        out
    }

    /// Mido's route piece names (`questParts`), for the view.
    pub fn quest_part_names() -> [&'static str; 4] {
        ["sword_chest", "kokiri_sword", "mido", "deku_tree_mouth"]
    }

    pub fn is_quest_model(model: &str) -> bool {
        quest_part(model).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn setup(course: &str, area: i32) -> (GameSim, ZeldaWorld, ZeldaSave) {
        let mut req = CourseRequest::new(course);
        req.area = area;
        let mut sim = GameSim::load(&assets(), req).unwrap();
        let save = ZeldaSave::default();
        let scene = ZeldaScene::load(&assets(), course, area).unwrap();
        let w = ZeldaWorld::new(&assets(), scene, &mut sim, &save);
        (sim, w, save)
    }

    fn idle() -> ControllerSnapshot {
        ControllerSnapshot::default()
    }

    #[test]
    fn forest_loads_and_ticks_with_world() {
        let (mut sim, mut w, mut save) = setup("kokiri_forest", 1);
        assert!(w.quest.is_some() && w.dungeon.is_none());
        for _ in 0..90 {
            let r = w.step(&mut sim, &idle(), &mut save, "Link", false, true, 0.0);
            if !r.skip {
                sim.tick(&idle());
            }
        }
        assert!(sim.mario.pos[1].is_finite());
        // Mido blocks the path with a wall until the path opens.
        let walls = w.quest_collision.as_ref().unwrap().walls.len();
        assert!(walls >= 2);
    }

    #[test]
    fn rupees_are_collected_and_kept_per_save() {
        let (mut sim, mut w, mut save) = setup("kokiri_forest", 1);
        let r = w.scene.kokiri.rupees.iter().find(|r| r.permanent).or(w.scene.kokiri.rupees.first()).unwrap().clone();
        sim.mario.pos = r.pos;
        w.step(&mut sim, &idle(), &mut save, "Link", false, true, 0.0);
        assert!(save.rupees >= r.value.min(99) && w.taken.contains(&r.key));
        assert!(w.coin_sfx);
    }

    #[test]
    fn deku_tree_exit_and_dungeon_world() {
        let (mut sim, mut w, mut save) = setup("deku_tree", 1);
        assert!(w.dungeon.is_some());
        let n = sim.mario.world.surfaces().len();
        assert!(n > sim.course.world.surfaces().len());
        for _ in 0..60 {
            let r = w.step(&mut sim, &idle(), &mut save, "Link", false, true, 0.0);
            if !r.skip {
                sim.tick(&idle());
            }
        }
        assert!(w.travel.is_none() || w.travel.as_ref().unwrap().course == "kokiri_forest");
    }

    #[test]
    fn boss_room_has_gohma() {
        let (mut sim, mut w, mut save) = setup("deku_tree", 2);
        assert!(w.gohma.is_some());
        for _ in 0..30 {
            let r = w.step(&mut sim, &idle(), &mut save, "Link", false, true, 0.0);
            if !r.skip {
                sim.tick(&idle());
            }
        }
    }
}
