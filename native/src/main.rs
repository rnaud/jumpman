mod movement;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::InputSystem;
use bevy::prelude::*;
use movement::{Controls, Course, Mario, Terrain};
use std::path::PathBuf;

const MARIO: &str = "actors/mario/mario_geo.glb";
const COURSE: &str = "courses/castle_grounds.json";
const SCALE: f32 = 0.01;

#[derive(Component)]
struct Avatar;

#[derive(Component)]
struct Tree;

#[derive(Resource)]
struct Clips {
    graph: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
}

fn asset_root() -> PathBuf {
    let bundle = std::env::current_exe()
        .expect("executable path")
        .parent()
        .unwrap()
        .join("../Resources/assets");
    if bundle.join(COURSE).is_file() {
        bundle
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }
}

fn main() {
    let root = asset_root();
    let course: Course =
        serde_json::from_slice(&std::fs::read(root.join(COURSE)).expect(
            "Castle Grounds assets missing. See the SM64 export instructions in README.md.",
        ))
        .expect("invalid Castle Grounds export");
    let terrain = Terrain::new(&course);
    App::new()
        .insert_resource(Mario::spawn(&terrain))
        .insert_resource(terrain)
        .insert_resource(Time::<Fixed>::from_hz(30.))
        .insert_resource(Controls::default())
        .insert_resource(ClearColor(Color::srgb(0.48, 0.76, 0.96)))
        .insert_resource(AmbientLight { brightness: 320., ..default() })
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: root.to_string_lossy().into_owned(), ..default()
        }).set(WindowPlugin { primary_window: Some(Window {
            title: "Jumpman  •  Castle Grounds     |     WASD move   Space jump   Shift + Space long jump   Q/E orbit   R reset".into(),
            resolution: (1040_f32, 680_f32).into(), ..default()
        }), ..default() }))
        .add_systems(Startup, move |commands: Commands, assets: Res<AssetServer>, graphs: ResMut<Assets<AnimationGraph>>, meshes: ResMut<Assets<Mesh>>, materials: ResMut<Assets<StandardMaterial>>|
            setup(commands, assets, graphs, meshes, materials, &course))
        .add_systems(PreUpdate, read_input.after(InputSystem))
        .add_systems(FixedUpdate, step)
        .add_systems(Update, (present, animate))
        .run();
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    course: &Course,
) {
    let scene =
        |path: &str| SceneRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(path.to_owned())));
    commands.spawn(scene(&course.model));
    for prop in &course.objects {
        let transform = Transform::from_translation(Vec3::from(prop.pos) * SCALE)
            .with_rotation(Quat::from_rotation_y(prop.yaw.to_radians()));
        let mut entity = commands.spawn((scene(&prop.model), transform));
        if prop.model.contains("/tree/") {
            entity.insert(Tree);
        }
    }
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.23, 0.67, 0.82, 0.45),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for &[_, x1, z1, x2, z2, y] in &course.water_boxes {
        commands.spawn((
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size((x2 - x1) * SCALE, (z2 - z1) * SCALE),
                ),
            ),
            MeshMaterial3d(water.clone()),
            Transform::from_xyz(
                (x1 + x2) * 0.5 * SCALE,
                (y - 4.) * SCALE,
                (z1 + z2) * 0.5 * SCALE,
            ),
        ));
    }
    let (graph, nodes) = AnimationGraph::from_clips(
        [0xc5, 0x48, 0x72, 0x4d, 0x13]
            .map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(MARIO))),
    );
    commands.insert_resource(Clips {
        graph: graphs.add(graph),
        nodes,
    });
    commands
        .spawn((Avatar, scene(MARIO)))
        .observe(prepare_avatar);
    commands.spawn((Camera3d::default(), Tonemapping::ReinhardLuminance));
    commands.spawn((
        DirectionalLight {
            illuminance: 1800.,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(-5., 10., 5.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut input: ResMut<Controls>,
    terrain: Res<Terrain>,
    mut mario: ResMut<Mario>,
    mut exit: EventWriter<AppExit>,
) {
    use KeyCode::*;
    let held = |a, b| i32::from(keys.pressed(a) || keys.pressed(b)) as f32;
    input.stick = Vec2::new(
        held(KeyD, ArrowRight) - held(KeyA, ArrowLeft),
        held(KeyS, ArrowDown) - held(KeyW, ArrowUp),
    )
    .normalize_or_zero();
    input.orbit += (held(KeyE, KeyE) - held(KeyQ, KeyQ)) * time.delta_secs() * 2.;
    input.jump_pressed |= keys.just_pressed(Space);
    input.jump_held = keys.pressed(Space);
    input.long_jump = keys.pressed(ShiftLeft) || keys.pressed(ShiftRight);
    if keys.just_pressed(KeyR) {
        *mario = Mario::spawn(&terrain);
        *input = Controls::default();
    }
    if keys.just_pressed(Escape) {
        exit.write(AppExit::Success);
    }
}

fn step(mut mario: ResMut<Mario>, mut input: ResMut<Controls>, terrain: Res<Terrain>) {
    mario.tick(&input, &terrain);
    input.jump_pressed = false;
}

fn present(
    mario: Res<Mario>,
    fixed: Res<Time<Fixed>>,
    input: Res<Controls>,
    mut avatar: Single<&mut Transform, With<Avatar>>,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<Avatar>)>,
    mut trees: Query<&mut Transform, (With<Tree>, Without<Avatar>, Without<Camera3d>)>,
) {
    let p = mario.previous.lerp(mario.pos, fixed.overstep_fraction()) * SCALE;
    avatar.translation = p;
    avatar.rotation = Quat::from_rotation_y(mario.angle());
    let focus = p + Vec3::Y * 1.6;
    let offset = Vec3::new(0., 1.35, 6.5);
    **camera = Transform::from_translation(focus + Quat::from_rotation_y(input.orbit) * offset)
        .looking_at(focus, Vec3::Y);
    for mut tree in &mut trees {
        let dir = camera.translation - tree.translation;
        tree.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
    }
}

#[derive(Component)]
struct MarioAnimation;

fn prepare_avatar(
    trigger: Trigger<bevy::scene::SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    parts: Query<(&Name, Option<&bevy::gltf::GltfExtras>)>,
    players: Query<&AnimationPlayer>,
) {
    for child in children.iter_descendants(trigger.target()) {
        if players.contains(child) {
            commands.entity(child).insert(MarioAnimation);
        }
        if let Ok((_, Some(extras))) = parts.get(child) {
            if extras.value.contains(r#""mario_cap_wings""#) {
                commands.entity(child).insert(Visibility::Hidden);
            }
        }
    }
}

fn animate(
    mut commands: Commands,
    clips: Res<Clips>,
    mario: Res<Mario>,
    mut players: Query<
        (Entity, &mut AnimationPlayer, Option<&AnimationGraphHandle>),
        With<MarioAnimation>,
    >,
) {
    let index = if !mario.grounded {
        if mario.long_jump {
            4
        } else {
            3
        }
    } else if mario.speed > 20. {
        2
    } else if mario.speed > 1. {
        1
    } else {
        0
    };
    let node = clips.nodes[index];
    for (entity, mut player, graph) in &mut players {
        if graph.is_none() {
            commands
                .entity(entity)
                .insert(AnimationGraphHandle(clips.graph.clone()));
        }
        if !player.is_playing_animation(node) {
            player.stop_all();
            let active = player.start(node);
            if mario.grounded {
                active.repeat();
            }
        }
    }
}
