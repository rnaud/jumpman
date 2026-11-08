//! Sunstone Shindig in 3D: the board props, the mini-game stage (a pooled set of coloured primitives), the four Mario pieces
//! (real SM64 rigs with their animation clips, driven by `CrowdMario`) and the cameras.
use super::app::*;
use super::board::*;
use super::common::*;
use super::game::*;
use super::minis::*;
use crate::experiments::crowd::{spawn_body, CrowdLook, CrowdMario};
use crate::experiments::{CamPose, CameraOverride, Host, XpEntity};
use crate::render::{scene as scene_handle, to_bevy};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jumpman_sim::math::atan2s;
use std::collections::HashMap;

pub const LOOKS: [CrowdLook; 4] = [
    CrowdLook { cap: 0xe23b2e, overalls: Some(0x2d4fd6) },
    CrowdLook { cap: 0x3d7bff, overalls: Some(0xf0f0ff) },
    CrowdLook { cap: 0x2fbf5a, overalls: Some(0x1d5b30) },
    CrowdLook { cap: 0xf5c518, overalls: Some(0x7a4a14) },
];

const STAR: &str = "actors/star/star_geo.glb";
const TOAD: &str = "actors/toad/toad_geo.glb";
const TREES: [&str; 3] = ["actors/tree/bubbly_tree_geo.glb", "actors/tree/palm_tree_geo.glb", "actors/tree/snow_tree_geo.glb"];

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
pub struct LavaPlane;

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
        self.mats.entry(color).or_insert_with(|| mats.add(StandardMaterial { base_color: hex(color), perceptual_roughness: 0.85, ..default() })).clone()
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

/// The parts that never change: the mini-game stage floor and the ceremony podium.
pub fn build_static(host: &mut Host) {
    let mini = host.lit(hex(0x39456a));
    host.spawn_box([MINI_O[0], MINI_O[1] - 20.0, MINI_O[2]], [2000.0, 40.0, 1100.0], 0.0, mini);
    let rim = host.lit(hex(0x1d2442));
    host.spawn_box([MINI_O[0], MINI_O[1] - 60.0, MINI_O[2]], [2100.0, 40.0, 1200.0], 0.0, rim);
    let floor = host.lit(hex(0x3a3f70));
    host.spawn_box([PODIUM_O[0], PODIUM_O[1] - 20.0, PODIUM_O[2]], [1700.0, 40.0, 800.0], 0.0, floor);
    let cols = [0xffcc33, 0xcfd6e6, 0xcd7f32, 0x7a86b8];
    for i in 0..4 {
        let (p, h) = podium_slot(i);
        let m = host.lit(hex(cols[i]));
        host.spawn_box([p[0], p[1] + h / 2.0 - 2.0, p[2]], [150.0, h, 150.0], 0.0, m);
    }
    // A key light for the whole thing (the course's own light is hidden with the course).
    host.spawn((DirectionalLight { illuminance: 11000.0, shadows_enabled: false, ..default() }, Transform::from_xyz(0.0, 10.0, 0.0).looking_to(Vec3::new(-0.35, -1.0, -0.45), Vec3::Y)));
}

// ---------------------------------------------------------------------------------------------
// Board props
// ---------------------------------------------------------------------------------------------

fn seg(a: [f32; 2], b: [f32; 2]) -> (Vec3, f32, f32) {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dz);
    (Vec3::new((a[0] + b[0]) / 2.0, 0.0, (a[1] + b[1]) / 2.0), len, dx.atan2(dz))
}

#[allow(clippy::too_many_arguments)]
pub fn board_props(
    mut commands: Commands,
    mut app: ResMut<PartyApp>,
    mut cache: ResMut<ViewCache>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    old: Query<Entity, With<BoardProp>>,
    assets: Res<AssetServer>,
    root: Res<crate::render::AssetRoot>,
    mut palette: ResMut<crate::fx::sky::SkyPalette>,
    mut clear: ResMut<ClearColor>,
) {
    if !matches!(app.screen, Screen::Title | Screen::Minis | Screen::Board | Screen::Results) {
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
    let origin = to_bevy(BOARD_O);
    let want = crate::fx::sky::SkyPalette::from_hex(board.sky[0], board.sky[1], board.sky[2]).with_clouds(0.4);
    *palette = want;
    clear.0 = hex(board.sky[0]);
    // Ground slab and a darker rim.
    let ground = cache.mat(&mut mats, board.ground);
    let rim = cache.mat(&mut mats, board.ground.wrapping_sub(0x1a1a1a) & 0xffffff);
    commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cube.clone()), MeshMaterial3d(ground), Transform::from_translation(origin + Vec3::new(0.0, -0.2, 0.0)).with_scale(Vec3::new(34.0, 0.4, 19.0))));
    commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cube.clone()), MeshMaterial3d(rim), Transform::from_translation(origin + Vec3::new(0.0, -0.45, 0.0)).with_scale(Vec3::new(35.0, 0.2, 20.0))));
    if board.gimmick == Gimmick::Tide {
        let lava = cache.emissive(&mut mats, 0xff5a1a);
        commands.spawn((XpEntity, BoardProp, LavaPlane, Mesh3d(cache.cube.clone()), MeshMaterial3d(lava), Transform::from_translation(origin + Vec3::new(0.0, 0.03, 0.0)).with_scale(Vec3::new(33.0, 0.04, 18.0))));
        // Rings of rock under each tier.
        for (rx, rz, c) in [(13.6f32, 7.4f32, 0x5a3a30u32), (8.9, 4.7, 0x6e463a), (4.4, 2.5, 0x82554a)] {
            let m = cache.mat(&mut mats, c);
            commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cyl.clone()), MeshMaterial3d(m), Transform::from_translation(origin + Vec3::new(0.0, 0.0, 0.0)).with_scale(Vec3::new(rx, 0.12, rz))));
        }
    }
    if board.gimmick == Gimmick::Wind {
        for i in 0..7 {
            let (x, z) = (-13.0 + i as f32 * 4.4, if i % 2 == 0 { -8.2 } else { 8.0 });
            let m = cache.mat(&mut mats, 0xffffff);
            commands.spawn((XpEntity, BoardProp, Mesh3d(cache.ball.clone()), MeshMaterial3d(m), Transform::from_translation(origin + Vec3::new(x, -0.7, z)).with_scale(Vec3::new(2.6, 0.9, 1.6))));
        }
    }
    // Paths.
    for (a, edges) in board.edges.iter().enumerate() {
        for e in edges {
            let (c, len, yaw) = seg(board.spaces[a].pos, board.spaces[e.to].pos);
            let color = match e.rule {
                Rule::Always => 0xd9d2c2,
                Rule::Flip(_) => 0xb07a3c,
                Rule::Wind => 0x9fe3ff,
            };
            let m = cache.mat(&mut mats, color);
            let w = if e.rule == Rule::Always { 0.34 } else { 0.5 };
            commands.spawn((XpEntity, BoardProp, EdgeSeg { rule: e.rule }, Mesh3d(cache.cube.clone()), MeshMaterial3d(m), Transform::from_translation(origin + c + Vec3::Y * 0.04).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::new(w, 0.08, len))));
        }
    }
    // Spaces.
    for (i, s) in board.spaces.iter().enumerate() {
        let m = cache.mat(&mut mats, s.kind.color());
        let r = if s.span { 0.5 } else { 0.62 };
        commands.spawn((XpEntity, BoardProp, SpaceDisc(i), Mesh3d(cache.cyl.clone()), MeshMaterial3d(m), Transform::from_translation(origin + Vec3::new(s.pos[0], 0.1, s.pos[1])).with_scale(Vec3::new(r, 0.16, r))));
        let trim = cache.mat(&mut mats, 0xffffff);
        commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cyl.clone()), MeshMaterial3d(trim), Transform::from_translation(origin + Vec3::new(s.pos[0], 0.06, s.pos[1])).with_scale(Vec3::new(r + 0.08, 0.1, r + 0.08))));
    }
    // Keeper: the Sunstone floats over a toad on its site.
    let kpos = board.spaces[board.sites[0]].pos;
    let keeper = commands.spawn((XpEntity, BoardProp, KeeperProp, Transform::from_translation(origin + Vec3::new(kpos[0], 0.2, kpos[1])), Visibility::Inherited)).id();
    if root.exists(STAR) {
        let mut star = crate::render::object_view::spawn_model(&mut commands, &assets, &root, STAR, true);
        star.insert((XpEntity, BoardProp, Transform::from_xyz(0.0, 2.2, 0.0).with_scale(Vec3::splat(2.2))));
        let sid = star.id();
        commands.entity(keeper).add_child(sid);
    }
    if root.exists(TOAD) {
        let mut toad = crate::render::object_view::spawn_model(&mut commands, &assets, &root, TOAD, true);
        toad.insert((XpEntity, BoardProp, Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(1.5))));
        let tid = toad.id();
        commands.entity(keeper).add_child(tid);
    }
    let beam = cache.emissive(&mut mats, 0xfff2a0);
    let b = commands.spawn((XpEntity, BoardProp, Mesh3d(cache.cyl.clone()), MeshMaterial3d(beam), Transform::from_xyz(0.0, 1.8, 0.0).with_scale(Vec3::new(0.03, 3.4, 0.03)))).id();
    commands.entity(keeper).add_child(b);
    // Highlight rings for fork choices.
    for k in 0..4 {
        let m = cache.emissive(&mut mats, 0xffffff);
        commands.spawn((XpEntity, BoardProp, Highlight(k), Mesh3d(cache.cyl.clone()), MeshMaterial3d(m), Transform::from_translation(origin).with_scale(Vec3::new(0.9, 0.03, 0.9)), Visibility::Hidden));
    }
    // Scenery around the rim.
    let tree = TREES[id % 3];
    if root.exists(tree) {
        for k in 0..12 {
            let a = k as f32 / 12.0 * std::f32::consts::TAU;
            let (x, z) = (a.cos() * 16.2, a.sin() * 9.0);
            if z > 4.0 {
                continue;
            }
            let mut t = crate::render::object_view::spawn_model(&mut commands, &assets, &root, tree, false);
            t.insert((XpEntity, BoardProp, Transform::from_translation(origin + Vec3::new(x, -0.3, z)).with_scale(Vec3::splat(0.9 + (k % 3) as f32 * 0.2))));
        }
    }
    let _ = scene_handle;
}

#[allow(clippy::too_many_arguments)]
pub fn board_props_update(
    app: Res<PartyApp>,
    time: Res<Time>,
    mut cache: ResMut<ViewCache>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut discs: Query<(&SpaceDisc, &mut MeshMaterial3d<StandardMaterial>, &mut Transform), (Without<EdgeSeg>, Without<KeeperProp>, Without<Highlight>, Without<LavaPlane>)>,
    mut edges: Query<(&EdgeSeg, &mut Visibility), Without<Highlight>>,
    mut keeper: Query<&mut Transform, (With<KeeperProp>, Without<SpaceDisc>, Without<Highlight>, Without<LavaPlane>)>,
    mut hl: Query<(&Highlight, &mut Visibility, &mut Transform), (Without<SpaceDisc>, Without<KeeperProp>, Without<EdgeSeg>, Without<LavaPlane>)>,
    mut lava: Query<&mut Transform, (With<LavaPlane>, Without<SpaceDisc>, Without<KeeperProp>, Without<Highlight>)>,
    mut palette: ResMut<crate::fx::sky::SkyPalette>,
) {
    let Some(id) = app.board_built else { return };
    let board = board(id);
    let cond = app.game.as_ref().map_or(Conditions::default(), |g| g.cond);
    let t = time.elapsed_secs();
    let origin = to_bevy(BOARD_O);
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
        let r = if board.spaces[d.0].span { 0.5 } else { 0.62 };
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
    let kp = board.spaces[keeper_space].pos;
    for mut tf in &mut keeper {
        let want = origin + Vec3::new(kp[0], 0.2, kp[1]);
        tf.translation = tf.translation.lerp(want, (time.delta_secs() * 4.0).min(1.0));
    }
    for (h, mut vis, mut tf) in &mut hl {
        match options.get(h.0) {
            Some(&s) => {
                let p = board.spaces[s].pos;
                tf.translation = origin + Vec3::new(p[0], 0.12, p[1]);
                let k = 1.0 + 0.12 * (t * 7.0).sin();
                tf.scale = Vec3::new(0.95 * k, 0.03, 0.95 * k);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    for mut tf in &mut lava {
        tf.translation.y = origin.y + 0.03 + 0.012 * (t * 1.3).sin();
    }
    // Sky mood follows the tide.
    if board.gimmick == Gimmick::Tide {
        let k = cond.tide as f32 * 0.5;
        let want = crate::fx::sky::SkyPalette::from_hex(board.sky[0], if k > 0.4 { 0xb03a20 } else { board.sky[1] }, board.sky[2]).with_clouds(0.35);
        if *palette != want {
            *palette = want;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mini-game stage shapes
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
    let o = to_bevy(MINI_O);
    for (_, s, mut tf, mut vis, mut mesh, mut mat) in &mut pool {
        let Some(sh) = shapes.get(s.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let (m, scale) = match sh.sh {
            Sh::Box => (cache.cube.clone(), Vec3::new(sh.s[0], sh.s[1], sh.s[2])),
            Sh::Disc => (cache.cyl.clone(), Vec3::new(sh.s[0], sh.s[1].max(0.01), if sh.s[2] > 0.0 { sh.s[2] } else { sh.s[0] })),
            Sh::Ball => (cache.ball.clone(), Vec3::splat(sh.s[0])),
            Sh::Cone => (cache.cone.clone(), Vec3::new(sh.s[0], sh.s[1], sh.s[0])),
        };
        if mesh.0 != m {
            mesh.0 = m;
        }
        let h = cache.mat(&mut mats, sh.col);
        if mat.0 != h {
            mat.0 = h;
        }
        tf.translation = o + Vec3::new(sh.p[0], sh.p[1] + 0.03, sh.p[2]);
        tf.rotation = Quat::from_rotation_y(sh.yaw);
        tf.scale = scale;
        *vis = Visibility::Inherited;
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
        let pos = mini_pos(-4.0 + p as f32 * 2.5, 6.5);
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
                let (x, z) = (board.spaces[board.start].pos[0] + [-1.4, -0.5, 0.5, 1.4][p], board.spaces[board.start].pos[1] + 1.2 + (p % 2) as f32 * 0.3);
                let target = board_pos([x, z]);
                let pc = &mut app.pieces[p];
                pc.mario.size = 0.8;
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
                let pc = &mut app.pieces[p];
                pc.mario.size = 0.8;
                pc.hidden = false;
                if place || pc.at.is_none() {
                    pc.at = Some(s);
                    pc.snap = true;
                }
                let sp = board.spaces[pc.at.unwrap()].pos;
                let off = slot_offset(p);
                pc.target = Some(board_pos([sp[0] + off[0], sp[1] + off[1]]));
            }
            app.place = false;
        }
        Screen::Mini => {
            let Some(run) = app.mini.as_ref() else { return };
            let av: Vec<Avatar> = run.scene.avatars.clone();
            let first = place;
            for p in 0..4 {
                let a = av.iter().find(|a| a.p == p);
                let pc = &mut app.pieces[p];
                pc.at = None;
                pc.mario.size = 1.0;
                match a {
                    Some(a) => {
                        pc.hidden = a.out;
                        pc.target = Some(mini_pos(a.x, a.z));
                        if first {
                            pc.snap = true;
                        }
                        let air = a.y > 0.25;
                        if air && !pc.air_prev {
                            pc.jump_next = true;
                        }
                        pc.hold_a = air;
                        pc.air_prev = air;
                    }
                    None => pc.hidden = true,
                }
            }
            if !av.is_empty() {
                app.place = false;
            }
        }
        Screen::Results => {
            let order = app.game.as_ref().map(|g| g.standings()).unwrap_or_else(|| vec![0, 1, 2, 3]);
            let winners = app.game.as_ref().map(|g| g.winners()).unwrap_or_default();
            for (rank, &p) in order.iter().enumerate() {
                let (pos, h) = podium_slot(rank);
                let pc = &mut app.pieces[p];
                pc.mario.size = 1.0;
                pc.hidden = false;
                pc.target = Some([pos[0], pos[1] + h, pos[2] + 10.0]);
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

pub fn drive_pieces(mut app: ResMut<PartyApp>) {
    for pc in &mut app.pieces {
        let pos = pc.mario.runtime.pos;
        let mut input = crate::experiments::crowd::DriveInput::default();
        if let Some(t) = pc.target {
            if pc.snap {
                pc.mario.teleport(t, None);
                pc.snap = false;
                pc.mario.drive(&input);
                continue;
            }
            let (dx, dz) = (t[0] - pos[0], t[2] - pos[2]);
            let d = dx.hypot(dz);
            if d > 1400.0 {
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
}

pub fn camera(mut app: ResMut<PartyApp>, mut cam: ResMut<CameraOverride>, time: Res<Time>, windows: Query<&Window, With<PrimaryWindow>>, mut st: Local<CamState>) {
    let win = windows.single().map_or(Vec2::new(874.0, 402.0), |w| w.size());
    let aspect = (win.x / win.y.max(1.0)).max(0.5);
    let fov = 36.0f32;
    let tan_h = (fov.to_radians() / 2.0).tan();
    let o = to_bevy(BOARD_O);
    let (eye_t, look_t) = match app.screen {
        Screen::Title | Screen::Minis => {
            let sway = (app.t as f32 * 0.25).sin();
            let d = (15.0 / (tan_h * aspect)).max(26.0);
            (o + Vec3::new(sway * 3.0, d * 0.78, d * 0.55), o + Vec3::new(0.0, 0.0, 0.5))
        }
        Screen::Board => {
            let focus_p = app.game.as_ref().map_or(0, |g| g.cur);
            if app.overview {
                let d = (14.5 / (tan_h * aspect)).max(26.0);
                (o + Vec3::new(0.0, d * 0.8, d * 0.58), o + Vec3::new(0.0, 0.0, 0.6))
            } else {
                let p = app.pieces.get(focus_p).map_or(o, |pc| to_bevy(pc.mario.pos()));
                // Keep the aspect in mind: a narrow window needs to be further out.
                let d = (11.5 / (tan_h * aspect)).max(17.0);
                (p + Vec3::new(0.0, d * 0.82, d * 0.57), p + Vec3::new(0.0, 0.3, -0.9))
            }
        }
        Screen::Mini => {
            let d = (12.0 / (tan_h * aspect)).max(17.5);
            let m = to_bevy(MINI_O);
            (m + Vec3::new(0.0, d * 0.82, d * 0.57), m + Vec3::new(0.0, 0.0, 0.1))
        }
        Screen::Results => {
            let p = to_bevy(PODIUM_O);
            (p + Vec3::new(0.1, 3.2, 8.4), p + Vec3::new(0.1, 1.0, 0.0))
        }
    };
    let snap = st.eye.is_none() || matches!(app.screen, Screen::Mini | Screen::Results) && (st.eye.unwrap() - eye_t).length() > 14.0;
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
