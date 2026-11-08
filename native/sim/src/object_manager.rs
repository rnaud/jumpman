//! Object list processor and Mario interaction dispatcher (src/sim/objectManager.ts).
//! Runs once per 30 Hz tick before Mario's action step, mirroring level_update.c.
//!
//! # Ownership
//! The `ObjectManager` is owned by [`MarioRuntime`] (`m.objects`), because Mario's actions
//! and the behaviors both need to reach objects and Mario at the same time. Stepping and
//! anything that needs both are therefore free functions taking `m: &mut MarioRuntime`
//! ([`step`], [`after_mario_step`], [`explode`]); the TS `ObjectContext` methods that only
//! push events (`waterBombEvent`, `spawnCoin`, ...) are plain `&mut self` methods.
//! There is no `world` field: behaviors read `m.world` (an `Arc<CollisionWorld>`; clone it
//! if you need it while holding a `&mut` into `m.objects`).
//!
//! # Behaviors
//! A behavior is a pair of plain functions registered per [`ObjectBehaviorKind`] in the
//! [`BehaviorTable`] (the TS `INITIALIZERS` / `UPDATERS` records):
//! `fn init(&mut SimObject)` and `fn update(id: ObjId, m: &mut MarioRuntime)`.
//! Ports of `src/sim/behaviors/*.ts` live in `src/behaviors/<name>.rs` and add themselves
//! in `behaviors::register_all`. Kinds without a registered function are inert.
use crate::behaviors::koopa::{KOOPA_DIVE, KOOPA_LYING, KOOPA_SQUISHED};
use crate::characters::sword_reach;
use crate::collision::FLOOR_LOWER_LIMIT;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;
use std::ops::{Index, IndexMut};

pub type InitFn = fn(&mut SimObject);
pub type UpdateFn = fn(ObjId, &mut MarioRuntime);
/// Mario-overlap hook of a behavior kind; `true` = handled, skip the generic dispatcher.
pub type InteractFn = fn(ObjId, &mut MarioRuntime) -> bool;

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectEvent {
    WaterBomb { pos: Vec3, phase: &'static str },
    WaterCannon { pos: Vec3 },
    Explosion { pos: Vec3, radius: f64, mine: bool },
    Break { pos: Vec3, wall: Option<&'static str> },
    BowlingBall { pos: Vec3, cue: &'static str },
    KoopaSound { pos: Vec3, cue: &'static str },
    CoinBounce { pos: Vec3 },
    MovingCoinCollected { pos: Vec3 },
    Coin { pos: Vec3, loot: bool, value: f64 },
    Hurt { pos: Vec3 },
    KingLeave,
    KingDialog { king: ObjId, id: f64 },
    Boss { pos: Vec3 },
    BigBoo { pos: Vec3 },
    OneUp { pos: Vec3 },
    WhompKing { pos: Vec3 },
    BigBully { pos: Vec3 },
    Bowser { pos: Vec3 },
    Chest { number: f64 },
    BoxStar { pos: Vec3, act: f64 },
    Snowman { pos: Vec3, cue: &'static str },
    RacerSound { pos: Vec3, cue: &'static str },
    PenguinSound { pos: Vec3, diving: bool },
    PenguinReward { pos: Vec3 },
    ChestReward,
    Race { state: &'static str },
    Dialogue { text: String },
    Warp { destination: String, entry: Option<String>, area: Option<i32> },
    /// SM64 sound effect (sounds.json name) at a position (`cur_obj_play_sound_2`), used by behaviors/g1.
    Sound { pos: Vec3, name: &'static str },
    /// `spawn_default_star(x, y, z)` from a boss / event behavior (SM64 round 2): the act's event star rises from `pos`.
    DefaultStar { pos: Vec3 },
}

/// Per-kind init/update function tables.
#[derive(Clone)]
pub struct BehaviorTable {
    init: Vec<Option<InitFn>>,
    update: Vec<Option<UpdateFn>>,
    interact: Vec<Option<InteractFn>>,
}

impl Default for BehaviorTable {
    fn default() -> Self { Self::new() }
}

impl BehaviorTable {
    /// Table with only the behaviors that are inline in objectManager.ts.
    pub fn new() -> Self {
        let mut t = Self { init: vec![None; ObjectBehaviorKind::COUNT], update: vec![None; ObjectBehaviorKind::COUNT], interact: vec![None; ObjectBehaviorKind::COUNT] };
        use ObjectBehaviorKind as K;
        t.init[K::GrillDoor.index()] = Some(|o| { o.visible = true; o.set_var("timer", 0.0); });
        t.init[K::MountainBallSpawner.index()] = Some(|o| { o.visible = false; o.timer = -1.0; });
        t.init[K::WaterBombSpawner.index()] = Some(|o| { o.visible = false; });
        t.init[K::WaterBombShadow.index()] = Some(|o| { o.scale = 1.5; });
        t.init[K::BlastWall.index()] = Some(|o| { o.hitbox.radius = 300.0; o.hitbox.height = 400.0; });
        t.update[K::BlastWall.index()] = Some(blast_wall);
        t
    }
    pub fn set_init(&mut self, kind: ObjectBehaviorKind, f: InitFn) { self.init[kind.index()] = Some(f); }
    pub fn set_update(&mut self, kind: ObjectBehaviorKind, f: UpdateFn) { self.update[kind.index()] = Some(f); }
    /// Register both at once (the usual case for a ported behavior file).
    pub fn register(&mut self, kind: ObjectBehaviorKind, init: InitFn, update: UpdateFn) {
        self.set_init(kind, init);
        self.set_update(kind, update);
    }
    pub fn set_interact(&mut self, kind: ObjectBehaviorKind, f: InteractFn) { self.interact[kind.index()] = Some(f); }
    pub fn interact_fn(&self, kind: ObjectBehaviorKind) -> Option<InteractFn> { self.interact[kind.index()] }
    pub fn init_fn(&self, kind: ObjectBehaviorKind) -> Option<InitFn> { self.init[kind.index()] }
    pub fn update_fn(&self, kind: ObjectBehaviorKind) -> Option<UpdateFn> { self.update[kind.index()] }
}

/// 'blastWall' updater (inline in objectManager.ts).
fn blast_wall(id: ObjId, m: &mut MarioRuntime) {
    if m.action == ACT_SHOT_FROM_CANNON && mario_overlaps_object(m, &m.objects[id]) {
        m.objects.break_object(id);
        let pos = m.objects[id].pos;
        m.take_damage(1.0, Some(pos));
        m.objects[id].deactivate(-1.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChestPuzzle {
    pub next: i32,
    pub locked: bool,
}

pub struct ObjectManager {
    pub chest_puzzle: ChestPuzzle,
    chest_reward_timer: f64,
    pub objects: Vec<SimObject>,
    pub events: Vec<ObjectEvent>,
    pub penguin_race: Option<PenguinRace>,
    pub snowman_path: Option<Vec<Vec3>>,
    pub behaviors: BehaviorTable,
    specs: Vec<ObjectSpec>,
}

impl Default for ObjectManager {
    fn default() -> Self { Self::new(vec![], None, None) }
}

impl Index<ObjId> for ObjectManager {
    type Output = SimObject;
    fn index(&self, id: ObjId) -> &SimObject { &self.objects[id.idx()] }
}
impl IndexMut<ObjId> for ObjectManager {
    fn index_mut(&mut self, id: ObjId) -> &mut SimObject { &mut self.objects[id.idx()] }
}

impl ObjectManager {
    pub fn new(specs: Vec<ObjectSpec>, penguin_race: Option<PenguinRace>, snowman_path: Option<Vec<Vec3>>) -> Self {
        let mut behaviors = BehaviorTable::new();
        crate::behaviors::register_all(&mut behaviors);
        let mut mgr = Self {
            chest_puzzle: ChestPuzzle { next: 1, locked: false },
            chest_reward_timer: 0.0,
            objects: vec![],
            events: vec![],
            penguin_race,
            snowman_path,
            behaviors,
            specs,
        };
        mgr.reset();
        mgr
    }

    pub fn len(&self) -> usize { self.objects.len() }
    pub fn is_empty(&self) -> bool { self.objects.is_empty() }
    pub fn ids(&self) -> impl Iterator<Item = ObjId> { (0..self.objects.len() as u32).map(ObjId) }

    fn init_object(&mut self, id: ObjId) {
        let kind = self[id].behavior;
        if let Some(f) = self.behaviors.init_fn(kind) { f(&mut self[id]); }
    }

    pub fn spawn(&mut self, spec: &ObjectSpec) -> ObjId {
        let mut object = SimObject::new(spec);
        if let Some(f) = self.behaviors.init_fn(object.behavior) { f(&mut object); }
        self.objects.push(object);
        ObjId(self.objects.len() as u32 - 1)
    }

    /// `spawnObject` / `spawn` of ObjectContext.
    pub fn spawn_object(&mut self, spec: &ObjectSpec) -> ObjId { self.spawn(spec) }

    pub fn reset_racer(&mut self) {
        let Some(id) = self.find_by_behavior(ObjectBehaviorKind::RacingPenguin) else { return };
        let Some(spec_yaw) = self.specs.iter().find(|s| s.behavior == ObjectBehaviorKind::RacingPenguin).map(|s| s.yaw) else { return };
        let racer = &mut self[id];
        racer.reactivate();
        racer.set_action(0);
        racer.move_yaw = spec_yaw;
        racer.face_yaw = spec_yaw;
        self.init_object(id);
    }

    pub fn find_by_behavior(&self, behavior: ObjectBehaviorKind) -> Option<ObjId> {
        self.objects.iter().position(|o| o.behavior == behavior && o.active).map(|i| ObjId(i as u32))
    }

    /// `CourseObjects.setSnowmanCompleted`: once the Snowman's Lost His Head star is owned the
    /// placeholder body (param -1) shows and the head sits on it.
    pub fn set_snowman_completed(&mut self, completed: bool) {
        let (Some(body), Some(head)) = (
            self.find_by_behavior(ObjectBehaviorKind::SnowmanBody),
            self.find_by_behavior(ObjectBehaviorKind::SnowmanHead),
        ) else { return };
        if self.objects[body.idx()].param != -1.0 { return; }
        self.objects[body.idx()].visible = completed;
        let h = &mut self.objects[head.idx()];
        h.pos = h.home;
        h.set_var("placed", if completed { 1.0 } else { 0.0 });
        if completed { h.pos[1] = -994.0; }
        h.set_action(if completed { 1 } else { 0 });
    }

    /// Rebuild every object from the course specs. Callers must also clear any `ObjId`s they
    /// hold (`MarioRuntime::reset_to_spawn` does this for Mario).
    pub fn reset(&mut self) {
        self.chest_puzzle = ChestPuzzle { next: 1, locked: false };
        self.chest_reward_timer = 0.0;
        self.objects.clear();
        self.events.clear();
        for i in 0..self.specs.len() {
            let spec = self.specs[i].clone();
            self.spawn(&spec);
        }
    }

    pub fn drain_events(&mut self) -> Vec<ObjectEvent> { std::mem::take(&mut self.events) }

    /// Rewrite the behavior kind (and optionally the param) of placed specs, then rebuild every
    /// object. Used to swap in ports without touching the exported placements.
    pub fn retarget(&mut self, f: &dyn Fn(&ObjectSpec) -> Option<(ObjectBehaviorKind, Option<f64>)>) {
        for s in &mut self.specs {
            if let Some((kind, param)) = f(s) {
                s.behavior = kind;
                if param.is_some() { s.param = param; }
            }
        }
        self.reset();
    }

    /// Append placements (they survive `reset`) and rebuild every object.
    pub fn add_specs(&mut self, specs: Vec<ObjectSpec>) {
        self.specs.extend(specs);
        self.reset();
    }

    // ObjectContext event helpers ------------------------------------------------------------

    pub fn water_bomb_event(&mut self, pos: Vec3, phase: &'static str) { self.events.push(ObjectEvent::WaterBomb { pos, phase }); }
    pub fn water_cannon_fired(&mut self, pos: Vec3) { self.events.push(ObjectEvent::WaterCannon { pos }); }
    pub fn bowling_ball_event(&mut self, pos: Vec3, cue: &'static str) { self.events.push(ObjectEvent::BowlingBall { pos, cue }); }
    pub fn koopa_sound(&mut self, pos: Vec3, cue: &'static str) { self.events.push(ObjectEvent::KoopaSound { pos, cue }); }
    pub fn coin_bounce(&mut self, pos: Vec3) { self.events.push(ObjectEvent::CoinBounce { pos }); }
    pub fn spawn_coin(&mut self, pos: Vec3, loot: bool, value: f64) { self.events.push(ObjectEvent::Coin { pos, loot, value }); }
    pub fn break_object(&mut self, source: ObjId) {
        let o = &self[source];
        let wall = if o.behavior == ObjectBehaviorKind::BlastWall { Some(if o.param != 0.0 { "right" } else { "left" }) } else { None };
        let pos = o.pos;
        self.events.push(ObjectEvent::Break { pos, wall });
    }
    pub fn king_leave(&mut self) { self.events.push(ObjectEvent::KingLeave); }
    pub fn king_dialog(&mut self, king: ObjId, id: f64) { self.events.push(ObjectEvent::KingDialog { king, id }); }
    pub fn boss_defeated(&mut self, pos: Vec3) { self.events.push(ObjectEvent::Boss { pos }); }
    pub fn big_boo_defeated(&mut self, pos: Vec3) { self.events.push(ObjectEvent::BigBoo { pos }); }
    pub fn one_up_collected(&mut self, pos: Vec3) { self.events.push(ObjectEvent::OneUp { pos }); }
    pub fn whomp_king_defeated(&mut self, pos: Vec3) { self.events.push(ObjectEvent::WhompKing { pos }); }
    pub fn big_bully_defeated(&mut self, pos: Vec3) { self.events.push(ObjectEvent::BigBully { pos }); }
    pub fn bowser_defeated(&mut self, pos: Vec3) { self.events.push(ObjectEvent::Bowser { pos }); }
    pub fn snowman_event(&mut self, pos: Vec3, cue: &'static str) { self.events.push(ObjectEvent::Snowman { pos, cue }); }
    pub fn racer_sound(&mut self, pos: Vec3, cue: &'static str) { self.events.push(ObjectEvent::RacerSound { pos, cue }); }
    pub fn penguin_sound(&mut self, pos: Vec3, diving: bool) { self.events.push(ObjectEvent::PenguinSound { pos, diving }); }
    pub fn penguin_reward(&mut self, pos: Vec3) { self.events.push(ObjectEvent::PenguinReward { pos }); }
    pub fn star_from_box(&mut self, box_id: ObjId) {
        let o = &self[box_id];
        let ev = ObjectEvent::BoxStar { pos: o.pos, act: o.param };
        self.events.push(ev);
    }
    pub fn chest_opened(&mut self, number: f64) { self.events.push(ObjectEvent::Chest { number }); }
    pub fn race_event(&mut self, state: &'static str) { self.events.push(ObjectEvent::Race { state }); }
    pub fn sound(&mut self, pos: Vec3, name: &'static str) { self.events.push(ObjectEvent::Sound { pos, name }); }
    pub fn dialogue(&mut self, text: &str) { self.events.push(ObjectEvent::Dialogue { text: text.to_string() }); }
    pub fn warp(&mut self, destination: &str, entry: Option<&str>, area: Option<i32>) {
        self.events.push(ObjectEvent::Warp { destination: destination.to_string(), entry: entry.map(str::to_string), area: area.filter(|a| *a != 0) });
    }
}

fn can_grab_action(m: &MarioRuntime) -> bool {
    ((m.action == ACT_PUNCHING || m.action == ACT_MOVE_PUNCHING) && m.action_arg < 2.0) || m.action == ACT_DIVE
}

fn call_update(m: &mut MarioRuntime, id: ObjId) {
    let kind = m.objects[id].behavior;
    if let Some(f) = m.objects.behaviors.update_fn(kind) { f(id, m); }
}

/// Run one object tick and refresh Mario's interaction targets (`ObjectManager.step`).
pub fn step(m: &mut MarioRuntime, time_stop_owner: Option<ObjId>, coin_handled: bool) {
    m.objects.events.clear();
    if let Some(owner) = time_stop_owner {
        m.objects[owner].timer += 1.0;
        call_update(m, owner);
        return;
    }
    if m.objects.chest_puzzle.next == 5 {
        m.objects.chest_reward_timer += 1.0;
        if m.objects.chest_reward_timer == 62.0 { m.objects.events.push(ObjectEvent::ChestReward); }
    }

    let mario_pos = m.pos;
    for i in 0..m.objects.objects.len() {
        let id = ObjId(i as u32);
        if !m.objects[id].active {
            let o = &mut m.objects[id];
            let far_enough = o.respawn_distance >= 0.0 && dist_3d(&o.home, &mario_pos) >= o.respawn_distance;
            let timer_ended = o.respawn_timer > 0.0 && { o.respawn_timer -= 1.0; o.respawn_timer == 0.0 };
            if far_enough || timer_ended {
                o.reactivate();
                if far_enough { o.move_yaw = 0.0; o.face_yaw = 0.0; }
                m.objects.init_object(id);
            }
            continue;
        }
        let o = &mut m.objects[id];
        o.timer += 1.0;
        o.distance_to_mario = dist_3d(&o.pos, &mario_pos);
        o.angle_to_mario = atan2s(mario_pos[2] - o.pos[2], mario_pos[0] - o.pos[0]) as f64;
        if o.held_state == HELD_HELD && m.held_obj == Some(id) { sync_held(m, id); }
    }

    if !coin_handled && m.action & ACT_FLAG_INTANGIBLE == 0 {
        let coin = m.objects.objects.iter().position(|o| o.active && o.behavior == ObjectBehaviorKind::MovingYellowCoin && o.timer > 10.0 && mario_overlaps_object(m, o));
        if let Some(c) = coin {
            let pos = m.objects.objects[c].pos;
            m.objects.events.push(ObjectEvent::MovingCoinCollected { pos });
            m.objects.objects[c].deactivate(-1.0);
        }
    }
    detect_mario_interactions(m);

    // Behaviors may spawn objects; the TS for-of loop visits objects pushed during iteration.
    let mut i = 0;
    while i < m.objects.objects.len() {
        let id = ObjId(i as u32);
        i += 1;
        if !m.objects[id].active { continue; }
        call_update(m, id);
        let o = &mut m.objects[id];
        if o.active && (o.pos[1] < FLOOR_LOWER_LIMIT || (!matches!(o.behavior, ObjectBehaviorKind::BkEnemy | ObjectBehaviorKind::Warp) && (o.pos[0].abs() > 8000.0 || o.pos[2].abs() > 8000.0))) {
            if o.behavior == ObjectBehaviorKind::Bobomb || o.behavior == ObjectBehaviorKind::BreakableBoxSmall {
                o.wait_to_respawn(3000.0);
            } else {
                o.deactivate(-1.0);
            }
        }
    }

    if let Some(h) = m.held_obj { sync_held(m, h); }
}

/// Keep a held object attached after Mario's action step has run.
pub fn after_mario_step(m: &mut MarioRuntime) {
    let held = m.held_obj;
    for (i, o) in m.objects.objects.iter_mut().enumerate() {
        if o.held_state == HELD_HELD && held != Some(ObjId(i as u32)) { o.held_state = HELD_DROPPED; }
    }
    if let Some(h) = held { sync_held(m, h); }
}

fn sync_held(m: &mut MarioRuntime, id: ObjId) {
    let yaw = m.face_angle[1];
    let mpos = m.pos;
    let o = &mut m.objects[id];
    // Bob-ombs hide their model while held in the original. Jumpman has no hand
    // attachment, so keep the object visible just in front of Mario instead.
    let (forward, height) = match o.behavior {
        ObjectBehaviorKind::Bowser | ObjectBehaviorKind::SmBowser => (315.0, 0.0),
        ObjectBehaviorKind::Bobomb => (150.0, 75.0),
        _ => (120.0, 55.0),
    };
    o.pos[0] = mpos[0] + js_sin((yaw / 32768.0) * std::f64::consts::PI) * forward;
    o.pos[1] = mpos[1] + height;
    o.pos[2] = mpos[2] + js_cos((yaw / 32768.0) * std::f64::consts::PI) * forward;
    o.move_yaw = yaw;
    o.face_yaw = yaw;
    o.forward_vel = 0.0;
    o.vel[1] = 0.0;
}

/// atBowserTail (behaviors/bowser.ts): tail anchor projected behind the root.
fn at_bowser_tail(o: &SimObject, mario: &MarioRuntime) -> bool {
    let dx = mario.pos[0] - (o.pos[0] - 300.0 * sins(o.move_yaw));
    let dz = mario.pos[2] - (o.pos[2] - 300.0 * coss(o.move_yaw));
    dx * dx + dz * dz < 137.0 * 137.0 && mario.pos[1] < o.pos[1] + 100.0 && mario.pos[1] + 160.0 > o.pos[1] + 50.0
}

fn detect_mario_interactions(m: &mut MarioRuntime) {
    m.interact_obj = None;
    m.interact_grabbable = false;
    m.near_grabbable = false;
    m.near_breakable = false;
    if m.action & ACT_FLAG_INTANGIBLE != 0 { return; }

    let count = m.objects.objects.len();
    for i in 0..count {
        let id = ObjId(i as u32);
        {
            let o = &m.objects[id];
            if !o.active || o.held_state == HELD_HELD { continue; }
        }
        let interact_type = m.objects[id].hitbox.interact_type;
        let reach = sword_reach(m);
        if !mario_overlaps_object(m, &m.objects[id]) {
            if reach == 0.0 || interact_type & (INTERACT_DAMAGE | INTERACT_BREAKABLE | INTERACT_KOOPA | INTERACT_GRABBABLE | INTERACT_MR_BLIZZARD) == 0 { continue; }
            if detect_interaction(m, &m.objects[id]) != ATTACK_PUNCH { continue; }
            let mut hitbox = mario_hitbox(m);
            hitbox.radius += reach;
            let o = &m.objects[id];
            if !hitboxes_overlap((&m.pos, &hitbox), (&o.pos, &o.hitbox)) { continue; }
        }

        let behavior = m.objects[id].behavior;
        if let Some(hook) = m.objects.behaviors.interact_fn(behavior) {
            if hook(id, m) {
                continue;
            }
        }
        if behavior == ObjectBehaviorKind::BobombBuddy || behavior == ObjectBehaviorKind::BobombBuddyAdvice {
            push_mario_out_of_object(m, id, -10.0);
            continue;
        }
        let attack = detect_interaction(m, &m.objects[id]);

        if behavior == ObjectBehaviorKind::SmBowser {
            crate::behaviors::bowser_sm64::touch(m, id, can_grab_action(m));
            continue;
        }
        if behavior == ObjectBehaviorKind::Bowser {
            if m.objects[id].action != 0 { continue; }
            let opos = m.objects[id].pos;
            if at_bowser_tail(&m.objects[id], m) {
                m.near_grabbable = true;
                if can_grab_action(m) {
                    m.interact_obj = Some(id);
                    m.interact_grabbable = true;
                }
            } else if js_hypot(m.pos[0] - opos[0], m.pos[2] - opos[2]) < 137.0 {
                m.take_damage(2.0, Some(opos));
            }
            continue;
        }

        if interact_type & INTERACT_POLE != 0 {
            m.try_grab_pole(id);
            continue;
        }

        if interact_type & INTERACT_KOOPA_SHELL != 0 {
            if m.riding_shell.is_none() && (attack == ATTACK_FROM_ABOVE || m.action == ACT_WALKING || m.action == ACT_HOLD_WALKING) {
                m.ride_shell(id);
            }
            continue;
        }
        if interact_type & INTERACT_KOOPA != 0 {
            if attack != 0 && behavior == ObjectBehaviorKind::Koopa {
                let o = &mut m.objects[id];
                let recovering = o.action == KOOPA_LYING || o.action == KOOPA_DIVE;
                if (recovering && o.action_timer <= 10.0) || o.action >= KOOPA_SQUISHED { continue; }
                o.set_var("hit", attack as f64);
                if attack == ATTACK_FROM_ABOVE {
                    let y = o.pos[1] + o.hitbox.height + 60.0;
                    m.pos[1] = y;
                    m.vel[1] = 30.0;
                    m.set_action(ACT_FREEFALL, 0.0);
                }
            }
            continue;
        }

        // BOUNCE_TOP / HIT_FROM_BELOW / SNUFIT_BULLET (interaction.c), behaviors/g1/interact.rs.
        if crate::behaviors::g1::interact::dispatch(m, id, attack) { continue; }

        if interact_type & (INTERACT_DAMAGE | INTERACT_MR_BLIZZARD) != 0 {
            if attack != 0 {
                attack_object(&mut m.objects[id], attack);
                continue;
            }
            let (opos, dmg) = { let o = &m.objects[id]; (o.pos, o.hitbox.damage_or_coin_value) };
            if m.take_damage(if dmg != 0.0 { dmg } else { 1.0 }, Some(opos)) {
                m.objects[id].interact_status |= INT_STATUS_INTERACTED;
            }
            continue;
        }

        if interact_type & INTERACT_GRABBABLE != 0 {
            if m.objects[id].interact_subtype & INT_SUBTYPE_NOT_GRABBABLE != 0 { continue; }
            if behavior == ObjectBehaviorKind::KingBobomb {
                let (blocked, no_cooldown, in_cone) = {
                    let o = &m.objects[id];
                    (
                        o.held_state != HELD_FREE || o.var("grabbing") != 0.0 || m.vanished(),
                        o.var("grabCooldown") == 0.0,
                        (to_s16(o.angle_to_mario - o.move_yaw) as f64).abs() <= 0x2aaa as f64,
                    )
                };
                if blocked { continue; }
                if no_cooldown && mario_overlaps_object(m, &m.objects[id]) && in_cone && m.start_king_grab(id) {
                    let o = &mut m.objects[id];
                    o.set_var("grabbing", 1.0);
                    o.set_var("grabTimer", 40.0);
                    o.forward_vel = 0.0;
                    let yaw = o.move_yaw;
                    m.face_angle[1] = yaw;
                    continue;
                }
                if m.action == ACT_DIVE { continue; }
            }
            m.near_grabbable = true;

            if attack == ATTACK_KICK_OR_TRIP || attack == ATTACK_FAST_ATTACK || (reach != 0.0 && attack == ATTACK_PUNCH) {
                let o = &mut m.objects[id];
                attack_object(o, attack);
                o.interact_status |= INT_STATUS_MARIO_KNOCKBACK_DMG;
                continue;
            }

            if can_grab_action(m) {
                m.interact_obj = Some(id);
                m.interact_grabbable = true;
            } else {
                // The source uses -5, but our interaction pass runs before Mario's action
                // step; a tighter push keeps the object inside the grab cone on the
                // punch frame that actually checks for a grab.
                push_mario_out_of_object(m, id, -20.0);
            }
            continue;
        }

        if interact_type & INTERACT_BREAKABLE != 0 {
            m.near_breakable = true;
            if attack == 0 {
                push_mario_out_of_object(m, id, 0.0);
                continue;
            }
            attack_object(&mut m.objects[id], attack);
            if attack == ATTACK_FROM_ABOVE {
                let o = &m.objects[id];
                let y = o.pos[1] + o.hitbox.height - o.hitbox.down_offset;
                m.pos[1] = y;
                m.vel[1] = 30.0;
                m.set_action(ACT_FREEFALL, 0.0);
            }
        }
    }
}

/// `ObjectManager.explode`: radial blast from `source`, damaging Mario if close.
pub fn explode(m: &mut MarioRuntime, source: ObjId, radius: f64, damage: f64) {
    let (spos, mine) = { let s = &m.objects[source]; (s.pos, matches!(s.behavior, ObjectBehaviorKind::BowserMine | ObjectBehaviorKind::SmBowserBomb)) };
    m.objects.events.push(ObjectEvent::Explosion { pos: spos, mine, radius });

    for (i, object) in m.objects.objects.iter_mut().enumerate() {
        if i == source.idx() || !object.active { continue; }
        let distance = dist_3d(&object.pos, &spos);
        if distance > radius { continue; }
        if object.behavior == ObjectBehaviorKind::Bobomb {
            object.interact_status |= INT_STATUS_TOUCHED_BOB_OMB;
        } else if object.behavior == ObjectBehaviorKind::BreakableBox {
            object.interact_status |= INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED;
        }
    }

    let dx = m.pos[0] - spos[0];
    let dz = m.pos[2] - spos[2];
    let vertical = m.pos[1] > spos[1] - 160.0 && m.pos[1] < spos[1] + 260.0;
    if dx * dx + dz * dz > (radius + 37.0) * (radius + 37.0) || !vertical { return; }

    if m.take_damage(damage, Some(spos)) {
        let pos = m.pos;
        m.objects.events.push(ObjectEvent::Hurt { pos });
    }
}
