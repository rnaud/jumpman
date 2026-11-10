//! The v3 front end's look as `bevy_ui` widgets, for the screens that are not drawn into the title's framebuffer: the pause
//! menu and its pages (status, settings, controls, how to play, save file).
//!
//! Everything here is rendered with the title's own pieces: [`Fb`]'s bitmap font, slabs and dialog frames, drawn at the
//! framebuffer's resolution and scaled up by an integer `k` (so one framebuffer pixel is `k` layout pixels, like the
//! title's stretched 320x240 buffer), plus the same palette, the gold chevron cursor, the wobbling extruded logo letters and
//! the scanline strip. Layout, focus, scrolling and activation stay with the shell's `bevy_ui` machinery: a [`RetroButton`]
//! is an ordinary `Button` + [`UiAction`] whose image changes with focus, selection and disabling.
use super::fb::{clean, mix, rgb, Fb, IRect, Rgba};
use crate::button_marker::Button;
use crate::shell::nav::Focus;
use crate::shell::theme::{Disabled, MenuCursor, Selected};
use crate::shell::{AppState, UiAction};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::widget::NodeImageMode;

/// Height of a button in framebuffer pixels (the title's rows are 19).
pub const ROW_H: i32 = 18;
pub const GOLD: Rgba = rgb(0xffd23a);
pub const WHITE: Rgba = rgb(0xf6f6ee);
pub const SKY: Rgba = rgb(0xaec6ff);
pub const PALE: Rgba = rgb(0xdfe6ff);
const SHADOW: Rgba = rgb(0x050a22);

/// Marks the root of a screen built with the skin: the star cursor gives way to the chevron while one is up.
#[derive(Component)]
pub struct RetroScreen;

/// Look of a button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Blue,
    Red,
    Green,
}

/// State of a button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Hot,
    Chosen,
    Off,
}

const STATES: [State; 4] = [State::Idle, State::Hot, State::Chosen, State::Off];

/// (top, bottom, light edge, dark edge, outline, ink)
fn palette(kind: Kind, state: State) -> [Rgba; 6] {
    match (state, kind) {
        (State::Off, _) => [rgb(0x6a6a78), rgb(0x3a3a46), rgb(0x9a9aa8), rgb(0x20202a), rgb(0x0a0a10), rgb(0xc0c0cc)],
        (State::Chosen, _) => [rgb(0xffd23a), rgb(0xe08a18), rgb(0xfff1a8), rgb(0x8a4a08), rgb(0x3a1a04), rgb(0x3a1a04)],
        (State::Hot, Kind::Blue) => [rgb(0x4a7dff), rgb(0x1f48d0), rgb(0xb8ccff), rgb(0x0f2a8a), rgb(0x040720), WHITE],
        (State::Idle, Kind::Blue) => [rgb(0x33449a), rgb(0x1a2468), rgb(0x5b70c8), rgb(0x0a1040), rgb(0x040720), PALE],
        (State::Idle, Kind::Red) => [rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603), WHITE],
        (State::Hot, Kind::Red) => [rgb(0xf06a52), rgb(0xb02a1c), rgb(0xffc8bc), rgb(0x700e08), rgb(0x2a0603), WHITE],
        (State::Idle, Kind::Green) => [rgb(0x4ad060), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a), WHITE],
        (State::Hot, Kind::Green) => [rgb(0x8aff8a), rgb(0x2cb046), rgb(0xe8ffe8), rgb(0x0a4a18), rgb(0x03220a), WHITE],
    }
}

/// A button face of `w` x `h` framebuffer pixels with its label centred.
pub fn draw_button(w: i32, h: i32, kind: Kind, state: State, text: &str, scale: i32) -> Fb {
    let [top, bot, light, dark, outline, ink] = palette(kind, state);
    let mut f = Fb::new(w, h);
    f.slab(IRect::new(0, 0, w, h), top, bot, light, dark, outline);
    let text = clean(text);
    let scale = if Fb::text_width(&text, scale) > w - 6 { 1 } else { scale };
    let ty = (h - 7 * scale) / 2;
    let tx = (w - Fb::text_width(&text, scale)) / 2;
    if matches!(state, State::Chosen) {
        f.glyphs(tx, ty, &text, scale, &|_| ink);
    } else {
        f.text(tx, ty, &text, scale, ink);
    }
    f
}

/// The dialog frame of the title's hint bar, for nine-slicing.
fn draw_dialog_tile(n: i32) -> Fb {
    let mut f = Fb::new(n, n);
    f.slab(IRect::new(0, 0, n, n), rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
    f
}

/// One letter of the logo: extruded side, then the outlined gradient face (as the title's `draw_logo`).
pub fn draw_logo_letter(ch: char, scale: i32) -> Fb {
    let step = 6 * scale + scale;
    let pad = scale + 2;
    let mut f = Fb::new(step + pad, 7 * scale + pad * 2);
    let s = ch.to_string();
    for d in (1..=scale + 1).rev() {
        f.glyphs(pad + d, pad + d, &s, scale, &|_| rgb(0x4a1d05));
    }
    f.text_outlined(pad, pad, &s, scale, rgb(0x5a2608), &|row| mix(rgb(0xffe56a), rgb(0xf08a18), row as f32 / 6.0));
    f
}

pub fn draw_chevron() -> Fb {
    let mut f = Fb::new(14, 14);
    let pts = [(1.0, 1.0), (10.0, 7.0), (1.0, 13.0), (4.0, 7.0)];
    let shadow: Vec<_> = pts.iter().map(|p| (p.0 + 1.0, p.1 + 1.0)).collect();
    f.poly(&shadow, SHADOW);
    f.poly(&pts, GOLD);
    f
}

/// Word-wrap `s` to lines of at most `max` characters.
pub fn wrap(s: &str, max: usize) -> Vec<String> {
    super::wrap_text(s, max.max(4))
}

/// Builds the widgets of one screen: owns the image uploads and the integer scale.
pub struct Skin<'a> {
    pub k: u32,
    images: &'a mut Assets<Image>,
    panel: Option<Handle<Image>>,
}

impl<'a> Skin<'a> {
    pub fn new(k: u32, images: &'a mut Assets<Image>) -> Self {
        Self { k, images, panel: None }
    }

    /// Layout px of `n` framebuffer px.
    pub fn px(&self, n: i32) -> Val {
        Val::Px((n * self.k as i32) as f32)
    }

    /// Upload a framebuffer scaled up by `k` (nearest), sampled bilinearly like the title.
    pub fn upload(&mut self, fb: &Fb) -> Handle<Image> {
        upload(self.images, fb, self.k)
    }

    fn image_node(&mut self, fb: &Fb) -> (ImageNode, Node) {
        let h = self.upload(fb);
        (ImageNode::new(h), Node { width: self.px(fb.w), height: self.px(fb.h), flex_shrink: 0.0, ..default() })
    }

    /// A line of text with the title's drop shadow. Returns its size in framebuffer px.
    pub fn text(&mut self, p: &mut ChildSpawnerCommands, s: &str, scale: i32, color: Rgba) -> IVec2 {
        let s = clean(s);
        let mut f = Fb::new((Fb::text_width(&s, scale) + scale).max(1), 8 * scale);
        f.text(0, 0, &s, scale, color);
        let size = IVec2::new(f.w, f.h);
        let bundle = self.image_node(&f);
        p.spawn(bundle);
        size
    }

    /// Wrapped paragraph in a column no wider than `max_w` framebuffer px.
    pub fn paragraph(&mut self, p: &mut ChildSpawnerCommands, s: &str, scale: i32, color: Rgba, max_w: i32) {
        let max = ((max_w + scale) / (6 * scale)).max(4) as usize;
        let lines = wrap(&clean(s), max);
        p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: self.px(2), ..default() }).with_children(|c| {
            for l in &lines {
                self.text(c, l, scale, color);
            }
        });
    }

    /// A button of `w` framebuffer px; the caller adds nothing else: it already carries `Button` and the action.
    pub fn button<'b>(&mut self, p: &'b mut ChildSpawnerCommands, kind: Kind, w: i32, text: &str, scale: i32, action: UiAction) -> EntityCommands<'b> {
        let states = STATES.map(|st| {
            let f = draw_button(w, ROW_H, kind, st, text, scale);
            self.upload(&f)
        });
        let node = Node { width: self.px(w), height: self.px(ROW_H), flex_shrink: 0.0, ..default() };
        let mut e = p.spawn((Button, action, node, ImageNode::new(states[0].clone()), RetroButton { states }));
        e.insert(Interaction::None);
        e
    }

    /// Width a fitted button needs for `text`.
    pub fn fit(text: &str, scale: i32) -> i32 {
        Fb::text_width(&clean(text), scale) + 14
    }

    /// The dialog frame as a nine-sliced panel; children go inside `node`'s padding.
    pub fn panel<'b>(&mut self, p: &'b mut ChildSpawnerCommands, node: Node) -> EntityCommands<'b> {
        let handle = match &self.panel {
            Some(h) => h.clone(),
            None => {
                let h = self.upload(&draw_dialog_tile(12));
                self.panel = Some(h.clone());
                h
            }
        };
        let b = (3 * self.k) as f32;
        let mut node = node;
        node.flex_direction = FlexDirection::Column;
        node.padding = UiRect::all(self.px(6));
        p.spawn((
            node,
            ImageNode {
                image: handle,
                image_mode: NodeImageMode::Sliced(TextureSlicer { border: BorderRect::all(b), center_scale_mode: SliceScaleMode::Stretch, sides_scale_mode: SliceScaleMode::Stretch, max_corner_scale: 1.0 }),
                ..default()
            },
        ))
    }

    /// A gold heading with a thin rule beneath it.
    pub fn heading(&mut self, p: &mut ChildSpawnerCommands, s: &str) {
        let w = Fb::text_width(&clean(s), 1) + 1;
        p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: self.px(1), margin: UiRect::top(self.px(3)), ..default() }).with_children(|c| {
            self.text(c, s, 1, GOLD);
            c.spawn((Node { width: self.px(w), height: Val::Px(self.k as f32), ..default() }, BackgroundColor(Color::srgba(0.435, 0.525, 0.878, 0.8))));
        });
    }

    /// The big wobbling title (the title screen's logo letters).
    pub fn logo(&mut self, p: &mut ChildSpawnerCommands, text: &str, scale: i32) {
        p.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() }).with_children(|row| {
            for (i, ch) in text.chars().enumerate() {
                let f = draw_logo_letter(ch, scale);
                let mut bundle = self.image_node(&f);
                // Letters overlap their padding a little, like the title's tight spacing.
                bundle.1.margin = UiRect::horizontal(self.px(-1));
                row.spawn((bundle.0, bundle.1, LogoLetter(i), UiTransform::IDENTITY));
            }
        });
    }

    /// Scanlines over the whole screen (the title's overlay strip); `root` is the screen root.
    pub fn scanlines(&mut self, p: &mut ChildSpawnerCommands) {
        let mut img = Image::new(Extent3d { width: 1, height: 2, depth_or_array_layers: 1 }, TextureDimension::D2, vec![0, 0, 8, 0, 0, 0, 8, 46], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        img.sampler = ImageSampler::nearest();
        let h = self.images.add(img);
        p.spawn((
            ImageNode { image: h, image_mode: NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: self.k as f32 / 2.0 }, ..default() },
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
            bevy::ui::FocusPolicy::Pass,
        ));
    }

    /// A key name in a small navy plate.
    pub fn key_plate(&mut self, p: &mut ChildSpawnerCommands, s: &str, w: i32) {
        let s = clean(s);
        let mut f = Fb::new(w, 13);
        f.slab(IRect::new(0, 0, w, 13), rgb(0x1a2468), rgb(0x0b1240), rgb(0x3a4eb0), rgb(0x050a22), rgb(0x040720));
        let scale = if Fb::text_width(&s, 1) > w - 4 { 1 } else { 1 };
        f.text_centered(w / 2, 3, &s, scale, GOLD);
        let bundle = self.image_node(&f);
        p.spawn(bundle);
    }
}

/// Upload `fb` scaled up by `k`.
pub fn upload(images: &mut Assets<Image>, fb: &Fb, k: u32) -> Handle<Image> {
    let (w, h) = (fb.w as u32 * k, fb.h as u32 * k);
    let mut data = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let sy = (y / k) as usize;
        for x in 0..w {
            let sx = (x / k) as usize;
            let s = (sy * fb.w as usize + sx) * 4;
            let d = ((y * w + x) * 4) as usize;
            data[d..d + 4].copy_from_slice(&fb.data[s..s + 4]);
        }
    }
    let mut img = Image::new(Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
    img.sampler = ImageSampler::linear();
    images.add(img)
}

/// The four faces of a button (indexed like [`STATES`]).
#[derive(Component)]
pub struct RetroButton {
    pub states: [Handle<Image>; 4],
}

#[derive(Component)]
pub struct LogoLetter(pub usize);

/// Which face a button wears.
pub fn face_for(focused: bool, hovered: bool, selected: bool, disabled: bool) -> usize {
    if disabled {
        3
    } else if focused || hovered {
        1
    } else if selected {
        2
    } else {
        0
    }
}

pub fn style_buttons(focus: Res<Focus>, mut buttons: Query<(Entity, &RetroButton, &Interaction, &mut ImageNode, Has<Selected>, Has<Disabled>)>) {
    for (e, b, i, mut img, selected, disabled) in &mut buttons {
        let want = &b.states[face_for(focus.0 == Some(e), matches!(i, Interaction::Hovered | Interaction::Pressed), selected, disabled)];
        if img.image != *want {
            img.image = want.clone();
        }
    }
}

/// Bob the logo letters like the title's.
pub fn animate_logo(time: Res<Time<Real>>, mut letters: Query<(&LogoLetter, &mut UiTransform)>) {
    let t = time.elapsed_secs();
    for (l, mut xf) in &mut letters {
        xf.translation = Val2::px(0.0, ((t * 2.4 - l.0 as f32 * 0.55).sin() * 2.0).round());
    }
}

/// The chevron that points at the focused button (the title's cursor) while a skinned screen is up.
#[derive(Component)]
pub struct RetroCursor;

const CHEVRON_PX: f32 = 28.0;

pub fn spawn_cursor(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let h = upload(&mut images, &draw_chevron(), 2);
    commands.spawn((
        RetroCursor,
        crate::shell::screens::ShellRoot,
        ImageNode::new(h),
        Node { position_type: PositionType::Absolute, width: Val::Px(CHEVRON_PX), height: Val::Px(CHEVRON_PX), ..default() },
        GlobalZIndex(61),
        Visibility::Hidden,
        bevy::ui::FocusPolicy::Pass,
    ));
}

#[allow(clippy::type_complexity)]
pub fn place_cursor(
    focus: Res<Focus>,
    state: Res<AppState>,
    time: Res<Time<Real>>,
    screens: Query<(), With<RetroScreen>>,
    targets: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility), (With<UiAction>, Without<RetroCursor>, Without<MenuCursor>)>,
    mut cursor: Query<(&mut Node, &mut Visibility), With<RetroCursor>>,
    mut star: Query<&mut Visibility, (With<MenuCursor>, Without<RetroCursor>)>,
) {
    let Ok((mut node, mut vis)) = cursor.single_mut() else { return };
    let up = !screens.is_empty();
    if up {
        for mut v in &mut star {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
        }
    }
    let target = focus.0.filter(|_| up && state.in_menu()).and_then(|e| targets.get(e).ok()).filter(|(c, _, v)| v.get() && c.size().x > 0.0);
    let Some((computed, gt, _)) = target else {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        return;
    };
    let inv = computed.inverse_scale_factor();
    let size = computed.size() * inv;
    let centre = gt.translation * inv;
    let bob = (time.elapsed_secs() * 7.0).sin().abs() * 5.0;
    let left = (centre.x - size.x * 0.5 - CHEVRON_PX * 0.8 - bob).max(0.0);
    let top = centre.y - CHEVRON_PX * 0.5;
    let (l, t) = (Val::Px(left), Val::Px(top));
    if node.left != l || node.top != t {
        node.left = l;
        node.top = t;
    }
    if *vis != Visibility::Inherited {
        *vis = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_have_four_distinct_faces_and_keep_their_label_inside() {
        let mut seen = Vec::new();
        for st in STATES {
            let f = draw_button(100, ROW_H, Kind::Blue, st, "SETTINGS", 2);
            assert_eq!((f.w, f.h), (100, ROW_H));
            assert!(f.data.chunks(4).all(|p| p[3] == 255), "a slab is opaque");
            seen.push(f.data);
        }
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(seen[i], seen[j], "faces {i} and {j} differ");
            }
        }
        // A label wider than the button drops to the small font instead of spilling.
        let f = draw_button(40, ROW_H, Kind::Blue, State::Idle, "VERY LONG LABEL", 2);
        assert_eq!(f.w, 40);
    }

    #[test]
    fn the_face_follows_focus_hover_selection_and_disabling_in_that_order() {
        assert_eq!(face_for(false, false, false, false), 0);
        assert_eq!(face_for(true, false, false, false), 1);
        assert_eq!(face_for(false, true, true, false), 1, "the pointer wins over selection");
        assert_eq!(face_for(false, false, true, false), 2);
        assert_eq!(face_for(true, true, true, true), 3, "disabled beats everything");
    }

    #[test]
    fn uploads_scale_pixels_by_the_integer_factor() {
        let mut images = Assets::<Image>::default();
        let mut f = Fb::new(2, 1);
        f.px(1, 0, rgb(0xff0000));
        let h = upload(&mut images, &f, 3);
        let img = images.get(&h).unwrap();
        assert_eq!((img.width(), img.height()), (6, 3));
        let d = img.data.as_ref().unwrap();
        assert_eq!(&d[(3 * 4)..(3 * 4 + 4)], &[255, 0, 0, 255]);
        assert_eq!(&d[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn the_logo_letter_leaves_room_for_the_extrusion() {
        let f = draw_logo_letter('P', 3);
        assert!(f.w >= 6 * 3 + 3 && f.h >= 7 * 3 + 4);
        assert!(f.data.chunks(4).any(|p| p[3] > 0));
    }
}
