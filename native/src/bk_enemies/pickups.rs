//! The real placements of Banjo's collectibles in the BK worlds: musical notes, blue eggs, red and gold feathers,
//! empty honeycombs and Jiggies from the maps' setup files (`bkPickups` of `assets/courses/bk_*.json`, written by
//! `scripts/export-bk-setup.py`). They replace the sandbox scatter of `banjo_ctl` in these worlds.
//!
//! A world has up to ~240 of them, so they are streamed: only those within [`PICKUP_RANGE`] of Banjo live in
//! `BanjoExt::pickups` (which `banjo_ctl` draws and `banjo_ext::collect_pickups` collects), at most
//! [`MAX_LIVE_PICKUPS`] at a time, and a collected one never comes back.
use crate::game::course_data::CourseData;
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use bevy::prelude::*;
use jumpman_sim::banjo_ext::{BkPickup, ClimbPole, Molehill, NoteDoor, PickupKind, SwitchActor, SwitchDoor, Form, MumboPad, BreakDoor, Podium};
use std::collections::HashSet;

pub const PICKUP_RANGE: f64 = 2600.0;
pub const MAX_LIVE_PICKUPS: usize = 120;

pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickupField>().add_systems(Update, stream_pickups.before(crate::CameraSet));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Away,
    Live,
    Collected,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: PickupKind,
    pub pos: [f64; 3],
    state: State,
}

#[derive(Resource, Default)]
pub struct PickupField {
    course: String,
    pub items: Vec<Item>,
    timer: f32,
    progression_started: bool,
}

/// `[kind, x, y, z]` rows of `bkPickups` (kind order of `PickupKind`).
pub fn parse_pickups(json: &serde_json::Value) -> Vec<Item> {
    let Some(rows) = json.get("bkPickups").and_then(|v| v.as_array()) else { return vec![] };
    rows.iter()
        .filter_map(|r| {
            let r = r.as_array()?;
            let kind = match r.first()?.as_u64()? {
                0 => PickupKind::Note,
                1 => PickupKind::Egg,
                2 => PickupKind::RedFeather,
                3 => PickupKind::GoldFeather,
                4 => PickupKind::Honeycomb,
                6 => PickupKind::MumboToken,
                _ => PickupKind::Jiggy,
            };
            Some(Item { kind, pos: [r.get(1)?.as_f64()?, r.get(2)?.as_f64()?, r.get(3)?.as_f64()?], state: State::Away })
        })
        .collect()
}

/// The other setup actors of a map: climbable poles (`bkClimbs`), Bottles' molehills (`bkMoles`) and the lair's note doors
/// (`bkNoteDoors`), as written by `scripts/export-bk-setup.py`.
#[derive(Default, Debug, Clone)]
pub struct Features {
    pub climbs: Vec<ClimbPole>,
    pub moles: Vec<Molehill>,
    pub doors: Vec<NoteDoor>,
    pub switches: Vec<SwitchActor>,
    pub switch_doors: Vec<SwitchDoor>,
    pub mumbo: Vec<MumboPad>,
    pub break_doors: Vec<BreakDoor>,
    pub podiums: Vec<Podium>,
}

pub fn parse_features(json: &serde_json::Value) -> Features {
    let rows = |key: &str| json.get(key).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let num = |r: &serde_json::Value, i: usize| r.get(i).and_then(|v| v.as_f64());
    let mut f = Features::default();
    for r in rows("bkClimbs") {
        if let (Some(bx), Some(by), Some(bz), Some(tx), Some(ty), Some(tz), Some(radius), Some(kind)) =
            (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3), num(&r, 4), num(&r, 5), num(&r, 6), num(&r, 7))
        {
            f.climbs.push(ClimbPole { bottom: [bx, by, bz], top: [tx, ty, tz], radius, kind: kind as u8 });
        }
    }
    for r in rows("bkMoles") {
        if let (Some(x), Some(y), Some(z), Some(id), Some(sm)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3), num(&r, 4)) {
            f.moles.push(Molehill { pos: [x, y, z], id: id as u32, sm: sm != 0.0 });
        }
    }
    for r in rows("bkNoteDoors") {
        if let (Some(x), Some(y), Some(z), Some(yaw), Some(index)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3), num(&r, 4)) {
            f.doors.push(NoteDoor { pos: [x, y, z], yaw, index: index as u32 });
        }
    }
    for r in rows("bkPodiums") {
        if let (Some(x), Some(y), Some(z), Some(pic)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 4)) {
            f.podiums.push(Podium { pos: [x, y, z], picture: pic as u32 });
        }
    }
    for r in rows("bkBreakDoors") {
        if let (Some(x), Some(y), Some(z), Some(yaw), Some(id)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3), num(&r, 4)) {
            f.break_doors.push(BreakDoor { pos: [x, y, z], yaw, id: id as u32 });
        }
    }
    for r in rows("bkMumbo") {
        if let (Some(x), Some(y), Some(z), Some(form)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3)) {
            f.mumbo.push(MumboPad { pos: [x, y, z], form: Form::from_id(form as u32) });
        }
    }
    for r in rows("bkSwitches") {
        if let (Some(x), Some(y), Some(z), Some(flag)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3)) {
            f.switches.push(SwitchActor { pos: [x, y, z], flag: flag as u32 });
        }
    }
    for r in rows("bkSwitchDoors") {
        if let (Some(x), Some(y), Some(z), Some(yaw), Some(flag)) = (num(&r, 0), num(&r, 1), num(&r, 2), num(&r, 3), num(&r, 4)) {
            f.switch_doors.push(SwitchDoor { pos: [x, y, z], yaw, flag: flag as u32 });
        }
    }
    f
}

pub fn load_features(root: &std::path::Path, course: &str) -> Features {
    if !course.starts_with("bk_") {
        return Features::default();
    }
    let Ok(text) = std::fs::read_to_string(CourseData::path(root, course, 1)) else { return Features::default() };
    serde_json::from_str::<serde_json::Value>(&text).map(|j| parse_features(&j)).unwrap_or_default()
}

pub fn load_items(root: &std::path::Path, course: &str) -> Vec<Item> {
    if !course.starts_with("bk_") {
        return vec![];
    }
    let Ok(text) = std::fs::read_to_string(CourseData::path(root, course, 1)) else { return vec![] };
    serde_json::from_str::<serde_json::Value>(&text).map(|j| parse_pickups(&j)).unwrap_or_default()
}

fn key(p: &[f64; 3]) -> (u64, u64, u64) {
    (p[0].to_bits(), p[1].to_bits(), p[2].to_bits())
}

/// One streaming pass around `focus`: returns the pickups to add to the sim list and updates the item states.
/// `current` are the pickups the sim still holds (a live item that vanished from it was collected).
pub fn stream(items: &mut [Item], current: &[BkPickup], focus: &[f64; 3]) -> (Vec<BkPickup>, Vec<[f64; 3]>) {
    let held: HashSet<(u64, u64, u64)> = current.iter().map(|p| key(&p.pos)).collect();
    let mut live = 0;
    let mut remove = vec![];
    for it in items.iter_mut() {
        if it.state == State::Live {
            if !held.contains(&key(&it.pos)) {
                it.state = State::Collected;
            } else if (it.pos[0] - focus[0]).hypot(it.pos[2] - focus[2]) > PICKUP_RANGE * 1.5 {
                it.state = State::Away;
                remove.push(it.pos);
            } else {
                live += 1;
            }
        }
    }
    let mut add = vec![];
    let mut near: Vec<(usize, f64)> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.state == State::Away)
        .map(|(i, it)| (i, (it.pos[0] - focus[0]).hypot(it.pos[2] - focus[2])))
        .filter(|(_, d)| *d < PICKUP_RANGE)
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (i, _) in near {
        if live >= MAX_LIVE_PICKUPS {
            break;
        }
        items[i].state = State::Live;
        live += 1;
        add.push(BkPickup { kind: items[i].kind, pos: items[i].pos, collected: false });
    }
    // never leave the list empty while some remain, or `banjo_ctl` would scatter its sandbox set
    if live == 0 && add.is_empty() && !items.iter().any(|it| it.state == State::Live) {
        let nearest = items.iter().enumerate().filter(|(_, it)| it.state == State::Away).min_by(|a, b| {
            let d = |it: &Item| (it.pos[0] - focus[0]).hypot(it.pos[2] - focus[2]);
            d(a.1).total_cmp(&d(b.1))
        });
        if let Some((i, _)) = nearest {
            items[i].state = State::Live;
            add.push(BkPickup { kind: items[i].kind, pos: items[i].pos, collected: false });
        }
    }
    (add, remove)
}

fn stream_pickups(time: Res<Time>, root: Res<AssetRoot>, mut sim: ResMut<GameSim>, mut field: ResMut<PickupField>) {
    if !sim.as_banjo {
        return;
    }
    let id = sim.course.request.id.clone();
    if field.course != id {
        field.course = id.clone();
        field.items = load_items(&root.0, &id);
        field.timer = 0.0;
        let features = load_features(&root.0, &id);
        let b = &mut sim.mario.banjo;
        b.climbs = features.climbs;
        // a spawn point inside the stand-in trunk of an unported tree (Mumbo's Mountain's default spawn) steps out of it
        if let Some(p) = crate::banjo_ctl::clear_of_trunks(&sim.mario.world, &sim.mario.banjo.climbs, sim.mario.pos) {
            sim.mario.pos = p;
        }
        let b = &mut sim.mario.banjo;
        b.moles = features.moles;
        b.door_actors = features.doors;
        b.door_side.clear();
        b.podiums = features.podiums;
        b.break_doors = features.break_doors;
        b.break_side.clear();
        b.mumbo_pads = features.mumbo;
        b.mumbo_armed = true;
        // test hook: `JUMPMAN_BANJO_FORM=termite|croc|walrus|pumpkin|bee` starts in that transformation
        if let Ok(f) = std::env::var("JUMPMAN_BANJO_FORM") {
            if let Some(form) = jumpman_sim::banjo_ext::ALL_FORMS.iter().find(|x| x.name().eq_ignore_ascii_case(&f) || (f == "croc" && **x == Form::Croc)) {
                b.form = *form;
            }
        }
        b.switch_actors = features.switches;
        b.switch_doors = features.switch_doors;
        b.switch_door_side.clear();
        // `JUMPMAN_BK_PROGRESSION=1`: a real game, with the moves to learn and the locks closed (first BK map only).
        if std::env::var_os("JUMPMAN_BK_PROGRESSION").is_some() && !field.progression_started {
            field.progression_started = true;
            b.begin_progression();
        }
        if !field.items.is_empty() {
            // the sandbox scatter of `banjo_ctl` stays out of the real worlds
            sim.mario.banjo.pickups.clear();
        }
    }
    if field.items.is_empty() {
        return;
    }
    field.timer -= time.delta_secs();
    if field.timer > 0.0 {
        return;
    }
    field.timer = 0.25;
    let focus = sim.mario.pos;
    let current = sim.mario.banjo.pickups.clone();
    let (add, remove) = stream(&mut field.items, &current, &focus);
    if !remove.is_empty() {
        let gone: HashSet<(u64, u64, u64)> = remove.iter().map(key).collect();
        sim.mario.banjo.pickups.retain(|p| !gone.contains(&key(&p.pos)));
    }
    sim.mario.banjo.pickups.extend(add);
}
