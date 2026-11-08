//! The cap switches (`bhvCapSwitch`, capswitch.inc.c): a coloured button on its base, squashed flat when Mario presses
//! it. The collision half lives in [`super::wing::WingSwitch`]; this draws it (red = wing cap, green = metal cap,
//! blue = vanish cap, the model's `geo_switch_anim_state`).
use super::flow::Progress;
use super::CourseScoped;
use crate::render::{scene, to_bevy, AssetRoot};
use bevy::prelude::*;

#[derive(Component)]
struct CapSwitchTop;

pub struct CapSwitchViewPlugin;

impl Plugin for CapSwitchViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_cap_switch);
    }
}

/// Model of the button for a switch of `kind` (0 wing, 1 metal, 2 vanish).
pub fn top_model(kind: i32) -> String {
    match kind {
        1 | 2 | 3 => format!("actors/capswitch/variants/cap_switch_geo/geo_switch_anim_state_{kind}.glb"),
        _ => "actors/capswitch/cap_switch_geo.glb".to_string(),
    }
}

fn sync_cap_switch(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, progress: Res<Progress>, mut top: Query<&mut Transform, With<CapSwitchTop>>) {
    let Some(w) = &progress.wing else { return };
    // `CourseScoped` entities disappear on a course switch (or a restart): spawn the pair again when the button is gone.
    if top.is_empty() {
        let base = "actors/capswitch/cap_switch_base.glb";
        let button = top_model(w.kind);
        if root.exists(base) && root.exists(&button) {
            commands.spawn((scene(&assets, base), Transform::from_translation(to_bevy(w.base_pos)).with_scale(Vec3::splat(0.5)), Visibility::Inherited, CourseScoped));
            commands.spawn((CapSwitchTop, scene(&assets, &button), Transform::from_translation(to_bevy(w.top_pos)).with_scale(Vec3::splat(0.5)), Visibility::Inherited, CourseScoped));
        }
        return;
    }
    for mut tf in &mut top {
        // `cur_obj_scale_over_time(2, 4, 0.5, 0.1)`: the button shrinks to a fifth of its height
        tf.scale = Vec3::new(0.5, 0.5 * w.height.max(0.2) as f32, 0.5);
        tf.translation = to_bevy(w.top_pos);
    }
}
