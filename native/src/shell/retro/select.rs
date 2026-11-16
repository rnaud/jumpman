//! The character select (fighting-game style): a roster grid of portrait tiles grouped by the game each character comes
//! from (SM64, OOT, BK, SMASH), and the highlighted character's real 3D model large on a pedestal under a spotlight, playing
//! its idle loop; READY makes it hop (a Smash fighter does its taunt, `Appeal`). The models are the real ones (Mario's,
//! Bowser's and the Smash fighters' glbs, Link / Young Link and Banjo from the user's imported data, posed through the same
//! animators the game uses), spawned the first time a character is highlighted and kept (hidden) while the screen is open:
//! [`manage_stage`] takes everything down when it closes. Characters whose model is missing and that are left out rather than
//! locked ([`Character::hidden_when_missing`]: Bowser, the Smash fighters) have no tile at all.
//!
//! The tiles are drawn, not rendered from ROM data: a group-coloured plate, the character's signature colour as a stripe and
//! its short name. Everything 3D is drawn by the menu's private camera (render layer [`super::scene::LAYER`]) into the small
//! scene image, the 2D layer (tiles, panel, READY) goes on top through the CPU framebuffer, exactly like the title and the
//! mode grid. No course is loaded for this screen.
use super::fb::{clean, mix, rgb, Fb, IRect, Rgba, GREEN_INK};
use super::model::{char_info, lock_reason, roster, Ctx, Hit, Menu, Page};
use super::scene::{self, CamState, MeshBuilder, SceneAssets, LAYER};
use super::{Anim, Hits, Insets, LowRes, MenuRes, RetroActive};
use crate::characters::banjo_pose::Part;
use crate::characters::link_pose;
use crate::characters::smash::{self, Fighter};
use crate::characters::{self, Rig, RigKind};
use crate::render::{AssetRoot, SCALE};
use crate::shell::{Character, Group};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::state::*;
use std::f32::consts::TAU;
use std::sync::Arc;

const GOLD: Rgba = rgb(0xffd23a);
const WHITE: Rgba = rgb(0xf6f6ee);
/// Slots of the roster (one per playable character; hidden ones are skipped, see `Ctx::shown`).
const N: usize = Character::COUNT;
/// Width of the group label column left of the tiles.
pub const HEAD_W: i32 = 34;
/// Gap between tiles.
const GAP: i32 = 3;
/// Narrowest tile (a 6-letter name at the 6 px font plus a margin).
pub const MIN_TILE_W: i32 = 40;
/// Smallest tile height: about 48 pt on a phone (3.4 pt a framebuffer px), a comfortable touch target.
pub const MIN_TILE_H: i32 = 14;
/// Height of the model on the pedestal as a fraction of its area (the tallest Smash fighters reach ~1.2x).
const MODEL_FILL: f32 = 0.74;

// ------------------------------------------------------------------------------------------
// Layout (pure)
// ------------------------------------------------------------------------------------------

/// Where everything of the select screen sits for a framebuffer of `w` x `h`.
#[derive(Clone, Debug)]
pub struct Layout {
    pub portrait: bool,
    /// Columns of the roster (a group wraps after this many tiles).
    pub cols: usize,
    /// Each slot's tile (`None` for a slot that is not shown).
    pub tiles: [Option<IRect>; N],
    /// The label of each game group, beside its first row.
    pub headers: Vec<(Group, IRect)>,
    /// The highlighted model's feet (framebuffer px) and how tall it stands (px).
    pub feet: (f32, f32),
    pub model_h: f32,
    /// Name, moves and lock note of the highlighted character.
    pub info: IRect,
    pub ready: IRect,
    pub back: IRect,
    pub hint: IRect,
    /// Top of the plate with the chosen level / mode, and the column it is centred in.
    pub plate_y: i32,
    pub plate_col: (i32, i32),
    /// The area the roster fills.
    pub roster: IRect,
}

impl Layout {
    /// The area the model stands in (its tap zone).
    pub fn model_zone(&self) -> IRect {
        let half = (self.model_h * 0.42).max(16.0);
        IRect::new((self.feet.0 - half) as i32, (self.feet.1 - self.model_h) as i32, (half * 2.0) as i32, self.model_h as i32 + 4)
    }
}

/// Columns that fit a roster area `w` px wide (label column included).
fn cols_for(w: i32) -> usize {
    ((w - HEAD_W + GAP) / (MIN_TILE_W + GAP)).clamp(2, 6) as usize
}

/// Place the tiles of `shown` in `area`, `cols` to a row, rows `tile_h` tall. Returns the tiles and the group labels.
fn place_tiles(area: IRect, shown: &[bool; N], cols: usize, tile_h: i32) -> ([Option<IRect>; N], Vec<(Group, IRect)>) {
    let mut tiles = [None; N];
    let mut headers = Vec::new();
    let tile_w = ((area.w - HEAD_W - (cols as i32 - 1) * GAP) / cols as i32).min(56);
    let mut y = area.y;
    for (g, rows) in roster(shown, cols) {
        headers.push((g, IRect::new(area.x, y, HEAD_W - 3, tile_h)));
        for row in rows {
            for (c, i) in row.into_iter().enumerate() {
                tiles[i] = Some(IRect::new(area.x + HEAD_W + c as i32 * (tile_w + GAP), y, tile_w, tile_h));
            }
            y += tile_h + GAP;
        }
        y += 2;
    }
    (tiles, headers)
}

/// Rows and groups the roster takes for `shown` at `cols` columns.
fn roster_size(shown: &[bool; N], cols: usize) -> (i32, i32) {
    let r = roster(shown, cols);
    (r.iter().map(|(_, rows)| rows.len() as i32).sum(), r.len() as i32)
}

/// Height of a roster of `rows` rows in `groups` groups with tiles `tile_h` tall.
fn roster_h(rows: i32, groups: i32, tile_h: i32) -> i32 {
    rows * tile_h + (rows - 1).max(0) * GAP + (groups - 1).max(0) * 2
}

/// The select screen for the slots `shown` (`Ctx::shown_slots`).
pub fn layout(w: i32, h: i32, sa: Insets, shown: &[bool; N]) -> Layout {
    let portrait = w < h;
    let back = IRect::new(8 + sa.l, 6 + sa.t, 38, 15);
    let hint = IRect::new(8 + sa.l, h - 17 - sa.b, w - 16 - sa.l - sa.r, 13);
    if portrait {
        // Top to bottom: BACK and the plate, the model, its name, the roster, READY, the hint bar.
        let (x0, cw) = (8 + sa.l, w - 16 - sa.l - sa.r);
        let plate_y = back.y + back.h + 5;
        let model_top = plate_y + 25 + 4;
        let ready = IRect::new(x0, hint.y - 5 - 20, cw, 20);
        let cols = cols_for(cw);
        let (rows, groups) = roster_size(shown, cols);
        // Tiles as tall as the screen allows while the model keeps at least 70 px.
        let room = ready.y - 5 - 38 - 18 - 70 - model_top;
        let tile_h = (18..=22).rev().chain((MIN_TILE_H..18).rev()).find(|t| roster_h(rows, groups, *t) <= room).unwrap_or(MIN_TILE_H);
        let rh = roster_h(rows, groups, tile_h);
        let area = IRect::new(x0, ready.y - 5 - rh, cw, rh);
        let info = IRect::new(x0, area.y - 5 - 38, cw, 38);
        // The feet a little above the panel so the pedestal shows.
        let feet = ((x0 + cw / 2) as f32, (info.y - 18) as f32);
        let (tiles, headers) = place_tiles(area, shown, cols, tile_h);
        // Never wider than the screen: a round fighter (Kirby) is about as wide as tall.
        let model_h = (feet.1 - model_top as f32).max(30.0).min(cw as f32 * 0.9);
        Layout { portrait, cols, tiles, headers, feet, model_h, info, ready, back, hint, plate_y, plate_col: (x0, cw), roster: area }
    } else {
        // Left: the model on its pedestal, its name and READY. Right: the plate and the roster.
        let x0 = 6 + sa.l;
        let colw = (w as f32 * 0.38) as i32 - sa.l;
        let ready = IRect::new(x0, hint.y - 5 - 18, colw, 18);
        let info = IRect::new(x0, ready.y - 4 - 36, colw, 36);
        let model_top = back.y + back.h + 4;
        let feet = ((x0 + colw / 2) as f32, (info.y - 12) as f32);
        let rx = x0 + colw + 6;
        let rw = w - 6 - sa.r - rx;
        let plate_y = 5 + sa.t;
        let top = plate_y + 25 + 4;
        let avail = hint.y - 5 - top;
        let cols = cols_for(rw);
        let (rows, groups) = roster_size(shown, cols);
        let tile_h = (MIN_TILE_H..=20).rev().find(|t| roster_h(rows, groups, *t) <= avail).unwrap_or(MIN_TILE_H);
        let rh = roster_h(rows, groups, tile_h);
        // The roster sits in the middle of the space it has.
        let area = IRect::new(rx, top + (avail - rh).max(0) / 2, rw, rh);
        let (tiles, headers) = place_tiles(area, shown, cols, tile_h);
        // Never wider than its column (Kirby, Jigglypuff and Donkey Kong are about as wide as tall).
        let model_h = (feet.1 - model_top as f32).max(30.0).min(colw as f32 * 0.95);
        Layout { portrait, cols, tiles, headers, feet, model_h, info, ready, back, hint, plate_y, plate_col: (rx, rw), roster: area }
    }
}

/// The stage camera for a framebuffer (the model plane is y = 0).
pub fn camera(w: i32, h: i32) -> (Vec3, Vec3) {
    if w < h {
        (Vec3::new(0.0, 4.6, 8.4), Vec3::new(0.0, 0.7, 0.0))
    } else {
        (Vec3::new(0.0, 2.6, 6.4), Vec3::new(0.0, 0.95, 0.0))
    }
}

/// The point of the floor (y = 0) that appears at framebuffer px (`px`, `py`).
pub fn floor_point(cam: &CamState, px: f32, py: f32, w: f32, h: f32) -> Option<Vec3> {
    let view = Mat4::look_at_rh(cam.eye, cam.target, Vec3::Y);
    let proj = Mat4::perspective_rh(scene::FOV, cam.aspect, 0.1, 200.0);
    let inv = (proj * view).inverse();
    let ndc = Vec2::new(px / w * 2.0 - 1.0, 1.0 - py / h * 2.0);
    let near = inv * Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
    let far = inv * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
    let (a, b) = (near.truncate() / near.w, far.truncate() / far.w);
    let d = b - a;
    if d.y.abs() < 1e-6 {
        return None;
    }
    let t = -a.y / d.y;
    (t > 0.0).then(|| a + d * t)
}

/// A camera state for a framebuffer size (what `animate_scene` stores).
pub fn cam_for(w: i32, h: i32) -> CamState {
    let (eye, target) = camera(w, h);
    CamState { eye, target, aspect: w as f32 / h as f32 }
}

/// Where the model stands (world) and the world height that appears `model_h` px tall there.
pub fn model_spot(l: &Layout, cam: &CamState, w: i32, h: i32) -> (Vec3, f32) {
    let p = floor_point(cam, l.feet.0, l.feet.1, w as f32, h as f32).unwrap_or(Vec3::ZERO);
    // Perspective is not linear in height: refine a first guess a few times.
    let px = |hw: f32| match (cam.project(p, w as f32, h as f32), cam.project(p + Vec3::Y * hw, w as f32, h as f32)) {
        (Some(a), Some(b)) => (a.1 - b.1).abs().max(0.5),
        _ => 60.0 * hw,
    };
    let mut world_h = l.model_h / px(1.0);
    for _ in 0..4 {
        world_h *= l.model_h / px(world_h);
    }
    (p, world_h)
}

// ------------------------------------------------------------------------------------------
// Tiles (pure)
// ------------------------------------------------------------------------------------------

/// The name on a roster tile (at most 6 letters).
pub fn tile_name(c: Character) -> &'static str {
    match c {
        Character::Mario => "MARIO",
        Character::AdultLink => "LINK",
        Character::YoungLink => "Y LINK",
        Character::Banjo => "BANJO",
        Character::Bowser => "BOWSER",
        Character::Smash(f) => f.short(),
    }
}

/// The character's signature colour (the stripe on its tile).
pub fn signature(c: Character) -> Rgba {
    rgb(match c {
        Character::Mario => 0xe03a2a,
        Character::AdultLink | Character::YoungLink => 0x3aa83a,
        Character::Banjo => 0xc08a3a,
        Character::Bowser => 0x6ab02a,
        Character::Smash(f) => match f {
            Fighter::Mario => 0xe03a2a,
            Fighter::Luigi => 0x2ab04a,
            Fighter::Donkey => 0x8a4a1a,
            Fighter::Link => 0x4ab83a,
            Fighter::Samus => 0xf08a1a,
            Fighter::Yoshi => 0x5ad04a,
            Fighter::Kirby => 0xff9ac8,
            Fighter::Fox => 0xd8a060,
            Fighter::Pikachu => 0xffe030,
            Fighter::Purin => 0xffb8d8,
            Fighter::Captain => 0x3a5ad8,
            Fighter::Ness => 0xd83a3a,
        },
    })
}

/// The plate colours of a group's tiles (top, bottom).
fn group_colors(g: Group) -> (Rgba, Rgba) {
    match g {
        Group::Sm64 => (rgb(0x9a3a4a), rgb(0x5a1a28)),
        Group::Oot => (rgb(0x2f7a5a), rgb(0x16402e)),
        Group::Bk => (rgb(0x8a6a2a), rgb(0x4a3410)),
        Group::Smash => (rgb(0x4a4aa8), rgb(0x22225e)),
    }
}

// ------------------------------------------------------------------------------------------
// The stage (3D)
// ------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct StageRoot;
#[derive(Component)]
pub struct Pedestal;
#[derive(Component)]
pub struct SpotLight;
#[derive(Component, Clone)]
pub struct PreviewAnimator;

enum Body {
    Mario(MarioAnim),
    Rig(Rig),
    /// Bowser's glb on the object views' clip driver (`characters::bowser`).
    Bowser { body: Entity, clock: characters::bowser::ClipClock },
    /// A Smash fighter's glb on the same driver, its clips from the import's motion table.
    Smash { body: Entity, fighter: Fighter, data: Arc<smash::FighterData>, clock: smash::ClipClock },
}

struct Slot {
    root: Entity,
    body: Body,
    /// Stands in for the model's game state: the animators read it as they read the sim's Mario.
    m: Box<MarioRuntime>,
    /// Seconds into a hop and its length / height.
    hop: Option<(f32, f32, f32)>,
    ready_hop: bool,
    /// 0 -> 1 grow-in after the model appears.
    grow: f32,
    /// Greyed out (the mode does not take this character): the model's materials are darkened, the originals kept here.
    dimmed: bool,
    orig: Vec<(Handle<StandardMaterial>, Color)>,
}

#[derive(Resource, Default)]
pub struct Stage {
    root: Option<Entity>,
    /// Models of the characters highlighted so far (only the highlighted one is visible).
    slots: [Option<Slot>; N],
    last_sel: Option<usize>,
    /// Slots whose model could not be loaded (a load error): the pedestal stays empty.
    pub missing: [bool; N],
    /// World position of the model's floor spot and the world height it is drawn at.
    pub spot: Vec3,
    pub world_h: f32,
    smash: smash::IndexCache,
}

impl Stage {
    pub fn is_spawned(&self) -> bool {
        self.root.is_some()
    }
    /// Number of 3D models currently on the stage (spawned so far; only the highlighted one is shown).
    pub fn model_count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

fn dummy_mario() -> Box<MarioRuntime> {
    use jumpman_sim::collision::CollisionWorld;
    let mut m = MarioRuntime::new(Arc::new(CollisionWorld::default()), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0);
    m.action = ACT_IDLE;
    Box::new(m)
}

fn pedestal_mesh(r: f32, h: f32) -> Mesh {
    let mut b = MeshBuilder::default();
    let seg = 24;
    let (top, side) = (rgb(0xffffff), rgb(0xc8c8d8));
    let ring = |rad: f32, y: f32, k: usize| Vec3::new((k as f32 / seg as f32 * TAU).cos() * rad, y, (k as f32 / seg as f32 * TAU).sin() * rad);
    for k in 0..seg {
        let (a, bq) = (k, k + 1);
        b.tri_away(Vec3::new(0.0, h, 0.0), ring(r, h, a), ring(r, h, bq), top, Vec3::new(0.0, -1.0, 0.0));
        b.quad_away([ring(r, 0.0, a), ring(r, 0.0, bq), ring(r, h, bq), ring(r, h, a)], [side; 4], Vec3::new(0.0, h / 2.0, 0.0));
        // A thin rim.
        b.quad_away([ring(r + 0.07, 0.0, a), ring(r + 0.07, 0.0, bq), ring(r + 0.07, h * 0.5, bq), ring(r + 0.07, h * 0.5, a)], [mix(side, rgb(0x303050), 0.4); 4], Vec3::new(0.0, h / 2.0, 0.0));
    }
    b.build()
}

fn floor_mesh() -> Mesh {
    let mut b = MeshBuilder::default();
    let (n, size) = (14i32, 1.1f32);
    for i in -n..n {
        for j in -7..9 {
            let dark = (i + j).rem_euclid(2) == 0;
            // The floor fades into the dark toward the back.
            let far = ((-(j as f32) * size) / 9.0).clamp(0.0, 1.0);
            let base = if dark { rgb(0x16206a) } else { rgb(0x2c44b8) };
            let col = mix(base, rgb(0x050824), far * 0.85);
            let (x0, z0) = (i as f32 * size, j as f32 * size);
            b.quad_away([Vec3::new(x0, 0.0, z0), Vec3::new(x0 + size, 0.0, z0), Vec3::new(x0 + size, 0.0, z0 + size), Vec3::new(x0, 0.0, z0 + size)], [col; 4], Vec3::new(0.0, -5.0, 0.0));
        }
    }
    b.build()
}

fn light_cone() -> Mesh {
    let mut b = MeshBuilder::default();
    b.cone(Vec3::ZERO, Vec3::new(0.0, 6.5, 0.0), 0.8, 16, rgb(0xffffff));
    b.build()
}

/// Spawn the stage: backdrop, floor, the pedestal and the spotlight (the models come as they are highlighted).
fn spawn_stage(stage: &mut Stage, commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, lit: &Handle<StandardMaterial>, unlit: &Handle<StandardMaterial>) {
    let layer = RenderLayers::layer(LAYER);
    let stage_root = commands.spawn((StageRoot, Transform::default(), Visibility::Inherited, layer.clone())).id();
    // Backdrop: a deep gradient with the floor in front of it.
    let back = meshes.add({
        let mut b = MeshBuilder::default();
        b.quad_cols(
            [Vec3::new(-60.0, -2.0, 0.0), Vec3::new(60.0, -2.0, 0.0), Vec3::new(60.0, 40.0, 0.0), Vec3::new(-60.0, 40.0, 0.0)],
            [rgb(0x1a2a8a), rgb(0x1a2a8a), rgb(0x04061c), rgb(0x04061c)],
        );
        b.build()
    });
    let floor = meshes.add(floor_mesh());
    let ped = meshes.add(pedestal_mesh(0.62, 0.07));
    let ped_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.82, 0.25), perceptual_roughness: 1.0, reflectance: 0.0, opaque_render_method: bevy::material::OpaqueRendererMethod::Forward, ..default() });
    let cone = meshes.add(light_cone());
    let spot_mat = mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.95, 0.7, 0.07), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    commands.entity(stage_root).with_children(|p| {
        p.spawn((Mesh3d(back), MeshMaterial3d(unlit.clone()), Transform::from_xyz(0.0, 0.0, -9.0), layer.clone()));
        p.spawn((Mesh3d(floor), MeshMaterial3d(lit.clone()), Transform::default(), layer.clone()));
        p.spawn((Pedestal, Mesh3d(ped), MeshMaterial3d(ped_mat), Transform::default(), layer.clone()));
        p.spawn((SpotLight, Mesh3d(cone), MeshMaterial3d(spot_mat), Transform::default(), layer.clone()));
    });
    stage.root = Some(stage_root);
    stage.last_sel = None;
}

/// Spawn the model of slot `i` (its first highlight). Records a load error as `missing`.
#[allow(clippy::too_many_arguments)]
fn spawn_slot(
    stage: &mut Stage,
    i: usize,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    graphs: &mut Assets<AnimationGraph>,
    assets: &AssetServer,
    asset_root: &AssetRoot,
) {
    let (Some(stage_root), Some(who)) = (stage.root, Character::ALL.get(i).copied()) else { return };
    let layer = RenderLayers::layer(LAYER);
    let built = match who {
        Character::Mario => spawn_mario(commands, assets, asset_root, graphs, &layer, PreviewAnimator),
        Character::Bowser => spawn_bowser(commands, assets, asset_root, &layer),
        Character::Smash(f) => spawn_smash(stage, commands, assets, asset_root, &layer, f),
        other => {
            let which = match other {
                Character::AdultLink => characters::Character::Link,
                Character::YoungLink => characters::Character::YoungLink,
                _ => characters::Character::Banjo,
            };
            characters::spawn_rig(commands, meshes, mats, images, asset_root, which).map(|rig| {
                // The game's own queries (Gem Clash scaling, the kart view) must not see these.
                commands.entity(rig.root).remove::<characters::CharacterRoot>().insert(layer.clone());
                for (e, _) in &rig.batches {
                    commands.entity(*e).insert(layer.clone());
                }
                (rig.root, Body::Rig(rig))
            })
        }
    };
    match built {
        Ok((root_e, body)) => {
            let mut m = dummy_mario();
            if matches!(who, Character::AdultLink | Character::YoungLink) {
                // Sword out: the ready stance.
                m.link.enabled = true;
                m.link.sword_out = 1.0;
            }
            commands.entity(stage_root).add_child(root_e);
            commands.entity(root_e).insert(Visibility::Hidden);
            stage.slots[i] = Some(Slot { root: root_e, body, m, hop: None, ready_hop: false, grow: 0.0, dimmed: false, orig: Vec::new() });
        }
        Err(e) => {
            warn!("character select: no model for {who:?}: {e}");
            stage.missing[i] = true;
        }
    }
}

/// Bowser: his glb under a root that carries the stage transform (the body keeps the model's own scale).
fn spawn_bowser(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, layer: &RenderLayers) -> Result<(Entity, Body), String> {
    use characters::bowser;
    if !bowser::installed(&root.0) {
        return Err(format!("{} missing", bowser::MODEL));
    }
    let layer_c = layer.clone();
    let body = crate::render::object_view::spawn_model(commands, assets, root, bowser::MODEL, false)
        .insert(layer.clone())
        .observe(move |trigger: On<WorldInstanceReady>, mut commands: Commands, children: Query<&Children>| {
            for child in children.iter_descendants(trigger.event_target()) {
                commands.entity(child).insert(layer_c.clone());
            }
        })
        .id();
    let e = commands.spawn((Transform::default(), Visibility::Inherited, layer.clone())).add_child(body).id();
    Ok((e, Body::Bowser { body, clock: Default::default() }))
}

/// A Smash fighter: its glb (skinned, seek-driven) under a root that carries the stage transform.
fn spawn_smash(stage: &mut Stage, commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, layer: &RenderLayers, f: Fighter) -> Result<(Entity, Body), String> {
    if !smash::installed(&root.0, f) {
        return Err(format!("{} missing", f.model()));
    }
    let data = stage.smash.get(&root.0).and_then(|i| i.get(f.stem()).cloned()).ok_or_else(|| format!("{} not in {}", f.stem(), smash::INDEX))?;
    let body = smash::spawn_body(commands, assets, root, f, Some(layer.clone()));
    let e = commands.spawn((Transform::default(), Visibility::Inherited, layer.clone())).add_child(body).id();
    Ok((e, Body::Smash { body, fighter: f, data: Arc::new(data), clock: Default::default() }))
}

/// Spawn the real Mario model (the player's imported `mario_geo.glb`, never bundled) on the menu's render layer, its
/// animation players tagged with `marker` so a menu system can pose it. Errors when the imported file is missing.
pub(super) fn spawn_mario_model<M: Component + Clone>(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, graphs: &mut Assets<AnimationGraph>, layer: &RenderLayers, marker: M) -> Result<(Entity, MarioAnim), String> {
    use crate::game::runner::MARIO_MODEL;
    let glb = std::fs::read(root.0.join(MARIO_MODEL)).map_err(|e| format!("{MARIO_MODEL}: {e}"))?;
    let meta = crate::game::sm64_animation::read_clip_meta(&glb);
    let count = crate::render::glb_animation_count(&root.0.join(MARIO_MODEL)).max(1);
    let (graph, nodes) = AnimationGraph::from_clips(crate::render::clips::clip_handles(assets, root, MARIO_MODEL, count));
    let graph = graphs.add(graph);
    let layer = layer.clone();
    let e = commands
        .spawn((crate::render::scene(assets, MARIO_MODEL), Transform::default(), Visibility::Inherited, layer.clone()))
        .observe(move |trigger: On<WorldInstanceReady>, mut commands: Commands, children: Query<&Children>, players: Query<&AnimationPlayer>, extras: Query<Option<&bevy::gltf::GltfExtras>>| {
            let target = trigger.event_target();
            commands.entity(target).insert(layer.clone());
            for child in children.iter_descendants(target) {
                commands.entity(child).insert(layer.clone());
                if players.contains(child) {
                    commands.entity(child).insert((marker.clone(), AnimationGraphHandle(graph.clone())));
                }
                if let Ok(Some(x)) = extras.get(child) {
                    if x.value.contains(r#""mario_cap_wings""#) {
                        commands.entity(child).insert(Visibility::Hidden);
                    }
                }
            }
        })
        .id();
    Ok((e, MarioAnim { player: Box::new(crate::game::sm64_animation::Sm64AnimationPlayer::new(meta)), nodes }))
}

fn spawn_mario<M: Component + Clone>(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, graphs: &mut Assets<AnimationGraph>, layer: &RenderLayers, marker: M) -> Result<(Entity, Body), String> {
    spawn_mario_model(commands, assets, root, graphs, layer, marker).map(|(e, a)| (e, Body::Mario(a)))
}

/// Mario's SM64 animation driver and the clip nodes of his animation graph.
pub(super) struct MarioAnim {
    pub player: Box<crate::game::sm64_animation::Sm64AnimationPlayer>,
    pub nodes: Vec<AnimationNodeIndex>,
}

impl MarioAnim {
    /// Pose every animation player tagged `M` for `input` (the clip the game would pick for that action, at its frame).
    pub fn pose<M: Component>(&mut self, input: &crate::game::sm64_animation::AnimInput, dt: f64, players: &mut Query<&mut AnimationPlayer, With<M>>) {
        let pose = self.player.update(input, dt);
        if let Some(&node) = self.nodes.get(pose.clip as usize) {
            for mut p in players.iter_mut() {
                if !p.is_playing_animation(node) {
                    p.stop_all();
                    p.play(node).pause();
                }
                if let Some(a) = p.animation_mut(node) {
                    a.seek_to(pose.sample_seconds);
                }
            }
        }
    }
}

/// Spawn the stage when the select page opens, the highlighted character's model the first time it is highlighted, and
/// take everything down when the page closes.
#[allow(clippy::too_many_arguments)]
pub fn manage_stage(
    mut commands: Commands,
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    ctx: Res<super::CtxRes>,
    mut stage: ResMut<Stage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    scene_assets: Option<Res<SceneAssets>>,
) {
    let want = active.0 && menu.0.page == Page::Select;
    if want && !stage.is_spawned() {
        let Some(sa) = scene_assets else { return };
        spawn_stage(&mut stage, &mut commands, &mut meshes, &mut mats, &sa.lit.clone(), &sa.unlit.clone());
    } else if !want && stage.is_spawned() {
        if let Some(e) = stage.root.take() {
            commands.entity(e).despawn();
        }
        // Models are children of the stage root and go with it; their meshes drop with the handles.
        *stage = Stage::default();
        return;
    }
    let sel = menu.0.sel.min(N - 1);
    if want && stage.slots[sel].is_none() && !stage.missing[sel] && ctx.0.hero_ok[sel] {
        let (Some(server), Some(root)) = (server, root) else { return };
        let t0 = std::time::Instant::now();
        spawn_slot(&mut stage, sel, &mut commands, &mut meshes, &mut mats, &mut images, &mut graphs, &server, &root);
        info!("character select: {:?} model up in {:.0} ms", Character::ALL[sel], t0.elapsed().as_secs_f32() * 1000.0);
    }
}

/// The world scale of `who`'s model so that it stands about `world_h` tall (each rig's own units, Smash sizes softened).
pub fn model_scale(who: Character, world_h: f32) -> f32 {
    // The old lineup's base scales draw Mario, Link, Banjo and Bowser all about 1.7 units tall.
    let k = world_h / 1.7;
    match who {
        Character::Mario => k,
        Character::Bowser => 0.3 * k,
        Character::AdultLink | Character::YoungLink => 0.044 * SCALE * k,
        Character::Banjo => 1.25 * SCALE * k,
        // Smash fighters at their in-game size relative to Mario, softened so Kirby is not tiny and Donkey Kong still fits.
        Character::Smash(f) => {
            let natural = smash::MARIO_HEIGHT * SCALE * f.height_ratio();
            let shown = world_h * f.height_ratio().powf(0.35).clamp(0.8, 1.2);
            f.scale() * shown / natural
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn pose_stage(
    time: Res<Time<Real>>,
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    ctx: Res<super::CtxRes>,
    low: Res<LowRes>,
    mut stage: ResMut<Stage>,
    mut tfs: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut players: Query<&mut AnimationPlayer, With<PreviewAnimator>>,
    mat_q: Query<&MeshMaterial3d<StandardMaterial>>,
    mut anim_rigs: Query<&mut crate::render::object_view::AnimRig>,
) {
    if !active.0 || menu.0.page != Page::Select || !stage.is_spawned() {
        return;
    }
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let m = &menu.0;
    let lay = layout(low.w, low.h, Insets::default(), &ctx.0.shown_slots());
    let cam = cam_for(low.w, low.h);
    let (spot, world_h) = model_spot(&lay, &cam, low.w, low.h);
    let world_h = world_h * MODEL_FILL;
    stage.spot = spot;
    stage.world_h = world_h;
    let sel = m.sel.min(N - 1);
    // A new highlight pops its character in with a little hop; READY makes a bigger one (a Smash fighter taunts).
    let confirming = m.pending.is_some();
    if stage.last_sel != Some(sel) {
        if let Some(Some(s)) = stage.slots.get_mut(sel) {
            s.hop = Some((0.0, 0.5, 0.18 * world_h));
            s.grow = 0.0;
        }
        stage.last_sel = Some(sel);
    }
    let step = 1.0 - (-10.0 * dt).exp();
    for i in 0..N {
        let is_sel = i == sel;
        let Some(slot) = stage.slots[i].as_mut() else { continue };
        if let Ok(mut v) = vis.get_mut(slot.root) {
            v.set_if_neq(if is_sel { Visibility::Inherited } else { Visibility::Hidden });
        }
        if !is_sel {
            continue;
        }
        if confirming && !slot.ready_hop {
            slot.ready_hop = true;
            slot.hop = Some((0.0, super::model::READY_TIME, 0.4 * world_h));
        }
        if !confirming {
            slot.ready_hop = false;
        }
        // Greyed out when the mode does not take this character.
        let want_dim = !ctx.0.allowed[i];
        if slot.dimmed != want_dim {
            if let Body::Rig(rig) = &slot.body {
                if slot.orig.is_empty() {
                    slot.orig = rig.batches.iter().filter_map(|(e, _)| mat_q.get(*e).ok()).filter_map(|m| mats.get(&m.0).map(|x| (m.0.clone(), x.base_color))).collect();
                }
                for (h, col) in &slot.orig {
                    if let Some(mut m) = mats.get_mut(h) {
                        m.base_color = if want_dim { col.mix(&Color::srgb(0.12, 0.12, 0.16), 0.9) } else { *col };
                    }
                }
            }
            // (The materials exist one frame after the spawn; try again until they were found.)
            slot.dimmed = !slot.orig.is_empty() && want_dim;
        }
        slot.grow += (1.0 - slot.grow) * step;
        // Hop: a parabola over the animation; the airborne pose goes to the animators. A taunting Smash fighter stays put.
        let taunts = confirming && matches!(slot.body, Body::Smash { .. });
        let mut lift = 0.0;
        let mut airborne = false;
        if let Some((ht, dur, height)) = slot.hop.as_mut() {
            *ht += dt;
            let u = (*ht / *dur).clamp(0.0, 1.0);
            lift = if taunts { 0.0 } else { *height * 4.0 * u * (1.0 - u) };
            airborne = u < 1.0 && !taunts;
            if *ht >= *dur {
                slot.hop = None;
            }
        }
        let yaw = (t * 0.9).sin() * 0.4;
        let at = Vec3::new(spot.x, spot.y + lift + 0.07 * world_h / 1.7, spot.z);
        let k = 0.85 + 0.15 * slot.grow;
        let who = Character::ALL[i];
        let place = |tfs: &mut Query<&mut Transform>, e: Entity, scale: f32| {
            if let Ok(mut tf) = tfs.get_mut(e) {
                *tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale * k));
            }
        };
        slot.m.action = if airborne { ACT_JUMP } else { ACT_IDLE };
        slot.m.vel[1] = if airborne { 12.0 } else { 0.0 };
        let hop_t = slot.hop.map_or(0.0, |h| h.0 / h.1);
        let root_e = slot.root;
        match &mut slot.body {
            Body::Mario(anim) => {
                let input = crate::game::sm64_animation::AnimInput { action: slot.m.action, vel_y: slot.m.vel[1], action_timer: (hop_t * 30.0) as f64, ..Default::default() };
                anim.pose(&input, dt as f64, &mut players);
                place(&mut tfs, root_e, model_scale(who, world_h));
            }
            Body::Bowser { body, clock } => {
                use characters::bowser;
                // Idle on the pedestal, the jump pose in the hop.
                let clip = bowser::clip_for(slot.m.action, 0.0, 0.0);
                let sample = clock.step(&clip, slot.m.action, dt);
                if let Ok(mut rig) = anim_rigs.get_mut(*body) {
                    rig.want = bowser::glb_clip(clip.anim);
                    rig.seek = Some(sample);
                }
                place(&mut tfs, root_e, model_scale(who, world_h));
            }
            Body::Smash { body, fighter, data, clock } => {
                // Wait on the pedestal; READY plays the taunt; the hop shows the jump.
                let (mv, play) = if taunts {
                    (smash::Move::Taunt, smash::Play::Once)
                } else if airborne {
                    (smash::Move::Jump, smash::Play::Once)
                } else {
                    (smash::Move::Wait, smash::Play::Loop)
                };
                let clip = smash::SmashClip { mv, play, speed: 1.0, stage: 0, tilt: 0.0 };
                let played = clock.step(&clip, mv as u32, dt);
                let name = data.resolve(mv);
                let sample = smash::sample_time(&clip, data.seconds(name), played);
                if let Ok(mut rig) = anim_rigs.get_mut(*body) {
                    if let Some(i) = rig.names.iter().position(|n| n == name) {
                        rig.want = i;
                        rig.seek = Some(sample);
                    }
                }
                place(&mut tfs, root_e, model_scale(Character::Smash(*fighter), world_h));
            }
            Body::Rig(rig) => {
                place(&mut tfs, rig.root, model_scale(who, world_h));
                pose_rig(rig, &slot.m, dt as f64, &mut meshes, &mut vis);
            }
        }
    }
}

/// The pedestal and the spotlight stand under the model.
pub fn place_props(stage: Res<Stage>, mut peds: Query<&mut Transform, (With<Pedestal>, Without<SpotLight>)>, mut spot: Query<&mut Transform, With<SpotLight>>) {
    if !stage.is_spawned() {
        return;
    }
    let k = (stage.world_h / 1.7).max(0.2);
    for mut tf in &mut peds {
        *tf = Transform::from_translation(stage.spot).with_scale(Vec3::new(k, k, k));
    }
    if let Ok(mut tf) = spot.single_mut() {
        *tf = Transform::from_translation(stage.spot).with_scale(Vec3::new(k, k.max(1.0), k));
    }
}

fn pose_rig(rig: &mut Rig, m: &MarioRuntime, dt: f64, meshes: &mut Assets<Mesh>, vis: &mut Query<&mut Visibility>) {
    match &mut rig.kind {
        RigKind::Link { asset, anim, young, modes } => {
            let (_, bones) = anim.update(asset, *young, m, dt, true);
            let drawn = link_pose::sword_drawn(m);
            for (i, ((entity, handle), batch)) in rig.batches.iter().zip(&asset.batches).enumerate() {
                if let Ok(mut v) = vis.get_mut(*entity) {
                    let limb = batch.vertices.first().map_or(0, |v| v[8] as usize);
                    let want = if link_pose::batch_visible(modes[i].as_str(), limb, drawn, None, false) { Visibility::Inherited } else { Visibility::Hidden };
                    v.set_if_neq(want);
                }
                let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                let mut positions = Vec::with_capacity(batch.vertices.len());
                let mut normals = Vec::with_capacity(batch.vertices.len());
                for v in &batch.vertices {
                    let bone = &bones[(v[8] as usize).min(bones.len() - 1)];
                    positions.push(bone.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array());
                    let n = bone.transform_vector3(Vec3::new(v[5] as f32, v[6] as f32, v[7] as f32) / 127.0);
                    normals.push(n.normalize_or_zero().to_array());
                }
                characters::write_mesh(&mut mesh, positions, normals);
            }
        }
        RigKind::Banjo { asset, anim, parts } => {
            anim.update(asset, m, dt, true);
            let mats = anim.matrices(asset);
            for (i, ((entity, handle), batch)) in rig.batches.iter().zip(&asset.batches).enumerate() {
                if let Some(part) = Part::from_name(&parts[i]) {
                    if let Ok(mut v) = vis.get_mut(*entity) {
                        v.set_if_neq(if anim.shown_parts.contains(&part) { Visibility::Inherited } else { Visibility::Hidden });
                    }
                }
                let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                let positions: Vec<[f32; 3]> = batch
                    .vertices
                    .iter()
                    .map(|v| mats[(v[9].max(0.0) as usize).min(mats.len() - 1)].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array())
                    .collect();
                let normals = characters::flat_normals(&positions);
                characters::write_mesh(&mut mesh, positions, normals);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------
// 2D layer
// ------------------------------------------------------------------------------------------

fn wrap_words(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn dialog(f: &mut Fb, r: IRect) {
    f.slab(r, rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
}

/// Pad / touch / keyboard hint line.
fn hint(pad: bool) -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        "TAP A HERO   TAP AGAIN OR READY TO PLAY"
    } else if pad {
        "D-PAD CHOOSE   L R GAME   A READY   B BACK"
    } else {
        "ARROWS CHOOSE   ENTER READY   ESC BACK"
    }
}

fn draw_lock(f: &mut Fb, cx: i32, y: i32) {
    f.rect(IRect::new(cx - 4, y + 4, 8, 6), rgb(0x9a9ab0));
    f.frame(IRect::new(cx - 4, y + 4, 8, 6), rgb(0x20202c));
    f.rect(IRect::new(cx - 3, y, 2, 5), rgb(0x9a9ab0));
    f.rect(IRect::new(cx + 1, y, 2, 5), rgb(0x9a9ab0));
    f.rect(IRect::new(cx - 3, y - 1, 6, 2), rgb(0x9a9ab0));
}

#[allow(clippy::too_many_arguments)]
pub fn draw_select(f: &mut Fb, menu: &Menu, ctx: &Ctx, hits: &mut Hits, anim: &mut Anim, t: f32, sa: Insets, pad: bool) {
    let _ = anim;
    let (w, h) = (f.w, f.h);
    let lay = layout(w, h, sa, &ctx.shown_slots());
    let sel = menu.sel.min(N - 1);
    let who = Character::ALL[sel];
    let info = char_info(who);

    // The model is a tap target too (tap the highlighted character again to confirm); tiles and buttons are pushed after it.
    hits.0.push((lay.model_zone(), Hit::Slot(sel)));

    // BACK.
    let back = lay.back;
    f.slab(back, rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603));
    f.text_centered(back.x + back.w / 2, back.y + 4, "BACK", 1, WHITE);
    hits.0.push((back.inflate(3), Hit::Back));

    // Plate with the chosen level / mode, centred over its column.
    let (pc_x, pc_w) = lay.plate_col;
    let name = clean(&ctx.target.name);
    let scale = if Fb::text_width(&name, 2) + 20 <= pc_w { 2 } else { 1 };
    let pw = (Fb::text_width(&name, scale) + 20).max(Fb::text_width(ctx.target.kind, 1) + 20).min(pc_w);
    let plate = IRect::new(pc_x + (pc_w - pw) / 2, lay.plate_y, pw, 25);
    dialog(f, plate);
    f.text_centered(plate.x + pw / 2, plate.y + 3, ctx.target.kind, 1, rgb(0x9fe0ff));
    f.text_centered(plate.x + pw / 2, plate.y + if scale == 2 { 11 } else { 13 }, &name, scale, GOLD);
    if ctx.target.mario_only {
        let note = "MARIO ONLY MODE";
        let nw = Fb::text_width(note, 1) + 10;
        let r = IRect::new(lay.feet.0 as i32 - nw / 2, lay.feet.1 as i32 - lay.model_h as i32 + 2, nw, 12);
        f.slab(r, rgb(0xd09a2a), rgb(0x8a5a0c), rgb(0xffe08a), rgb(0x4a2a04), rgb(0x2a1602));
        f.ink_centered(r.x + r.w / 2, r.y + 3, note, 1, rgb(0x2a1602));
    }

    // The roster: group labels, then the tiles.
    for (g, r) in &lay.headers {
        let label = g.label();
        let col = group_colors(*g).0;
        f.rect(IRect::new(r.x, r.y, 2, r.h), mix(col, WHITE, 0.3));
        f.text(r.x + 4, r.y + (r.h - 7) / 2, label, 1, mix(col, WHITE, 0.65));
    }
    for i in (0..N).filter(|i| ctx.shown(*i)) {
        let Some(r) = lay.tiles[i] else { continue };
        let c = Character::ALL[i];
        let locked = lock_reason(c, ctx.hero_ok[i], ctx.allowed[i]).is_some();
        let on = i == sel;
        let (top, bottom) = group_colors(c.group());
        if on {
            f.slab(r, mix(GOLD, WHITE, ((t * 6.0).sin() * 0.5 + 0.5) * 0.35), rgb(0xe08a18), rgb(0xfff1a8), rgb(0x8a4a08), rgb(0x3a1a04));
        } else if locked {
            f.slab(r, rgb(0x4a4a5a), rgb(0x26262e), rgb(0x7a7a90), rgb(0x16161e), rgb(0x08080c));
        } else {
            f.slab(r, top, bottom, mix(top, WHITE, 0.45), mix(bottom, rgb(0), 0.4), rgb(0x040720));
        }
        // The character's colour as a stripe along the top (the tile's "portrait").
        let stripe = if locked { mix(signature(c), rgb(0x303038), 0.7) } else { signature(c) };
        f.rect(IRect::new(r.x + 2, r.y + 2, r.w - 4, 2), stripe);
        let label = tile_name(c);
        let col = if on { rgb(0x3a1a04) } else if locked { rgb(0x9a9aae) } else { rgb(0xeef2ff) };
        let ty = r.y + 4 + (r.h - 4 - 7) / 2;
        if on { f.ink_centered(r.x + r.w / 2, ty, label, 1, col) } else { f.text_centered(r.x + r.w / 2, ty, label, 1, col) }
        if locked && !on {
            draw_lock(f, r.x + r.w - 6, r.y + 1);
        }
        if on {
            f.frame(r.inflate(1), WHITE);
        }
        hits.0.push((r.inflate(1), Hit::Slot(i)));
    }

    // Info panel: name, moves (or why it is locked).
    let p = lay.info;
    dialog(f, p);
    let pad_x = p.x + 6;
    let max_chars = ((p.w - 12) / 6).max(8) as usize;
    let ns = if Fb::text_width(info.name, 2) <= p.w - 12 { 2 } else { 1 };
    f.text(pad_x, p.y + 5, info.name, ns, GOLD);
    let mut y = p.y + 5 + 7 * ns + 4;
    let reason = lock_reason(who, ctx.hero_ok[sel], ctx.allowed[sel]);
    let (text, col) = match reason {
        Some(r) => (format!("LOCKED: {r}"), rgb(0xffb0a0)),
        None => (info.moves.to_string(), WHITE),
    };
    let lines = wrap_words(&text, max_chars);
    let room = ((p.y + p.h - 3 - y + 2) / 9).max(1) as usize;
    for (k, l) in lines.iter().take(room).enumerate() {
        // A list cut short by the panel ends on its last whole item, not on a dangling comma.
        let l = if k + 1 == room && lines.len() > room { l.trim_end_matches(',') } else { l.as_str() };
        f.text(pad_x, y, l, 1, col);
        y += 9;
    }

    // READY.
    let rb = lay.ready;
    if reason.is_none() {
        let pulse = (t * 5.0).sin() * 0.5 + 0.5;
        f.slab(rb, mix(rgb(0x4ad060), rgb(0x8aff8a), pulse * 0.5), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a));
        f.ink_centered(rb.x + rb.w / 2, rb.y + (rb.h - 14) / 2, "READY", 2, GREEN_INK);
    } else {
        f.slab(rb, rgb(0x6a6a78), rgb(0x3a3a46), rgb(0x9a9aa8), rgb(0x20202a), rgb(0x0a0a10));
        f.text_centered(rb.x + rb.w / 2, rb.y + (rb.h - 14) / 2, "LOCKED", 2, rgb(0xc0c0cc));
    }
    hits.0.push((rb.inflate(2), Hit::Ready));
    // Hint bar.
    let bar = lay.hint;
    dialog(f, bar);
    let text = hint(pad);
    let text = if Fb::text_width(text, 1) > bar.w - 8 { "TAP TO CHOOSE" } else { text };
    f.text(bar.x + 5, bar.y + 3, text, 1, WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::retro::model::{Ctx, Target};

    fn slot(c: Character) -> usize {
        Character::ALL.iter().position(|x| *x == c).unwrap()
    }

    /// Everyone installed but Young Link; with or without the twelve Smash fighters.
    fn ctx(smash: bool) -> Ctx {
        let ok = std::array::from_fn(|i| Character::ALL[i] != Character::YoungLink && (smash || !matches!(Character::ALL[i], Character::Smash(_))));
        let mut c = Ctx::from_registry(Character::Mario, ok, |_| true);
        c.target = Target { kind: "COURSE", name: "BOB-OMB BATTLEFIELD".into(), mario_only: false };
        c
    }

    /// Landscape 2.2:1, 16:9, 4:3, and portrait phones / tablets (the framebuffer sizes `LowRes` produces), with safe areas.
    const SIZES: [(i32, i32, Insets); 7] = [
        (528, 240, Insets { l: 0, r: 0, t: 0, b: 0 }),
        (428, 240, Insets { l: 0, r: 0, t: 0, b: 0 }),
        (320, 240, Insets { l: 0, r: 0, t: 0, b: 0 }),
        (520, 240, Insets { l: 22, r: 22, t: 0, b: 8 }),
        (240, 520, Insets { l: 0, r: 0, t: 22, b: 10 }),
        (240, 345, Insets { l: 0, r: 0, t: 0, b: 0 }),
        (240, 640, Insets { l: 0, r: 0, t: 0, b: 0 }),
    ];

    fn overlap(a: IRect, b: IRect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }
    fn inside(a: IRect, w: i32, h: i32) -> bool {
        a.x >= 0 && a.y >= 0 && a.x + a.w <= w && a.y + a.h <= h
    }

    #[test]
    fn floor_point_inverts_the_projection() {
        for (w, h) in [(520, 240), (240, 522), (320, 240)] {
            let cam = cam_for(w, h);
            for (px, py) in [(w as f32 * 0.3, h as f32 * 0.7), (w as f32 * 0.6, h as f32 * 0.5)] {
                let p = floor_point(&cam, px, py, w as f32, h as f32).expect("on the floor");
                assert!(p.y.abs() < 1e-3);
                let (x, y, _) = cam.project(p, w as f32, h as f32).unwrap();
                assert!((x - px).abs() < 0.5 && (y - py).abs() < 0.5, "{w}x{h}: {x},{y} vs {px},{py}");
            }
        }
    }

    /// Every aspect ratio: all 17 tiles (or the 5 without Smash) inside the screen, inside the safe area, in reading order,
    /// big enough to tap, apart from each other and from the model, the panel, READY and the hint bar.
    #[test]
    fn roster_fits_every_aspect_ratio_with_comfortable_tiles() {
        for smash in [true, false] {
            let c = ctx(smash);
            for (w, h, sa) in SIZES {
                let l = layout(w, h, sa, &c.shown_slots());
                assert_eq!(l.portrait, w < h);
                let tiles: Vec<(usize, IRect)> = (0..N).filter_map(|i| l.tiles[i].map(|r| (i, r))).collect();
                assert_eq!(tiles.len(), (0..N).filter(|i| c.shown(*i)).count(), "{w}x{h}");
                assert_eq!(tiles.len(), if smash { 17 } else { 5 }, "{w}x{h}");
                let safe = IRect::new(sa.l, sa.t, w - sa.l - sa.r, h - sa.t - sa.b);
                for (i, r) in &tiles {
                    assert!(r.h >= MIN_TILE_H && r.w >= MIN_TILE_W - 4, "{w}x{h}: tile {i} {r:?} is too small to tap");
                    assert!(Fb::text_width(tile_name(Character::ALL[*i]), 1) <= r.w - 4, "{w}x{h}: {} in {r:?}", tile_name(Character::ALL[*i]));
                    assert!(r.x >= safe.x && r.y >= safe.y && r.x + r.w <= safe.x + safe.w && r.y + r.h <= safe.y + safe.h, "{w}x{h}: tile {i} {r:?} outside the safe area");
                    for other in [l.info, l.ready, l.hint, l.back, l.model_zone()] {
                        assert!(!overlap(*r, other), "{w}x{h}: tile {i} {r:?} runs into {other:?}");
                    }
                }
                for (k, (_, a)) in tiles.iter().enumerate() {
                    for (_, b) in &tiles[k + 1..] {
                        assert!(!overlap(*a, *b), "{w}x{h}: tiles {a:?} {b:?}");
                    }
                }
                // Reading order: the roster order of the model is top-to-bottom, left-to-right on screen.
                let mut c2 = c.clone();
                c2.roster_cols = l.cols;
                let order = c2.order();
                for p in order.windows(2) {
                    let (a, b) = (l.tiles[p[0]].unwrap(), l.tiles[p[1]].unwrap());
                    assert!(b.y > a.y || (b.y == a.y && b.x > a.x), "{w}x{h}: {:?} before {:?}", p[0], p[1]);
                }
                for r in [l.info, l.ready, l.hint, l.back] {
                    assert!(inside(r, w, h), "{w}x{h}: {r:?}");
                }
                assert!(l.ready.h >= 18 && l.info.w >= 100, "{w}x{h}");
                assert!(l.model_h >= 60.0, "{w}x{h}: model only {} px tall", l.model_h);
                // Group labels sit left of their first row, inside the screen.
                assert_eq!(l.headers.len(), if smash { 4 } else { 3 });
                for (_, r) in &l.headers {
                    assert!(inside(*r, w, h) && Fb::text_width("SMASH", 1) <= r.w, "{w}x{h}: {r:?}");
                }
            }
        }
    }

    fn draw(w: i32, h: i32, sel: usize, c: &Ctx) -> (Fb, Hits) {
        let mut menu = Menu::default();
        menu.open_select(c);
        menu.sel = sel;
        let mut f = Fb::new(w, h);
        let mut hits = Hits::default();
        let mut anim = Anim::default();
        draw_select(&mut f, &menu, c, &mut hits, &mut anim, 1.0, Insets::default(), false);
        (f, hits)
    }

    #[test]
    fn every_control_has_a_hit_zone_and_taps_land_on_their_tile() {
        let c = ctx(true);
        for (w, h, _) in SIZES {
            let (f, hits) = draw(w, h, 1, &c);
            for want in [Hit::Back, Hit::Ready] {
                assert!(hits.0.iter().any(|(_, x)| *x == want), "{w}x{h} missing {want:?}");
            }
            assert!(f.data.chunks(4).filter(|p| p[3] > 0).count() > 2000);
            for i in (0..N).filter(|i| c.shown(*i)) {
                // The topmost hit at the tile's centre is that tile.
                let (r, _) = hits.0.iter().rev().find(|(r, x)| *x == Hit::Slot(i) && r.h <= 26).unwrap_or_else(|| panic!("{w}x{h}: no tile for {i}"));
                let (cx, cy) = r.center();
                let top = hits.0.iter().rev().find(|(rect, _)| rect.contains(cx as f32, cy as f32)).map(|(_, h)| *h);
                assert_eq!(top, Some(Hit::Slot(i)), "{w}x{h}");
            }
        }
    }

    /// Without the Smash import the SMASH group is not there at all; with eleven fighters, the missing one has no tile.
    #[test]
    fn missing_fighters_leave_no_tile() {
        let mut c = ctx(true);
        let ness = slot(Character::Smash(Fighter::Ness));
        c.hero_ok[ness] = false;
        for (w, h, sa) in SIZES {
            let l = layout(w, h, sa, &c.shown_slots());
            assert!(l.tiles[ness].is_none(), "{w}x{h}");
            assert!(l.tiles[slot(Character::Smash(Fighter::Captain))].is_some());
            let (_, hits) = draw(w, h, 0, &c);
            assert!(!hits.0.iter().any(|(_, x)| *x == Hit::Slot(ness)));
        }
        let none = ctx(false);
        let l = layout(320, 240, Insets::default(), &none.shown_slots());
        assert!(l.headers.iter().all(|(g, _)| *g != Group::Smash));
        assert!(Fighter::ALL.iter().all(|f| l.tiles[slot(Character::Smash(*f))].is_none()));
    }

    #[test]
    fn locked_and_mario_only_slots_say_why() {
        let mut c = ctx(true);
        let (_, _) = draw(520, 240, slot(Character::YoungLink), &c);
        assert_eq!(lock_reason(Character::YoungLink, c.hero_ok[2], c.allowed[2]), Some("IMPORT OCARINA OF TIME"));
        c.allowed = std::array::from_fn(|i| i == 0);
        c.target.mario_only = true;
        let (_, _) = draw(240, 522, slot(Character::Smash(Fighter::Fox)), &c);
        assert_eq!(lock_reason(Character::Smash(Fighter::Fox), true, false), Some("MARIO ONLY"));
    }

    #[test]
    fn model_spot_is_on_the_floor_under_the_feet_and_scales_with_the_layout() {
        for (w, h, sa) in SIZES {
            let l = layout(w, h, sa, &ctx(true).shown_slots());
            let cam = cam_for(w, h);
            let (p, world_h) = model_spot(&l, &cam, w, h);
            assert!(p.y.abs() < 1e-3 && world_h > 0.3, "{w}x{h}: {p} {world_h}");
            let (x, y, _) = cam.project(p, w as f32, h as f32).unwrap();
            assert!((x - l.feet.0).abs() < 1.0 && (y - l.feet.1).abs() < 1.0);
            // A model of `world_h` reaches the top of its area, not past the plate.
            let (_, top, _) = cam.project(p + Vec3::Y * world_h, w as f32, h as f32).unwrap();
            assert!((l.feet.1 - top - l.model_h).abs() < 2.0, "{w}x{h}");
        }
        // Each rig's own units: Mario's scale is the world height ratio, the Smash fighters at about Mario's height.
        let h = 1.7;
        assert!((model_scale(Character::Mario, h) - 1.0).abs() < 1e-5);
        let shown = |f: Fighter| model_scale(Character::Smash(f), h) * smash::MARIO_HEIGHT * SCALE * f.height_ratio() / f.scale();
        assert!((shown(Fighter::Mario) - h).abs() < 1e-3);
        assert!(shown(Fighter::Kirby) < h && shown(Fighter::Kirby) >= 0.8 * h - 1e-3);
        assert!(shown(Fighter::Donkey) > h && shown(Fighter::Donkey) <= 1.2 * h + 1e-3);
    }

    /// The stage exists only while the select page is open; only the highlighted model is spawned, others as they come up.
    #[test]
    fn models_spawn_on_highlight_and_despawn_on_exit() {
        use crate::shell::retro::{CtxRes, MenuRes};
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let have_data = data.join(crate::game::runner::MARIO_MODEL).is_file() && smash::installed(&data, Fighter::Pikachu);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Image>()
            .init_asset::<AnimationGraph>()
            .init_asset::<AnimationClip>()
            .init_asset::<bevy::gltf::Gltf>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .insert_resource(AssetRoot::new(data))
            .insert_resource(RetroActive(true))
            .insert_resource(MenuRes(Menu::default()))
            .insert_resource(CtxRes(ctx(true)))
            .init_resource::<Stage>()
            .add_systems(Update, manage_stage);
        let (lit, unlit) = {
            let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            (mats.add(StandardMaterial::default()), mats.add(StandardMaterial::default()))
        };
        app.insert_resource(SceneAssets { lit, unlit });
        // Title page: nothing.
        app.update();
        assert!(!app.world().resource::<Stage>().is_spawned());
        let count = |app: &mut App| app.world_mut().query_filtered::<Entity, With<RenderLayers>>().iter(app.world()).count();
        assert_eq!(count(&mut app), 0);
        // Open the select.
        {
            let c = ctx(true);
            app.world_mut().resource_mut::<MenuRes>().0.open_select(&c);
        }
        app.world_mut().resource_mut::<CtxRes>().0.hero_ok = if have_data { std::array::from_fn(|i| i == 0 || i == slot(Character::Smash(Fighter::Pikachu))) } else { [false; N] };
        app.update();
        app.update();
        let st = app.world().resource::<Stage>();
        assert!(st.is_spawned());
        let spawned = count(&mut app);
        assert!(spawned >= 5, "root, floor, backdrop, pedestal and spot: {spawned}");
        if have_data {
            assert_eq!(app.world().resource::<Stage>().model_count(), 1, "only the highlighted model");
            app.world_mut().resource_mut::<MenuRes>().0.sel = slot(Character::Smash(Fighter::Pikachu));
            app.update();
            app.update();
            assert_eq!(app.world().resource::<Stage>().model_count(), 2, "Pikachu comes up on its first highlight");
            assert!(!app.world().resource::<Stage>().missing.iter().any(|m| *m));
        }
        // Leave: everything is gone, also the models.
        app.world_mut().resource_mut::<MenuRes>().0.close_select();
        app.update();
        app.update();
        assert!(!app.world().resource::<Stage>().is_spawned());
        assert_eq!(count(&mut app), 0, "no stage entity survives the page");
        assert_eq!(app.world_mut().query::<&StageRoot>().iter(app.world()).count(), 0);
        // Re-entering (highlight back on Mario) does not leak.
        {
            let c = ctx(true);
            let mut m = app.world_mut().resource_mut::<MenuRes>();
            m.0.open_select(&c);
            m.0.sel = 0;
        }
        app.update();
        app.update();
        let again = count(&mut app);
        assert!(again <= spawned + 2, "{again} vs {spawned}");
        app.world_mut().resource_mut::<MenuRes>().0.close_select();
        app.update();
        assert_eq!(count(&mut app), 0);
    }

    #[test]
    fn stage_helpers_start_empty() {
        let s = Stage::default();
        assert!(!s.is_spawned());
        assert_eq!(s.model_count(), 0);
    }

    #[test]
    fn meshes_are_valid() {
        for m in [pedestal_mesh(0.6, 0.07), floor_mesh(), light_cone()] {
            let n = m.count_vertices();
            assert!(n > 0 && n % 3 == 0);
        }
    }

    #[test]
    fn tile_names_and_colours_cover_everyone() {
        for c in Character::ALL {
            assert!(tile_name(c).len() <= 6 && tile_name(c) == clean(tile_name(c)), "{c:?}");
            let _ = signature(c);
        }
        assert_eq!(tile_name(Character::Smash(Fighter::Mario)), "MARIO", "the SMASH group label tells it from SM64 Mario");
    }
}
