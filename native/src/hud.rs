//! Port of `HUD.ts` on `bevy_ui`: power meter, lives, coins, stars, act name, action/speed
//! readout, a toast line and an optional debug panel (Backquote).
//!
//! TODO(hud): star-ready banner, interaction prompt, dialog pages (`showDialogPage`), the
//! course timer / FPS readout and the textured SM64 power-meter pie.
use crate::game::courses;
use crate::game::runner::{GameSim, SimEvent};
use crate::input::PlayerInput;
use crate::sim_plugin::SimEventMsg;
use crate::safe_area::SafeArea;
use crate::shell::retro::hud_kit::{HudKit, Icon, PixelText, Plate, COL_GOLD, COL_PALE, COL_WHITE};
use bevy::prelude::*;
use jumpman_sim::state::MarioEvent;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        crate::shell::retro::hud_kit::ensure(app);
        app.init_resource::<Toast>().init_resource::<HudMinimal>().add_systems(Startup, spawn_hud.in_set(crate::progress::PersistentSpawn)).add_systems(Update, (update_hud, punch_counters.after(update_hud), place_hud, update_race_badge, update_debug, update_toast, toggle_debug));
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

/// Modes with their own HUD (Star Swarm) set this to hide Mario's power pips, lives, coins, stars,
/// the course title and the action readout.
#[derive(Resource, Default)]
pub struct HudMinimal(pub bool);

#[derive(Component)]
struct CourseTitle;

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
struct ToastRow;
#[derive(Component)]
struct DebugText;
#[derive(Component)]
struct RaceBadge;

const SEGMENTS: usize = 9;

/// A counter that bumps when its number changes (coins, stars, lives).
#[derive(Component)]
struct Punch {
    last: Option<i64>,
    /// Seconds since the last change.
    age: f32,
    /// Lost something (a life): shake sideways instead of swelling.
    loss: bool,
}

impl Default for Punch {
    fn default() -> Self {
        Self { last: None, age: PUNCH_SECONDS, loss: false }
    }
}

pub const PUNCH_SECONDS: f32 = 0.3;

/// Scale of a counter `age` seconds after its number went up: snaps to 1.3x, then settles with one small overshoot.
pub fn punch_scale(age: f32) -> f32 {
    if !(0.0..PUNCH_SECONDS).contains(&age) {
        return 1.0;
    }
    let k = age / PUNCH_SECONDS;
    1.0 + 0.3 * (1.0 - k).powi(2) * (k * std::f32::consts::PI * 1.5).cos()
}

/// Sideways offset (px) of a counter `age` seconds after its number went down.
pub fn loss_shake(age: f32) -> f32 {
    if !(0.0..PUNCH_SECONDS).contains(&age) {
        return 0.0;
    }
    let k = age / PUNCH_SECONDS;
    7.0 * (1.0 - k) * (k * std::f32::consts::TAU * 3.0).sin()
}

/// Power-meter feedback: seconds left of the flash after the health changed, and what it was before.
#[derive(Default)]
struct HealthFlash {
    last: Option<usize>,
    from: usize,
    timer: f32,
}

const FLASH_SECONDS: f32 = 0.4;

fn mix(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.to_srgba(), b.to_srgba());
    let t = t.clamp(0.0, 1.0);
    Color::srgba(a.red + (b.red - a.red) * t, a.green + (b.green - a.green) * t, a.blue + (b.blue - a.blue) * t, a.alpha + (b.alpha - a.alpha) * t)
}

/// Colour of power-meter segment `i` with `health` lit segments, `flash` (1 -> 0) after a change from `from` lit segments
/// and `pulse` (0..1) the low-health throb.
fn segment_color(i: usize, health: usize, from: usize, flash: f32, pulse: f32) -> Color {
    let level = if health <= 2 { Color::srgb(0.9, 0.15, 0.15) } else if health <= 4 { Color::srgb(0.95, 0.65, 0.1) } else { Color::srgb(0.25, 0.8, 0.3) };
    let dark = Color::srgba(0.1, 0.1, 0.1, 0.6);
    if flash <= 0.0 && (health > 2 || i >= health) {
        return if i < health { level } else { dark };
    }
    if i < health {
        // Gained segments flash white; at two or fewer the meter throbs.
        let base = if health <= 2 { mix(level, Color::srgb(1.0, 0.55, 0.5), pulse) } else { level };
        if i >= from { mix(base, Color::WHITE, flash) } else if from > health { mix(base, Color::WHITE, flash * 0.6) } else { base }
    } else if i < from {
        // Lost segments burn out from white-red to dark.
        mix(dark, Color::srgb(1.0, 0.5, 0.45), flash)
    } else {
        dark
    }
}

/// The whole top-left panel (power meter and counters).
#[derive(Component)]
struct MarioPanel;
/// The number inside a counter (the counter row itself carries [`LivesText`] / [`CoinsText`] / [`StarsText`] and the punch).
#[derive(Component)]
struct CounterValue;
#[derive(Component)]
struct RaceLabel;
#[derive(Component)]
struct ActionChip;

/// Distance of the HUD from the screen edge (layout px), pushed in by the safe area so the notch never covers it.
const EDGE_X: f32 = 12.0;
const EDGE_Y: f32 = 10.0;

/// Where a HUD element anchored `base` px from an edge sits given that edge's safe-area inset.
pub fn inset_edge(base: f32, inset: f32) -> f32 {
    base.max(inset + base * 0.5)
}

/// One counter: icon and number in a row; the row gets the punch animation.
fn counter_row(p: &mut ChildSpawnerCommands, kit: &mut HudKit, icon: Icon, text: &str, marker: impl Component) {
    let (img, inode) = kit.icon(icon);
    let value = kit.text(text, COL_WHITE);
    p.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: kit.px(2), ..default() }, UiTransform::IDENTITY, marker, Punch::default())).with_children(|r| {
        r.spawn((img, inode));
        r.spawn((value, CounterValue));
    });
}

fn spawn_hud(mut commands: Commands, sim: Res<GameSim>, mut kit: HudKit) {
    let mission = courses::missions_for(&sim.course.request.id).into_iter().find(|m| m.act == sim.course.request.act);
    let title = match mission {
        Some(m) if courses::missions_for(&sim.course.request.id).len() > 1 => format!("{} - {}", sim.course.name, m.name),
        _ => sim.course.name.clone(),
    };
    // Power meter above the counters, all in one navy plate (the menus' dialog frame).
    let (mut node, frame) = kit.chip(Plate::Navy);
    node.position_type = PositionType::Absolute;
    node.left = Val::Px(EDGE_X);
    node.top = Val::Px(EDGE_Y);
    node.flex_direction = FlexDirection::Column;
    node.align_items = AlignItems::Start;
    node.row_gap = kit.px(3);
    let seg = (kit.px(6), kit.px(7), kit.px(1));
    let gap = kit.px(1);
    let counter_gap = kit.px(6);
    commands.spawn((node, frame, MarioPanel)).with_children(|p| {
        p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: gap, padding: UiRect::all(gap), ..default() }).with_children(|row| {
            for i in 0..SEGMENTS {
                row.spawn((Node { width: seg.0, height: seg.1, border: UiRect::all(seg.2), ..default() }, BackgroundColor(Color::srgb(0.9, 0.2, 0.2)), BorderColor::all(Color::srgb_u8(4, 7, 32)), HealthSegment(i)));
            }
        });
        p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: counter_gap, ..default() }).with_children(|row| {
            counter_row(row, &mut kit, Icon::Life, "x 4", LivesText);
            counter_row(row, &mut kit, Icon::Coin, "x 0", CoinsText);
            counter_row(row, &mut kit, Icon::Star, "x 0", StarsText);
        });
    });
    let (mut node, frame) = kit.chip(Plate::Navy);
    node.position_type = PositionType::Absolute;
    node.top = Val::Px(EDGE_Y);
    node.right = Val::Px(EDGE_X);
    let t = kit.text(title, COL_GOLD);
    commands.spawn((node, frame, CourseTitle, crate::hints::ControlHint)).with_child(t);
    let (mut node, frame) = kit.chip(Plate::Dark);
    node.position_type = PositionType::Absolute;
    node.left = Val::Px(EDGE_X);
    node.bottom = Val::Px(EDGE_Y);
    let t = kit.text("", COL_PALE);
    commands.spawn((node, frame, ActionChip, crate::hints::ControlHint)).with_child((t, ActionText));
    // A full-width row that centres the toast line.
    let toast = kit.text_big("", COL_GOLD, 2);
    commands
        .spawn((Node { position_type: PositionType::Absolute, bottom: Val::Px(TOAST_BOTTOM), left: Val::Px(0.), right: Val::Px(0.), justify_content: JustifyContent::Center, ..default() }, ToastRow, bevy::ui::FocusPolicy::Pass))
        .with_child((toast, ToastText, UiTransform::IDENTITY));
    // `#race-timer` badge (green slab under the course title).
    let (mut node, frame) = kit.chip(Plate::Green);
    node.position_type = PositionType::Absolute;
    node.top = Val::Px(EDGE_Y + 20.0 * kit.k() as f32);
    node.right = Val::Px(EDGE_X);
    let t = kit.text("", COL_WHITE);
    commands.spawn((node, frame, Visibility::Hidden, RaceBadge)).with_child((t, RaceLabel));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: FontSize::Px(13.), ..default() },
        TextColor(Color::srgb(0.8, 1.0, 0.8)),
        Node { position_type: PositionType::Absolute, right: Val::Px(16.), bottom: Val::Px(12.), ..default() },
        if std::env::var("JUMPMAN_DEBUG").is_ok_and(|v| !v.is_empty() && v != "0") { Visibility::Inherited } else { Visibility::Hidden },
        DebugText,
    ));
}

/// Keep the panels clear of the notch / home indicator.
#[allow(clippy::type_complexity)]
fn place_hud(
    area: Res<SafeArea>,
    scale: Res<crate::shell::retro::hud_kit::HudScale>,
    mut panel: Query<&mut Node, With<MarioPanel>>,
    mut title: Query<&mut Node, (With<CourseTitle>, Without<MarioPanel>)>,
    mut badge: Query<&mut Node, (With<RaceBadge>, Without<MarioPanel>, Without<CourseTitle>)>,
    mut action: Query<&mut Node, (With<ActionChip>, Without<MarioPanel>, Without<CourseTitle>, Without<RaceBadge>)>,
) {
    if !area.is_changed() {
        return;
    }
    for mut n in &mut panel {
        n.left = Val::Px(inset_edge(EDGE_X, area.left));
        n.top = Val::Px(inset_edge(EDGE_Y, area.top));
    }
    for mut n in &mut title {
        n.right = Val::Px(inset_edge(EDGE_X, area.right));
        n.top = Val::Px(inset_edge(EDGE_Y, area.top));
    }
    for mut n in &mut badge {
        n.right = Val::Px(inset_edge(EDGE_X, area.right));
        n.top = Val::Px(inset_edge(EDGE_Y, area.top) + 20.0 * scale.0 as f32);
    }
    for mut n in &mut action {
        n.left = Val::Px(inset_edge(EDGE_X, area.left));
        n.bottom = Val::Px(inset_edge(EDGE_Y, area.bottom));
    }
}

/// Whether Mario's own HUD (power meter, lives, coins, stars, action line, course title) stays off screen: when a
/// mode asked for it (`minimal`), in the experiments that draw their own, and for every character but Mario
/// (Link and Banjo each bring a HUD of their own).
pub fn mario_hud_hidden(character: crate::characters::Character, minimal: bool, experiment: Option<crate::experiments::ExperimentId>) -> bool {
    character != crate::characters::Character::Mario || mode_hud_hidden(minimal, experiment)
}

/// Whether a running mode wants the course HUDs (Mario's, Banjo's) off the screen because it draws its own.
pub fn mode_hud_hidden(minimal: bool, experiment: Option<crate::experiments::ExperimentId>) -> bool {
    minimal || matches!(experiment, Some(crate::experiments::ExperimentId::Kart | crate::experiments::ExperimentId::Deck))
}

#[allow(clippy::type_complexity)]
fn update_hud(
    sim: Res<GameSim>,
    mut segments: Query<(&HealthSegment, &mut BackgroundColor, &mut Node)>,
    mut texts: ParamSet<(Query<(&mut PixelText, &ChildOf), With<CounterValue>>, Query<&mut PixelText, With<ActionText>>)>,
    counters: Query<(Has<LivesText>, Has<CoinsText>, Has<StarsText>)>,
    minimal: Res<HudMinimal>,
    experiment: Res<crate::experiments::ExperimentSlot>,
    mut chrome: Query<&mut Visibility, Or<(With<MarioPanel>, With<ActionChip>, With<CourseTitle>)>>,
    mut was_minimal: Local<bool>,
    mut flash: Local<HealthFlash>,
    time: Res<Time>,
    character: Res<crate::characters::Character>,
) {
    let minimal = mario_hud_hidden(*character, minimal.0, experiment.id);
    if minimal != *was_minimal {
        *was_minimal = minimal;
        for mut v in &mut chrome {
            *v = if minimal { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
    let m = &sim.mario;
    let extended = m.max_health > 0x880 as f64;
    let health = ((m.health / 0x110 as f64).ceil().max(0.0) as usize).min(SEGMENTS);
    if flash.last.is_some_and(|l| l != health) {
        flash.from = flash.last.unwrap_or(health);
        flash.timer = FLASH_SECONDS;
    }
    flash.last = Some(health);
    flash.timer = (flash.timer - time.delta_secs()).max(0.0);
    if flash.timer == 0.0 {
        flash.from = health;
    }
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 9.0).sin();
    for (seg, mut color, mut node) in &mut segments {
        let want = if seg.0 == 8 && !extended { Display::None } else { Display::Flex };
        if node.display != want {
            node.display = want;
        }
        let c = segment_color(seg.0, health, flash.from, flash.timer / FLASH_SECONDS, pulse);
        if color.0 != c {
            *color = BackgroundColor(c);
        }
    }
    for (mut t, parent) in &mut texts.p0() {
        let Ok((lives, coins, stars)) = counters.get(parent.parent()) else { continue };
        let value = if lives {
            format!("x {}", m.lives)
        } else if coins {
            format!("x {}", sim.coins)
        } else if stars {
            // `render_hud_stars`: no multiply sign from 100 stars on.
            if sim.stars < 100 { format!("x {}", sim.stars) } else { sim.stars.to_string() }
        } else {
            continue;
        };
        PixelText::set(&mut t, value);
    }
    let name = crate::action_names::action_name(m.action).map_or_else(|| format!("0x{:x}", m.action & 0x1ff), str::to_string);
    if let Ok(mut t) = texts.p1().single_mut() {
        PixelText::set(&mut t, format!("{name}   {:.1} u/f", m.forward_vel));
    }
}

/// Bump the lives / coins / stars counters when their number changes.
fn punch_counters(sim: Res<GameSim>, time: Res<Time>, mut counters: Query<(&mut Punch, &mut UiTransform, Has<CoinsText>, Has<StarsText>)>) {
    for (mut punch, mut xf, coins, stars) in &mut counters {
        let value = if coins { sim.coins as i64 } else if stars { sim.stars as i64 } else { sim.mario.lives as i64 };
        if let Some(last) = punch.last.filter(|l| *l != value) {
            // A counter reset (course restart) is not a loss worth shaking for.
            if value > last || !coins {
                punch.age = 0.0;
                punch.loss = value < last;
            }
        }
        punch.last = Some(value);
        if punch.age >= PUNCH_SECONDS {
            continue;
        }
        punch.age += time.delta_secs();
        let (scale, dx) = if punch.loss { (1.0, loss_shake(punch.age)) } else { (punch_scale(punch.age), 0.0) };
        // The labels are left-aligned: push right by half the growth so the glyph stays on its margin.
        *xf = UiTransform { scale: Vec2::splat(scale), translation: Val2::px(dx, 0.0), ..UiTransform::IDENTITY };
    }
}

/// `updateRaceHud`: elapsed time of a running Koopa the Quick race (and the racing penguin's).
fn update_race_badge(sim: Res<GameSim>, mut badge: Query<&mut Visibility, With<RaceBadge>>, mut text: Query<&mut PixelText, With<RaceLabel>>) {
    use jumpman_sim::behaviors::koopa_the_quick::{KOOPA_RACE_RESULT_DIALOG, KOOPA_RACE_RUNNING};
    use jumpman_sim::object::ObjectBehaviorKind as K;
    let (Ok(mut vis), Ok(mut text)) = (badge.single_mut(), text.single_mut()) else { return };
    let mut label = None;
    for o in &sim.mario.objects.objects {
        if !o.active {
            continue;
        }
        let a = o.action as i32;
        if o.behavior == K::KoopaTheQuick && (KOOPA_RACE_RUNNING..=KOOPA_RACE_RESULT_DIALOG).contains(&a) {
            label = Some(format!("KOOPA RACE - {}", crate::experiments::race::format_time(o.var("elapsed") / 30.0)));
        } else if o.behavior == K::PssTimer && o.var("shown") != 0.0 {
            // Princess's Secret Slide: the HUD timer of `pss_begin_slide` (stays shown after the end line).
            let t = if o.var("running") != 0.0 { o.var("elapsed") } else { o.var("finalTime") };
            label = Some(format!("SLIDE - {}", crate::experiments::race::format_time(t / 30.0)));
        } else if o.behavior == K::RacingPenguin && a == 3 {
            label = Some(format!("PENGUIN RACE - {}", crate::experiments::race::format_time(o.action_timer / 30.0)));
        }
    }
    *vis = if label.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if let Some(l) = label {
        PixelText::set(&mut text, l);
    }
}

fn update_debug(sim: Res<GameSim>, progress: Res<crate::progress::flow::Progress>, mut debug: Query<(&mut Text, &Visibility), With<DebugText>>, mut last_logged: Local<u64>, mut last_flow: Local<String>) {
    if std::env::var("JUMPMAN_DEBUG_LOG").is_ok() {
        let name: String = format!("{:?}", progress.flow).chars().take_while(|c| c.is_alphanumeric()).collect();
        if *last_flow != name {
            info!("FLOW {} -> {} (tick {})", *last_flow, name, sim.ticks);
            *last_flow = name;
        }
    }
    // `JUMPMAN_DEBUG_LOG=<every n ticks>` prints Mario's state (headless motion checks).
    if let Some(n) = std::env::var("JUMPMAN_DEBUG_LOG").ok().and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0) {
        if sim.ticks / n != *last_logged {
            *last_logged = sim.ticks / n;
            let m = &sim.mario;
            let flow = format!("{:?}", progress.flow);
            info!("MARIO tick={} pos=({:.0},{:.0},{:.0}) floor={:.0} action=0x{:x} flow={}", sim.ticks, m.pos[0], m.pos[1], m.pos[2], m.floor_height, m.action, flow.chars().take(24).collect::<String>());
        }
    }
    if let Ok((mut text, vis)) = debug.single_mut() {
        if *vis != Visibility::Hidden {
            text.0 = debug_text(&sim);
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

/// Toast height above the bottom edge; an experiment mode has its status lines and dock down there, so it sits higher.
const TOAST_BOTTOM: f32 = 60.0;
const TOAST_BOTTOM_EXPERIMENT: f32 = 170.0;

/// Scale of the toast line `age` seconds after it appeared with `left` seconds to go: pops in, shrinks out.
pub fn toast_scale(age: f32, left: f32) -> f32 {
    let pop = if age < 0.1 { 0.6 + 0.55 * age / 0.1 } else if age < 0.2 { 1.15 - 0.15 * (age - 0.1) / 0.1 } else { 1.0 };
    pop * (left / 0.12).clamp(0.0, 1.0)
}

fn update_toast(time: Res<Time>, mut toast: ResMut<Toast>, mut events: MessageReader<SimEventMsg>, text: Single<(&mut PixelText, &mut UiTransform), With<ToastText>>, mut node: Single<&mut Node, With<ToastRow>>, sim: Res<GameSim>, slot: Res<crate::experiments::ExperimentSlot>, mut age: Local<f32>) {
    let (mut text, mut xf) = text.into_inner();
    let bottom = Val::Px(if slot.id.is_some() { TOAST_BOTTOM_EXPERIMENT } else { TOAST_BOTTOM });
    if node.bottom != bottom {
        node.bottom = bottom;
    }
    for SimEventMsg(e) in events.read() {
        match e {
            SimEvent::CoinCollected { .. } if crate::hints::setting() => toast.show(format!("COIN {}", sim.coins), 0.7),
            SimEvent::Mario(MarioEvent::Death) if crate::hints::setting() => toast.show("OUCH", 1.5),
            _ => {}
        }
    }
    toast.timer = (toast.timer - time.delta_secs()).max(0.0);
    let shown = if toast.timer > 0.0 { toast.text.as_str() } else { "" };
    if text.text != shown {
        PixelText::set(&mut text, shown);
        *age = 0.0;
    }
    *age += time.delta_secs();
    let scale = if shown.is_empty() { 1.0 } else { toast_scale(*age, toast.timer) };
    if xf.scale.x != scale {
        xf.scale = Vec2::splat(scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_clears_the_safe_area() {
        // No inset: the plain margin. A notch pushes the panel in past it, never out.
        assert_eq!(inset_edge(12.0, 0.0), 12.0);
        assert!(inset_edge(12.0, 59.0) >= 59.0 + 6.0, "clear of the notch with some air");
        assert!(inset_edge(10.0, 21.0) >= 21.0, "clear of the home indicator");
        for inset in [0.0, 20.0, 47.0, 59.0, 100.0] {
            assert!(inset_edge(12.0, inset) >= 12.0 && inset_edge(12.0, inset) >= inset);
        }
    }

    #[test]
    fn the_mario_panel_is_built_from_the_kit_and_follows_the_safe_area() {
        use crate::shell::retro::hud_kit::{HudScale, PixelText};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).insert_resource(SafeArea { left: 59.0, right: 59.0, bottom: 21.0, top: 0.0 });
        crate::shell::retro::hud_kit::test_resources(&mut app);
        app.add_systems(Update, place_hud);
        let world = app.world_mut();
        let k = world.resource::<HudScale>().0;
        let panel = world.spawn((Node { left: Val::Px(EDGE_X), top: Val::Px(EDGE_Y), ..default() }, MarioPanel)).id();
        let title = world.spawn((Node::default(), CourseTitle)).id();
        let badge = world.spawn((Node::default(), RaceBadge)).id();
        let action = world.spawn((Node::default(), ActionChip)).id();
        let _ = PixelText::new("", [0; 4]);
        app.update();
        let n = |e| app.world().get::<Node>(e).unwrap().clone();
        assert_eq!(n(panel).left, Val::Px(inset_edge(EDGE_X, 59.0)));
        assert_eq!(n(title).right, Val::Px(inset_edge(EDGE_X, 59.0)));
        assert_eq!(n(action).bottom, Val::Px(inset_edge(EDGE_Y, 21.0)));
        // The race badge sits one chip height under the title.
        assert_eq!(n(badge).top, Val::Px(inset_edge(EDGE_Y, 0.0) + 20.0 * k as f32));
        assert!(n(panel).left != Val::Px(EDGE_X), "moved in from the plain margin");
    }

    #[test]
    fn mario_hud_shows_for_mario_only() {
        use crate::characters::Character;
        use crate::experiments::ExperimentId;
        assert!(!mario_hud_hidden(Character::Mario, false, None));
        for c in [Character::Banjo, Character::Link, Character::YoungLink] {
            assert!(mario_hud_hidden(c, false, None), "{c:?}");
        }
        assert!(mario_hud_hidden(Character::Mario, true, None));
        assert!(mario_hud_hidden(Character::Mario, false, Some(ExperimentId::Kart)));
        assert!(mario_hud_hidden(Character::Mario, false, Some(ExperimentId::Deck)));
        assert!(mode_hud_hidden(true, None) && mode_hud_hidden(false, Some(ExperimentId::Deck)) && !mode_hud_hidden(false, None));
    }

    #[test]
    fn counter_punch_snaps_up_and_settles_at_one() {
        assert!((punch_scale(0.0) - 1.3).abs() < 1e-6);
        assert_eq!(punch_scale(PUNCH_SECONDS), 1.0);
        assert_eq!(punch_scale(-1.0), 1.0);
        let mut k = 0.0;
        while k < PUNCH_SECONDS {
            let s = punch_scale(k);
            assert!((0.9..=1.4001).contains(&s), "{k}: {s}");
            k += 0.01;
        }
        assert!((punch_scale(PUNCH_SECONDS - 0.001) - 1.0).abs() < 0.01, "no pop at the end");
        assert_eq!(loss_shake(0.0), 0.0);
        assert!(loss_shake(PUNCH_SECONDS - 0.001).abs() < 0.1 && loss_shake(PUNCH_SECONDS) == 0.0);
    }

    #[test]
    fn power_meter_flashes_lost_and_gained_segments() {
        let dark = Color::srgba(0.1, 0.1, 0.1, 0.6);
        // Settled: lit segments carry the level colour, the rest are dark.
        assert_eq!(segment_color(0, 8, 8, 0.0, 0.0), Color::srgb(0.25, 0.8, 0.3));
        assert_eq!(segment_color(7, 5, 5, 0.0, 0.0), dark);
        // Just lost three: the lost ones glow, then end dark.
        assert_ne!(segment_color(6, 5, 8, 1.0, 0.0), dark);
        assert_eq!(segment_color(6, 5, 8, 0.0, 0.0), dark);
        // Just healed: the new segment starts white.
        let healed = segment_color(5, 6, 5, 1.0, 0.0).to_srgba();
        assert!(healed.red > 0.999 && healed.green > 0.999 && healed.blue > 0.999, "{healed:?}");
        // Low health throbs.
        assert_ne!(segment_color(0, 2, 2, 0.0, 0.0), segment_color(0, 2, 2, 0.0, 1.0));
        assert_eq!(segment_color(0, 3, 3, 0.0, 0.0), segment_color(0, 3, 3, 0.0, 1.0));
    }

    #[test]
    fn toast_pops_in_and_shrinks_out() {
        assert!((toast_scale(0.0, 2.0) - 0.6).abs() < 1e-6);
        assert!(toast_scale(0.1, 2.0) > 1.1);
        assert_eq!(toast_scale(0.5, 2.0), 1.0);
        assert!(toast_scale(1.0, 0.03) < 0.3);
    }
}
