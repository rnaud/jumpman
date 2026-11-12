//! Navigation logic of the v3 front end, independent of rendering: the file-select list of the
//! title screen, the hub room (a ring of doors, one wing for games and one for the lab) and the
//! character select that sits between choosing a level / mode and starting it.
//! Input goes in as [`In`]; the result is a list of [`Out`] (menu actions and sounds).
use crate::shell::{Character, ExperimentId, MenuSfx, Screen, UiAction};
use bevy::prelude::IVec2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Title,
    Hub,
    /// Character select (fighting-game style): a lineup of the playable characters, READY starts the run.
    Select,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wing {
    Games,
    Lab,
}

impl Wing {
    pub fn idx(self) -> usize {
        match self {
            Wing::Games => 0,
            Wing::Lab => 1,
        }
    }
    pub fn other(self) -> Wing {
        match self {
            Wing::Games => Wing::Lab,
            Wing::Lab => Wing::Games,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Wing::Games => "GAMES",
            Wing::Lab => "LAB",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleRow {
    Games,
    Levels,
    Lab,
    File,
    Options,
    Controls,
}

pub const TITLE_ROWS: [TitleRow; 6] = [TitleRow::Games, TitleRow::Levels, TitleRow::Lab, TitleRow::File, TitleRow::Options, TitleRow::Controls];

/// Something on screen a pointer can hit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Row(usize),
    Door(usize),
    Wing(Wing),
    Back,
    Enter,
    Prev,
    Next,
    Quit,
    /// A character on the select screen (its slot tab or its model).
    Slot(usize),
    /// READY on the select screen.
    Ready,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum In {
    Move(IVec2),
    Activate,
    Back,
    /// Previous / next wing (shoulder buttons, Tab).
    Wing(i32),
    /// Mouse hover (selects on the title list only).
    Hover(Hit),
    /// Mouse click / tap.
    Click(Hit),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Out {
    Action(UiAction),
    Sfx(&'static str),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Door {
    pub id: ExperimentId,
    /// Ported and installed: can be entered.
    pub enabled: bool,
}

/// What the model needs to know about the rest of the app.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub doors: [Vec<Door>; 2],
    pub hero: Character,
    /// Which of [`Character::ALL`] the installed data can play (ROM-derived models present).
    pub hero_ok: [bool; 4],
    /// Which of [`Character::ALL`] the chosen mode supports (all of them for a level).
    pub allowed: [bool; 4],
    /// What the select screen is choosing a character for.
    pub target: Target,
}

/// The level or mode the character select belongs to.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Target {
    /// "COURSE" or "MODE".
    pub kind: &'static str,
    /// Plate text, already uppercase ("BOB-OMB BATTLEFIELD", "GEM CLASH").
    pub name: String,
    /// The mode takes Mario only: the other slots are greyed out with a note.
    pub mario_only: bool,
}

impl Ctx {
    pub fn from_registry(hero: Character, hero_ok: [bool; 4], available: impl Fn(ExperimentId) -> bool) -> Self {
        let list = |tab| crate::shell::ExperimentId::for_tab(tab).into_iter().map(|id| Door { id, enabled: available(id) }).collect();
        Self { doors: [list(crate::shell::Tab::Games), list(crate::shell::Tab::Experiments)], hero, hero_ok, allowed: [true; 4], target: Target::default() }
    }

    /// Whether slot `i` of the roster can be confirmed.
    pub fn pickable(&self, i: usize) -> bool {
        self.hero_ok.get(i).copied().unwrap_or(false) && self.allowed.get(i).copied().unwrap_or(false)
    }

    /// First pickable slot at or after `from` (wrapping); `from` itself when none is.
    pub fn first_pickable(&self, from: usize) -> usize {
        (0..Character::ALL.len()).map(|k| (from + k) % Character::ALL.len()).find(|i| self.pickable(*i)).unwrap_or(from)
    }
}

/// Text of the select screen for one character (all ASCII, drawable by the bitmap font).
pub struct CharInfo {
    pub name: &'static str,
    /// Moves, comma separated, shown under the name.
    pub moves: &'static str,
    pub blurb: &'static str,
    /// What the character collects (`characters::Character::theme`).
    pub collects: &'static str,
}

pub fn char_info(c: Character) -> CharInfo {
    match c {
        Character::Mario => CharInfo { name: "MARIO", moves: "PUNCH, KICK, GROUND POUND", blurb: "ACROBAT: TRIPLE JUMPS, WALL KICKS", collects: "COINS" },
        Character::AdultLink => CharInfo { name: "LINK", moves: "SWORD COMBO, SHIELD, ITEMS", blurb: "HERO OF TIME: Z-TARGET AND ROLL", collects: "RUPEES" },
        Character::YoungLink => CharInfo { name: "YOUNG LINK", moves: "SWORD, SHIELD, ITEMS", blurb: "THE KID HERO: SMALL BUT QUICK", collects: "RUPEES" },
        Character::Banjo => CharInfo { name: "BANJO", moves: "CLAW, BARGE, PECK", blurb: "BEAR AND BIRD: FLAP FLIP, BEAK BARGE", collects: "NOTES" },
    }
}

/// Why a locked slot is locked (short, for the plate under the model).
pub fn lock_reason(c: Character, hero_ok: bool, allowed: bool) -> Option<&'static str> {
    if !allowed {
        Some("MARIO ONLY")
    } else if !hero_ok {
        Some(match c {
            Character::AdultLink | Character::YoungLink => "IMPORT OCARINA OF TIME",
            Character::Banjo => "IMPORT BANJO-KAZOOIE",
            Character::Mario => "NOT INSTALLED",
        })
    } else {
        None
    }
}

/// How long the camera dollies into a door before the mode starts.
pub const ENTER_TIME: f32 = 0.55;
/// How long the confirmed character does its pose before the run starts.
pub const READY_TIME: f32 = 0.9;

#[derive(Clone, PartialEq, Debug)]
pub struct Pending {
    pub action: UiAction,
    pub t: f32,
    pub dur: f32,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub page: Page,
    pub row: usize,
    pub wing: Wing,
    pub door: [usize; 2],
    pub pending: Option<Pending>,
    /// Highlighted slot of the character select (index into [`Character::ALL`]).
    pub sel: usize,
    /// The page the character select returns to (Title or Hub; the row / door selection is kept in the fields above).
    pub back_to: Page,
}

impl Default for Menu {
    fn default() -> Self {
        Self { page: Page::Title, row: 0, wing: Wing::Games, door: [0, 0], pending: None, sel: 0, back_to: Page::Title }
    }
}

fn wrap(i: usize, d: i32, n: usize) -> usize {
    if n == 0 { 0 } else { (i as i32 + d).rem_euclid(n as i32) as usize }
}

impl Menu {
    pub fn doors<'a>(&self, ctx: &'a Ctx) -> &'a [Door] {
        &ctx.doors[self.wing.idx()]
    }

    pub fn selected_door(&self, ctx: &Ctx) -> Option<Door> {
        self.doors(ctx).get(self.door[self.wing.idx()]).copied()
    }

    pub fn cur_door(&self) -> usize {
        self.door[self.wing.idx()]
    }

    /// Open the character select; the highlight starts on the player's last character (or the first one that can be played).
    pub fn open_select(&mut self, ctx: &Ctx) {
        if self.page != Page::Select {
            self.back_to = self.page;
        }
        self.page = Page::Select;
        self.pending = None;
        let start = Character::ALL.iter().position(|c| *c == ctx.hero).unwrap_or(0);
        self.sel = if ctx.pickable(start) { start } else { ctx.first_pickable(0) };
    }

    /// Leave the character select (back to the page it was opened from).
    pub fn close_select(&mut self) {
        if self.page == Page::Select {
            self.page = if self.back_to == Page::Select { Page::Title } else { self.back_to };
        }
        self.pending = None;
    }

    pub fn selected_hero(&self) -> Character {
        Character::ALL[self.sel.min(Character::ALL.len() - 1)]
    }

    /// Highlight a character (script / pointer); `None` when out of range.
    pub fn set_sel(&mut self, i: usize, out: &mut Vec<Out>) {
        if i < Character::ALL.len() && i != self.sel {
            self.sel = i;
            out.push(Out::Sfx(MenuSfx::MOVE));
        }
    }

    /// READY: confirm the highlighted character. Sets it as the played character now, starts the run after the pose.
    fn confirm_select(&mut self, ctx: &Ctx, out: &mut Vec<Out>) {
        if !ctx.pickable(self.sel) {
            out.push(Out::Sfx(MenuSfx::DENIED));
            return;
        }
        out.push(Out::Action(UiAction::Character(self.selected_hero())));
        out.push(Out::Sfx(MenuSfx::START));
        self.pending = Some(Pending { action: UiAction::Ready, t: 0.0, dur: READY_TIME });
    }

    fn open_hub(&mut self, wing: Wing, out: &mut Vec<Out>) {
        self.page = Page::Hub;
        self.wing = wing;
        out.push(Out::Sfx(MenuSfx::OPEN));
    }

    fn activate_row(&mut self, out: &mut Vec<Out>) {
        match TITLE_ROWS[self.row] {
            TitleRow::Games => self.open_hub(Wing::Games, out),
            TitleRow::Lab => self.open_hub(Wing::Lab, out),
            TitleRow::Levels => out.push(Out::Action(UiAction::Open(Screen::Courses))),
            TitleRow::File => out.push(Out::Action(UiAction::Open(Screen::Slots))),
            TitleRow::Options => out.push(Out::Action(UiAction::Open(Screen::Settings))),
            TitleRow::Controls => out.push(Out::Action(UiAction::Open(Screen::Controls))),
        }
    }

    fn activate_door(&mut self, ctx: &Ctx, out: &mut Vec<Out>) {
        match self.selected_door(ctx) {
            Some(d) if d.enabled => {
                self.pending = Some(Pending { action: UiAction::Experiment(d.id), t: 0.0, dur: ENTER_TIME });
                out.push(Out::Sfx(MenuSfx::START));
            }
            _ => out.push(Out::Sfx(MenuSfx::DENIED)),
        }
    }

    fn step_door(&mut self, d: i32, ctx: &Ctx, out: &mut Vec<Out>) {
        let n = self.doors(ctx).len();
        if n > 1 {
            let w = self.wing.idx();
            self.door[w] = wrap(self.door[w], d, n);
            out.push(Out::Sfx(MenuSfx::MOVE));
        }
    }

    fn set_wing(&mut self, wing: Wing, out: &mut Vec<Out>) {
        if self.wing != wing {
            self.wing = wing;
            out.push(Out::Sfx(MenuSfx::TAB));
        }
    }

    pub fn input(&mut self, ev: In, ctx: &Ctx) -> Vec<Out> {
        let mut out = Vec::new();
        if self.pending.is_some() {
            return out;
        }
        match self.page {
            Page::Title => match ev {
                In::Move(d) => {
                    if d.y != 0 {
                        self.row = wrap(self.row, d.y.signum(), TITLE_ROWS.len());
                        out.push(Out::Sfx(MenuSfx::MOVE));
                    }
                }
                In::Activate => self.activate_row(&mut out),
                In::Wing(d) => {
                    // Shoulder buttons jump straight into the hub's wings.
                    self.open_hub(if d > 0 { Wing::Lab } else { Wing::Games }, &mut out);
                }
                In::Hover(Hit::Row(i)) if i < TITLE_ROWS.len() && i != self.row => {
                    self.row = i;
                    out.push(Out::Sfx(MenuSfx::MOVE));
                }
                In::Click(Hit::Row(i)) if i < TITLE_ROWS.len() => {
                    self.row = i;
                    self.activate_row(&mut out);
                }
                In::Click(Hit::Quit) => out.push(Out::Action(UiAction::Quit)),
                _ => {}
            },
            Page::Hub => match ev {
                In::Move(d) => {
                    let step = if d.x != 0 { d.x.signum() } else { d.y.signum() };
                    if step != 0 {
                        self.step_door(step, ctx, &mut out);
                    }
                }
                In::Activate | In::Click(Hit::Enter) => self.activate_door(ctx, &mut out),
                In::Back | In::Click(Hit::Back) => {
                    self.page = Page::Title;
                    out.push(Out::Sfx(MenuSfx::BACK));
                }
                In::Wing(d) => {
                    let w = if d > 0 { self.wing.other() } else { self.wing.other() };
                    self.set_wing(w, &mut out);
                }
                In::Click(Hit::Wing(w)) => self.set_wing(w, &mut out),
                In::Click(Hit::Prev) => self.step_door(-1, ctx, &mut out),
                In::Click(Hit::Next) => self.step_door(1, ctx, &mut out),
                In::Click(Hit::Door(i)) => {
                    if i == self.cur_door() {
                        self.activate_door(ctx, &mut out);
                    } else if i < self.doors(ctx).len() {
                        let w = self.wing.idx();
                        self.door[w] = i;
                        out.push(Out::Sfx(MenuSfx::MOVE));
                    }
                }
                _ => {}
            },
            Page::Select => match ev {
                In::Move(d) => {
                    let step = if d.x != 0 { d.x.signum() } else { d.y.signum() };
                    if step != 0 {
                        self.set_sel(wrap(self.sel, step, Character::ALL.len()), &mut out);
                    }
                }
                // The shoulder buttons step the lineup too.
                In::Wing(d) if d != 0 => self.set_sel(wrap(self.sel, d.signum(), Character::ALL.len()), &mut out),
                In::Activate | In::Click(Hit::Ready) => self.confirm_select(ctx, &mut out),
                In::Back | In::Click(Hit::Back) => out.push(Out::Action(UiAction::Back)),
                In::Hover(Hit::Slot(i)) => self.set_sel(i, &mut out),
                In::Click(Hit::Prev) => self.set_sel(wrap(self.sel, -1, Character::ALL.len()), &mut out),
                In::Click(Hit::Next) => self.set_sel(wrap(self.sel, 1, Character::ALL.len()), &mut out),
                In::Click(Hit::Slot(i)) if i < Character::ALL.len() => {
                    // Tap once to look, tap again to confirm.
                    if i == self.sel {
                        self.confirm_select(ctx, &mut out);
                    } else {
                        self.set_sel(i, &mut out);
                    }
                }
                _ => {}
            },
        }
        out
    }

    /// Advance the enter animation; returns the action once the camera has reached the door.
    pub fn tick(&mut self, dt: f32) -> Option<UiAction> {
        let p = self.pending.as_mut()?;
        p.t += dt;
        if p.t >= p.dur {
            return self.pending.take().map(|p| p.action);
        }
        None
    }

    /// 0..1 progress of the dolly into a door.
    pub fn enter_progress(&self) -> f32 {
        self.pending.as_ref().map_or(0.0, |p| (p.t / p.dur.max(0.001)).clamp(0.0, 1.0))
    }
}

/// Short sign text for a mode (the registry's titles are long); the registry stays the source of
/// the list, order and category. Unknown ids fall back to the registry title.
pub fn sign_name(id: ExperimentId) -> String {
    match id {
        ExperimentId::Brawl => "GEM CLASH",
        ExperimentId::Relic => "RELIC DESCENT",
        ExperimentId::LastStar => "LAST STAND",
        ExperimentId::Kart => "KART RACING",
        ExperimentId::Swarm => "SWARM",
        ExperimentId::Deck => "CARD RUN",
        ExperimentId::Halls => "PAINTED HALLS",
        ExperimentId::Gate => "HOLD THE GATE",
        ExperimentId::Party => "SUNSTONE PARTY",
        ExperimentId::Race => "SUMMIT RACE",
        ExperimentId::Runner => "ENDLESS RUN",
        ExperimentId::God => "PLATFORM WAR",
        ExperimentId::Diorama => "DIORAMA",
        ExperimentId::Dimensions => "DIMENSIONS",
        ExperimentId::Shrink => "SHRINK RAY",
        ExperimentId::Planet => "TINY PLANET",
        ExperimentId::Hundred => "THE CROWD",
    }
    .to_string()
}

/// Break a sign name into at most two lines of up to `max` characters.
pub fn sign_lines(name: &str, max: usize) -> Vec<String> {
    if name.chars().count() <= max {
        return vec![name.to_string()];
    }
    let words: Vec<&str> = name.split(' ').collect();
    let mut best = (usize::MAX, 1);
    for cut in 1..words.len() {
        let a = words[..cut].join(" ").chars().count();
        let b = words[cut..].join(" ").chars().count();
        if a.max(b) < best.0 {
            best = (a.max(b), cut);
        }
    }
    vec![words[..best.1].join(" "), words[best.1..].join(" ")]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx::from_registry(Character::Mario, [true, true, false, true], |_| true)
    }

    #[test]
    fn title_rows_wrap_and_tick() {
        let c = ctx();
        let mut m = Menu::default();
        let o = m.input(In::Move(IVec2::new(0, -1)), &c);
        assert_eq!(m.row, TITLE_ROWS.len() - 1);
        assert_eq!(o, vec![Out::Sfx(MenuSfx::MOVE)]);
        m.input(In::Move(IVec2::new(0, 1)), &c);
        assert_eq!(m.row, 0);
    }

    #[test]
    fn games_row_opens_the_hub_and_back_returns() {
        let c = ctx();
        let mut m = Menu::default();
        let o = m.input(In::Activate, &c);
        assert_eq!((m.page, m.wing), (Page::Hub, Wing::Games));
        assert_eq!(o, vec![Out::Sfx(MenuSfx::OPEN)]);
        m.input(In::Back, &c);
        assert_eq!(m.page, Page::Title);
        m.row = 2;
        m.input(In::Activate, &c);
        assert_eq!((m.page, m.wing), (Page::Hub, Wing::Lab));
    }

    #[test]
    fn menu_rows_emit_screen_actions() {
        let c = ctx();
        let mut m = Menu::default();
        for (row, screen) in [(1, Screen::Courses), (3, Screen::Slots), (4, Screen::Settings), (5, Screen::Controls)] {
            m.row = row;
            assert_eq!(m.input(In::Activate, &c), vec![Out::Action(UiAction::Open(screen))]);
        }
    }

    fn select_ctx() -> Ctx {
        // Young Link's data is missing (see `ctx`).
        Ctx { allowed: [true; 4], ..ctx() }
    }

    #[test]
    fn title_has_no_play_as_row() {
        assert_eq!(TITLE_ROWS.len(), 6);
        assert!(TITLE_ROWS.iter().all(|r| !format!("{r:?}").contains("PlayAs")));
    }

    #[test]
    fn select_opens_on_the_last_character_and_remembers_where_it_came_from() {
        let mut c = select_ctx();
        c.hero = Character::Banjo;
        let mut m = Menu { page: Page::Hub, wing: Wing::Lab, door: [0, 3], ..Default::default() };
        m.open_select(&c);
        assert_eq!((m.page, m.back_to, m.selected_hero()), (Page::Select, Page::Hub, Character::Banjo));
        m.close_select();
        assert_eq!((m.page, m.wing, m.cur_door()), (Page::Hub, Wing::Lab, 3), "back lands on the same door");
        // The default highlight falls to the first playable one when the last character cannot be played.
        c.hero = Character::YoungLink;
        m.open_select(&c);
        assert_eq!(m.selected_hero(), Character::Mario);
        // Opening twice must not lose the way back.
        m.open_select(&c);
        assert_eq!(m.back_to, Page::Hub);
    }

    #[test]
    fn select_moves_through_the_lineup_including_locked_slots() {
        let c = select_ctx();
        let mut m = Menu::default();
        m.open_select(&c);
        assert_eq!(m.sel, 0);
        m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::AdultLink);
        m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::YoungLink, "locked slots can be looked at");
        m.input(In::Wing(1), &c);
        assert_eq!(m.selected_hero(), Character::Banjo);
        m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::Mario, "wraps");
        m.input(In::Move(IVec2::new(-1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::Banjo);
        // Hover (mouse) highlights.
        assert_eq!(m.input(In::Hover(Hit::Slot(1)), &c), vec![Out::Sfx(MenuSfx::MOVE)]);
        assert_eq!(m.selected_hero(), Character::AdultLink);
    }

    #[test]
    fn ready_sets_the_character_then_fires_ready_after_the_pose() {
        let c = select_ctx();
        let mut m = Menu::default();
        m.open_select(&c);
        m.input(In::Move(IVec2::new(1, 0)), &c);
        let o = m.input(In::Activate, &c);
        assert_eq!(o, vec![Out::Action(UiAction::Character(Character::AdultLink)), Out::Sfx(MenuSfx::START)]);
        assert!(m.input(In::Move(IVec2::new(1, 0)), &c).is_empty(), "locked in while the pose plays");
        assert_eq!(m.tick(READY_TIME * 0.5), None);
        assert!(m.enter_progress() > 0.4);
        assert_eq!(m.tick(READY_TIME), Some(UiAction::Ready));
    }

    #[test]
    fn locked_slots_deny_ready() {
        let c = select_ctx();
        let mut m = Menu::default();
        m.open_select(&c);
        m.sel = 2;
        assert!(!c.pickable(2));
        assert_eq!(m.input(In::Activate, &c), vec![Out::Sfx(MenuSfx::DENIED)]);
        assert_eq!(m.input(In::Click(Hit::Ready), &c), vec![Out::Sfx(MenuSfx::DENIED)]);
        assert!(m.pending.is_none());
        assert_eq!(lock_reason(Character::YoungLink, false, true), Some("IMPORT OCARINA OF TIME"));
        assert_eq!(lock_reason(Character::Banjo, false, true), Some("IMPORT BANJO-KAZOOIE"));
        assert_eq!(lock_reason(Character::Mario, true, true), None);
    }

    #[test]
    fn mario_only_modes_grey_out_everyone_else() {
        let mut c = select_ctx();
        c.hero = Character::Banjo;
        c.allowed = [true, false, false, false];
        c.target = Target { kind: "MODE", name: "PAINTED HALLS".into(), mario_only: true };
        let mut m = Menu::default();
        m.open_select(&c);
        assert_eq!(m.selected_hero(), Character::Mario, "the highlight starts on the only playable character");
        m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::AdultLink);
        assert_eq!(m.input(In::Activate, &c), vec![Out::Sfx(MenuSfx::DENIED)]);
        assert_eq!(lock_reason(Character::AdultLink, true, false), Some("MARIO ONLY"));
        m.input(In::Move(IVec2::new(-1, 0)), &c);
        assert_eq!(m.input(In::Activate, &c)[0], Out::Action(UiAction::Character(Character::Mario)));
    }

    #[test]
    fn tapping_a_slot_highlights_then_confirms() {
        let c = select_ctx();
        let mut m = Menu::default();
        m.open_select(&c);
        assert_eq!(m.input(In::Click(Hit::Slot(3)), &c), vec![Out::Sfx(MenuSfx::MOVE)]);
        assert_eq!(m.selected_hero(), Character::Banjo);
        assert!(m.pending.is_none());
        let o = m.input(In::Click(Hit::Slot(3)), &c);
        assert_eq!(o[0], Out::Action(UiAction::Character(Character::Banjo)));
        assert!(m.pending.is_some());
    }

    #[test]
    fn back_leaves_through_the_ui_stack() {
        let c = select_ctx();
        let mut m = Menu::default();
        m.open_select(&c);
        assert_eq!(m.input(In::Back, &c), vec![Out::Action(UiAction::Back)]);
        assert_eq!(m.input(In::Click(Hit::Back), &c), vec![Out::Action(UiAction::Back)]);
        // The shell then pops its screen stack and `close_select` restores the page.
        m.close_select();
        assert_eq!(m.page, Page::Title);
    }

    #[test]
    fn every_character_has_drawable_text() {
        for ch in Character::ALL {
            let i = char_info(ch);
            for t in [i.name, i.moves, i.blurb, i.collects] {
                assert_eq!(t, super::super::fb::clean(t), "{t}");
            }
        }
        assert_eq!(char_info(Character::AdultLink).moves, "SWORD COMBO, SHIELD, ITEMS");
        assert_eq!(char_info(Character::Banjo).moves, "CLAW, BARGE, PECK");
        assert_eq!(char_info(Character::Mario).moves, "PUNCH, KICK, GROUND POUND");
    }

    #[test]
    fn hub_doors_wrap_per_wing_and_remember() {
        let c = ctx();
        let mut m = Menu { page: Page::Hub, ..Default::default() };
        let n = c.doors[0].len();
        m.input(In::Move(IVec2::new(-1, 0)), &c);
        assert_eq!(m.cur_door(), n - 1);
        m.input(In::Wing(1), &c);
        assert_eq!(m.wing, Wing::Lab);
        assert_eq!(m.cur_door(), 0);
        m.input(In::Move(IVec2::new(0, 1)), &c);
        assert_eq!(m.cur_door(), 1);
        m.input(In::Wing(-1), &c);
        assert_eq!(m.cur_door(), n - 1);
    }

    #[test]
    fn entering_a_door_dollies_then_fires_the_experiment() {
        let c = ctx();
        let mut m = Menu { page: Page::Hub, ..Default::default() };
        let first = c.doors[0][0].id;
        let o = m.input(In::Activate, &c);
        assert_eq!(o, vec![Out::Sfx(MenuSfx::START)]);
        assert!(m.input(In::Move(IVec2::new(1, 0)), &c).is_empty(), "input is ignored while entering");
        assert_eq!(m.tick(ENTER_TIME * 0.5), None);
        assert!(m.enter_progress() > 0.4);
        assert_eq!(m.tick(ENTER_TIME), Some(UiAction::Experiment(first)));
        assert!(m.pending.is_none());
    }

    #[test]
    fn disabled_doors_are_denied() {
        let mut c = ctx();
        c.doors[0][0].enabled = false;
        let mut m = Menu { page: Page::Hub, ..Default::default() };
        assert_eq!(m.input(In::Activate, &c), vec![Out::Sfx(MenuSfx::DENIED)]);
        assert!(m.pending.is_none());
    }

    #[test]
    fn pointer_selects_then_enters_doors() {
        let c = ctx();
        let mut m = Menu { page: Page::Hub, ..Default::default() };
        m.input(In::Click(Hit::Door(3)), &c);
        assert_eq!(m.cur_door(), 3);
        assert!(m.pending.is_none());
        m.input(In::Click(Hit::Door(3)), &c);
        assert!(m.pending.is_some());
    }

    #[test]
    fn pointer_hover_selects_title_rows_and_click_activates() {
        let c = ctx();
        let mut m = Menu::default();
        assert_eq!(m.input(In::Hover(Hit::Row(4)), &c), vec![Out::Sfx(MenuSfx::MOVE)]);
        assert_eq!(m.row, 4);
        assert!(m.input(In::Hover(Hit::Row(4)), &c).is_empty());
        assert_eq!(m.input(In::Click(Hit::Row(4)), &c), vec![Out::Action(UiAction::Open(Screen::Settings))]);
    }

    #[test]
    fn sign_names_cover_every_mode_and_wrap() {
        for id in ExperimentId::ALL {
            let n = sign_name(id);
            assert_eq!(n, super::super::fb::clean(&n), "{n} must be drawable");
            for l in sign_lines(&n, 8) {
                assert!(l.chars().count() <= 14);
            }
        }
        assert_eq!(sign_lines("RELIC DESCENT", 8), vec!["RELIC", "DESCENT"]);
        assert_eq!(sign_lines("SWARM", 8), vec!["SWARM"]);
    }

    #[test]
    fn registry_drives_the_doors() {
        let c = ctx();
        assert_eq!(c.doors[0].len() + c.doors[1].len(), ExperimentId::ALL.len());
        assert!(c.doors[0].iter().all(|d| d.id.category() == crate::experiments::Category::Game));
        assert!(c.doors[1].iter().all(|d| d.id.category() == crate::experiments::Category::Experiment));
    }
}
