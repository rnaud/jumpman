//! Which playable characters (`characters::Character`) each mode can be played with, and the fallback for the modes that
//! cannot: such a mode plays as Mario for the run (with a toast) and the player's own selection comes back afterwards.
//!
//! The table itself is the shared `ExperimentId::supported_characters` (Gem Clash branch `gemclash-characters`, with its own
//! `character_gate` in `experiments/mod.rs` for the modes it lists as Mario-only: Kart and Halls) plus the one mode it does not
//! list: Party. This module's [`enforce`] therefore only acts where the shared gate does not (Party today); when the shared
//! table learns about Party this file's `supported` is just a forwarder and `enforce` can go. The player's own pick while any gate
//! has put Mario in its place is read by the menu through [`own_pick`].
//!
//! What "supported" means for the rest: the rules read the player's `MarioRuntime::action`, and Link's slash and Banjo's claw,
//! barge, peck and ground pound reuse the punch / jump-kick / ground-pound action ids on purpose, so Swarm, Gate and Last Star
//! Standing see them as the usual hits. Link has no dive or pound there, and inside a mode he is Link's model on Mario's physics
//! (the faithful controller is off while an experiment runs, `link::link_active`; A is a jump there, `link::a_jumps`, not the OoT roll); Banjo keeps his own moves.
//!
//! Why the Mario-only modes are Mario-only:
//! * Kart: the player drives one of the eight MK64 racers; the on-foot part afterwards is Mario's "kart anywhere" world.
//! * Party: the four board pieces and the mini-games are `CrowdMario` bodies, the main player is not on the board at all.
//! * Halls: the abilities you find (wall kick, ground pound, long jump, the power caps) are Mario's move set.
use super::ExperimentId;
use crate::characters::Character;
use bevy::prelude::*;

const MARIO: [Character; 1] = [Character::Mario];

/// The characters this mode can be played with (never empty; Mario is always first).
pub fn supported(id: ExperimentId) -> &'static [Character] {
    match id {
        ExperimentId::Party => &MARIO,
        other => other.supported_characters(),
    }
}

pub fn is_supported(id: ExperimentId, c: Character) -> bool {
    supported(id).contains(&c)
}

/// The character a run of `id` uses when the player has selected `selected`.
pub fn resolve(id: ExperimentId, selected: Character) -> Character {
    if is_supported(id, selected) {
        selected
    } else {
        supported(id)[0]
    }
}

/// A short note for the mode card / menu (ASCII).
pub fn note(id: ExperimentId) -> &'static str {
    if supported(id).len() == 1 {
        "Mario only"
    } else {
        "Plays as Mario, Link, Young Link or Banjo"
    }
}

/// Remembers the player's own selection while a mode that cannot use it has put Mario in its place.
#[derive(Resource, Default, Debug)]
pub struct CharacterFallback {
    pub stash: Option<Character>,
}

/// The player's own pick: what a gate (this one or the shared `character_gate`) is holding while it plays a mode as Mario,
/// else the live selection.
pub fn own_pick(fallback: Option<&CharacterFallback>, gate: Option<&super::GatedCharacter>, current: Character) -> Character {
    fallback.and_then(|f| f.stash).or(gate.and_then(|g| g.stash)).unwrap_or(current)
}

/// What [`step`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    None,
    /// The mode cannot use the selection: play as this character instead.
    Forced(Character),
    /// The mode is over: the player's own selection is back.
    Restored(Character),
}

/// One frame of the fallback. `active` is the running mode (if any), `selected` the current `Character` resource;
/// returns the new value for it. The stash always holds the player's latest real choice, even when they pick another
/// character in the hub while the Mario-only mode runs.
pub fn step(stash: &mut Option<Character>, selected: Character, active: Option<ExperimentId>) -> (Character, Change) {
    match active {
        Some(id) if !is_supported(id, selected) => {
            let forced = resolve(id, selected);
            *stash = Some(selected);
            (forced, Change::Forced(forced))
        }
        Some(_) => (selected, Change::None),
        None => match stash.take() {
            Some(own) if own != selected => (own, Change::Restored(own)),
            _ => (selected, Change::None),
        },
    }
}

/// Debug aid for the inventory runs: `JUMPMAN_NO_CHARACTER_FALLBACK=1` shows what a mode does with a character it never expected.
fn disabled() -> bool {
    std::env::var_os("JUMPMAN_NO_CHARACTER_FALLBACK").is_some()
}

pub fn enforce(
    slot: Option<Res<super::ExperimentSlot>>,
    mut current: ResMut<Character>,
    mut fallback: ResMut<CharacterFallback>,
    mut toast: ResMut<crate::hud::Toast>,
    mut last_entered: Local<Option<ExperimentId>>,
) {
    let active = slot.as_deref().and_then(|s| s.id);
    if disabled() {
        return;
    }
    // The shared gate in `experiments/mod.rs` owns the modes the shared table lists as Mario-only.
    if active.is_some_and(|id| !id.supported_characters().contains(&current)) {
        return;
    }
    let (next, change) = step(&mut fallback.stash, *current, active);
    match change {
        Change::Forced(c) => {
            info!("character fallback: {:?} cannot play {:?}, playing as {:?} (own pick kept)", fallback.stash, active, c);
            *current = c;
            toast.show(format!("{} IS MARIO ONLY - PLAYING AS {}", active.map_or("THIS MODE".into(), |i| i.title().to_uppercase()), c.label()), 2.6);
        }
        Change::Restored(c) => {
            info!("character fallback: restored {:?}", c);
            *current = c;
            toast.show(format!("BACK TO {}", c.label()), 1.6);
        }
        Change::None => {
            // First frame of a mode that takes the selection: say who you are and where to change it.
            if active != *last_entered {
                if let Some(id) = active {
                    if supported(id).len() > 1 {
                        toast.show(format!("PLAYING AS {} - CHANGE IT IN THE MENU (PLAY AS)", current.label()), 2.4);
                    }
                }
            }
            if *current != next {
                *current = next;
            }
        }
    }
    *last_entered = active;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::EXPERIMENT_IDS;

    #[test]
    fn every_mode_has_an_answer_that_starts_with_mario() {
        for id in EXPERIMENT_IDS {
            let s = supported(id);
            assert!(!s.is_empty(), "{id:?}");
            assert_eq!(s[0], Character::Mario, "{id:?}");
            assert!(!note(id).is_empty());
            for c in [Character::Mario, Character::Link, Character::YoungLink, Character::Banjo] {
                assert!(is_supported(id, resolve(id, c)), "{id:?} {c:?}");
            }
        }
        // The shell's list and this module's list cover the same 17 modes.
        assert_eq!(crate::shell::ExperimentId::ALL.len(), EXPERIMENT_IDS.len());
        for e in crate::shell::ExperimentId::ALL {
            assert!(e.theirs().is_some(), "{e:?}");
        }
    }

    #[test]
    fn the_mario_only_modes_are_exactly_these() {
        let mario_only: Vec<_> = EXPERIMENT_IDS.into_iter().filter(|id| supported(*id).len() == 1).collect();
        for id in [ExperimentId::Kart, ExperimentId::Halls, ExperimentId::Party] {
            assert!(mario_only.contains(&id), "{id:?}");
        }
        assert_eq!(mario_only.len(), 3);
        assert_eq!(resolve(ExperimentId::Kart, Character::Banjo), Character::Mario);
        assert_eq!(resolve(ExperimentId::Relic, Character::Banjo), Character::Banjo);
    }

    #[test]
    fn unsupported_falls_back_and_the_selection_comes_back() {
        let mut stash = None;
        // Supported mode: untouched.
        assert_eq!(step(&mut stash, Character::Link, Some(ExperimentId::Relic)), (Character::Link, Change::None));
        assert_eq!(stash, None);
        // Mario-only mode: Mario for the run, own choice remembered.
        assert_eq!(step(&mut stash, Character::Link, Some(ExperimentId::Kart)), (Character::Mario, Change::Forced(Character::Mario)));
        assert_eq!(stash, Some(Character::Link));
        // Stays Mario on the following frames without a new toast.
        assert_eq!(step(&mut stash, Character::Mario, Some(ExperimentId::Kart)), (Character::Mario, Change::None));
        assert_eq!(stash, Some(Character::Link));
        // The player picks Banjo in the hub mid-run: forced back, stash follows the choice.
        assert_eq!(step(&mut stash, Character::Banjo, Some(ExperimentId::Kart)), (Character::Mario, Change::Forced(Character::Mario)));
        assert_eq!(stash, Some(Character::Banjo));
        // Leaving restores it, once.
        assert_eq!(step(&mut stash, Character::Mario, None), (Character::Banjo, Change::Restored(Character::Banjo)));
        assert_eq!(step(&mut stash, Character::Banjo, None), (Character::Banjo, Change::None));
        assert_eq!(stash, None);
    }

    #[test]
    fn mario_selected_needs_no_fallback() {
        let mut stash = None;
        assert_eq!(step(&mut stash, Character::Mario, Some(ExperimentId::Party)), (Character::Mario, Change::None));
        assert_eq!(step(&mut stash, Character::Mario, None), (Character::Mario, Change::None));
    }

    #[test]
    fn system_forces_then_restores_in_a_world() {
        let mut app = App::new();
        app.insert_resource(Character::Banjo)
            .init_resource::<CharacterFallback>()
            .init_resource::<crate::hud::Toast>()
            .insert_resource(super::super::ExperimentSlot::default())
            .add_systems(Update, enforce);
        app.update();
        assert_eq!(*app.world().resource::<Character>(), Character::Banjo);
        app.world_mut().resource_mut::<super::super::ExperimentSlot>().id = Some(ExperimentId::Party);
        app.update();
        assert_eq!(*app.world().resource::<Character>(), Character::Mario);
        app.world_mut().resource_mut::<super::super::ExperimentSlot>().id = None;
        app.update();
        assert_eq!(*app.world().resource::<Character>(), Character::Banjo);
    }
}
