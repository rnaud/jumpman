//! The pause menu and its pages in the v3 look (see [`super::skin`]): PAUSED, OPTIONS, CONTROLS, HOW TO PLAY and
//! SAVE FILE. The content and the actions are the shell's; only the presentation lives here. The same builders serve the
//! title's OPTIONS / CONTROLS / SAVE FILE rows, which open these pages too.
use super::fb::{clean, Fb};
use super::skin::{Kind, Skin, GOLD, PALE, SKY, WHITE};
use crate::shell::screens::Metrics;
use crate::shell::settings::{DisplaySettings, Settings};
use crate::shell::{DisplayKey, ExperimentId, SaveSlotSummaries, Screen, UiAction};
use bevy::prelude::*;

/// Layout pixels per framebuffer pixel for the sub-pages: the title is 1 px = 2.8 px on a desktop window and 1.6 pt on a phone;
/// narrow windows (a phone in portrait) drop to 1 so the text still fits.
pub fn scale_for(m: &Metrics) -> u32 {
    let k = if m.compact { 2 } else { 3 };
    if m.w / (k as f32) < 200.0 { 1 } else { k }
}

/// How the pause menu is laid out: the scale, the logo size, whether the sub-line fits, columns and button width.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PausePlan {
    pub k: u32,
    pub logo: i32,
    pub sub: bool,
    pub two_cols: bool,
    pub bw: i32,
    /// Height of the content in framebuffer px.
    pub h: i32,
}

/// The largest scale (and the roomiest layout) whose whole menu, LEAVE included, fits the window without scrolling.
pub fn pause_plan(m: &Metrics, buttons: usize, keys: usize) -> PausePlan {
    let top = if m.compact { 3 } else { 4 };
    let mut best = None;
    for k in (1..=top.min(if m.compact { 2 } else { 3 })).rev() {
        for logo in [if m.compact { 3 } else { 4 }, 2, 1] {
            for sub in [true, false] {
                for two in [false, true] {
                    let avail_w = (m.w / k as f32) as i32 - 8;
                    let bw = if m.compact { 118 } else { 150 };
                    let cols = if two { 2 } else { 1 };
                    let bw_eff = bw.min(avail_w - 12 - if keys > 0 && !two { 0 } else { 0 });
                    let panel_w = cols * bw_eff + (cols - 1) * 3 + 12;
                    if panel_w > avail_w || bw_eff < 70 {
                        continue;
                    }
                    let rows = (buttons as i32 + cols - 1) / cols;
                    let list_h = rows * (super::skin::ROW_H + 3) + 14;
                    // Mode keys sit beside the list when the width allows, else below it.
                    let side = keys > 0 && avail_w >= panel_w + 6 + 80;
                    let keys_h = if keys > 0 && !side { 22 + ((keys as i32 + 1) / 2) * 16 } else { 0 };
                    let h = 9 * logo + 4 + 6 + if sub { 11 } else { 0 } + list_h + keys_h + 12;
                    let plan = PausePlan { k, logo, sub, two_cols: two, bw: bw_eff, h };
                    if (h as f32) * k as f32 <= m.h - 8.0 {
                        return plan;
                    }
                    if best.is_none() {
                        best = Some(plan);
                    }
                }
            }
        }
    }
    best.unwrap_or(PausePlan { k: 1, logo: 1, sub: false, two_cols: true, bw: 90, h: 0 })
}

/// Framebuffer px available to a page.
fn avail(skin: &Skin, m: &Metrics) -> i32 {
    (m.w / skin.k as f32) as i32 - 8
}

fn col(gap: i32, skin: &Skin) -> Node {
    Node { flex_direction: FlexDirection::Column, row_gap: skin.px(gap), ..default() }
}

fn row(gap: i32, skin: &Skin) -> Node {
    Node { flex_direction: FlexDirection::Row, column_gap: skin.px(gap), align_items: AlignItems::Center, ..default() }
}

/// Title of a page with BACK on its left (the hub's top bar).
fn header(skin: &mut Skin, c: &mut ChildSpawnerCommands, m: &Metrics, title: &str, width: i32) {
    let scale = if m.compact { 2 } else { 3 };
    let back_w = 46;
    c.spawn(Node { width: skin.px(width), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
        skin.button(r, Kind::Red, back_w, "BACK", 1, UiAction::Back);
        skin.logo(r, title, scale);
        r.spawn(Node { width: skin.px(back_w), ..default() });
    });
}

/// Common frame: header, then a panel `width` wide that the caller fills.
fn framed(skin: &mut Skin, p: &mut ChildSpawnerCommands, m: &Metrics, title: &str, width: i32, body: impl FnOnce(&mut Skin, &mut ChildSpawnerCommands, i32)) {
    let width = width.min(avail(skin, m));
    let inner = width - 16;
    p.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: skin.px(5), flex_shrink: 0.0, margin: UiRect::vertical(Val::Auto), ..default() }).with_children(|c| {
        header(skin, c, m, title, width);
        let node = Node { width: skin.px(width), row_gap: skin.px(3), ..default() };
        skin.panel(c, node).with_children(|c| body(skin, c, inner));
    });
}

pub struct PauseData<'a> {
    pub has_save: bool,
    pub has_howto: bool,
    pub experiment: Option<ExperimentId>,
    /// The running mode's own keys (label, key, selected) offered here instead of the dock.
    pub items: &'a [(String, KeyCode, bool)],
}

/// The buttons of the pause menu, top to bottom.
pub fn pause_list(has_save: bool, has_howto: bool) -> Vec<(Kind, &'static str, UiAction)> {
    let mut list = vec![(Kind::Green, "RESUME", UiAction::Resume)];
    // kart_anywhere hook: CALL KART / PARK KART while a level is being played on foot or driven.
    if let Some(label) = crate::kart_anywhere::pause_label() {
        list.push((Kind::Blue, label, UiAction::ToggleKart));
    }
    if has_howto {
        list.push((Kind::Blue, "HOW TO PLAY", UiAction::Open(Screen::HowTo)));
    }
    list.push((Kind::Blue, "OPTIONS", UiAction::Open(Screen::Settings)));
    list.push((Kind::Blue, "CONTROLS", UiAction::Open(Screen::Controls)));
    if has_save {
        list.push((Kind::Blue, "SAVE FILE", UiAction::Open(Screen::Slots)));
    }
    list.push((Kind::Red, "LEAVE", UiAction::MainMenu));
    list
}

/// The pause menu: logo, the mode name, the buttons of the title's list and, beside them, the running mode's own keys.
pub fn pause(skin: &mut Skin, p: &mut ChildSpawnerCommands, m: &Metrics, d: &PauseData, plan: PausePlan) {
    p.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: skin.px(3), flex_shrink: 0.0, margin: UiRect::vertical(Val::Auto), ..default() }).with_children(|c| {
        skin.logo(c, "PAUSED", plan.logo);
        if plan.sub {
            let line = d.experiment.map_or_else(|| "A 3D COLLECTION".to_string(), |x| x.title().to_uppercase());
            skin.text(c, &line, 1, SKY);
        }
        c.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, align_items: AlignItems::FlexStart, column_gap: skin.px(6), row_gap: skin.px(4), ..default() }).with_children(|r| {
            let list = pause_list(d.has_save, d.has_howto);
            let bw = plan.bw;
            let width = if plan.two_cols { 2 * bw + 3 } else { bw };
            skin.panel(r, Node { width: skin.px(width + 12), ..default() }).with_children(|c| {
                c.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: skin.px(3), row_gap: skin.px(3), ..default() }).with_children(|c| {
                    for (kind, text, action) in list {
                        skin.button(c, kind, bw, text, 2, action);
                    }
                });
            });
            if !d.items.is_empty() {
                let kw = if m.compact { 112 } else { 150 };
                skin.panel(r, Node { width: skin.px(kw + 12), ..default() }).with_children(|c| {
                    skin.heading(c, "THIS MODE");
                    c.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: skin.px(3), row_gap: skin.px(3), ..default() }).with_children(|k| {
                        for (text, key, selected) in d.items {
                            let w = Skin::fit(text, 1).min(kw);
                            let mut e = skin.button(k, Kind::Blue, w, text, 1, UiAction::DockKey(*key));
                            if *selected {
                                e.insert(crate::shell::theme::Selected);
                            }
                        }
                    });
                });
            }
        });
    });
}

fn toggle(skin: &mut Skin, r: &mut ChildSpawnerCommands, w: i32, name: &str, on: bool, action: UiAction) {
    let mut e = skin.button(r, Kind::Blue, w, &format!("{name}: {}", if on { "ON" } else { "OFF" }), 1, action);
    if on {
        e.insert(crate::shell::theme::Selected);
    }
}

/// A setting: its name, then `< VALUE >`; on a narrow screen the name sits above the button.
fn cycle(skin: &mut Skin, c: &mut ChildSpawnerCommands, inner: i32, name: &str, key: DisplayKey, value: &str) {
    let stacked = inner < 300;
    let mut node = row(6, skin);
    if stacked {
        node = col(1, skin);
    }
    c.spawn(node).with_children(|r| {
        r.spawn(Node { width: skin.px(96), ..default() }).with_children(|n| {
            skin.text(n, name, 1, PALE);
        });
        skin.button(r, Kind::Blue, if stacked { inner } else { 196 }, &format!("<  {value}  >"), 1, UiAction::Cycle(key, 1));
    });
}

/// Two toggles per row when they fit.
fn toggles(skin: &mut Skin, c: &mut ChildSpawnerCommands, inner: i32, pair: Vec<(&str, bool, UiAction)>) {
    let two = inner >= 2 * 152 + 4;
    let w = if two { 152 } else { inner };
    let mut node = row(4, skin);
    if !two {
        node = col(3, skin);
    }
    c.spawn(node).with_children(|r| {
        for (name, on, action) in pair {
            toggle(skin, r, w, name, on, action);
        }
    });
}

pub fn settings(skin: &mut Skin, p: &mut ChildSpawnerCommands, m: &Metrics, s: &Settings) {
    let d = &s.display;
    framed(skin, p, m, "OPTIONS", 316, |skin, c, inner| {
        skin.heading(c, "DISPLAY");
        c.spawn(row(4, skin)).with_children(|r| {
            let pw = ((inner - 4) / 2).min(152);
            for (text, modern) in [("CLASSIC PRESET", false), ("MODERN PRESET", true)] {
                let on = *d == if modern { DisplaySettings::MODERN } else { DisplaySettings::CLASSIC };
                let mut e = skin.button(r, Kind::Green, pw, text, 1, UiAction::Preset(modern));
                if on {
                    e.insert(crate::shell::theme::Selected);
                }
            }
        });
        cycle(skin, c, inner, "RESOLUTION", DisplayKey::Resolution, d.resolution.label());
        cycle(skin, c, inner, "ASPECT RATIO", DisplayKey::Aspect, d.aspect.label());
        cycle(skin, c, inner, "LIGHTING", DisplayKey::Look, d.look.label());
        cycle(skin, c, inner, "MOVEMENT", DisplayKey::Motion, d.motion.label());
        cycle(skin, c, inner, "UPSCALING", DisplayKey::Scaling, d.scaling.label());
        cycle(skin, c, inner, "LIGHT QUALITY", DisplayKey::Quality, s.lighting.label());
        cycle(skin, c, inner, "CAMERA", DisplayKey::Camera, s.camera.label());
        skin.paragraph(c, &crate::lighting::ray_tracing_note(), 1, SKY, inner);
        skin.heading(c, "AUDIO, SCREEN AND PERFORMANCE");
        toggles(skin, c, inner, vec![("AUDIO", !s.muted, UiAction::ToggleMute), ("SHOW FPS", s.show_fps, UiAction::ToggleShowFps)]);
        toggles(skin, c, inner, vec![("UNCAP FPS", s.uncap_fps, UiAction::ToggleUncap), ("SM64 EXTRAS", s.classic_intro, UiAction::ToggleClassicIntro)]);
        toggles(skin, c, inner, vec![("HINTS AND TIPS", s.show_hints, UiAction::ToggleShowHints)]);
        skin.paragraph(c, "Display preferences are saved on this device.", 1, SKY, inner);
        if crate::byor::is_byor_install() {
            skin.heading(c, "ROM IMPORT");
            let have = |g: &str| if crate::byor::installed(g) { "INSTALLED" } else if crate::byor::game_stale(g) { "UPDATE AVAILABLE" } else { "NOT IMPORTED" };
            skin.paragraph(c, &format!("SM64: {}   MK64: {}   OOT: {}   BK: {}", have("sm64"), have("mk64"), have("oot"), have("bk")), 1, PALE, inner);
            skin.button(c, Kind::Green, 190.min(inner), "IMPORT / RE-EXTRACT ROMS", 1, UiAction::ImportRoms);
        }
    });
}

fn binding_rows(skin: &mut Skin, c: &mut ChildSpawnerCommands, title: &str, rows: Vec<(String, String, bool, UiAction)>) {
    skin.heading(c, title);
    for (label, text, waiting, action) in rows {
        c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|r| {
            skin.text(r, &label, 1, PALE);
            let shown = if waiting { "PRESS A KEY...".to_string() } else { text };
            // Fit long labels ("SHIFT (L) / SHIFT (R)"): the plate used to clip them at both ends.
            let mut e = skin.button(r, Kind::Blue, 104.max(Skin::fit(&shown, 1)), &shown, 1, action);
            if waiting {
                e.insert(crate::shell::theme::Selected);
            }
        });
    }
}

fn fixed_rows(skin: &mut Skin, c: &mut ChildSpawnerCommands, inner: i32, title: &str, rows: &[(&str, &str)]) {
    skin.heading(c, title);
    for (input, what) in rows {
        let wide = Fb::text_width(&clean(input), 1) + Fb::text_width(&clean(what), 1) + 10 <= inner;
        let mut node = Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: skin.px(6), ..default() };
        if !wide {
            node.flex_direction = FlexDirection::Column;
            node.align_items = AlignItems::FlexStart;
            node.column_gap = Val::Px(0.0);
        }
        c.spawn(node).with_children(|r| {
            skin.text(r, input, 1, GOLD);
            skin.text(r, what, 1, PALE);
        });
    }
}

pub fn controls(skin: &mut Skin, p: &mut ChildSpawnerCommands, m: &Metrics, pad: bool, bound: Option<&crate::bindings::Bindings>) {
    use crate::bindings::{Action, Bindings, Listen};
    let fallback = Bindings::default();
    let b = bound.unwrap_or(&fallback);
    framed(skin, p, m, "CONTROLS", 316, |skin, c, inner| {
        skin.paragraph(c, crate::shell::screens::basics_text(pad), 1, SKY, inner);
        let keys = Action::KEYBOARD.iter().map(|&x| (x.label().to_string(), b.key_text(x), b.listening == Some(Listen::Key(x)), UiAction::Rebind(x))).collect();
        binding_rows(skin, c, "KEYBOARD", keys);
        let pads = Action::PAD.iter().map(|&x| (x.label().to_string(), b.pad_text(x), b.listening == Some(Listen::Pad(x)), UiAction::RebindPad(x))).collect();
        binding_rows(skin, c, "GAMEPAD", pads);
        fixed_rows(skin, c, inner, "FIXED", crate::shell::screens::FIXED_BINDINGS);
        skin.paragraph(c, if pad { "Gamepad detected." } else { "No gamepad detected - plug one in and press any button." }, 1, if pad { super::fb::rgb(0x9cf06a) } else { SKY }, inner);
        skin.paragraph(c, "Click a binding, then press the new key or button (Esc cancels).", 1, SKY, inner);
        skin.button(c, Kind::Red, 150, "RESET DEFAULTS", 1, UiAction::ResetBindings);
    });
}

pub fn how_to(skin: &mut Skin, p: &mut ChildSpawnerCommands, m: &Metrics, experiment: Option<ExperimentId>) {
    framed(skin, p, m, "HOW TO PLAY", 316, |skin, c, inner| {
        let help = crate::hints::help();
        if let Some(x) = experiment {
            skin.text(c, &x.title().to_uppercase(), 2, GOLD);
        }
        if !help.pitch.is_empty() {
            skin.paragraph(c, &help.pitch, 1, WHITE, inner);
        }
        for (k, v) in &help.keys {
            c.spawn(Node { width: Val::Percent(100.0), column_gap: skin.px(8), align_items: AlignItems::FlexStart, ..default() }).with_children(|r| {
                let w = Fb::text_width(&clean(k), 1).max(30) + 8;
                r.spawn(Node { width: skin.px(w.min(110)), flex_shrink: 0.0, ..default() }).with_children(|n| {
                    skin.key_plate(n, k, w.min(110));
                });
                skin.paragraph(r, v, 1, PALE, inner - w.min(110) - 8);
            });
        }
        for n in &help.notes {
            skin.paragraph(c, n, 1, SKY, inner);
        }
        if help.keys.is_empty() && help.notes.is_empty() && help.pitch.is_empty() {
            skin.paragraph(c, "Nothing special: the basics are on the CONTROLS screen.", 1, PALE, inner);
        }
        skin.paragraph(c, crate::shell::screens::basics_text(false), 1, SKY, inner);
    });
}

pub fn slots(skin: &mut Skin, p: &mut ChildSpawnerCommands, m: &Metrics, active: u32, slots: &SaveSlotSummaries, confirm: bool) {
    framed(skin, p, m, "SAVE FILE", 250, |skin, c, inner| {
        for n in 1..=4u8 {
            let text = match slots.0[n as usize - 1] {
                Some(stars) => format!("FILE {}   {} STARS", (b'A' + n - 1) as char, stars),
                None => format!("FILE {}   NEW", (b'A' + n - 1) as char),
            };
            let mut e = skin.button(c, Kind::Blue, inner, &text, 2, UiAction::Slot(n));
            if active == u32::from(n) {
                e.insert(crate::shell::theme::Selected);
            }
        }
        let mut e = skin.button(c, Kind::Red, inner, if confirm { "PRESS AGAIN TO ERASE THIS FILE" } else { "RESET STARS & SAVED COINS" }, 1, UiAction::ResetSave);
        if confirm {
            e.insert(crate::shell::theme::Selected);
        }
        skin.paragraph(c, "Star and coin records are kept per file.", 1, SKY, inner);
    });
}
