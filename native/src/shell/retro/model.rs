//! Navigation logic of the v3 front end, independent of rendering: the file-select list of the
//! title screen and the hub room (a ring of doors, one wing for games and one for the lab).
//! Input goes in as [`In`]; the result is a list of [`Out`] (menu actions and sounds).
use crate::shell::{Character, ExperimentId, MenuSfx, Screen, UiAction};
use bevy::prelude::IVec2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Title,
    Hub,
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
    PlayAs,
    File,
    Options,
    Controls,
}

pub const TITLE_ROWS: [TitleRow; 7] = [TitleRow::Games, TitleRow::Levels, TitleRow::Lab, TitleRow::PlayAs, TitleRow::File, TitleRow::Options, TitleRow::Controls];

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
    /// Which of [`Character::ALL`] can be picked.
    pub hero_ok: [bool; 4],
}

impl Ctx {
    pub fn from_registry(hero: Character, hero_ok: [bool; 4], available: impl Fn(ExperimentId) -> bool) -> Self {
        let list = |tab| crate::shell::ExperimentId::for_tab(tab).into_iter().map(|id| Door { id, enabled: available(id) }).collect();
        Self { doors: [list(crate::shell::Tab::Games), list(crate::shell::Tab::Experiments)], hero, hero_ok }
    }
}

/// How long the camera dollies into a door before the mode starts.
pub const ENTER_TIME: f32 = 0.55;

#[derive(Clone, PartialEq, Debug)]
pub struct Pending {
    pub action: UiAction,
    pub t: f32,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub page: Page,
    pub row: usize,
    pub wing: Wing,
    pub door: [usize; 2],
    pub pending: Option<Pending>,
}

impl Default for Menu {
    fn default() -> Self {
        Self { page: Page::Title, row: 0, wing: Wing::Games, door: [0, 0], pending: None }
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

    /// The next hero after `d` steps that can be picked (stays put when none can).
    pub fn cycle_hero(ctx: &Ctx, d: i32) -> Character {
        let start = Character::ALL.iter().position(|c| *c == ctx.hero).unwrap_or(0);
        for k in 1..=Character::ALL.len() as i32 {
            let i = wrap(start, d * k, Character::ALL.len());
            if ctx.hero_ok[i] {
                return Character::ALL[i];
            }
        }
        ctx.hero
    }

    fn open_hub(&mut self, wing: Wing, out: &mut Vec<Out>) {
        self.page = Page::Hub;
        self.wing = wing;
        out.push(Out::Sfx(MenuSfx::OPEN));
    }

    fn activate_row(&mut self, ctx: &Ctx, out: &mut Vec<Out>) {
        match TITLE_ROWS[self.row] {
            TitleRow::Games => self.open_hub(Wing::Games, out),
            TitleRow::Lab => self.open_hub(Wing::Lab, out),
            TitleRow::Levels => out.push(Out::Action(UiAction::Open(Screen::Courses))),
            TitleRow::File => out.push(Out::Action(UiAction::Open(Screen::Slots))),
            TitleRow::Options => out.push(Out::Action(UiAction::Open(Screen::Settings))),
            TitleRow::Controls => out.push(Out::Action(UiAction::Open(Screen::Controls))),
            TitleRow::PlayAs => {
                let next = Self::cycle_hero(ctx, 1);
                if next != ctx.hero {
                    out.push(Out::Action(UiAction::Character(next)));
                } else {
                    out.push(Out::Sfx(MenuSfx::DENIED));
                }
            }
        }
    }

    fn activate_door(&mut self, ctx: &Ctx, out: &mut Vec<Out>) {
        match self.selected_door(ctx) {
            Some(d) if d.enabled => {
                self.pending = Some(Pending { action: UiAction::Experiment(d.id), t: 0.0 });
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
                    } else if d.x != 0 && TITLE_ROWS[self.row] == TitleRow::PlayAs {
                        let next = Self::cycle_hero(ctx, d.x.signum());
                        if next != ctx.hero {
                            out.push(Out::Action(UiAction::Character(next)));
                        }
                    }
                }
                In::Activate => self.activate_row(ctx, &mut out),
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
                    self.activate_row(ctx, &mut out);
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
        }
        out
    }

    /// Advance the enter animation; returns the action once the camera has reached the door.
    pub fn tick(&mut self, dt: f32) -> Option<UiAction> {
        let p = self.pending.as_mut()?;
        p.t += dt;
        if p.t >= ENTER_TIME {
            return self.pending.take().map(|p| p.action);
        }
        None
    }

    /// 0..1 progress of the dolly into a door.
    pub fn enter_progress(&self) -> f32 {
        self.pending.as_ref().map_or(0.0, |p| (p.t / ENTER_TIME).clamp(0.0, 1.0))
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
        for (row, screen) in [(1, Screen::Courses), (4, Screen::Slots), (5, Screen::Settings), (6, Screen::Controls)] {
            m.row = row;
            assert_eq!(m.input(In::Activate, &c), vec![Out::Action(UiAction::Open(screen))]);
        }
    }

    #[test]
    fn hero_row_cycles_and_skips_missing() {
        let c = ctx();
        let mut m = Menu::default();
        m.row = 3;
        let o = m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(o, vec![Out::Action(UiAction::Character(Character::AdultLink))]);
        // Young Link is not installed: stepping on from Adult Link skips it.
        let c2 = Ctx { hero: Character::AdultLink, ..c.clone() };
        assert_eq!(Menu::cycle_hero(&c2, 1), Character::Banjo);
        assert_eq!(Menu::cycle_hero(&c2, -1), Character::Mario);
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
        assert_eq!(m.input(In::Hover(Hit::Row(5)), &c), vec![Out::Sfx(MenuSfx::MOVE)]);
        assert_eq!(m.row, 5);
        assert!(m.input(In::Hover(Hit::Row(5)), &c).is_empty());
        assert_eq!(m.input(In::Click(Hit::Row(5)), &c), vec![Out::Action(UiAction::Open(Screen::Settings))]);
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
