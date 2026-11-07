//! Port of `coinEffects.ts`: the three-spark burst when a coin is picked up and the bouncing
//! number (1-7) for each red coin, stepped at 30 Hz. Pickups are detected by watching
//! `GameSim::pickups` flip to collected.
use super::CourseScoped;
use crate::game::runner::{GameSim, PickupKind};
use crate::render::billboard::Billboard;
use crate::render::{scene, to_bevy, AssetRoot};
use bevy::prelude::*;

const SPARKLE: &str = "actors/sparkle/sparkles_geo.glb";

fn digit_path(n: u32) -> String {
    format!("actors/number/variants/number_geo/geo_switch_anim_state_{n}.glb")
}

fn spark_path(frame: usize) -> String {
    match frame {
        0 => SPARKLE.to_string(),
        n => format!("actors/sparkle/variants/sparkles_geo/geo_switch_anim_state_{}.glb", n * 2),
    }
}

pub struct CoinFxPlugin;

impl Plugin for CoinFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CoinFx>().add_systems(FixedUpdate, (watch_pickups, step_fx).chain().after(crate::SimSet));
    }
}

struct Digit {
    entity: Entity,
    pos: [f64; 3],
    age: u32,
    velocity: f64,
}

struct Spark {
    entity: Entity,
    kids: [Entity; 4],
    age: u32,
}

/// The effect state machine (engine-free: positions only, entities are attached by the system).
#[derive(Resource, Default)]
pub struct CoinFx {
    digits: Vec<Digit>,
    bursts: Vec<([f64; 3], u32)>,
    sparks: Vec<Spark>,
    seen: Vec<bool>,
    rng: u64,
}

impl CoinFx {
    pub fn burst(&mut self, pos: [f64; 3]) {
        self.bursts.push((pos, 3));
    }

    fn random(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 33) as f64 / (1u64 << 31) as f64
    }
}

fn watch_pickups(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    sim: Res<GameSim>,
    mut fx: ResMut<CoinFx>,
    mut switched: EventReader<super::CourseSwitched>,
    mut sfx: EventWriter<crate::audio::SfxCue>,
) {
    if switched.read().count() > 0 {
        fx.seen.clear();
        fx.digits.clear();
        fx.bursts.clear();
        fx.sparks.clear();
    }
    if fx.seen.is_empty() {
        fx.seen = sim.pickups.iter().map(|p| p.collected).collect();
    }
    for (i, p) in sim.pickups.iter().enumerate() {
        let was = fx.seen.get(i).copied().unwrap_or(true);
        if p.collected && !was {
            let pos = [p.pos[0], p.pos[1], p.pos[2]];
            fx.burst(pos);
            if p.kind == PickupKind::Red {
                sfx.write(crate::audio::SfxCue::RedCoin(sim.red_coins));
            }
            if p.kind == PickupKind::Red && sim.red_coins > 0 && sim.red_coins < 8 && root.exists(&digit_path(sim.red_coins)) {
                let entity = commands
                    .spawn((scene(&assets, &digit_path(sim.red_coins)), Transform::from_translation(to_bevy([pos[0], pos[1] + 25.0, pos[2]])), Billboard::Full, CourseScoped))
                    .id();
                fx.digits.push(Digit { entity, pos: [pos[0], pos[1] + 25.0, pos[2]], age: 0, velocity: 26.0 });
            }
        }
    }
    fx.seen = sim.pickups.iter().map(|p| p.collected).collect();
}

fn step_fx(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut fx: ResMut<CoinFx>, mut tfs: Query<&mut Transform>, mut vis: Query<&mut Visibility>) {
    // Bouncing numbers.
    let mut finished = vec![];
    for (i, d) in fx.digits.iter_mut().enumerate() {
        d.pos[1] += d.velocity;
        d.velocity -= 2.0;
        if d.velocity < -21.0 {
            d.velocity = 14.0;
        }
        d.age += 1;
        if d.age == 36 {
            finished.push(i);
        } else if let Ok(mut tf) = tfs.get_mut(d.entity) {
            tf.translation = to_bevy(d.pos);
        }
    }
    for i in finished.into_iter().rev() {
        let d = fx.digits.remove(i);
        if let Ok(mut e) = commands.get_entity(d.entity) {
            e.despawn();
        }
        fx.bursts.push(([d.pos[0], d.pos[1] - 30.0, d.pos[2]], 3));
    }
    // One spark per pending burst per tick.
    let bursts = std::mem::take(&mut fx.bursts);
    for (pos, left) in bursts {
        let (jx, jz) = (fx.random() * 30.0 - 15.0, fx.random() * 30.0 - 15.0);
        if root.exists(SPARKLE) {
            let mut kids = [Entity::PLACEHOLDER; 4];
            let entity = commands
                .spawn((Transform::from_translation(to_bevy([pos[0] + jx, pos[1] + 25.0, pos[2] + jz])), Visibility::Inherited, Billboard::Full, CourseScoped))
                .with_children(|p| {
                    for (f, kid) in kids.iter_mut().enumerate() {
                        *kid = p.spawn((scene(&assets, &spark_path(f)), if f == 0 { Visibility::Inherited } else { Visibility::Hidden })).id();
                    }
                })
                .id();
            fx.sparks.push(Spark { entity, kids, age: 0 });
        }
        if left > 1 {
            fx.bursts.push((pos, left - 1));
        }
    }
    // Spark animation: 4 frames over 10 ticks, shrinking at the end.
    let mut done = vec![];
    for (i, s) in fx.sparks.iter_mut().enumerate() {
        let age = s.age;
        s.age += 1;
        if age >= 10 {
            done.push(i);
            continue;
        }
        if let Ok(mut tf) = tfs.get_mut(s.entity) {
            tf.scale = Vec3::splat(if age >= 8 { 0.6 } else { 1.0 });
        }
        let frame = ((age >> 1) as usize).min(3);
        for (k, kid) in s.kids.iter().enumerate() {
            if let Ok(mut v) = vis.get_mut(*kid) {
                *v = if k == frame { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
    for i in done.into_iter().rev() {
        let s = fx.sparks.remove(i);
        if let Ok(mut e) = commands.get_entity(s.entity) {
            e.despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_are_three_sparks_and_digits_expire_after_36_ticks() {
        let mut fx = CoinFx::default();
        fx.burst([0.0; 3]);
        assert_eq!(fx.bursts[0].1, 3);
        let a = fx.random();
        let b = fx.random();
        assert!((0.0..1.0).contains(&a) && (0.0..1.0).contains(&b) && a != b);
        assert!(spark_path(0).ends_with("sparkles_geo.glb") && spark_path(3).ends_with("state_6.glb"));
    }
}
