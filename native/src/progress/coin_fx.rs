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
        app.init_resource::<CoinFx>().add_systems(FixedUpdate, (debug_numbers, watch_pickups, step_fx).chain().after(crate::SimSet))
            .add_systems(Update, no_shadow_for_effects);
    }
}

/// Marks the root of a number / spark model: its meshes never cast sun shadows (in the web build only the
/// player characters have `castShadow`).
#[derive(Component)]
pub struct EffectModel;

/// Propagate [`EffectModel`] to the meshes of the loaded glb.
fn no_shadow_for_effects(mut commands: Commands, new: Query<Entity, Added<Mesh3d>>, parents: Query<&ChildOf>, roots: Query<(), With<EffectModel>>) {
    for e in &new {
        let mut cur = e;
        while let Ok(up) = parents.get(cur) {
            cur = up.parent();
            if roots.contains(cur) {
                commands.entity(e).try_insert(bevy::light::NotShadowCaster);
                break;
            }
        }
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
    /// Numbers requested by other systems (`number(n, pos)`), spawned on the next tick.
    numbers: Vec<(u32, [f64; 3])>,
}

impl CoinFx {
    /// `coinEffects.number`: a bouncing 1-7 above `pos`.
    pub fn number(&mut self, n: u32, pos: [f64; 3]) {
        self.numbers.push((n, pos));
    }

    pub fn burst(&mut self, pos: [f64; 3]) {
        self.bursts.push((pos, 3));
    }

    fn random(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 33) as f64 / (1u64 << 31) as f64
    }
}

/// Headless hook: `JUMPMAN_FX_NUMBER=<n>` spawns the bouncing secret/red-coin number `n` (and every
/// number up to it, in a row) in front of Mario on tick 20 for screenshots.
fn debug_numbers(sim: Res<GameSim>, mut fx: ResMut<CoinFx>) {
    if sim.ticks != 20 {
        return;
    }
    let Some(n) = std::env::var("JUMPMAN_FX_NUMBER").ok().and_then(|v| v.parse::<u32>().ok()) else { return };
    for k in 1..=n {
        let p = sim.mario.pos;
        fx.number(k, [p[0] + (k as f64 - (n as f64 + 1.0) / 2.0) * 90.0, p[1] + 60.0, p[2] + 150.0]);
    }
}

fn watch_pickups(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    sim: Res<GameSim>,
    mut fx: ResMut<CoinFx>,
    mut switched: MessageReader<super::CourseSwitched>,
    mut sfx: MessageWriter<crate::audio::SfxCue>,
    character: Option<Res<crate::characters::Character>>,
    mut pickup_fx: Option<ResMut<crate::fx::pickup_fx::PickupFx>>,
) {
    use jumpman_sim::collectible_theme::{skin, Source, Theme};
    let theme = crate::render::currency_skin::current_theme(character.as_deref());
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
            // Mario gets the SM64 three-spark burst; rupees and notes pop and sparkle like the shaped pickups of the other worlds.
            match (theme, pickup_fx.as_mut()) {
                (Theme::Coin, _) | (_, None) => fx.burst(pos),
                (theme, Some(pfx)) => {
                    let value = if p.kind == PickupKind::Red { 2 } else { p.value };
                    pfx.collect(crate::fx::pickup_fx::Look::of_skin(skin(theme, Source::Coin(value))), to_bevy(p.pos) + Vec3::Y * 32.0 * crate::render::SCALE);
                }
            }
            if p.kind == PickupKind::Red {
                sfx.write(crate::audio::SfxCue::RedCoin(sim.red_coins));
            }
            if p.kind == PickupKind::Red && sim.red_coins > 0 && sim.red_coins < 8 && root.exists(&digit_path(sim.red_coins)) {
                let entity = commands
                    .spawn((scene(&assets, &digit_path(sim.red_coins)), Transform::from_translation(to_bevy([pos[0], pos[1] + 25.0, pos[2]])), Billboard::Full, EffectModel, CourseScoped))
                    .id();
                fx.digits.push(Digit { entity, pos: [pos[0], pos[1] + 25.0, pos[2]], age: 0, velocity: 26.0 });
            }
        }
    }
    fx.seen = sim.pickups.iter().map(|p| p.collected).collect();
    for (n, pos) in std::mem::take(&mut fx.numbers) {
        if n > 0 && n < 8 && root.exists(&digit_path(n)) {
            let entity = commands.spawn((scene(&assets, &digit_path(n)), Transform::from_translation(to_bevy([pos[0], pos[1] + 25.0, pos[2]])), Billboard::Full, EffectModel, CourseScoped)).id();
            fx.digits.push(Digit { entity, pos: [pos[0], pos[1] + 25.0, pos[2]], age: 0, velocity: 26.0 });
        }
    }
}

/// What the effect state machine needs from the engine (entities are attached by the system; the
/// differential test plugs in a recorder).
pub trait FxSink {
    /// A spark group appears at `pos`; returns its entity and the four frame children.
    fn spawn_spark(&mut self, pos: [f64; 3]) -> (Entity, [Entity; 4]);
    fn number_moved(&mut self, entity: Entity, pos: [f64; 3]);
    fn despawn(&mut self, entity: Entity);
    fn spark_pose(&mut self, entity: Entity, kids: [Entity; 4], scale: f64, frame: usize);
}

impl CoinFx {
    /// `CoinEffects.step`: bouncing numbers, then one spark per pending burst, then spark animation.
    pub fn advance(&mut self, sink: &mut dyn FxSink) {
        // Bouncing numbers.
        let mut finished = vec![];
        for (i, d) in self.digits.iter_mut().enumerate() {
            d.pos[1] += d.velocity;
            d.velocity -= 2.0;
            if d.velocity < -21.0 {
                d.velocity = 14.0;
            }
            d.age += 1;
            if d.age == 36 {
                finished.push(i);
            } else {
                sink.number_moved(d.entity, d.pos);
            }
        }
        for i in finished.into_iter().rev() {
            let d = self.digits.remove(i);
            sink.despawn(d.entity);
            self.bursts.push(([d.pos[0], d.pos[1] - 30.0, d.pos[2]], 3));
        }
        // One spark per pending burst per tick.
        let bursts = std::mem::take(&mut self.bursts);
        for (pos, left) in bursts {
            let (jx, jz) = (self.random() * 30.0 - 15.0, self.random() * 30.0 - 15.0);
            let (entity, kids) = sink.spawn_spark([pos[0] + jx, pos[1] + 25.0, pos[2] + jz]);
            self.sparks.push(Spark { entity, kids, age: 0 });
            if left > 1 {
                self.bursts.push((pos, left - 1));
            }
        }
        // Spark animation: 4 frames over 10 ticks, shrinking at the end.
        let mut done = vec![];
        for (i, s) in self.sparks.iter_mut().enumerate() {
            let age = s.age;
            s.age += 1;
            if age >= 10 {
                done.push(i);
                continue;
            }
            sink.spark_pose(s.entity, s.kids, if age >= 8 { 0.6 } else { 1.0 }, ((age >> 1) as usize).min(3));
        }
        for i in done.into_iter().rev() {
            let s = self.sparks.remove(i);
            sink.despawn(s.entity);
        }
    }

    /// `(y, age, velocity)` of each number, burst `left` counters, `(age, scale, visible frame)` of each spark.
    #[cfg(test)]
    pub fn snapshot(&self) -> (Vec<(f64, u32, f64)>, Vec<u32>, Vec<(u32, f64, usize)>) {
        let sparks = self
            .sparks
            .iter()
            .map(|s| {
                // `advance` already counted the animation tick that was just shown.
                let age = s.age - 1;
                (s.age, if age >= 8 { 0.6 } else { 1.0 }, ((age >> 1) as usize).min(3))
            })
            .collect();
        (self.digits.iter().map(|d| (d.pos[1], d.age, d.velocity)).collect(), self.bursts.iter().map(|b| b.1).collect(), sparks)
    }

    #[cfg(test)]
    pub fn advance_for_test(&mut self) {
        for (_, pos) in std::mem::take(&mut self.numbers) {
            self.digits.push(Digit { entity: Entity::PLACEHOLDER, pos: [pos[0], pos[1] + 25.0, pos[2]], age: 0, velocity: 26.0 });
        }
        struct Null;
        impl FxSink for Null {
            fn spawn_spark(&mut self, _: [f64; 3]) -> (Entity, [Entity; 4]) {
                (Entity::PLACEHOLDER, [Entity::PLACEHOLDER; 4])
            }
            fn number_moved(&mut self, _: Entity, _: [f64; 3]) {}
            fn despawn(&mut self, _: Entity) {}
            fn spark_pose(&mut self, _: Entity, _: [Entity; 4], _: f64, _: usize) {}
        }
        self.advance(&mut Null);
    }

    #[cfg(test)]
    pub fn clear_for_test(&mut self) {
        self.digits.clear();
        self.sparks.clear();
        self.bursts.clear();
    }
}

struct BevySink<'a, 'w, 's, 'q1, 'q2, 'q3, 'q4, 'q5, 'q6> {
    commands: &'a mut Commands<'w, 's>,
    assets: &'a AssetServer,
    root: &'a AssetRoot,
    tfs: &'a mut Query<'q1, 'q2, &'q3 mut Transform>,
    vis: &'a mut Query<'q4, 'q5, &'q6 mut Visibility>,
}

impl FxSink for BevySink<'_, '_, '_, '_, '_, '_, '_, '_, '_> {
    fn spawn_spark(&mut self, pos: [f64; 3]) -> (Entity, [Entity; 4]) {
        let mut kids = [Entity::PLACEHOLDER; 4];
        if !self.root.exists(SPARKLE) {
            return (Entity::PLACEHOLDER, kids);
        }
        let assets = self.assets;
        let entity = self
            .commands
            .spawn((Transform::from_translation(to_bevy(pos)), Visibility::Inherited, Billboard::Full, EffectModel, CourseScoped))
            .with_children(|p| {
                for (f, kid) in kids.iter_mut().enumerate() {
                    *kid = p.spawn((scene(assets, &spark_path(f)), if f == 0 { Visibility::Inherited } else { Visibility::Hidden })).id();
                }
            })
            .id();
        (entity, kids)
    }

    fn number_moved(&mut self, entity: Entity, pos: [f64; 3]) {
        if let Ok(mut tf) = self.tfs.get_mut(entity) {
            tf.translation = to_bevy(pos);
        }
    }

    fn despawn(&mut self, entity: Entity) {
        if let Ok(mut e) = self.commands.get_entity(entity) {
            e.despawn();
        }
    }

    fn spark_pose(&mut self, entity: Entity, kids: [Entity; 4], scale: f64, frame: usize) {
        if let Ok(mut tf) = self.tfs.get_mut(entity) {
            tf.scale = Vec3::splat(scale as f32);
        }
        for (k, kid) in kids.iter().enumerate() {
            if let Ok(mut v) = self.vis.get_mut(*kid) {
                *v = if k == frame { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

fn step_fx(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut fx: ResMut<CoinFx>, mut tfs: Query<&mut Transform>, mut vis: Query<&mut Visibility>) {
    let mut sink = BevySink { commands: &mut commands, assets: &assets, root: &root, tfs: &mut tfs, vis: &mut vis };
    fx.advance(&mut sink);
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
