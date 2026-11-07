//! Zelda UI: the OoT textbox (`ootTextbox.ts` `draw`), the context prompt, the rupee wallet and
//! the slingshot reticle. The box is composited on the CPU into a 256x64 RGBA image exactly like
//! the web canvas (glyph atlas tinted per colour code, 1 px black shadow, mirrored background
//! half) and shown through `bevy_ui`.
use super::save::ZeldaSave;
use super::textbox::{Icon, Layout, COLORS, WOOD_COLORS};
use super::world::ZeldaWorld;
use crate::camera::MainCamera;
use crate::progress::CourseScoped;
use crate::render::AssetRoot;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

#[derive(Clone)]
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Rgba {
    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let x = x.clamp(0, self.w as i64 - 1) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        let i = (y * self.w + x) * 4;
        [self.data[i] as f32, self.data[i + 1] as f32, self.data[i + 2] as f32, self.data[i + 3] as f32 / 255.0]
    }

    /// Bilinear sample at continuous source coordinates (texel centres at +0.5).
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let (x, y) = (u - 0.5, v - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let mut out = [0.0; 4];
        let mut a_sum = 0.0;
        for (dx, dy, w) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
            let t = self.texel(x0 + dx, y0 + dy);
            for k in 0..3 {
                out[k] += t[k] * t[3] * w;
            }
            a_sum += t[3] * w;
        }
        if a_sum > 0.0 {
            for k in 0..3 {
                out[k] /= a_sum;
            }
        }
        out[3] = a_sum;
        out
    }
}

pub fn parse_color(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).unwrap_or(255);
    if h.len() == 3 {
        [v(0, 1) * 17, v(1, 1) * 17, v(2, 1) * 17]
    } else {
        [v(0, 2), v(2, 2), v(4, 2)]
    }
}

/// How a tinted source is combined: text/black art keep the tint (`source-in`), the wood plate
/// multiplies it with the texture.
#[derive(Clone, Copy, PartialEq)]
pub enum Tint {
    SourceIn([u8; 3]),
    Multiply([u8; 3]),
}

/// Draw `src` (region `sx,sy,sw,sh`) into `dst` at `dx,dy` scaled to `dw x dh`, mirrored if asked.
#[allow(clippy::too_many_arguments)]
pub fn blit(dst: &mut Rgba, src: &Rgba, region: (usize, usize, usize, usize), at: (f32, f32, f32, f32), tint: Tint, alpha: f32, mirror: bool) {
    let (sx, sy, sw, sh) = region;
    let (dx, dy, dw, dh) = at;
    let (x0, x1) = (dx.floor().max(0.0) as i64, (dx + dw).ceil().min(dst.w as f32) as i64);
    let (y0, y1) = (dy.floor().max(0.0) as i64, (dy + dh).ceil().min(dst.h as f32) as i64);
    for py in y0..y1 {
        for px in x0..x1 {
            let fx = (px as f32 + 0.5 - dx) / dw;
            let fy = (py as f32 + 0.5 - dy) / dh;
            if !(0.0..1.0).contains(&fx) || !(0.0..1.0).contains(&fy) {
                continue;
            }
            let fx = if mirror { 1.0 - fx } else { fx };
            let s = src.sample(sx as f32 + fx * sw as f32, sy as f32 + fy * sh as f32);
            let a = s[3] * alpha;
            if a <= 0.0 {
                continue;
            }
            let rgb = match tint {
                Tint::SourceIn(c) => [c[0] as f32, c[1] as f32, c[2] as f32],
                Tint::Multiply(c) => [s[0] * c[0] as f32 / 255.0, s[1] * c[1] as f32 / 255.0, s[2] * c[2] as f32 / 255.0],
            };
            let i = (py as usize * dst.w + px as usize) * 4;
            let da = dst.data[i + 3] as f32 / 255.0;
            let oa = a + da * (1.0 - a);
            for k in 0..3 {
                let d = dst.data[i + k] as f32;
                dst.data[i + k] = ((rgb[k] * a + d * da * (1.0 - a)) / oa).round().clamp(0.0, 255.0) as u8;
            }
            dst.data[i + 3] = (oa * 255.0).round() as u8;
        }
    }
}

/// The dialogue art (`assets/oot-dialogue`).
pub struct DialogueArt {
    pub glyphs: Rgba,
    pub black: Rgba,
    pub wood: Rgba,
    pub arrow: Rgba,
    pub next: Rgba,
    pub end: Rgba,
    pub widths: Vec<f64>,
}

fn decode(path: &std::path::Path) -> Option<Rgba> {
    let bytes = std::fs::read(path).ok()?;
    let img = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::nearest(), RenderAssetUsages::default()).ok()?;
    let rgba = img.try_into_dynamic().ok()?.to_rgba8();
    Some(Rgba { w: rgba.width() as usize, h: rgba.height() as usize, data: rgba.into_raw() })
}

impl DialogueArt {
    pub fn load(root: &std::path::Path) -> Option<Self> {
        let dir = root.join("oot-dialogue");
        Some(Self {
            glyphs: decode(&dir.join("glyphs.png"))?,
            black: decode(&dir.join("black.png"))?,
            wood: decode(&dir.join("wood.png"))?,
            arrow: decode(&dir.join("arrow.png"))?,
            next: decode(&dir.join("next.png"))?,
            end: decode(&dir.join("end.png"))?,
            widths: super::textbox::load_widths(root).ok()?,
        })
    }

    /// `draw`'s canvas part: render a layout into a 256x64 image.
    pub fn compose(&self, layout: &Layout) -> Rgba {
        let mut out = Rgba { w: 256, h: 64, data: vec![0; 256 * 64 * 4] };
        let (bg, tint, alpha) = if layout.wood { (&self.wood, Tint::Multiply(parse_color("#654429")), 230.0 / 255.0) } else { (&self.black, Tint::SourceIn([0, 0, 0]), 170.0 / 255.0) };
        blit(&mut out, bg, (0, 0, 128, 64), (0.0, 0.0, 128.0, 64.0), tint, alpha, false);
        blit(&mut out, bg, (0, 0, 128, 64), (128.0, 0.0, 128.0, 64.0), tint, alpha, true);
        let colors = if layout.wood { &WOOD_COLORS } else { &COLORS };
        for g in &layout.glyphs {
            let (sx, sy) = (g.index % 16 * 16, g.index / 16 * 16);
            let (x, y) = (g.x as f32, g.y as f32);
            blit(&mut out, &self.glyphs, (sx, sy, 16, 16), (x + 1.0, y + 1.0, 12.0, 12.0), Tint::SourceIn([0, 0, 0]), 1.0, false);
            blit(&mut out, &self.glyphs, (sx, sy, 16, 16), (x, y, 12.0, 12.0), Tint::SourceIn(parse_color(colors[g.color & 7])), 1.0, false);
        }
        if let Some(icon) = layout.icon {
            let (img, x, y) = match icon {
                Icon::Arrow { x, y } => (&self.arrow, x, y),
                Icon::Next { x, y } => (&self.next, x, y),
                Icon::End { x, y } => (&self.end, x, y),
            };
            blit(&mut out, img, (0, 0, 16, 16), (x as f32, y as f32, 12.0, 12.0), Tint::SourceIn(parse_color("#65afff")), 1.0, false);
        }
        out
    }
}

#[derive(Resource)]
pub struct ZeldaUi {
    art: Option<DialogueArt>,
    image: Handle<Image>,
}

#[derive(Component)]
pub struct TextboxNode;
#[derive(Component)]
pub struct PromptText;
#[derive(Component)]
pub struct WalletText;
#[derive(Component)]
pub struct Reticle;

pub fn spawn_ui(mut commands: Commands, root: Res<AssetRoot>, mut images: ResMut<Assets<Image>>, world: Option<Res<ZeldaWorld>>) {
    if world.is_none() {
        return;
    }
    let mut image = Image::new_fill(Extent3d { width: 256, height: 64, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.sampler = ImageSampler::linear();
    let handle = images.add(image);
    let art = DialogueArt::load(&root.0);
    if art.is_none() {
        warn!("oot-dialogue art missing; the textbox is disabled");
    }
    commands.insert_resource(ZeldaUi { art, image: handle.clone() });
    commands.spawn((CourseScoped, TextboxNode, ImageNode::new(handle), Node { position_type: PositionType::Absolute, width: Val::Px(256.0), height: Val::Px(64.0), ..default() }, Visibility::Hidden, ZIndex(20)));
    commands.spawn((
        CourseScoped,
        PromptText,
        Text::new(""),
        TextFont { font_size: 20.0, ..default() },
        TextColor(Color::WHITE),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, bottom: Val::Percent(4.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
        TextLayout::new_with_justify(JustifyText::Center),
    ));
    commands.spawn((
        CourseScoped,
        WalletText,
        Text::new(""),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::srgb(0.7, 1.0, 0.7)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, top: Val::Px(150.0), left: Val::Px(32.0), ..default() },
    ));
    commands.spawn((
        CourseScoped,
        Reticle,
        Text::new("+"),
        TextFont { font_size: 40.0, ..default() },
        TextColor(Color::srgb(1.0, 0.9, 0.4)),
        Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(50.0), margin: UiRect { left: Val::Px(-12.0), top: Val::Px(-24.0), ..default() }, ..default() },
        Visibility::Hidden,
    ));
}

#[allow(clippy::too_many_arguments)]
pub fn update_ui(
    time: Res<Time<Real>>,
    ui: Option<Res<ZeldaUi>>,
    world: Option<ResMut<ZeldaWorld>>,
    save: Res<ZeldaSave>,
    sim: Res<crate::game::runner::GameSim>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut boxes: Query<(&mut Node, &mut Visibility), (With<TextboxNode>, Without<Reticle>)>,
    mut prompt: Query<&mut Text, (With<PromptText>, Without<WalletText>)>,
    mut wallet: Query<&mut Text, (With<WalletText>, Without<PromptText>)>,
    mut reticle: Query<&mut Visibility, (With<Reticle>, Without<TextboxNode>)>,
) {
    let (Some(ui), Some(mut world)) = (ui, world) else { return };
    let now = time.elapsed_secs_f64() * 1000.0;
    if let Ok(mut t) = prompt.single_mut() {
        let text = if world.visit.textbox.shown() { String::new() } else { world.prompt.clone().unwrap_or_default() };
        if t.0 != text {
            t.0 = text;
        }
    }
    if let Ok(mut t) = wallet.single_mut() {
        let text = format!("{} rupees{}{}", save.rupees, if save.sword { " - Kokiri Sword" } else { "" }, if save.shield { " - Deku Shield" } else { "" });
        if t.0 != text {
            t.0 = text;
        }
    }
    if let Ok(mut v) = reticle.single_mut() {
        *v = if world.aiming { Visibility::Inherited } else { Visibility::Hidden };
    }
    let Ok((mut node, mut vis)) = boxes.single_mut() else { return };
    let (Some(art), true) = (&ui.art, world.visit.textbox.shown()) else {
        *vis = Visibility::Hidden;
        return;
    };
    // Where the speaker's head is on screen decides top/bottom placement for unpositioned pages.
    let head = [sim.mario.pos[0] as f32 * 0.01, sim.mario.pos[1] as f32 * 0.01 + 1.1, sim.mario.pos[2] as f32 * 0.01];
    let actor_y = camera.0.world_to_ndc(camera.1, Vec3::from(head)).map_or(0.0, |ndc| (1.0 - ndc.y) as f64 / 2.0);
    let layout = world.visit.textbox.layout(&art.widths, actor_y, now);
    let img = art.compose(&layout);
    if let Some(target) = images.get_mut(&ui.image) {
        target.data = Some(img.data);
    }
    let (w, h) = (window.width(), window.height());
    let scale = (w / 320.0).min(h / 240.0);
    let grow = layout.grow as f32;
    let (cx, cy) = (w / 2.0 + 2.0 * scale, h / 2.0 + (layout.top as f32 - 120.0) * scale + 32.0 * scale);
    node.width = Val::Px(256.0 * scale * grow);
    node.height = Val::Px(64.0 * scale * grow);
    node.left = Val::Px(cx - 128.0 * scale * grow);
    node.top = Val::Px(cy - 32.0 * scale * grow);
    *vis = Visibility::Inherited;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zelda::textbox::{MessagePage, OotTextbox};

    #[test]
    fn composes_text_onto_a_dark_plate() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let art = DialogueArt::load(&root).expect("dialogue art");
        let mut tb = OotTextbox::default();
        tb.show("Navi", MessagePage { text: "Hello, \u{5}Alink!".into(), style: 0, branches: None }, false, true, 0.0);
        let layout = tb.layout(&art.widths, 0.0, 1000.0);
        let img = art.compose(&layout);
        let opaque = img.data.chunks(4).filter(|p| p[3] > 100).count();
        let bright = img.data.chunks(4).filter(|p| p[3] > 100 && p[0] > 200 && p[1] > 200).count();
        assert!(opaque > 256 * 64 / 2, "plate drawn, {opaque}");
        assert!(bright > 20, "white glyph pixels, {bright}");
        // Wood style uses the brown plate.
        tb.show("Deku", MessagePage { text: "Hi".into(), style: 0x10, branches: None }, false, true, 0.0);
        let wood = art.compose(&tb.layout(&art.widths, 0.0, 1000.0));
        let px = &wood.data[(32 * 256 + 4) * 4..][..4];
        assert!(px[0] > px[2], "brown plate {px:?}");
    }
}
