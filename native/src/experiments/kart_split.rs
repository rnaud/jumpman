//! Split-screen VS (two people on one screen): the second camera, the two viewports, the second controller and the
//! per-half HUD. The rules are `KartRace::humans` (`kart_race.rs`); the second person drives racer 1.
//!
//! * Two `Camera3d`s share the window (or the low-resolution target): the main camera keeps its rig and takes one half, a
//!   [`SplitCamera`] posed from [`SplitView`] takes the other. The halves are side by side on a wide window and stacked
//!   on a tall one (a phone held upright), like the original's top / bottom split.
//! * Things that turn to face the viewer (kart sprites, bananas, shells, the tree billboards) exist once per viewer on
//!   render layer 1 (main camera) and 2 (second camera); the track and the item boxes are on layer 0 and seen by both.
//! * bevy_ui lays out against the default UI camera, which would now be a half-window camera: while split a plain 2D
//!   camera over the whole picture takes that role.
//! * The second person plays on the keyboard (IJKL, U drift, O item, P Lakitu) or the second gamepad (left stick, A drift,
//!   B item, Y Lakitu); touch controls stay with the first.
use super::kart_physics::Input as KartInput;
use crate::camera::MainCamera;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraUpdateSystems, ClearColorConfig, RenderTarget, Viewport};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::sync::Mutex;

/// Render layer of what the main camera's viewer sees and the second's does not.
pub const LAYER_P1: usize = 1;
/// And the other way round.
pub const LAYER_P2: usize = 2;
/// Pixels between the two pictures.
const GAP: u32 = 2;

pub fn p1_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, LAYER_P1])
}
pub fn p2_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, LAYER_P2])
}

/// A wide picture splits down the middle, a tall one across.
pub fn side_by_side(size: UVec2) -> bool {
    size.x as f32 >= size.y as f32 * 1.5
}

/// The rectangle the game's picture occupies: the 4:3 letterbox of the classic aspect, the whole window, or the whole
/// low-resolution image (physical pixels: origin, size).
pub fn base_rect(window: UVec2, letterbox: bool, image: Option<UVec2>) -> (UVec2, UVec2) {
    if let Some(img) = image {
        return (UVec2::ZERO, img);
    }
    if !letterbox {
        return (UVec2::ZERO, window);
    }
    let (w, h) = (window.x as f32, window.y as f32);
    let (vw, vh) = if w / h > 4.0 / 3.0 { (h * 4.0 / 3.0, h) } else { (w, w * 3.0 / 4.0) };
    (UVec2::new(((w - vw) / 2.0) as u32, ((h - vh) / 2.0) as u32), UVec2::new(vw as u32, vh as u32))
}

/// The two viewports of a picture: first person's (left or top), second person's.
pub fn halves(origin: UVec2, size: UVec2) -> [(UVec2, UVec2); 2] {
    if side_by_side(size) {
        let w = (size.x.saturating_sub(GAP)) / 2;
        [(origin, UVec2::new(w, size.y)), (origin + UVec2::new(size.x - w, 0), UVec2::new(w, size.y))]
    } else {
        let h = (size.y.saturating_sub(GAP)) / 2;
        [(origin, UVec2::new(size.x, h)), (origin + UVec2::new(0, size.y - h), UVec2::new(size.x, h))]
    }
}

/// The second camera's pose (Bevy units, radians). The resource exists while the race is split.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SplitView {
    pub eye: Vec3,
    pub look: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

/// How the picture is divided this frame (read by the HUD).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SplitLayout {
    pub side_by_side: bool,
}

#[derive(Component)]
pub struct SplitCamera;

/// An entity that faces its viewer exists once per viewer: viewer 0's copy is on layer 0 until the race splits and layer 1
/// after, viewer 1's copy is always on layer 2 (only the second camera shows it).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewerLayer(pub u8);

/// The render layer of a viewer's copy.
pub fn layer_for(viewer: u8, split: bool) -> RenderLayers {
    match (viewer, split) {
        (0, false) => RenderLayers::layer(0),
        (0, true) => RenderLayers::layer(LAYER_P1),
        _ => RenderLayers::layer(LAYER_P2),
    }
}

fn sync_layers(view: Option<Res<SplitView>>, all: Query<(Entity, &ViewerLayer)>, added: Query<(), Added<ViewerLayer>>, mut commands: Commands, mut last: Local<Option<bool>>) {
    let split = view.is_some();
    if *last == Some(split) && added.is_empty() {
        return;
    }
    *last = Some(split);
    for (e, v) in &all {
        commands.entity(e).insert(layer_for(v.0, split));
    }
}
#[derive(Component)]
struct SplitUi;

pub struct KartSplitPlugin;

impl Plugin for KartSplitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SplitLayout>()
            .add_systems(Update, (read_second_pad, update_hud, sync_layers))
            .add_systems(PostUpdate, apply_cameras.before(CameraUpdateSystems));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply_cameras(
    mut commands: Commands,
    view: Option<Res<SplitView>>,
    display: Option<Res<crate::fx::display::DisplaySettings>>,
    state: Option<Res<crate::shell::AppState>>,
    scale: Res<crate::fx::render_scale::RenderScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut layout: ResMut<SplitLayout>,
    mut main: Query<(Entity, &mut Camera, &RenderTarget, &Msaa, Option<&DistanceFog>, Has<IsDefaultUiCamera>), (With<MainCamera>, Without<SplitCamera>)>,
    mut second: Query<(Entity, &mut Camera, &mut Transform, &mut Projection, &mut RenderTarget), (With<SplitCamera>, Without<MainCamera>)>,
    ui: Query<Entity, With<SplitUi>>,
    mut was_split: Local<bool>,
    mut took_ui: Local<bool>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((main_entity, mut main_cam, main_target, msaa, fog, main_is_ui)) = main.single_mut() else { return };
    let playing = state.as_ref().is_none_or(|s| matches!(**s, crate::shell::AppState::Playing | crate::shell::AppState::Paused | crate::shell::AppState::Transition));
    let classic = display.as_ref().is_some_and(|d| d.aspect == crate::fx::display::Aspect::Classic);
    let letterbox = classic && playing;
    // The picture is the camera's own target: the window, or the image of a low-resolution / off-screen run.
    let target_size = main_cam.physical_target_size().unwrap_or_else(|| window.physical_size());
    let (origin, size) = base_rect(target_size, letterbox && matches!(main_target, RenderTarget::Window(_)), scale.image);
    let Some(view) = view else {
        if *was_split {
            *was_split = false;
            // Back to one picture: the viewport the letterbox rule gives, the default layers, no second camera.
            main_cam.viewport = (letterbox && scale.image.is_none()).then(|| Viewport { physical_position: origin, physical_size: size, ..default() });
            commands.entity(main_entity).remove::<RenderLayers>();
            if std::mem::take(&mut *took_ui) {
                commands.entity(main_entity).insert(IsDefaultUiCamera);
            }
            for (e, ..) in &second {
                commands.entity(e).despawn();
            }
            for e in &ui {
                commands.entity(e).despawn();
            }
        }
        return;
    };
    *was_split = true;
    layout.side_by_side = side_by_side(size);
    let [a, b] = halves(origin, size);
    let port = |(position, size): (UVec2, UVec2)| Viewport { physical_position: position, physical_size: size, ..default() };
    main_cam.viewport = Some(port(a));
    commands.entity(main_entity).insert(p1_layers());
    // A half-picture camera must not be what bevy_ui lays out against (an off-screen run makes the main camera the UI's).
    if main_is_ui {
        *took_ui = true;
        commands.entity(main_entity).remove::<IsDefaultUiCamera>();
    }
    let projection = Projection::Perspective(PerspectiveProjection { fov: view.fov, near: view.near, far: view.far, ..default() });
    let transform = Transform::from_translation(view.eye).looking_at(view.look, Vec3::Y);
    match second.single_mut() {
        Ok((e, mut cam, mut tf, mut proj, mut target)) => {
            cam.viewport = Some(port(b));
            *tf = transform;
            *proj = projection;
            if !same_target(&target, main_target) {
                *target = main_target.clone();
            }
            match fog {
                Some(f) => {
                    commands.entity(e).insert(f.clone());
                }
                None => {
                    commands.entity(e).remove::<DistanceFog>();
                }
            }
        }
        Err(_) => {
            let mut e = commands.spawn((
                SplitCamera,
                Camera3d::default(),
                Camera { order: 1, viewport: Some(port(b)), ..default() },
                main_target.clone(),
                *msaa,
                Tonemapping::ReinhardLuminance,
                projection,
                transform,
                p2_layers(),
            ));
            if let Some(f) = fog {
                e.insert(f.clone());
            }
        }
    }
    if (scale.image.is_none() || *took_ui) && ui.is_empty() {
        commands.spawn((
            SplitUi,
            Camera2d,
            main_target.clone(),
            Camera { order: 20, viewport: Some(Viewport { physical_position: origin, physical_size: size, ..default() }), clear_color: ClearColorConfig::None, ..default() },
            RenderLayers::layer(31),
            IsDefaultUiCamera,
        ));
    } else if scale.image.is_some() && !*took_ui {
        for e in &ui {
            commands.entity(e).despawn();
        }
    }
}

fn same_target(a: &RenderTarget, b: &RenderTarget) -> bool {
    match (a, b) {
        (RenderTarget::Image(x), RenderTarget::Image(y)) => x.handle == y.handle,
        (RenderTarget::Window(_), RenderTarget::Window(_)) => true,
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The second controller
// ---------------------------------------------------------------------------------------------------------------

/// The second person's controls, accumulated by [`read_second_pad`] every frame and consumed once per 30 Hz tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SecondPad {
    /// Stick, -1..1 (right and forward positive).
    pub x: f32,
    pub y: f32,
    pub hop_down: bool,
    pub item_down: bool,
    /// Edges since the last `take`.
    pub hop_pressed: bool,
    pub item_pressed: bool,
    pub item_released: bool,
    pub rescue: bool,
}

static SECOND: Mutex<SecondPad> = Mutex::new(SecondPad { x: 0., y: 0., hop_down: false, item_down: false, hop_pressed: false, item_pressed: false, item_released: false, rescue: false });

/// The pad for the next tick; the edges are cleared.
pub fn take_second_pad() -> SecondPad {
    let Ok(mut p) = SECOND.lock() else { return SecondPad::default() };
    let out = *p;
    p.hop_pressed = false;
    p.item_pressed = false;
    p.item_released = false;
    p.rescue = false;
    out
}

/// A kart controller reading from a stick in -1..1: `raw` in N64 stick units (about -80..80), accelerate on stick forward,
/// brake / reverse on stick back (the first person's mapping in `kart.rs`).
pub fn kart_input(p: &SecondPad, hop_pressed: bool) -> KartInput {
    let raw = |v: f32| (v.clamp(-1., 1.) * 80.).round() as i32;
    KartInput { raw_x: raw(p.x), raw_y: raw(p.y), accel: p.y * 64. > 16., brake: p.y * 64. < -16., hop_down: p.hop_down, hop_pressed }
}

/// Keyboard keys of the second person: stick up / down / left / right, drift, item, Lakitu.
pub const KEYS: [KeyCode; 7] = [KeyCode::KeyI, KeyCode::KeyK, KeyCode::KeyJ, KeyCode::KeyL, KeyCode::KeyU, KeyCode::KeyO, KeyCode::KeyP];

/// One device's reading: stick x / y, drift held, item held, Lakitu pressed.
pub type Reading = (f32, f32, bool, bool, bool);

/// Merge a keyboard and a gamepad reading (the larger deflection wins) into the pad state.
pub fn merge(state: &mut SecondPad, keys: Reading, pad: Option<Reading>) {
    let (kx, ky, khop, kitem, krescue) = keys;
    let (px, py, phop, pitem, prescue) = pad.unwrap_or((0., 0., false, false, false));
    let pick = |a: f32, b: f32| if a.abs() >= b.abs() { a } else { b };
    state.x = pick(kx, px);
    state.y = pick(ky, py);
    let hop = khop || phop;
    let item = kitem || pitem;
    state.hop_pressed |= hop && !state.hop_down;
    state.item_pressed |= item && !state.item_down;
    state.item_released |= !item && state.item_down;
    state.hop_down = hop;
    state.item_down = item;
    state.rescue |= krescue || prescue;
}

fn read_second_pad(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, view: Option<Res<SplitView>>) {
    let Ok(mut state) = SECOND.lock() else { return };
    if view.is_none() {
        *state = SecondPad::default();
        return;
    }
    let axis = |neg: KeyCode, pos: KeyCode| f32::from(keys.pressed(pos)) - f32::from(keys.pressed(neg));
    let kb = (axis(KEYS[2], KEYS[3]), axis(KEYS[1], KEYS[0]), keys.pressed(KEYS[4]), keys.pressed(KEYS[5]), keys.just_pressed(KEYS[6]));
    // The first gamepad drives the first person through the shared controller; the second one is this player's.
    let pad = pads.iter().nth(1).map(|g| {
        let s = g.left_stick();
        (s.x, s.y, g.pressed(GamepadButton::South), g.pressed(GamepadButton::East) || g.pressed(GamepadButton::West), g.just_pressed(GamepadButton::North))
    });
    merge(&mut state, kb, pad);
}

// ---------------------------------------------------------------------------------------------------------------
// The HUD of each half
// ---------------------------------------------------------------------------------------------------------------

/// What one half of the screen says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Panel {
    /// "1st", "2nd".
    pub place: String,
    /// "LAP 2/3".
    pub lap: String,
    /// The held item, or what the roulette shows.
    pub item: String,
    pub clock: String,
}

/// Both halves and the banner in the middle (the countdown, GO!, FINISH).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitHud {
    pub panels: [Panel; 2],
    pub banner: String,
}

/// "FINISH" for a person who has crossed the line, else their lap.
pub fn lap_text(lap: u32, laps: u32, finished: bool) -> String {
    if finished {
        "FINISH".into()
    } else {
        format!("LAP {}/{}", lap.min(laps), laps)
    }
}

#[derive(Component)]
struct SplitHudRoot;
#[derive(Component)]
struct HudField(usize, u8);
#[derive(Component)]
struct HudBanner;

/// Where a field of viewer `viewer`'s panel sits, in percent of the picture: place top left, lap top right, item bottom
/// left, clock bottom right.
fn field_pos(viewer: usize, side: bool, field: u8) -> (f32, f32) {
    let (left, top, width, height) = if side { (viewer as f32 * 50., 0., 50., 100.) } else { (0., viewer as f32 * 50., 100., 50.) };
    let (fx, fy) = match field {
        0 => (0.02, 0.02),
        1 => (0.72, 0.02),
        2 => (0.02, 0.90),
        _ => (0.72, 0.90),
    };
    (left + width * fx, top + height * fy)
}

fn field_node(viewer: usize, side: bool, field: u8) -> Node {
    let (x, y) = field_pos(viewer, side, field);
    Node { position_type: PositionType::Absolute, left: Val::Percent(x), top: Val::Percent(y), ..default() }
}

fn update_hud(
    mut commands: Commands,
    hud: Option<Res<SplitHud>>,
    layout: Res<SplitLayout>,
    root: Query<Entity, With<SplitHudRoot>>,
    mut fields: Query<(&HudField, &mut Text, &mut Node), Without<HudBanner>>,
    mut banner: Query<(&mut Text, &mut Visibility), (With<HudBanner>, Without<HudField>)>,
) {
    let Some(hud) = hud else {
        for e in &root {
            commands.entity(e).despawn();
        }
        return;
    };
    if root.is_empty() {
        let text = |size: f32| (Text::new(""), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(Color::srgb(1., 0.97, 0.7)));
        commands.spawn((SplitHudRoot, Node { position_type: PositionType::Absolute, left: Val::Px(0.), top: Val::Px(0.), width: Val::Percent(100.), height: Val::Percent(100.), ..default() })).with_children(|c| {
            for viewer in 0..2 {
                for field in 0..4u8 {
                    let size = if field == 0 { 34. } else { 18. };
                    c.spawn((HudField(viewer, field), text(size), field_node(viewer, layout.side_by_side, field)));
                }
            }
            c.spawn((HudBanner, text(72.), Node { position_type: PositionType::Absolute, left: Val::Percent(46.), top: Val::Percent(40.), ..default() }, Visibility::Hidden));
        });
        return;
    }
    for (f, mut t, mut node) in &mut fields {
        let p = &hud.panels[f.0.min(1)];
        let want = match f.1 {
            0 => &p.place,
            1 => &p.lap,
            2 => &p.item,
            _ => &p.clock,
        };
        if t.0 != *want {
            t.0 = want.clone();
        }
        let placed = field_node(f.0, layout.side_by_side, f.1);
        if node.left != placed.left || node.top != placed.top {
            node.left = placed.left;
            node.top = placed.top;
        }
    }
    for (mut t, mut v) in &mut banner {
        if t.0 != hud.banner {
            t.0 = hud.banner.clone();
        }
        *v = if hud.banner.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_picture_splits_down_the_middle_and_a_tall_one_across() {
        let [a, b] = halves(UVec2::ZERO, UVec2::new(1600, 900));
        assert_eq!((a.0, a.1), (UVec2::new(0, 0), UVec2::new(799, 900)));
        assert_eq!((b.0, b.1), (UVec2::new(801, 0), UVec2::new(799, 900)));
        let [a, b] = halves(UVec2::ZERO, UVec2::new(900, 1600));
        assert_eq!((a.0, a.1), (UVec2::new(0, 0), UVec2::new(900, 799)));
        assert_eq!((b.0, b.1), (UVec2::new(0, 801), UVec2::new(900, 799)));
        // 4:3 (the classic letterbox) is not wide enough to go side by side.
        assert!(!side_by_side(UVec2::new(1200, 900)) && side_by_side(UVec2::new(1350, 900)));
    }

    #[test]
    fn halves_keep_their_origin_and_never_overlap() {
        let (origin, size) = base_rect(UVec2::new(1920, 1080), true, None);
        assert_eq!((origin, size), (UVec2::new(240, 0), UVec2::new(1440, 1080)));
        let [a, b] = halves(origin, size);
        assert!(a.0.y + a.1.y <= b.0.y, "stacked halves do not overlap: {a:?} {b:?}");
        assert!(a.0.x >= origin.x && b.0.x + b.1.x <= origin.x + size.x);
        let [a, b] = halves(UVec2::new(10, 20), UVec2::new(1000, 400));
        assert!(a.0.x + a.1.x <= b.0.x && b.0.x + b.1.x == 1010 && a.0.y == 20);
    }

    #[test]
    fn the_picture_is_the_image_when_the_game_renders_small() {
        assert_eq!(base_rect(UVec2::new(1920, 1080), true, Some(UVec2::new(640, 480))), (UVec2::ZERO, UVec2::new(640, 480)));
        assert_eq!(base_rect(UVec2::new(800, 600), false, None), (UVec2::ZERO, UVec2::new(800, 600)));
        // A tall window letterboxes to a 4:3 band.
        assert_eq!(base_rect(UVec2::new(600, 1200), true, None), (UVec2::new(0, 375), UVec2::new(600, 450)));
    }

    #[test]
    fn the_second_pad_accelerates_forward_brakes_back_and_scales_to_stick_units() {
        let fwd = kart_input(&SecondPad { y: 1., ..default() }, false);
        assert!(fwd.accel && !fwd.brake && fwd.raw_y == 80);
        let back = kart_input(&SecondPad { y: -1., x: 0.5, ..default() }, false);
        assert!(back.brake && !back.accel && back.raw_x == 40);
        let idle = kart_input(&SecondPad { y: 0.2, ..default() }, true);
        assert!(!idle.accel && !idle.brake && idle.hop_pressed);
    }

    #[test]
    fn keyboard_and_gamepad_merge_with_edges_that_wait_to_be_taken() {
        let mut p = SecondPad::default();
        merge(&mut p, (0., 1., false, false, false), Some((0.4, 0.3, false, false, false)));
        assert_eq!((p.x, p.y), (0.4, 1.));
        // Pressing the item key sets one press edge; holding it adds none; letting go sets the release edge.
        merge(&mut p, (0., 0., false, true, false), None);
        merge(&mut p, (0., 0., false, true, false), None);
        assert!(p.item_pressed && p.item_down);
        p.item_pressed = false;
        merge(&mut p, (0., 0., false, true, false), None);
        assert!(!p.item_pressed, "held, not pressed again");
        merge(&mut p, (0., 0., false, false, false), None);
        assert!(p.item_released && !p.item_down);
        merge(&mut p, (0., 0., true, false, false), None);
        assert!(p.hop_pressed && p.hop_down);
    }

    #[test]
    fn the_lap_line_says_finish_once_a_person_is_done() {
        assert_eq!(lap_text(2, 3, false), "LAP 2/3");
        assert_eq!(lap_text(3, 3, true), "FINISH");
        assert_eq!(lap_text(9, 3, false), "LAP 3/3");
    }

    #[test]
    fn a_viewers_copies_are_on_their_own_layers_once_the_race_splits() {
        assert_eq!(layer_for(0, false), RenderLayers::layer(0));
        assert_eq!(layer_for(0, true), RenderLayers::layer(LAYER_P1));
        assert_eq!(layer_for(1, true), RenderLayers::layer(LAYER_P2));
        assert_eq!(layer_for(1, false), RenderLayers::layer(LAYER_P2), "the second camera does not exist, nobody sees it");
        // The main camera shows layers 0 and 1, the second 0 and 2: the track is shared, the sprites are not.
        assert!(p1_layers().intersects(&layer_for(0, true)) && !p2_layers().intersects(&layer_for(0, true)));
        assert!(p2_layers().intersects(&layer_for(1, true)) && !p1_layers().intersects(&layer_for(1, true)));
        assert!(p1_layers().intersects(&RenderLayers::layer(0)) && p2_layers().intersects(&RenderLayers::layer(0)));
    }

    #[test]
    fn each_panel_keeps_to_its_own_half() {
        for side in [true, false] {
            for field in 0..4u8 {
                let (x0, y0) = field_pos(0, side, field);
                let (x1, y1) = field_pos(1, side, field);
                if side {
                    assert!(x0 < 50. && x1 >= 50. && y0 == y1);
                } else {
                    assert!(y0 < 50. && y1 >= 50. && x0 == x1);
                }
            }
        }
    }
}
