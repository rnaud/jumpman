# Minigames fun audit, round 2

Scope: the seven Mario-physics modes in `src/experiments/` (Endless Run, A Hundred Marios, Shrink Ray, Tiny Planet,
Mario vs. God, Across Dimensions, Diorama) and the Star Swarm follow-ups. Each mode was played headlessly
(`JUMPMAN_OFFSCREEN=1280x720`, `JUMPMAN_WALK=1`, the new `JUMPMAN_XP_LOG=<secs>` hook that prints the mode's status line,
toasts and debug state) before and after the changes, and the rules are covered by unit tests. A walk bot is not a
player: the notes below say what was measured and what is judgement.

## Verdicts at a glance (after the changes)

| Rank | Mode | Real game? | Course | Verdict |
| --- | --- | --- | --- | --- |
| 1 | Endless Run | yes | generated (on purpose) | Keep. A complete arcade loop: one fall ends it, daily seed, ghost, records. |
| 2 | Mario vs. God | yes (new) | real Bob-omb Battlefield | Keep. The star ladder turns the sandbox into a build puzzle with a score. Needs a mouse or patience on touch. |
| 3 | Across Dimensions | light | real Bob-omb Battlefield | Keep. Clever and pretty; the timed summit with medals gives it a reason to replay. |
| 4 | A Hundred Marios | light (new) | real Bob-omb Battlefield | Keep as a spectacle with a race goal; the crowd is too slow to be a rival (see below). |
| 5 | Shrink Ray | light (new) | real Bob-omb Battlefield | Keep or merge into Dimensions as a "size" switch. Tiny is a great climb, Giant is a joke by design. |
| 6 | Tiny Planet | weak | generated sphere | Cut, or rebuild around a real course. A tech demo with a coin rush bolted on. |
| 7 | Diorama | no | real Bob-omb Battlefield | Cut from the mode list (or keep as a "toy" in the Lab). It is a toy, not a game: its goal completes itself. |

## Per mode

### Endless Run (`runner.rs`, `runner_gen.rs`)

- **Core loop**: auto-run along +X, steer, jump gaps and hurdles, dodge pillars and boulders. Real SM64 physics.
- **Goal / stakes**: one fall ends the run; score in metres; daily seed with a ghost of your best daily run; random mode.
- **Before**: the difficulty ramp took 55 segments (about two minutes) and the first theme ran for 16 segments; a walk bot
  that only taps jump covered 214 m in 40 s without a fall (measured). No feedback while running apart from the HUD line;
  passing your best was silent.
- **After**: the ramp is 40 segments, themes change at segments 12 and 26 (the brick fortress shows up about 40 s
  earlier); a toast and a coin chime every 100 m; a star chime and "PAST YOUR BEST" / "GHOST BEATEN" the moment the run
  beats the stored best. The scripted-player test still runs 14 segments of three seeds on the faster ramp (it now jumps
  a little earlier at gaps and short-hops onto short islands, as a player would).
- **Generator vs. real course**: keep the generator. The mode's whole point is "endless" and "same track for everyone
  today"; a real course ends, cannot be streamed forever, and its geometry was never built for an auto-runner. Real
  textures (grass, brick, rainbow) already tie it to the game.
- **Still weak**: Mario's top speed is the SM64 cap, so the run never gets faster, only denser; coins do not feed the
  score. Ideas: a coin multiplier on the score, a late "lava rises behind you" pressure.
- **Session**: 1-3 minutes a run, instant retry. Good.

### Mario vs. God (`god.rs`)

- **Core loop**: play Mario while placing blocks, ramps, springs, planks, portals, Bob-ombs on the real Bob-omb
  Battlefield (real collision, undo/redo, gravity and slow motion).
- **Before**: one goal, the star on top of a staircase the mode builds for you. A walk up four blocks wins; zero tools
  needed (the shipped test proved exactly that). Sandbox with a token goal.
- **After**: a ladder of five stars. Star 1 is the staircase (the tutorial). Stars 2-5 float 650, 950, 1350 and 1800
  units over the real ground, 1400 units on from the last one and turning each time, out of a plain jump's reach: the
  player builds up to them. Each star is scored by the pieces placed for it (gold / silver / bronze pars 1-2-4 up to
  4-6-9), the HUD line shows the medals so far, and the best ladder time is kept (`jumpman.records.god.ladder.time`).
  Unit test: the rungs rise, keep their spacing and stay above jump height.
- **Unverified**: the pars are design guesses (no build bot); a placed star may land over a cliff edge where the real
  terrain helps. Touch players get the "Place" button only (no drag), which makes the higher stars slow.

### Across Dimensions (`dimensions.rs`)

- **Core loop**: one Mario, one level, four cameras (3D, 2D side with a clipped cross-section, top-down, 2.5D path that
  follows the mountain route); switch any time.
- **Before**: "reach the summit in any dimension", timer shown at the end only, nothing kept.
- **After**: the climb is against the clock: live time and the gold par in the HUD line, gold / silver / bronze at
  1:15 / 1:50 / 2:50, the best time kept, "NEW RECORD (was ...)" on the finish.
- **Unverified**: the pars are guesses; the crowd's racer personality needs 250 s for the route (measured), a human in
  3D should need about a minute. Switching dimensions gives no score bonus yet (an idea: a "flat run" medal for 2D-only).

### A Hundred Marios (`hundred.rs`, `mario_crowd.rs`)

- **Core loop**: a crowd of up to 100 real-physics Marios with five personalities floods Bob-omb Battlefield; inspect,
  watch, punch them.
- **Before**: "climb to the summit through the crowd" with the crowd already spread everywhere; "N Marios were there
  before you" counted their lifetime summits. Mostly a spectacle.
- **After**: a race. Entering and `R` put you and the whole crowd back at the foot together; your place is one plus the
  crowd Marios who summit first in this race; live race clock and "already up" count; best place and best time kept;
  the medal is for the time (1:10 / 1:40 / 2:30).
- **Still weak**: the crowd's racers need about four minutes (measured: 250.6 s for the racer test), so a decent player
  always wins first place. The crowd is an obstacle course (44 knockbacks in the first 10 s of a walk), not a rival. A
  faster "rival" personality would make the place matter.

### Shrink Ray (`shrink.rs`)

- **Core loop**: the same Mario in Bob-omb Battlefield scaled x8 (tiny), x3, x1 or x1/4 (giant).
- **Before**: summit at any size once and the mode said "COMPLETE".
- **After**: the goal is every size; each climb is timed from the foot (`R` resets the clock), the best time per size is
  kept and shown, and the finish toast says how many sizes are left. Tiny is a long, real climb; Giant is a few steps by
  design (it is the joke of the mode).
- **Verdict**: a fun ten minutes, thin after. Could fold into Across Dimensions as a "size" axis.

### Tiny Planet (`planet.rs`)

- **Core loop**: run around a 2200-unit sphere; gravity follows you round.
- **Before**: one lap (16.1 s for a walk bot, measured) and done.
- **After**: the lap is the warm-up; the real goal is a coin rush (all 51 coins, the ones on pillar tops included)
  against the clock; best lap and best rush kept; "N COINS LEFT" countdown at the end.
- **Verdict**: still a tech demo on a generated ball with flat colours. The user prefers real courses; there is no good
  way to wrap a real course round a sphere. Recommend cutting it, or keeping it in the Lab as a toy.

### Diorama (`diorama.rs`)

- **Core loop**: orbit a miniature Bob-omb Battlefield, poke Marios and Goombas, drop coins, shake the case.
- **Finding**: the goal ("the Marios collect 30 coins") completed in 24.4 s with no input at all (unit test and the
  headless run both). Nothing the player does matters to it. This is a toy, not a game.
- **Recommendation**: cut it from the mode list, or keep it under the Lab as an explicit toy with no goal text. Not
  changed in this round.

## Star Swarm

- **a) Chests**: the scripted walker now reaches 62 of 64 sampled chests (8 per level, `SWARM_BOT_CHESTS=8`), was 56/64.
  Fixes: the walker skips zigzag waypoints it already passed, replans from where it stands after a fall or when stuck,
  lets go when a jump left it hanging from a ceiling (the four Bob-omb chests: it jumped off the narrow ramp near the
  start and grabbed the hangable underside), and only jumps for steep rises (no jumps on ramps); the planner refuses a
  step between two cells when a wall stands between them at Mario's wall-check heights (fixed the Kokiri one). The two
  left (Graveyard chest 9, Koopa Troopa Beach chest 0) end with the walker pushing into something the planner's probes do
  not see; no object is near. Unresolved.
- **b) Exits**: the SM64 warp objects were already disarmed; the Zelda scenes were not. Hyrule Field's exit floors
  (market, Kakariko, Lon Lon, Zora's River x2, Gerudo Valley, Lost Woods, Lake Hylia x2), Kokiri Forest's houses / Lost
  Woods / Deku Tree and the Graveyard's exits use the same exit floors and would have ended a run (tested on Hyrule). New `Experiment::keeps_course` (Swarm returns
  true) turns the scene's exit floors off. Tests: every Hyrule exit floor travels in normal play and does not with the
  flag; every battlefield's warps and fading-warp pads leave the course alone with the warps disarmed.
- **c) Effects quality**: Options > Display > MINIGAME EFFECTS (Off - flat / Low / High - full N64 look), saved in
  settings.json (`effects`), applied live through `fx::n64::set_quality`; `JUMPMAN_FX_QUALITY` still wins for test runs.
- **d) Difficulty**: kiting bots (`tune_tier`, cap 300, no rank perks). Before: casual bots reached and killed the Tyrant
  in 8 of 8 seeds at level ~78. NORMAL is now hp x1.6, spawn x1.15, damage x1.25, xp x0.8; Banjo starts with a second
  star level (he fell behind once the swarm bit). 16 seeds:

  | Tier | Player | Mario | Link | Banjo |
  | --- | --- | --- | --- | --- |
  | NORMAL | casual | 9/16 | 9/16 | 9/16 |
  | NORMAL | skilled | 12/16 | 12/16 | 13/16 |
  | HARD (8 seeds) | skilled | 2/8 | 3/8 | 3/8 |
  | HARD (8 seeds) | casual | 1/8 | 1/8 | 2/8 |
  | NIGHTMARE (8 seeds) | skilled | 1/8 | 0/8 | 0/8 |

  Casual runs that fail die around 2-4 minutes; the bots never dodge and use no rank perks, so humans with ranks will do
  better on HARD / NIGHTMARE. A bot that reaches the Tyrant also kills it: the Tyrant itself may need more health.
- **e) Solid foes**: `Swarm::body_block` sweeps the hero's step (last position to this one) against every foe body low
  enough to bump into (a jump still clears small ones): light foes are shoved off the path, heavy ones (bosses, elites,
  radius >= 100: brutes, kings, chomps) stop him at their surface; a few passes resolve being pressed between bodies, and
  a hero wedged between two heavy bodies stays where he stood. Bodies are 0.8 of the contact radius, so touching still
  hurts; a hit knocks the light foes around him back 70 units (no pinning). The host moves Mario by the push only onto
  walkable, level ground. Test: 600 ticks at 90 units a tick weaving through 260 foes: no overlap, no foe crossed in a
  step, stopped by heavy bodies 269 times; 10 us per whole tick with 260 foes.
- **f) Icons**: `swarm_icons.rs` draws a 16x16 pixel icon for each of the 38 upgrades in code (original art, no files):
  red frame weapon, blue tome, green technique, gold evolution (the base weapon's picture, brighter, with a sparkle).
  Shown on the level-up and chest cards (with the category name and level pips) and before every build-list row.

## Follow-up

Approved: Tiny Planet and Diorama are hidden from every menu (hub doors, mode cards, the dock list and its number keys); their
code, tests and `JUMPMAN_EXPERIMENT=planet|diorama` hooks stay. EXPERIMENTS now lists Mario vs. God, Across Dimensions,
A Hundred Marios and Shrink Ray, in that order.

## Menu order

The GAMES list (hub doors, mode cards, the in-game mode list and its number keys) now reads:

1. **Gem Clash** and 2. **Star Swarm**: fixed at the top (the two headline modes, reworked the most).
3. **Hold the Gate**: first in the first audit (clearest goal, best escalation, Mario matters), on the real lawn.
4. **Star Deck**: second there (most decisions per minute, best-tested balance).
5. **Last Star Standing**: real island, real fights, the right arc in four minutes since the pacing fix.
6. **Summit Race**: a real race with classes and splits; one route.
7. **Endless Run** (up from 9th): first in this audit. It is the most complete arcade loop in the project (one fall ends
   it, instant retry, daily seed, ghost, records) and it is all Mario's own physics. It is short, which is a strength in a
   list of modes people dip into, so it goes above the two long, uneven ones below.
8. **Sunstone Shindig**: much better since the physics rebuild, but solo the board half is still watching three CPUs
   roll, and its five mini-games repeat inside a session; at its best with people on pads, which nobody has tested.
9. **Relic Descent**: the deepest systems, but anonymous procedural boxes and a punishing first fight.
10. **Mario Kart 64**: one track and Mario only; a port to admire more than a mode to replay.

EXPERIMENTS keep this audit's ranking (Mario vs. God, Across Dimensions, A Hundred Marios, Shrink Ray). Tiny Planet,
Diorama and The Painted Halls stay hidden. `games_come_first_with_the_headline_modes_on_top` pins the whole order.

## Mode icons

Every mode has its own 16x16 pixel icon, drawn in code (`experiments/mode_icons.rs`, original art in the same style as the
Swarm upgrade icons): a rounded tile in the mode's own colour (no two closer than 60 in summed RGB) and a bold, outlined glyph
(gem, star, gatehouse, cards, parachute, summit flag, running shoe, die, skull, kart, storm cloud, cube, "100", magnifier;
a painting, a dome and a ringed planet for the hidden ones). They are on the hub's door signs and in its dialog box (2x),
on the classic mode cards beside the name, and on each row of the in-game mode list. The door signs use the icon colour.

## Gem Clash pace

The user found Gem Clash slow. Measured on flat ground (`fighters_run_fast_start_at_once_and_turn_on_a_dime`, ticks at 30 Hz):

| Mario | Before | After |
| --- | --- | --- |
| Top speed (units/s) | 520 | 760 |
| Distance in the first second | 496 | 737 |
| Ticks to full speed | 4 | 3 |
| Ticks to run the other way at full speed | 8 | 4 |
| Ticks to stop | 4 | 3 |
| Ticks to face the other way | 6 | 3 |

Link 478 -> 699, Young Link 593 -> 866, Banjo 504 -> 737 (their speed factors are unchanged). Dives are 25% faster to stay a
lunge. Pulling the stick against your motion brakes twice as hard (a snap reversal instead of a skid). No dash button: it
would need a new on-screen button (the in-game HUD is out of scope); the faster dive is the burst.

Retuned to match: Easy bots run at 0.85 of full speed (0.68) and swing a little more often (the novice test drifted to
93-96% wins against them at the new pace and is back at 83%); Normal bots run at 0.86. Course set pieces and monsters move 20% faster and set pieces
come round a sixth sooner, telegraphs unchanged (novice with monsters: Easy 88%, Normal 42%, Hard 17%; was 75/42/8). The camera follows faster (focus 8/s, zoom 11/s out and 3/s in), keeps
a quarter second of every fighter's run in frame, and frames all four fighters (it only kept the first 16 frame points).
Arena sizes are unchanged except the Shifting Sand Pit (below): the box arenas are 1600-1960 wide, which at 760 is a
two-second crossing, Power Stone's scale.

Shifting Sand Pit: the old rectangle (1360 x 880) sat inside the pit with the camera south of it, so the south wall stood
between the camera and the fighters even with the 0.32 lift. The pit is walled on the west, south and east and opens north
onto the dune slope; the region now fills the whole floor up to the walls and a little way up the slope (1440 x 1060, 27%
more room) and is framed from the north (`Region::view_yaw`), so the walls stand behind and beside the fighters.

Bot balance after the change (same tests as before): Hard vs Normal 46-14 (was 47-13), Normal vs Easy 42-18 (42-18), Hard vs
Easy 55-5 (54-6); novice vs two bots Easy 83% (85%), Normal 42% (48%), Hard 19% (19%); heroes vs Mario 49-58% (47-53%).

## Unverified / limits

- Medal pars (God, Dimensions, Hundred) are design guesses, not measured on players.
- God's ladder stars 2-5 were not climbed headlessly (no build bot).
- iOS simulator and desktop windowed runs were not done; everything was headless.
- Graveyard chest 9 and Beach chest 0 remain unreached by the walker.
- No evolution card came up in the headless card screenshots; the evolution icons were checked on the icon sheet.
