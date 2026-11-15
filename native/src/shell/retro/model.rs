//! Navigation logic of the v3 front end, independent of rendering: the file-select list of the
//! title screen, the mode grid (two tabs of tiles: GAMES and LAB), a mode's setup page (its start
//! choices, for the modes that have some) and the character select that sits between choosing a
//! level / mode and starting it. Nothing is loaded until READY on the character select.
//! Input goes in as [`In`]; the result is a list of [`Out`] (menu actions and sounds).
use crate::shell::{Character, ExperimentId, MenuSfx, Screen, UiAction};
use bevy::prelude::IVec2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Title,
    /// The mode grid: one tab of tiles per [`Wing`].
    Grid,
    /// MODE SETUP: the chosen mode's start options (arena, difficulty...), then on to the character select.
    Setup,
    /// Character select (fighting-game style): a lineup of the playable characters, READY starts the run.
    Select,
}

/// A tab of the mode grid.
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
    /// A mode tile of the grid (index into the current tab).
    Tile(usize),
    Wing(Wing),
    Back,
    /// PLAY in the grid's info box.
    Enter,
    Prev,
    Next,
    Quit,
    /// A character on the select screen (its slot tab or its model).
    Slot(usize),
    /// READY on the select screen; NEXT on the setup page.
    Ready,
    /// A row of the setup page.
    Opt(usize),
    /// The left (-1) / right (+1) arrow of a setup row.
    OptStep(usize, i8),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum In {
    Move(IVec2),
    Activate,
    Back,
    /// Previous / next tab (shoulder buttons, Tab).
    Wing(i32),
    /// Mouse hover (highlights on the title list, the grid and the lineup).
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
pub struct Tile {
    pub id: ExperimentId,
    /// Ported and installed: can be started.
    pub enabled: bool,
    /// Why it cannot be started (the info box says so), when it cannot.
    pub lock: Option<&'static str>,
}

/// One start option of the mode on the setup page (`experiments::start_setup::Row`, as text).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SetupRow {
    pub label: String,
    pub values: Vec<String>,
    pub value: usize,
    /// Values that cannot be chosen (not imported, not unlocked yet).
    pub locked: Vec<bool>,
    /// The row applies with the other choices (greyed and skipped otherwise).
    pub enabled: bool,
}

impl SetupRow {
    /// The next choosable value `d` steps away (wrapping, skipping locked values); `None` when there is no other.
    pub fn step(&self, d: i32) -> Option<usize> {
        let n = self.values.len();
        (1..n).map(|k| wrap(self.value, d.signum() * k as i32, n)).find(|v| !self.locked.get(*v).copied().unwrap_or(false))
    }
}

/// What the model needs to know about the rest of the app.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// The tiles of each tab ([`Wing::idx`]), in menu order.
    pub tiles: [Vec<Tile>; 2],
    /// Columns of each tab's grid: the front end sets them from its layout, up / down step by that many tiles.
    pub cols: [usize; 2],
    pub hero: Character,
    /// Which of [`Character::ALL`] the installed data can play (ROM-derived models present).
    pub hero_ok: [bool; Character::COUNT],
    /// Which of [`Character::ALL`] the chosen mode supports (all of them for a level).
    pub allowed: [bool; Character::COUNT],
    /// What the select screen is choosing a character for.
    pub target: Target,
    /// The start options of the picked mode (the setup page).
    pub setup: Vec<SetupRow>,
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

/// What a tile that is not ported says.
pub const NOT_PORTED: &str = "NOT AVAILABLE YET";

impl Ctx {
    pub fn from_registry(hero: Character, hero_ok: [bool; Character::COUNT], available: impl Fn(ExperimentId) -> bool) -> Self {
        let list = |tab| {
            crate::shell::ExperimentId::for_tab(tab)
                .into_iter()
                .map(|id| {
                    let enabled = available(id);
                    Tile { id, enabled, lock: (!enabled).then_some(NOT_PORTED) }
                })
                .collect()
        };
        Self { tiles: [list(crate::shell::Tab::Games), list(crate::shell::Tab::Experiments)], cols: [4, 4], hero, hero_ok, allowed: [true; Character::COUNT], target: Target::default(), setup: Vec::new() }
    }

    /// Whether slot `i` stands in the lineup: a character whose model is missing is shown locked, except the ones left out
    /// entirely ([`Character::hidden_when_missing`]: Bowser).
    pub fn shown(&self, i: usize) -> bool {
        Character::ALL.get(i).is_some_and(|c| !c.hidden_when_missing() || self.hero_ok[i])
    }

    /// Which of the [`Character::ALL`] slots stand in the lineup, in order.
    pub fn shown_slots(&self) -> [bool; Character::COUNT] {
        std::array::from_fn(|i| self.shown(i))
    }

    /// Whether slot `i` of the roster can be confirmed.
    pub fn pickable(&self, i: usize) -> bool {
        self.shown(i) && self.hero_ok.get(i).copied().unwrap_or(false) && self.allowed.get(i).copied().unwrap_or(false)
    }

    /// First pickable slot at or after `from` (wrapping); `from` itself when none is.
    pub fn first_pickable(&self, from: usize) -> usize {
        (0..Character::ALL.len()).map(|k| (from + k) % Character::ALL.len()).find(|i| self.pickable(*i)).unwrap_or(from)
    }

    /// The shown slot `d` steps from `from` along the lineup (wrapping, hidden slots skipped).
    pub fn step_slot(&self, from: usize, d: i32) -> usize {
        let n = Character::ALL.len() as i32;
        let mut i = from as i32;
        for _ in 0..n {
            i = (i + d.signum()).rem_euclid(n);
            if self.shown(i as usize) {
                return i as usize;
            }
        }
        from
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
        Character::Bowser => CharInfo { name: "BOWSER", moves: "FIRE BREATH, KICK, GROUND POUND", blurb: "KING OF THE KOOPAS: MARIO'S MOVES, TWICE THE SIZE", collects: "COINS" },
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
            Character::Mario | Character::Bowser => "NOT INSTALLED",
        })
    } else {
        None
    }
}

/// Why a mode tile is locked: the ROM its data comes from was not imported.
pub fn tile_lock(missing: crate::content::Game) -> &'static str {
    match missing {
        crate::content::Game::Mk64 => "IMPORT MARIO KART 64",
        crate::content::Game::Oot => "IMPORT OCARINA OF TIME",
        crate::content::Game::Bk => "IMPORT BANJO-KAZOOIE",
    }
}

/// How long a pressed tile flashes before the mode is chosen (the character select opens next).
pub const ENTER_TIME: f32 = 0.18;
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
    /// Highlighted tile of each tab.
    pub tile: [usize; 2],
    pub pending: Option<Pending>,
    /// Highlighted slot of the character select (index into [`Character::ALL`]).
    pub sel: usize,
    /// The page the character select / the setup page return to (Title or Grid; the row / tile selection is kept above).
    pub back_to: Page,
    /// Cursor of the setup page: a row, or `setup.len()` for NEXT.
    pub opt: usize,
}

impl Default for Menu {
    fn default() -> Self {
        Self { page: Page::Title, row: 0, wing: Wing::Games, tile: [0, 0], pending: None, sel: 0, back_to: Page::Title, opt: 0 }
    }
}

fn wrap(i: usize, d: i32, n: usize) -> usize {
    if n == 0 { 0 } else { (i as i32 + d).rem_euclid(n as i32) as usize }
}

/// One d-pad step on a grid of `n` tiles laid out row by row, `cols` to a row. Left / right walk the list (from the end of
/// a row to the start of the next, and from the last tile around to the first); up / down move a whole row in the same
/// column, wrapping top <-> bottom, and land on the last tile when the short last row has nothing in that column.
pub fn grid_step(i: usize, d: IVec2, n: usize, cols: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    if d.x != 0 {
        return wrap(i, d.x.signum(), n);
    }
    if d.y == 0 {
        return i;
    }
    let cols = cols.clamp(1, n);
    let rows = n.div_ceil(cols);
    let (r, c) = (i / cols, i % cols);
    (wrap(r, d.y.signum(), rows) * cols + c).min(n - 1)
}

impl Menu {
    pub fn tiles<'a>(&self, ctx: &'a Ctx) -> &'a [Tile] {
        &ctx.tiles[self.wing.idx()]
    }

    pub fn selected_tile(&self, ctx: &Ctx) -> Option<Tile> {
        self.tiles(ctx).get(self.tile[self.wing.idx()]).copied()
    }

    pub fn cur_tile(&self) -> usize {
        self.tile[self.wing.idx()]
    }

    /// Open the character select; the highlight starts on the player's last character (or the first one that can be played).
    pub fn open_select(&mut self, ctx: &Ctx) {
        if matches!(self.page, Page::Title | Page::Grid) {
            self.back_to = self.page;
        }
        self.page = Page::Select;
        self.pending = None;
        let start = Character::ALL.iter().position(|c| *c == ctx.hero).unwrap_or(0);
        self.sel = if ctx.pickable(start) { start } else { ctx.first_pickable(0) };
    }

    /// Open a mode's setup page (the cursor on its first row).
    pub fn open_setup(&mut self, ctx: &Ctx) {
        if matches!(self.page, Page::Title | Page::Grid) {
            self.back_to = self.page;
        }
        self.page = Page::Setup;
        self.pending = None;
        self.opt = ctx.setup.iter().position(|r| r.enabled).unwrap_or(ctx.setup.len());
    }

    /// Leave the character select / the setup page (back to the page they were opened from).
    pub fn close_select(&mut self) {
        if matches!(self.page, Page::Select | Page::Setup) {
            self.page = if matches!(self.back_to, Page::Title | Page::Grid) { self.back_to } else { Page::Title };
        }
        self.pending = None;
    }

    /// Change the value of setup row `row` by `d` steps (skipping locked values).
    fn step_opt(&mut self, row: usize, d: i32, ctx: &Ctx, out: &mut Vec<Out>) {
        let Some(r) = ctx.setup.get(row) else { return };
        if !r.enabled {
            out.push(Out::Sfx(MenuSfx::DENIED));
            return;
        }
        if let Some(v) = r.step(d) {
            out.push(Out::Action(UiAction::SetupOption(row as u8, v as u8)));
            out.push(Out::Sfx(MenuSfx::MOVE));
        }
    }

    /// Move the setup cursor `d` steps over the enabled rows and NEXT.
    fn move_opt(&mut self, d: i32, ctx: &Ctx, out: &mut Vec<Out>) {
        let n = ctx.setup.len() + 1;
        let usable = |i: usize| i == ctx.setup.len() || ctx.setup[i].enabled;
        if let Some(next) = (1..n).map(|k| wrap(self.opt.min(n - 1), d.signum() * k as i32, n)).find(|i| usable(*i)) {
            self.opt = next;
            out.push(Out::Sfx(MenuSfx::MOVE));
        }
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

    fn open_grid(&mut self, wing: Wing, out: &mut Vec<Out>) {
        self.page = Page::Grid;
        self.wing = wing;
        out.push(Out::Sfx(MenuSfx::OPEN));
    }

    fn activate_row(&mut self, out: &mut Vec<Out>) {
        match TITLE_ROWS[self.row] {
            TitleRow::Games => self.open_grid(Wing::Games, out),
            TitleRow::Lab => self.open_grid(Wing::Lab, out),
            TitleRow::Levels => out.push(Out::Action(UiAction::Open(Screen::Courses))),
            TitleRow::File => out.push(Out::Action(UiAction::Open(Screen::Slots))),
            TitleRow::Options => out.push(Out::Action(UiAction::Open(Screen::Settings))),
            TitleRow::Controls => out.push(Out::Action(UiAction::Open(Screen::Controls))),
        }
    }

    fn activate_tile(&mut self, ctx: &Ctx, out: &mut Vec<Out>) {
        match self.selected_tile(ctx) {
            Some(d) if d.enabled => {
                self.pending = Some(Pending { action: UiAction::Experiment(d.id), t: 0.0, dur: ENTER_TIME });
                out.push(Out::Sfx(MenuSfx::START));
            }
            _ => out.push(Out::Sfx(MenuSfx::DENIED)),
        }
    }

    fn move_tile(&mut self, d: IVec2, ctx: &Ctx, out: &mut Vec<Out>) {
        let w = self.wing.idx();
        let next = grid_step(self.tile[w], d, ctx.tiles[w].len(), ctx.cols[w]);
        if next != self.tile[w] {
            self.tile[w] = next;
            out.push(Out::Sfx(MenuSfx::MOVE));
        }
    }

    fn set_tile(&mut self, i: usize, ctx: &Ctx, out: &mut Vec<Out>) {
        let w = self.wing.idx();
        if i < ctx.tiles[w].len() && i != self.tile[w] {
            self.tile[w] = i;
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
                    // Shoulder buttons jump straight into the grid's tabs.
                    self.open_grid(if d > 0 { Wing::Lab } else { Wing::Games }, &mut out);
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
            Page::Grid => match ev {
                In::Move(d) => self.move_tile(d, ctx, &mut out),
                In::Activate | In::Click(Hit::Enter) => self.activate_tile(ctx, &mut out),
                In::Back | In::Click(Hit::Back) => {
                    self.page = Page::Title;
                    out.push(Out::Sfx(MenuSfx::BACK));
                }
                In::Wing(d) if d != 0 => self.set_wing(self.wing.other(), &mut out),
                In::Click(Hit::Wing(w)) => self.set_wing(w, &mut out),
                In::Click(Hit::Prev) => self.move_tile(IVec2::new(-1, 0), ctx, &mut out),
                In::Click(Hit::Next) => self.move_tile(IVec2::new(1, 0), ctx, &mut out),
                In::Hover(Hit::Tile(i)) => self.set_tile(i, ctx, &mut out),
                In::Click(Hit::Tile(i)) => {
                    // Tap once to read about it, tap again to choose it (a mouse hover has highlighted it already).
                    if i == self.cur_tile() {
                        self.activate_tile(ctx, &mut out);
                    } else {
                        self.set_tile(i, ctx, &mut out);
                    }
                }
                _ => {}
            },
            Page::Setup => match ev {
                In::Move(d) if d.y != 0 => self.move_opt(d.y, ctx, &mut out),
                In::Move(d) if d.x != 0 => self.step_opt(self.opt, d.x, ctx, &mut out),
                In::Wing(d) if d != 0 => self.step_opt(self.opt, d, ctx, &mut out),
                // A (or Enter, or NEXT) goes on to the character select with the choices as they stand.
                In::Activate | In::Click(Hit::Ready) => out.push(Out::Action(UiAction::SetupDone)),
                In::Back | In::Click(Hit::Back) => out.push(Out::Action(UiAction::Back)),
                In::Hover(Hit::Opt(i)) | In::Click(Hit::Opt(i)) if i < ctx.setup.len() && ctx.setup[i].enabled && i != self.opt => {
                    self.opt = i;
                    out.push(Out::Sfx(MenuSfx::MOVE));
                }
                In::Click(Hit::OptStep(i, d)) if i < ctx.setup.len() => {
                    self.opt = i;
                    self.step_opt(i, d as i32, ctx, &mut out);
                }
                _ => {}
            },
            Page::Select => match ev {
                In::Move(d) => {
                    let step = if d.x != 0 { d.x.signum() } else { d.y.signum() };
                    if step != 0 {
                        self.set_sel(ctx.step_slot(self.sel, step), &mut out);
                    }
                }
                // The shoulder buttons step the lineup too.
                In::Wing(d) if d != 0 => self.set_sel(ctx.step_slot(self.sel, d), &mut out),
                In::Activate | In::Click(Hit::Ready) => self.confirm_select(ctx, &mut out),
                In::Back | In::Click(Hit::Back) => out.push(Out::Action(UiAction::Back)),
                In::Hover(Hit::Slot(i)) => self.set_sel(i, &mut out),
                In::Click(Hit::Prev) => self.set_sel(ctx.step_slot(self.sel, -1), &mut out),
                In::Click(Hit::Next) => self.set_sel(ctx.step_slot(self.sel, 1), &mut out),
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

    /// Advance the pressed-tile flash / READY pose; returns the action once it has played.
    pub fn tick(&mut self, dt: f32) -> Option<UiAction> {
        let p = self.pending.as_mut()?;
        p.t += dt;
        if p.t >= p.dur {
            return self.pending.take().map(|p| p.action);
        }
        None
    }

    /// 0..1 progress of the pending action.
    pub fn enter_progress(&self) -> f32 {
        self.pending.as_ref().map_or(0.0, |p| (p.t / p.dur.max(0.001)).clamp(0.0, 1.0))
    }
}

/// Short tile text for a mode (the registry's titles are long); the registry stays the source of
/// the list, order and category.
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

/// Break a tile name into at most two lines of up to `max` characters.
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
        Ctx::from_registry(Character::Mario, [true, true, false, true, false], |_| true)
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
    fn games_row_opens_the_grid_and_back_returns() {
        let c = ctx();
        let mut m = Menu::default();
        let o = m.input(In::Activate, &c);
        assert_eq!((m.page, m.wing), (Page::Grid, Wing::Games));
        assert_eq!(o, vec![Out::Sfx(MenuSfx::OPEN)]);
        m.input(In::Back, &c);
        assert_eq!(m.page, Page::Title);
        m.row = 2;
        m.input(In::Activate, &c);
        assert_eq!((m.page, m.wing), (Page::Grid, Wing::Lab));
        // A shoulder button on the title jumps into a tab directly.
        let mut m = Menu::default();
        m.input(In::Wing(1), &c);
        assert_eq!((m.page, m.wing), (Page::Grid, Wing::Lab));
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
        Ctx { allowed: [true; Character::COUNT], ..ctx() }
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
        let mut m = Menu { page: Page::Grid, wing: Wing::Lab, tile: [0, 3], ..Default::default() };
        m.open_select(&c);
        assert_eq!((m.page, m.back_to, m.selected_hero()), (Page::Select, Page::Grid, Character::Banjo));
        m.close_select();
        assert_eq!((m.page, m.wing, m.cur_tile()), (Page::Grid, Wing::Lab, 3), "back lands on the same tile");
        // The default highlight falls to the first playable one when the last character cannot be played.
        c.hero = Character::YoungLink;
        m.open_select(&c);
        assert_eq!(m.selected_hero(), Character::Mario);
        // Opening twice must not lose the way back.
        m.open_select(&c);
        assert_eq!(m.back_to, Page::Grid);
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

    /// Bowser stands at the end of the lineup when his model is installed and is left out (not locked) when it is not.
    #[test]
    fn bowser_joins_the_lineup_only_with_his_model() {
        let mut c = select_ctx();
        assert!(!c.shown(4) && !c.pickable(4), "no glb: not in the lineup");
        let mut m = Menu::default();
        m.open_select(&c);
        m.sel = 3;
        m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::Mario, "Banjo wraps to Mario past the hidden slot");
        c.hero = Character::Bowser;
        m.open_select(&c);
        assert_eq!(m.selected_hero(), Character::Mario, "a hidden last pick falls back to the first playable one");
        // Installed: the fifth slot, reachable both ways, and READY takes him.
        c.hero_ok[4] = true;
        assert!(c.shown(4) && c.pickable(4));
        m.close_select();
        m.open_select(&c);
        assert_eq!(m.selected_hero(), Character::Bowser);
        m.input(In::Move(IVec2::new(1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::Mario);
        m.input(In::Move(IVec2::new(-1, 0)), &c);
        assert_eq!(m.selected_hero(), Character::Bowser);
        assert_eq!(m.input(In::Activate, &c)[0], Out::Action(UiAction::Character(Character::Bowser)));
        assert_eq!(char_info(Character::Bowser).name, "BOWSER");
        assert_eq!(lock_reason(Character::Bowser, true, false), Some("MARIO ONLY"));
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
        c.allowed = [true, false, false, false, false];
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
    fn grid_step_moves_in_two_dimensions() {
        // 10 tiles, 4 to a row:   0 1 2 3 / 4 5 6 7 / 8 9
        let (n, cols) = (10, 4);
        let right = IVec2::new(1, 0);
        let left = IVec2::new(-1, 0);
        let down = IVec2::new(0, 1);
        let up = IVec2::new(0, -1);
        assert_eq!(grid_step(0, right, n, cols), 1);
        assert_eq!(grid_step(3, right, n, cols), 4, "right runs on into the next row");
        assert_eq!(grid_step(9, right, n, cols), 0, "and wraps from the last tile to the first");
        assert_eq!(grid_step(0, left, n, cols), 9);
        assert_eq!(grid_step(1, down, n, cols), 5);
        assert_eq!(grid_step(5, down, n, cols), 9);
        assert_eq!(grid_step(6, down, n, cols), 9, "short last row: the last tile");
        assert_eq!(grid_step(9, down, n, cols), 1, "down from the bottom wraps to the top, same column");
        assert_eq!(grid_step(1, up, n, cols), 9);
        assert_eq!(grid_step(2, up, n, cols), 9, "up from the top into a column the last row lacks");
        assert_eq!(grid_step(8, up, n, cols), 4);
        assert_eq!(grid_step(4, IVec2::ZERO, n, cols), 4);
        // Degenerate grids.
        assert_eq!(grid_step(0, down, 0, 4), 0);
        assert_eq!(grid_step(2, down, 3, 0), 0, "zero columns act as one column");
        assert_eq!(grid_step(0, down, 3, 8), 0, "a single row: up / down stay put");
        // Every move stays in range for every shape.
        for n in 1..14 {
            for cols in 1..7 {
                for i in 0..n {
                    for d in [right, left, up, down] {
                        assert!(grid_step(i, d, n, cols) < n);
                    }
                }
            }
        }
    }

    #[test]
    fn grid_keys_move_per_tab_and_remember() {
        let mut c = ctx();
        c.cols = [4, 2];
        let mut m = Menu { page: Page::Grid, ..Default::default() };
        let n = c.tiles[0].len();
        m.input(In::Move(IVec2::new(-1, 0)), &c);
        assert_eq!(m.cur_tile(), n - 1);
        m.input(In::Move(IVec2::new(0, -1)), &c);
        assert_eq!(m.cur_tile(), n - 1 - 4, "up a row");
        assert_eq!(m.input(In::Wing(1), &c), vec![Out::Sfx(MenuSfx::TAB)]);
        assert_eq!((m.wing, m.cur_tile()), (Wing::Lab, 0));
        m.input(In::Move(IVec2::new(0, 1)), &c);
        assert_eq!(m.cur_tile(), 2, "the lab tab has 2 columns");
        m.input(In::Wing(-1), &c);
        assert_eq!((m.wing, m.cur_tile()), (Wing::Games, n - 1 - 4), "each tab keeps its own highlight");
        // Tabs by pointer.
        m.input(In::Click(Hit::Wing(Wing::Lab)), &c);
        assert_eq!(m.wing, Wing::Lab);
        assert!(m.input(In::Click(Hit::Wing(Wing::Lab)), &c).is_empty(), "the open tab again: nothing");
    }

    #[test]
    fn the_grid_keeps_the_registry_order() {
        let c = ctx();
        let games: Vec<_> = c.tiles[0].iter().map(|t| t.id).collect();
        assert_eq!(&games[..2], &[ExperimentId::Brawl, ExperimentId::Swarm], "Gem Clash and Star Swarm first");
        assert_eq!(games, ExperimentId::for_tab(crate::shell::Tab::Games));
        let lab: Vec<_> = c.tiles[1].iter().map(|t| t.id).collect();
        assert_eq!(lab, ExperimentId::for_tab(crate::shell::Tab::Experiments));
    }

    #[test]
    fn choosing_a_tile_flashes_then_fires_the_experiment() {
        let c = ctx();
        let mut m = Menu { page: Page::Grid, ..Default::default() };
        let first = c.tiles[0][0].id;
        let o = m.input(In::Activate, &c);
        assert_eq!(o, vec![Out::Sfx(MenuSfx::START)]);
        assert!(m.input(In::Move(IVec2::new(1, 0)), &c).is_empty(), "input is ignored while the tile flashes");
        assert_eq!(m.tick(ENTER_TIME * 0.5), None);
        assert_eq!(m.tick(ENTER_TIME), Some(UiAction::Experiment(first)));
        assert!(m.pending.is_none());
        // PLAY in the info box does the same.
        m.input(In::Click(Hit::Enter), &c);
        assert_eq!(m.tick(1.0), Some(UiAction::Experiment(first)));
    }

    #[test]
    fn locked_tiles_are_denied_and_say_why() {
        let mut c = ctx();
        c.tiles[0][0].enabled = false;
        c.tiles[0][0].lock = Some(tile_lock(crate::content::Game::Mk64));
        let mut m = Menu { page: Page::Grid, ..Default::default() };
        assert_eq!(m.input(In::Activate, &c), vec![Out::Sfx(MenuSfx::DENIED)]);
        assert_eq!(m.input(In::Click(Hit::Tile(0)), &c), vec![Out::Sfx(MenuSfx::DENIED)]);
        assert!(m.pending.is_none());
        assert_eq!(m.selected_tile(&c).unwrap().lock, Some("IMPORT MARIO KART 64"));
        let unported = Ctx::from_registry(Character::Mario, [true; Character::COUNT], |id| id != ExperimentId::Swarm);
        assert_eq!(unported.tiles[0][1].lock, Some(NOT_PORTED));
        assert_eq!(unported.tiles[0][0].lock, None);
    }

    #[test]
    fn pointer_hover_or_tap_highlights_then_a_second_tap_chooses() {
        let c = ctx();
        let mut m = Menu { page: Page::Grid, ..Default::default() };
        assert_eq!(m.input(In::Hover(Hit::Tile(3)), &c), vec![Out::Sfx(MenuSfx::MOVE)]);
        assert_eq!(m.cur_tile(), 3);
        assert!(m.pending.is_none(), "hover never chooses");
        m.input(In::Click(Hit::Tile(5)), &c);
        assert_eq!(m.cur_tile(), 5);
        assert!(m.pending.is_none(), "the first tap only highlights (touch)");
        m.input(In::Click(Hit::Tile(5)), &c);
        assert!(m.pending.is_some(), "the second tap chooses");
        // Out-of-range hits are ignored.
        let mut m = Menu { page: Page::Grid, ..Default::default() };
        assert!(m.input(In::Click(Hit::Tile(99)), &c).is_empty());
        assert_eq!(m.cur_tile(), 0);
        // BACK by pointer.
        m.input(In::Click(Hit::Back), &c);
        assert_eq!(m.page, Page::Title);
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

    /// Choosing a mode never starts it: the grid hands `Experiment` to the shell, which opens the character select; only
    /// READY there produces the action that loads anything.
    #[test]
    fn flow_from_grid_to_ready_has_one_start_point() {
        let c = select_ctx();
        let mut m = Menu::default();
        m.input(In::Activate, &c);
        assert_eq!(m.page, Page::Grid);
        m.input(In::Move(IVec2::new(1, 0)), &c);
        m.input(In::Activate, &c);
        let chosen = m.tick(1.0);
        assert_eq!(chosen, Some(UiAction::Experiment(c.tiles[0][1].id)));
        // The shell answers by opening the select (sync_state calls this when the screen is CharSelect).
        m.open_select(&c);
        assert_eq!((m.page, m.back_to), (Page::Select, Page::Grid));
        // Nothing is pending until READY.
        for _ in 0..10 {
            assert_eq!(m.tick(0.5), None);
        }
        m.input(In::Activate, &c);
        assert_eq!(m.tick(READY_TIME), Some(UiAction::Ready));
        // Back from the select returns to the same tile of the grid.
        m.close_select();
        assert_eq!((m.page, m.cur_tile()), (Page::Grid, 1));
    }

    fn setup_ctx() -> Ctx {
        let row = |label: &str, n: usize| SetupRow { label: label.into(), values: (0..n).map(|i| format!("V{i}")).collect(), value: 0, locked: vec![false; n], enabled: true };
        let mut c = select_ctx();
        c.setup = vec![row("MODE", 2), row("ARENA", 4), row("BOTS", 3)];
        c.setup[1].locked[1] = true;
        c.setup[2].enabled = false;
        c
    }

    #[test]
    fn setup_rows_step_values_and_skip_what_cannot_be_chosen() {
        let c = setup_ctx();
        let mut m = Menu { page: Page::Grid, ..Default::default() };
        m.open_setup(&c);
        assert_eq!((m.page, m.back_to, m.opt), (Page::Setup, Page::Grid, 0));
        assert_eq!(m.input(In::Move(IVec2::new(1, 0)), &c), vec![Out::Action(UiAction::SetupOption(0, 1)), Out::Sfx(MenuSfx::MOVE)]);
        m.input(In::Move(IVec2::new(0, 1)), &c);
        assert_eq!(m.opt, 1);
        // Value 1 of ARENA is locked: right jumps to 2, left wraps to 3.
        assert_eq!(m.input(In::Move(IVec2::new(1, 0)), &c)[0], Out::Action(UiAction::SetupOption(1, 2)));
        assert_eq!(m.input(In::Move(IVec2::new(-1, 0)), &c)[0], Out::Action(UiAction::SetupOption(1, 3)));
        // BOTS is disabled: the cursor skips it and lands on NEXT, then wraps to the top.
        m.input(In::Move(IVec2::new(0, 1)), &c);
        assert_eq!(m.opt, c.setup.len(), "NEXT");
        m.input(In::Move(IVec2::new(0, 1)), &c);
        assert_eq!(m.opt, 0);
        assert_eq!(m.input(In::Click(Hit::OptStep(2, 1)), &c), vec![Out::Sfx(MenuSfx::DENIED)], "a disabled row does not change");
        // Pointer: arrows step, a row tap moves the cursor.
        assert_eq!(m.input(In::Click(Hit::OptStep(1, -1)), &c)[0], Out::Action(UiAction::SetupOption(1, 3)));
        assert_eq!(m.opt, 1);
        assert!(m.input(In::Click(Hit::Opt(2)), &c).is_empty(), "disabled rows cannot take the cursor");
    }

    #[test]
    fn setup_goes_on_to_the_select_and_back_returns_to_the_grid() {
        let c = setup_ctx();
        let mut m = Menu { page: Page::Grid, wing: Wing::Games, tile: [4, 0], ..Default::default() };
        m.open_setup(&c);
        assert_eq!(m.input(In::Activate, &c), vec![Out::Action(UiAction::SetupDone)]);
        assert_eq!(m.input(In::Click(Hit::Ready), &c), vec![Out::Action(UiAction::SetupDone)]);
        // The shell opens the select; back from it shows the setup again, back from that the same grid tile.
        m.open_select(&c);
        assert_eq!(m.back_to, Page::Grid, "the select does not return into the setup page by itself");
        m.open_setup(&c);
        assert_eq!(m.page, Page::Setup);
        assert_eq!(m.input(In::Back, &c), vec![Out::Action(UiAction::Back)]);
        m.close_select();
        assert_eq!((m.page, m.cur_tile()), (Page::Grid, 4));
        // No setup rows at all: the cursor sits on NEXT.
        let mut m = Menu::default();
        m.open_setup(&select_ctx());
        assert_eq!(m.opt, 0);
        assert_eq!(m.input(In::Activate, &select_ctx()), vec![Out::Action(UiAction::SetupDone)]);
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
    fn registry_drives_the_tiles() {
        let c = ctx();
        assert_eq!(c.tiles[0].len() + c.tiles[1].len(), crate::experiments::MENU_IDS.len());
        // The modes cut from the menus have no tile.
        for cut in [ExperimentId::Planet, ExperimentId::Diorama, ExperimentId::Halls] {
            assert!(c.tiles.iter().flatten().all(|d| d.id != cut), "{cut:?} has a tile");
        }
        assert!(c.tiles[0].iter().all(|d| d.id.category() == crate::experiments::Category::Game));
        assert!(c.tiles[1].iter().all(|d| d.id.category() == crate::experiments::Category::Experiment));
    }
}
