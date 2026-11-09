//! Pushable dungeon blocks of the exported OoT dungeons (`Obj_Oshihiki` actors in the placement file's `mechanisms`):
//! `jumpman_sim::oot_blocks` moves them, each one is a collision cube in the world and a coloured box on screen
//! (the dungeon colour of `sColors`: Dodongo's Cavern grey-green, Forest Temple red-brown).
//!
//! Test hook: `JUMPMAN_MOVERS_LOG=1` prints Link's hold and the blocks while he is at one (and the platforms of `movers.rs`).
use crate::game::course_data::CollisionData;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot};
use bevy::prelude::*;
use jumpman_sim::link::moves::GrabFace;
use jumpman_sim::oot_blocks::{Block, BlockEvent, Blocks, Grip, U};

#[derive(Resource, Default)]
pub struct BlockWorld {
    pub blocks: Blocks,
    colliders: Vec<super::dungeon::Collider>,
    placed: Vec<Option<[f64; 3]>>,
    key: String,
    spawned: bool,
}

#[derive(Component)]
pub struct BlockView(pub usize);

/// A cube of side `2 * half` standing on its origin.
pub fn cube(half: f64) -> CollisionData {
    let h = half;
    let vertices = vec![[-h, 0.0, -h], [h, 0.0, -h], [h, 2.0 * h, -h], [-h, 2.0 * h, -h], [-h, 0.0, h], [h, 0.0, h], [h, 2.0 * h, h], [-h, 2.0 * h, h]];
    let t = |a: i32, b: i32, c: i32| [a, b, c, 0, 0];
    let triangles = vec![t(4, 5, 6), t(4, 6, 7), t(1, 0, 3), t(1, 3, 2), t(1, 6, 5), t(1, 2, 6), t(0, 4, 7), t(0, 7, 3), t(3, 7, 6), t(3, 6, 2)];
    CollisionData { rooms: vec![], vertices, triangles, hookable: None }
}

impl BlockWorld {
    pub fn build(mechanisms: &[crate::zelda_enemies::Mechanism]) -> Self {
        let list: Vec<Block> = mechanisms.iter().filter(|m| m.actor == "ACTOR_OBJ_OSHIHIKI").map(|m| Block::new(m.pos, m.param)).collect();
        let colliders = list.iter().map(|b| super::dungeon::Collider::new(cube(b.half), 0.0)).collect();
        let placed = vec![None; list.len()];
        Self { blocks: Blocks::new(list), colliders, placed, key: String::new(), spawned: false }
    }

    fn sync(&mut self, world: &mut jumpman_sim::collision::CollisionWorld) {
        for (i, b) in self.blocks.list.iter().enumerate() {
            if self.placed[i] != Some(b.pos) {
                self.colliders[i].move_to(world, Some(b.pos));
                self.placed[i] = Some(b.pos);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_blocks(
    mut commands: Commands,
    mut blocks: ResMut<BlockWorld>,
    mut sim: ResMut<GameSim>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut views: Query<(Entity, &BlockView, &mut Transform)>,
) {
    let req = sim.course.request.clone();
    let key = format!("{}:{}", req.id, req.area);
    // (`zelda::movers` adds the slabs it offers after this)
    sim.mario.link.grab_face = None;
    if blocks.key != key {
        for (e, _, _) in &views {
            commands.entity(e).despawn();
        }
        let mech = crate::zelda_enemies::load_placements(&root.0, &req.id, req.area).map(|p| p.mechanisms).unwrap_or_default();
        *blocks = BlockWorld::build(&mech);
        blocks.key = key;
        let color = match req.id.as_str() {
            "dodongos_cavern" => Color::srgb(0.42, 0.47, 0.43),
            "forest_temple" => Color::srgb(0.56, 0.39, 0.34),
            _ => Color::srgb(0.43, 0.34, 0.16),
        };
        for (i, b) in blocks.blocks.list.iter().enumerate() {
            let side = 2.0 * b.half as f32 * crate::render::SCALE;
            commands.spawn((BlockView(i), Mesh3d(meshes.add(Cuboid::new(side, side, side))), MeshMaterial3d(materials.add(StandardMaterial { base_color: color, perceptual_roughness: 0.9, ..default() })), Transform::default()));
        }
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        blocks.sync(world);
        return;
    }
    if blocks.blocks.list.is_empty() {
        return;
    }
    let player = sim.mario.pos;
    let strong = sim.mario.link.inv.strength >= 1;
    // The face Link could take hold of (A), and the hold he has (`Mv::Grab`).
    if sim.mario.link.enabled {
        sim.mario.link.grab_face = blocks.blocks.grab_face(player, sim.mario.face_angle[1], strong).map(|(i, yaw)| GrabFace { id: i as u32, yaw });
    }
    let hold = sim.mario.link.grip.filter(|g| (g.id as usize) < blocks.blocks.list.len());
    if std::env::var("JUMPMAN_MOVERS_LOG").is_ok() && (sim.mario.link.grab_face.is_some() || hold.is_some() || blocks.blocks.list.iter().any(|b| b.moving())) {
        eprintln!("BLOCKS mv={:?} face={:?} grip={:?} link={:?} yaw={} blocks={:?}", sim.mario.link.mv, sim.mario.link.grab_face, hold, sim.mario.pos, sim.mario.face_angle[1], blocks.blocks.list.iter().map(|b| b.pos).collect::<Vec<_>>());
    }
    // Probing clones the collision world: only when a block moves or Link stands at one.
    let busy = blocks.blocks.list.iter().any(|b| b.moving() || (player[0] - b.pos[0]).hypot(player[2] - b.pos[2]) < b.half + 120.0 * U);
    if !busy {
        update_views(&blocks, &mut views);
        return;
    }
    // Probe the world with the blocks' own colliders taken out (a block would otherwise bump into itself).
    let mut carry: Option<[f64; 3]> = None;
    let mut world_arc = sim.mario.world.clone();
    let events = {
        let b = &mut *blocks;
        let world = std::sync::Arc::make_mut(&mut world_arc);
        for (i, c) in b.colliders.iter_mut().enumerate() {
            c.move_to(world, None);
            b.placed[i] = None;
        }
        let probe = world.clone();
        let wall = |pos: [f64; 3], half: f64, _i: usize| {
            let mut p = [pos[0], pos[1] + half, pos[2]];
            probe.f32_find_wall_collision(&mut p, 0.0, half * 0.9) > 0
        };
        let floor = |pos: [f64; 3]| {
            let hit = probe.find_floor(pos[0], pos[1] + 30.0, pos[2]);
            hit.surface.is_some().then_some(hit.height)
        };
        // Leaning toward the face pushes, away from it pulls (only with room behind Link: `Player_Action_8084B9E4`).
        let grip = hold.filter(|g| g.force != 0).and_then(|g| {
            let toward = [(g.yaw * std::f64::consts::PI / 32768.0).sin(), (g.yaw * std::f64::consts::PI / 32768.0).cos()];
            let dir = if g.force > 0 { toward } else { [-toward[0], -toward[1]] };
            (g.force > 0 || back_clear(&probe, player, toward)).then_some(Grip { block: g.id as usize, dir })
        });
        let before = hold.map(|g| b.blocks.list[g.id as usize].pos);
        let events = b.blocks.step(grip, strong, &wall, &floor);
        carry = hold.zip(before).map(|(g, p0)| {
            let p1 = b.blocks.list[g.id as usize].pos;
            [p1[0] - p0[0], 0.0, p1[2] - p0[2]]
        });
        events
    };
    blocks.sync(std::sync::Arc::make_mut(&mut world_arc));
    sim.mario.world = world_arc;
    // Link walks along with the block he is pushing or pulling.
    if let Some(d) = carry {
        for k in 0..3 {
            sim.mario.pos[k] += d[k];
        }
    }
    for e in &events {
        match e {
            BlockEvent::Pushing(_) => {
                crate::audio::oot_sfx::play("EV_BLOCK_RISING");
            }
            BlockEvent::Settled(_) | BlockEvent::Fell(_) | BlockEvent::Bumped(_) => {
                crate::audio::oot_sfx::play("EV_BLOCK_BOUND");
            }
        }
    }
    update_views(&blocks, &mut views);
}

/// `Player_Action_8084B9E4`: Link only pulls when the spot 40 units behind him has a floor at his height and no wall.
pub(super) fn back_clear(world: &jumpman_sim::collision::CollisionWorld, pos: [f64; 3], toward: [f64; 2]) -> bool {
    let p = [pos[0] - toward[0] * 40.0 * U, pos[1], pos[2] - toward[1] * 40.0 * U];
    let floor = world.find_floor(p[0], p[1] + 26.0 * U, p[2]);
    if floor.surface.is_none() || (floor.height - pos[1]).abs() >= 20.0 * U {
        return false;
    }
    let mut at = [p[0], pos[1] + 26.0 * U, p[2]];
    world.f32_find_wall_collision(&mut at, 0.0, 30.0) == 0
}

fn update_views(blocks: &BlockWorld, views: &mut Query<(Entity, &BlockView, &mut Transform)>) {
    for (_, v, mut tf) in views.iter_mut() {
        if let Some(b) = blocks.blocks.list.get(v.0) {
            tf.translation = to_bevy([b.pos[0], b.pos[1] + b.half, b.pos[2]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_is_a_closed_box_of_the_right_size() {
        let c = cube(100.0);
        assert_eq!(c.vertices.len(), 8);
        assert_eq!(c.triangles.len(), 10);
        let max = c.vertices.iter().map(|v| v[1]).fold(0.0, f64::max);
        assert_eq!(max, 200.0);
    }

    #[test]
    fn blocks_come_from_the_oshihiki_actors_only() {
        let m = |a: &str, p: i32| crate::zelda_enemies::Mechanism { actor: a.into(), param: p, pos: [1.0, 2.0, 3.0], rot: [0.0; 3] };
        let w = BlockWorld::build(&[m("ACTOR_OBJ_OSHIHIKI", 1), m("ACTOR_DOOR_SHUTTER", 0), m("ACTOR_OBJ_OSHIHIKI", 3)]);
        assert_eq!(w.blocks.list.len(), 2);
        assert!(w.blocks.list[1].heavy && !w.blocks.list[0].heavy);
    }
}
