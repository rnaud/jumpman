//! Menu logic (`ActSelector.ts`, `CourseResult.ts`, and the bonus-star save prompt of
//! `Game.ts`). Pure state machines stepped at 30 Hz with the controller snapshot; `ui.rs` draws
//! them with `bevy_ui`.
use jumpman_sim::state::{ControllerSnapshot, A_BUTTON, B_BUTTON};

/// `actChoices`: which of the six mission slots are shown/selectable (star_select.c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActChoices {
    pub first_open: i32,
    pub visible: i32,
    pub selectable: Vec<i32>,
}

pub fn act_choices(collected: &[bool]) -> ActChoices {
    let next = collected.iter().position(|c| !c);
    let first_open = next.map_or(6, |i| i as i32 + 1);
    let last_collected = collected.iter().rposition(|c| *c).map_or(0, |i| i as i32 + 1);
    let visible = first_open.max(last_collected);
    let selectable = (1..=visible).filter(|&act| collected.get(act as usize - 1).copied().unwrap_or(false) || act == first_open).collect();
    ActChoices { first_open, visible, selectable }
}

/// The act selector shown when entering a course painting/warp.
#[derive(Clone, Debug)]
pub struct ActSelector {
    pub course: String,
    pub entry: Option<String>,
    /// Area the chosen act starts in (THI's tiny painting loads area 2).
    pub area: i32,
    pub collected: [bool; 6],
    pub hundred_coin: bool,
    pub best_coins: u32,
    pub choices: ActChoices,
    pub selection: i32,
    pub timer: u32,
    hold_timer: u32,
    last_direction: i32,
    /// Spin angle of the highlighted star (radians), for the view.
    pub spin: f32,
    /// Selection changes since the last frame (for the move sound).
    pub moved: bool,
}

impl ActSelector {
    pub fn new(course: &str, entry: Option<String>, collected: [bool; 6], hundred_coin: bool, best_coins: u32) -> Self {
        let choices = act_choices(&collected);
        let selection = if collected.iter().all(|c| *c) { 1 } else { choices.first_open };
        Self { course: course.to_string(), entry, area: 1, collected, hundred_coin, best_coins, choices, selection, timer: 0, hold_timer: 0, last_direction: 0, spin: 0.0, moved: false }
    }

    fn select(&mut self, act: i32) {
        if act != self.selection {
            self.selection = act;
            self.moved = true;
        }
    }

    /// Select an act with the pointer; returns true when it can be confirmed.
    pub fn click(&mut self, act: i32) -> Option<i32> {
        if self.timer <= 10 || !self.choices.selectable.contains(&act) {
            return None;
        }
        self.select(act);
        self.confirm()
    }

    /// The act to load, once the 10 frame input delay has passed.
    pub fn confirm(&self) -> Option<i32> {
        (self.timer > 10).then(|| self.choices.first_open.min(self.selection))
    }

    /// One tick; returns the confirmed act.
    pub fn step(&mut self, input: &ControllerSnapshot) -> Option<i32> {
        self.moved = false;
        let direction = if input.stick_x.abs() > 45.0 { input.stick_x.signum() as i32 } else { 0 };
        if direction != 0 && direction != self.last_direction {
            let acts = &self.choices.selectable;
            let next = acts.iter().position(|a| *a == self.selection).map_or(-1, |i| i as i32) + direction;
            if next >= 0 && (next as usize) < acts.len() {
                let act = acts[next as usize];
                self.select(act);
            }
        }
        if self.hold_timer == 10 {
            self.hold_timer = 8;
            self.last_direction = 0;
        } else {
            self.hold_timer += 1;
            self.last_direction = direction;
        }
        if direction == 0 {
            self.hold_timer = 0;
        }
        self.spin += std::f32::consts::PI / 16.0;
        let confirmed = (input.button_pressed & (A_BUTTON | B_BUTTON) != 0).then(|| self.confirm()).flatten();
        self.timer += 1;
        confirmed
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveChoice {
    Save,
    Quit,
    Continue,
}

const CHOICES: [SaveChoice; 3] = [SaveChoice::Save, SaveChoice::Quit, SaveChoice::Continue];

#[derive(Clone, Debug, PartialEq)]
pub struct CompletedCourse {
    pub title: String,
    pub heading: String,
    pub coins: u32,
    pub high_score: bool,
    pub key: bool,
}

/// The course result screen: coin tally, "new record", and Save / Save & Quit / Continue.
#[derive(Clone, Debug)]
pub struct CourseResult {
    pub course: CompletedCourse,
    pub shown: u32,
    pub timer: u32,
    pub selection: usize,
    pub options_visible: bool,
    pub buttons_enabled: bool,
    last_direction: i32,
    repeat: u32,
}

/// What a result tick produced.
#[derive(Debug, Default, PartialEq)]
pub struct ResultStep {
    /// Coin counter ticked to this value (play the coin sound; second = record jingle).
    pub counted: Option<(u32, bool)>,
    pub choice: Option<SaveChoice>,
}

impl CourseResult {
    pub fn new(course: CompletedCourse) -> Self {
        Self { course, shown: 0, timer: 0, selection: 0, options_visible: false, buttons_enabled: false, last_direction: 0, repeat: 0 }
    }

    pub fn complete(&self) -> bool {
        self.shown >= self.course.coins
    }

    pub fn show_record(&self) -> bool {
        self.complete() && self.course.high_score
    }

    pub fn confirm(&self, index: usize) -> Option<SaveChoice> {
        (self.options_visible && self.timer > 110).then(|| CHOICES[index.min(2)])
    }

    pub fn step(&mut self, input: &ControllerSnapshot) -> ResultStep {
        let mut out = ResultStep::default();
        if !self.complete() && ((self.timer & 1) != 0 || self.course.coins > 70) {
            self.shown += 1;
            out.counted = Some((self.shown, self.course.high_score && self.shown == self.course.coins));
        }
        if self.complete() && self.timer > 100 {
            self.options_visible = true;
        }
        if self.options_visible {
            if self.timer > 110 {
                self.buttons_enabled = true;
            }
            let direction = if input.stick_y.abs() > 30.0 { -(input.stick_y.signum() as i32) } else { 0 };
            if direction != 0 {
                let go = if direction != self.last_direction {
                    true
                } else {
                    self.repeat += 1;
                    self.repeat >= 10
                };
                if go {
                    self.selection = (self.selection as i32 + direction).clamp(0, 2) as usize;
                    self.repeat = 0;
                }
            }
            self.last_direction = direction;
            if input.button_pressed & A_BUTTON != 0 {
                out.choice = self.confirm(self.selection);
            }
        }
        self.timer += 1;
        out
    }
}

/// The "Do you want to Save?" prompt after a bonus star (100 coins or a no-exit star).
#[derive(Clone, Debug)]
pub struct BonusSave {
    /// 100-coin star: a single yes/no question; otherwise a congratulation page first.
    pub hundred: bool,
    pub page: u32,
    pub choice: usize,
}

pub enum BonusResult {
    None,
    /// Turned to the question page.
    NextPage,
    /// 0 = save (Yes / You Bet), 1 = don't.
    Answered(usize),
}

impl BonusSave {
    pub fn new(hundred: bool) -> Self {
        Self { hundred, page: 0, choice: 0 }
    }

    pub fn question(&self) -> &'static str {
        if self.hundred {
            "YOU'VE COLLECTED 100 COINS! MARIO GAINS MORE POWER FROM THE CASTLE. DO YOU WANT TO SAVE?"
        } else if self.page > 0 {
            "DO YOU WANT TO SAVE?"
        } else {
            "WOW! ANOTHER POWER STAR! MARIO GAINS MORE COURAGE FROM THE POWER OF THE CASTLE."
        }
    }

    pub fn has_choices(&self) -> bool {
        self.hundred || self.page > 0
    }

    pub fn labels(&self) -> [&'static str; 2] {
        if self.hundred { ["YES", "NO"] } else { ["YOU BET", "NOT NOW"] }
    }

    pub fn step(&mut self, input: &ControllerSnapshot) -> BonusResult {
        if self.has_choices() && input.stick_x.abs() > 30.0 {
            self.choice = usize::from(input.stick_x > 0.0);
        }
        if input.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
            if !self.has_choices() {
                self.page += 1;
                return BonusResult::NextPage;
            }
            return BonusResult::Answered(self.choice);
        }
        BonusResult::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(b: u32) -> ControllerSnapshot {
        ControllerSnapshot { button_pressed: b, button_down: b, ..Default::default() }
    }
    fn stick(x: f64, y: f64) -> ControllerSnapshot {
        ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: 64.0, ..Default::default() }
    }

    #[test]
    fn act_choices_match_star_select_c() {
        let c = act_choices(&[false; 6]);
        assert_eq!((c.first_open, c.visible, c.selectable.clone()), (1, 1, vec![1]));
        let c = act_choices(&[true, true, false, false, false, false]);
        assert_eq!((c.first_open, c.visible, c.selectable.clone()), (3, 3, vec![1, 2, 3]));
        // A later collected star keeps earlier gaps visible but not selectable.
        let c = act_choices(&[true, false, false, true, false, false]);
        assert_eq!((c.first_open, c.visible, c.selectable.clone()), (2, 4, vec![1, 2, 4]));
        let c = act_choices(&[true; 6]);
        assert_eq!((c.first_open, c.visible, c.selectable.len()), (6, 6, 6));
    }

    #[test]
    fn selector_moves_and_confirms_after_delay() {
        let mut s = ActSelector::new("bob", None, [true, true, false, false, false, false], false, 0);
        assert_eq!(s.selection, 3);
        assert_eq!(s.step(&press(A_BUTTON)), None, "too early");
        for _ in 0..10 {
            s.step(&stick(0.0, 0.0));
        }
        s.step(&stick(-80.0, 0.0));
        assert_eq!(s.selection, 2);
        assert!(s.moved);
        // Holding the stick does not skip through every act, it repeats after a delay.
        for _ in 0..3 {
            s.step(&stick(-80.0, 0.0));
        }
        assert_eq!(s.selection, 2);
        assert_eq!(s.step(&press(A_BUTTON)), Some(2));
        // Everything collected starts on act 1; first_open caps the choice.
        let s = ActSelector::new("bob", None, [true; 6], false, 0);
        assert_eq!(s.selection, 1);
    }

    #[test]
    fn click_requires_a_selectable_act() {
        let mut s = ActSelector::new("bob", None, [false; 6], false, 0);
        for _ in 0..12 {
            s.step(&stick(0.0, 0.0));
        }
        assert_eq!(s.click(4), None);
        assert_eq!(s.click(1), Some(1));
    }

    #[test]
    fn result_counts_coins_then_offers_choices() {
        let mut r = CourseResult::new(CompletedCourse { title: "BIG BOB-OMB".into(), heading: "COURSE 1".into(), coins: 5, high_score: true, key: false });
        let mut counted = vec![];
        let mut choice = None;
        for tick in 0..140 {
            let input = if tick > 125 { press(A_BUTTON) } else { stick(0.0, 0.0) };
            let out = r.step(&input);
            if let Some(c) = out.counted {
                counted.push(c);
            }
            if out.choice.is_some() {
                choice = out.choice;
                break;
            }
        }
        assert_eq!(counted, vec![(1, false), (2, false), (3, false), (4, false), (5, true)]);
        assert!(r.show_record());
        assert_eq!(choice, Some(SaveChoice::Save));
    }

    #[test]
    fn result_selection_moves_down_and_clamps() {
        let mut r = CourseResult::new(CompletedCourse { title: String::new(), heading: String::new(), coins: 0, high_score: false, key: false });
        for _ in 0..115 {
            r.step(&stick(0.0, 0.0));
        }
        r.step(&stick(0.0, -80.0));
        assert_eq!(r.selection, 1);
        r.step(&stick(0.0, 0.0));
        r.step(&stick(0.0, -80.0));
        r.step(&stick(0.0, 0.0));
        r.step(&stick(0.0, -80.0));
        assert_eq!(r.selection, 2, "clamped at Continue");
        assert_eq!(r.step(&press(A_BUTTON)).choice, Some(SaveChoice::Continue));
    }

    #[test]
    fn bonus_prompt_pages() {
        let mut b = BonusSave::new(false);
        assert!(!b.has_choices());
        assert!(matches!(b.step(&press(A_BUTTON)), BonusResult::NextPage));
        assert!(b.has_choices());
        b.step(&stick(60.0, 0.0));
        assert_eq!(b.choice, 1);
        assert!(matches!(b.step(&press(A_BUTTON)), BonusResult::Answered(1)));
        let mut h = BonusSave::new(true);
        assert!(h.has_choices());
        assert!(matches!(h.step(&press(B_BUTTON)), BonusResult::Answered(0)));
    }
}
