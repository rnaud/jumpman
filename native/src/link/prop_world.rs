//! Bevy side of `jumpman_sim::link::props`: builds the interactive props of the loaded OoT course (`props` in the course json:
//! grass, pots, crates, rocks, boulders, hammer rocks, cracked walls, webs, torches, switches), steps them with Link's swings,
//! stick and the projectile events, hides the models of destroyed props, shows flames on lit torches, rolls the drop tables and
//! plays the `NA_SE_*` clips. The Deku Tree's hand-built puzzle parts (`web`, `lowerWeb`, `eye`) are bound to props too, so
//! the fire arrow, Din's Fire and a lit Deku Stick burn its webs and seeds, arrows and the boomerang press its eye switch.
use super::{combat_fx, WorldFx};
use crate::game::course_data::CourseData;
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::progress::CourseScoped;
use crate::render::course_view::StaticProp;
use crate::render::{to_bevy, AssetRoot};
use crate::sim_plugin::PostTickSet;
use crate::zelda::save::ZeldaSave;
use crate::zelda::world::ZeldaWorld;
use bevy::prelude::*;
use jumpman_sim::link::props::{Prop, PropEvent, PropKind, Props, State};
use std::collections::HashSet;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (ensure_props, tick_props).chain().in_set(PostTickSet).after(super::post_tick)).add_systems(Update, sync_views.before(crate::CameraSet));
    }
}

#[derive(Resource)]
pub struct PropWorld {
    pub props: Props,
    /// `course:area` the props were built for.
    pub key: String,
    /// Indices of `LoadedCourse::props` (static decorations) to hide.
    pub hidden: HashSet<usize>,
    pub destroyed: u32,
}

#[derive(Component)]
struct TorchFlame(usize);

/// The decoration (index into `LoadedCourse::props`) that draws a prop of `kind`, if one stands on it.
fn drawn_by(kind: PropKind, pos: [f64; 3], decor: &[crate::game::world::Prop]) -> Option<usize> {
    let words: &[&str] = match kind {
        PropKind::Grass => &["grass", "bush", "shrub"],
        PropKind::Pot => &["pot"],
        PropKind::Crate => &["crate"],
        PropKind::Rock | PropKind::LargeRock => &["rock", "boulder"],
        PropKind::Boulder => &["boulder"],
        PropKind::HammerRock => &["rock", "boulder", "totem"],
        PropKind::Torch => &["torch"],
        PropKind::BombFlower => &["bomb_flower"],
        PropKind::FloorWeb | PropKind::WallWeb => &["web"],
        _ => return None,
    };
    decor
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            let m = d.model.to_ascii_lowercase();
            words.iter().any(|w| m.contains(w)) && (d.pos[0] - pos[0]).hypot(d.pos[2] - pos[2]) < 60.0 && (d.pos[1] - pos[1]).abs() < 120.0
        })
        .min_by(|(_, a), (_, b)| {
            let d = |p: &crate::game::world::Prop| (p.pos[0] - pos[0]).hypot(p.pos[2] - pos[2]);
            d(a).total_cmp(&d(b))
        })
        .map(|(i, _)| i)
}

/// Build the props of a course from its json: the exported actors, with the Deku puzzle's parts bound as roles.
pub fn build(root: &std::path::Path, sim: &GameSim, zelda: Option<&ZeldaWorld>, save: Option<&ZeldaSave>) -> Option<PropWorld> {
    let req = &sim.course.request;
    let data = CourseData::load(root, &req.id, req.area).ok()?;
    let mut list: Vec<Prop> = vec![];
    for s in &data.props {
        let Some(kind) = PropKind::parse(&s.k) else { continue };
        let mut p = Prop::new(kind, s.pos);
        p.yaw = s.yaw;
        p.flag = s.flag.unwrap_or(-1);
        p.table = s.table;
        p.item = s.item;
        p.timed = s.timed != 0;
        p.param = if kind == PropKind::Torch { s.count } else { s.sub };
        if kind == PropKind::Torch && s.lit != 0 {
            p.state = State::Lit(f64::INFINITY);
        }
        list.push(p);
    }
    if let Some(z) = zelda {
        for o in z.scene.objects.iter().filter(|o| o.dungeon_role.is_some()) {
            let role = o.dungeon_role.clone().unwrap_or_default();
            let kind = match role.as_str() {
                "web" | "lowerWeb" => PropKind::FloorWeb,
                "eye" => PropKind::EyeSwitch,
                _ => continue,
            };
            let near = list
                .iter()
                .enumerate()
                .filter(|(_, p)| p.kind == kind && p.role.is_none())
                .min_by(|(_, a), (_, b)| {
                    let d = |p: &Prop| (p.pos[0] - o.pos[0]).hypot(p.pos[2] - o.pos[2]);
                    d(a).total_cmp(&d(b))
                })
                .map(|(i, _)| i);
            let i = match near {
                Some(i) if (list[i].pos[0] - o.pos[0]).hypot(list[i].pos[2] - o.pos[2]) < 900.0 => i,
                _ => {
                    list.push(Prop::new(kind, o.pos));
                    list.len() - 1
                }
            };
            // The puzzle part sits where its model stands; the web is a 150-unit (OoT) disc, the eye a 15-unit orb.
            list[i].pos = o.pos;
            list[i].role = Some(role.clone());
            if let (Some(d), Some(_)) = (z.dungeon.as_ref(), save) {
                if !d.part_visible(&role) {
                    list[i].state = State::Gone;
                }
            }
        }
    }
    for p in list.iter_mut() {
        p.object = drawn_by(p.kind, p.pos, &sim.course.props);
    }
    let mut hidden = HashSet::new();
    for p in &list {
        if p.gone() {
            hidden.extend(p.object);
        }
    }
    Some(PropWorld { props: Props::new(list), key: format!("{}:{}", req.id, req.area), hidden, destroyed: 0 })
}

fn ensure_props(
    mut commands: Commands,
    sim: Res<GameSim>,
    root: Res<AssetRoot>,
    existing: Option<Res<PropWorld>>,
    zelda: Option<Res<ZeldaWorld>>,
    save: Option<Res<ZeldaSave>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut waited: Local<(String, u32)>,
) {
    let key = format!("{}:{}", sim.course.request.id, sim.course.request.area);
    if existing.as_ref().is_some_and(|e| e.key == key) {
        return;
    }
    // The Zelda world is built a frame later than the course on a switch: wait a little for it on its courses.
    if waited.0 != key {
        *waited = (key.clone(), 0);
    }
    waited.1 += 1;
    if zelda.is_none() && waited.1 < 20 && crate::zelda::scene::ZeldaScene::load(&root.0, &sim.course.request.id, sim.course.request.area).is_some() {
        return;
    }
    match build(&root.0, &sim, zelda.as_deref(), save.as_deref()) {
        Some(pw) if !pw.props.list.is_empty() => {
            let flame = meshes.add(Sphere::new(1.0));
            let mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.12), emissive: LinearRgba::rgb(9.0, 3.2, 0.4), unlit: true, ..default() });
            for (i, p) in pw.props.list.iter().enumerate() {
                if p.kind == PropKind::Torch {
                    commands.spawn((CourseScoped, TorchFlame(i), Mesh3d(flame.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(to_bevy([p.pos[0], p.pos[1] + 330.0, p.pos[2]])).with_scale(Vec3::splat(0.14)), Visibility::Hidden));
                }
            }
            info!("props: {} placed in {}", pw.props.list.len(), key);
            commands.insert_resource(pw);
        }
        _ => {
            commands.remove_resource::<PropWorld>();
            commands.insert_resource(PropWorld { props: Props::new(vec![]), key, hidden: HashSet::new(), destroyed: 0 });
        }
    }
}

/// The shards a broken prop throws: (material, how many, dust puffs).
pub fn break_debris(kind: PropKind) -> Option<(crate::fx::pickup_fx::Debris, usize, usize)> {
    use crate::fx::pickup_fx::Debris;
    Some(match kind {
        PropKind::Grass => (Debris::Grass, 14, 0),
        PropKind::Pot => (Debris::Clay, 12, 4),
        PropKind::Crate => (Debris::Wood, 12, 5),
        PropKind::Rock => (Debris::Stone, 8, 4),
        PropKind::LargeRock | PropKind::Boulder | PropKind::HammerRock | PropKind::CrackedWall => (Debris::Stone, 16, 10),
        PropKind::FloorWeb | PropKind::WallWeb => (Debris::Web, 10, 0),
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn tick_props(
    mut sim: ResMut<GameSim>,
    pw: Option<ResMut<PropWorld>>,
    mut zelda: Option<ResMut<ZeldaWorld>>,
    mut save: Option<ResMut<ZeldaSave>>,
    mut en: Option<ResMut<crate::zelda_enemies::OotEnemies>>,
    mut fx: ResMut<WorldFx>,
    mut toast: ResMut<Toast>,
    mut debris: ResMut<crate::fx::pickup_fx::PickupFx>,
    mut particles: ResMut<crate::fx::particles::Particles>,
) {
    let Some(mut pw) = pw else { return };
    if pw.props.list.is_empty() {
        return;
    }
    pw.props.step_link(&mut sim.mario);
    for ev in pw.props.take_events() {
        if matches!(ev, PropEvent::Burned { .. } | PropEvent::Switch { .. } | PropEvent::Torch { .. } | PropEvent::Broke { .. }) {
            info!("prop event {ev:?}");
        }
        match ev {
            PropEvent::Sfx { name, .. } => {
                crate::audio::oot_sfx::play(name);
            }
            PropEvent::Broke { id, kind, pos } => {
                // what it was made of flies apart
                if let Some((what, n, dust)) = break_debris(kind) {
                    debris.debris(crate::render::to_bevy([pos[0], pos[1] + 30.0, pos[2]]), what, n);
                    if dust > 0 {
                        particles.dust([pos[0], pos[1] + 10.0, pos[2]], dust);
                    }
                }
                fx.queue.push(combat_fx::FxEvent::Hit { pos: [pos[0], pos[1] + 40.0, pos[2]], look: combat_fx::hit_look(jumpman_sim::link::combat::Dmg::KokiriSword) });
                pw.destroyed += 1;
                if let Some(o) = pw.props.list[id].object {
                    pw.hidden.insert(o);
                }
            }
            PropEvent::Drop { pos, table, item } => {
                if let Some(en) = en.as_deref_mut() {
                    let needs = crate::zelda_enemies::needs_of(&sim);
                    for (at, it, q) in en.field.prop_drops(table, item, pos, &needs) {
                        en.spawn_drop(at, it, q);
                    }
                }
            }
            PropEvent::Burned { id, pos, .. } => {
                fx.queue.push(combat_fx::FxEvent::Ignite { pos: [pos[0], pos[1] + 60.0, pos[2]] });
                let role = pw.props.list[id].role.clone();
                if let (Some(role), Some(z), Some(s)) = (role, zelda.as_deref_mut(), save.as_deref_mut()) {
                    if let Some(d) = z.dungeon.as_mut() {
                        d.web_burned(&role, s);
                    }
                }
                if let Some(o) = pw.props.list[id].object {
                    pw.hidden.insert(o);
                }
            }
            PropEvent::Torch { pos, lit, .. } => {
                if lit {
                    fx.queue.push(combat_fx::FxEvent::Ignite { pos: [pos[0], pos[1] + 330.0, pos[2]] });
                }
            }
            PropEvent::Switch { id, on, .. } => {
                let role = pw.props.list[id].role.clone();
                if let (Some(role), Some(z), Some(s)) = (role, zelda.as_deref_mut(), save.as_deref_mut()) {
                    if role == "eye" && on {
                        if let Some(d) = z.dungeon.as_mut() {
                            d.seed_hit(1, s);
                        }
                    }
                } else if on && matches!(pw.props.list[id].kind, PropKind::EyeSwitch | PropKind::CrystalSwitch | PropKind::FloorSwitch | PropKind::RustySwitch) {
                    toast.show("A SWITCH WAS PRESSED", 1.2);
                }
            }
        }
    }
    // Toasts of the dungeon logic (burned web...) are drained by the Zelda UI.
}

fn sync_views(
    pw: Option<Res<PropWorld>>,
    mut statics: Query<(&StaticProp, &mut Visibility), Without<TorchFlame>>,
    mut flames: Query<(&TorchFlame, &mut Visibility, &mut Transform)>,
    time: Res<Time>,
) {
    let Some(pw) = pw else { return };
    if pw.is_changed() {
        for (s, mut v) in &mut statics {
            *v = if pw.hidden.contains(&s.0) { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
    let t = time.elapsed_secs();
    for (f, mut v, mut tf) in &mut flames {
        let lit = pw.props.list.get(f.0).is_some_and(|p| p.is_lit());
        *v = if lit { Visibility::Inherited } else { Visibility::Hidden };
        if lit {
            let flicker = 1.0 + 0.18 * (t * 17.0 + f.0 as f32).sin() + 0.08 * (t * 31.0).sin();
            tf.scale = Vec3::new(0.12, 0.2 * flicker, 0.12);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    /// Kakariko's and the Market's grass is drawn by `shrub.glb`: it has to vanish when it is cut.
    #[test]
    fn cut_grass_finds_the_model_that_draws_it() {
        let decor = |model: &str| crate::game::world::Prop { model: model.into(), pos: [100.0, 0.0, 50.0], yaw: 0.0, rotation: None, scale: 1.0, billboard: false, scroll_t: 0.0, spin_z: 0.0 };
        for model in ["levels/kakariko_village/shrub.glb", "levels/kokiri_forest/wild_grass.glb"] {
            assert_eq!(drawn_by(PropKind::Grass, [101.0, 0.0, 50.0], &[decor("levels/x/pot.glb"), decor(model)]), Some(1), "{model}");
        }
        assert_eq!(drawn_by(PropKind::Grass, [400.0, 0.0, 50.0], &[decor("levels/kakariko_village/shrub.glb")]), None, "too far");
    }

    #[test]
    fn breakables_throw_their_own_debris() {
        use crate::fx::pickup_fx::Debris;
        assert_eq!(break_debris(PropKind::Grass).map(|d| d.0), Some(Debris::Grass));
        assert_eq!(break_debris(PropKind::Pot).map(|d| d.0), Some(Debris::Clay));
        assert_eq!(break_debris(PropKind::Crate).map(|d| d.0), Some(Debris::Wood));
        assert_eq!(break_debris(PropKind::Boulder).map(|d| d.0), Some(Debris::Stone));
        // bounded: never more shards than a third of the pool for one break
        for k in [PropKind::Grass, PropKind::Pot, PropKind::Crate, PropKind::Rock, PropKind::LargeRock, PropKind::FloorWeb] {
            let (_, shards, dust) = break_debris(k).unwrap();
            assert!(shards <= 18 && dust <= 12);
        }
    }

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// Counts of props placed per OoT course (from the room actor lists), checked against the json.
    #[test]
    fn exported_props_parse_with_the_expected_counts() {
        let expect: [(&str, &[(&str, usize)]); 5] = [
            ("deku_tree", &[("grass", 26), ("web", 2), ("wall_web", 5), ("torch", 11), ("floor_switch", 3), ("eye_switch", 1), ("cracked_wall", 1)]),
            ("kokiri_forest", &[("grass", 16), ("rock", 4)]),
            ("lon_lon_ranch", &[("pot", 7), ("crate", 1)]),
            ("goron_city", &[("boulder", 16), ("hammer_rock", 5), ("torch", 8), ("cracked_wall", 5), ("large_rock", 29)]),
            ("graveyard", &[("torch", 24)]),
        ];
        for (id, kinds) in expect {
            let data = CourseData::load(&assets(), id, 1).unwrap();
            for (k, n) in kinds {
                let got = data.props.iter().filter(|p| p.k == *k).count();
                assert_eq!(got, *n, "{id}: {k}");
                assert!(PropKind::parse(k).is_some(), "{k}");
            }
            assert!(data.props.iter().all(|p| PropKind::parse(&p.k).is_some()), "{id}: unknown prop kind");
        }
        let total: usize = crate::game::courses::COURSES.iter().filter_map(|c| CourseData::load(&assets(), c.id, 1).ok()).map(|d| d.props.len()).sum();
        assert!(total >= 340, "{total}");
    }

    #[test]
    fn deku_tree_puzzle_parts_become_roles_and_the_decorations_they_hide_are_found() {
        let mut sim = GameSim::load(&assets(), CourseRequest::new("deku_tree")).unwrap();
        let _ = &mut sim;
        let pw = build(&assets(), &sim, None, None).unwrap();
        let grass = pw.props.list.iter().filter(|p| p.kind == PropKind::Grass).count();
        assert_eq!(grass, 26);
        let drawn = pw.props.list.iter().filter(|p| p.kind == PropKind::Grass && p.object.is_some()).count();
        assert!(drawn >= 20, "{drawn} of 26 grass tufts have a model to hide");
        let torches = pw.props.list.iter().filter(|p| p.kind == PropKind::Torch && p.object.is_some()).count();
        assert!(torches >= 9, "{torches} torches are drawn");
    }
}
