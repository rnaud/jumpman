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
        let mut e = commands.spawn((Transform::from_translation(to_bevy(p.pos)), Visibility::Inherited, PickupView(i), Billboard::Full));
        if root.exists(model) {
            e.insert(scene(&assets, model));
        }
        views.entities.push(e.id());
    }
}

fn sync_pickups(sim: Res<GameSim>, mut views: Query<(&PickupView, &mut Transform, &mut Visibility)>) {
    for (view, mut tf, mut vis) in &mut views {
        let Some(p) = sim.pickups.get(view.0) else { continue };
        tf.translation = to_bevy(p.pos);
        let hidden = p.collected || p.loot.as_ref().map_or(false, |l| !l.visible);
        *vis = if hidden { Visibility::Hidden } else { Visibility::Inherited };
        if p.kind == PickupKind::Dynamic && p.loot.is_some() {
            tf.scale = Vec3::splat(0.7);
        }
    }
}
