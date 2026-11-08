//! Visible doors for the Banjo-Kazooie maps: a glowing ring on the floor (and a faint light column) at every warp
//! trigger of a `bk_*` course (`warps` of `assets/courses/bk_*.json`, written by `scripts/export-bk-setup.py`).
//!
//! The level models contain the door frames, pipes and windows but not the door *actors*; this marks where stepping
//! through takes you. Gold: a door, pipe or window of the map; green: the lair door of a world; blue: the pad that
//! returns from a world to its lair door (the game does that from the pause menu); grey: the mirror of a one-way
//! entrance (Clanker's gills, Mr. Vile's nostrils, the rain barrel...).
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::prelude::*;
use jumpman_sim::object::ObjectBehaviorKind;

pub struct BkWarpsPlugin;

impl Plugin for BkWarpsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Markers>().add_systems(Update, (sync_markers, spin).chain());
    }
}

#[derive(Resource, Default)]
struct Markers {
    course: String,
    entities: Vec<Entity>,
}

#[derive(Component)]
struct Ring;

/// (red, green, blue) of a warp kind: see the module doc.
pub fn kind_color(kind: &str, from: &str, to: &str) -> [f32; 3] {
    match kind {
        "return" => [0.3, 0.6, 1.0],
        "mirror" => [0.75, 0.75, 0.8],
        // a lair room's warp into one of the 14 world maps is a world door
        _ if from.starts_with("bk_gl_") && !crate::game::courses::is_bk_subarea(to) => [0.35, 1.0, 0.45],
        _ => [1.0, 0.8, 0.25],
    }
}

/// `warp` objects of the course with their JSON rows (kind, target), in file order.
fn rows(root: &std::path::Path, course: &str) -> Vec<(String, String)> {
    let Ok(bytes) = std::fs::read(crate::game::course_data::CourseData::path(root, course, 1)) else { return vec![] };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return vec![] };
    json.get("warps")
        .and_then(|w| w.as_array())
        .map(|a| a.iter().map(|w| (w["kind"].as_str().unwrap_or("setup").to_string(), w["warpTo"].as_str().unwrap_or("").to_string())).collect())
        .unwrap_or_default()
}

fn sync_markers(
    mut commands: Commands,
    sim: Option<Res<GameSim>>,
    root: Option<Res<AssetRoot>>,
    mut state: ResMut<Markers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(sim), Some(root)) = (sim, root) else { return };
    let id = &sim.course.request.id;
    if *id == state.course {
        return;
    }
    state.course = id.clone();
    for e in state.entities.drain(..) {
        commands.entity(e).despawn();
    }
    if !id.starts_with("bk_") {
        return;
    }
    let rows = rows(&root.0, id);
    let warps = sim.mario.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::Warp);
    for (o, (kind, to)) in warps.zip(rows) {
        let radius = (o.param % 1000.0) * 10.0;
        let height = ((o.param / 1000.0).floor() * 10.0).max(50.0);
        let [r, g, b] = kind_color(&kind, id, &to);
        // the ring lies on the floor under the trigger (or mid-volume when there is none)
        let floor = sim.mario.world.find_floor(o.pos[0], o.pos[1] + height + 60.0, o.pos[2]);
        let y = if floor.surface.is_some() && floor.height > o.pos[1] - 260.0 { floor.height + 6.0 } else { o.pos[1] + height * 0.5 };
        let ring_r = radius.clamp(50.0, 150.0) as f32;
        let ring = meshes.add(Annulus::new(ring_r * SCALE * 0.86, ring_r * SCALE));
        let column = meshes.add(Cylinder::new(ring_r * SCALE * 0.9, 0.7));
        let glow = materials.add(StandardMaterial { base_color: Color::srgba(r, g, b, 0.9), unlit: true, alpha_mode: AlphaMode::Add, double_sided: true, cull_mode: None, ..default() });
        let haze = materials.add(StandardMaterial { base_color: Color::srgba(r, g, b, 0.06), unlit: true, alpha_mode: AlphaMode::Add, double_sided: true, cull_mode: None, ..default() });
        let at = to_bevy([o.pos[0], y, o.pos[2]]);
        let ring_e = commands.spawn((Mesh3d(ring), MeshMaterial3d(glow), Transform::from_translation(at).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)), Ring)).id();
        let column_e = commands.spawn((Mesh3d(column), MeshMaterial3d(haze), Transform::from_translation(at + Vec3::Y * 0.35))).id();
        state.entities.extend([ring_e, column_e]);
    }
}

/// The rings breathe a little so they read as live.
fn spin(time: Res<Time>, mut rings: Query<&mut Transform, With<Ring>>) {
    let s = 1.0 + 0.06 * (time.elapsed_secs() * 3.0).sin();
    for mut t in &mut rings {
        t.scale = Vec3::new(s, s, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_tell_the_kinds_apart() {
        let door = kind_color("setup", "bk_gl_ttc_lobby", "bk_treasure_trove_cove");
        assert_ne!(kind_color("return", "bk_treasure_trove_cove", "bk_gl_ttc_lobby"), door);
        assert_ne!(door, kind_color("setup", "bk_treasure_trove_cove", "bk_ttc_sandcastle"));
        assert_ne!(kind_color("mirror", "bk_a", "bk_b"), kind_color("setup", "bk_a", "bk_b"));
    }
}
