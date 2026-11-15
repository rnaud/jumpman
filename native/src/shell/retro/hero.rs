//! The title screen's hero: the real Mario model, loaded from the player's own imported data exactly as the
//! character select loads it ([`super::select::spawn_mario_model`]; nothing is bundled with the app), standing
//! left of the menu and playing the game's own idle animation (SM64's head-turning idle, an occasional stretch).
//!
//! It exists only while the title page is on screen ([`manage_hero`] spawns it and despawns it). The title is
//! only reached once ROM data is present (a first run opens the import screen and starts the menus after the
//! import, see `byor::launch_screen`), but if the model file is missing anyway (a damaged or partial import)
//! the title simply shows no hero: the spinning gem moves to the middle of the left half instead.
use super::model::Page;
use super::scene::{half_view_width, LAYER, TITLE_DEPTH};
use super::select::{spawn_mario_model, MarioAnim};
use super::{LowRes, MenuRes, RetroActive};
use crate::render::AssetRoot;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jumpman_sim::state::{ACT_IDLE, ACT_START_SLEEPING};

/// Where the hero stands: this fraction of the half view width left of the centre.
pub const TITLE_X: f32 = 0.55;
/// Mario's size on the title (the glb is in the same units as the select stage, where 1.0 stands about 1.6 tall).
const TITLE_SCALE: f32 = 1.55;

#[derive(Component, Clone)]
pub struct TitleAnimator;

#[derive(Resource, Default)]
pub struct TitleHero {
    root: Option<Entity>,
    anim: Option<MarioAnim>,
    /// The model could not be loaded: no hero (checked again the next time the title opens).
    missing: bool,
    t: f32,
}

impl TitleHero {
    /// Whether Mario stands on the title right now.
    pub fn shown(&self) -> bool {
        self.root.is_some()
    }
}

/// Whether the imported data holds Mario's model (`actors/mario/mario_geo.glb` under the asset root).
pub fn model_present(root: Option<&AssetRoot>) -> bool {
    root.is_some_and(|r| r.0.join(crate::game::runner::MARIO_MODEL).is_file())
}

/// Spawn Mario when the title page opens and take him down when it closes.
#[allow(clippy::too_many_arguments)]
pub fn manage_hero(
    mut commands: Commands,
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    mut hero: ResMut<TitleHero>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
) {
    let want = active.0 && menu.0.page == Page::Title;
    if want && hero.root.is_none() && !hero.missing {
        let (Some(server), Some(root)) = (server, root) else { return };
        if !model_present(Some(&root)) {
            hero.missing = true;
            info!("title: no Mario model in the imported data, showing no hero");
            return;
        }
        match spawn_mario_model(&mut commands, &server, &root, &mut graphs, &RenderLayers::layer(LAYER), TitleAnimator) {
            Ok((e, anim)) => {
                commands.entity(e).insert(Visibility::Inherited);
                hero.root = Some(e);
                hero.anim = Some(anim);
                hero.t = 0.0;
            }
            Err(e) => {
                hero.missing = true;
                warn!("title: no Mario model: {e}");
            }
        }
    } else if !want {
        if let Some(e) = hero.root.take() {
            commands.entity(e).despawn();
        }
        hero.anim = None;
        // Look again next time (the data may have been imported meanwhile).
        hero.missing = false;
    }
}

/// The idle loop: SM64's standing idle (it cycles its three head-turning clips), and every 14 s a scratch and a yawn
/// (the first beats of his falling-asleep routine) before he snaps back. Returns the action and its action state.
pub fn idle_action(t: f32) -> (u32, f64) {
    let cycle = t % 14.0;
    if (10.0..11.3).contains(&cycle) {
        (ACT_START_SLEEPING, 1.0)
    } else if (11.3..12.8).contains(&cycle) {
        (ACT_START_SLEEPING, 2.0)
    } else {
        (ACT_IDLE, 0.0)
    }
}

pub fn pose_hero(time: Res<Time<Real>>, low: Res<LowRes>, mut hero: ResMut<TitleHero>, mut tfs: Query<&mut Transform>, mut players: Query<&mut AnimationPlayer, With<TitleAnimator>>) {
    let Some(root) = hero.root else { return };
    let dt = time.delta_secs().min(0.1);
    hero.t += dt;
    let t = hero.t;
    let (action, state) = idle_action(t);
    if let Some(anim) = hero.anim.as_mut() {
        let input = crate::game::sm64_animation::AnimInput { action, action_state: state, ..Default::default() };
        anim.pose(&input, dt as f64, &mut players);
    }
    if let Ok(mut tf) = tfs.get_mut(root) {
        let x = -half_view_width(TITLE_DEPTH, low.aspect()) * TITLE_X;
        // Turned a little toward the menu on the right, swaying slowly.
        let yaw = 0.4 + (t * 0.5).sin() * 0.18;
        *tf = Transform::from_translation(Vec3::new(x, -0.25, 0.0)).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(TITLE_SCALE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_loops_with_an_occasional_stretch() {
        assert_eq!(idle_action(1.0).0, ACT_IDLE);
        assert_eq!(idle_action(11.0).0, ACT_START_SLEEPING);
        assert_eq!(idle_action(15.0).0, ACT_IDLE, "loops");
        assert!(idle_action(10.5).1 < idle_action(12.0).1, "scratch, then yawn");
    }

    #[test]
    fn no_data_means_no_hero() {
        assert!(!model_present(None));
        assert!(!model_present(Some(&AssetRoot::new(std::path::PathBuf::from("/nonexistent-jumpman-data")))));
    }

    /// Without imported data the title spawns nothing (no stand-in character, nothing bundled); with data Mario comes and
    /// goes with the title page.
    #[test]
    fn hero_follows_the_title_page_and_needs_the_import() {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let have_data = data.join(crate::game::runner::MARIO_MODEL).is_file();
        let run = |root: std::path::PathBuf| {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AssetPlugin::default()))
                .init_asset::<Mesh>()
                .init_asset::<StandardMaterial>()
                .init_asset::<Image>()
                .init_asset::<AnimationGraph>()
                .init_asset::<AnimationClip>()
                .init_asset::<bevy::gltf::Gltf>()
                .init_asset::<bevy::world_serialization::WorldAsset>()
                .insert_resource(AssetRoot::new(root))
                .insert_resource(RetroActive(true))
                .insert_resource(MenuRes(super::super::model::Menu::default()))
                .init_resource::<TitleHero>()
                .add_systems(Update, manage_hero);
            app.update();
            app.update();
            let shown = app.world().resource::<TitleHero>().shown();
            app.world_mut().resource_mut::<MenuRes>().0.page = Page::Grid;
            app.update();
            app.update();
            (shown, app.world().resource::<TitleHero>().shown())
        };
        assert_eq!(run(std::path::PathBuf::from("/nonexistent-jumpman-data")), (false, false), "no import: no hero");
        if have_data {
            assert_eq!(run(data), (true, false), "Mario on the title, gone on the grid");
        }
    }
}
