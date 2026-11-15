//! Headless viewer for the Super Smash Bros. fighters the BYOR import writes (`public/models/ssb64/*.glb`, see
//! native/SSB64.md): loads one fighter on a floor, plays the named clips one after another and saves off-screen stills.
//!
//!   SSB64_GLB=<data>/public/models/ssb64/mario.glb SSB64_OUT=/tmp/shots SSB64_CLIPS=Wait,Run,Jab1 \
//!     cargo run --release --example ssb64_view
//!
//! Each clip is shown for `SSB64_CLIP_SECONDS` (default 1.2) and captured at 0 %, 33 % and 66 % of that window as
//! `<out>/<fighter>_<clip>_<n>.png`; `SSB64_YAW` turns the camera (degrees, default 30). Nothing is written into the repo.
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::path::PathBuf;

#[derive(Resource)]
struct View {
    glb: String,
    out: PathBuf,
    stem: String,
    clips: Vec<String>,
    names: Vec<String>,
    seconds: f32,
    image: Handle<Image>,
    graph: Option<(Handle<AnimationGraph>, Vec<AnimationNodeIndex>)>,
    player: Option<Entity>,
    started: Option<f32>,
    shots: usize,
}

/// Animation names in glTF order, straight from the .glb JSON chunk.
fn animation_names(path: &std::path::Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else { return vec![] };
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return vec![];
    }
    let len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes[20..20 + len]) else { return vec![] };
    json["animations"].as_array().map(|a| a.iter().map(|x| x["name"].as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()
}

fn main() {
    let glb = PathBuf::from(std::env::var("SSB64_GLB").expect("SSB64_GLB=<path to a fighter .glb>"));
    let out = PathBuf::from(std::env::var("SSB64_OUT").unwrap_or_else(|_| "/tmp/ssb64-shots".into()));
    std::fs::create_dir_all(&out).unwrap();
    let names = animation_names(&glb);
    let clips: Vec<String> = match std::env::var("SSB64_CLIPS") {
        Ok(list) => list.split(',').map(str::to_string).filter(|c| names.contains(c)).collect(),
        Err(_) => names.clone(),
    };
    let dir = glb.parent().unwrap().to_path_buf();
    let file = glb.file_name().unwrap().to_string_lossy().into_owned();
    let (w, h) = (640_u32, 640_u32);
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin { file_path: dir.to_string_lossy().into_owned(), ..default() })
            .set(WindowPlugin { primary_window: Some(Window { visible: false, resolution: (w, h).into(), ..default() }), ..default() }),
    );
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    let mut img = Image::new_fill(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, default());
    img.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(img);
    let stem = file.trim_end_matches(".glb").to_string();
    let seconds = std::env::var("SSB64_CLIP_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(1.2);
    app.insert_resource(View { glb: file, out, stem, clips, names, seconds, image, graph: None, player: None, started: None, shots: 0 })
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .insert_resource(ClearColor(Color::srgb(0.55, 0.7, 0.9)))
        .add_systems(Startup, setup)
        .add_systems(Update, (bind_player, step))
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, mut view: ResMut<View>, mut graphs: ResMut<Assets<AnimationGraph>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let handles: Vec<Handle<AnimationClip>> = (0..view.names.len()).map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(view.glb.clone()))).collect();
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    view.graph = Some((graphs.add(graph), nodes));
    commands.spawn((WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(view.glb.clone()))), Transform::default()));
    commands.spawn((Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 8.0))), MeshMaterial3d(materials.add(Color::srgb(0.35, 0.55, 0.3)))));
    let yaw = std::env::var("SSB64_YAW").ok().and_then(|s| s.parse::<f32>().ok()).unwrap_or(30.0).to_radians();
    let dist = std::env::var("SSB64_DIST").ok().and_then(|s| s.parse::<f32>().ok()).unwrap_or(4.2);
    let eye = Vec3::new(yaw.sin() * dist, 1.5, yaw.cos() * dist);
    commands.spawn((Camera3d::default(), bevy::camera::RenderTarget::Image(view.image.clone().into()), Transform::from_translation(eye).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y)));
    commands.spawn((DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
}

fn bind_player(mut commands: Commands, mut view: ResMut<View>, players: Query<Entity, Added<AnimationPlayer>>) {
    for e in &players {
        if let Some((graph, _)) = &view.graph {
            commands.entity(e).insert(AnimationGraphHandle(graph.clone()));
            view.player = Some(e);
        }
    }
}

fn step(mut commands: Commands, mut view: ResMut<View>, time: Res<Time>, mut players: Query<&mut AnimationPlayer>, mut exit: MessageWriter<AppExit>) {
    let now = time.elapsed_secs();
    let Some(entity) = view.player else {
        if now > 20.0 {
            eprintln!("no AnimationPlayer appeared");
            exit.write(AppExit::error());
        }
        return;
    };
    let Ok(mut player) = players.get_mut(entity) else { return };
    // Give the assets a moment, then walk through the clips.
    let start = *view.started.get_or_insert(now + 1.5);
    if now < start {
        return;
    }
    let slot = ((now - start) / view.seconds) as usize;
    if slot >= view.clips.len() {
        exit.write(AppExit::Success);
        return;
    }
    let clip = view.clips[slot].clone();
    let index = view.names.iter().position(|n| *n == clip).unwrap();
    let node = view.graph.as_ref().unwrap().1[index];
    if !player.is_playing_animation(node) {
        player.stop_all();
        player.play(node).repeat();
    }
    let phase = (now - start) / view.seconds - slot as f32;
    let shot = slot * 3 + (phase * 3.0) as usize;
    if shot >= view.shots && phase * 3.0 - ((phase * 3.0) as usize as f32) > 0.15 {
        view.shots = shot + 1;
        let path = view.out.join(format!("{}_{}_{}.png", view.stem, clip, shot % 3));
        commands.spawn(Screenshot::image(view.image.clone())).observe(save_to_disk(path));
    }
}
