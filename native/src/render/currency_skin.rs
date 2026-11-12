//! The collectible theme in the SM64 worlds: every coin of a course (placed coins, loot coins, coin objects) is drawn as the
//! currency of the character being played. Mario keeps the real SM64 coin models; Link sees rupees and Banjo notes, drawn by the
//! shapes of `fx::pickup_fx` (no model of either exists in the imported data: they are generated in code).
//!
//! A coin view carries two children: the glb ([`CoinModel`]) and a [`CurrencySkin`] mesh; [`restyle`] shows one or the other
//! from the live [`Character`], every frame, so a character switch re-skins coins that are already out.
use crate::characters::Character;
use crate::fx::pickup_fx::{Look, PickupLooks};
use crate::fx::anchor::{skin_local, SM64_COIN};
use bevy::prelude::*;
use jumpman_sim::collectible_theme::{skin, Source, Theme};

pub struct CurrencySkinPlugin;

impl Plugin for CurrencySkinPlugin {
    fn build(&self, app: &mut App) {
        // after the billboards turned to the camera: the skin is counter-rotated against its parent's rotation of THIS frame
        app.add_systems(Update, restyle.after(super::billboard::face_camera));
    }
}

/// The live theme: the character in the shell, Mario when there is no selection (the experiments).
pub fn current_theme(character: Option<&Character>) -> Theme {
    character.map_or(Theme::Coin, |c| c.theme())
}

/// The real SM64 coin glb of a coin view: shown only to Mario.
#[derive(Component)]
pub struct CoinModel;

/// The generated stand-in for a coin: a rupee or a note that turns. `source` is what the world defined.
#[derive(Component)]
pub struct CurrencySkin {
    pub source: Source,
    look: Option<Look>,
}

/// The skin entity for a coin; spawn it as a child of the coin's view.
pub fn skin_bundle(source: Source, looks: &PickupLooks, theme: Theme) -> impl Bundle {
    let look = Look::of_skin(skin(theme, source));
    (
        CurrencySkin { source, look: Some(look) },
        Mesh3d(looks.mesh(look)),
        MeshMaterial3d(looks.material(look)),
        skin_local(look, SM64_COIN, 0.0, Quat::IDENTITY),
        if theme == Theme::Coin { Visibility::Hidden } else { Visibility::Inherited },
        bevy::light::NotShadowCaster,
    )
}

fn restyle(
    character: Option<Res<Character>>,
    time: Res<Time>,
    looks: Option<Res<PickupLooks>>,
    mut skins: Query<(&mut CurrencySkin, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility, &ChildOf), Without<CoinModel>>,
    mut models: Query<&mut Visibility, (With<CoinModel>, Without<CurrencySkin>)>,
    parents: Query<&Transform, Without<CurrencySkin>>,
) {
    let theme = current_theme(character.as_deref());
    let want = if theme == Theme::Coin { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut models {
        if *v != want {
            *v = want;
        }
    }
    let Some(looks) = looks else { return };
    let show = if theme == Theme::Coin { Visibility::Hidden } else { Visibility::Inherited };
    let now = time.elapsed_secs();
    for (mut s, mut mesh, mut mat, mut tf, mut vis, parent) in &mut skins {
        if *vis != show {
            *vis = show;
        }
        if theme == Theme::Coin {
            continue;
        }
        let look = Look::of_skin(skin(theme, s.source));
        if s.look != Some(look) {
            s.look = Some(look);
            mesh.0 = looks.mesh(look);
            mat.0 = looks.material(look);
        }
        // all of a course's coins turn together, like the SM64 coins do; the shape stays upright whatever the view's billboard
        // rotation is, with its centre where the SM64 coin's is (`fx::anchor`)
        let parent_rot = parents.get(parent.parent()).map_or(Quat::IDENTITY, |p| p.rotation);
        *tf = skin_local(look, SM64_COIN, now, parent_rot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_character_picks_the_theme() {
        assert_eq!(current_theme(None), Theme::Coin);
        assert_eq!(current_theme(Some(&Character::Mario)), Theme::Coin);
        assert_eq!(current_theme(Some(&Character::Link)), Theme::Rupee);
        assert_eq!(current_theme(Some(&Character::YoungLink)), Theme::Rupee);
        assert_eq!(current_theme(Some(&Character::Banjo)), Theme::Note);
    }
}
