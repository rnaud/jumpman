//! Drawing Link's projectiles, spells and the aim reticle with plain unlit primitives. The set is
//! rebuilt every frame (a handful of entities at most), which keeps the view stateless.
use super::WorldFx;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, SCALE};
use bevy::prelude::*;
use jumpman_sim::link::moves::Mv;
use jumpman_sim::link::projectiles::{Kind, Phase};

pub struct LinkViewPlugin;

impl Plugin for LinkViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, draw.after(crate::CameraSet));
    }
}

#[derive(Component)]
struct LinkVisual;

#[derive(Resource)]
struct Assets3d {
    sphere: Handle<Mesh>,
    cyl: Handle<Mesh>,
    cube: Handle<Mesh>,
    mats: [Handle<StandardMaterial>; 14],
}

const ARROW: usize = 0;
const FIRE: usize = 1;
const ICE: usize = 2;
const LIGHT: usize = 3;
const SEED: usize = 4;
const NUT: usize = 5;
const BOOM: usize = 6;
const HOOK: usize = 7;
const BOMB: usize = 8;
const BURST: usize = 9;
const DINS: usize = 10;
const NAYRU: usize = 11;
const FARORE: usize = 12;
const RETICLE: usize = 13;

fn unlit(c: Color) -> StandardMaterial {
    StandardMaterial { base_color: c, unlit: true, alpha_mode: if c.alpha() < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque }, ..default() }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let colors = [
        Color::srgb(0.95, 0.9, 0.6),
        Color::srgb(1.0, 0.45, 0.1),
        Color::srgb(0.5, 0.9, 1.0),
        Color::srgb(1.0, 1.0, 0.6),
        Color::srgb(0.55, 0.35, 0.15),
        Color::srgb(0.4, 0.75, 0.25),
        Color::srgb(0.95, 0.75, 0.2),
        Color::srgb(0.7, 0.7, 0.75),
        Color::srgb(0.12, 0.12, 0.15),
        Color::srgba(1.0, 0.6, 0.15, 0.55),
        Color::srgba(1.0, 0.25, 0.1, 0.4),
        Color::srgba(0.3, 0.55, 1.0, 0.3),
        Color::srgba(0.3, 1.0, 0.4, 0.5),
        Color::srgba(1.0, 1.0, 1.0, 0.9),
    ];
    let mats = colors.map(|c| materials.add(unlit(c)));
    commands.insert_resource(Assets3d { sphere: meshes.add(Sphere::new(1.0)), cyl: meshes.add(Cylinder::new(1.0, 1.0)), cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)), mats });
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    old: Query<Entity, With<LinkVisual>>,
    assets: Option<Res<Assets3d>>,
    sim: Res<GameSim>,
    mut fx: ResMut<WorldFx>,
    time: Res<Time>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let Some(a) = assets else { return };
    let link = &sim.mario.link;
    if !link.enabled {
        return;
    }
    let mut sphere = |c: &mut Commands, mat: usize, at: Vec3, r: f32| {
        c.spawn((LinkVisual, Mesh3d(a.sphere.clone()), MeshMaterial3d(a.mats[mat].clone()), Transform::from_translation(at).with_scale(Vec3::splat(r)), Visibility::Inherited));
    };
    let t = sim.ticks as f32;
    for p in &link.projectiles.list {
        if p.phase == Phase::Done {
            continue;
        }
        let at = to_bevy(p.pos);
        match p.kind {
            Kind::Arrow | Kind::FireArrow | Kind::IceArrow | Kind::LightArrow => {
                let dir = if p.vel == [0.0; 3] { Vec3::new(p.yaw.sin() as f32, 0.0, p.yaw.cos() as f32) } else { Vec3::new(p.vel[0] as f32, p.vel[1] as f32, p.vel[2] as f32).normalize() };
                let mat = match p.kind {
                    Kind::FireArrow => FIRE,
                    Kind::IceArrow => ICE,
                    Kind::LightArrow => LIGHT,
                    _ => ARROW,
                };
                let len = 80.0 * SCALE;
                commands.spawn((
                    LinkVisual,
                    Mesh3d(a.cyl.clone()),
                    MeshMaterial3d(a.mats[mat].clone()),
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir)).with_scale(Vec3::new(3.0 * SCALE, len, 3.0 * SCALE)),
                    Visibility::Inherited,
                ));
                if mat != ARROW {
                    sphere(&mut commands, mat, at, 9.0 * SCALE);
                }
            }
            Kind::Seed => sphere(&mut commands, SEED, at, 7.0 * SCALE),
            Kind::Nut => sphere(&mut commands, NUT, at, 11.0 * SCALE),
            Kind::Boomerang => {
                commands.spawn((
                    LinkVisual,
                    Mesh3d(a.cube.clone()),
                    MeshMaterial3d(a.mats[BOOM].clone()),
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_y(t * 1.1)).with_scale(Vec3::new(46.0 * SCALE, 6.0 * SCALE, 14.0 * SCALE)),
                    Visibility::Inherited,
                ));
                commands.spawn((
                    LinkVisual,
                    Mesh3d(a.cube.clone()),
                    MeshMaterial3d(a.mats[BOOM].clone()),
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_y(t * 1.1 + 1.0)).with_scale(Vec3::new(30.0 * SCALE, 6.0 * SCALE, 12.0 * SCALE)),
                    Visibility::Inherited,
                ));
            }
            Kind::Hookshot | Kind::Longshot => {
                sphere(&mut commands, HOOK, at, 12.0 * SCALE);
                // The chain back to Link's hand.
                let hand = to_bevy([sim.mario.pos[0], sim.mario.pos[1] + 100.0, sim.mario.pos[2]]);
                let d = at - hand;
                if d.length() > 0.01 {
                    commands.spawn((
                        LinkVisual,
                        Mesh3d(a.cyl.clone()),
                        MeshMaterial3d(a.mats[HOOK].clone()),
                        Transform::from_translation(hand + d * 0.5).with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize())).with_scale(Vec3::new(1.8 * SCALE, d.length(), 1.8 * SCALE)),
                        Visibility::Inherited,
                    ));
                }
            }
            Kind::Bomb | Kind::Bombchu => {
                if p.phase == Phase::Burst {
                    sphere(&mut commands, BURST, at, p.radius as f32 * SCALE);
                } else {
                    // Flashes faster as the fuse runs down (`flashSpeedScale`).
                    let flash = (p.timer < 30.0 && (p.timer as i32 / 3) % 2 == 0) || (p.timer < 70.0 && (p.timer as i32 / 7) % 2 == 0);
                    sphere(&mut commands, if flash { FIRE } else { BOMB }, at + Vec3::Y * 14.0 * SCALE, if p.kind == Kind::Bomb { 16.0 } else { 10.0 } * SCALE);
                }
            }
            Kind::DinsFire => {
                if p.radius > 1.0 {
                    sphere(&mut commands, DINS, at, p.radius as f32 * SCALE);
                }
            }
        }
    }
    if link.nayru > 0.0 {
        let pulse = 1.0 + 0.04 * (t * 0.3).sin();
        sphere(&mut commands, NAYRU, to_bevy([sim.mario.pos[0], sim.mario.pos[1] + 90.0, sim.mario.pos[2]]), 170.0 * SCALE * pulse);
    }
    if let Some(p) = link.farore {
        let base = to_bevy(p);
        commands.spawn((
            LinkVisual,
            Mesh3d(a.cyl.clone()),
            MeshMaterial3d(a.mats[FARORE].clone()),
            Transform::from_translation(base + Vec3::Y * 150.0 * SCALE).with_scale(Vec3::new(30.0 * SCALE, 300.0 * SCALE, 30.0 * SCALE)),
            Visibility::Inherited,
        ));
    }
    if link.mv == Mv::Aim {
        if let Some(aim) = link.aim {
            let o = to_bevy([sim.mario.pos[0], sim.mario.pos[1] + 100.0, sim.mario.pos[2]]);
            let ang = aim.yaw * std::f64::consts::PI / 32768.0;
            let dir = Vec3::new((ang.sin() * aim.pitch.cos()) as f32, aim.pitch.sin() as f32, (ang.cos() * aim.pitch.cos()) as f32);
            // A dotted sight line plus the reticle where it ends.
            for i in 1..=8 {
                sphere(&mut commands, RETICLE, o + dir * (i as f32 * 120.0 * SCALE), 2.0 * SCALE);
            }
            sphere(&mut commands, RETICLE, o + dir * (1100.0 * SCALE), 10.0 * SCALE);
        }
    }
    // Flashes (explosions' shockwave, nut flash, hammer slam): grow and fade.
    let dt = time.delta_secs();
    fx.flashes.retain_mut(|f| {
        f.2 += dt;
        f.2 < 0.45
    });
    for (pos, size, age) in &fx.flashes {
        let k = age / 0.45;
        sphere(&mut commands, BURST, *pos, (40.0 + 260.0 * k) * size * SCALE);
    }
}
