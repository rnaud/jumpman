# Minigames audit 3C: Star Swarm, Summit Race, Sunstone Shindig

An audit of three modes against the checklists in `MINIGAMES_RESEARCH.md` (sections 2, 6 and 8), after the bot and pacing
fixes that came just before it (stall detection, solid trees, HIGH/LOW calls, Fast >>, the shuffle bag, Tyrant meteor
dodging). Each criterion was measured headless where a measurement exists. The highest-impact failures were then fixed and
measured again, and tests pin what could be pinned. A later request added one more requirement: bodies must not pass
through each other in any of the three modes.

How it was measured:

- **Star Swarm**: `audit_swarm_run` (ignored test in `swarm_core.rs`). The scripted kiting player (`kite_skill`, skilled and
  casual, with the meteor dodging from the last pass) plays each hero for 32 seeds on NORMAL in the deterministic core,
  until 7:00 plus a 240 s Tyrant fight, which is the same window as `tune_tier`. It logs level-ups, power spikes (a new
  weapon, an evolution, a chest), the cards of every offer, the live foe count every tick, and Tyrant results. "Before"
  replays the old rules on the same build using the test knobs (`SWARM_VISIBLE=0 SWARM_LULL=100 SWARM_TYRANT_HP=80000
  SWARM_TUNE=1.6,1,1.15,1.25,0.8`). This is the arena core: the real levels' world chests and shrines are not in it, so
  power-spike gaps are an upper bound for a real run.
- **Summit Race**: `audit_race` (ignored, `race.rs`). Seven rivals plus a stand-in player on the back row, all full-physics
  crowd Marios on the real Bob-omb Battlefield. The stand-in is driven by the race brain with the Champion's fastest habits,
  as `JUMPMAN_RACE_AUTO` does. `find_race_shortcuts` and `try_race_line` (ignored) look for faster lines and run them on
  the real physics. A headless game (`JUMPMAN_EXPERIMENT=race JUMPMAN_RACE_AUTO=1 JUMPMAN_RACE_CLASS=1`) checked the
  in-game result.
- **Sunstone Shindig**: `audit_shindig_solo` (ignored, `party/flow.rs`) plays six whole 6-round games with one person who
  acts as soon as the screen asks. It records the time between the person's actions (own choices, calls on CPU rolls,
  playing the mini-game), CPU turn length, real time per round, mini-game length and session length. Two existing reports
  were also run: `balance_report` (Hard against three Easy per mini-game) and `comeback_report` (new, 600 CPU games).

## Star Swarm

| # | Criterion (target) | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1a | First level-up within 20-30 s | 7-14 s mean (max 27 s) | same | Pass. It comes sooner than the band, which is allowed |
| 1b | Level-ups every 15-30 s through 3:00 | Every 7.2-8.7 s. Level 20-26 at 3:00, about 63-71 at 7:00 | Same | **Not fixed** (see below) |
| 1c | First evolution by about 4-5 min | 231-293 s skilled in 10-14 of 16 runs | 253-280 s skilled in 20-25 of 32 runs | Pass |
| 2 | A power spike every 60-90 s (runs with a gap over 90 s before the Tyrant) | Skilled 16/32, 21/32, 15/32 (Mario, Link, Banjo); casual 8, 4, 11 | Skilled 11, 13, 10; casual 8, 11, 14 | Improved for skilled players. The arena core has no world chests or shrines (the real levels have 10-36 chests and 4-9 shrines), so this overstates the gaps. Not verified in a real level |
| 3 | At least 2 of 3 cards per offer visibly change the screen | 32-40% of offers | **71-76%** of offers. Every offer has 2 whenever the build still has 2 such cards | **Fixed** |
| 4 | Kill feedback scales with the crowd | One particle burst per tick (3-12 particles), with no extra shake or flash for a big clear | A tick with 10+ kills adds a white flash ring (500-1300 units, by the number of kills), shake of 0.2-0.9 and a white burst. A 0.06 s beat at 20+ kills | **Fixed** (looks checked on one still; frame time on a clear not measured) |
| 5a | Density lulls after bosses | Live foes 10 s after a boss = 1.05-1.11x the count at the kill | **0.49-0.51x** | **Fixed** |
| 5b | Last 60 s before the Tyrant is the densest | 1.06-1.13x the busiest earlier 30 s | 1.00-1.02x | Pass (the curve now has dips, so the end is no longer above the peak) |
| 6 | Risk shrines pay | Not measurable in the arena core (no shrines) | Not measured | Open |
| 7 | Contact readable (i-frames, 0.7 s telegraphs) | 24 i-frame ticks after a hit (0.8 s), with a flash and shake. Elite and boss wind-ups were not timed | Same | Not re-audited |
| 8 | Run length 7-10 min with the Tyrant; death summary | Tyrant at 7:00, fights of 30-150 s | Same | Pass (summary screen not re-checked) |
| 9, 10 | Meta options, chest jackpot | Not audited in this pass | | Open |

**Tyrant balance and heroes** (share of the runs that reach the Tyrant that kill it; target about 80% skilled, about 50%
casual):

| Player | Setting | Mario | Link | Banjo | All | Reached the Tyrant |
| --- | --- | --- | --- | --- | --- | --- |
| skilled | before (32 seeds) | 21/24 | 16/17 | 24/27 | 90% | 68/96 |
| skilled | **after** | 20/25 | 15/20 | 21/24 | **81%** | 69/96 |
| casual | before | 5/10 | 6/10 | 6/10 | 57% | 30/96 |
| casual | **after** | 8/15 | 8/20 | 7/17 | **44%** | 52/96 |

What changed:

- **Visible cards** (`roll_choices_ex`, `VISIBLE_CARDS = 2`). After an evolution and the new-weapon slots, cards are drawn
  from picks that change the screen until two are in hand: a new or levelled weapon, an evolution, Area, Quantity, the
  Ground Quake and Dive Strike techniques, or the Power Tome (`Upgrade::visible_change`). The rule only applies when the
  build still has two such picks; when slots and weapons are maxed, the hand falls back to tomes. The **Power Tome** now
  draws every projectile 7% bigger per level (`swarm::power_look`, look only), so it changes the screen too.
- **Lull** (`LULL_TICKS` 12 s, `LULL_SHARE` 45%). After a King or Chomp dies, the director keeps less than half its usual
  crowd for 12 s.
- **Big clear** (`BIG_CLEAR` 10 kills in one tick). The core adds a flash ring and sets `ev.big_clear`; the host adds
  shake, particles and a short beat.
- **Retune.** More weapon levels come at the cost of defensive tomes, so with the Tyrant unchanged skilled bots reached it
  less often (23/48) but won every fight they reached. NORMAL is now hp 1.6 -> 1.5 and contact damage 1.25 -> 1.1; HARD and
  NIGHTMARE are scaled by the same ratios but not re-measured. The Tyrant's base health goes 80000 -> 130000. Sweeps tried
  125000 (skilled 84%, casual 58%) and 135000 (74% / 65%); 130000 is in between (81% / 44%). The sweeps are noisy: 16-25
  runs per cell, and two settings can move a cell by 3-5 wins.
- **Hero balance.** Banjo is no longer near-certain against the Tyrant (24/27 -> 21/24). Link is still the weakest: he
  reaches the Tyrant least (20/32 skilled) and wins 75% of his fights. Hero kits were not changed.

**Why the level-up cadence was not changed.** The research band asks for a level-up every 15-30 s; ours comes every 7-8 s.
Two steeper XP curves were measured. `xp_to_next = 5 + 3l + 2.8l^2` (the band) gave level 22 at 7:00, but skilled bots
reached the Tyrant in 3-4 runs of 16 and casual bots died at 1:40. `5 + 3l + 1.2l^2` only stretched the gap to 10-11 s
and still dropped skilled reach to 25/96. Reaching the band would need every pick to be much stronger (a power redesign),
not a curve change. The gap stays at 7-8 s; the visible-card rule is the cheaper answer to the same complaint, that each
level-up should mean something. `SWARM_XP_CURVE=a,b,c` stays as a test knob for a later pass.

Tests: `offers_show_two_visible_cards_while_the_build_has_them` (800 offers over random builds),
`a_fallen_boss_brings_a_lull` (a kill thins 130 live foes to 61 in 10 s, against 137 without one),
`a_big_clear_flashes_and_a_single_kill_does_not`. `a_kiting_player_reaches_the_final_boss` now needs 2 of 8 instead of
3 of 8. That test uses a hero with no kit (stars only), which the visible-card rule leaves with fewer defensive tomes; the
real heroes are measured by the audit.

Body collision (the extra requirement): Star Swarm already had `Swarm::body_block`, pinned by
`the_hero_never_clips_into_foes_even_at_top_speed`. It was not changed.

Screenshot: `swarm_power.png` (in the session scratchpad `audit_c/`): Power Tome 8 stars on the real level, about 1.56x
their old size.

## Summit Race

| # | Criterion (target) | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1a | Pro: 1st-4th under 5 s in most races | 3/6 (mean 5.8 s) | **6/8** (mean 4.3 s) with solid bodies; 7/8 (3.2 s) before the bodies were made solid | **Fixed** |
| 1b | Player's place changes 3+ times | Pro 16.8, Champion 29.7, Rookie 1.8 | Pro 19.2, Champion 24.9, Rookie 3.4 | Pass (Pro, Champion); Rookie is a ladder rung the player is meant to win |
| 2 | A risky shortcut that saves 2+ s | Not found as intended. Found instead: the gold-dot route followed Koopa's loop over the plateau, and **a Mario running straight down the slope from the grid reached the meadow in 4.3 s against about 10 s round the loop**, with no risk. Every rival took the loop, so anyone who cut it won by about 6 s | The route skips the loop. The remaining cuts the finder reports save under 1 s (walks across the meadow). No line on the mountain saves anything | **Free cut removed; risky shortcut not built** |
| 3 | A rival within 15 m 70%+ of the time (Pro) | 80% | 85% | Pass |
| 4 | Splits with colour and sound | Present (not re-checked on a phone) | | Not re-audited |
| 5 | Contact staggers but rarely knocks you off | Not measured | | Open |
| 6 | Lakitu lift under 2 s, the same for everyone | 1.7 s for everyone | Same | Pass |
| 7 | Instant restart | R / Race again restarts at once | | Pass (not timed) |
| 8 | Class ladder (Rookie in 3 tries) | The stand-in player wins Rookie 6/6 by about 10 s | 8/8 | Pass |
| 9 | Ghost of your best | None | None | Open |
| 10 | Race length 55-90 s | Stand-in player 54-58 s; Pro field 52-65 s | **Stand-in 46-50 s; Pro field 43-58 s; Rookie winner 67 s** | **Now short for a fast player** (consequence of #2). A first human run is likely about 1:00 |

What changed:

- **Route** (`race_course`, `SKIPPED_LOOP`). Koopa the Quick's checkpoints 1-7, his detour over the plateau east of the
  start, are left out. The race starts down the slope to the meadow, then goes to the Lakitu lift and up the mountain.
  `find_race_shortcuts` compares straight lines against the route using the planner's own walk check, plus a single
  long-jump gap. Floors only, so it is permissive. `try_race_line` runs the candidates on the real physics. Plateau lines
  were either blocked by a wall at its edge (walking: stuck at (-1070, 768, 1345)) or became the long-jump drop we no longer
  need.
- **Pro pack.** The Pro rivals' stick spread is tighter (61.5-57 instead of 61-55). A soft rubber band acts near the
  player: x0.95 when more than 12 m ahead, +3 stick when more than 15 m behind. The strong bands beyond 25 m and 35 m are
  unchanged. `the_rubber_band_eases_leaders_and_pushes_stragglers` still holds.
- **Solid racers** (`apart_moves`, `keep_racers_apart`). The crowd physics stepped every rival alone, so rivals ran
  through each other and through the player. After each crowd step, racers closer than 2 x 37 units (within 150 in
  height) are pushed apart, half each. A rival pressed into the player takes the whole push; the player's own physics is
  never moved. Pushes only go onto floor at about the same height. A racer hanging from or climbing a ledge is left to its
  action. `racers_never_pass_through_each_other`: worst overlap in a Pro race is 1 unit (it was 35 before the ledge rule).

In-game check: Pro with the stand-in player won in 46.1 s, and the next rival finished 1.2 s later
(`race_75s.log`/`.png`). `race_9s.png` shows the new start down the slope with the gold dots.

Not done: the risky shortcut. The research wants a line that saves 2+ s and costs more if missed. On this course and with
this route, the only such lines were the free cut above. A real one would need new route design (for example a cannon or
a wing-cap skip), which is left open. In the simulation the stand-in player was sometimes picked up by the stall rescue
in the pack after bodies became solid (up to 6 times in one seed). A human gets no pick-ups.

## Sunstone Shindig

| # | Criterion (target) | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1a | CPU turn under 4 s by default | 2.7 s if the person calls every roll; **4.4 s if they never call**; 1.7 s with Fast >> | 2.7 / **3.4** / 1.6 s | **Fixed** |
| 1b | The person acts at least every 15 s | Calling: longest wait 5.5 s. Never calling: mean 5.0 s, longest 18.9 s, **14 waits over 15 s in 6 games** | Never calling: mean 4.2 s, longest 15.6 s, **1 wait over 15 s** | **Fixed** (one 15.6 s wait left) |
| 2 | A mini-game every 60-90 s | A round takes 48-54 s | 49-51 s | Pass |
| 3 | Mini-game 20-45 s | 30-35 s | Same | Pass |
| 4 | Skill wins: Hard against three Easy 70%+ | Coin 98%, Crown 60%, Rain 96%, **Star Tag 34%**, Stomp 100% | Coin 92%, **Crown 65%**, Rain 97%, **Star Tag 91%**, Stomp 100% | Star Tag fixed; Summit Crown still short |
| 5 | Comeback: last after round 3 wins 10-25% | Not measured | 9.4% of 600 CPU games | Just short; not changed |
| 6 | No mini-game more than twice in 6 rounds | Shuffle bag (earlier pass) | Same | Pass (pinned by `every_mini_game_comes_up_before_any_repeats`) |
| 7, 8, 10 | Board decisions, staged social beats, real gamepads | Not audited in this pass | | Open |
| 9 | Session 8-12 min | 5.5-6 min with a person who acts within 0.3 s | Same | Probably in range for a real person; not verified |

What changed:

- **Calls nobody makes** (`CALL_IGNORED` 2, `CALL_SECS_IDLE` 0.5 s). After two CPU rolls in a row with no call, the CPU
  waits 0.5 s instead of 1.6 s for one. The next call restores the long window. Test:
  `a_person_who_never_calls_is_not_kept_waiting` (CPU turn under 4 s, no wait over 15 s, 3 seeds).
- **Star Tag skill.** The holder runs at 0.7 + 0.3 x skill of a full stick (Hard against Easy went 34% -> 55%). Once bodies
  were solid, the pile-on round the holder made the star a lottery (15%). Chasers now punch with a chance that grows with
  skill (0.03 + 0.35 x skill^3 per step), which gives 91%.
- **Solid bodies** (the extra requirement). The four mini-game Marios are pushed apart when closer than 2 x 37 units within
  100 in height (`separate_bodies`). This runs on the real physics after the step and again after the rules put someone
  back (`keep_apart`), and on the kinematic stand-in. A push that would leave the floor or enter a trunk is not made, and
  the other body takes it. Goombas are pushed out to 80 units from a Mario standing on the ground (`GOOMBA_BODY`), so they
  no longer walk through him in the 1.2 s after a bump. Bob-ombs only fall and blast, so nothing changed there. Worst
  Mario-Mario overlap on the real battlefield: 0-9 units per game. Tests: `marios_do_not_pass_through_each_other`
  (kinematic, every game, 4 seeds, under 10), `goombas_do_not_walk_through_marios`,
  `separating_bodies_pushes_overlaps_apart_half_each`, and the real-physics test now asserts overlap under 15.

Not done: Summit Crown skill (65%) and the comeback rate (9.4%) are just under their targets. The board pieces (on spaces,
off the mini-games) were not given body collision, because they stand on four fixed slots per space.

## Tests and limits

- Full suite (`cargo test --release -p jumpman -p jumpman-sim`, sources touched first) after all changes: 2306 passed,
  0 failed, in 55 test binaries (jumpman lib: 1702 passed, 50 ignored).
- Not verified: no human play, no gamepad or touch, no iOS simulator run, no phone aspect check. The Swarm numbers come
  from the arena core and bots, not the real levels (chests and shrines). Frame time on a big clear was not measured.
  HARD and NIGHTMARE were retuned by ratio only.
- Screenshots and logs (session scratchpad, not in git): `audit_c/swarm_power.png`, `audit_c/race_9s.png`,
  `audit_c/race_75s.png` and `.log`, plus the raw audit outputs `before32.txt`, `after.txt`, `race_before.txt`,
  `race_after.txt`, `shindig_before.txt`.
