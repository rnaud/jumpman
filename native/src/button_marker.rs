//! `bevy_ui::widget::Button` became a deprecated type alias that cannot be used as a value in 0.20,
//! so the menus use this plain marker instead (Interaction is still driven by `ui_focus_system`).
use bevy::prelude::*;

#[derive(Component, Default, Clone, Copy, Debug)]
#[require(Node, bevy::ui::FocusPolicy::Block, Interaction)]
#[allow(deprecated)]
pub struct Button;

/// 0.20 moved `BorderRadius` into `Node::border_radius`; this component keeps the call sites and a system copies it over.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BorderRadius(pub bevy::ui::BorderRadius);

impl BorderRadius {
    pub const MAX: Self = Self(bevy::ui::BorderRadius::MAX);
    pub fn all(v: Val) -> Self { Self(bevy::ui::BorderRadius::all(v)) }
}

pub fn apply_border_radius(mut q: Query<(&BorderRadius, &mut Node), Changed<BorderRadius>>) {
    for (r, mut n) in &mut q { n.border_radius = r.0; }
}

pub struct ShimPlugin;
impl Plugin for ShimPlugin {
    fn build(&self, app: &mut App) { app.add_systems(PostUpdate, apply_border_radius.before(bevy::ui::UiSystems::Prepare)); }
}

/// 0.20 replaced `AssetServer::load_with_settings` by the load builder.
pub trait LoadWithSettings {
    fn load_builder_with_settings<'a, A: bevy::asset::Asset, S: bevy::asset::meta::Settings>(&self, path: impl Into<bevy::asset::AssetPath<'a>>, f: impl Fn(&mut S) + Send + Sync + 'static) -> Handle<A>;
}
impl LoadWithSettings for AssetServer {
    fn load_builder_with_settings<'a, A: bevy::asset::Asset, S: bevy::asset::meta::Settings>(&self, path: impl Into<bevy::asset::AssetPath<'a>>, f: impl Fn(&mut S) + Send + Sync + 'static) -> Handle<A> {
        self.load_builder().with_settings(f).load(path)
    }
}
