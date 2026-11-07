//! 240p / 480p render targets (`displaySettings.ts` resolution + scaling). When the resolution is
//! not `Native` the main 3D camera renders into a small image and a 2D camera stretches it over the
//! (letterboxed) window with nearest ("sharp") or bilinear ("soft") sampling. bevy_ui rides on the
//! 2D camera, so menus and the HUD stay crisp.
use super::display::{display_size, Aspect, DisplaySettings, Resolution, Scaling};
use crate::camera::MainCamera;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::camera::{ClearColorConfig, RenderTarget, ScalingMode, Viewport};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::RenderLayers;
use bevy::window::PrimaryWindow;

/// Where the low-res image lands in the window (logical px) when scaling is active.
#[derive(Resource, Default, Clone, Copy)]
pub struct RenderScale {
    pub image: Option<UVec2>,
    pub origin: Vec2,
    pub size: Vec2,
}

impl RenderScale {
    /// Window-space cursor -> coordinates in the 3D camera's render target.
    pub fn to_target(&self, p: Vec2) -> Vec2 {
        match self.image {
            Some(img) if self.size.x > 0.0 && self.size.y > 0.0 => (p - self.origin) / self.size * img.as_vec2(),
            _ => p,
        }
    }
    /// Camera target coordinates -> window space.
    pub fn to_window(&self, p: Vec2) -> Vec2 {
        match self.image {
            Some(img) if img.x > 0 && img.y > 0 => p / img.as_vec2() * self.size + self.origin,
            _ => p,
        }
    }
}

#[derive(Component)]
struct Upscaler;
#[derive(Component)]
struct UpscaleSprite;

#[derive(Resource, Default)]
struct Target(Option<Handle<Image>>);

pub struct RenderScalePlugin;

impl Plugin for RenderScalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderScale>().init_resource::<Target>().add_systems(Update, apply);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    settings: Res<DisplaySettings>,
    state: Option<Res<crate::shell::AppState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<&mut Camera, (With<MainCamera>, Without<Upscaler>)>,
    mut up_cams: Query<(Entity, &mut Camera), With<Upscaler>>,
    mut sprites: Query<&mut Sprite, With<UpscaleSprite>>,
    sprite_entities: Query<Entity, With<UpscaleSprite>>,
    mut images: ResMut<Assets<Image>>,
    mut target: ResMut<Target>,
    mut scale: ResMut<RenderScale>,
    mut last_scaling: Local<Option<Scaling>>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok(mut cam) = cams.single_mut() else { return };
    if settings.resolution == Resolution::Native {
        if target.0.take().is_some() {
            cam.target = RenderTarget::default();
            for (e, _) in &up_cams {
                commands.entity(e).despawn();
            }
            for e in &sprite_entities {
                commands.entity(e).despawn();
            }
            *scale = RenderScale::default();
        }
        return;
    }
    let playing = state.map_or(true, |s| matches!(*s, crate::shell::AppState::Playing | crate::shell::AppState::Paused | crate::shell::AppState::Transition));
    let (w, h) = (window.width(), window.height());
    let letterbox = settings.aspect == Aspect::Classic && playing;
    let aspect_settings = DisplaySettings { aspect: if letterbox { Aspect::Classic } else { Aspect::Wide }, ..*settings };
    let d = display_size(&aspect_settings, w, h, 1.0);
    let (vw, vh) = if letterbox { (d.css_width, d.css_height) } else { (w, h) };
    let origin = Vec2::new((w - vw) / 2.0, (h - vh) / 2.0);
    let sf = window.scale_factor();
    let img_size = UVec2::new(d.width.max(1), d.height.max(1));
    let filter = if settings.scaling == Scaling::Sharp { ImageSampler::nearest() } else { ImageSampler::Descriptor(ImageSamplerDescriptor::linear()) };

    let handle = target
        .0
        .get_or_insert_with(|| {
            let mut img = Image::new_fill(Extent3d { width: img_size.x, height: img_size.y, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, default());
            img.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
            img.sampler = filter.clone();
            images.add(img)
        })
        .clone();
    if let Some(img) = images.get_mut(&handle) {
        if img.texture_descriptor.size.width != img_size.x || img.texture_descriptor.size.height != img_size.y {
            img.resize(Extent3d { width: img_size.x, height: img_size.y, depth_or_array_layers: 1 });
        }
        if *last_scaling != Some(settings.scaling) {
            *last_scaling = Some(settings.scaling);
            img.sampler = filter;
        }
    }
    if !matches!(&cam.target, RenderTarget::Image(t) if t.handle == handle) {
        cam.target = RenderTarget::Image(handle.clone().into());
    }
    if cam.viewport.is_some() {
        cam.viewport = None;
    }
    *scale = RenderScale { image: Some(img_size), origin, size: Vec2::new(vw, vh) };
    let viewport = Viewport { physical_position: UVec2::new((origin.x * sf) as u32, (origin.y * sf) as u32), physical_size: UVec2::new(((vw * sf) as u32).max(1), ((vh * sf) as u32).max(1)), ..default() };
    if let Ok((_, mut up)) = up_cams.single_mut() {
        if up.viewport.as_ref().map(|v| (v.physical_position, v.physical_size)) != Some((viewport.physical_position, viewport.physical_size)) {
            up.viewport = Some(viewport);
        }
        for mut s in &mut sprites {
            if s.image != handle {
                s.image = handle.clone();
            }
        }
    } else {
        commands.spawn((
            Upscaler,
            Camera2d,
            Camera { order: 10, viewport: Some(viewport), clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: 1.0, height: 1.0 }, ..OrthographicProjection::default_2d() }),
            RenderLayers::layer(1),
            IsDefaultUiCamera,
        ));
        commands.spawn((UpscaleSprite, Sprite { image: handle, custom_size: Some(Vec2::ONE), ..default() }, RenderLayers::layer(1)));
    }
}
