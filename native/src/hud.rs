//! Port of `HUD.ts` on `bevy_ui`: power meter, lives, coins, stars, act name, action/speed
//! readout, a toast line and an optional debug panel (Backquote).
//!
//! TODO(hud): star-ready banner, interaction prompt, dialog pages (`showDialogPage`), the
//! course timer / FPS readout and the textured SM64 power-meter pie.
use crate::game::courses;
use crate::game::runner::{GameSim, SimEvent};
use crate::input::PlayerInput;
use crate::sim_plugin::SimEventMsg;
use bevy::prelude::*;
use jumpman_sim::state::MarioEvent;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toast>().add_systems(Startup, spawn_hud).add_systems(Update, (update_hud, update_race_badge, update_debug, update_toast, toggle_debug));
    }
}

/// Show a short message (`HUD.showToast`).
#[derive(Resource, Default)]
pub struct Toast {
    pub text: String,
    pub timer: f32,
}

impl Toast {
    pub fn show(&mut self, text: impl Into<String>, seconds: f32) {
        self.text = text.into();
        self.timer = seconds;
    }
}

#[derive(Component)]
struct HealthSegment(usize);
#[derive(Component)]
struct LivesText;
#[derive(Component)]
struct CoinsText;
#[derive(Component)]
struct StarsText;
#[derive(Component)]
struct ActionText;
#[derive(Component)]
struct ToastText;
#[derive(Component)]
struct DebugText;
#[derive(Component)]
struct RaceBadge;

const SEGMENTS: usize = 9;

fn label(commands: &mut ChildSpawnerCommands, marker: impl Component, text: &str, size: f32) {
    commands.spawn((Text::new(text), TextFont { font_size: size, ..default() }, TextColor(Color::WHITE), marker));
}

fn spawn_hud(mut commands: Commands, sim: Res<GameSim>) {
    let mission = courses::missions_for(&sim.course.request.id).into_iter().find(|m| m.act == sim.course.request.act);
    let title = match mission {
        Some(m) if courses::missions_for(&sim.course.request.id).len() > 1 => format!("{} - {}", sim.course.name, m.name),
        _ => sim.course.name.clone(),
    };
    commands
        .spawn(Node { position_type: PositionType::Absolute, left: Val::Px(16.), top: Val::Px(12.), flex_direction: FlexDirection::Column, row_gap: Val::Px(6.), ..default() })
        .with_children(|p| {
            p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(3.), ..default() }).with_children(|row| {
                for i in 0..SEGMENTS {
                    row.spawn((Node { width: Val::Px(16.), height: Val::Px(16.), ..default() }, BackgroundColor(Color::srgb(0.9, 0.2, 0.2)), HealthSegment(i)));
                }
            });
            label(p, LivesText, "LIVES x 4", 20.);
            label(p, CoinsText, "COINS 0", 20.);
            label(p, StarsText, "STARS 0", 20.);
        });
    // The default font has no curly apostrophe.
    let title = title.replace('\u{2019}', "'");
    commands.spawn((
        Text::new(title),
        TextFont { font_size: 22., ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, top: Val::Px(12.), right: Val::Px(16.), ..default() },
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 16., ..default() },
        TextColor(Color::srgb(0.85, 0.95, 1.0)),
        Node { position_type: PositionType::Absolute, left: Val::Px(16.), bottom: Val::Px(12.), ..default() },
        ActionText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 26., ..default() },
        TextColor(Color::srgb(1.0, 0.9, 0.4)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(60.), left: Val::Percent(35.), ..default() },
        ToastText,
    ));
    // `#race-timer` badge (green pill under the top bar).
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 20., ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, top: Val::Px(56.), right: Val::Px(16.), padding: UiRect::axes(Val::Px(14.), Val::Px(5.)), ..default() },
        BackgroundColor(Color::srgb(0.13, 0.49, 0.18)),
        BorderRadius::all(Val::Px(12.)),
        Visibility::Hidden,
        RaceBadge,
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 13., ..default() },
        TextColor(Color::srgb(0.8, 1.0, 0.8)),
        Node { position_type: PositionType::Absolute, right: Val::Px(16.), bottom: Val::Px(12.), ..default() },
        if std::env::var("JUMPMAN_DEBUG").is_ok_and(|v| !v.is_empty() && v != "0") { Visibility::Inherited } else { Visibility::Hidden },
        DebugText,
    ));
}

#[allow(clippy::type_complexity)]
fn update_hud(
    sim: Res<GameSim>,
    mut segments: Query<(&HealthSegment, &mut BackgroundColor, &mut Visibility)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<LivesText>>,
        Query<&mut Text, With<CoinsText>>,
        Query<&mut Text, With<StarsText>>,
        Query<&mut Text, With<ActionText>>,
    )>,
) {
    let m = &sim.mario;
    let extended = m.max_health > 0x880 as f64;
    let health = ((m.health / 0x110 as f64).ceil().max(0.0) as usize).min(SEGMENTS);
    let level = if health <= 2 { Color::srgb(0.9, 0.15, 0.15) } else if health <= 4 { Color::srgb(0.95, 0.65, 0.1) } else { Color::srgb(0.25, 0.8, 0.3) };
    for (seg, mut color, mut vis) in &mut segments {
        *vis = if seg.0 == 8 && !extended { Visibility::Hidden } else { Visibility::Inherited };
        *color = BackgroundColor(if seg.0 < health { level } else { Color::srgba(0.1, 0.1, 0.1, 0.6) });
    }
    set(texts.p0().single_mut(), format!("LIVES x {}", m.lives));
    set(texts.p1().single_mut(), format!("COINS {}", sim.coins));
    set(texts.p2().single_mut(), format!("STARS {}", sim.stars));
    let name = crate::action_names::action_name(m.action).map_or_else(|| format!("0x{:x}", m.action & 0x1ff), str::to_string);
    set(texts.p3().single_mut(), format!("{name}   {:.1} u/f", m.forward_vel));
}

/// `updateRaceHud`: elapsed time of a running Koopa the Quick race (and the racing penguin's).
fn update_race_badge(sim: Res<GameSim>, mut badge: Query<(&mut Text, &mut Visibility), With<RaceBadge>>) {
    use jumpman_sim::behaviors::koopa_the_quick::{KOOPA_RACE_RESULT_DIALOG, KOOPA_RACE_RUNNING};
    use jumpman_sim::object::ObjectBehaviorKind as K;
    let Ok((mut text, mut vis)) = badge.single_mut() else { return };
    let mut label = None;
    for o in &sim.mario.objects.objects {
        if !o.active {
            continue;
        }
        let a = o.action as i32;
        if o.behavior == K::KoopaTheQuick && (KOOPA_RACE_RUNNING..=KOOPA_RACE_RESULT_DIALOG).contains(&a) {
            label = Some(format!("KOOPA RACE - {}", crate::experiments::race::format_time(o.var("elapsed") / 30.0)));
        } else if o.behavior == K::RacingPenguin && a == 3 {
            label = Some(format!("PENGUIN RACE - {}", crate::experiments::race::format_time(o.action_timer / 30.0)));
        }
    }
    *vis = if label.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if let Some(l) = label {
        if text.0 != l {
            text.0 = l;
        }
    }
}

fn update_debug(sim: Res<GameSim>, mut debug: Query<(&mut Text, &Visibility), With<DebugText>>) {
    if let Ok((mut text, vis)) = debug.single_mut() {
        if *vis != Visibility::Hidden {
            text.0 = debug_text(&sim);
        }
    }
}

fn set(text: Result<Mut<Text>, impl std::fmt::Debug>, value: String) {
    if let Ok(mut t) = text {
        if t.0 != value {
            t.0 = value;
        }
    }
}

fn debug_text(sim: &GameSim) -> String {
    let m = &sim.mario;
    format!(
        "action 0x{:x}\npos {:.0} {:.0} {:.0}\nvel {:.1} {:.1} {:.1}\nforward {:.2}  yaw {}\nfloor h={:.1}\nhealth 0x{:x}\nobjects {}  pickups {}\nclip 0x{:x} frame {}\ntick {}",
        m.action, m.pos[0], m.pos[1], m.pos[2], m.vel[0], m.vel[1], m.vel[2], m.forward_vel, m.face_angle[1], m.floor_height, m.health as u32,
        m.objects.len(), sim.pickups.len(), sim.pose.clip, sim.pose.frame, sim.ticks
    )
}

fn toggle_debug(mut input: ResMut<PlayerInput>, mut debug: Query<&mut Visibility, With<DebugText>>) {
    if input.toggle_debug {
        input.toggle_debug = false;
        for mut v in &mut debug {
            *v = if *v == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn update_toast(time: Res<Time>, mut toast: ResMut<Toast>, mut events: EventReader<SimEventMsg>, mut text: Single<&mut Text, With<ToastText>>, sim: Res<GameSim>) {
    for SimEventMsg(e) in events.read() {
        match e {
            SimEvent::CoinCollected { .. } => toast.show(format!("COIN {}", sim.coins), 0.7),
            SimEvent::Mario(MarioEvent::Death) => toast.show("OUCH", 1.5),
            _ => {}
        }
    }
    toast.timer = (toast.timer - time.delta_secs()).max(0.0);
    let shown = if toast.timer > 0.0 { toast.text.as_str() } else { "" };
    if text.0 != shown {
        text.0 = shown.to_string();
    }
}
