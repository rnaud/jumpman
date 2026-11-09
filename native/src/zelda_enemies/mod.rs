//! The Ocarina of Time enemies in the exported OoT courses (Bevy glue around the engine-free
//! `jumpman_sim::oot_enemy`). Placements come from `assets/levels/oot_enemies/<course>.json`
//! (written by `scripts/export-oot-enemies.py` from the scene/room actor lists), models from the
//! same folder. Everything here is additive: the plugin is inert on courses without a placement
//! file and costs nothing on SM64/MK64/BK courses.
//!
//! Per tick (`PostTickSet`, after the player's step): shield reflections, the field step (AI of
//! every enemy within the active radius), the player's blows (Link's swings via
//! `oot_enemy::player::strikes`, Mario's and Banjo's SM64 attack states, Banjo's eggs), Link's
//! projectile hits queued by the hook in `link::post_tick`, then the events: player damage,
//! deaths and drops. In `PreTickSet` the lockable enemies are appended to Link's Z-focus list.
use crate::audio::{oot_sfx, SfxCue};
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::objects::goombas::interpolate_pose;
use crate::progress::CourseScoped;
use crate::render::object_view::{spawn_model, AnimRig};
use crate::render::{to_bevy, AssetRoot};
use crate::sim_plugin::{PostTickSet, PreTickSet};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use jumpman_sim::link::combat::Dmg;
use jumpman_sim::link::items::{Ammo, Age, HEART};
use jumpman_sim::link::projectiles::{Target, TargetKind};
use jumpman_sim::math::Vec3 as V3;
use jumpman_sim::oot_enemy::player::{apply_strikes, hurt_player, reflect_with_shield, strikes};
use jumpman_sim::oot_enemy::{Ev, Field, Item, Kind, Needs, Spawner, St, TARGET_BASE, TARGET_END, U};
use std::collections::HashMap;

pub struct ZeldaEnemiesPlugin;

impl Plugin for ZeldaEnemiesPlugin {
    fn build(&self, app: &mut App) {
        crate::objects::register_rebuild(app, rebuild);
        app.add_systems(Startup, setup.after(crate::render::course_view::spawn_course))
            .add_systems(FixedFirst, remember_previous)
            .add_systems(FixedUpdate, (publish_focus.in_set(PreTickSet).after(crate::objects::goombas::step_goombas), step_field.in_set(PostTickSet)))
            .add_systems(Update, (sync_views, sync_shots, sync_drops).before(crate::CameraSet));
    }
}

pub const MODEL_DIR: &str = "levels/oot_enemies";
/// Enemies farther than this (Jumpman units, ~2000 OoT units) are neither simulated nor drawn.
pub const ACTIVE_RADIUS: f64 = 9000.0;

#[derive(Clone, Debug)]
pub struct DropItem {
    pub id: u32,
    pub pos: V3,
    pub vel: V3,
    pub item: Item,
    pub qty: u8,
    pub age: f64,
    pub done: bool,
    /// Placed `En_Item00` (scene actor list): its collectible flag key; such an item floats where it is, never
    /// despawns and stays gone once taken (`Flags_SetCollectible`).
    pub key: Option<String>,
}

#[derive(Resource)]
pub struct OotEnemies {
    pub field: Field,
    pub drops: Vec<DropItem>,
    /// Link projectile hits waiting for the next field step: (enemy index, damage row, position).
    pub pending: Vec<(usize, Dmg, V3)>,
    /// Clip name -> index in each model's animation list.
    pub clips: HashMap<&'static str, Vec<String>>,
    pub tokens: u32,
    pub kills: u32,
    /// Collectible flag keys taken since the last drain (the app writes them into the save).
    pub taken: Vec<String>,
    next_drop: u32,
    /// A boss is down: seconds until the blue warp takes Link to `.1` (`Door_Warp1`).
    pub warp: Option<(f64, &'static str)>,
    prev: Vec<(V3, f64)>,
    pub course: String,
}

impl OotEnemies {
    /// Targets Link's projectiles can hit (ids `TARGET_BASE + index`).
    pub fn link_targets(&self) -> Vec<Target> {
        self.field
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.asleep && e.targetable())
            .map(|(i, e)| {
                let (c, r, h) = e.body();
                Target { id: TARGET_BASE + i as u32, pos: c, radius: r, height: h, kind: TargetKind::Enemy }
            })
            .collect()
    }

    /// A `LinkEvent::HitExtra` for one of ours.
    pub fn hit_extra(&mut self, id: u32, dmg: Dmg, pos: V3) -> bool {
        if !(TARGET_BASE..TARGET_END).contains(&id) {
            return false;
        }
        self.pending.push(((id - TARGET_BASE) as usize, dmg, pos));
        true
    }

    pub fn alive(&self) -> usize {
        self.field.enemies.iter().filter(|e| e.alive()).count()
    }

    /// A pickup tossed out of `at` (a broken pot, cut grass): it falls, waits and is collected like an enemy's drop.
    pub fn spawn_drop(&mut self, at: V3, item: Item, qty: u8) {
        for k in 0..qty.max(1) {
            let a = (self.next_drop as f64 * 2.4 + k as f64 * 1.7 + at[0]) % 6.28;
            self.next_drop += 1;
            self.drops.push(DropItem { id: self.next_drop, pos: [at[0], at[1] + 40.0, at[2]], vel: [a.sin() * 5.0, 22.0, a.cos() * 5.0], item, qty: 1, age: 0.0, done: false, key: None });
        }
    }
}

/// `{enemies, spawners, skipped}` of one course file.
#[derive(serde::Deserialize, Default)]
pub struct Placements {
    #[serde(default)]
    pub enemies: Vec<Placed>,
    #[serde(default)]
    pub spawners: Vec<PlacedSpawner>,
    #[serde(default)]
    pub skipped: HashMap<String, u32>,
    /// `En_Item00` placements (`scripts/export-oot-enemies.py`).
    #[serde(default)]
    pub collectibles: Vec<PlacedCollectible>,
    /// Doors, switches, blocks, chests...: every other actor of the scene (`scripts`-free Rust exporter).
    #[serde(default)]
    pub mechanisms: Vec<Mechanism>,
    /// The scene header's time / light settings (`oot_kankyo`).
    #[serde(default)]
    pub environment: jumpman_sim::oot_kankyo::Environment,
}

/// A non-enemy actor of the scene or its rooms (`actor` is the decomp name, `ACTOR_DOOR_SHUTTER`...).
#[derive(serde::Deserialize, Clone, Debug)]
pub struct Mechanism {
    pub actor: String,
    #[serde(default)]
    pub param: i32,
    pub pos: [f64; 3],
    #[serde(default)]
    pub rot: [f64; 3],
}

/// One `En_Item00` of a room's actor list: `ty` = `params & 0xFF` (`Item00Type`), `flag` = collectible flag.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct PlacedCollectible {
    #[serde(rename = "type")]
    pub ty: u8,
    pub flag: u8,
    pub pos: [f64; 3],
}

/// The save key of a placed collectible (`ZeldaSave::collected`).
pub fn collectible_key(course: &str, flag: u8) -> String {
    format!("item00:{course}:{flag}")
}

impl OotEnemies {
    /// Put the not-yet-taken placed collectibles of `p` in the world (`EnItem00_Init`: a taken flag kills the actor).
    pub fn place_collectibles(&mut self, p: &Placements, course: &str, taken: &dyn Fn(&str) -> bool, needs: &Needs, floor: impl Fn(V3) -> V3) {
        for c in &p.collectibles {
            let key = collectible_key(course, c.flag);
            if taken(&key) || self.drops.iter().any(|d| d.key.as_deref() == Some(key.as_str())) {
                continue;
            }
            let Some((item, qty)) = jumpman_sim::oot_enemy::placed_item(c.ty, needs) else { continue };
            // Hearts and keys sit on the floor at the placed height; the actor's own y is kept when it floats.
            let at = floor(c.pos);
            self.next_drop += 1;
            self.drops.push(DropItem { id: self.next_drop, pos: [at[0], at[1] + 30.0, at[2]], vel: [0.0; 3], item, qty, age: 0.0, done: false, key: Some(key) });
        }
    }
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct Placed {
    pub kind: String,
    pub pos: [f64; 3],
    pub yaw: f64,
    #[serde(default)]
    pub param: i32,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct PlacedSpawner {
    pub pos: [f64; 3],
    #[serde(rename = "spawnType")]
    pub spawn_type: u32,
    #[serde(rename = "maxLive")]
    pub max_live: u32,
    pub total: u32,
    pub range: f64,
}

pub fn placements_path(root: &std::path::Path, id: &str, area: i32) -> std::path::PathBuf {
    let name = if area > 1 { format!("{id}.{area}.json") } else { format!("{id}.json") };
    root.join(MODEL_DIR).join(name)
}

pub fn load_placements(root: &std::path::Path, id: &str, area: i32) -> Option<Placements> {
    serde_json::from_slice(&std::fs::read(placements_path(root, id, area)).ok()?).ok()
}

/// Build the engine-free field from placements: unknown kinds are dropped (the parse test lists them).
pub fn build_field(p: &Placements, seed: u64, floor: impl Fn(V3) -> V3) -> Field {
    let mut field = Field::new(seed);
    field.active_radius = ACTIVE_RADIUS;
    for e in &p.enemies {
        if let Some(kind) = Kind::from_name(&e.kind) {
            field.add(kind, floor(e.pos), e.yaw, e.param);
        }
    }
    for s in &p.spawners {
        // EnEncount1 spawner types: 0 leevers; the rest (tektites, stalchildren/wolfos) are not placed.
        if s.spawn_type == 0 {
            field.spawners.push(Spawner { kind: Kind::Leever, pos: s.pos, range: s.range, max_live: s.max_live.min(5) as usize, total: s.total as usize, spawned: 0, timer: 0.0, live: vec![] });
        }
    }
    field
}

#[derive(Component)]
struct EnemyView {
    index: usize,
    model: Entity,
    base: f32,
}

#[derive(Component)]
struct StemPiece {
    index: usize,
    piece: usize,
    base: f32,
}

#[derive(Component)]
struct ShotView(usize);

#[derive(Component)]
struct DropView(u32);

fn rebuild(world: &mut World) {
    world.remove_resource::<OotEnemies>();
    let _ = world.run_system_once(setup);
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, sim: Res<GameSim>, existing: Option<Res<OotEnemies>>, save: Option<Res<crate::zelda::save::ZeldaSave>>) {
    if existing.is_some() {
        return;
    }
    let req = &sim.course.request;
    let p = load_placements(&root.0, &req.id, req.area).unwrap_or_default();
    // Courses with breakable props (grass, pots...) need the drop pipeline even without enemies.
    let has_props = crate::game::course_data::CourseData::load(&root.0, &req.id, req.area).map(|c| !c.props.is_empty()).unwrap_or(false);
    if p.enemies.is_empty() && p.spawners.is_empty() && p.collectibles.is_empty() && !has_props {
        return;
    }
    let world = sim.mario.world.clone();
    let field = build_field(&p, 0x6f6f74, |pos| {
        // Ground walkers start on the collision floor; flyers and ceiling dwellers keep their placed height.
        let f = world.find_floor(pos[0], pos[1] + 80.0, pos[2]);
        if f.surface.is_some() && (pos[1] - f.height).abs() < 120.0 { [pos[0], f.height, pos[2]] } else { pos }
    });
    let mut clips: HashMap<&'static str, Vec<String>> = HashMap::new();
    for (i, e) in field.enemies.iter().enumerate() {
        let spec = e.spec();
        let rel = format!("{MODEL_DIR}/{}.glb", spec.model);
        let abs = root.0.join(&rel);
        clips.entry(spec.model).or_insert_with(|| crate::zelda::glb::animation_names(&abs));
        if !root.exists(&rel) {
            warn!("enemy model missing: {rel}");
            continue;
        }
        let base = crate::zelda::glb::bevy_scale(&abs) * spec.scale as f32;
        let parent = commands.spawn((CourseScoped, EnemyView { index: i, model: Entity::PLACEHOLDER, base }, Transform::from_translation(to_bevy(e.pos)), Visibility::Hidden)).id();
        let model = spawn_model(&mut commands, &assets, &root, &rel, true).insert(Transform::from_scale(Vec3::splat(base))).id();
        commands.entity(parent).add_child(model).insert(EnemyView { index: i, model, base });
        if e.kind.is_baba() {
            for piece in 0..3 {
                let rel = format!("{MODEL_DIR}/deku_baba_stem{piece}.glb");
                if root.exists(&rel) {
                    let s = crate::zelda::glb::bevy_scale(&root.0.join(&rel)) * spec.scale as f32;
                    let stem = spawn_model(&mut commands, &assets, &root, &rel, false).insert((CourseScoped, StemPiece { index: i, piece, base: s }, Transform::from_scale(Vec3::splat(s)), Visibility::Hidden)).id();
                    let _ = stem;
                }
            }
            let rel = format!("{MODEL_DIR}/deku_baba_baseleaves.glb");
            if root.exists(&rel) {
                let s = crate::zelda::glb::bevy_scale(&root.0.join(&rel)) * spec.scale as f32;
                spawn_model(&mut commands, &assets, &root, &rel, false).insert((CourseScoped, Transform::from_translation(to_bevy(e.home)).with_scale(Vec3::splat(s)), Visibility::Inherited));
            }
        }
    }
    let prev = field.enemies.iter().map(|e| (e.pos, e.yaw)).collect();
    let mut en = OotEnemies { field, drops: vec![], pending: vec![], clips, tokens: 0, kills: 0, taken: vec![], next_drop: 0, warp: None, prev, course: req.id.clone() };
    let needs = needs_of(&sim);
    let floor = |pos: V3| {
        let f = sim.mario.world.find_floor(pos[0], pos[1] + 80.0, pos[2]);
        if f.surface.is_some() && (pos[1] - f.height).abs() < 120.0 { [pos[0], f.height, pos[2]] } else { pos }
    };
    en.place_collectibles(&p, &req.id, &|k| save.as_ref().is_some_and(|s| s.collected.contains(k)), &needs, floor);
    commands.insert_resource(en);
    info!("oot enemies: {} placed, {} collectibles in {}", p.enemies.len(), p.collectibles.len(), req.id);
}

fn remember_previous(en: Option<ResMut<OotEnemies>>) {
    let Some(mut en) = en else { return };
    let prev: Vec<(V3, f64)> = en.field.enemies.iter().map(|e| (e.pos, e.yaw)).collect();
    en.prev = prev;
}

/// Link's Z-targeting list: append the lockable enemies after the goombas (indexes only matter
/// to `LinkFocus::point`, which reads the same vector).
fn publish_focus(mut sim: ResMut<GameSim>, en: Option<Res<OotEnemies>>, goombas: Res<crate::objects::goombas::Goombas>) {
    let Some(en) = en else { return };
    sim.focus_goombas.truncate(goombas.0.len());
    for e in &en.field.enemies {
        if !e.asleep && e.lockable() {
            let p = e.lock_point();
            let feet = e.hit_center();
            sim.focus_goombas.push(Some((feet, p[1])));
        } else {
            sim.focus_goombas.push(None);
        }
    }
}

pub fn needs_of(sim: &GameSim) -> Needs {
    let l = &sim.mario.link;
    if !l.enabled {
        return Needs::default();
    }
    Needs { link: true, adult: l.age == Age::Adult, health: l.inv.health, magic: l.inv.magic, magic_level: i32::from(l.inv.magic_cap > 0), seeds: l.inv.ammo(Ammo::Seed), arrows: l.inv.ammo(Ammo::Arrow), bombs: l.inv.ammo(Ammo::Bomb), rupees: l.inv.rupees }
}

/// Something the Bevy side should play or show after a tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    Cue(&'static str, Option<[f64; 3]>),
    /// An Ocarina of Time clip (`audio::oot_sfx`, name without the `NA_SE_` prefix).
    Oot(&'static str),
    /// A music jingle (`audio::oot_music::JINGLE_*`, an `NA_BGM_*` sequence id) over the scene music.
    Jingle(u8),
    Coin,
    Toast(String),
}

#[allow(clippy::too_many_arguments)]
fn step_field(mut start: MessageWriter<crate::shell::StartCourse>, time: Res<Time<Fixed>>, mut sim: ResMut<GameSim>, en: Option<ResMut<OotEnemies>>, mut toast: ResMut<Toast>, mut cues: MessageWriter<SfxCue>, mut save: Option<ResMut<crate::zelda::save::ZeldaSave>>, mut music: Option<ResMut<crate::zelda::music::OotMusic>>, root: Res<crate::render::AssetRoot>) {
    let Some(mut en) = en else { return };
    if let Some((left, to)) = en.warp {
        let left = left - time.delta_secs_f64();
        if left <= 0.0 {
            en.warp = None;
            start.write(crate::shell::StartCourse(crate::game::world::CourseRequest::new(to)));
        } else {
            en.warp = Some((left, to));
        }
    }
    if let Some(save) = save.as_mut() {
        for key in en.taken.drain(..) {
            save.collected.insert(key);
        }
    }
    for out in tick(&mut sim, &mut en) {
        match out {
            Out::Cue(kind, pos) => {
                cues.write(SfxCue::Effect { kind, pos });
            }
            Out::Oot(name) => {
                crate::audio::oot_sfx::play(name);
            }
            Out::Jingle(seq) => {
                if let Some(m) = music.as_mut() {
                    let now = m.now;
                    m.jingle(&root.0, now, seq);
                }
            }
            Out::Coin => {
                cues.write(SfxCue::Coin { underwater: false });
            }
            Out::Toast(text) => toast.show(text, 2.0),
        }
    }
}

/// One 30 Hz tick of the enemy layer, run after the player's step. Engine-free apart from the
/// `GameSim` it reads and writes (the scenario tests drive it directly).
pub fn tick(sim: &mut GameSim, en: &mut OotEnemies) -> Vec<Out> {
    let mut outs = vec![];
    let world = sim.mario.world.clone();
    let player = sim.mario.pos;
    reflect_with_shield(&sim.mario, &mut en.field.shots);
    en.field.step(player, &world);
    // The player's blows (this tick's attack flags and hit windows).
    let blows = strikes(&mut sim.mario, &en.field);
    let link = sim.mario.link.enabled;
    for (s, r) in apply_strikes(&mut sim.mario, &mut en.field, &blows) {
        // Link's blows and every enemy's cry are the OoT clips (below); Mario and Banjo keep the SM64 hit sound.
        if r.applied && !link {
            outs.push(Out::Cue("stomp", Some(s.from)));
        }
    }
    for (i, dmg, from) in std::mem::take(&mut en.pending) {
        en.field.hit(i, dmg, from);
    }
    let needs = needs_of(sim);
    let events = std::mem::take(&mut en.field.events);
    for ev in &events {
        match ev {
            Ev::HurtPlayer { units, from, .. } => {
                if hurt_player(&mut sim.mario, *units, *from) && link {
                    outs.push(Out::Oot("PL_DAMAGE"));
                }
            }
            Ev::Clank { pos } => outs.push(if link { Out::Oot(oot_sfx::CLANK) } else { Out::Cue("bonk", Some(*pos)) }),
            Ev::Spark { pos, .. } => {
                if !link {
                    outs.push(Out::Cue("bonk", Some(*pos)));
                }
            }
            Ev::Hurt { kind, dmg, pos } => {
                // The weapon's sound (Link only), then the enemy's own cry unless this blow killed it.
                if link {
                    if let Some(name) = blow_sound(*dmg) {
                        outs.push(Out::Oot(name));
                    }
                }
                let killed = events.iter().any(|e| matches!(e, Ev::Died { kind: k, pos: p, .. } if k == kind && (p[0] - pos[0]).hypot(p[2] - pos[2]) < 200.0));
                if !killed {
                    outs.push(Out::Oot(oot_sfx::enemy_hurt(*kind)));
                }
            }
            Ev::Died { kind, pos, .. } => {
                en.kills += 1;
                if *kind == jumpman_sim::oot_enemy::Kind::KingDodongo {
                    outs.push(Out::Jingle(crate::audio::oot_music::JINGLE_BOSS_CLEAR));
                    outs.push(Out::Toast("KING DODONGO DEFEATED".into()));
                    en.warp = Some((7.0, "death_mountain_trail"));
                }
                // `Actor_Kill` puff and the enemy's death cry, whoever delivered the blow.
                outs.push(Out::Oot(oot_sfx::enemy_dead(*kind)));
                outs.push(Out::Oot(oot_sfx::EXTINCT));
                for (at, item, qty) in en.field.drops_for(*kind, *pos, &needs) {
                    for k in 0..qty.max(1) {
                        let a = (en.kills as f64 * 2.4 + k as f64 * 1.7 + at[0]) % 6.28;
                        en.next_drop += 1;
                        en.drops.push(DropItem { id: en.next_drop, pos: [at[0], at[1] + 40.0, at[2]], vel: [a.sin() * 8.0, 26.0, a.cos() * 8.0], item, qty: 1, age: 0.0, done: false, key: None });
                    }
                }
            }
            Ev::Sfx { name, .. } => match *name {
                "deku_attack" => outs.push(Out::Oot("EN_DEKU_ATTACK")),
                "wolfos_howl" => outs.push(Out::Oot("EN_WOLFOS_ATTACK")),
                _ => {}
            },
            Ev::Spit { kind, .. } => outs.push(Out::Oot(if *kind == jumpman_sim::oot_enemy::ShotKind::Rock { "EN_OCTAROCK_ROCK" } else { "EN_NUTS_THROW" })),
            Ev::Drop { .. } => {}
        }
    }
    en.field.events = events;
    step_drops(sim, en, &mut outs);
    outs
}

/// What a landed blow sounds like (`NA_SE_IT_*`): blades strike, the hammer thuds, arrows stick in the creature.
fn blow_sound(dmg: jumpman_sim::link::combat::Dmg) -> Option<&'static str> {
    use jumpman_sim::link::combat::Dmg;
    match dmg {
        d if d.is_sword() => Some(oot_sfx::HIT_BLADE),
        Dmg::Stick => Some("IT_WALL_HIT_SOFT"),
        Dmg::HammerSwing | Dmg::HammerJump => Some(oot_sfx::HIT_BLUNT),
        Dmg::Arrow | Dmg::FireArrow | Dmg::IceArrow | Dmg::LightArrow => Some("IT_ARROW_STICK_CRE"),
        Dmg::Slingshot | Dmg::Nut | Dmg::Boomerang | Dmg::Hookshot => Some("IT_SWORD_IMPACT"),
        Dmg::Explosive => None,
        _ => Some("IT_SWORD_IMPACT"),
    }
}

fn rupee_value(item: Item) -> i32 {
    match item {
        Item::RupeeGreen => 1,
        Item::RupeeBlue => 5,
        Item::RupeeRed => 20,
        Item::RupeePurple => 50,
        Item::RupeeOrange => 100,
        _ => 0,
    }
}

fn step_drops(sim: &mut GameSim, en: &mut OotEnemies, outs: &mut Vec<Out>) {
    let world = sim.mario.world.clone();
    let p = sim.mario.pos;
    for d in &mut en.drops {
        d.age += 1.0;
        if d.key.is_some() {
            // A placed collectible hangs where the room put it.
            let dist = ((d.pos[0] - p[0]).powi(2) + (d.pos[2] - p[2]).powi(2)).sqrt();
            if dist < 80.0 && d.pos[1] > p[1] - 60.0 && d.pos[1] < p[1] + 190.0 {
                d.done = true;
                en.taken.extend(d.key.clone());
                collect(sim, d.item, d.qty, &mut en.tokens, outs);
            }
            continue;
        }
        d.vel[1] -= 2.0;
        d.pos = [d.pos[0] + d.vel[0], d.pos[1] + d.vel[1], d.pos[2] + d.vel[2]];
        let f = world.find_floor(d.pos[0], d.pos[1] + 60.0, d.pos[2]);
        if f.surface.is_some() && d.pos[1] < f.height + 20.0 {
            d.pos[1] = f.height + 20.0;
            d.vel = [d.vel[0] * 0.3, 0.0, d.vel[2] * 0.3];
        }
        // despawnTimer 220 OoT frames.
        if d.age > 330.0 {
            d.done = true;
            continue;
        }
        let dist = ((d.pos[0] - p[0]).powi(2) + (d.pos[2] - p[2]).powi(2)).sqrt();
        if d.age > 12.0 && dist < 80.0 && d.pos[1] > p[1] - 40.0 && d.pos[1] < p[1] + 190.0 {
            d.done = true;
            collect(sim, d.item, d.qty, &mut en.tokens, outs);
        }
    }
    en.drops.retain(|d| !d.done);
}

fn collect(sim: &mut GameSim, item: Item, qty: u8, tokens: &mut u32, outs: &mut Vec<Out>) {
    let rupees = rupee_value(item);
    if sim.mario.link.enabled {
        let inv = &mut sim.mario.link.inv;
        let ammo = |inv: &mut jumpman_sim::link::items::Inventory, a: Ammo, n: i32| {
            let cap = inv.upgrades.cap(a);
            let cur = inv.ammo(a);
            if cap > 0 {
                inv.set_ammo(a, (cur + n * qty as i32).min(cap));
            }
        };
        match item {
            Item::Heart => inv.heal(HEART),
            Item::Fairy => inv.heal(3 * HEART),
            Item::Bombs => ammo(inv, Ammo::Bomb, 5),
            Item::Arrows(n) => ammo(inv, Ammo::Arrow, [5, 10, 30][(n as usize - 1).min(2)]),
            Item::ArrowsSingle => ammo(inv, Ammo::Arrow, 1),
            Item::Seeds => ammo(inv, Ammo::Seed, 5),
            Item::Nuts => ammo(inv, Ammo::Nut, 5),
            Item::Stick => ammo(inv, Ammo::Stick, 1),
            Item::MagicSmall => inv.add_magic(8),
            Item::MagicLarge => inv.add_magic(24),
            Item::Token => {
                *tokens += 1;
                let n = inv.give_skulltula();
                outs.push(Out::Toast(format!("GOLD SKULLTULA TOKEN {n}")));
                // The House of Skulltula is not in the game: the cursed people's reward is given on the spot.
                while let Some((m, what)) = inv.claim_skulltula_reward() {
                    outs.push(Out::Toast(format!("{m} TOKENS - THE CURSE LIFTED: {what}")));
                    outs.push(Out::Jingle(crate::audio::oot_music::JINGLE_ITEM_GET));
                }
            }
            Item::HeartPiece => {
                if inv.give_heart_piece() {
                    outs.push(Out::Toast("HEART CONTAINER".into()));
                } else {
                    outs.push(Out::Toast(format!("PIECE OF HEART ({}/4)", inv.heart_pieces)));
                }
            }
            Item::HeartContainer => {
                inv.give_heart_container();
                outs.push(Out::Toast("HEART CONTAINER".into()));
            }
            Item::SmallKey => inv.small_keys += 1,
            _ => inv.add_rupees(rupees),
        }
    } else {
        // Mario and Banjo spend coins and wedges: hearts heal a wedge, rupees are coins, the rest is a coin.
        match item {
            Item::Heart | Item::Fairy => {
                let m = &mut sim.mario;
                let add = if item == Item::Fairy { 0x300 } else { 0x100 };
                m.state.health = (m.state.health + add as f64).min(m.max_health);
            }
            // Mario and Banjo: a heart piece or container refills their health (they have no heart count to raise).
            Item::HeartPiece | Item::HeartContainer => {
                let m = &mut sim.mario;
                m.state.health = m.max_health;
                outs.push(Out::Toast("HEART".into()));
            }
            Item::SmallKey => {}
            Item::Token => {
                *tokens += 1;
                sim.coins += 5;
                outs.push(Out::Toast(format!("GOLD SKULLTULA TOKEN {}", tokens)));
            }
            _ => sim.coins += if rupees > 0 { (rupees as f64).sqrt().ceil() as u32 } else { 1 },
        }
    }
    if sim.mario.link.enabled {
        match item {
            Item::HeartPiece | Item::HeartContainer => outs.push(Out::Jingle(crate::audio::oot_music::JINGLE_HEART_GET)),
            Item::Token => outs.push(Out::Jingle(crate::audio::oot_music::JINGLE_SMALL_ITEM_GET)),
            _ => {}
        }
        outs.push(Out::Oot(match item {
            Item::Heart | Item::Fairy => "SY_HP_RECOVER",
            Item::Token | Item::HeartPiece | Item::HeartContainer | Item::SmallKey => "SY_GET_ITEM",
            _ if rupees > 0 => "SY_GET_RUPY",
            _ => "SY_GET_ITEM",
        }));
    } else {
        outs.push(Out::Coin);
    }
}

// ----------------------------------------------------------------------------------------------
// Views

fn clip_index(names: &[String], wanted: &str) -> usize {
    names.iter().position(|n| n == wanted).unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn sync_views(
    en: Option<Res<OotEnemies>>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    mut views: Query<(&EnemyView, &mut Transform, &mut Visibility), Without<StemPiece>>,
    mut rigs: Query<&mut AnimRig>,
    mut stems: Query<(&StemPiece, &mut Transform, &mut Visibility), Without<EnemyView>>,
) {
    let Some(en) = en else { return };
    let t = fixed.overstep_fraction() as f64;
    let player = sim.mario.pos;
    for (view, mut tf, mut vis) in &mut views {
        let Some(e) = en.field.enemies.get(view.index) else { continue };
        let near = ((e.pos[0] - player[0]).powi(2) + (e.pos[2] - player[2]).powi(2)).sqrt() < ACTIVE_RADIUS * 1.3;
        let cur = (e.pos, e.yaw);
        let (pos, yaw) = interpolate_pose(en.prev.get(view.index).copied().unwrap_or(cur), cur, t);
        let hide_head = e.kind.is_baba() && e.st == St::Hidden;
        *vis = if e.visible && near && e.st != St::Gone && !hide_head { Visibility::Inherited } else { Visibility::Hidden };
        if e.kind.is_baba() {
            // The head model hangs at the end of the stem; its pitch follows the stem slope.
            let head = to_bevy(e.head);
            let root = to_bevy(e.home);
            let dir = (head - root).normalize_or_zero();
            let pitch = dir.y.clamp(-1.0, 1.0).asin();
            tf.translation = head;
            tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(yaw) as f32) * Quat::from_rotation_x(-pitch * 0.6 + 0.35);
            let shrink = if e.st == St::Dying { e.reach as f32 } else { 1.0 };
            tf.scale = Vec3::splat(shrink);
        } else {
            tf.translation = to_bevy(pos);
            tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(yaw) as f32);
            let pulse = if e.flash > 0.0 { 1.0 + 0.08 * e.flash as f32 / 12.0 } else { 1.0 };
            let dying = if e.st == St::Dying { (1.0 - e.t as f32 / 36.0).clamp(0.05, 1.0) } else { 1.0 };
            tf.scale = Vec3::splat(pulse * dying);
        }
        if let Ok(mut rig) = rigs.get_mut(view.model) {
            let names = en.clips.get(e.spec().model).map(Vec::as_slice).unwrap_or(&[]);
            let want = clip_index(names, e.clip);
            rig.looped = !matches!(e.clip, "Death" | "Die");
            if rig.want != want {
                rig.want = want;
            }
            rig.speed = if e.stun > 0.0 || e.frozen > 0.0 { 0.0 } else { 1.0 };
        }
    }
    for (stem, mut tf, mut vis) in &mut stems {
        let Some(e) = en.field.enemies.get(stem.index) else { continue };
        if e.st == St::Hidden || e.st == St::Gone {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let root = to_bevy([e.home[0], e.home[1], e.home[2]]);
        let head = to_bevy(e.head);
        let len = (head - root).length().max(0.001);
        let bulge = Vec3::Y * len * 0.15 * (1.0 + e.reach as f32);
        let point = |k: f32| root.lerp(head, k) + bulge * (k * std::f32::consts::PI).sin();
        let a = point(stem.piece as f32 / 3.0);
        let b = point((stem.piece as f32 + 1.0) / 3.0);
        let dir = b - a;
        tf.translation = a;
        tf.look_to(-dir.normalize_or_zero(), Vec3::Y);
        // Each exported stem piece is 20 OoT units (88 model units) long; stretch it to span its third of the stem.
        let stretch = (dir.length() / (20.0 * U as f32 * stem.base)).clamp(0.05, 4.0);
        tf.scale = Vec3::new(stem.base, stem.base, stem.base * stretch);
    }
}

#[derive(Resource, Default)]
struct ShotAssets {
    mesh: Handle<Mesh>,
    rock: Handle<StandardMaterial>,
    nut: Handle<StandardMaterial>,
    heart: Handle<StandardMaterial>,
    piece: Handle<StandardMaterial>,
    green: Handle<StandardMaterial>,
    blue: Handle<StandardMaterial>,
    red: Handle<StandardMaterial>,
    other: Handle<StandardMaterial>,
}

fn sync_shots(
    mut commands: Commands,
    en: Option<Res<OotEnemies>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<Option<ShotAssets>>,
    mut views: Query<(Entity, &ShotView, &mut Transform, &mut Visibility), Without<DropView>>,
) {
    let Some(en) = en else {
        for (e, ..) in &views {
            commands.entity(e).despawn();
        }
        return;
    };
    let a = assets.get_or_insert_with(|| {
        let mut mat = |c: Color| materials.add(StandardMaterial { base_color: c, unlit: true, ..default() });
        ShotAssets {
            mesh: meshes.add(Sphere::new(1.0)),
            rock: mat(Color::srgb(0.45, 0.4, 0.35)),
            nut: mat(Color::srgb(0.85, 0.7, 0.25)),
            heart: mat(Color::srgb(0.95, 0.15, 0.2)),
            piece: mat(Color::srgb(1.0, 0.55, 0.7)),
            green: mat(Color::srgb(0.2, 0.9, 0.3)),
            blue: mat(Color::srgb(0.2, 0.4, 1.0)),
            red: mat(Color::srgb(1.0, 0.2, 0.15)),
            other: mat(Color::srgb(0.9, 0.9, 0.5)),
        }
    });
    let mut have = 0;
    for (e, v, mut tf, mut vis) in &mut views {
        if let Some(s) = en.field.shots.get(v.0) {
            tf.translation = to_bevy(s.pos);
            *vis = Visibility::Inherited;
            have = have.max(v.0 + 1);
        } else {
            commands.entity(e).despawn();
        }
    }
    for i in have..en.field.shots.len() {
        let s = &en.field.shots[i];
        let mat = if s.kind == jumpman_sim::oot_enemy::ShotKind::Rock { a.rock.clone() } else { a.nut.clone() };
        commands.spawn((CourseScoped, ShotView(i), Mesh3d(a.mesh.clone()), MeshMaterial3d(mat), Transform::from_translation(to_bevy(s.pos)).with_scale(Vec3::splat(0.14)), Visibility::Inherited));
    }
}

fn sync_drops(
    mut commands: Commands,
    en: Option<Res<OotEnemies>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<Option<ShotAssets>>,
    mut views: Query<(Entity, &DropView, &mut Transform), Without<ShotView>>,
    time: Res<Time>,
) {
    let Some(en) = en else {
        for (e, ..) in &views {
            commands.entity(e).despawn();
        }
        return;
    };
    let a = assets.get_or_insert_with(|| {
        let mut mat = |c: Color| materials.add(StandardMaterial { base_color: c, unlit: true, ..default() });
        ShotAssets {
            mesh: meshes.add(Sphere::new(1.0)),
            rock: mat(Color::srgb(0.45, 0.4, 0.35)),
            nut: mat(Color::srgb(0.85, 0.7, 0.25)),
            heart: mat(Color::srgb(0.95, 0.15, 0.2)),
            piece: mat(Color::srgb(1.0, 0.55, 0.7)),
            green: mat(Color::srgb(0.2, 0.9, 0.3)),
            blue: mat(Color::srgb(0.2, 0.4, 1.0)),
            red: mat(Color::srgb(1.0, 0.2, 0.15)),
            other: mat(Color::srgb(0.9, 0.9, 0.5)),
        }
    });
    let mut seen = std::collections::HashSet::new();
    for (e, v, mut tf) in &mut views {
        if let Some(d) = en.drops.iter().find(|d| d.id == v.0) {
            tf.translation = to_bevy(d.pos) + Vec3::Y * (time.elapsed_secs() * 3.0).sin() * 0.04;
            tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 2.0);
            seen.insert(d.id);
        } else {
            commands.entity(e).despawn();
        }
    }
    for d in en.drops.iter() {
        if seen.contains(&d.id) {
            continue;
        }
        let mat = match d.item {
            Item::Heart | Item::Fairy | Item::HeartContainer => a.heart.clone(),
            Item::HeartPiece => a.piece.clone(),
            Item::RupeeGreen => a.green.clone(),
            Item::RupeeBlue => a.blue.clone(),
            Item::RupeeRed | Item::RupeePurple | Item::RupeeOrange => a.red.clone(),
            _ => a.other.clone(),
        };
        commands.spawn((CourseScoped, DropView(d.id), Mesh3d(a.mesh.clone()), MeshMaterial3d(mat), Transform::from_translation(to_bevy(d.pos)).with_scale(Vec3::new(0.1, 0.16, 0.1)), Visibility::Inherited));
    }
}

#[cfg(test)]
mod tests;
