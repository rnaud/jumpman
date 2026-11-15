//! The 3D half of the v3 front end: flat/gouraud-shaded low-poly props rendered by a private camera
//! into a small image (see `mod.rs`): the title backdrop and its spinning gem. The title's hero is the
//! real Mario model from the player's imported data ([`super::hero`]); the character select builds its
//! own stage ([`super::select`]).
use super::fb::Rgba;
use super::model::Page;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use std::f32::consts::{PI, TAU};

pub const LAYER: usize = 7;
pub const FOV: f32 = 52.0 * PI / 180.0;

// ------------------------------------------------------------------------------------------
// Geometry helpers
// ------------------------------------------------------------------------------------------

fn lin(c: Rgba) -> [f32; 4] {
    Color::srgb_u8(c[0], c[1], c[2]).to_linear().to_f32_array()
}

#[derive(Default)]
pub struct MeshBuilder {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl MeshBuilder {
    /// One flat-shaded triangle with a colour per corner (gouraud across the face).
    pub fn tri_cols(&mut self, a: Vec3, b: Vec3, c: Vec3, ca: Rgba, cb: Rgba, cc: Rgba) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let base = self.pos.len() as u32;
        for (p, col) in [(a, ca), (b, cb), (c, cc)] {
            self.pos.push(p.to_array());
            self.nor.push(n.to_array());
            self.col.push(lin(col));
        }
        self.idx.extend([base, base + 1, base + 2]);
    }

    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Rgba) {
        self.tri_cols(a, b, c, col, col, col);
    }

    /// Triangle wound so its front faces away from `behind`.
    pub fn tri_away(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Rgba, behind: Vec3) {
        let n = (b - a).cross(c - a);
        if n.dot((a + b + c) / 3.0 - behind) >= 0.0 {
            self.tri(a, b, c, col);
        } else {
            self.tri(a, c, b, col);
        }
    }

    pub fn quad_cols(&mut self, p: [Vec3; 4], c: [Rgba; 4]) {
        self.tri_cols(p[0], p[1], p[2], c[0], c[1], c[2]);
        self.tri_cols(p[0], p[2], p[3], c[0], c[2], c[3]);
    }

    pub fn quad(&mut self, p: [Vec3; 4], col: Rgba) {
        self.quad_cols(p, [col; 4]);
    }

    /// Quad wound to face away from `behind` (corners in loop order).
    pub fn quad_away(&mut self, p: [Vec3; 4], c: [Rgba; 4], behind: Vec3) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.dot((p[0] + p[1] + p[2] + p[3]) / 4.0 - behind) >= 0.0 {
            self.quad_cols(p, c);
        } else {
            self.quad_cols([p[0], p[3], p[2], p[1]], [c[0], c[3], c[2], c[1]]);
        }
    }

    /// Pyramid / cone with `seg` sides on a base at `base` growing along `axis` (length = height).
    pub fn cone(&mut self, base: Vec3, axis: Vec3, radius: f32, seg: usize, col: Rgba) {
        let up = axis.normalize();
        let e1 = if up.x.abs() < 0.9 { up.cross(Vec3::X) } else { up.cross(Vec3::Z) }.normalize();
        let e2 = up.cross(e1);
        let apex = base + axis;
        let inside = base + axis * 0.3;
        for i in 0..seg {
            let (a0, a1) = (i as f32 / seg as f32 * TAU, (i + 1) as f32 / seg as f32 * TAU);
            let p0 = base + (e1 * a0.cos() + e2 * a0.sin()) * radius;
            let p1 = base + (e1 * a1.cos() + e2 * a1.sin()) * radius;
            self.tri_away(p0, p1, apex, col, inside);
            self.tri_away(p0, p1, base, col, inside);
        }
    }

    pub fn build(self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        m.insert_indices(Indices::U32(self.idx));
        m
    }
}

fn c(hex: u32) -> Rgba {
    super::fb::rgb(hex)
}

fn shade(col: Rgba, f: f32) -> Rgba {
    [(col[0] as f32 * f).min(255.0) as u8, (col[1] as f32 * f).min(255.0) as u8, (col[2] as f32 * f).min(255.0) as u8, 255]
}

fn gem_mesh() -> Mesh {
    let mut b = MeshBuilder::default();
    let (top, bot) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -1.0, 0.0));
    let ring: Vec<Vec3> = (0..6).map(|i| { let a = i as f32 / 6.0 * TAU; Vec3::new(a.cos() * 0.55, 0.0, a.sin() * 0.55) }).collect();
    for i in 0..6 {
        let (p, q) = (ring[i], ring[(i + 1) % 6]);
        let tone = 0.75 + 0.25 * ((i as f32 * 1.7).sin() * 0.5 + 0.5);
        b.tri_away(p, q, top, shade(c(0xffa020), tone), Vec3::ZERO);
        b.tri_away(p, q, bot, shade(c(0xd06a10), tone), Vec3::ZERO);
    }
    b.build()
}

fn gradient_quad(w: f32, h: f32, top: Rgba, bottom: Rgba) -> Mesh {
    let mut b = MeshBuilder::default();
    let (hw, hh) = (w / 2.0, h / 2.0);
    b.quad_cols([Vec3::new(-hw, -hh, 0.0), Vec3::new(hw, -hh, 0.0), Vec3::new(hw, hh, 0.0), Vec3::new(-hw, hh, 0.0)], [bottom, bottom, top, top]);
    b.build()
}

#[derive(Component)]
pub struct TitleRoot;
#[derive(Component)]
pub struct GemProp;
#[derive(Component)]
pub struct RetroCam;

#[derive(Resource)]
pub struct SceneAssets {
    pub lit: Handle<StandardMaterial>,
    pub unlit: Handle<StandardMaterial>,
}

pub fn spawn_scene(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, target: Handle<Image>) {
    let lit = mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, reflectance: 0.0, opaque_render_method: bevy::material::OpaqueRendererMethod::Forward, ..default() });
    let unlit = mats.add(StandardMaterial { base_color: Color::WHITE, unlit: true, cull_mode: None, ..default() });
    let layer = RenderLayers::layer(LAYER);
    commands.insert_resource(SceneAssets { lit: lit.clone(), unlit: unlit.clone() });
    commands.spawn((
        RetroCam,
        Camera3d::default(),
        Camera { order: -5, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(target.into()),
        Msaa::Off,
        Tonemapping::None,
        // Bevy warns each time this camera is created when dithering is left on with no tonemapping.
        bevy::core_pipeline::tonemapping::DebandDither::Disabled,
        AmbientLight { color: Color::WHITE, brightness: 900.0, ..default() },
        Projection::Perspective(PerspectiveProjection { fov: FOV, near: 0.1, far: 200.0, ..default() }),
        Transform::from_xyz(0.0, 1.5, 6.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        layer.clone(),
    ));
    // No directional light: the props are lit by the camera's ambient light alone. A 12000 lux sun used to sit here, but
    // the classic renderer never applied it to this layer, while the deferred pipeline of the ray-traced tier did (blown
    // out floors and hero). Dropping it keeps both renderers on the same, original look.

    // Title (and the grid behind its tiles): gradient backdrop and the gem.
    let back = meshes.add(gradient_quad(90.0, 70.0, c(0x050c3c), c(0x2a63e8)));
    commands.spawn((TitleRoot, Transform::from_xyz(0.0, 1.0, -30.0), Visibility::Hidden, layer.clone())).with_children(|t| {
        t.spawn((Mesh3d(back), MeshMaterial3d(unlit.clone()), Transform::default(), layer.clone()));
    });
    let gem = meshes.add(gem_mesh());
    commands.spawn((GemProp, Mesh3d(gem), MeshMaterial3d(lit.clone()), Transform::from_scale(Vec3::splat(0.9)), Visibility::Hidden, layer.clone()));
}

// ------------------------------------------------------------------------------------------
// Camera
// ------------------------------------------------------------------------------------------

/// The camera the overlay projects with (same maths as the real one).
#[derive(Resource, Clone, Copy, Debug)]
pub struct CamState {
    pub eye: Vec3,
    pub target: Vec3,
    pub aspect: f32,
}

impl Default for CamState {
    fn default() -> Self {
        Self { eye: Vec3::new(0.0, 1.5, 6.0), target: Vec3::new(0.0, 1.0, 0.0), aspect: 4.0 / 3.0 }
    }
}

impl CamState {
    /// World point -> framebuffer px (x, y) and the view depth; `None` behind the camera.
    pub fn project(&self, p: Vec3, w: f32, h: f32) -> Option<(f32, f32, f32)> {
        let view = Mat4::look_at_rh(self.eye, self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(FOV, self.aspect, 0.1, 200.0);
        let v = view * p.extend(1.0);
        if v.z >= -0.05 {
            return None;
        }
        let clip = proj * v;
        let ndc = clip.truncate() / clip.w;
        Some(((ndc.x * 0.5 + 0.5) * w, (0.5 - ndc.y * 0.5) * h, -v.z))
    }
}

pub fn title_camera() -> (Vec3, Vec3) {
    (Vec3::new(0.0, 1.25, 6.4), Vec3::new(0.0, 1.05, 0.0))
}

/// Distance from the title camera to the plane the hero and the gem stand in.
pub const TITLE_DEPTH: f32 = 6.4;

/// Half-width of the view at distance `d` for the given aspect.
pub fn half_view_width(d: f32, aspect: f32) -> f32 {
    d * (FOV / 2.0).tan() * aspect
}

#[allow(clippy::type_complexity)]
pub fn animate_scene(
    time: Res<Time<Real>>,
    menu: Res<super::MenuRes>,
    mut cam_state: ResMut<CamState>,
    active: Res<super::RetroActive>,
    low: Res<super::LowRes>,
    hero: Res<super::hero::TitleHero>,
    mut cam: Query<(&mut Transform, &mut Camera, &mut Projection), With<RetroCam>>,
    mut gem: Query<(&mut Transform, &mut Visibility), (With<GemProp>, Without<RetroCam>)>,
    mut title: Query<&mut Visibility, (With<TitleRoot>, Without<GemProp>)>,
) {
    let Ok((mut cam_tf, mut camera, mut proj)) = cam.single_mut() else { return };
    camera.is_active = active.0;
    if !active.0 {
        return;
    }
    let t = time.elapsed_secs();
    let m = &menu.0;
    let aspect = low.aspect();
    cam_state.aspect = aspect;
    if let Projection::Perspective(p) = &mut *proj {
        p.aspect_ratio = aspect;
    }
    let select = m.page == Page::Select;
    let title_page = m.page == Page::Title;
    for mut v in &mut title {
        *v = if select { Visibility::Hidden } else { Visibility::Inherited };
    }
    let (eye, target) = if select { super::select::camera(low.w, low.h) } else { title_camera() };
    cam_state.eye = eye;
    cam_state.target = target;
    *cam_tf = Transform::from_translation(eye).looking_at(target, Vec3::Y);

    if let Ok((mut tf, mut vis)) = gem.single_mut() {
        *vis = if title_page { Visibility::Inherited } else { Visibility::Hidden };
        // Beside Mario; alone in the middle of the left half when there is no model to show.
        let k = if hero.shown() { 0.12 } else { super::hero::TITLE_X };
        let x = -half_view_width(TITLE_DEPTH, aspect) * k;
        *tf = Transform::from_translation(Vec3::new(x, 1.35 + (t * 1.6).sin() * 0.12, 0.0)).with_rotation(Quat::from_rotation_y(t * 1.1)).with_scale(Vec3::splat(1.15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_props_project_into_the_left_half() {
        for aspect in [4.0 / 3.0, 16.0 / 9.0, 2.2] {
            let (eye, target) = title_camera();
            let cam = CamState { eye, target, aspect };
            let w = 240.0 * aspect;
            let x = -half_view_width(TITLE_DEPTH, aspect) * super::super::hero::TITLE_X;
            let (px, _, _) = cam.project(Vec3::new(x, 1.0, 0.0), w, 240.0).expect("in front");
            assert!(px > 0.0 && px < w / 2.0, "aspect {aspect}: hero at x {px}");
        }
    }

    /// The ray-traced tier renders opaque materials deferred; the private camera has no Solari lighting pass, so every
    /// lit material of the front end must be forward (or the props come out black), the props must stay on their own
    /// layer (out of the world's acceleration structure) and keep their CPU data (`mesh_is_editable`).
    #[test]
    fn private_scene_survives_the_ray_traced_renderer() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>().init_resource::<Assets<StandardMaterial>>();
        app.add_systems(Startup, move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>| {
            spawn_scene(&mut commands, &mut meshes, &mut mats, Handle::default());
        });
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, Option<&RenderLayers>)>();
        let found: Vec<_> = q.iter(world).map(|(m, mat, l)| (m.0.clone(), mat.0.clone(), l.cloned())).collect();
        assert!(found.len() >= 2, "{} meshes", found.len());
        for (mesh, mat, layers) in found {
            assert!(!crate::lighting::in_world_layer(layers.as_ref()), "private props must stay out of the world's RT scene");
            assert!(crate::lighting::mesh_is_editable(world.resource::<Assets<Mesh>>().get(&mesh).unwrap()), "mesh lost its CPU data");
            let m = world.resource::<Assets<StandardMaterial>>().get(&mat).unwrap();
            assert!(m.unlit || m.opaque_render_method == bevy::material::OpaqueRendererMethod::Forward, "a lit menu material would render black under deferred");
        }
        let mut lights = world.query::<&DirectionalLight>();
        assert_eq!(lights.iter(world).count(), 0, "a sun here was applied by the deferred pipeline only: the look must not depend on the renderer");
    }

    #[test]
    fn builders_make_valid_meshes() {
        for m in [gem_mesh(), gradient_quad(2.0, 1.0, c(0), c(0xffffff))] {
            let n = m.count_vertices();
            assert!(n > 0 && n % 3 == 0);
        }
    }
}
