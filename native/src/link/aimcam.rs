//! First-person aiming for the Fairy Slingshot, Fairy Bow (all arrows) and Hookshot / Longshot.
//!
//! The camera is `CAM_MODE_AIM_ADULT` / `CAM_MODE_AIM_CHILD` of `z_camera_data.inc.c` (`CAM_FUNC_SUBJ3`,
//! `Camera_Subj3` in `z_camera.c`): the eye sits at `yOffset` -7 % of the player height off the head, `eyeDist` 14
//! units behind it, looking along the aim, `fov` 45. The aim itself is the sim's `Mv::Aim` (`moves.rs`
//! `run_aim`): the left stick, the right stick (gamepad), the arrow keys, a mouse or touch drag anywhere
//! outside the on-screen buttons turn and tilt it; releasing the C button fires, A cancels. The reticle is a
//! `bevy_ui` ring that closes and turns red while the sight line crosses an enemy or a hook target.
use super::{link_active, LinkInput};
use crate::camera::rig::CameraRig;
use crate::camera::MainCamera;
use crate::characters::Character;
use crate::experiments::ExperimentSlot;
use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::render::to_bevy;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::link::items::{Ammo, Item};

pub struct AimCamPlugin;

impl Plugin for AimCamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimCam>()
            .add_systems(Startup, spawn_reticle.in_set(crate::progress::PersistentSpawn))
            .add_systems(PreUpdate, collect_look.after(crate::touch::drive_touch).after(super::read_link_keys))
            .add_systems(Update, (first_person_camera, update_reticle).chain().after(super::zcam::z_camera));
    }
}

/// `sSetNormal0ModeAimAdultData` / `...ChildData` (`CAM_FUNCDATA_SUBJ3`).
pub const AIM_Y_OFFSET: f64 = -0.07; // fraction of the player height, added to the head
pub const AIM_EYE_DIST: f64 = 14.0; // OoT units behind the head
pub const AIM_FOV_DEG: f32 = 45.0;
pub const OOT_UNIT: f64 = jumpman_sim::link::combat::OOT_SCALE;

#[derive(Resource, Default)]
pub struct AimCam {
    /// 0 = the normal camera, 1 = fully first person.
    pub w: f32,
    yaw: Option<f64>,
    pitch: f64,
    fov_touched: bool,
    pub locked: bool,
}

/// Eye, look-at point and vertical fov for an aim along (`yaw` s16, `pitch` radians) from a player standing at `feet`.
pub fn aim_placement(feet: DVec3, yaw: f64, pitch: f64, height: f64) -> (DVec3, DVec3, f32) {
    let ang = yaw * std::f64::consts::PI / 32768.0;
    let dir = DVec3::new(ang.sin() * pitch.cos(), pitch.sin(), ang.cos() * pitch.cos());
    let head = feet + DVec3::Y * (height * (1.0 + AIM_Y_OFFSET));
    let eye = head - dir * (AIM_EYE_DIST * OOT_UNIT);
    (eye, head + dir * 100.0, AIM_FOV_DEG.to_radians())
}

/// Whether the sight line from `eye` along `dir` (unit) passes within `radius` of `center` (the reticle's lock-on test).
pub fn sight_crosses(eye: DVec3, dir: DVec3, center: DVec3, radius: f64) -> bool {
    let to = center - eye;
    let along = to.dot(dir);
    if along <= 0.0 {
        return false;
    }
    (to - dir * along).length() <= radius
}

pub fn aim_dir(yaw: f64, pitch: f64) -> DVec3 {
    let ang = yaw * std::f64::consts::PI / 32768.0;
    DVec3::new(ang.sin() * pitch.cos(), pitch.sin(), ang.cos() * pitch.cos())
}

/// Shortest signed s16 difference `b - a`.
fn yaw_delta(a: f64, b: f64) -> f64 {
    let d = (b - a).rem_euclid(65536.0);
    if d > 32768.0 {
        d - 65536.0
    } else {
        d
    }
}

/// Drag pixels and the right stick / arrow keys while aiming, handed to the sim at the next tick (`pre_tick`).
fn collect_look(character: Res<Character>, slot: Res<ExperimentSlot>, sim: Res<GameSim>, input: Res<PlayerInput>, mut li: ResMut<LinkInput>) {
    if !link_active(&character, &slot) || !sim.mario.link.first_person() {
        li.look_px = Vec2::ZERO;
        li.look_stick = Vec2::ZERO;
        return;
    }
    let (drag, orbit) = input.look_peek();
    li.look_px += drag;
    // The right stick / arrow keys: x right-positive, y up-positive, full deflection = a full left-stick push (64).
    li.look_stick = Vec2::new(orbit.x, -orbit.y) * 64.0;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn first_person_camera(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    sim: Res<GameSim>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
    mut cam: ResMut<AimCam>,
    mut camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let dt = time.delta_secs().min(0.1);
    let on = link_active(&character, &slot) && sim.mario.link.first_person();
    let goal = if on { 1.0 } else { 0.0 };
    cam.w += (goal - cam.w) * (1.0 - (-14.0 * dt).exp());
    if (goal - cam.w).abs() < 0.01 {
        cam.w = goal;
    }
    let (tf, proj) = &mut *camera;
    if cam.w <= 0.0 {
        cam.yaw = None;
        if cam.fov_touched {
            if let Projection::Perspective(p) = &mut **proj {
                p.fov = 50f32.to_radians();
            }
            cam.fov_touched = false;
        }
        return;
    }
    if let (Some(aim), true) = (sim.mario.link.aim, on) {
        // Smooth the 30 Hz aim for 60+ fps rendering.
        let k = 1.0 - (-35.0 * dt as f64).exp();
        let cur = cam.yaw.unwrap_or(aim.yaw);
        cam.yaw = Some(cur + yaw_delta(cur, aim.yaw) * k);
        cam.pitch += (aim.pitch - cam.pitch) * k;
        if cam.w < 0.05 {
            cam.pitch = aim.pitch;
        }
    }
    let Some(yaw) = cam.yaw else { return };
    let child = *character == Character::YoungLink;
    let feet = DVec3::from(sim.mario.pos);
    let (eye, at, fov) = aim_placement(feet, yaw, cam.pitch, super::zcam::link_height(child));
    let w = cam.w as f64;
    let be = rig.eye.lerp(eye, w);
    let bl = rig.look_at.lerp(at, w);
    **tf = Transform::from_translation(to_bevy(be.into())).looking_at(to_bevy(bl.into()), Vec3::Y);
    // Leaving the aim continues from behind Link, facing where he aimed.
    rig.yaw = yaw * std::f64::consts::PI / 32768.0 + std::f64::consts::PI;
    if let Projection::Perspective(p) = &mut **proj {
        p.fov = 50f32.to_radians() + (fov - 50f32.to_radians()) * cam.w;
        cam.fov_touched = true;
    }
}

// ---------------------------------------------------------------------------------------------
// Reticle

#[derive(Component)]
struct Reticle;
#[derive(Component)]
struct ReticleRing;
#[derive(Component)]
struct ReticleTick(usize);
#[derive(Component)]
struct ReticleAmmo;

const RING_PX: f32 = 54.0;

fn spawn_reticle(mut commands: Commands) {
    commands
        .spawn((Reticle, Visibility::Hidden, GlobalZIndex(25), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
        .with_children(|p| {
            // Ring + centre dot.
            p.spawn((ReticleRing, Node { width: Val::Px(RING_PX), height: Val::Px(RING_PX), border: UiRect::all(Val::Px(2.0)), position_type: PositionType::Absolute, ..default() }, BorderColor::all(Color::WHITE), crate::button_marker::BorderRadius::MAX));
            p.spawn((Node { width: Val::Px(4.0), height: Val::Px(4.0), position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::WHITE), crate::button_marker::BorderRadius::MAX, ReticleTick(4)));
            // Four ticks (up, down, left, right) that slide inward when locked.
            for i in 0..4 {
                let (w, h) = if i < 2 { (2.0, 12.0) } else { (12.0, 2.0) };
                p.spawn((ReticleTick(i), Node { width: Val::Px(w), height: Val::Px(h), position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::WHITE)));
            }
            p.spawn((ReticleAmmo, Text::new(""), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::WHITE), Node { position_type: PositionType::Absolute, top: Val::Percent(58.0), ..default() }));
        });
}

/// What the hookshot's sight line meets: `Some(true)` a surface it would latch onto (an OoT course's marked surfaces, else any
/// wall or ceiling), `Some(false)` scenery that only reflects it, `None` open air within `range`.
pub fn hook_sight(world: &jumpman_sim::collision::CollisionWorld, eye: DVec3, dir: DVec3, range: f64) -> Option<bool> {
    let (a, b) = (eye.to_array(), (eye + dir * range).to_array());
    jumpman_sim::link::projectiles::terrain_hit_flags(world, &a, &b).map(|(_, normal, flags)| {
        if world.hook_marked {
            flags & jumpman_sim::surfaces::SURFACE_FLAG_HOOKABLE != 0
        } else {
            normal[1].abs() < 0.7
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn update_reticle(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    sim: Res<GameSim>,
    mut cam: ResMut<AimCam>,
    oot: Option<Res<crate::zelda_enemies::OotEnemies>>,
    mut root: Query<&mut Visibility, With<Reticle>>,
    mut ring: Query<&mut BorderColor, With<ReticleRing>>,
    mut ticks: Query<(&ReticleTick, &mut Node, &mut BackgroundColor), Without<ReticleRing>>,
    mut ammo: Query<&mut Text, With<ReticleAmmo>>,
) {
    let on = link_active(&character, &slot) && sim.mario.link.first_person();
    for mut v in &mut root {
        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !on {
        cam.locked = false;
        return;
    }
    let link = &sim.mario.link;
    let aim = link.aim.unwrap();
    let child = *character == Character::YoungLink;
    let (eye, _, _) = aim_placement(DVec3::from(sim.mario.pos), aim.yaw, aim.pitch, super::zcam::link_height(child));
    let dir = aim_dir(aim.yaw, aim.pitch);
    let mut locked = sim.link_opponent.is_some_and(|p| sight_crosses(eye, dir, DVec3::from(p), 90.0));
    if let Some(en) = oot.as_ref() {
        locked |= en.link_targets().iter().any(|t| sight_crosses(eye, dir, DVec3::from(t.pos) + DVec3::Y * t.height * 0.5, t.radius.max(30.0)));
    }
    // The hookshot: the ring closes and turns green over a surface (or enemy) the hook would hold on to.
    let hooking = aim.item == Item::Hookshot && !locked && {
        let range = 260.0 * jumpman_sim::link::combat::OOT_SCALE * if link.hook_long { 2.0 } else { 1.0 };
        hook_sight(&sim.mario.world, eye, dir, range) == Some(true)
    };
    cam.locked = locked || hooking;
    let col = if locked {
        Color::srgb(1.0, 0.25, 0.2)
    } else if hooking {
        Color::srgb(0.35, 1.0, 0.4)
    } else {
        Color::WHITE
    };
    for mut b in &mut ring {
        *b = BorderColor::all(col);
    }
    let gap = if locked || hooking { 6.0 } else { 14.0 };
    for (ReticleTick(i), mut node, mut bg) in &mut ticks {
        bg.0 = col;
        let (dx, dy) = match i {
            0 => (0.0, -(RING_PX / 2.0 - gap)),
            1 => (0.0, RING_PX / 2.0 - gap),
            2 => (-(RING_PX / 2.0 - gap), 0.0),
            3 => (RING_PX / 2.0 - gap, 0.0),
            _ => (0.0, 0.0),
        };
        node.margin = UiRect { left: Val::Px(dx), top: Val::Px(dy), ..default() };
    }
    for mut t in &mut ammo {
        let n = match aim.item {
            Item::Slingshot => Some(link.inv.ammo(Ammo::Seed)),
            Item::Hookshot => None,
            _ => Some(link.inv.ammo(Ammo::Arrow)),
        };
        **t = n.map_or(String::new(), |n| format!("x{n}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::zcam::link_height;

    #[test]
    fn camera_sits_at_the_head_behind_the_aim_with_the_aim_fov() {
        let h = link_height(false);
        let (eye, at, fov) = aim_placement(DVec3::ZERO, 0.0, 0.0, h);
        assert!((fov - 45f32.to_radians()).abs() < 1e-6);
        assert!((eye.y - h * 0.93).abs() < 1e-6, "eye at 93% of the height");
        assert!((eye.z + AIM_EYE_DIST * OOT_UNIT).abs() < 1e-6, "14 units behind the head");
        assert!(at.z > 0.0);
    }

    #[test]
    fn hook_sight_tells_hookable_surfaces_from_reflecting_ones() {
        use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
        let mut w = CollisionWorld::default();
        w.hook_marked = true;
        for (x0, x1, flags) in [(-400.0, 0.0, jumpman_sim::surfaces::SURFACE_FLAG_HOOKABLE), (0.0, 400.0, 0)] {
            let o = SurfaceOptions { flags, ..Default::default() };
            w.add_triangle([x0, 0.0, 600.0], [x1, 400.0, 600.0], [x1, 0.0, 600.0], &o);
            w.add_triangle([x0, 0.0, 600.0], [x0, 400.0, 600.0], [x1, 400.0, 600.0], &o);
        }
        let dir = DVec3::Z;
        assert_eq!(hook_sight(&w, DVec3::new(-200.0, 100.0, 0.0), dir, 1000.0), Some(true));
        assert_eq!(hook_sight(&w, DVec3::new(200.0, 100.0, 0.0), dir, 1000.0), Some(false));
        assert_eq!(hook_sight(&w, DVec3::new(200.0, 100.0, 0.0), dir, 300.0), None, "out of range");
    }

    #[test]
    fn looking_up_raises_the_look_point() {
        let h = link_height(true);
        let (_, flat, _) = aim_placement(DVec3::ZERO, 0.0, 0.0, h);
        let (_, up, _) = aim_placement(DVec3::ZERO, 0.0, 0.5, h);
        assert!(up.y > flat.y + 10.0);
    }

    #[test]
    fn sight_line_locks_on_targets_ahead_only() {
        let eye = DVec3::ZERO;
        let dir = DVec3::Z;
        assert!(sight_crosses(eye, dir, DVec3::new(10.0, 0.0, 500.0), 40.0));
        assert!(!sight_crosses(eye, dir, DVec3::new(200.0, 0.0, 500.0), 40.0));
        assert!(!sight_crosses(eye, dir, DVec3::new(0.0, 0.0, -500.0), 40.0), "behind the eye");
    }

    #[test]
    fn yaw_delta_takes_the_short_way_round() {
        assert!((yaw_delta(65000.0, 100.0) - 636.0).abs() < 1e-9);
        assert!((yaw_delta(100.0, 65000.0) + 636.0).abs() < 1e-9);
    }
}
