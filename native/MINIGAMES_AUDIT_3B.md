# Minigames audit 3B: Relic Descent and the four experiments against the research checklist

The checklists are the ones in `MINIGAMES_RESEARCH.md` (section 0 shared items, section 9 Relic Descent, section 11
experiments). Every number below was measured headless on this branch, on the local ROM import (the real Ocarina of
Time dungeons and Bob-omb Battlefield); nothing was played by a person with a pad. "Before" is the branch point
(`d487e63c`), "after" is this branch. A threshold miss is reported as a miss even where the fix only narrowed it.

How the numbers were taken:

- `relic_audit_tests.rs` (new, a child of `relic_core`): a scripted player on the deterministic core, in two styles.
  *Expert* swings, dodges every telegraph it sees, casts skills, drinks potions. *Novice* swings, dodges one telegraph in
  four, never casts a skill or drinks a potion (the research checklist's "new player who only attacks and dodges
  sometimes"). It walks the floor's tile path room by room, then into the exit room, and takes the stairs.
  `cargo test --release --lib relic_core::audit -- --nocapture` prints every run; `audit_novice_runs` is `#[ignore]`d
  (numbers only).
- `hundred::bob_audit` (new): a scripted Mario on the real BoB collision following the crowd's mountain route (full
  stick, jumps at ledges and walls, a counted warp after 6 s without progress), the crowd racers alone and in a mixed
  100, and the same climb in Shrink Ray's rescaled worlds. `audit_bob_route_times` is `#[ignore]`d (numbers only).
- In-game, headless: `JUMPMAN_RELIC_BOT=1` now drives Mario with the stick on the real physics (it used to slide him
  toward the nearest foe and never explored), `JUMPMAN_XP_LOG=5`, `JUMPMAN_OFFSCREEN=1280x720`,
  `JUMPMAN_SCREENSHOT_AT`, `JUMPMAN_CAMLOG`, `JUMPMAN_SAVE_DIR` (a scratch save, so the bot's gold never reaches the
  real save). `JUMPMAN_RELIC_CAM_OLD=1` keeps the previous camera for A/B runs, `JUMPMAN_RELIC_NO_HITSTOP=1` turns the
  new hitstop off.

---

## Relic Descent (`relic*.rs`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | First fight winnable by a dodge-light novice (9 of 10 seeds clear their first room) | generated floors 5/10, real floors 7/10 | 10/10 and 10/10 | fixed, pinned (`audit_first_fight_is_winnable_by_a_dodge_light_player`) |
| 1b | Novice plays whole runs (numbers only) | wins: generated 2/10, real 0/10 (most died on floor 1) | wins: generated 10/10, real 4/10 (7 of 10 reached the boss lair) | measured; the generated floors may now be too easy for a novice who never uses a skill or a potion: worth a look with a person |
| 2 | First item drop within 60 s | real (expert): 2-19 s; generated: 5-47 s | real 2-15 s | pass (was passing) |
| 2b | First Rare-or-better within 3-5 min | real 3-70 s (one run none); generated 6-105 s | real 4-59 s | pass |
| 2c | Each rarity has a distinct beam/sound across the room | rarity-coloured burst + scale per rarity on drops; one sound (`star_sound`) for every pickup | unchanged | not fixed (no per-rarity sound) |
| 3 | Every enemy attack telegraphs at least 0.5 s; elites and the boss longer | 15 of 23 attacks under 0.5 s: Scamp, Skulltula, Gold Skulltula, Tektite, Grublin, Bigbutt (contact), Snippet, Mutie Snippet, Ticker, Limbo 0 s (no wind-up at all); Guay, Bat 0.27 s; Wraith 0.30 s; Wolfos 0.33 s; Octorok, Mad Scrub 0.40 s; Deku Baba 0.47 s | every attack 0.50-1.00 s; elites +1/3; Bomb King stomp 1.0 s, charge 0.8 s | fixed, pinned (`audit_every_attack_telegraphs_half_a_second`) |
| 4 | Hits have hitstop and sparks, kills a puff and a loot pop | sparks, puffs, shake, sounds; **no hitstop** | hitstop: 1 tick on a hit, 2 on a kill or crit, 3 on a hurt, 0.18 s on a boss phase, 0.3 s on the boss kill (world and Mario frozen, buttons kept) | fixed, pinned (`hits_freeze_the_world_for_a_beat_and_bigger_events_for_longer`); not checked on a pad |
| 5 | A skill point within 3 minutes, visible change when spent | first level-up at 4-22 s (real), 10-22 s (generated) | same | pass |
| 5b | The skill-point lesson must not replace the arrival message | lessons and event toasts called `toast()` directly and cut the floor's name short | lessons wait until the arrival lines have been shown; event toasts queue behind them (at most four) | fixed, pinned (`messages_wait_for_the_arrival_lines`); in-game log order checked (below) |
| 6 | Death progresses the meta | banking rule unchanged (tested) | unchanged | not re-audited |
| 7 | Floors have a goal shape, time to the exit under 4 min per floor; **a bot completes a floor end to end** | the core bot died or stalled on 4 of 10 real runs (6/10 won) and stalled forever on 3 (exit-room guards out of its reach); the in-game bot never left the start | core bot: 10/10 real runs won in the measurement run, 9/10 in the pinned run (one stall: see limits); floors 12-92 s; in-game bot on the real physics clears Deku Tree floor 1 and takes the golden light in 52 s | fixed, pinned (`audit_a_bot_completes_real_floors_end_to_end`, 9/10 and every floor under 4 min) |
| 7b | Floor sizes: King Dodongo's / Phantom Ganon's lairs had 2 rooms (boss straight after the entrance) | lairs 2-5 rooms | every lair at least 3 rooms (entrance, a guard room, the boss) | fixed, pinned (`every_site_builds_a_reachable_floor_on_the_imported_dungeons` now asks 3 rooms of every site) |
| 8 | Loot comparisons one glance | not re-audited | - | not measured |
| 9 | Run length 20-35 min | expert bot on real floors: 1.4-2.9 min; generated: 4.3-6.0 min | real 1.7-2.9 min | **fail** (see below) |
| 10 | Real dungeon recognisable in the first 10 s | arrival toast names the place at 0-4 s | unchanged | pass by toast; not judged by a person |
| C | Camera close to walls in corridors (known weak spot) | same 50 s bot run on Deku Tree lower halls: 33% of frames pinned at the 220-unit minimum, where a nearer wall puts the camera inside it (8 s: a blank brown screen) | pitch tries steeper views first (looks down past corridor walls) and may come in to 90 units: the 8 s frame now shows Mario and the foe beside him; median distance 8.7 -> 9.7 Bevy units | improved; one run measured, frames inside geometry not counted automatically |

### What was changed

- **Telegraphs** (`relic_foes.rs`, `relic_core.rs`). `BITE_WINDUP` (15 ticks) for every biter (Scamp and all the
  swarmers, Skulltulas after they drop, the Snippets): it rears back, and bites only if the hero is still in reach, so
  stepping away or diving makes it miss. `MIN_WINDUP` (16) for the lunges, dives, hops and shots that were faster
  (Deku Baba, Wolfos, Guay, Bat, Octorok, Mad Scrub, Wraith, the Skulltula drop and its shadow). Tektites crouch before
  the hop; Bigbutt's unannounced shove became its charge. Elites wind up a third longer; the Bomb King's stomp marker
  is 1.0 s, his charge 0.8 s. The view already shakes a winding-up foe and switches it to its attack pose.
- **First fight** (`relic_core::warm_up`). The first room woken on floor 1 is a warm-up: at most 5 foes, no elite,
  heavies and bombers (Bruiser, Sapper, Bigbutt, Seer, Lancer) swapped for Scamps.
- **Hitstop** (`relic.rs`, `hitstop_for`). The experiment's `time_scale` is 0 while it lasts, so Mario's physics and
  the rules both stop; `before_step` leaves the buttons pressed meanwhile for the step after.
- **Messages** (`relic.rs`, `say`, `toast_slot_free`). The floor's arrival lines are never cut by a lesson or an event
  toast. In-game log of the bot run, floor 1 to 2: `THE WAY DOWN IS OPEN`, `FLOOR 2: FOREST TEMPLE - THE ENTRY HALL`,
  `FIND THE GOLDEN LIGHT TO GO DEEPER`, then later `LEVEL 3 - SKILL POINT!`, `SKILL POINT! T OPENS THE SKILL TREE`.
- **Bots.** `Dungeon::path` (tile BFS) is shared by the tests and the game. The core bot walks tile centre to tile
  centre (one-tile corridors and staircase bends on the real floors stopped it before) and goes after any awake foe of a
  woken room wherever it hangs back. The in-game bot (`bot_step`) uses the stick on the real collision, fights, dives
  through telegraphs, spends points, equips upgrades, picks up drops, sweeps the rooms, then the exit; it hops and
  veers when pressed against a lip.
- **Lairs** (`relic_real.rs`): three rooms at least on every floor.
- **Camera** (`relic.rs`): pitches tried 0.95, 1.12, 1.28, 0.8, 0.66, 0.52, 0.4, 1.45 (the steeper ones look down past
  a corridor's side walls); the camera may come in to 90 units from the hero instead of stopping at 220 inside a wall.

### Expert runs on real dungeons (after, seeds 1-10)

Floors in seconds; all ten won in the measurement run (`t6`):

| seed | sites | floors (s) | run | first item / rare / level-up (s) |
| --- | --- | --- | --- | --- |
| 1 | deku_hollow, dodongo_hall, gohma_lair | 12, 60, 92 | 2.7 min | 2 / 4 / 7 |
| 2 | deku_hollow, jabu_belly, phantom_lair | 35, 62, 32 | 2.1 min | 10 / 27 / 15 |
| 3 | deku_basement, forest_court, king_dodongo_lair | 38, 38, 33 | 1.8 min | 11 / 11 / 18 |
| 6 | deku_halls, dodongo_hall, king_dodongo_lair | 20, 61, 40 | 2.0 min | 5 / 59 / 7 |
| 10 | deku_basement, dodongo_east, king_dodongo_lair | 29, 55, 40 | 2.1 min | 11 / 15 / 20 |

(The others are in the test output.) Site sizes in tiles: deku_halls 434, king_dodongo_lair 502, phantom_lair 607 up
to dodongo_hall 2145 and gohma_lair 2205; floor times follow (12-40 s on the small Deku sites, 55-92 s on the big ones).

### Still open / not verified

- **Run length** (criterion 9) fails badly on real floors: the expert bot needs about 2 minutes for three floors, a
  human perhaps 3-5x that, still far below 20-35 min. The real sites are 2-5 rooms each. Options: more floors (a
  run of 5-6 sites), bigger room caps on the small Deku sites, or treating Relic Descent as a short-session mode and
  changing the target. Not changed here: a design decision.
- The pinned real-floor test allows one stall in ten: seed 8 (forest_court) stalls when a Wraith kites at ~700 units
  from a spot the scripted hero's circle cannot leave; a person with Mario's real movement (or the Volley skill) is not
  bound by that grid, but it was not checked by a person.
- The in-game bot completes floor 1 on the real physics but died on floor 2 (Forest Temple entry hall) in the one
  180 s run: it plays worse than the core bot (no skills aimed, real knockback). End to end for the whole run was not
  achieved in-game.
- The enemy cast is unchanged (the classic seven + the floor themes); the floor theme is by depth, not by dungeon
  (no Dodongos in Dodongo's Cavern).
- No per-rarity sound; loot comparison, meta progression and phone layout not re-audited. Hitstop not felt on a pad.

Screenshots (scratch, not committed): `bot3_8.png` (old wall-blocked view), `bot3_60.png` (Forest Temple fight),
`cmp_8.png` .. `cmp_32.png` (old camera left, new right), `camnew2_8.png` (new camera at the same moment).

---

## Mario vs. God (`god.rs`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | Tutorial star teaches place/test/undo in 60 s without reading | star 1 is a staircase the mode builds; a walk wins it (tested), so nothing is placed or undone | unchanged | **fail** (not changed) |
| 2 | Two solutions per star | - | - | not measured (needs a person) |
| 3 | Gold pars proven reachable | pars were design guesses | measured: the spring bounce peaks at 1200 units; the cheapest stack per star is 1, 1, 1 and 4 pieces for stars 2-5 against gold pars 1, 2, 3, 4 | pass, pinned (`gold_pars_afford_a_spring_stack_that_reaches_each_star`); stars 3 and 4 have slack (gold could be 1) |
| 4 | Place-test-undo under 3 s, undo on every input | Z / Y and Backspace; touch has Place only | unchanged | not measured on touch |
| 5 | Touch can drag pieces | no | no | **fail** (not changed) |

The pinned test is a stacking proof on flat ground with the real bounce height, not a placed-piece playthrough on BoB
(the real terrain under a star can help or hurt).

## Across Dimensions (`dimensions.rs`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | 3+ spots where a view switch is the fast way | no spot designed for it | unchanged | **fail** (not changed; needs level work) |
| 2 | Switch under 0.5 s with momentum kept | 0.75 s camera blend; the physics never pauses | 0.45 s blend; a test runs Mario, switches him into the side lane and checks he keeps 90%+ of his speed | fixed, pinned (`a_switch_is_quick_and_keeps_marios_momentum`) |
| 3 | Pars measured, not guessed | 1:15 / 1:50 / 2:50 (guesses) | scripted climb from the foot: 129.9 s with 5 warps (about 30 s of those are its warps); pars 1:40 / 2:15 / 3:10 | fixed, pinned (`pars_hold_for_the_scripted_climb_and_the_crowd_leads`: the scripted climb makes silver) |
| 4 | Flat-run medal | no | no | not done |

## A Hundred Marios (`hundred.rs`, `mario_crowd.rs`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | 3+ crowd Marios within 10 s of a good time | solo racer 247 s; mixed 100: first summits 217, 220, 228 s; good time (scripted) about 100-130 s | racers now jump at ledges, steps, gaps and checkpoints above them: solo 228 s, mixed 100: 140, 157, 161, 171 s | improved, **still a miss** (the third rival is ~30-60 s behind a good climb); pinned that 3+ summit inside bronze |
| 2 | Start jostle then string-out | not measured (needs the rendered crowd and a person) | - | not measured |
| 3 | Crowd knockbacks under 1 s per 10 s | 141 crowd-on-crowd knockbacks in 330 s among 100 (no player) | 135 | player cost not measured |
| P | Medal pars measured | 1:10 / 1:40 / 2:30 (guesses) | 1:40 / 2:15 / 3:10 from the scripted climb; the card text says so | fixed, pinned |

## Shrink Ray (`shrink.rs`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | A distinct challenge per size | every size is the same route to the summit | unchanged; measured scripted climbs: Tiny x8 659.5 s (7 warps), Small x3 261.1 s (4), Normal 129.9 s (5), Giant x1/4 55.0 s (2) | **fail** (not changed): the sizes differ only in length; Tiny is an 11-minute walk, Giant under a minute |
| 2 | First "wow" under 10 s | the mode starts at Normal size; Tiny needs a key press | unchanged | not measured on screen |
| 3 | Keeps the timer if merged into Dimensions | not merged | - | n/a |

Recommendation: give Tiny a shorter goal (a landmark a few hundred units away, e.g. the first Goomba's patch) and
Giant a stomp score, or fold the sizes into Across Dimensions as the research suggests.

---

## Tests added or changed

- `relic_core::audit_tests` (new file `relic_audit_tests.rs`): `audit_first_fight_is_winnable_by_a_dodge_light_player`,
  `audit_a_bot_completes_real_floors_end_to_end`, `audit_every_attack_telegraphs_half_a_second`, `audit_novice_runs`
  (ignored, numbers). The real-floor ones skip without the import.
- `relic::tests`: `hits_freeze_the_world_for_a_beat_and_bigger_events_for_longer`, `messages_wait_for_the_arrival_lines`.
- `relic_real::tests::every_site_builds_a_reachable_floor_on_the_imported_dungeons`: 3 rooms for every site.
- `god::tests::gold_pars_afford_a_spring_stack_that_reaches_each_star`.
- `dimensions::tests::a_switch_is_quick_and_keeps_marios_momentum`.
- `hundred::bob_audit::pars_hold_for_the_scripted_climb_and_the_crowd_leads`, `audit_bob_route_times` (ignored).

## Not done in this pass

iOS simulator and desktop windowed runs; any human playtest; phone aspect checks; the shared checklist's retry and
time-to-first-input items were not re-measured for these modes.
