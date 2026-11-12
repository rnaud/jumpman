//! Which meshes are in the ray-traced scene, and as what.
//!
//! Solari only builds an acceleration structure for a mesh whose vertex layout is exactly position, normal, uv0,
//! tangent with u32 indices (`is_mesh_raytracing_compatible`). Almost every mesh of the game carries vertex colours
//! (N64 vertex shading), so before this module the TLAS was nearly empty: nothing cast a traced shadow, nothing was
//! ambient-occluded and nothing bounced light. The raster meshes are left alone (their colours matter to the G-buffer);
//! each one gets a *proxy* mesh with the exact layout, shared by every entity that uses the source mesh, and the entity
//! is registered with `RaytracingMesh3d(proxy)`. `RaytracingMesh3d` may point at a different asset than `Mesh3d`.
//!
//! Meshes that change every frame (the CPU-posed Link / Banjo / BK enemy rigs: their vertices are rewritten each tick)
//! have their proxy rebuilt when the source is modified, which makes Solari rebuild that BLAS: characters therefore cast
//! correct traced shadows in whatever pose they are in.
//!
//! Membership follows visibility: a hidden mesh (an unused castle room, a pooled sprite) leaves the acceleration
//! structure and comes back when it is shown, so it neither shadows nor bounces light into the visible scene.
use crate::lighting::{in_world_layer, mesh_is_editable};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use std::collections::{HashMap, HashSet};

/// Opts a mesh with an alpha-mask material into the ray-traced scene as a solid (characters and enemies: their masked
/// materials only cut out hair or eye texels, they are not billboards).
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct RtSolid;

/// The entity was looked at and must stay out of the ray-traced scene (until its mesh or material is replaced).
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct RtSkip;

/// Where a mesh goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtRoute {
    /// In the acceleration structure: casts traced shadows and bounces light.
    Traced,
    /// Drawn but not traced, for the stated reason.
    Left(&'static str),
}

/// The routing rule, kept free of ECS so it can be tested.
pub fn route(layers: Option<&RenderLayers>, mat: &StandardMaterial, solid: bool, skinned: bool) -> RtRoute {
    // Private scenes (the front end's props, the character stage) live on their own render layer and are drawn by their
    // own camera: they must not enter the world's acceleration structure (they would shadow and bounce light into the
    // level at the origin).
    if !in_world_layer(layers) {
        return RtRoute::Left("private render layer");
    }
    if skinned {
        return RtRoute::Left("GPU-skinned mesh");
    }
    if crate::lighting::rt_excluded(mat, solid) {
        return RtRoute::Left("cut-out, blended or emissive material");
    }
    RtRoute::Traced
}

/// Proxy meshes by source mesh.
#[derive(Resource, Default)]
pub struct RtProxies {
    map: HashMap<AssetId<Mesh>, Handle<Mesh>>,
    /// Sources that cannot be proxied (no CPU data, not triangles): never retried.
    failed: HashSet<AssetId<Mesh>>,
}

impl RtProxies {
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Dynamic sources above this many vertices are not rebuilt every time they change (a deforming level mesh).
const MAX_DYNAMIC_VERTICES: usize = 40_000;

/// Vertex normals turned to agree with the triangle winding (missing normals become the area-weighted face normal).
pub fn oriented_normals(pos: &[[f32; 3]], idx: &[u32], source: Option<&[[f32; 3]]>) -> Vec<Vec3> {
    let n = pos.len();
    let mut normals: Vec<Vec3> = match source {
        Some(v) => v.iter().map(|&n| Vec3::from(n)).collect(),
        None => vec![Vec3::ZERO; n],
    };
    let mut score = vec![0.0f32; n];
    let mut face_sum = vec![Vec3::ZERO; n];
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (Vec3::from(pos[t[0] as usize]), Vec3::from(pos[t[1] as usize]), Vec3::from(pos[t[2] as usize]));
        let f = (b - a).cross(c - a);
        for &i in t {
            score[i as usize] += normals[i as usize].dot(f);
            face_sum[i as usize] += f;
        }
    }
    for i in 0..n {
        normals[i] = if source.is_none() || normals[i].length_squared() < 1e-12 {
            face_sum[i].normalize_or_zero()
        } else if score[i] < 0.0 {
            -normals[i]
        } else {
            normals[i]
        };
        if normals[i] == Vec3::ZERO {
            normals[i] = Vec3::Y;
        }
    }
    normals
}

/// A shading normal may lean at most this far from its triangle's geometric normal before the ray tracer stops trusting it.
const MAX_NORMAL_DEVIATION_COS: f32 = 0.866; // 30 degrees

/// Ray tracing traces against the real triangles, so a shading normal that disagrees with its face makes the rays leave
/// the surface through the wall: the sun ray of a sun-facing normal on a wall facing away runs straight into that wall,
/// and the whole polygon renders black. N64 level meshes carry authored vertex normals that lean far off their (often
/// vertical) faces, which the rasteriser's ambient hid. Corners whose normal leans more than 30 degrees off the face
/// get the face normal (unsharing vertices when the corners of a shared vertex disagree); gently smooth shading is kept.
/// Returns whether the mesh was changed.
pub fn conform_normals(m: &mut Mesh) -> bool {
    if m.primitive_topology() != PrimitiveTopology::TriangleList || !mesh_is_editable(m) {
        return false;
    }
    let Some(pos) = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).map(|p| p.to_vec()) else { return false };
    let idx: Vec<u32> = match m.indices() {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => (0..pos.len() as u32).collect(),
    };
    if idx.iter().any(|&i| i as usize >= pos.len()) {
        return false;
    }
    let Some(old) = m.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).filter(|v| v.len() == pos.len()).map(|p| p.to_vec()) else { return false };
    // Per corner: the normal to use.
    let mut corners: Vec<[f32; 3]> = Vec::with_capacity(idx.len());
    let mut changed = false;
    for t in idx.chunks(3) {
        let face = if t.len() == 3 {
            let (a, b, c) = (Vec3::from(pos[t[0] as usize]), Vec3::from(pos[t[1] as usize]), Vec3::from(pos[t[2] as usize]));
            (b - a).cross(c - a).normalize_or_zero()
        } else {
            Vec3::ZERO
        };
        for &i in t {
            let vn = Vec3::from(old[i as usize]);
            if face != Vec3::ZERO && vn.normalize_or_zero().dot(face) < MAX_NORMAL_DEVIATION_COS {
                corners.push(face.to_array());
                changed = true;
            } else {
                corners.push(old[i as usize]);
            }
        }
    }
    if !changed {
        return false;
    }
    m.duplicate_vertices();
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, corners);
    true
}

/// The Solari-compatible twin of `src`: position, normal, uv0, tangent, u32 indices, no CPU copy kept. Normals are made to
/// agree with the winding (the exporters and the CPU rigs do not always orient them), so the traced hit shading faces the
/// right way. `None` when the mesh cannot be read or is not a triangle list.
pub fn build_proxy(src: &Mesh) -> Option<Mesh> {
    if src.primitive_topology() != PrimitiveTopology::TriangleList || !mesh_is_editable(src) {
        return None;
    }
    let pos = src.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
    let n = pos.len();
    let mut idx: Vec<u32> = match src.indices() {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => (0..n as u32).collect(),
    };
    idx.truncate(idx.len() / 3 * 3);
    if idx.is_empty() || idx.iter().any(|&i| i as usize >= n) {
        return None;
    }
    let source_normals = src.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).filter(|v| v.len() == n);
    let normals = oriented_normals(pos, &idx, source_normals);
    let uv: Vec<[f32; 2]> = match src.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(v)) if v.len() == n => v.clone(),
        _ => vec![[0.0, 0.0]; n],
    };
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.to_vec());
    out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.iter().map(|v| v.to_array()).collect::<Vec<_>>());
    out.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; n]);
    out.insert_indices(Indices::U32(idx));
    Some(out)
}

/// Gives every visible, traceable mesh entity a `RaytracingMesh3d` pointing at its proxy; takes it away from hidden ones.
#[allow(clippy::type_complexity)]
pub fn register_rt_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut proxies: ResMut<RtProxies>,
    materials: Res<Assets<StandardMaterial>>,
    todo: Query<(Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>, Option<&RenderLayers>, &InheritedVisibility, Has<RtSolid>, Has<bevy::mesh::skinning::SkinnedMesh>), (Without<RaytracingMesh3d>, Without<RtSkip>)>,
    swapped: Query<Entity, (Or<(Changed<Mesh3d>, Changed<MeshMaterial3d<StandardMaterial>>)>, Or<(With<RaytracingMesh3d>, With<RtSkip>)>)>,
    leaving: Query<(Entity, &InheritedVisibility), (With<RaytracingMesh3d>, Changed<InheritedVisibility>)>,
) {
    for e in &swapped {
        commands.entity(e).try_remove::<(RaytracingMesh3d, RtSkip)>();
    }
    for (e, vis) in &leaving {
        if !vis.get() {
            commands.entity(e).try_remove::<RaytracingMesh3d>();
        }
    }
    for (e, mesh, mat, layers, vis, solid, skinned) in &todo {
        if !vis.get() {
            continue;
        }
        let Some(material) = materials.get(&mat.0) else { continue };
        if let RtRoute::Left(_) = route(layers, material, solid, skinned) {
            commands.entity(e).try_insert(RtSkip);
            continue;
        }
        let id = mesh.0.id();
        if proxies.failed.contains(&id) {
            commands.entity(e).try_insert(RtSkip);
            continue;
        }
        let proxy = match proxies.map.get(&id) {
            Some(h) => h.clone(),
            None => {
                let Some(mut m) = meshes.get_mut(id) else { continue };
                if std::env::var_os("JUMPMAN_RT_KEEPN").is_none() {
                    conform_normals(&mut m);
                }
                drop(m);
                let Some(src) = meshes.get(id) else { continue };
                match build_proxy(src) {
                    Some(p) => {
                        let h = meshes.add(p);
                        proxies.map.insert(id, h.clone());
                        h
                    }
                    None => {
                        proxies.failed.insert(id);
                        commands.entity(e).try_insert(RtSkip);
                        continue;
                    }
                }
            }
        };
        commands.entity(e).try_insert(RaytracingMesh3d(proxy));
    }
}

/// Keeps proxies in step with their source: a source that is rewritten (a posed character) rebuilds its proxy, a source
/// that is dropped frees it.
pub fn sync_proxies(mut events: MessageReader<AssetEvent<Mesh>>, mut meshes: ResMut<Assets<Mesh>>, mut proxies: ResMut<RtProxies>) {
    let mut changed = HashSet::new();
    for ev in events.read() {
        match ev {
            AssetEvent::Modified { id } if proxies.map.contains_key(id) => {
                changed.insert(*id);
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                proxies.map.remove(id);
                proxies.failed.remove(id);
            }
            _ => {}
        }
    }
    for id in changed {
        let Some(proxy) = proxies.map.get(&id).map(|h| h.id()) else { continue };
        let Some(src) = meshes.get(id) else { continue };
        if src.count_vertices() > MAX_DYNAMIC_VERTICES {
            continue;
        }
        if let Some(new) = build_proxy(src) {
            let _ = meshes.insert(proxy, new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::PrimitiveTopology;

    fn quad(colors: bool, flipped_normals: bool) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, -1.0], [0.0, 0.0, -1.0]]);
        let n = if flipped_normals { [0.0f32, -1.0, 0.0] } else { [0.0f32, 1.0, 0.0] };
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![n; 4]);
        if colors {
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 4]);
        }
        m.insert_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]));
        m
    }

    #[test]
    fn proxy_has_exactly_the_layout_solari_needs() {
        let p = build_proxy(&quad(true, false)).expect("proxy");
        let names: Vec<_> = p.attributes().map(|(a, _)| a.id).collect();
        assert_eq!(names, vec![Mesh::ATTRIBUTE_POSITION.id, Mesh::ATTRIBUTE_NORMAL.id, Mesh::ATTRIBUTE_UV_0.id, Mesh::ATTRIBUTE_TANGENT.id], "vertex colours are dropped, nothing else added");
        assert!(matches!(p.indices(), Some(Indices::U32(v)) if v.len() == 6));
        assert!(p.enable_raytracing);
        assert!(p.asset_usage == RenderAssetUsages::RENDER_WORLD, "no CPU copy of the proxy is kept");
    }

    #[test]
    fn proxy_keeps_the_geometry_bounds() {
        let src = quad(false, false);
        let p = build_proxy(&src).unwrap();
        let a = src.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec();
        let b = p.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec();
        assert_eq!(a, b);
    }

    #[test]
    fn normals_are_turned_to_face_the_winding() {
        // The winding (0,0,0)-(1,0,0)-(1,0,-1) has an upward face normal; a downward vertex normal is flipped.
        let p = build_proxy(&quad(false, true)).unwrap();
        let n = p.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().as_float3().unwrap();
        assert!(n.iter().all(|n| n[1] > 0.99), "{n:?}");
        let ok = build_proxy(&quad(false, false)).unwrap();
        assert!(ok.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().as_float3().unwrap().iter().all(|n| n[1] > 0.99));
    }

    #[test]
    fn non_indexed_meshes_and_missing_normals_are_completed() {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, -1.0]]);
        let p = build_proxy(&m).unwrap();
        assert!(matches!(p.indices(), Some(Indices::U32(v)) if v == &vec![0, 1, 2]));
        assert!(p.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap().as_float3().unwrap()[0][1] > 0.99);
    }

    #[test]
    fn unreadable_or_non_triangle_meshes_get_no_proxy() {
        assert!(build_proxy(&Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())).is_none());
        let mut gpu_only = quad(false, false);
        gpu_only.asset_usage = RenderAssetUsages::RENDER_WORLD;
        assert!(build_proxy(&gpu_only).is_none());
        let mut bad = quad(false, false);
        bad.insert_indices(Indices::U32(vec![0, 1, 9]));
        assert!(build_proxy(&bad).is_none(), "an index past the vertices would crash the BLAS build");
    }

    #[test]
    fn routing_rules() {
        let opaque = StandardMaterial::default();
        assert_eq!(route(None, &opaque, false, false), RtRoute::Traced);
        let mask = StandardMaterial { alpha_mode: AlphaMode::Mask(0.5), ..default() };
        assert!(matches!(route(None, &mask, false, false), RtRoute::Left(_)), "billboards stay out");
        assert_eq!(route(None, &mask, true, false), RtRoute::Traced, "a masked character is solid");
        let blend = StandardMaterial { alpha_mode: AlphaMode::Blend, ..default() };
        assert!(matches!(route(None, &blend, true, false), RtRoute::Left(_)), "blended sprites and water do not occlude");
        let glow = StandardMaterial { emissive: LinearRgba::rgb(0.5, 0.4, 0.0), ..default() };
        assert!(matches!(route(None, &glow, false, false), RtRoute::Left(_)), "an emissive instance would be a light that out-votes the sun");
        assert!(matches!(route(Some(&RenderLayers::layer(7)), &opaque, false, false), RtRoute::Left(_)));
        assert!(matches!(route(None, &opaque, false, true), RtRoute::Left(_)));
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
        app.init_asset::<Mesh>().init_asset::<StandardMaterial>().init_resource::<RtProxies>();
        app.add_systems(Update, (register_rt_meshes, sync_proxies).chain());
        app
    }

    fn spawn(app: &mut App, mesh: Mesh, mat: StandardMaterial, vis: bool, extra: impl Bundle) -> Entity {
        let m = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
        let ma = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(mat);
        app.world_mut().spawn((Mesh3d(m), MeshMaterial3d(ma), InheritedVisibility::from(vis), extra)).id()
    }

    #[test]
    fn every_visible_mesh_is_traced_or_deliberately_left_out() {
        let mut app = app();
        let solid = spawn(&mut app, quad(true, false), StandardMaterial::default(), true, ());
        let twin = spawn(&mut app, quad(true, false), StandardMaterial::default(), true, ());
        let masked_char = spawn(&mut app, quad(true, false), StandardMaterial { alpha_mode: AlphaMode::Mask(0.3), ..default() }, true, RtSolid);
        let billboard = spawn(&mut app, quad(false, false), StandardMaterial { alpha_mode: AlphaMode::Mask(0.5), ..default() }, true, ());
        let coin = spawn(&mut app, quad(false, false), StandardMaterial { emissive: LinearRgba::rgb(0.5, 0.4, 0.1), ..default() }, true, ());
        let menu = spawn(&mut app, quad(false, false), StandardMaterial::default(), true, RenderLayers::layer(7));
        let hidden_room = spawn(&mut app, quad(false, false), StandardMaterial::default(), false, ());
        app.update();
        app.update();
        let w = app.world();
        for e in [solid, twin, masked_char] {
            assert!(w.entity(e).contains::<RaytracingMesh3d>());
        }
        for e in [billboard, coin, menu] {
            assert!(!w.entity(e).contains::<RaytracingMesh3d>() && w.entity(e).contains::<RtSkip>(), "left out on purpose, and remembered");
        }
        assert!(!w.entity(hidden_room).contains::<RaytracingMesh3d>() && !w.entity(hidden_room).contains::<RtSkip>(), "hidden: pending, not traced");
        // Entities that share a source mesh handle share one proxy.
        let h = |e: Entity| w.entity(e).get::<RaytracingMesh3d>().unwrap().0.id();
        assert_ne!(h(solid), h(twin), "different source meshes, different proxies");
        assert_eq!(app.world().resource::<RtProxies>().len(), 3);
    }

    #[test]
    fn hidden_meshes_leave_the_scene_and_return() {
        let mut app = app();
        let e = spawn(&mut app, quad(true, false), StandardMaterial::default(), true, ());
        app.update();
        app.update();
        assert!(app.world().entity(e).contains::<RaytracingMesh3d>());
        let proxy_count = app.world().resource::<RtProxies>().len();
        app.world_mut().entity_mut(e).insert(InheritedVisibility::HIDDEN);
        app.update();
        app.update();
        assert!(!app.world().entity(e).contains::<RaytracingMesh3d>(), "a hidden room must not shadow the visible one");
        app.world_mut().entity_mut(e).insert(InheritedVisibility::VISIBLE);
        app.update();
        app.update();
        assert!(app.world().entity(e).contains::<RaytracingMesh3d>());
        assert_eq!(app.world().resource::<RtProxies>().len(), proxy_count, "the proxy is reused");
    }

    #[test]
    fn a_posed_source_mesh_rebuilds_its_proxy() {
        let mut app = app();
        let e = spawn(&mut app, quad(true, false), StandardMaterial::default(), true, ());
        app.update();
        app.update();
        let src = app.world().entity(e).get::<Mesh3d>().unwrap().0.clone();
        let proxy = app.world().entity(e).get::<RaytracingMesh3d>().unwrap().0.clone();
        {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            let mut m = meshes.get_mut(&src).unwrap();
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 5.0, 0.0], [1.0, 5.0, 0.0], [1.0, 5.0, -1.0], [0.0, 5.0, -1.0]]);
        }
        app.update();
        app.update();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let y = meshes.get(&proxy).unwrap().attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap()[0][1];
        assert_eq!(y, 5.0, "the traced shadow follows the pose");
    }
}
