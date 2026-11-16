//! What a human goes through against Gem Clash's bots: a scripted player (an ordinary one that walks up and punches with a
//! human's reaction time, or a passive one that only wanders) in seat 0 against the app's default line-up (two bots of the
//! menu's difficulty, monsters, two rounds to win), on the box arenas. Measured: the share of the player's time spent
//! stunned (hurt, knocked down, getting up), the longest stretch without control, damage taken a minute, how often two
//! bots hit the player within the same second, and the bot hits landed while the player was still stunned (combos).
//!
//! `cargo test --release -p jumpman human_experience -- --ignored --nocapture` prints the table;
//! `bots_leave_the_player_room` (always on) keeps the numbers human.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Player {
    /// Walks to the nearest foe, punches in reach (a human's ~0.25 s reaction, not every chance), jumps now and then.
    Ordinary,
    /// Never attacks: wanders between the arena's spots.
    Passive,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Felt {
    pub ticks: u32,
    pub stunned: u32,
    pub longest: u32,
    pub damage: f32,
    /// Blows by the bots (hazards, monsters and lava only count in `damage`).
    pub hits: u32,
    pub bot_damage: f32,
    /// Hits that landed while the player was still in hitstun / knocked down from an earlier hit.
    pub combo_hits: u32,
    /// Hits by a second bot within 30 ticks of another bot's hit.
    pub gang_hits: u32,
    pub deaths: u32,
    pub dealt: f32,
    pub rounds_won: u32,
    pub rounds: u32,
}

impl Felt {
    fn add(&mut self, o: &Felt) {
        self.ticks += o.ticks;
        self.stunned += o.stunned;
        self.longest = self.longest.max(o.longest);
        self.damage += o.damage;
        self.hits += o.hits;
        self.bot_damage += o.bot_damage;
        self.combo_hits += o.combo_hits;
        self.gang_hits += o.gang_hits;
        self.deaths += o.deaths;
        self.dealt += o.dealt;
        self.rounds_won += o.rounds_won;
        self.rounds += o.rounds;
    }
    pub fn stunned_share(&self) -> f32 {
        self.stunned as f32 / self.ticks.max(1) as f32
    }
    pub fn per_min(&self, x: f32) -> f32 {
        x * 30.0 * 60.0 / self.ticks.max(1) as f32
    }
}

fn stunned(f: &Fighter) -> bool {
    matches!(f.act, Act::Hurt | Act::Down | Act::GetUp)
}

/// The scripted player's input for this tick, deciding on what it saw `REACT` ticks ago.
fn player_input(br: &Brawl, who: Player, seen: &[Fighter; MAXF], rng: &mut Rng, t: u32) -> Input {
    let me = br.f[0];
    let mut inp = Input::default();
    let here = [me.pos[0], me.pos[2]];
    // The nearest live foe as the player saw it.
    let foe = (1..br.n).filter(|j| seen[*j].alive).map(|j| (j, dist2(here, [seen[j].pos[0], seen[j].pos[2]]))).min_by(|a, b| a.1.total_cmp(&b.1));
    match who {
        Player::Passive => {
            let s = br.arena.item_spots[((t / 90) as usize * 7 + 3) % br.arena.item_spots.len()];
            inp.mv = norm2([s[0] - here[0], s[2] - here[1]], [0.0; 2]);
        }
        Player::Ordinary => {
            if let Some((j, d)) = foe {
                let o = seen[j];
                inp.mv = norm2([o.pos[0] - here[0], o.pos[2] - here[1]], [0.0; 2]);
                if d < 150.0 + o.radius() {
                    inp.mv = [inp.mv[0] * 0.3, inp.mv[1] * 0.3];
                    // a few presses a second, not frame-perfect
                    if t % 5 == 0 && rng.chance(0.6) {
                        inp.attack = true;
                    }
                    if me.smash.is_some() && t % 20 == 0 && rng.chance(0.25) {
                        inp.special = true;
                        inp.special_held = true;
                        inp.slot = 0;
                    }
                }
                if me.grounded && t % 45 == 0 && rng.chance(0.3) {
                    inp.jump = true;
                }
            }
        }
    }
    inp.jump_held = inp.jump || (!me.grounded && me.vel[1] > 0.0);
    inp
}

const REACT: usize = 8;

/// One match (or `cap` ticks): the scripted player in seat 0, `bots` bots of `diff`.
pub fn play(who: Player, smash: Option<SmashFighter>, items: bool, diff: Diff, fair: bool, arena: ArenaKind, seed: u64, cap: u32) -> Felt {
    let cfg = Config { seed, humans: 1, bots: 2, arena, diff, rounds_to_win: 2, monsters: Monsters::Few, smash: [smash, None, None, None], smash_items: items, ..Default::default() };
    let mut br = Brawl::new(cfg);
    br.fair_to_humans = fair;
    if let Some(sc) = super::balance_tests::scripts() {
        br.set_scripts(Some(sc));
    }
    let mut rng = Rng::new(seed ^ 0x9e37);
    let mut felt = Felt::default();
    let mut past: std::collections::VecDeque<[Fighter; MAXF]> = std::collections::VecDeque::new();
    let mut streak = 0u32;
    let mut last_hit: [i64; MAXF] = [-1000; MAXF];
    for t in 0..cap {
        if br.phase == Phase::MatchEnd {
            break;
        }
        past.push_back(br.f);
        if past.len() > REACT {
            past.pop_front();
        }
        let seen = *past.front().unwrap();
        let mut inputs = [Input::default(); MAXF];
        inputs[0] = player_input(&br, who, &seen, &mut rng, t);
        let before = br.f[0];
        let dealt0: f32 = br.f[0].dmg_dealt;
        br.tick(&inputs);
        let after = br.f[0];
        felt.dealt += br.f[0].dmg_dealt - dealt0;
        if before.alive && br.phase == Phase::Fight {
            felt.ticks += 1;
            if stunned(&after) {
                felt.stunned += 1;
                streak += 1;
                felt.longest = felt.longest.max(streak);
            } else {
                streak = 0;
            }
            let lost = (before.hp - after.hp).max(0.0);
            if lost > 0.0 {
                felt.damage += lost;
                // A fighter's blow stamps `hit_tick`; hazards, monsters, lava and the like do not.
                if after.hit_tick != before.hit_tick && after.last_hit_by > 0 {
                    let by = after.last_hit_by as usize;
                    felt.hits += 1;
                    felt.bot_damage += lost;
                    if stunned(&before) {
                        felt.combo_hits += 1;
                    }
                    if (1..MAXF).any(|k| k != by && t as i64 - last_hit[k] < 30) {
                        felt.gang_hits += 1;
                    }
                    last_hit[by] = t as i64;
                }
            }
            if !after.alive {
                felt.deaths += 1;
            }
        }
    }
    felt.rounds = (br.f[0].wins + (1..br.n).map(|j| br.f[j].wins).max().unwrap_or(0)) as u32;
    felt.rounds_won = br.f[0].wins as u32;
    felt
}

pub fn sweep(who: Player, smash: Option<SmashFighter>, items: bool, diff: Diff, fair: bool, seeds: u64) -> Felt {
    let mut all = Felt::default();
    for (k, arena) in ARENAS.iter().enumerate() {
        for s in 0..seeds {
            all.add(&play(who, smash, items, diff, fair, *arena, s * 13 + k as u64 * 101 + 7, 30 * 60 * 4));
        }
    }
    all
}

fn row(label: &str, f: &Felt) -> String {
    format!(
        "HUMAN {label:<40} stunned {:>4.1}%  longest {:>4.1} s  dmg/min {:>5.1} (bots {:>5.1})  bot hits/min {:>4.1}  combo {:>4.1}%  gang {:>4.1}%  deaths/min {:>4.2}  dealt/min {:>5.1}  rounds won {}/{}",
        f.stunned_share() * 100.0,
        f.longest as f32 / 30.0,
        f.per_min(f.damage),
        f.per_min(f.bot_damage),
        f.per_min(f.hits as f32),
        100.0 * f.combo_hits as f32 / f.hits.max(1) as f32,
        100.0 * f.gang_hits as f32 / f.hits.max(1) as f32,
        f.per_min(f.deaths as f32),
        f.per_min(f.dealt),
        f.rounds_won,
        f.rounds
    )
}

/// The table (`-- --ignored --nocapture`): the player as Mario (Gem Clash before the Smash combat) and as Smash fighters,
/// with and without the SSB64 items, against Easy (the menu's default) and Normal bots.
#[test]
#[ignore]
fn human_experience() {
    let seeds = std::env::var("JUMPMAN_HUMAN_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    for diff in [Diff::Easy, Diff::Normal, Diff::Hard] {
        for who in [Player::Ordinary, Player::Passive] {
            for (smash, items) in [(None, false), (None, true), (Some(SmashFighter::Captain), true), (Some(SmashFighter::Purin), true), (Some(SmashFighter::Mario), true)] {
                for fair in [false, true] {
                    let f = sweep(who, smash, items, diff, fair, seeds);
                    let label = format!("{:?} {:?} {} items {} fair {}", diff, who, smash.map_or("Mario (Gem Clash)".to_string(), |s| format!("SSB {}", smash::name(s))), items as u8, fair as u8);
                    println!("{}", row(&label, &f));
                }
            }
        }
    }
}

/// Against Easy and Normal bots, a player who is an SSB64 fighter (with the SSB64 items) takes no more from the bots than a
/// player had in Gem Clash before the Smash combat (Mario, Gem Clash's items, no fair-play rules), is rarely hit again
/// while still reeling, and is not traded between two bots.
#[test]
fn bots_leave_the_player_room() {
    for diff in [Diff::Easy, Diff::Normal] {
        let base = sweep(Player::Ordinary, None, false, diff, false, 2);
        println!("{}", row(&format!("{diff:?} before (Mario, no fair play)"), &base));
        for smash in [None, Some(SmashFighter::Captain), Some(SmashFighter::Purin)] {
            let f = sweep(Player::Ordinary, smash, true, diff, true, 2);
            let name = smash.map_or("Mario", smash::name);
            println!("{}", row(&format!("{diff:?} {name}"), &f));
            assert!(f.per_min(f.bot_damage) <= base.per_min(base.bot_damage) * 1.1, "{diff:?} {name}: bots deal {:.0} a minute (Gem Clash's own {:.0})", f.per_min(f.bot_damage), base.per_min(base.bot_damage));
            assert!(f.stunned_share() <= base.stunned_share() + 0.04, "{diff:?} {name}: stunned {:.0}% of the time", f.stunned_share() * 100.0);
            let share = |x: &Felt, n: u32| n as f32 / x.hits.max(1) as f32;
            let (combo, gang) = (share(&f, f.combo_hits), share(&f, f.gang_hits));
            assert!(combo <= share(&base, base.combo_hits) + 0.03, "{diff:?} {name}: {:.0}% of the bots' hits land on a reeling player", combo * 100.0);
            assert!(gang <= share(&base, base.gang_hits).max(0.06) + 0.01, "{diff:?} {name}: {:.0}% of the hits come right after another bot's", gang * 100.0);
        }
    }
}
