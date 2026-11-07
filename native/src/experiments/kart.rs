//! MK64 free roam. Shares scripts/export-mk64.py output and driving rules with kart.ts.
//! The track and occupied kart are original MK64 assets; the empty kart is procedural.
use super::kart_physics::{Floor, Ground, Input as KartInput, Kart, TOP_SPEED};
use super::{dock::DockItem, CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::{to_bevy, SCALE};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::{Affine2, DVec3};
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::math::{to_s16, Vec3 as V3};
use jumpman_sim::state::*;
use jumpman_sim::surfaces::SURFACE_NOT_SLIPPERY;
use serde::Deserialize;
use std::{collections::BTreeMap, f64::consts::PI, path::Path, sync::Arc};

#[derive(Deserialize)]
struct TextureSpec {
    file: String,
    width: f64,
    height: f64,
}
#[derive(Deserialize)]
struct Raceway {
    vertices: Vec<[f64; 8]>,
    groups: BTreeMap<String, Vec<usize>>,
    textures: BTreeMap<String, TextureSpec>,
    path: Vec<V3>,
    scale: f64,
}
impl Raceway {
    fn read(root: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(root.join("luigi-raceway.json"))
            .map_err(|e| format!("MK64 assets missing: {e}. Run pnpm export:mk64 <ROM>"))?;
        let data: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if data.path.is_empty() || data.scale != 8. || data.groups.is_empty() {
            return Err("Invalid MK64 course export".into());
        }
        for (name, indices) in &data.groups {
            if indices.len() % 3 != 0
                || indices.iter().any(|i| *i >= data.vertices.len())
                || !data.textures.contains_key(name)
            {
                return Err(format!("Invalid MK64 display list {name}"));
            }
        }
        for texture in data.textures.values() {
            if !root.join(&texture.file).is_file() {
                return Err(format!("Missing MK64 texture {}", texture.file));
            }
        }
        if !root.join("mario-kart.png").is_file() {
            return Err("Missing Mario kart atlas".into());
        }
        Ok(data)
    }
    fn point(&self, index: usize) -> V3 {
        let v = self.vertices[index];
        [v[0] * self.scale, v[1] * self.scale, v[2] * self.scale]
    }
    fn world(&self) -> CollisionWorld {
        let mut world = CollisionWorld::new(1024., 100000.);
        for (name, indices) in &self.groups {
            let options = SurfaceOptions {
                kind: if name.contains("Grass") {
                    SURFACE_NOT_SLIPPERY
                } else {
                    0
                },
                ..default()
            };
            for tri in indices.chunks_exact(3) {
                world.add_triangle(
                    self.point(tri[0]),
                    self.point(tri[1]),
                    self.point(tri[2]),
                    &options,
                );
            }
        }
        world.finalize();
        world
    }
}

/// Course units -> world units (the MK64 export scale).
const UNIT: f64 = 8.;

/// Adapts the sim's collision world to the kart physics (which works in course units).
struct WorldGround<'a>(&'a CollisionWorld);
impl Ground for WorldGround<'_> {
    fn floor(&self, x: f64, y: f64, z: f64) -> Option<Floor> {
        let hit = self.0.find_floor(x * UNIT, y * UNIT, z * UNIT);
        let s = hit.surface?;
        Some(Floor {
            height: hit.height / UNIT,
            normal: [s.normal[0], s.normal[1], s.normal[2]],
            grass: s.kind == SURFACE_NOT_SLIPPERY,
        })
    }
    fn wall(&self, pos: [f64; 3]) -> Option<[f64; 3]> {
        let mut p = [pos[0] * UNIT, pos[1] * UNIT, pos[2] * UNIT];
        (self.0.f32_find_wall_collision(&mut p, 50., 65.) > 0).then(|| [p[0] / UNIT, p[1] / UNIT, p[2] / UNIT])
    }
}

#[derive(Debug)]
struct KartState {
    pos: V3,
    spawn: V3,
    yaw: f64,
    speed: f64,
    driving: bool,
    phys: Kart,
}
impl KartState {
    fn new(spawn: V3) -> Self {
        Self {
            pos: spawn,
            spawn,
            yaw: PI,
            speed: 0.,
            driving: true,
            phys: Kart::new(spawn.map(|v| v / UNIT), -32768),
        }
    }
    fn set_yaw(&mut self, yaw: f64) {
        self.yaw = yaw;
        self.phys.rotation = to_s16(-yaw * 32768. / PI) as i32;
    }
    fn stop(&mut self) {
        self.speed = 0.;
        self.phys.velocity = [0.; 3];
        self.phys.current_speed = 0.;
        self.phys.speed = 0.;
    }
    /// Percentage of the top speed for the HUD.
    fn throttle_percent(&self) -> f64 {
        self.phys.current_speed / TOP_SPEED * 100.
    }
    fn reset(&mut self) {
        *self = Self::new(self.spawn);
    }
    fn distance(&self, p: V3) -> f64 {
        (DVec3::from(p) - DVec3::from(self.pos)).length()
    }
    fn exit_position(&self, world: &CollisionWorld) -> Option<V3> {
        for offset in [PI / 2., -PI / 2., PI] {
            let mut p = [
                self.pos[0] + (self.yaw + offset).sin() * 170.,
                self.pos[1],
                self.pos[2] + (self.yaw + offset).cos() * 170.,
            ];
            if world.f32_find_wall_collision(&mut p, 60., 50.) > 0 {
                continue;
            }
            let hit = world.find_floor(p[0], p[1] + 100., p[2]);
            if hit.surface.as_ref().is_some_and(|s| s.normal[1] >= 0.7)
                && (hit.height - p[1]).abs() <= 90.
            {
                p[1] = hit.height;
                return Some(p);
            }
        }
        None
    }
    fn can_enter(&self, p: V3) -> bool {
        self.distance(p) < 300. && (p[1] - self.pos[1]).abs() < 100.
    }
    /// One 30 Hz sim tick = two 60 Hz MK64 kart updates (see `kart_physics`).
    fn tick(&mut self, input: &ControllerSnapshot, world: &CollisionWorld) {
        let pad = KartInput {
            raw_x: (input.stick_x * 80. / 64.).round() as i32,
            raw_y: (input.stick_y * 80. / 64.).round() as i32,
            accel: input.stick_y > 16.,
            brake: input.button_down & Z_TRIG != 0 || input.stick_y < -16.,
            hop_down: input.button_down & A_BUTTON != 0,
            hop_pressed: input.button_pressed & A_BUTTON != 0,
        };
        let ground = WorldGround(world);
        self.phys.step(&pad, &ground);
        self.phys.step(&KartInput { hop_pressed: false, ..pad }, &ground);
        self.pos = self.phys.pos.map(|v| v * UNIT);
        self.yaw = self.phys.yaw();
        self.speed = self.phys.speed;
        if self.pos[1] < -5000. {
            // Fell off the course: Lakitu-style rescue back to the line.
            self.reset();
        }
    }
}

pub struct KartExperiment {
    state: KartState,
    ready: bool,
    sprite: Option<Entity>,
    sprite_material: Option<Handle<StandardMaterial>>,
    parked: Option<Entity>,
    toggle: bool,
    chase: Option<DVec3>,
}
impl KartExperiment {
    pub fn new() -> Self {
        Self {
            state: KartState::new([0.; 3]),
            ready: false,
            sprite: None,
            sprite_material: None,
            parked: None,
            toggle: false,
            chase: None,
        }
    }
    fn ui(&self, host: &mut Host) {
        let near = self.state.can_enter(host.sim.mario.pos);
        host.set_dock(vec![
            DockItem::new(
                if self.state.driving {
                    "Get out · F"
                } else if near {
                    "Get in · F"
                } else {
                    "Return to kart"
                },
                KeyCode::KeyF,
                0,
            ),
            DockItem::new("Reset · R", KeyCode::KeyR, 0),
        ]);
        host.status(if self.state.driving {
            format!(
                "DRIVING | SPEED {:.0}%",
                self.state.throttle_percent()
            )
        } else {
            "ON FOOT | Walk back to the kart to drive".into()
        });
    }
    fn switch(&mut self, host: &mut Host) {
        if self.state.driving {
            if let Some(pos) = self.state.exit_position(&host.sim.mario.world) {
                self.state.driving = false;
                self.state.stop();
                host.place_mario(pos, Some(self.state.yaw * 32768. / PI));
                host.sim.mario.set_action(ACT_IDLE, 0.);
                host.set_player_visible(true);
                host.cam.pose = None;
            } else {
                host.toast("Move the kart to open ground before getting out.", 2.);
            }
        } else if self.state.can_enter(host.sim.mario.pos) {
            self.state.driving = true;
            host.place_mario(self.state.pos, Some(self.state.yaw * 32768. / PI));
            host.set_player_visible(false);
            self.chase = None;
        } else {
            host.toast("Walk back to the kart to get in.", 2.);
        }
        info!(
            "MK64 mode={} kart={:?} mario={:?}",
            if self.state.driving {
                "driving"
            } else {
                "walking"
            },
            self.state.pos,
            host.sim.mario.pos
        );
    }
    fn texture(host: &Host, file: &str) -> Handle<Image> {
        host.assets
            .load_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
                s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    mag_filter: ImageFilterMode::Nearest,
                    min_filter: ImageFilterMode::Nearest,
                    ..default()
                });
            })
    }
    fn build(&mut self, host: &mut Host) -> Result<(), String> {
        let data = Raceway::read(&host.asset_root.0.join("../public/mk64"))?;
        let world = data.world();
        let mut spawn = data.path[0].map(|v| v * data.scale);
        let floor = world.find_floor(spawn[0], spawn[1] + 200., spawn[2]);
        if floor.surface.is_none() {
            return Err("Luigi Raceway spawn has no floor".into());
        }
        spawn[1] = floor.height;
        for (name, indices) in &data.groups {
            let spec = &data.textures[name];
            let (mut positions, mut colors, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
            for &index in indices {
                let v = data.vertices[index];
                positions.push(to_bevy(data.point(index)).to_array());
                colors.push(
                    Color::srgb(v[5] as f32 / 255., v[6] as f32 / 255., v[7] as f32 / 255.)
                        .to_linear()
                        .to_f32_array(),
                );
                uvs.push([
                    (v[3] / (32. * spec.width)) as f32,
                    (v[4] / (32. * spec.height)) as f32,
                ]);
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.compute_flat_normals();
            let texture = Self::texture(host, &spec.file);
            let material = host.materials.add(StandardMaterial {
                base_color_texture: Some(texture),
                unlit: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Mask(0.5),
                ..default()
            });
            let mesh = host.meshes.add(mesh);
            host.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::default()));
        }
        let texture = Self::texture(host, "mario-kart.png");
        let material = host.materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            unlit: true,
            cull_mode: None,
            alpha_mode: AlphaMode::Mask(0.5),
            ..default()
        });
        let mesh = host.meshes.add(Rectangle::new(240. * SCALE, 240. * SCALE));
        self.sprite = Some(host.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::default(),
        )));
        self.sprite_material = Some(material);
        let root = host.spawn((Transform::default(), Visibility::Hidden));
        self.parked = Some(root);
        let red = host.lit(Color::srgb(0.84, 0.17, 0.13));
        let black = host.lit(Color::srgb(0.08, 0.08, 0.1));
        let white = host.lit(Color::srgb(0.9, 0.9, 0.92));
        for (center, size, material) in [
            ([0., 30., 0.], [100., 24., 140.], red),
            ([0., 44., 45.], [65., 14., 60.], white.clone()),
            ([0., 61., -38.], [62., 55., 16.], black.clone()),
            ([0., 25., 78.], [140., 10., 12.], white),
        ] {
            let child = host.spawn_box(center, size, 0., material);
            host.commands.entity(child).remove::<super::XpEntity>();
            host.commands.entity(root).add_child(child);
        }
        for x in [-62., 62.] {
            for z in [-45., 45.] {
                let mesh = host.meshes.add(Cylinder::new(25. * SCALE, 22. * SCALE));
                let child = host.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(black.clone()),
                    Transform::from_translation(to_bevy([x, 25., z]))
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
                host.commands.entity(child).remove::<super::XpEntity>();
                host.commands.entity(root).add_child(child);
            }
        }
        self.state = KartState::new(spawn);
        host.set_mario_world(
            Arc::new(world),
            SpawnPoint {
                pos: spawn,
                face_yaw: -32768.,
                action: None,
            },
            -5000.,
        );
        host.place_mario(spawn, Some(-32768.));
        host.commands
            .insert_resource(crate::characters::Character::Mario);
        host.set_player_visible(false);
        host.shell.plain_camera = true;
        host.cam.far_scale = 12.;
        host.cam.fog = Some((Color::srgb(0.65, 0.85, 0.95), 18000., 45000.));
        host.card(
            "10",
            "Luigi Raceway",
            "Drive the original Mario Kart 64 track, park and explore as Mario.",
            &[
                ("W/S · stick", "accelerate / brake / reverse"),
                ("A/D · stick", "steer"),
                ("Space · A", "hop; hold while steering to drift, counter-steer to charge a mini-turbo"),
                ("F · B button", "get out / get in nearby"),
                ("Space", "jump on foot"),
                ("R", "reset to starting line"),
            ],
        );
        self.ready = true;
        self.ui(host);
        Ok(())
    }
}
impl Experiment for KartExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Kart
    }
    fn course(&self) -> Option<&'static str> {
        Some("playground")
    }
    fn freezes_player(&self) -> bool {
        self.ready
    }
    fn replaces_course(&self) -> bool {
        true
    }
    fn enter(&mut self, host: &mut Host) {
        if let Err(error) = self.build(host) {
            error!("{error}");
            host.shell.course_hidden = false;
            host.card(
                "10",
                "Mario Kart 64",
                "Assets unavailable. Run pnpm export:mk64 <ROM>, then reopen the map.",
                &[],
            );
            host.toast(&error, 8.);
        }
    }
    fn exit(&mut self, host: &mut Host) {
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        let pos = host.sim.mario.pos;
        let yaw = host.sim.mario.face_angle[1];
        host.place_mario(pos, Some(yaw));
        host.cam.far_scale = 1.;
        host.cam.fog = None;
        host.cam.pose = None;
    }
    fn before_step(&mut self, host: &mut Host, input: &ControllerSnapshot) -> StepOverride {
        if !self.ready {
            return default();
        }
        host.sim.prev_pose = host.sim.pose;
        host.sim.prev_visual = host.sim.visual;
        let previous_action = host.sim.mario.action;
        if self.toggle || input.button_pressed & B_BUTTON != 0 {
            self.toggle = false;
            self.switch(host);
        } else if self.state.driving {
            self.state.tick(input, &host.sim.mario.world);
            host.sim.mario.pos = self.state.pos;
            host.sim.mario.face_angle[1] = to_s16(self.state.yaw * 32768. / PI) as f64;
            host.sim.mario.forward_vel = 0.;
            host.sim.refresh_view();
        } else {
            host.sim.camera_yaw = host.rig.yaw_s16();
            host.sim.tick_mario_only(input);
        }
        host.sim.visual = crate::game::sm64_animation::visual_rotation(
            &crate::game::sm64_animation::AnimInput::from_mario(&host.sim.mario),
        );
        host.sim.visual_continuous = previous_action == host.sim.mario.action;
        self.ui(host);
        default()
    }
    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if !self.ready {
            return false;
        }
        match key {
            KeyCode::KeyF => {
                self.toggle = true;
                true
            }
            KeyCode::KeyR => {
                self.state.reset();
                self.chase = None;
                self.toggle = false;
                host.place_mario(self.state.pos, Some(-32768.));
                host.set_player_visible(false);
                self.ui(host);
                true
            }
            _ => false,
        }
    }
    fn frame(&mut self, host: &mut Host, dt: f64) {
        if !self.ready {
            return;
        }
        let p = DVec3::from(host.render_pos());
        if let Some(parked) = self.parked {
            host.commands.entity(parked).insert((
                Transform::from_translation(to_bevy(self.state.pos))
                    .with_rotation(Quat::from_rotation_y(self.state.yaw as f32)),
                if self.state.driving {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
            ));
        }
        if let Some(sprite) = self.sprite {
            host.commands.entity(sprite).insert(if self.state.driving {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            if self.state.driving {
                let target = p + DVec3::new(
                    -self.state.yaw.sin() * 680.,
                    320.,
                    -self.state.yaw.cos() * 680.,
                );
                let eye = self
                    .chase
                    .map_or(target, |eye| eye.lerp(target, 1. - (-dt * 7.).exp()));
                self.chase = Some(eye);
                host.cam.pose = Some(CamPose {
                    eye,
                    look: p + DVec3::Y * 90.,
                    up: DVec3::Y,
                    fov: 50.,
                    near: 10.,
                    far: 65000.,
                });
                let angle = (eye.x - p.x).atan2(eye.z - p.z) - self.state.yaw - PI;
                let relative = angle.sin().atan2(angle.cos());
                let degrees = relative.abs().to_degrees();
                let frame = (if degrees < 45. {
                    degrees / 2.86
                } else {
                    degrees / 9. + 15.
                })
                .round()
                .min(34.) as f32;
                if let Some(material) = self
                    .sprite_material
                    .as_ref()
                    .and_then(|m| host.materials.get_mut(m))
                {
                    let flip = relative < 0.;
                    material.uv_transform = Affine2::from_scale_angle_translation(
                        Vec2::new(if flip { -1. / 35. } else { 1. / 35. }, 1.),
                        0.,
                        Vec2::new((frame + if flip { 1. } else { 0. }) / 35., 0.),
                    );
                }
                let position = to_bevy((p + DVec3::Y * 100.).into());
                let eye = to_bevy(eye.into());
                host.commands.entity(sprite).insert(
                    Transform::from_translation(position).looking_to(position - eye, Vec3::Y),
                );
            } else {
                host.cam.pose = None;
            }
        }
    }
    fn debug(&self) -> String {
        format!(
            "ready={} driving={} speed={:.1} pos={:?}",
            self.ready, self.state.driving, self.state.speed, self.state.pos
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn flat(kind: i32) -> CollisionWorld {
        let mut world = CollisionWorld::new(1024., 100000.);
        world.add_triangle(
            [-20000., 0., -20000.],
            [-20000., 0., 20000.],
            [20000., 0., 20000.],
            &SurfaceOptions { kind, ..default() },
        );
        world.add_triangle(
            [-20000., 0., -20000.],
            [20000., 0., 20000.],
            [20000., 0., -20000.],
            &SurfaceOptions { kind, ..default() },
        );
        world
    }
    #[test]
    fn mk64_physics_drives_steers_reverses_and_slows_on_grass() {
        let world = flat(0);
        let mut kart = KartState::new([0.; 3]);
        let mut input = ControllerSnapshot {
            stick_y: 64.,
            ..default()
        };
        for _ in 0..170 {
            kart.tick(&input, &world);
        }
        assert_eq!(kart.phys.current_speed, TOP_SPEED);
        assert!(kart.pos[2] < -5000., "{:?}", kart.pos);
        input.stick_x = 64.;
        for _ in 0..30 {
            kart.tick(&input, &world);
        }
        assert!(kart.yaw < PI - 0.5, "right stick turns right");
        input.stick_x = 0.;
        input.stick_y = 0.;
        let before = kart.speed;
        for _ in 0..60 {
            kart.tick(&input, &world);
        }
        assert!(kart.speed < before, "coasts down");
        kart.reset();
        input.stick_y = -64.;
        for _ in 0..90 {
            kart.tick(&input, &world);
        }
        assert!(kart.phys.back_up, "holding back at a stop reverses");
        kart.reset();
        input.stick_y = 64.;
        let grass = flat(SURFACE_NOT_SLIPPERY);
        for _ in 0..170 {
            kart.tick(&input, &grass);
        }
        assert!(kart.speed < 2.6, "{}", kart.speed);
    }
    #[test]
    fn safe_dismount_proximity_reset_and_rescue() {
        let mut kart = KartState::new([0.; 3]);
        let world = flat(0);
        let exit = kart.exit_position(&world).unwrap();
        assert!(kart.can_enter(exit));
        assert!(!kart.can_enter([1000., 0., 0.]));
        assert!(!kart.can_enter([0., 110., 0.]));
        assert!(kart.exit_position(&CollisionWorld::default()).is_none());
        let mut edge = KartState::new([0., 0., -19000.]);
        let go = ControllerSnapshot {
            stick_y: 64.,
            ..default()
        };
        for _ in 0..1500 {
            edge.tick(&go, &world);
        }
        assert!(edge.pos[1] > -5000., "rescued after leaving the course");
        kart.stop();
        kart.reset();
        assert_eq!(kart.pos, [0.; 3]);
        assert_eq!(kart.speed, 0.);
    }
    #[test]
    fn extracted_track_supports_driving_walking_and_barriers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        if !root.join("luigi-raceway.json").exists() {
            return;
        }
        let data = Raceway::read(&root).unwrap();
        let world = Arc::new(data.world());
        let spawn = data.path[0].map(|v| v * data.scale);
        let mut kart = KartState::new(spawn);
        for _ in 0..200 {
            kart.tick(
                &ControllerSnapshot {
                    stick_y: 64.,
                    ..default()
                },
                &world,
            );
        }
        assert!(kart.distance(spawn) > 2500.);
        let exit = kart.exit_position(&world).unwrap();
        let mut mario = jumpman_sim::mario_runtime::MarioRuntime::new(
            world.clone(),
            SpawnPoint {
                pos: exit,
                face_yaw: -32768.,
                action: None,
            },
            -5000.,
        );
        for _ in 0..40 {
            mario.step(
                &ControllerSnapshot {
                    stick_y: 64.,
                    stick_mag: 64.,
                    ..default()
                },
                0.,
                false,
                false,
            );
        }
        assert!((DVec3::from(mario.pos) - DVec3::from(exit)).length() > 300.);
        assert!(mario.floor.is_some());
        kart.reset();
        kart.set_yaw(PI / 2.);
        for _ in 0..300 {
            kart.tick(
                &ControllerSnapshot {
                    stick_y: 64.,
                    ..default()
                },
                &world,
            );
        }
        assert!(
            kart.distance(spawn) < 7000.,
            "kart crossed the track boundary"
        );
    }

    #[test]
    fn autopilot_laps_the_real_track() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        if !root.join("luigi-raceway.json").exists() {
            return;
        }
        let data = Raceway::read(&root).unwrap();
        let world = Arc::new(data.world());
        let path: Vec<V3> = data
            .path
            .iter()
            .filter(|p| p[0] > -30000.)
            .map(|p| p.map(|v| v * data.scale))
            .collect();
        let mut kart = KartState::new(path[0]);
        let mut next = 5usize;
        let (mut hits, mut max_speed, mut min_y) = (0, 0f64, 0f64);
        let mut laps_at = None;
        for tick in 0..6000 {
            let target = path[next % path.len()];
            let dx = target[0] - kart.pos[0];
            let dz = target[2] - kart.pos[2];
            if (dx * dx + dz * dz).sqrt() < 600. {
                next += 4;
            }
            let want = dx.atan2(dz);
            let mut diff = want - kart.yaw;
            while diff > PI {
                diff -= 2. * PI;
            }
            while diff < -PI {
                diff += 2. * PI;
            }
            kart.tick(
                &ControllerSnapshot {
                    stick_y: 64.,
                    stick_x: (-diff * 120.).clamp(-64., 64.),
                    ..default()
                },
                &world,
            );
            hits += kart.phys.bumped as i32;
            max_speed = max_speed.max(kart.speed);
            min_y = min_y.min(kart.pos[1] - path[0][1]);
            if next >= path.len() + 5 && laps_at.is_none() {
                laps_at = Some(tick);
            }
        }
        println!(
            "lap at tick {laps_at:?} ({:.1}s), wall bumps {hits}, max speed {max_speed:.2}, lowest {min_y:.0}, node {next}/{}",
            laps_at.unwrap_or(0) as f64 / 30.,
            path.len()
        );
        assert!(laps_at.is_some(), "autopilot should finish a lap, got to node {next}");
    }
}
