//! Sunstone Shindig in 3D, on the real Bob-omb Battlefield: the board props laid on the course floor, the mini-game props
//! (pooled primitives plus the real Goomba, Bob-omb and Power Star models), the four Mario pieces (real SM64 rigs driven by
//! `CrowdMario` on the course collision; in a mini-game the full physics steps the rules) and the cameras.
use super::app::*;
use super::board::*;
use super::common::*;
use super::game::*;
use super::minis::*;
use super::sites::{Frame, BOARD_SCALE};
use crate::experiments::crowd::{spawn_body, CrowdLook, CrowdMario, DriveInput};
use crate::experiments::{CamPose, CameraOverride, XpEntity};
use crate::render::to_bevy;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::math::atan2s;
use jumpman_sim::state::*;
use std::collections::HashMap;

pub const LOOKS: [CrowdLook; 4] = [
    CrowdLook { cap: 0xe23b2e, overalls: Some(0x2d4fd6) },
    CrowdLook { cap: 0x3d7bff, overalls: Some(0xf0f0ff) },
    CrowdLook { cap: 0x2fbf5a, overalls: Some(0x1d5b30) },
    CrowdLook { cap: 0xf5c518, overalls: Some(0x7a4a14) },
];

const STAR: &str = "actors/star/star_geo.glb";
const TOAD: &str = "actors/toad/toad_geo.glb";
const GOOMBA: &str = "actors/goomba/goomba_geo.glb";
const BOBOMB: &str = "actors/bobomb/black_bobomb_geo.glb";

pub fn hex(c: u32) -> Color {
    Color::srgb(((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0)
}

#[derive(Component)]
pub struct BoardProp;
#[derive(Component)]
pub struct SpaceDisc(pub usize);
#[derive(Component)]
pub struct EdgeSeg {
    pub rule: Rule,
}
#[derive(Component)]
pub struct KeeperProp;
#[derive(Component)]
pub struct Highlight(pub usize);
#[derive(Component)]
pub struct StageShape(pub usize);
#[derive(Component)]
pub struct PodiumDisc;
/// A pooled model of a mini-game: kind (0 Goomba, 1 Bob-omb, 2 star, 3 crown) and slot.
#[derive(Component)]
pub struct MiniModel(pub u8, pub usize);

/// Unit meshes and a colour -> material cache.
#[derive(Resource)]
pub struct ViewCache {
    pub cube: Handle<Mesh>,
    pub cyl: Handle<Mesh>,
    pub ball: Handle<Mesh>,
    pub cone: Handle<Mesh>,
    pub mats: HashMap<u32, Handle<StandardMaterial>>,
}

impl FromWorld for ViewCache {
    fn from_world(w: &mut World) -> Self {
        let mut meshes = w.resource_mut::<Assets<Mesh>>();
        ViewCache {
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            cyl: meshes.add(Cylinder::new(1.0, 1.0)),
            ball: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
            cone: meshes.add(Cone { radius: 1.0, height: 1.0 }),
            mats: HashMap::new(),
        }
    }
}

impl ViewCache {
    pub fn mat(&mut self, mats: &mut Assets<StandardMaterial>, color: u32) -> Handle<StandardMaterial> {
        self.mats.entry(color).or_insert_with(|| mats.add(StandardMaterial { base_color: hex(color), unlit: true, ..default() })).clone()
    }
    pub fn emissive(&mut self, mats: &mut Assets<StandardMaterial>, color: u32) -> Handle<StandardMaterial> {
        self.mats
            .entry(color | 0x8000_0000)
            .or_insert_with(|| {
                let c = hex(color);
                mats.add(StandardMaterial { base_color: c, emissive: c.to_linear() * 1.2, perceptual_roughness: 0.9, ..default() })
            })
            .clone()
    }
}

/// A board-local direction turned into the course frame (the board may lie rotated); lengths are kept.
pub fn turn_vec(frame: &Frame, v: Vec3) -> Vec3 {
    let w = frame.to_world([v.x, v.z]);
    let o = frame.to_world([0.0, 0.0]);
    Vec3::new(((w[0] - o[0]) / BOARD_SCALE) as f32, v.y, ((w[1] - o[1]) / BOARD_SCALE) as f32)
}

/// Bevy units per board unit (board sizes and offsets are authored in board units).
pub const K: f32 = (BOARD_SCALE / 100.0) as f32;

// ---------------------------------------------------------------------------------------------
// Board props
// ---------------------------------------------------------------------------------------------

/// A flat strip from `a` to `b` (course positions): centre, length, yaw and pitch.
fn strip(a: [f64; 3], b: [f64; 3]) -> (Vec3, f32, Quat) {
    let (pa, pb) = (to_bevy(a), to_bevy(b));
    let d = pb - pa;
    let flat = Vec2::new(d.x, d.z).length();
    let yaw = d.x.atan2(d.z);
    let pitch = -(d.y).atan2(flat);
    ((pa + pb) / 2.0, d.length(), Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch))
}

#[allow(clippy::too_many_arguments)]
pub fn board_props(mut commands: Commands, mut app: ResMut<PartyApp>, mut cache: ResMut<ViewCache>, mut mats: ResMut<Assets<StandardMaterial>>, old: Query<Entity, With<BoardProp>>, assets: Res<AssetServer>, root: Res<crate::render::AssetRoot>) {
    if !matches!(app.screen, Screen::Title | Screen::Minis | Screen::Board | Screen::Results | Screen::Mini) {
        return;
    }
    let id = app.game.as_ref().map_or(app.cfg.board, |g| g.cfg.board);
    if app.board_built == Some(id) {
        return;
    }
    app.board_built = Some(id);
    for e in &old {
        commands.entity(e).despawn();
    }
    let board = board(id);
    let lift = 6.0;
    // Paths: strips that follow the course floor (one per path, split where the ground bends).
    for (a, edges) in board.edges.iter().enumerate() {
        for e in edges {
            let color = match e.rule {
                Rule::Always => 0xe8dcc0,
                Rule::Flip(_) => 0xb07a3c,
                Rule::Wind => 0x9fe3ff,
            };
            let m = cache.mat(&mut mats, color);
            let w = K * if e.rule == Rule::Always { 0.3 } else { 0.46 };
            let (pa, pb) = (board.spaces[a].pos, board.spaces[e.to].pos);
            let n = ((pb[0] - pa[0]).hypot(pb[1] - pa[1]) / 0.6).ceil().max(1.0) as usize;
            for k in 0..n {
                let t0 = k as f32 / n as f32;
                let t1 = (k + 1) as f32 / n as f32;
                let lerp = |t: f32| [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
                let (c, len, rot) = strip(app.board_world(lerp(t0), lift), app.board_world(lerp(t1), lift));
                commands.spawn((XpEntity, BoardProp, EdgeSeg { rule: e.rule }, Mesh3d(cache.cube.clone()), MeshMaterial3d(m.clone()), Transform::from_translation(c).with_rotation(rot).with_scale(Vec3::new(w, 0.06, len + 0.02))));
            }
        }
    }
    // Spaces: discs on the floor with a white trim.
    for (i, s) in board.spaces.iter().enumerate() {
        let m = cache.mat(&mut mats, s.kind.color());
        let r = K * if s.span { 0.5 } else { 0.62 };
        let at = to_bevy(app.board_world(s.pos, lift + 4.0));
        commands.spawn((XpEntity, BoardProp, SpaceDisc(i), Mesh3d(cache.cyl.clone()), MeshMaterial3d(m), Transform::from_translation(at).with_scale(Vec3::new(r, 0.16, r))));
        let trim = cache.mat(&mut mats, 0xffffff);
        commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cyl.clone()), MeshMaterial3d(trim), Transform::from_translation(at - Vec3::Y * 0.04).with_scale(Vec3::new(r + 0.08, 0.1, r + 0.08))));
    }
    // Keeper: the Sunstone floats over a Toad on its site.
    let kpos = board.spaces[board.sites[0]].pos;
    let keeper = commands.spawn((XpEntity, BoardProp, KeeperProp, Transform::from_translation(to_bevy(app.board_world(kpos, 20.0))), Visibility::Inherited)).id();
    if root.exists(STAR) {
        let mut star = crate::render::object_view::spawn_model(&mut commands, &assets, &root, STAR, true);
        star.insert((XpEntity, BoardProp, Transform::from_xyz(0.0, 2.3, 0.0).with_scale(Vec3::splat(1.3))));
        let sid = star.id();
        commands.entity(keeper).add_child(sid);
    }
    if root.exists(TOAD) {
        let mut toad = crate::render::object_view::spawn_model(&mut commands, &assets, &root, TOAD, true);
        toad.insert((XpEntity, BoardProp, Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(1.15))));
        let tid = toad.id();
        commands.entity(keeper).add_child(tid);
    }
    let beam = cache.emissive(&mut mats, 0xfff2a0);
    let b = commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cyl.clone()), MeshMaterial3d(beam), Transform::from_xyz(0.0, 4.0, 0.0).with_scale(Vec3::new(0.04, 8.0, 0.04)))).id();
    commands.entity(keeper).add_child(b);
    // Highlight rings for fork choices.
    for k in 0..4 {
        let m = cache.emissive(&mut mats, 0xffffff);
        commands.spawn((XpEntity, BoardProp, Highlight(k), Mesh3d(cache.cyl.clone()), MeshMaterial3d(m), Transform::default().with_scale(Vec3::new(0.9, 0.03, 0.9)), Visibility::Hidden));
    }
    // The ceremony's pedestals (gold, silver, bronze, plain), shown on the results screen.
    for (i, c) in [0xffcc33u32, 0xcfd6e6, 0xcd7f32, 0x7a86b8].iter().enumerate() {
        let m = cache.mat(&mut mats, *c);
        let at = to_bevy(app.board_world(podium_spot(i), lift + 8.0));
        commands.spawn((XpEntity, BoardProp, PodiumDisc, Mesh3d(cache.cyl.clone()), MeshMaterial3d(m), Transform::from_translation(at).with_scale(Vec3::new(0.62 * K, 0.2, 0.62 * K)), Visibility::Hidden));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn board_props_update(
    app: Res<PartyApp>,
    time: Res<Time>,
    mut cache: ResMut<ViewCache>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut discs: Query<(&SpaceDisc, &mut MeshMaterial3d<StandardMaterial>, &mut Transform), (Without<EdgeSeg>, Without<KeeperProp>, Without<Highlight>, Without<PodiumDisc>)>,
    mut edges: Query<(&EdgeSeg, &mut Visibility), (Without<Highlight>, Without<PodiumDisc>)>,
    mut keeper: Query<&mut Transform, (With<KeeperProp>, Without<SpaceDisc>, Without<Highlight>, Without<PodiumDisc>)>,
    mut hl: Query<(&Highlight, &mut Visibility, &mut Transform), (Without<SpaceDisc>, Without<KeeperProp>, Without<EdgeSeg>, Without<PodiumDisc>)>,
    mut podium: Query<&mut Visibility, (With<PodiumDisc>, Without<SpaceDisc>, Without<KeeperProp>, Without<EdgeSeg>, Without<Highlight>)>,
) {
    let Some(id) = app.board_built else { return };
    let board = board(id);
    let cond = app.game.as_ref().map_or(Conditions::default(), |g| g.cond);
    let t = time.elapsed_secs();
    let keeper_space = app.game.as_ref().map_or(board.sites[0], |g| g.keeper);
    let options: Vec<usize> = match app.game.as_ref().map(|g| &g.phase) {
        Some(Phase::Branch(o)) => o.clone(),
        _ => vec![],
    };
    for (d, mut mat, mut tf) in &mut discs {
        let flooded = board.flooded(d.0, &cond);
        let kind = if flooded { Kind::Pit } else { board.spaces[d.0].kind };
        let color = if kind == Kind::Keeper && d.0 != keeper_space { 0xb59a3a } else if flooded { 0xff5a1a } else { kind.color() };
        let h = if flooded { cache.emissive(&mut mats, color) } else { cache.mat(&mut mats, color) };
        if mat.0 != h {
            mat.0 = h;
        }
        let pulse = app.pulse.filter(|p| p.0 == d.0).map_or(0.0, |p| (1.0 - ((app.t - p.1) / 0.5) as f32).max(0.0));
        let r = K * if board.spaces[d.0].span { 0.5 } else { 0.62 };
        tf.scale = Vec3::new(r * (1.0 + 0.25 * pulse), 0.16 + 0.1 * pulse, r * (1.0 + 0.25 * pulse));
    }
    for (e, mut vis) in &mut edges {
        let open = match e.rule {
            Rule::Always => true,
            Rule::Flip(f) => cond.flip == f,
            Rule::Wind => cond.wind,
        };
        let v = if open { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != v {
            *vis = v;
        }
    }
    let want = to_bevy(app.board_world(board.spaces[keeper_space].pos, 20.0));
    for mut tf in &mut keeper {
        tf.translation = tf.translation.lerp(want, (time.delta_secs() * 4.0).min(1.0));
    }
    for (h, mut vis, mut tf) in &mut hl {
        match options.get(h.0) {
            Some(&s) => {
                tf.translation = to_bevy(app.board_world(board.spaces[s].pos, 14.0));
                let k = 1.0 + 0.12 * (t * 7.0).sin();
                tf.scale = Vec3::new(0.95 * K * k, 0.03, 0.95 * K * k);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    let show = app.screen == Screen::Results;
    for mut vis in &mut podium {
        let v = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != v {
            *vis = v;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mini-game props
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn stage_shapes(
    mut commands: Commands,
    app: Res<PartyApp>,
    mut cache: ResMut<ViewCache>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut pool: Query<(Entity, &StageShape, &mut Transform, &mut Visibility, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>)>,
    mut spawned: Local<usize>,
) {
    let shapes: &[Shape] = match (&app.screen, &app.mini) {
        (Screen::Mini, Some(m)) => &m.scene.shapes,
        _ => &[],
    };
    while *spawned < shapes.len().min(400) {
        commands.spawn((XpEntity, StageShape(*spawned), Mesh3d(cache.cube.clone()), MeshMaterial3d(cache.mat(&mut mats, 0xffffff)), Transform::default(), Visibility::Hidden));
        *spawned += 1;
    }
    for (_, s, mut tf, mut vis, mut mesh, mut mat) in &mut pool {
        let Some(sh) = shapes.get(s.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let (m, scale, rot) = match sh.sh {
            Sh::Box => (cache.cube.clone(), Vec3::new(sh.s[0], sh.s[1], sh.s[2]), Quat::from_rotation_y(sh.yaw)),
            Sh::Disc => (cache.cyl.clone(), Vec3::new(sh.s[0], sh.s[1].max(0.01), if sh.s[2] > 0.0 { sh.s[2] } else { sh.s[0] }), Quat::from_rotation_y(sh.yaw)),
            Sh::Ball => (cache.ball.clone(), Vec3::splat(sh.s[0]), Quat::IDENTITY),
            Sh::Cone => (cache.cone.clone(), Vec3::new(sh.s[0], sh.s[1], sh.s[0]), Quat::from_rotation_y(sh.yaw)),
            Sh::Coin => (cache.cyl.clone(), Vec3::new(sh.s[0], sh.s[1], sh.s[0]), Quat::from_rotation_y(sh.yaw) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        };
        if mesh.0 != m {
            mesh.0 = m;
        }
        let h = if sh.sh == Sh::Coin { cache.emissive(&mut mats, sh.col) } else { cache.mat(&mut mats, sh.col) };
        if mat.0 != h {
            mat.0 = h;
        }
        tf.translation = to_bevy(sh.p);
        tf.rotation = rot;
        tf.scale = scale;
        *vis = Visibility::Inherited;
    }
}

/// The real models of the mini-games (Goombas, falling Bob-ombs, the Power Star, the crown star), pooled.
#[allow(clippy::too_many_arguments)]
pub fn mini_models(mut commands: Commands, app: Res<PartyApp>, assets: Res<AssetServer>, root: Res<crate::render::AssetRoot>, time: Res<Time>, mut pool: Query<(&MiniModel, &mut Transform, &mut Visibility)>, mut counts: Local<[usize; 4]>) {
    let scene = match (&app.screen, &app.mini) {
        (Screen::Mini, Some(m)) => Some(&m.scene),
        _ => None,
    };
    let piece_head = |p: usize| app.pieces.get(p).map(|pc| to_bevy(pc.mario.pos()) + Vec3::Y * 2.3);
    let mut want: [Vec<(Vec3, f32, f32)>; 4] = Default::default();
    if let Some(s) = scene {
        want[0] = s.goombas.iter().map(|(p, yaw, k)| (to_bevy(*p), *yaw, *k)).collect();
        want[1] = s.bombs.iter().map(|p| (to_bevy(*p), 0.0, 1.0)).collect();
        let spin = time.elapsed_secs() * 3.0;
        if let Some(p) = s.star_at {
            want[2].push((to_bevy(p), spin, 1.0));
        }
        if let Some(h) = s.star_on.and_then(piece_head) {
            want[2].push((h, spin, 0.8));
        }
        if let Some(h) = s.crown.and_then(piece_head) {
            want[3].push((h, spin, 0.6));
        }
    }
    let paths = [GOOMBA, BOBOMB, STAR, STAR];
    for kind in 0..4 {
        while counts[kind] < want[kind].len() && root.exists(paths[kind]) {
            let mut e = crate::render::object_view::spawn_model(&mut commands, &assets, &root, paths[kind], true);
            e.insert((XpEntity, MiniModel(kind as u8, counts[kind]), Visibility::Hidden));
            counts[kind] += 1;
        }
    }
    for (m, mut tf, mut vis) in &mut pool {
        match want[m.0 as usize].get(m.1) {
            Some((p, yaw, k)) => {
                tf.translation = *p;
                tf.rotation = Quat::from_rotation_y(*yaw);
                tf.scale = match m.0 {
                    0 => Vec3::new(1.0, *k, 1.0),
                    _ => Vec3::splat(*k),
                };
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------------------------

pub fn ensure_pieces(mut commands: Commands, mut app: ResMut<PartyApp>, assets: Res<AssetServer>) {
    if app.pieces.len() >= 4 || app.meta.is_none() {
        return;
    }
    let meta = app.meta.clone().unwrap();
    let world = app.world.clone();
    for p in app.pieces.len()..4 {
        let pos = app.board_world([-1.5 + p as f32, 0.0], 0.0);
        let spawn = jumpman_sim::mario_runtime::SpawnPoint { action: None, pos, face_yaw: 0.0 };
        let mut m = CrowdMario::new(world.clone(), spawn, Some(meta.as_slice()));
        m.runtime.reset_to_spawn();
        m.sync_previous();
        m.entity = Some(spawn_body(&mut commands, &assets, LOOKS[p]));
        app.pieces.push(Piece { mario: m, at: None, snap: true, jump_next: false, air_prev: false, target: Some(pos), hold_a: false, hidden: true, fx_t: 0.0 });
    }
}

/// Decide each piece's target from the current screen.
pub fn piece_targets(mut app: ResMut<PartyApp>) {
    if app.pieces.len() < 4 {
        return;
    }
    let screen = app.screen;
    let place = app.place;
    let board_id = app.game.as_ref().map_or(app.cfg.board, |g| g.cfg.board);
    let board = board(board_id);
    let t = app.t;
    match screen {
        Screen::Title | Screen::Minis => {
            for p in 0..4 {
                let target = app.board_world([-1.5 + p as f32, 0.6], 0.0);
                let pc = &mut app.pieces[p];
                pc.mario.size = 1.0;
                pc.hidden = false;
                if place || pc.at.is_some() {
                    pc.snap = true;
                    pc.at = None;
                }
                pc.target = Some(target);
            }
            app.place = false;
        }
        Screen::Board => {
            let snaps: Vec<(usize, usize)> = app.game.as_ref().map(|g| (0..4).map(|p| (p, g.players[p].space)).collect()).unwrap_or_default();
            for (p, s) in snaps {
                if place || app.pieces[p].at.is_none() {
                    app.pieces[p].at = Some(s);
                    app.pieces[p].snap = true;
                }
                let sp = board.spaces[app.pieces[p].at.unwrap()].pos;
                let off = slot_offset(p);
                let target = app.board_world([sp[0] + off[0], sp[1] + off[1]], 0.0);
                let pc = &mut app.pieces[p];
                pc.mario.size = 1.0;
                pc.hidden = false;
                pc.target = Some(target);
            }
            app.place = false;
        }
        Screen::Mini => {
            let Some(run) = app.mini.as_ref() else { return };
            let playing = run.stage == MStage::Play;
            let starts: Vec<[f64; 3]> = (0..4).map(|p| run.session.game.start(p)).collect();
            let shown: Vec<bool> = (0..4).map(|p| run.session.active[p] && !run.session.game.out(p)).collect();
            for p in 0..4 {
                let pc = &mut app.pieces[p];
                pc.at = None;
                pc.mario.size = 1.0;
                pc.hidden = !shown[p];
                if place {
                    pc.snap = true;
                    pc.target = Some(starts[p]);
                } else if playing {
                    // The physics (and the rules) move them now.
                    pc.target = None;
                }
            }
            app.place = false;
        }
        Screen::Results => {
            let order = app.game.as_ref().map(|g| g.standings()).unwrap_or_else(|| vec![0, 1, 2, 3]);
            let winners = app.game.as_ref().map(|g| g.winners()).unwrap_or_default();
            for (rank, &p) in order.iter().enumerate() {
                let target = app.board_world(podium_spot(rank), 30.0);
                let pc = &mut app.pieces[p];
                pc.mario.size = 1.0;
                pc.hidden = false;
                pc.target = Some(target);
                if place {
                    pc.snap = true;
                }
                // Winners cheer: a hop every second or so.
                if winners.contains(&p) && ((t * 1.1 + p as f64 * 0.3).fract() < 0.04) {
                    pc.jump_next = true;
                }
            }
            app.place = false;
        }
    }
}

/// What a physics body is doing, for the rules.
pub fn body_of(m: &MarioRuntime) -> Body {
    Body { pos: m.pos, yaw: m.face_angle[1], grounded: m.action & ACT_FLAG_AIR == 0, vy: m.vel[1], attack: m.action & ACT_FLAG_ATTACKING != 0 }
}

/// Keep a piece out of the course's solid objects (trees): the crowd Marios step on the course collision only, which has no
/// trunks in it, so they are pushed out of each trunk's cylinder after the step (onto floor only), as SM64 pushes Mario.
pub fn keep_out_of_solids(m: &mut MarioRuntime, solids: &[Solid]) {
    if let Some(q) = push_out(solids, m.pos) {
        if m.world.find_floor(q[0], q[1] + 100.0, q[2]).surface.is_some() {
            m.pos[0] = q[0];
            m.pos[2] = q[2];
            // Running into a trunk stops the run into it (no sliding round at full speed).
            m.forward_vel = m.forward_vel.min(12.0);
        }
    }
}

/// The four Marios of a mini-game do not pass through each other: overlapping bodies are pushed apart (half each), onto
/// floor only and never into a tree (the crowd physics steps each body alone, so it does not do this itself).
pub fn keep_apart(pieces: &mut [Piece], live: &[bool; 4], solids: &[Solid]) {
    if pieces.len() < 4 {
        return;
    }
    let pos: [[f64; 3]; 4] = std::array::from_fn(|p| pieces[p].mario.runtime.pos);
    let moved = super::minis::field::separate_bodies(&pos, live);
    for p in 0..4 {
        if moved[p] == [0.0, 0.0] {
            continue;
        }
        let m = &mut pieces[p].mario.runtime;
        let q = [m.pos[0] + moved[p][0], m.pos[1], m.pos[2] + moved[p][1]];
        let floor = m.world.find_floor(q[0], q[1] + 100.0, q[2]);
        if floor.surface.is_some() && (floor.height - m.pos[1]).abs() < 120.0 || m.action & ACT_FLAG_AIR != 0 && floor.surface.is_some() {
            if push_out(solids, q).is_none() {
                m.pos[0] = q[0];
                m.pos[2] = q[2];
            }
        }
    }
}

/// A rules drive as synthetic controller input.
pub fn drive_input(d: &Drive) -> DriveInput {
    DriveInput { yaw: d.dir.map(|v| atan2s(v[1], v[0]) as f64), mag: Some(d.mag.clamp(0.0, 1.0) * 64.0), jump: d.jump, hold_a: d.hold || d.jump, punch: d.punch, ..Default::default() }
}

/// A rules knockback on a physics body: the real knockback action, then a harder shove (as Last Star Standing does).
pub fn knock(m: &mut MarioRuntime, from: [f64; 3], power: f64) {
    m.invinc_timer = 0.0;
    if m.take_damage(2.0, Some(from)) {
        m.forward_vel = m.forward_vel.signum() * m.forward_vel.abs().max(power * 1.15);
        m.vel[1] = m.vel[1].max(24.0);
        if m.action & ACT_FLAG_AIR == 0 {
            m.set_action(if m.forward_vel < 0.0 { ACT_BACKWARD_AIR_KB } else { ACT_FORWARD_AIR_KB }, 0.0);
        }
    }
    m.health = 0x880 as f64;
    m.hurt_counter = 0.0;
}

/// One 30 Hz step of a running mini-game on the full physics: read the bodies, plan every seat's drive (people's sticks,
/// bots' policies), step the four Marios, then let the rules see where they ended up and apply what they ask for.
pub fn mini_physics_step(app: &mut PartyApp) -> bool {
    let playing = app.screen == Screen::Mini && app.mini.as_ref().is_some_and(|m| m.stage == MStage::Play && !m.session.done());
    if !playing || app.pieces.len() < 4 {
        return false;
    }
    let PartyApp { mini, pieces, cues, .. } = app;
    let run = mini.as_mut().unwrap();
    let human: [Drive; 4] = std::array::from_fn(|p| run.human_drive(p));
    for i in &mut run.inp {
        i.act = false;
        i.b = false;
    }
    step_session(&mut run.session, pieces, &human, cues);
    true
}

/// [`mini_physics_step`] without the app: one step of `session` on the four pieces' physics bodies.
pub fn step_session(session: &mut Session, pieces: &mut [Piece], human: &[Drive; 4], cues: &mut Vec<(&'static str, [f64; 3])>) {
    let bodies: [Body; 4] = std::array::from_fn(|p| body_of(&pieces[p].mario.runtime));
    let drives = session.plan(&bodies, human);
    for p in 0..4 {
        let shown = session.active[p] && !session.game.out(p);
        pieces[p].hidden = !shown;
        let input = if shown { drive_input(&drives[p]) } else { DriveInput::default() };
        pieces[p].mario.drive(&input);
        keep_out_of_solids(&mut pieces[p].mario.runtime, &session.field.solids);
    }
    keep_apart(pieces, &std::array::from_fn(|p| session.active[p] && !session.game.out(p)), &session.field.solids);
    let after: [Body; 4] = std::array::from_fn(|p| body_of(&pieces[p].mario.runtime));
    for ev in session.advance(&after, &drives) {
        match ev {
            MiniEv::Knock { p, from, power } => knock(&mut pieces[p].mario.runtime, from, power),
            MiniEv::Place { p, pos } => pieces[p].mario.teleport(pos, None),
            MiniEv::Bounce { p } => {
                let m = &mut pieces[p].mario.runtime;
                m.vel[1] = m.vel[1].max(42.0);
            }
            MiniEv::Cue { kind, pos } => cues.push((kind, pos)),
        }
    }
    // A Mario put back on the field may land on another: part them again.
    keep_apart(pieces, &std::array::from_fn(|p| session.active[p] && !session.game.out(p)), &session.field.solids);
}

pub fn drive_pieces(mut app: ResMut<PartyApp>) {
    if mini_physics_step(&mut app) {
        return;
    }
    let PartyApp { pieces, solids, .. } = &mut *app;
    for pc in pieces.iter_mut() {
        let pos = pc.mario.runtime.pos;
        let mut input = DriveInput::default();
        if let Some(t) = pc.target {
            if pc.snap {
                pc.mario.teleport(t, None);
                pc.snap = false;
                pc.mario.drive(&input);
                continue;
            }
            let (dx, dz) = (t[0] - pos[0], t[2] - pos[2]);
            let d = dx.hypot(dz);
            // Far off (a warp, or stuck on the course): put it there.
            if d > 1400.0 || (pos[1] - t[1]).abs() > 400.0 {
                pc.mario.teleport(t, None);
                continue;
            }
            if d > 40.0 {
                input.yaw = Some(atan2s(dz, dx) as f64);
                input.mag = Some(if d > 160.0 { 64.0 } else { (d / 2.5).clamp(26.0, 64.0) });
            }
        }
        if pc.jump_next {
            input.jump = true;
            pc.jump_next = false;
        }
        input.hold_a = pc.hold_a;
        pc.mario.drive(&input);
        keep_out_of_solids(&mut pc.mario.runtime, solids);
    }
}

pub fn render_pieces(mut commands: Commands, mut app: ResMut<PartyApp>, time: Res<Time>, fixed: Res<Time<Fixed>>, cams: Query<&Transform, With<crate::camera::MainCamera>>) {
    let alpha = fixed.overstep_fraction() as f64;
    let eye = cams.single().map(|t| [t.translation.x as f64 / 0.01, t.translation.y as f64 / 0.01, t.translation.z as f64 / 0.01]).unwrap_or([0.0; 3]);
    let dt = time.delta_secs_f64();
    for pc in &mut app.pieces {
        let p = pc.mario.pos();
        let d = ((eye[0] - p[0]).powi(2) + (eye[1] - p[1]).powi(2) + (eye[2] - p[2]).powi(2)).sqrt();
        pc.mario.render(&mut commands, alpha, dt, d);
        if let Some(e) = pc.mario.entity {
            commands.entity(e).insert(if pc.hidden { Visibility::Hidden } else { Visibility::Inherited });
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Cameras
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct CamState {
    eye: Option<Vec3>,
    look: Option<Vec3>,
    screen: Option<Screen>,
}

/// Where the mini-game camera looks from: behind the players along the site's `look`, far enough to frame them all.
pub fn mini_cam(run: &MiniRun, field: &Field, bodies: &[[f64; 3]], aspect: f32) -> (Vec3, Vec3) {
    let alive: Vec<[f64; 3]> = bodies.iter().copied().filter(|b| !field.fallen(*b)).collect();
    let (c, spread) = if run.stage == MStage::Play && !alive.is_empty() {
        let n = alive.len() as f64;
        let c = [alive.iter().map(|b| b[0]).sum::<f64>() / n, alive.iter().map(|b| b[1]).sum::<f64>() / n, alive.iter().map(|b| b[2]).sum::<f64>() / n];
        // Pull toward the arena centre so the action keeps its context.
        let c = [c[0] * 0.7 + field.center[0] * 0.3, c[1] * 0.7 + field.center[1] * 0.3, c[2] * 0.7 + field.center[2] * 0.3];
        (c, alive.iter().map(|b| hdist(*b, c)).fold(0.0, f64::max))
    } else {
        (field.center, field.radius * 0.6)
    };
    let f = run.site.look;
    let d = ((spread * 1.6 + 1100.0) / (aspect as f64 / 1.78).min(1.0)).clamp(1500.0, 3200.0) / 100.0;
    let cv = to_bevy(c);
    let back = Vec3::new(-f[0] as f32, 0.0, -f[1] as f32);
    (cv + back * d as f32 * 0.55 + Vec3::Y * d as f32 * 0.82, cv + Vec3::Y * 0.6 - back * 1.5)
}

pub fn camera(mut app: ResMut<PartyApp>, mut cam: ResMut<CameraOverride>, time: Res<Time>, windows: Query<&Window, With<PrimaryWindow>>, mut st: Local<CamState>) {
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let aspect = (win.x / win.y.max(1.0)).max(0.5);
    let fov = 36.0f32;
    let tan_h = (fov.to_radians() / 2.0).tan();
    let frame = app.ground().frame;
    let o = to_bevy(app.board_world([0.0, 0.0], 0.0));
    // Views of the board from its south side (board-local +z), turned with the board.
    let view = |d_up: f32, d_back: f32| turn_vec(&frame, Vec3::new(0.0, d_up, d_back));
    let (eye_t, look_t) = match app.screen {
        Screen::Title | Screen::Minis => {
            let sway = (app.t as f32 * 0.25).sin();
            let d = (14.0 / (tan_h * aspect)).max(24.0) * K;
            (o + view(d * 0.62, d * 0.72) + turn_vec(&frame, Vec3::new(sway * 3.0, 0.0, 0.0)), o + view(0.0, 0.5))
        }
        Screen::Board => {
            let focus_p = app.game.as_ref().map_or(0, |g| g.cur);
            if app.overview {
                let d = (14.5 / (tan_h * aspect)).max(26.0) * K;
                (o + view(d * 0.8, d * 0.58), o + view(0.0, 0.6))
            } else {
                // The piece, unless it is still far from where it is going (just back from a mini-game, or warped).
                let p = app.pieces.get(focus_p).map_or(o, |pc| {
                    let pos = pc.mario.pos();
                    match pc.target {
                        Some(t) if pc.snap || hdist(pos, t) > 600.0 || (pos[1] - t[1]).abs() > 400.0 => to_bevy(t),
                        _ => to_bevy(pos),
                    }
                });
                let d = (11.5 / (tan_h * aspect)).max(17.0);
                (p + view(d * 0.82, d * 0.57), p + view(0.3, -0.9))
            }
        }
        Screen::Mini => match app.mini.as_ref() {
            Some(run) => {
                let bodies: Vec<[f64; 3]> = (0..4).filter(|&p| run.session.active[p] && !run.session.game.out(p)).filter_map(|p| app.pieces.get(p).map(|pc| pc.mario.pos())).collect();
                mini_cam(run, &run.session.field, &bodies, aspect)
            }
            None => (o + view(20.0, 20.0), o),
        },
        Screen::Results => {
            let p = to_bevy(app.board_world([0.6, 0.3], 0.0));
            (p + view(3.4, 9.0), p + view(1.0, 0.0))
        }
    };
    let switched = st.screen != Some(app.screen);
    st.screen = Some(app.screen);
    let snap = st.eye.is_none() || switched && matches!(app.screen, Screen::Mini | Screen::Results | Screen::Board) && (st.eye.unwrap() - eye_t).length() > 14.0;
    let k = if snap { 1.0 } else { (time.delta_secs() * if app.screen == Screen::Board { 3.2 } else { 2.2 }).min(1.0) };
    let eye = st.eye.map_or(eye_t, |e| e.lerp(eye_t, k));
    let look = st.look.map_or(look_t, |l| l.lerp(look_t, k));
    st.eye = Some(eye);
    st.look = Some(look);
    let shake = app.shake;
    let jitter = Vec3::new((app.t as f32 * 70.0).sin(), (app.t as f32 * 53.0).cos(), 0.0) * shake * 0.12;
    app.confetti = app.confetti.max(0.0);
    let s = 100.0;
    cam.pose = Some(CamPose { eye: DVec3::new((eye.x + jitter.x) as f64 * s, (eye.y + jitter.y) as f64 * s, eye.z as f64 * s), look: DVec3::new(look.x as f64 * s, look.y as f64 * s, look.z as f64 * s), up: DVec3::Y, fov: fov as f64, near: 20.0, far: 60000.0 });
}

#[cfg(test)]
mod physics_tests {
    use super::*;
    use crate::game::world::{CourseRequest, LoadedCourse};
    use std::sync::Arc;

    /// Play one mini-game to the end on the real battlefield: four full-physics Marios (the app's own step), Normal bots.
    fn play(course: &LoadedCourse, kind: Mini, seed: u64, levels: [Level; 4]) -> ([f32; 4], usize) {
        let solids = super::super::sites::solids(&course.objects.objects);
        let field = Arc::new(super::super::sites::sample_field(&course.world, kind).with_solids(&solids));
        let mut session = Session::new(kind, seed, [true; 4], [false; 4], levels, field);
        let mut pieces: Vec<Piece> = (0..4)
            .map(|p| {
                let pos = session.game.start(p);
                let spawn = jumpman_sim::mario_runtime::SpawnPoint { action: None, pos, face_yaw: 0.0 };
                let mut m = CrowdMario::new(course.world.clone(), spawn, None);
                m.runtime.reset_to_spawn();
                Piece { mario: m, at: None, snap: false, jump_next: false, air_prev: false, target: None, hold_a: false, hidden: false, fx_t: 0.0 }
            })
            .collect();
        let mut cues = vec![];
        let trees = session.field.solids.clone();
        let mut near_trees = 0;
        let mut worst = 0.0f64;
        while !session.done() && session.steps < 30 * 60 {
            step_session(&mut session, &mut pieces, &[Drive::default(); 4], &mut cues);
            // No two Marios stand inside each other (body radius each, a little tolerance for a shove that a wall held back).
            for a in 0..4 {
                for b in a + 1..4 {
                    if pieces[a].hidden || pieces[b].hidden {
                        continue;
                    }
                    let (p, q) = (pieces[a].mario.pos(), pieces[b].mario.pos());
                    if (p[1] - q[1]).abs() < 100.0 {
                        worst = worst.max(2.0 * BODY_R - hdist(p, q));
                    }
                }
            }
            for pc in &pieces {
                let p = pc.mario.pos();
                for t in trees.iter().filter(|t| p[1] > t.pos[1] - 120.0 && p[1] < t.pos[1] + t.h) {
                    let d = hdist(p, t.pos);
                    near_trees += (d < t.r + BODY_R + 80.0) as usize;
                    assert!(d >= t.r + BODY_R - 1.0, "{kind:?}: a Mario inside the tree at {:?} ({d:.0} from it)", t.pos);
                }
            }
        }
        if !trees.is_empty() {
            eprintln!("{kind:?}: {} trees in the arena, {near_trees} body-steps close to one", trees.len());
        }
        assert!(session.done(), "{kind:?} ended");
        eprintln!("{kind:?}: worst overlap between two Marios {worst:.1}");
        assert!(worst < 15.0, "{kind:?}: two Marios overlapped by {worst:.1}");
        (session.game.scores(), cues.len())
    }

    /// Every mini-game plays out on the real course with real Mario physics: the bots score, the rules see real
    /// collisions (coins taken, punches landed, blasts dodged and taken, stomps), and the same seed plays the same game.
    #[test]
    fn every_mini_game_plays_on_the_real_battlefield_with_mario_physics() {
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !assets.join("courses/bob.json").exists() {
            return;
        }
        let course = LoadedCourse::load(&assets, CourseRequest::new("bob")).expect("course loads");
        for kind in ALL_MINIS {
            let (s, cues) = play(&course, kind, 7, [Level::Normal; 4]);
            eprintln!("{kind:?}: scores {s:?}, {cues} cues");
            let total: f32 = s.iter().sum();
            let least = s.iter().cloned().fold(f32::MAX, f32::min);
            let most = s.iter().cloned().fold(f32::MIN, f32::max);
            assert!(cues > 3, "{kind:?}: nothing happened");
            match kind {
                Mini::CoinScramble => assert!(total >= 20.0, "coins taken: {total}"),
                Mini::SummitCrown => assert!(total >= 5.0, "seconds held alone: {total}"),
                // Someone took a blast: not everyone ends with the full clock and three hearts.
                Mini::BombRain => assert!(least < bomb_rain::SECS + 6.0 - 0.5, "nobody was hit: {s:?}"),
                Mini::StarTag => assert!(total >= 15.0, "seconds the star was held: {total}"),
                Mini::GoombaStomp => assert!(total >= 8.0, "stomps: {total}"),
            }
            assert!(most > least, "{kind:?} is a dead heat: {s:?}");
        }
        // Deterministic by seed, physics included.
        assert_eq!(play(&course, Mini::StarTag, 3, [Level::Normal; 4]), play(&course, Mini::StarTag, 3, [Level::Normal; 4]));
    }
}
