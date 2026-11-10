//! Coin pickups (static, red and runtime-spawned): billboarded coin models hidden when taken.
//!
//! TODO(coin-effects): `coinEffects.ts` sparkle bursts, star/red-coin reward models
//! (`redReward`, `sideStars`, `hundredCoinStar`) and collected-star ghost models.
use super::billboard::Billboard;
use super::{scene, to_bevy, AssetRoot};
use crate::game::runner::{GameSim, PickupKind};
use bevy::prelude::*;

pub struct PickupViewPlugin;

impl Plugin for PickupViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickupViews>().add_systems(Update, (spawn_pickups, sync_pickups).chain().before(crate::CameraSet));
    }
}

#[derive(Component)]
pub struct PickupView(pub usize);

/// A coin that was just taken: instead of vanishing on the spot it jumps up and shrinks away over [`POP_SECONDS`].
#[derive(Component, Default)]
pub struct PickupPop {
    was_collected: bool,
    /// Seconds since it was taken; `None` when not popping.
    age: Option<f32>,
}

pub const POP_SECONDS: f32 = 0.2;

/// `(rise in SM64 units, scale factor)` of a taken coin `age` seconds after the pickup, `None` once it is gone.
pub fn pop_pose(age: f32) -> Option<(f32, f32)> {
    if !(0.0..POP_SECONDS).contains(&age) {
        return None;
    }
    let k = age / POP_SECONDS;
    // Up fast and easing out; swells a little, then shrinks to nothing.
    let rise = 90.0 * (1.0 - (1.0 - k) * (1.0 - k));
    let scale = if k < 0.3 { 1.0 + 0.35 * k / 0.3 } else { 1.35 * (1.0 - (k - 0.3) / 0.7) };
    Some((rise, scale))
}

#[derive(Resource, Default)]
pub struct PickupViews {
    entities: Vec<Entity>,
}

const YELLOW: &str = "actors/coin/yellow_coin_no_shadow_geo.glb";
const RED: &str = "actors/coin/red_coin_no_shadow_geo.glb";
const BLUE: &str = "actors/coin/blue_coin_geo.glb";

fn spawn_pickups(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, sim: Res<GameSim>, mut views: ResMut<PickupViews>) {
    while views.entities.len() > sim.pickups.len() {
        commands.entity(views.entities.pop().unwrap()).despawn();
    }
    for i in views.entities.len()..sim.pickups.len() {
        let p = &sim.pickups[i];
        let model = match (p.kind, p.value) {
            (PickupKind::Red, _) => RED,
            (_, 5) => BLUE,
            _ => YELLOW,
        };
        let mut e = commands.spawn((Transform::from_translation(to_bevy(p.pos)), Visibility::Inherited, PickupView(i), PickupPop { was_collected: p.collected, age: None }, Billboard::Full));
        if root.exists(model) {
            e.insert(scene(&assets, model));
        }
        views.entities.push(e.id());
    }
}

fn sync_pickups(sim: Res<GameSim>, time: Res<Time>, mut views: Query<(&PickupView, &mut PickupPop, &mut Transform, &mut Visibility)>) {
    let mario = sim.mario.pos;
    for (view, mut pop, mut tf, mut vis) in &mut views {
        let Some(p) = sim.pickups.get(view.0) else { continue };
        tf.translation = to_bevy(p.pos);
        let base = if p.kind == PickupKind::Dynamic && p.loot.is_some() { 0.7 } else { 1.0 };
        // Taken by Mario (a loot coin that timed out far away just disappears).
        if p.collected && !pop.was_collected && (p.pos[0] - mario[0]).hypot(p.pos[2] - mario[2]) < 400.0 {
            pop.age = Some(0.0);
        }
        pop.was_collected = p.collected;
        if !p.collected {
            pop.age = None;
        }
        if let Some(age) = pop.age {
            match pop_pose(age) {
                Some((rise, scale)) => {
                    tf.translation.y += rise * super::SCALE;
                    tf.scale = Vec3::splat(base * scale);
                    *vis = Visibility::Inherited;
                    pop.age = Some(age + time.delta_secs());
                    continue;
                }
                None => pop.age = None,
            }
        }
        let hidden = p.collected || p.loot.as_ref().map_or(false, |l| !l.visible);
        *vis = if hidden { Visibility::Hidden } else { Visibility::Inherited };
        if tf.scale.x != base {
            tf.scale = Vec3::splat(base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_coin_rises_swells_and_shrinks_away() {
        let (r0, s0) = pop_pose(0.0).unwrap();
        assert_eq!((r0, s0), (0.0, 1.0));
        let (r1, s1) = pop_pose(POP_SECONDS * 0.3).unwrap();
        assert!(r1 > 40.0 && (s1 - 1.35).abs() < 1e-3);
        let (r2, s2) = pop_pose(POP_SECONDS * 0.95).unwrap();
        assert!(r2 > r1 && r2 <= 90.0 && s2 < 0.15);
        assert!(pop_pose(POP_SECONDS).is_none() && pop_pose(-0.1).is_none());
    }
}
