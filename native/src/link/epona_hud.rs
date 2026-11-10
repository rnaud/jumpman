//! Epona's HUD and touch-button labels: the six carrots (`interfaceCtx.numHorseBoosts`), the contextual A button
//! (MOUNT next to her, CARROT xN while she moves, DISMOUNT when she stands) and a short prompt line for keyboard
//! and gamepad players.
use super::ui::LinkMenu;
use super::link_active;
use crate::characters::Character;
use crate::experiments::ExperimentSlot;
use crate::game::runner::GameSim;
use crate::touch::ButtonLabels;
use bevy::prelude::*;
use crate::button_marker::BorderRadius;
use jumpman_sim::link::epona::MAX_BOOSTS;
use jumpman_sim::link::items::Age;
use jumpman_sim::link::Mv;

pub struct EponaHudPlugin;

impl Plugin for EponaHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn.in_set(crate::progress::PersistentSpawn))
            .add_systems(Update, (update_pips, update_labels).before(crate::CameraSet));
    }
}

#[derive(Component)]
struct CarrotRow;
#[derive(Component)]
struct Carrot(u8);
#[derive(Component)]
struct Prompt;

fn spawn(mut commands: Commands) {
    commands
        .spawn((CarrotRow, Visibility::Hidden, GlobalZIndex(31), Node { position_type: PositionType::Absolute, left: Val::Px(16.0), top: Val::Px(84.0), column_gap: Val::Px(4.0), ..default() }))
        .with_children(|p| {
            for i in 0..MAX_BOOSTS {
                // A carrot: orange triangle-ish pip with a green top.
                p.spawn((Carrot(i), Node { width: Val::Px(11.0), height: Val::Px(20.0), ..default() }, BackgroundColor(Color::srgb(0.95, 0.5, 0.1)), BorderRadius::all(Val::Px(5.0))));
            }
        });
    commands.spawn((Prompt, crate::hints::ControlHint, Visibility::Hidden, GlobalZIndex(31), Text::new(""), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::srgb(1.0, 0.95, 0.7)), Node { position_type: PositionType::Absolute, bottom: Val::Px(120.0), left: Val::Percent(36.0), ..default() }));
}

/// What A does right now (the OoT `DO_ACTION_*`: Climb / Faster / Down).
#[derive(Clone, PartialEq, Debug)]
pub enum AAction {
    None,
    Mount,
    Carrot(u8),
    Dismount,
}

pub fn a_action(sim: &GameSim) -> AAction {
    let link = &sim.mario.link;
    let e = &link.epona;
    if !link.enabled || !e.present || link.age != Age::Adult {
        return AAction::None;
    }
    if e.riding() {
        return if e.can_dismount() { AAction::Dismount } else { AAction::Carrot(e.boosts) };
    }
    if e.is_mounted() {
        return AAction::None;
    }
    if link.mv == Mv::Free && e.mount_side(sim.mario.pos).is_some() {
        return AAction::Mount;
    }
    AAction::None
}

fn update_pips(
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    menu: Res<LinkMenu>,
    sim: Res<GameSim>,
    mut row: Query<&mut Visibility, (With<CarrotRow>, Without<Prompt>)>,
    mut pips: Query<(&Carrot, &mut BackgroundColor)>,
    mut prompt: Query<(&mut Visibility, &mut Text), (With<Prompt>, Without<CarrotRow>)>,
) {
    let e = &sim.mario.link.epona;
    let on = link_active(&character, &slot) && sim.mario.link.enabled && e.is_mounted() && !menu.open;
    for mut v in &mut row {
        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (c, mut bg) in &mut pips {
        let have = c.0 < e.boosts;
        let want = if have { Color::srgb(0.95, 0.5, 0.1) } else { Color::srgba(0.3, 0.2, 0.1, 0.5) };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    let text = if link_active(&character, &slot) && !menu.open {
        match a_action(&sim) {
            AAction::None => String::new(),
            AAction::Mount => "A  MOUNT EPONA".into(),
            AAction::Carrot(n) => format!("A  CARROT x{n}"),
            AAction::Dismount => "A  DISMOUNT".into(),
        }
    } else {
        String::new()
    };
    for (mut v, mut t) in &mut prompt {
        *v = if text.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
        if t.0 != text {
            t.0 = text.clone();
        }
    }
}

/// Rename the round A / B touch buttons for the situation (see `touch::ButtonLabels`).
fn update_labels(character: Res<Character>, slot: Res<ExperimentSlot>, sim: Res<GameSim>, mut labels: ResMut<ButtonLabels>, mut owned: Local<bool>) {
    if !link_active(&character, &slot) {
        // Hand the buttons back (Mario's B is not a sword) unless an experiment has taken them over.
        if std::mem::take(&mut *owned) && slot.id.is_none() {
            *labels = ButtonLabels::default();
        }
        return;
    }
    *owned = true;
    let mut want = ButtonLabels::default();
    match a_action(&sim) {
        AAction::None => {}
        AAction::Mount => want.jump = Some("MOUNT".into()),
        AAction::Carrot(n) => {
            want.jump = Some(format!("CARROT x{n}"));
            if n > 0 {
                want.jump_tint = Some(Color::srgba(0.85, 0.42, 0.08, 0.9));
            }
        }
        AAction::Dismount => {
            want.jump = Some("DISMOUNT".into());
            want.punch = Some("OFF".into());
        }
    }
    // B is the weapon button; a phone has no key hints, so the button says what it swings.
    if want.punch.is_none() {
        want.punch = Some(b_label(&sim.mario.link).into());
    }
    if *labels != want {
        *labels = want;
    }
}

/// What the round B button is for Link: the blade in his hand (or the Deku Stick / hammer that replaced it).
pub fn b_label(link: &jumpman_sim::link::moves::LinkExt) -> &'static str {
    use jumpman_sim::link::combat::Weapon;
    match link.weapon() {
        Weapon::Sword(_) => "SWORD",
        Weapon::Stick => "STICK",
        Weapon::Hammer => "HAMMER",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_says_what_it_swings() {
        let mut link = jumpman_sim::link::moves::LinkExt::default();
        link.enable(Age::Adult);
        assert_eq!(b_label(&link), "SWORD");
        link.enable(Age::Child);
        assert_eq!(b_label(&link), "SWORD");
        link.inv.sword = None;
        assert_eq!(b_label(&link), "STICK");
    }

    #[test]
    fn a_action_follows_the_horse() {
        let mut sim = crate::game::runner::GameSim::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"), crate::game::world::CourseRequest::new("bob")).unwrap();
        sim.mario.link.enable(Age::Adult);
        assert_eq!(a_action(&sim), AAction::None);
        let world = sim.mario.world.clone();
        let p = sim.mario.pos;
        sim.mario.link.epona.place(&world, [p[0] + 200.0, p[1], p[2]], 0.0);
        assert_eq!(a_action(&sim), AAction::Mount);
        sim.mario.link.epona.start_mounted();
        assert_eq!(a_action(&sim), AAction::Dismount);
    }
}
