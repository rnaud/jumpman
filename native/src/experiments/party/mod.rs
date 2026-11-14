//! Sunstone Shindig: an original board-game party mode (native only; no web port), played on the real Bob-omb Battlefield:
//! the board is laid on the course floor and every mini-game is a physics game on one of its real spots.
//!
//! * `common.rs`, `board.rs`, `game.rs` (+ `game/`), `minis/`: the engine-free, seeded core with unit tests.
//! * `sites.rs`: where the boards and the mini-games lie on the course (sampled from its collision, checked by tests).
//! * `app.rs` (state, experiment shell, save, hooks), `flow.rs` (frame flow), `view.rs` (3D), `ui.rs` (screens).
pub mod app;
pub mod board;
pub mod common;
pub mod flow;
pub mod game;
pub mod minis;
pub mod sites;
pub mod ui;
pub mod view;

use app::PartyApp;
use bevy::prelude::*;

/// Ordering handle: the UI systems run after the game flow.
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct PartySet;

pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        let active = resource_exists::<PartyApp>;
        app.add_systems(
            Update,
            (flow::flow, view::ensure_pieces, view::board_props, view::board_props_update, view::stage_shapes, view::mini_models, view::piece_targets, view::render_pieces, view::camera)
                .chain()
                .in_set(PartySet)
                .run_if(active)
                .after(crate::CameraSet)
                .before(crate::experiments::apply_camera),
        )
        .add_systems(FixedUpdate, view::drive_pieces.after(crate::SimSet).run_if(active))
        .add_plugins(ui::PartyUiPlugin);
    }
}
