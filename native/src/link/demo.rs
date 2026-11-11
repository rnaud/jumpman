//! Scripted input for headless checks (`JUMPMAN_LINK_DEMO`), like `JUMPMAN_WALK`:
//! `JUMPMAN_LINK_DEMO="30:lock,40:stick=up,60:a,90:c1:20,120:shot=bow"`, entries `tick:what[:hold]`
//! (sim ticks since the course started). `what` is `a` / `b` / `z` (button, held `hold` ticks), `c1` `c2` `c3` `c4`
//! (C-left, down, right, up), `stick=up|down|left|right|upleft|upright|none`, `lock` (toggle Z-target) or
//! `shot=name` (a PNG into `JUMPMAN_LINK_SHOTS`, default the current directory).
use super::{LinkInput, LockOn};
use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::sim_plugin::PreTickSet;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use jumpman_sim::link::items::{CItem, Item};
use jumpman_sim::link::moves::{C_DOWN, C_LEFT, C_RIGHT, C_UP};
use jumpman_sim::state::{A_BUTTON, B_BUTTON, Z_TRIG};

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Button(u32, u32),
    C(u32, u32),
    Stick(f32, f32),
    Lock,
    Shot(String),
    /// Toggle the item screen (`menu`), or open it on a tab (`menu=2`).
    Menu(Option<usize>),
    /// A finger tap through the real touch layer (`TouchInput` messages): `tap=c1` (c1..c4, items, a, b, z, l, pause), an item-screen button
    /// (`tap=cell3`, `tab2`, `assign1`, `close`, `backdrop`) or a position (`tap=120.300`), held `hold` ticks (default 3).
    Tap(String, u32),
    /// `set1=bomb`: put an item on C-left (`set1`, `set2`, `set3`) or give it first (`give=ocarina`), as the loadout hook does.
    Set(usize, Item),
    /// A finger that stays down and drags: `drag=x0.y0.x1.y1`, `hold` ticks.
    Drag([f32; 4], u32),
    /// `tp=x/y/z/yaw_degrees`: put Link there (SM64 units, yaw from +z towards +x), standing, like `JUMPMAN_TELEPORT` but at a tick
    /// of the script: a headless capture can start its fight after the first (slow) seconds of loading.
    Teleport([f64; 4]),
}

pub fn parse(script: &str) -> Vec<(u64, Step)> {
    let mut out = vec![];
    for entry in script.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let mut parts = entry.splitn(3, ':');
        let (Some(t), Some(what)) = (parts.next().and_then(|t| t.parse::<u64>().ok()), parts.next()) else { continue };
        let hold_arg = parts.next().and_then(|h| h.parse::<u32>().ok());
        let (hold, has_hold) = (hold_arg.unwrap_or(1), hold_arg.is_some());
        let step = match what {
            "a" => Step::Button(A_BUTTON, hold),
            "b" => Step::Button(B_BUTTON, hold),
            "z" => Step::Button(Z_TRIG, hold),
            "c1" => Step::C(C_LEFT, hold),
            "c2" => Step::C(C_DOWN, hold),
            "c3" => Step::C(C_RIGHT, hold),
            "c4" => Step::C(C_UP, hold),
            "lock" => Step::Lock,
            "stick=up" => Step::Stick(0.0, 64.0),
            "stick=down" => Step::Stick(0.0, -64.0),
            "stick=left" => Step::Stick(-64.0, 0.0),
            "stick=right" => Step::Stick(64.0, 0.0),
            "stick=upleft" => Step::Stick(-45.0, 45.0),
            "stick=upright" => Step::Stick(45.0, 45.0),
            "stick=none" => Step::Stick(0.0, 0.0),
            "menu" => Step::Menu(None),
            s if s.starts_with("menu=") => Step::Menu(s[5..].parse().ok()),
            s if s.starts_with("set") && s.len() > 5 && s.as_bytes()[4] == b'=' => match (s[3..4].parse::<usize>(), parse_loadout(&s[5..]).first()) {
                (Ok(slot @ 1..=3), Some(item)) => Step::Set(slot - 1, *item),
                _ => continue,
            },
            s if s.starts_with("tap=") => Step::Tap(s[4..].to_string(), if has_hold { hold } else { 3 }),
            s if s.starts_with("drag=") => {
                let v: Vec<f32> = s[5..].split('.').filter_map(|n| n.parse().ok()).collect();
                if v.len() != 4 {
                    continue;
                }
                Step::Drag([v[0], v[1], v[2], v[3]], if has_hold { hold } else { 10 })
            }
            s if s.starts_with("shot=") => Step::Shot(s[5..].to_string()),
            s if s.starts_with("tp=") => {
                let v: Vec<f64> = s[3..].split('/').filter_map(|n| n.parse().ok()).collect();
                if v.len() < 3 {
                    continue;
                }
                Step::Teleport([v[0], v[1], v[2], v.get(3).copied().unwrap_or(0.0)])
            }
            _ => continue,
        };
        out.push((t, step));
    }
    out
}

#[derive(Resource, Default)]
struct Demo {
    steps: Vec<(u64, Step)>,
    stick: (f32, f32),
    held: Vec<(u32, u32, bool)>,
    done: usize,
    /// Scripted fingers: (id, from, to, ticks left, ticks in all).
    fingers: Vec<(u64, Vec2, Vec2, u32, u32)>,
    next_finger: u64,
    /// Rendered frames left before the lockstep sim runs on (`JUMPMAN_LINK_LOCKSTEP`).
    hold_frames: u32,
}

/// Lets the paused lockstep sim run on once the still of its tick has had its frames.
fn lockstep_release(mut demo: ResMut<Demo>, mut vtime: ResMut<Time<Virtual>>) {
    if demo.hold_frames > 0 {
        demo.hold_frames -= 1;
        if demo.hold_frames == 0 {
            vtime.unpause();
        }
    }
}

/// Where a scripted tap lands: a touch control, a C button, an item-screen button or a literal `x.y`.
fn resolve_tap(name: &str, size: Vec2, area: &crate::safe_area::SafeArea, menu: &super::ui::MenuRects) -> Option<Vec2> {
    use crate::bindings::Action;
    use crate::touch::{layout, Control};
    let inset = Vec3::new(area.left, area.right, area.bottom);
    let c = super::ui::button_rects(size, inset);
    let idx = ["c1", "c2", "c3", "c4", "items"].iter().position(|n| *n == name);
    if let Some(i) = idx {
        return Some(c[i].1.center());
    }
    let ctl = match name {
        "a" => Some(Control::Button(Action::Jump)),
        "b" => Some(Control::Button(Action::Punch)),
        "z" => Some(Control::Button(Action::Crouch)),
        "l" => Some(Control::Button(Action::Focus)),
        "stick" => Some(Control::Stick),
        _ => None,
    };
    if let Some(ctl) = ctl {
        return layout(size, inset).into_iter().find(|(k, _)| *k == ctl).map(|(_, r)| r.center());
    }
    if name == "pause" {
        return Some(crate::touch::pause_rect(size, area.right, area.top).center());
    }
    if let Some((p, _)) = menu.0.get(name) {
        return Some(*p);
    }
    let v: Vec<f32> = name.split('.').filter_map(|n| n.parse().ok()).collect();
    (v.len() == 2).then(|| Vec2::new(v[0], v[1]))
}

/// `JUMPMAN_LINK_C="firearrow,dins,stick"`: the items on C-left, C-down, C-right (names: bow firearrow icearrow lightarrow slingshot
/// stick nut bomb bombchu hookshot boomerang hammer dins farore nayru lens bean ocarina), set once Link is the active character.
pub fn parse_loadout(spec: &str) -> Vec<Item> {
    use Item::*;
    spec.split(',')
        .filter_map(|n| {
            Some(match n.trim() {
                "bow" => Bow,
                "firearrow" => FireArrow,
                "icearrow" => IceArrow,
                "lightarrow" => LightArrow,
                "slingshot" => Slingshot,
                "stick" => DekuStick,
                "nut" => DekuNut,
                "bomb" => Bomb,
                "bombchu" => Bombchu,
                "hookshot" => Hookshot,
                "boomerang" => Boomerang,
                "hammer" => MegatonHammer,
                "dins" => DinsFire,
                "farore" => FaroresWind,
                "nayru" => NayrusLove,
                "lens" => LensOfTruth,
                "bean" => MagicBean,
                "ocarina" => Ocarina,
                _ => return None,
            })
        })
        .take(3)
        .collect()
}

fn set_loadout(mut sim: ResMut<GameSim>, mut done: Local<bool>) {
    if *done || !sim.mario.link.enabled {
        return;
    }
    *done = true;
    let Ok(spec) = std::env::var("JUMPMAN_LINK_C") else { return };
    let age = sim.mario.link.age;
    for (slot, item) in parse_loadout(&spec).into_iter().enumerate() {
        sim.mario.link.inv.set_c(slot, Some(CItem::Item(item)), age);
    }
}

pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("JUMPMAN_LINK_C").is_some() {
            app.add_systems(FixedUpdate, set_loadout.in_set(PreTickSet).before(super::pre_tick));
        }
        let Ok(script) = std::env::var("JUMPMAN_LINK_DEMO") else { return };
        app.insert_resource(Demo { steps: parse(&script), ..default() }).add_systems(FixedUpdate, run_demo.in_set(PreTickSet).before(super::pre_tick)).add_systems(Update, lockstep_release);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_demo(mut touch: MessageWriter<TouchInput>, windows: Query<(Entity, &Window), With<PrimaryWindow>>, area: Res<crate::safe_area::SafeArea>, menu_rects: Res<super::ui::MenuRects>, mut demo: ResMut<Demo>, mut menu: ResMut<super::ui::LinkMenu>, mut sim: ResMut<GameSim>, mut input: ResMut<PlayerInput>, mut li: ResMut<LinkInput>, mut lock: ResMut<LockOn>, mut commands: Commands, cams: Query<&bevy::camera::RenderTarget, With<crate::camera::MainCamera>>, mut vtime: ResMut<Time<Virtual>>) {
    let now = sim.ticks;
    while demo.done < demo.steps.len() && demo.steps[demo.done].0 <= now {
        let (_, step) = demo.steps[demo.done].clone();
        demo.done += 1;
        match step {
            Step::Button(mask, hold) => {
                input.latch(mask);
                demo.held.push((mask, hold, false));
            }
            Step::C(bit, hold) => {
                li.c_pressed |= bit;
                demo.held.push((bit, hold, true));
            }
            Step::Stick(x, y) => demo.stick = (x, y),
            Step::Set(slot, item) => {
                let age = sim.mario.link.age;
                sim.mario.link.inv.give(item);
                sim.mario.link.inv.set_c(slot, Some(CItem::Item(item)), age);
            }
            Step::Tap(name, hold) => {
                if let Some((win, w)) = windows.iter().next() {
                    match resolve_tap(&name, w.size(), &area, &menu_rects) {
                        Some(pos) => {
                            // Offscreen runs have no window camera, where Bevy resolves `Interaction` from touches: press the
                            // item-screen button by hand (the layer above it, `menu_clicks`, is what the tap exercises).
                            if let Some((_, e)) = menu_rects.0.get(&name).filter(|_| std::env::var_os("JUMPMAN_OFFSCREEN").is_some()) {
                                commands.entity(*e).insert(Interaction::Pressed);
                            }
                            let id = 100 + demo.next_finger;
                            demo.next_finger += 1;
                            touch.write(TouchInput { phase: TouchPhase::Started, position: pos, window: win, force: None, id });
                            demo.fingers.push((id, pos, pos, hold, hold));
                        }
                        None => warn!("demo: no touch target '{name}'"),
                    }
                }
            }
            Step::Drag(v, hold) => {
                if let Some((win, _)) = windows.iter().next() {
                    let id = 100 + demo.next_finger;
                    demo.next_finger += 1;
                    let (from, to) = (Vec2::new(v[0], v[1]), Vec2::new(v[2], v[3]));
                    touch.write(TouchInput { phase: TouchPhase::Started, position: from, window: win, force: None, id });
                    demo.fingers.push((id, from, to, hold, hold));
                }
            }
            Step::Lock => {
                lock.pulse = true;
            }
            Step::Teleport(p) => {
                use jumpman_sim::state::*;
                eprintln!("DEMO tick {now} teleport {p:?} (was {:?})", sim.mario.pos);
                let m = &mut sim.mario;
                m.pos = [p[0], p[1], p[2]];
                m.vel = [0.0; 3];
                m.forward_vel = 0.0;
                m.face_angle[1] = jumpman_sim::math::deg_to_angle(p[3]) as f64;
                m.link.phys.fall_start = p[1];
                m.invinc_timer = 0.0;
                m.set_action(ACT_IDLE, 0.0);
                sim.curr = sim.snapshot();
                sim.prev = sim.curr.clone();
            }
            Step::Menu(tab) => {
                match tab {
                    // `menu=N`: open on tab N, or switch tab when it is already open.
                    Some(t) => {
                        if !menu.open {
                            li.menu_toggle = true;
                        }
                        menu.show_tab(t);
                    }
                    None => li.menu_toggle = true,
                }
            }
            Step::Shot(name) => {
                let dir = std::env::var("JUMPMAN_LINK_SHOTS").unwrap_or_else(|_| ".".into());
                let path = format!("{dir}/{name}.png");
                // Offscreen runs (`JUMPMAN_OFFSCREEN`) render into an image: the window surface comes back black there.
                let shot = cams.iter().find_map(|rt| match rt {
                    bevy::camera::RenderTarget::Image(t) => Some(Screenshot::image(t.handle.clone())),
                    _ => None,
                });
                commands.spawn(shot.unwrap_or_else(Screenshot::primary_window)).observe(save_to_disk(path));
                // `JUMPMAN_LINK_LOCKSTEP=1`: the sim waits (virtual time paused) until the still of this tick has been rendered, so a busy machine
                // cannot run several ticks inside one frame and drop the stills of the others ("Duplicate render target for screenshot").
                if std::env::var_os("JUMPMAN_LINK_LOCKSTEP").is_some() {
                    demo.hold_frames = 4;
                    vtime.pause();
                }
            }
        }
    }
    if let Some((win, _)) = windows.iter().next() {
        for f in demo.fingers.iter_mut() {
            f.3 = f.3.saturating_sub(1);
            let t = 1.0 - f.3 as f32 / f.4.max(1) as f32;
            let pos = f.1.lerp(f.2, t);
            let phase = if f.3 == 0 { TouchPhase::Ended } else { TouchPhase::Moved };
            if f.1 != f.2 || f.3 == 0 {
                touch.write(TouchInput { phase, position: pos, window: win, force: None, id: f.0 });
            }
        }
        demo.fingers.retain(|f| f.3 > 0);
    }
    let (x, y) = demo.stick;
    if x != 0.0 || y != 0.0 {
        input.stick = Vec2::new(x, y);
        input.stick_mag = (x * x + y * y).sqrt().min(64.0);
    }
    let mut down_c = 0;
    let mut down_b = 0;
    for h in demo.held.iter_mut() {
        if h.1 > 0 {
            h.1 -= 1;
            if h.2 {
                down_c |= h.0;
            } else {
                down_b |= h.0;
            }
        }
    }
    demo.held.retain(|h| h.1 > 0);
    li.c_down |= down_c;
    input.down |= down_b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_script_grammar() {
        let s = parse("30:lock, 40:stick=up,60:a:5,90:c1:20,120:shot=bow,x:y,70:nonsense");
        assert_eq!(s.len(), 5);
        assert_eq!(s[0], (30, Step::Lock));
        assert_eq!(s[1], (40, Step::Stick(0.0, 64.0)));
        assert_eq!(s[2], (60, Step::Button(A_BUTTON, 5)));
        assert_eq!(s[3], (90, Step::C(C_LEFT, 20)));
        assert_eq!(s[4], (120, Step::Shot("bow".into())));
    }

    #[test]
    fn parses_the_menu_steps() {
        let s = parse("10:menu,20:menu=2,30:menu=x");
        assert_eq!(s[0], (10, Step::Menu(None)));
        assert_eq!(s[1], (20, Step::Menu(Some(2))));
        assert_eq!(s[2], (30, Step::Menu(None)), "an unparsable tab just toggles");
    }

    #[test]
    fn parses_the_touch_steps() {
        let s = parse("10:tap=c1,20:tap=cell3:7,30:set2=bomb,40:drag=100.200.300.200:12,50:set1=nonsense,60:tap=120.300");
        assert_eq!(s[0], (10, Step::Tap("c1".into(), 3)), "a tap lasts 3 ticks unless told otherwise");
        assert_eq!(s[1], (20, Step::Tap("cell3".into(), 7)));
        assert_eq!(s[2], (30, Step::Set(1, Item::Bomb)));
        assert_eq!(s[3], (40, Step::Drag([100.0, 200.0, 300.0, 200.0], 12)));
        assert_eq!(s[4], (60, Step::Tap("120.300".into(), 3)));
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn scripted_taps_land_on_the_touch_controls() {
        use crate::touch::{layout, Control};
        let area = crate::safe_area::SafeArea::default();
        let menu = super::super::ui::MenuRects::default();
        for size in [Vec2::new(874.0, 402.0), Vec2::new(667.0, 375.0), Vec2::new(844.0, 390.0), Vec2::new(390.0, 844.0)] {
            for name in ["c1", "c2", "c3", "c4", "items", "a", "b", "z", "l"] {
                let p = resolve_tap(name, size, &area, &menu).unwrap_or_else(|| panic!("{name} resolves"));
                assert!(p.x > 0.0 && p.x < size.x && p.y > 0.0 && p.y < size.y, "{name} on screen at {size:?}");
            }
            // The C buttons and the A / B / Z / L grid never overlap, whatever the phone.
            let c = super::super::ui::button_rects(size, Vec3::ZERO);
            for (_, r) in &c {
                for (ctl, g) in layout(size, Vec3::ZERO) {
                    assert!(r.intersect(g).is_empty(), "C box overlaps {ctl:?} at {size:?}");
                }
            }
        }
    }

    #[test]
    fn parses_the_loadout() {
        assert_eq!(parse_loadout("firearrow, dins ,stick,bow,nonsense"), vec![Item::FireArrow, Item::DinsFire, Item::DekuStick]);
        assert!(parse_loadout("x").is_empty());
    }
}
