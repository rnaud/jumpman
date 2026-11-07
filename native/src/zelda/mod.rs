//! Zelda-course content logic (engine-free ports): `ZeldaSave` (save.ts's village/slingshot/Deku
//! parts + `zeldaTreasure.ts`), `FairySlingshot`, `Gohma`, `KokiriSwordQuest` and the OoT
//! textbox layout. The scene content that hosts them (`kokiriVillage.ts`, `dekuDungeon.ts`) and
//! their Bevy views are still to do; see `native/PORTING.md`.
#![allow(dead_code)]
pub mod dungeon;
pub mod gohma;
pub mod save;
pub mod scene;
pub mod slingshot;
pub mod sword_quest;
pub mod textbox;
pub mod visit;

use bevy::prelude::*;

pub struct ZeldaPlugin;

impl Plugin for ZeldaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<save::ZeldaSave>();
    }
}

