//! Port of `mushroom2d.ts`: the hand-authored side-scrolling "Mushroom Kingdom 1-1" level
//! (segmented ground with two gaps, brick rows, floating platforms, pipes, a staircase, coins,
//! Goombas and a finish star). The course is entered in 2D side mode by the Dimensions
//! experiment (`objects::mushroom`), exactly like `Game.ts` starts it.
use super::course_data::Goomba;
use super::playground::{add_piece_collision, Material, Piece, Shape};
use super::world::{CourseRequest, CourseVisual, LoadedCourse, Prop};
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::object_manager::ObjectManager;
use std::sync::Arc;

pub const LANE_DEPTH: f64 = 360.0;
/// Coins needed for the finish star (`requiredCoins`).
pub const REQUIRED_COINS: u32 = 8;

fn slab(pieces: &mut Vec<Piece>, x: f64, top: f64, w: f64, h: f64, d: f64, material: Material) {
    pieces.push(Piece { shape: Shape::Slab { x, z: 0.0, w, top, h, d }, material, surface: 0, hidden: false });
}

pub fn pieces() -> Vec<Piece> {
    use Material::*;
    let mut p = vec![];
    // Segmented ground with two jumpable gaps.
    for (from, to) in [(-2300.0, -520.0), (-360.0, 430.0), (610.0, 1180.0), (1320.0, 2640.0)] {
        slab(&mut p, (from + to) / 2.0, 0.0, to - from, 280.0, LANE_DEPTH, Grass);
    }
    // Brick rows and floating platforms.
    for x in [-1700.0, -1600.0, -1500.0, -1400.0, -100.0, 0.0, 100.0] {
        slab(&mut p, x, 300.0, 90.0, 90.0, 90.0, Stone);
    }
    slab(&mut p, 780.0, 390.0, 420.0, 80.0, 160.0, Wood);
    slab(&mut p, 1100.0, 520.0, 300.0, 80.0, 160.0, Wood);
    slab(&mut p, 1450.0, 300.0, 120.0, 120.0, 120.0, Stone);
    // Classic staircase to the finish.
    for i in 0..6 {
        let i = i as f64;
        slab(&mut p, 1750.0 + i * 100.0, 100.0 + i * 90.0, 100.0, 100.0 + i * 90.0, 170.0, Wood);
    }
    // Pipe collision columns.
    for x in PIPES {
        slab(&mut p, x, 300.0, 160.0, 300.0, 160.0, Wood);
    }
    p
}

const PIPES: [f64; 3] = [-700.0, 430.0, 1550.0];
const COINS: [[f64; 2]; 13] = [
    [-1700.0, 380.0],
    [-1600.0, 380.0],
    [-1500.0, 380.0],
    [-1400.0, 380.0],
    [-1800.0, 120.0],
    [-650.0, 120.0],
    [-250.0, 360.0],
    [80.0, 360.0],
    [430.0, 140.0],
    [780.0, 450.0],
    [1120.0, 580.0],
    [1550.0, 360.0],
    [1900.0, 700.0],
];
const GOOMBAS: [f64; 5] = [-1150.0, -700.0, 180.0, 980.0, 1500.0];

pub fn load(request: CourseRequest) -> LoadedCourse {
    let pieces = pieces();
    let mut world = CollisionWorld::default();
    for piece in &pieces {
        add_piece_collision(&mut world, piece);
    }
    let prop = |model: &str, pos: [f64; 3], scale: f64, billboard: bool| Prop { model: model.into(), pos, yaw: 0.0, rotation: None, scale, billboard, scroll_t: 0.0, spin_z: 0.0 };
    let mut props = vec![];
    // Glb units are metres (x0.01): pipes are 2 tall (fit to 300), trees 8 (fit to 260), the post 4.3 (to 620).
    for x in PIPES {
        props.push(prop("actors/warp_pipe/warp_pipe_geo.glb", [x, 0.0, 0.0], 1.5, false));
    }
    for (x, z) in [(-1600.0, -320.0), (-1250.0, -320.0), (-850.0, -320.0), (-350.0, -320.0), (300.0, -320.0), (900.0, -320.0), (1500.0, -320.0), (2050.0, -320.0), (2300.0, 260.0)] {
        props.push(prop("actors/tree/bubbly_tree_geo.glb", [x, 0.0, z], 0.325, true));
    }
    props.push(prop("actors/poundable_pole/wooden_post_geo.glb", [2380.0, 0.0, 0.0], 1.44, false));

    let mut extras = super::course_extras::CourseExtras::default();
    extras.goombas = GOOMBAS.iter().map(|&x| Goomba { pos: [x, 0.0, 0.0], triplet: false }).collect();
    LoadedCourse {
        request,
        name: "Mushroom Kingdom 1-1".into(),
        visual: CourseVisual::Playground(pieces),
        world: Arc::new(world),
        spawn: SpawnPoint { action: None, pos: [-1900.0, 90.0, 0.0], face_yaw: 0x4000 as f64 },
        death_height: -900.0,
        sky: None,
        objects: ObjectManager::new(vec![], None, None),
        props,
        coins: COINS.iter().map(|c| [c[0], c[1], 0.0]).collect(),
        red_coins: vec![],
        doors: vec![],
        warp_spawns: Default::default(),
        fading_warps: vec![],
        death_warp: None,
        fall_warp: None,
        success_entry: None,
        success_area: None,
        success_course: None,
        skipped_behaviors: vec![],
        extras,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_gaps_and_blocks() {
        let c = load(CourseRequest::new("mushroom2d"));
        let h = |x: f64| c.world.find_floor(x, 2000.0, 0.0);
        assert_eq!(h(-1900.0).height, 0.0);
        assert!(h(-440.0).surface.is_none(), "gap");
        assert_eq!(h(-1600.0).height, 300.0 + 0.0, "brick top");
        assert_eq!(h(1750.0 + 500.0).height, 100.0 + 450.0, "last stair");
        assert_eq!(c.coins.len(), 13);
        assert_eq!(c.extras.goombas.len(), 5);
        let s = c.world.find_floor(c.spawn.pos[0], c.spawn.pos[1] + 200.0, 0.0);
        assert_eq!(s.height, 0.0);
    }
}
