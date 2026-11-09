//! MK64 free roam. Shares scripts/export-mk64.py output and driving rules with kart.ts.
//! The track and occupied kart are original MK64 assets; the empty kart is procedural.
use crate::button_marker::LoadWithSettings;
use super::kart_audio;
use super::kart_camera::{CameraKart, CameraPose, KartCamera};
use super::kart_fx::{FxFrame, Kind, KartFx, POOL, VIEW};
use super::kart_puffs::PuffIn;
use super::kart_physics::{Floor, Ground, Input as KartInput, Kart, Wall, WallHit, TOP_SPEED};
use super::kart_race_view::{take_leave_press, take_retry_press, KartRaceView, RaceCmd};
use super::kart_sprite_view::{self, SpriteIn};
use super::kart_sound::{engine_sound, KartSounds, SoundFrame};
use super::kart_rescue::Lakitu;
use super::{dock::DockItem, CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::{to_bevy, SCALE};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions, WallData};
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::math::{to_s16, Vec3 as V3};
use jumpman_sim::state::*;
use jumpman_sim::surfaces::{SURFACE_FLAG_X_PROJECTION, SURFACE_NOT_SLIPPERY};
use serde::Deserialize;
use std::{collections::BTreeMap, f64::consts::PI, path::Path, sync::Arc};

#[derive(Deserialize)]
struct TextureSpec {
    file: String,
    width: f64,
    height: f64,
}
#[derive(Deserialize)]
pub(super) struct Raceway {
    #[serde(default)]
    vertices: Vec<[f64; 8]>,
    #[serde(default)]
    groups: BTreeMap<String, Vec<usize>>,
    #[serde(default)]
    textures: BTreeMap<String, TextureSpec>,
    /// Courses other than Luigi Raceway draw the walkable course's glb (assets root relative).
    #[serde(default)]
    pub(super) model: Option<String>,
    pub(super) path: Vec<V3>,
    pub(super) scale: f64,
    /// Collision of the walkable course (`assets/courses/mk64_<dir>.json`), used when `model` is set.
    #[serde(skip)]
    walk: Option<crate::game::course_data::CollisionData>,
}

/// One selectable kart track (the decomp's course directory name) and its cup.
pub(super) struct Track {
    pub cup: &'static str,
    pub dir: &'static str,
    pub name: &'static str,
}

/// The four cups of Mario Kart 64 in race order.
pub(super) const TRACKS: [Track; 16] = [
    Track { cup: "Mushroom", dir: "luigi_raceway", name: "Luigi Raceway" },
    Track { cup: "Mushroom", dir: "moo_moo_farm", name: "Moo Moo Farm" },
    Track { cup: "Mushroom", dir: "koopa_troopa_beach", name: "Koopa Troopa Beach" },
    Track { cup: "Mushroom", dir: "kalimari_desert", name: "Kalimari Desert" },
    Track { cup: "Flower", dir: "toads_turnpike", name: "Toad's Turnpike" },
    Track { cup: "Flower", dir: "frappe_snowland", name: "Frappe Snowland" },
    Track { cup: "Flower", dir: "choco_mountain", name: "Choco Mountain" },
    Track { cup: "Flower", dir: "mario_raceway", name: "Mario Raceway" },
    Track { cup: "Star", dir: "wario_stadium", name: "Wario Stadium" },
    Track { cup: "Star", dir: "sherbet_land", name: "Sherbet Land" },
    Track { cup: "Star", dir: "royal_raceway", name: "Royal Raceway" },
    Track { cup: "Star", dir: "bowsers_castle", name: "Bowser's Castle" },
    Track { cup: "Special", dir: "dks_jungle_parkway", name: "D.K.'s Jungle Parkway" },
    Track { cup: "Special", dir: "yoshi_valley", name: "Yoshi Valley" },
    Track { cup: "Special", dir: "banshee_boardwalk", name: "Banshee Boardwalk" },
    Track { cup: "Special", dir: "rainbow_road", name: "Rainbow Road" },
];
/// The battle arenas have no lap path: they are walk-only (LEVELS tab) and not offered by the track picker.
pub(super) const WALK_ONLY: [(&str, &str); 4] = [
    ("Big Donut", "battle arena, no lap path"),
    ("Block Fort", "battle arena, no lap path"),
    ("Double Deck", "battle arena, no lap path"),
    ("Skyscraper", "battle arena, no lap path"),
];

static SELECTED: std::sync::Mutex<Option<usize>> = std::sync::Mutex::new(None);
static TRACK_CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Index into `TRACKS` of the track the next kart session loads: `JUMPMAN_KART_COURSE=<dir>` (the decomp's
/// directory name, with or without the `mk64_` prefix), else the one picked in the mode (Luigi Raceway first).
pub(super) fn selected_index() -> usize {
    if let Some(i) = SELECTED.lock().ok().and_then(|g| *g) {
        return i;
    }
    if let Ok(v) = std::env::var("JUMPMAN_KART_COURSE") {
        let v = v.trim_start_matches(crate::game::mk64::PREFIX);
        if let Some(i) = TRACKS.iter().position(|t| t.dir == v) {
            return i;
        }
    }
    0
}
pub(super) fn selected_track() -> &'static Track {
    &TRACKS[selected_index()]
}
/// Pick the next (or previous) track; the experiment restarts on it (see `restart_on_track_change`).
pub(super) fn cycle_track(step: isize) {
    let next = (selected_index() as isize + step).rem_euclid(TRACKS.len() as isize) as usize;
    if let Ok(mut g) = SELECTED.lock() {
        *g = Some(next);
    }
    TRACK_CHANGED.store(true, std::sync::atomic::Ordering::SeqCst);
}
/// Minimap picture of the selected track (Luigi Raceway's is the extracted outline, the others are drawn from the path).
pub(super) fn minimap_file() -> String {
    format!("hud-minimap-{}.png", selected_track().dir.replace('_', "-"))
}
/// A track change restarts the kart mode through the experiment manager (fresh course load, then `enter`).
pub(super) fn restart_on_track_change(mut slot: ResMut<super::ExperimentSlot>) {
    if TRACK_CHANGED.swap(false, std::sync::atomic::Ordering::SeqCst) && slot.id == Some(ExperimentId::Kart) {
        slot.request = Some(ExperimentId::Kart);
    }
}

impl Raceway {
    /// Luigi Raceway (the original kart course, drawn from its own display-list groups).
    pub(super) fn read(root: &Path) -> Result<Self, String> {
        Self::read_course(root, None, "luigi_raceway")
    }
    /// A kart course by decomp directory name; `assets` is the assets root (for the walkable course data).
    pub(super) fn read_course(root: &Path, assets: Option<&Path>, dir: &str) -> Result<Self, String> {
        let file = format!("{}.json", dir.replace('_', "-"));
        let bytes = std::fs::read(root.join(&file))
            .map_err(|e| format!("MK64 assets missing: {e}. Run python3 scripts/export-mk64.py <ROM>"))?;
        let mut data: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if data.path.is_empty() || data.scale != 8. {
            return Err("Invalid MK64 course export".into());
        }
        if dir != "luigi_raceway" {
            let assets = assets.ok_or("assets root needed")?;
            let model = data.model.as_deref().ok_or("course has no model")?;
            if !assets.join(model).is_file() {
                return Err(format!("Missing MK64 course model {model}"));
            }
            let walk = crate::game::course_data::CourseData::load(assets, &format!("{}{dir}", crate::game::mk64::PREFIX), 1)?;
            data.walk = Some(walk.collision);
        } else if data.groups.is_empty() {
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
    /// Extra mode: the course flipped left-right (`gIsMirrorMode`: x is negated for the path, the collision and the
    /// drawing). Vertices, triangle winding, the lap path and the walkable collision are mirrored here; the glb of
    /// the walkable courses is mirrored by its scene transform.
    pub(super) fn mirrored(mut self) -> Self {
        for v in &mut self.vertices {
            v[0] = -v[0];
        }
        for indices in self.groups.values_mut() {
            for tri in indices.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
        }
        // The export ends the path with a (-32768, -32768, -32768) sentinel: keep it as it is.
        for p in self.path.iter_mut().filter(|p| p[0] > -30000.) {
            p[0] = -p[0];
        }
        if let Some(walk) = &mut self.walk {
            for v in &mut walk.vertices {
                v[0] = -v[0];
            }
            for tri in &mut walk.triangles {
                tri.swap(1, 2);
            }
        }
        self
    }
    pub(super) fn world(&self) -> CollisionWorld {
        let mut world = CollisionWorld::new(1024., 100000.);
        if let Some(walk) = &self.walk {
            crate::game::native_collision::add_native_collision(&mut world, walk, [0.; 3], 0.);
            world.finalize();
            return world;
        }
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

/// Display-list groups that are drawn over another group occupying the very same plane (sand shoulders over the
/// grass, the lamp bricks over the wall bricks, kerbs over the road). The N64 resolves those by draw order; a depth
/// buffer cannot, so the pair z-fights and shimmers. Each layer is lifted off its surface by `DECAL_LIFT`
/// (shared with `kart.ts`).
const DECAL_LAYERS: [(&str, u32); 8] = [
    ("gLRTexture673C68", 1),
    ("gLRTexture6735DC", 2),
    ("gLRTexture653608", 1),
    ("gLRTexture65112C", 1),
    ("gLRTexture67BBD8", 1),
    ("gLRTextureRoad1", 2),
    ("gLRTextureRoad2", 2),
    ("gLRTextureRoadFinish1", 3),
];
const DECAL_LIFT: f32 = 0.6;

/// Push each triangle of a decal group off its plane (towards the sky for floors) in Bevy units.
fn lift_decal(group: &str, positions: &mut [[f32; 3]]) {
    let Some(&(_, layer)) = DECAL_LAYERS.iter().find(|(name, _)| *name == group) else { return };
    for tri in positions.chunks_exact_mut(3) {
        let [a, b, c] = [Vec3::from(tri[0]), Vec3::from(tri[1]), Vec3::from(tri[2])];
        let mut n = (b - a).cross(c - a).normalize_or_zero();
        if n.y < 0. {
            n = -n;
        }
        let lift = n * (layer as f32 * DECAL_LIFT * SCALE);
        for v in tri.iter_mut() {
            *v = (Vec3::from(*v) + lift).to_array();
        }
    }
}

/// Course units -> world units (the MK64 export scale).
const UNIT: f64 = 8.;

/// Adapts the sim's collision world to the kart physics (which works in course units).
pub(super) struct WorldGround<'a>(pub(super) &'a CollisionWorld);
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
    fn wall(&self, pos: [f64; 3]) -> Option<Wall> {
        let mut data = WallData { x: pos[0] * UNIT, y: pos[1] * UNIT, z: pos[2] * UNIT, offset_y: 50., radius: 65., walls: vec![] };
        if self.0.find_wall_collisions(&mut data, false) == 0 {
            return None;
        }
        Some(Wall {
            pos: [data.x / UNIT, data.y / UNIT, data.z / UNIT],
            hits: data
                .walls
                .iter()
                .map(|s| WallHit { normal: [s.normal[0], s.normal[1], s.normal[2]], x_facing: s.flags & SURFACE_FLAG_X_PROJECTION != 0 })
                .collect(),
        })
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
    /// Graceful out-of-bounds rescue (set once the course path is known).
    lakitu: Option<Lakitu>,
    /// The kart was just put back on the track; the host snaps the camera and clears this.
    teleported: bool,
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
            lakitu: None,
            teleported: false,
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
        let mut lakitu = self.lakitu.take();
        if let Some(l) = &mut lakitu {
            l.forget();
        }
        *self = Self::new(self.spawn);
        self.lakitu = lakitu;
    }
    /// Lakitu drops the kart on the path node nearest to where it last had ground, facing down the track.
    fn rescue_to(&mut self, world: &CollisionWorld) {
        let Some((mut pos, yaw)) = self.lakitu.as_ref().and_then(|l| l.destination(self.pos)) else {
            return self.reset();
        };
        let floor = world.find_floor(pos[0], pos[1] + 200., pos[2]);
        if floor.surface.is_some() {
            pos[1] = floor.height;
        }
        self.pos = pos;
        self.driving = true;
        self.phys = Kart::new(pos.map(|v| v / UNIT), 0);
        self.set_yaw(yaw);
        self.speed = 0.;
        self.teleported = true;
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
    /// Lakitu's rescue, one tick: true while the kart is held (physics paused).
    fn lakitu_holds(&mut self, world: &CollisionWorld) -> bool {
        if let Some(l) = &mut self.lakitu {
            if l.active() {
                let teleport = l.tick();
                let holding = l.holding();
                if teleport {
                    self.rescue_to(world);
                }
                return holding;
            }
        }
        false
    }
    /// After the kart stepped in free roam: remember solid ground, call Lakitu when it fell off the course.
    fn lakitu_after(&mut self) {
        if let Some(l) = &mut self.lakitu {
            if !self.phys.midair {
                l.note_grounded(self.pos);
            }
            if l.should_rescue(self.pos) {
                l.start();
                kart_audio::play(super::kart_sound::Cue::Lakitu);
            }
        } else if self.pos[1] < -5000. {
            // No course path (tests): hard reset to the line.
            self.reset();
        }
    }
    /// One 30 Hz sim tick = two 60 Hz MK64 kart updates (see `kart_physics`). The experiment drives the kart
    /// through the item layer (`tick_free` / `tick_race`); this plain step is what the unit tests exercise.
    #[cfg(test)]
    fn tick(&mut self, input: &ControllerSnapshot, world: &CollisionWorld) {
        if self.lakitu_holds(world) {
            return;
        }
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
        self.lakitu_after();
    }
    /// Race mode (`kart_race_view`): the race layer steps the kart (countdown hold, spin, boost) and
    /// puts it back on the last gate after a fall, so Lakitu stays out of it.
    fn tick_race(&mut self, input: &ControllerSnapshot, race: &mut KartRaceView, world: &CollisionWorld) {
        let pad = KartInput {
            raw_x: (input.stick_x * 80. / 64.).round() as i32,
            raw_y: (input.stick_y * 80. / 64.).round() as i32,
            accel: input.stick_y > 16.,
            brake: input.button_down & Z_TRIG != 0 || input.stick_y < -16.,
            hop_down: input.button_down & A_BUTTON != 0,
            hop_pressed: input.button_pressed & A_BUTTON != 0,
        };
        // B holds a trailing item (banana, fake box, a shell behind the kart); letting go drops or fires it.
        race.item_held(input.button_down & B_BUTTON != 0);
        race.step(&pad, input.button_pressed & B_BUTTON != 0, &mut self.phys, &WorldGround(world));
        self.pos = self.phys.pos.map(|v| v * UNIT);
        self.yaw = self.phys.yaw();
        self.speed = self.phys.speed;
    }
    /// Free roam with items: Lakitu still fishes the kart out, the item layer steps the kart.
    fn tick_free(&mut self, input: &ControllerSnapshot, race: &mut KartRaceView, world: &CollisionWorld) {
        if self.lakitu_holds(world) {
            return;
        }
        self.tick_race(input, race, world);
        self.lakitu_after();
    }
}

/// Textures of the effect particles (MK64 smoke / flame / spark sprites, a soft glow and a star).
struct FxTextures {
    soft: Handle<Image>,
    star: Handle<Image>,
    fire: Handle<Image>,
    smoke: [Handle<Image>; 4],
    spark: [Handle<Image>; 4],
    drift_small: Handle<Image>,
    drift_large: Handle<Image>,
    grass: Handle<Image>,
}

/// A white four-point star with a soft glow, 32 x 32.
fn star_texture() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let n = 32usize;
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x as f32 + 0.5 - 16.) / 16., (y as f32 + 0.5 - 16.) / 16.);
            let arm = (1. - (dx.abs().min(dy.abs()) * 5.).min(1.)) * (1. - dx.hypot(dy)).max(0.);
            let glow = (1. - dx.hypot(dy) * 1.4).max(0.).powi(2) * 0.6;
            let a = ((arm + glow).min(1.) * 255.) as u8;
            data.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

pub struct KartExperiment {
    state: KartState,
    ready: bool,
    sprite: Option<Entity>,
    sprite_material: Option<Handle<StandardMaterial>>,
    parked: Option<Entity>,
    toggle: bool,
    camera: KartCamera,
    /// The camera pose before and after the last 30 Hz tick, blended by the render alpha.
    poses: Option<(CameraPose, CameraPose)>,
    fx: KartFx,
    /// Pooled camera-facing quads that draw `fx`.
    fx_quads: Vec<(Entity, Handle<StandardMaterial>)>,
    fx_tex: Option<FxTextures>,
    sounds: KartSounds,
    accel: bool,
    bumps_seen: u32,
    bumped_tick: bool,
    /// Full-screen black overlay for the Lakitu rescue fade.
    fade: Option<Entity>,
    fade_alpha: f32,
    /// Card wording follows the touch overlay: (touch shown, seconds the card has been up).
    card_touch: bool,
    card_age: f64,
    race: KartRaceView,
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
            camera: KartCamera::new(),
            poses: None,
            fx: KartFx::default(),
            fx_quads: vec![],
            fx_tex: None,
            sounds: KartSounds::default(),
            accel: false,
            bumps_seen: 0,
            bumped_tick: false,
            fade: None,
            fade_alpha: 0.,
            card_touch: false,
            card_age: 0.,
            race: KartRaceView::new(),
        }
    }
    /// Drift / mini-turbo readout for the status line: charge pips while drifting, a flash for the boost.
    fn drift_status(&self) -> String {
        let k = &self.state.phys;
        if k.mini_turbo {
            " | MINI-TURBO!".into()
        } else if k.drifting {
            let pips = k.drift_state.clamp(0, 2) as usize;
            format!(" | DRIFT [{}{}]{}", "#".repeat(pips), "-".repeat(2 - pips), if pips >= 2 { " RELEASE" } else { "" })
        } else {
            String::new()
        }
    }
    fn ui(&self, host: &mut Host) {
        let near = self.state.can_enter(host.sim.mario.pos);
        let touch = host.touch;
        let key = |label: &str, key: &str| if touch { label.to_string() } else { format!("{label} · {key}") };
        host.set_dock(self.race.dock(vec![
            DockItem::new(
                &if self.state.driving {
                    key("Get out", "F")
                } else if near {
                    key("Get in", "F")
                } else {
                    "Return to kart".into()
                },
                KeyCode::KeyF,
                0,
            ),
            DockItem::new(&key("Reset", "R"), KeyCode::KeyR, 0),
            DockItem::new(&if touch { "Track".to_string() } else { format!("Track · V ({})", selected_track().name) }, KeyCode::KeyV, 0),
            DockItem::new(&if touch { super::kart_setup::cc_label().to_string() } else { format!("Class · C ({})", super::kart_setup::cc_label()) }, KeyCode::KeyC, 0),
            DockItem::new(&if touch { super::kart_setup::racer_label().to_string() } else { format!("Racer · H ({})", super::kart_setup::racer_label()) }, KeyCode::KeyH, 0),
        ], touch));
        host.status(if self.race.active() {
            String::new()
        } else if self.state.driving {
            format!("DRIVING | SPEED {:.0}%{}", self.state.throttle_percent(), self.drift_status())
        } else if touch {
            "ON FOOT | Walk back to the kart".into()
        } else {
            "ON FOOT | Walk back to the kart to drive".into()
        });
        // Touch buttons say what they do here: A hops and drifts (and glows while a drift charges), B gets in/out.
        let k = &self.state.phys;
        host.labels.jump = Some(if self.state.driving { "HOP\nDRIFT".into() } else { "JUMP".into() });
        host.labels.punch = Some(if self.state.driving { "ITEM".into() } else { "GET\nIN".into() });
        host.labels.crouch = Some(if self.state.driving { "BRAKE".into() } else { "Z".into() });
        host.labels.jump_tint = if !self.state.driving {
            None
        } else if k.mini_turbo {
            Some(Color::srgba(1.0, 0.5, 0.05, 0.95))
        } else if k.drifting {
            Some(Color::srgba(0.95, 0.72, 0.1, 0.9))
        } else {
            None
        };
    }
    /// Title card: keyboard wording on desktop, finger wording (and shorter) on touch.
    fn set_card(&mut self, host: &mut Host) {
        self.card_touch = host.touch;
        if host.touch {
            let track = selected_track();
            host.card(
                "10",
                &format!("{} · {} Cup", track.name, track.cup),
                "",
                &[
                    ("TRACK", "tap Track to change the course"),
                    ("RACE", "tap RACE for 3 laps vs 3 karts"),
                    ("Stick", "steer; up drives, down brakes"),
                    ("A", "hop; hold while turning to drift"),
                    ("B / ITEM", "use a box item"),
                ],
            );
        } else {
            let track = selected_track();
            host.card(
                "10",
                &format!("{} · {} Cup", track.name, track.cup),
                "Drive through the item boxes, or press T for a 3-lap race against 3 CPU karts.",
                &[
                    ("V · TRACK", "next course (16 tracks in four cups); the arenas are walk-only, see LEVELS"),
                    ("T · RACE", "3-lap Grand Prix, 3 CPU karts"),
                    ("W/S · stick", "accelerate / brake / reverse"),
                    ("A/D · stick", "steer"),
                    ("Space · A", "hop; hold while steering to drift, counter-steer to charge a mini-turbo"),
                    ("E · Shift · B", "use the item (mushroom, banana, green shell)"),
                    ("F", "get out / get in nearby"),
                    ("R", "reset to starting line"),
                ],
            );
        }
        self.card_age = 0.;
    }
    /// The kart was put somewhere new (start, reset, rescue, getting back in): drop the camera behind it.
    fn snap_camera(&mut self) {
        let k = self.camera_kart(false);
        self.camera.snap(&k);
        self.poses = None;
        self.sounds.reset();
        self.bumps_seen = self.state.phys.bumps;
    }
    fn camera_kart(&self, bumped: bool) -> CameraKart {
        let me = self.race.me();
        CameraKart::of(&self.state.phys, me.is_some_and(|m| m.boost > 0), me.is_some_and(|m| m.spin > 0), bumped)
    }
    fn sound_frame(&self) -> SoundFrame {
        let k = &self.state.phys;
        let me = self.race.me();
        SoundFrame {
            hop: k.hop,
            drifting: k.drifting,
            drift_state: k.drift_state,
            mini_turbo: k.mini_turbo,
            bumped: self.bumped_tick,
            midair: k.midair,
            speed_ratio: k.current_speed / TOP_SPEED,
            grass: k.grass,
            accel: self.accel,
            reversing: k.back_up,
            roulette: me.map_or(0, |m| m.roulette),
            clock: self.race.clock(),
            spinning: me.is_some_and(|m| m.spin > 0),
            boosting: me.is_some_and(|m| m.boost > 0),
            driving: self.state.driving,
        }
    }
    /// Two 60 Hz camera updates per 30 Hz tick, then the sounds and box shards for what just happened.
    fn tick_effects(&mut self, host: &mut Host, input: &ControllerSnapshot) {
        let bumps = self.state.phys.bumps;
        self.bumped_tick = bumps != self.bumps_seen;
        self.bumps_seen = bumps;
        self.accel = input.stick_y > 16.;
        let ck = self.camera_kart(self.bumped_tick);
        let before = self.camera.initialised().then(|| self.camera.pose());
        self.camera.step(&ck);
        self.camera.step(&CameraKart { bumped: false, ..ck });
        let after = self.camera.pose();
        self.poses = Some((before.unwrap_or(after), after));
        for cue in self.sounds.update(&self.sound_frame(), &self.race.last_events) {
            kart_audio::play(cue);
        }
        for at in std::mem::take(&mut self.race.smashed) {
            self.fx.burst(at);
        }
        for (at, colour) in std::mem::take(&mut self.race.hit_fx) {
            self.fx.smash(at, colour);
        }
    }
    /// Spawn the pooled quads for the effects and load their MK64 textures (smoke, flame, sparks).
    fn build_fx(&mut self, host: &mut Host) {
        let soft = host.assets.add(crate::fx::particles::soft_texture());
        let star = host.assets.add(star_texture());
        let smooth = |host: &Host, file: &str| -> Handle<Image> {
            host.assets.load_builder_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
                s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::ClampToEdge,
                    address_mode_v: ImageAddressMode::ClampToEdge,
                    ..ImageSamplerDescriptor::linear()
                });
            })
        };
        let tex = FxTextures {
            soft: soft.clone(),
            star,
            fire: smooth(host, "mk64-particle-fire.png"),
            smoke: [1, 2, 3, 4].map(|i| smooth(host, &format!("mk64-particle-smoke-{i}.png"))),
            spark: [1, 2, 3, 4].map(|i| smooth(host, &format!("mk64-particle-spark-{i}.png"))),
            drift_small: smooth(host, "mk64-drift-puff-small.png"),
            drift_large: smooth(host, "mk64-drift-puff-large.png"),
            grass: smooth(host, "mk64-grass-particle.png"),
        };
        let quad = host.meshes.add(Rectangle::new(1., 1.));
        for _ in 0..POOL + VIEW {
            let material = host.materials.add(StandardMaterial {
                base_color_texture: Some(soft.clone()),
                base_color: Color::srgba(1., 1., 1., 0.),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            });
            let e = host.spawn((Mesh3d(quad.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
            self.fx_quads.push((e, material));
        }
        self.fx_tex = Some(tex);
    }
    /// Place the live particles facing the camera `eye` (sim units).
    fn draw_fx(&mut self, host: &mut Host, eye: Vec3) {
        let Some(tex) = self.fx_tex.as_ref() else { return };
        for (p, (entity, material)) in self.fx.drawn().zip(&self.fx_quads) {
            if !p.alive {
                host.commands.entity(*entity).insert(Visibility::Hidden);
                continue;
            }
            let at = to_bevy(p.pos);
            let to_eye = (eye - at).normalize_or_zero();
            let mut t = Transform::from_translation(at).looking_to(-to_eye, Vec3::Y);
            if p.kind == Kind::Streak {
                // Long axis along the flight path (world +y -> local y).
                let along = to_bevy(p.velocity()).normalize_or_zero();
                let y = (along - to_eye * along.dot(to_eye)).normalize_or_zero();
                if y != Vec3::ZERO {
                    let x = y.cross(to_eye).normalize_or_zero();
                    t.rotation = Quat::from_mat3(&Mat3::from_cols(x, y, to_eye));
                }
            }
            let size = p.size() as f32 * SCALE;
            t.scale = Vec3::new(size * p.kind.aspect() as f32, size, 1.);
            host.commands.entity(*entity).insert((t, Visibility::Inherited));
            if let Some(mut m) = host.materials.get_mut(material) {
                let c = Color::srgb_u8((p.color >> 16) as u8, (p.color >> 8) as u8, p.color as u8).to_srgba();
                m.base_color = Color::srgba(c.red, c.green, c.blue, p.opacity() as f32);
                let image = match p.kind {
                    Kind::Soft | Kind::Streak => &tex.soft,
                    Kind::Smoke => &tex.smoke[p.variant as usize % 4],
                    Kind::Fire => &tex.fire,
                    Kind::Spark => &tex.spark[p.variant as usize % 4],
                    Kind::Star => &tex.star,
                    Kind::DriftSmall => &tex.drift_small,
                    Kind::DriftLarge => &tex.drift_large,
                    Kind::Grass => &tex.grass,
                };
                if m.base_color_texture.as_ref() != Some(image) {
                    m.base_color_texture = Some(image.clone());
                }
                let mode = if p.kind.additive() { AlphaMode::Add } else { AlphaMode::Blend };
                if m.alpha_mode != mode {
                    m.alpha_mode = mode;
                }
            }
        }
    }
    fn fx_frame(&self, p: DVec3) -> FxFrame {
        let k = &self.state.phys;
        let spun = self.race.me().is_some_and(|m| m.spin > 0);
        let mut puff = PuffIn {
            pos: p.into(),
            yaw: self.state.yaw,
            slip: k.slip_raw(),
            current_speed: k.current_speed,
            kmh: k.kmh(),
            character: 0,
            driving: self.state.driving,
            throttle: self.accel,
            mushroom: self.sound_frame().boosting,
            human: true,
            drifting: k.drifting,
            hop: k.hop,
            midair: k.midair,
            disabled: spun,
            drift_state: k.drift_state,
            drift_duration: k.drift_duration(),
            tyres: k.back_tyres().map(|t| [t[0] * UNIT, t[1] * UNIT, t[2] * UNIT]),
            tyre_grass: k.back_grass(),
        };
        // Test hook: `JUMPMAN_KART_FX=drift0|drift1|drift2|boost|grass|spin` forces that effect on the player's kart.
        if let Ok(mode) = std::env::var("JUMPMAN_KART_FX") {
            puff.throttle = true;
            puff.current_speed = puff.current_speed.max(0.9 * TOP_SPEED);
            puff.kmh = puff.kmh.max(60.);
            match mode.as_str() {
                "drift0" | "drift1" | "drift2" => {
                    puff.drifting = true;
                    puff.hop = false;
                    puff.midair = false;
                    puff.slip = puff.slip.signum().max(1) * 22 * 182;
                    puff.drift_state = (mode.as_bytes()[5] - b'0') as i32;
                    puff.drift_duration = 20 + 40 * puff.drift_state;
                }
                "boost" => puff.mushroom = true,
                "grass" => puff.tyre_grass = [true, true],
                "spin" => puff.disabled = true,
                _ => {}
            }
        }
        FxFrame { pos: p.into(), yaw: self.state.yaw, driving: self.state.driving, midair: k.midair, spun: puff.disabled, puff }
    }
    /// T: start (or restart) a three-lap race from the grid.
    fn start_race(&mut self, host: &mut Host) {
        if !self.ready {
            return;
        }
        self.state.driving = true;
        self.state.teleported = false;
        if let Some(l) = &mut self.state.lakitu {
            l.forget();
        }
        self.toggle = false;
        host.set_player_visible(false);
        let world = host.sim.mario.world.clone();
        self.race.begin(host, &mut self.state.phys, &WorldGround(&world));
        self.state.pos = self.state.phys.pos.map(|v| v * UNIT);
        self.state.yaw = self.state.phys.yaw();
        self.state.speed = 0.;
        self.snap_camera();
        host.place_mario(self.state.pos, Some(self.state.yaw * 32768. / PI));
        self.ui(host);
    }
    /// Free roam with item boxes from the starting line (after a race, a reset or on entering).
    fn begin_free_roam(&mut self, host: &mut Host) {
        let world = host.sim.mario.world.clone();
        self.race.begin_practice(host, &self.state.phys, &WorldGround(&world));
        self.snap_camera();
    }
    /// N / Back to free roam: drop the race and return to the starting line.
    fn end_race(&mut self, host: &mut Host) {
        self.race.end(host);
        self.state.reset();
        self.begin_free_roam(host);
        self.toggle = false;
        host.place_mario(self.state.pos, Some(-32768.));
        host.set_player_visible(false);
        self.ui(host);
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
            self.snap_camera();
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
    /// Course surfaces: N64-style bilinear magnification, and anisotropic filtering so the road, kerbs and grass
    /// do not shimmer at grazing angles (the textures have no mip chain).
    fn course_texture(host: &Host, file: &str) -> Handle<Image> {
        host.assets.load_builder_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 16,
                ..default()
            });
        })
    }
    pub(super) fn texture(host: &Host, file: &str) -> Handle<Image> {
        host.assets
            .load_builder_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
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
        let track = selected_track();
        let mirror = super::kart_setup::mirrored();
        let mk64_root = host.asset_root.0.join("../public/mk64");
        let mut data = Raceway::read_course(&mk64_root, Some(&host.asset_root.0), track.dir)?;
        if mirror {
            data = data.mirrored();
        }
        self.race.set_course(&mk64_root, track.dir);
        let world = data.world();
        let mut spawn = data.path[0].map(|v| v * data.scale);
        let floor = world.find_floor(spawn[0], spawn[1] + 200., spawn[2]);
        if floor.surface.is_none() {
            return Err(format!("{} spawn has no floor", track.name));
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
            lift_decal(name, &mut positions);
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.compute_flat_normals();
            let texture = Self::course_texture(host, &spec.file);
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
        let look = if let Some(model) = &data.model {
            // Every course but Luigi Raceway is drawn from its walkable-course glb (decal layers, wrap modes and
            // translucent water baked in by the exporter).
            let scale = crate::zelda::glb::bevy_scale(&host.asset_root.0.join(model));
            let course_scene = crate::render::scene(host.assets, model);
            let flip = if mirror { -1. } else { 1. };
            host.spawn((course_scene, Transform::from_scale(Vec3::new(scale * flip, scale, scale)), crate::render::decal::DecalScene));
            crate::game::mk64::look(&host.asset_root.0, &format!("{}{}", crate::game::mk64::PREFIX, track.dir))
        } else {
            None
        };
        let texture = Self::texture(host, "mario-kart.png");
        let material = kart_sprite_view::sprite_material(host, super::kart_setup::get().character);
        let mesh = kart_sprite_view::sprite_mesh(host);
        self.sprite = Some(host.spawn((Mesh3d(mesh), MeshMaterial3d(material.clone()), Transform::default())));
        self.sprite_material = Some(material);
        self.race.build(host, texture.clone(), &data.path);
        self.build_fx(host);
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
        let nodes: Vec<V3> = data.path.iter().map(|p| p.map(|v| v * data.scale)).collect();
        self.state.lakitu = Some(Lakitu::new(&nodes));
        kart_audio::set_active(true);
        super::kart_music::session_start(selected_track().dir);
        self.fade = Some(host.spawn((
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.), height: Val::Percent(100.), ..default() },
            BackgroundColor(Color::srgba(0., 0., 0., 0.)),
            GlobalZIndex(35),
            Visibility::Hidden,
        )));
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
        if let Some(l) = &look {
            let [top, mid, bottom] = l.sky;
            host.set_sky(crate::fx::sky::SkyPalette::from_hex(top, mid, bottom));
            let c = crate::fx::particles::hex_rgb(l.fog.0);
            // The kart camera sits higher and further back than the walking one.
            host.cam.fog = Some((Color::srgb(c[0], c[1], c[2]), l.fog.1 as f64 * 1.4, l.fog.2 as f64 * 1.6));
        }
        self.set_card(host);
        self.ready = true;
        self.begin_free_roam(host);
        self.ui(host);
        host.toast(if host.touch { "Grab the item boxes, or tap RACE for 3 laps" } else { "Grab the item boxes, or press T for a 3-lap race" }, 5.);
        // Test hook: JUMPMAN_KART_RACE=1 starts a race at once (see kart_race_view::test_hook for the others).
        if std::env::var_os("JUMPMAN_KART_RACE").is_some() {
            self.start_race(host);
        }
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
                "Assets unavailable. Run python3 scripts/export-mk64.py <ROM>, then reopen the map.",
                &[],
            );
            host.toast(&error, 8.);
        }
    }
    fn exit(&mut self, host: &mut Host) {
        kart_audio::set_active(false);
        super::kart_music::session_end();
        self.race.end(host);
        *host.labels = default();
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
        if self.race.active() && take_retry_press() {
            self.start_race(host);
        } else if self.race.active() && take_leave_press() {
            self.end_race(host);
        } else if !self.race.active() && (self.toggle || (!self.state.driving && input.button_pressed & B_BUTTON != 0)) {
            self.toggle = false;
            self.switch(host);
        } else if self.state.driving {
            if self.race.active() {
                self.state.tick_race(input, &mut self.race, &host.sim.mario.world);
            } else {
                self.state.tick_free(input, &mut self.race, &host.sim.mario.world);
            }
            self.tick_effects(host, input);
            if std::mem::take(&mut self.state.teleported) {
                // Lakitu has put the kart back on the track: snap the camera behind it.
                self.snap_camera();
                host.place_mario(self.state.pos, Some(self.state.yaw * 32768. / PI));
                host.set_player_visible(false);
                host.toast("Lakitu fished you out.", 2.);
            }
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
        match self.race.key(key) {
            Some(RaceCmd::Start) => {
                self.start_race(host);
                return true;
            }
            Some(RaceCmd::Leave) => {
                self.end_race(host);
                return true;
            }
            Some(RaceCmd::Handled) => return true,
            None => {}
        }
        match key {
            KeyCode::KeyF => {
                self.toggle = !self.race.active();
                true
            }
            KeyCode::KeyV if !self.race.active() => {
                cycle_track(1);
                host.toast(&format!("Loading {}...", selected_track().name), 2.);
                true
            }
            KeyCode::KeyC if !self.race.active() => {
                // 50cc -> 100cc -> 150cc -> Extra (the mirrored course: reload it) -> 50cc.
                if super::kart_setup::cycle_cc() {
                    TRACK_CHANGED.store(true, std::sync::atomic::Ordering::SeqCst);
                    host.toast(&format!("{}: loading the course...", super::kart_setup::cc_label()), 2.);
                } else {
                    host.toast(&format!("Class: {}", super::kart_setup::cc_label()), 1.5);
                    self.ui(host);
                }
                true
            }
            KeyCode::KeyH if !self.race.active() => {
                super::kart_setup::cycle_racer();
                let stats = self.race.player_stats();
                host.toast(
                    &format!("Racer: {} ({})", super::kart_setup::racer_label(), super::kart_stats::weight_class(stats.character)),
                    1.5,
                );
                if let Some(material) = self.sprite_material.clone() {
                    kart_sprite_view::set_character(host, &material, stats.character);
                }
                self.ui(host);
                true
            }
            KeyCode::KeyR => {
                self.state.reset();
                self.begin_free_roam(host);
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
        if host.touch != self.card_touch {
            self.set_card(host);
        } else if host.touch {
            // The phone card is only a reminder: it steps aside after a few seconds of driving.
            self.card_age += dt;
            if self.card_age > 12. && !host.ui.card.is_empty() {
                host.ui.card.clear();
            }
        }
        let alpha = self.state.lakitu.as_ref().map_or(0., Lakitu::alpha);
        if let (Some(fade), true) = (self.fade, alpha != self.fade_alpha) {
            self.fade_alpha = alpha;
            host.commands.entity(fade).insert((
                BackgroundColor(Color::srgba(0., 0., 0., alpha)),
                if alpha > 0. { Visibility::Inherited } else { Visibility::Hidden },
            ));
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
                // The real MK64 camera (kart_camera.rs), stepped at 60 Hz in course units and blended between 30 Hz ticks.
                let pose = match self.poses {
                    Some((a, b)) => a.lerp(&b, host.alpha.clamp(0., 1.)),
                    None => self.camera.pose(),
                };
                let eye = DVec3::from(pose.eye) * UNIT;
                host.cam.pose = Some(CamPose { eye, look: DVec3::from(pose.look) * UNIT, up: DVec3::Y, fov: pose.fov, near: 10., far: 65000. });
                let k = &self.state.phys;
                let mut input = SpriteIn {
                    pos: p,
                    eye,
                    yaw: self.state.yaw,
                    slip: k.slip_raw(),
                    slope_accel: k.slope_accel,
                    tilt: k.view_tilt(),
                    spin: self.race.spin_angle(),
                    spinning: self.race.me().is_some_and(|m| m.spin > 0),
                    midair: k.midair,
                    look: self.race.player_look(),
                };
                // Test hooks: `JUMPMAN_KART_FX=turn` turns the kart slowly through every view angle (frame selection),
                // `=spin` spins it through the spin-out table.
                match std::env::var("JUMPMAN_KART_FX").as_deref() {
                    Ok("turn") => input.spin = host.time() * 0.9,
                    Ok("spin") => {
                        input.spinning = true;
                        input.spin = host.time() * 4.;
                    }
                    _ => {}
                }
                let world = host.sim.mario.world.clone();
                self.race.frame(host, eye, host.alpha, &WorldGround(&world));
                if let Some(material) = self.sprite_material.clone() {
                    kart_sprite_view::place_sprite(host, sprite, &material, &input);
                }
                self.race.shadow(host, 0, &input, &WorldGround(&world));
                let eye = to_bevy(eye.into());
                // Sparks, smoke and exhaust flames; the engine note follows the speed.
                let mut frames = vec![self.fx_frame(p)];
                frames.extend(self.race.cpu_fx_frames());
                self.fx.update(dt, &frames);
                self.draw_fx(host, eye);
                kart_audio::set_engine(engine_sound(&self.sound_frame()));
            } else {
                host.cam.pose = None;
                let world = host.sim.mario.world.clone();
                let parked = DVec3::from(self.state.pos);
                let input = SpriteIn { pos: parked, eye: parked, yaw: self.state.yaw, slip: 0, slope_accel: 0, tilt: (0., 0.), spin: 0., spinning: false, midair: false, look: Default::default() };
                self.race.shadow(host, 0, &input, &WorldGround(&world));
                kart_audio::set_engine(engine_sound(&self.sound_frame()));
            }
        }
    }
    fn debug(&self) -> String {
        format!(
            "ready={} driving={} speed={:.1} pos={:?} fov={:.1} engine={}{}",
            self.ready,
            self.state.driving,
            self.state.speed,
            self.state.pos,
            self.camera.fov,
            kart_audio::last_engine().map_or(String::new(), |e| format!("{:.0}Hz/{:.2}/{:.2}", e.freq, e.gain, e.skid)),
            self.race.debug()
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
    fn falling_off_the_course_is_rescued_gracefully_to_the_nearest_node() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        if !root.join("luigi-raceway.json").exists() {
            return;
        }
        let data = Raceway::read(&root).unwrap();
        let world = Arc::new(data.world());
        let nodes: Vec<V3> = data.path.iter().map(|p| p.map(|v| v * data.scale)).collect();
        let on_track = nodes[50];
        let mut start = on_track;
        start[1] = world.find_floor(start[0], start[1] + 200., start[2]).height;
        let mut kart = KartState::new(start);
        kart.lakitu = Some(Lakitu::new(&nodes));
        let go = ControllerSnapshot { stick_y: 64., ..default() };
        for _ in 0..30 {
            kart.tick(&go, &world);
        }
        assert!(!kart.phys.midair, "grounded on the road");
        // Leave the map sideways: there is no floor out there, so the kart just falls.
        kart.pos[0] += 60000.;
        kart.phys.pos = kart.pos.map(|v| v / UNIT);
        let (mut max_alpha, mut teleports, mut held) = (0f32, 0, 0);
        let mut after = None;
        for tick in 0..600 {
            kart.tick(&go, &world);
            max_alpha = max_alpha.max(kart.lakitu.as_ref().unwrap().alpha());
            if std::mem::take(&mut kart.teleported) {
                teleports += 1;
                after = Some((tick, kart.pos, kart.yaw));
            }
            held += kart.lakitu.as_ref().unwrap().holding() as i32;
        }
        let (_, pos, yaw) = after.expect("rescued");
        assert_eq!(teleports, 1);
        assert!(held > 10 && (max_alpha - 1.).abs() < 1e-6, "fades to black: held {held} alpha {max_alpha}");
        let near = nodes.iter().map(|n| (n[0] - pos[0]).hypot(n[2] - pos[2])).fold(f64::INFINITY, f64::min);
        assert!(near < 30., "put on a path node: {near}");
        assert!((pos[0] - on_track[0]).hypot(pos[2] - on_track[2]) < 1500., "close to where it left the road: {pos:?}");
        assert!(world.find_floor(pos[0], pos[1] + 50., pos[2]).surface.is_some());
        // Facing down the track: the node a few steps ahead is in front of the kart.
        let ahead = nodes[nodes.iter().position(|n| (n[0] - pos[0]).hypot(n[2] - pos[2]) < 30.).unwrap() + 3];
        let want = (ahead[0] - pos[0]).atan2(ahead[2] - pos[2]);
        assert!(((yaw - want + PI).rem_euclid(2. * PI) - PI).abs() < 0.2, "yaw {yaw} want {want}");
        assert!(kart.speed > 0.5, "driving again after the fade, speed {}", kart.speed);
    }

    #[test]
    fn drift_indicator_shows_charge_and_the_mini_turbo() {
        let mut xp = KartExperiment::new();
        assert_eq!(xp.drift_status(), "");
        xp.state.phys.drifting = true;
        assert_eq!(xp.drift_status(), " | DRIFT [--]");
        xp.state.phys.drift_state = 1;
        assert_eq!(xp.drift_status(), " | DRIFT [#-]");
        xp.state.phys.drift_state = 2;
        assert_eq!(xp.drift_status(), " | DRIFT [##] RELEASE");
        xp.state.phys.drifting = false;
        xp.state.phys.mini_turbo = true;
        assert_eq!(xp.drift_status(), " | MINI-TURBO!");
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
