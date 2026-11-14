# Minigames fun audit

An honest look at seven of the original modes in `native/src/experiments/`: what each one is like to play,
what was wrong, what changed in this pass, and which ones are worth keeping. Gem Clash and Star Swarm were
reworked separately and are not covered; Runner, Hundred Marios and the other experiments belong to another pass.

How these were played: headless runs (`JUMPMAN_OFFSCREEN=1280x720`, `JUMPMAN_SCREENSHOT_AT`), each mode's own
demo/bot hooks, and the deterministic cores' simulations in unit tests (whole matches, whole runs, whole races on
the real course collision). A new hook, `JUMPMAN_XP_LOG=<seconds>`, prints the running mode's `debug()` line so a
headless run shows the match state over time. Nothing here was played by a person with a controller; the
"Unverified" section at the end lists what that leaves open.

The user's two standing complaints were the yardstick: modes that feel "way too light and boring", and modes
that play in made-up arenas instead of the real game levels.

## Summary table

| Mode | Before | After this pass | Real level? | Keep? |
| --- | --- | --- | --- | --- |
| Hold the Gate | Solid tower defence on a walled, made-up meadow | Same rules on the real castle grounds | Yes (new) | Keep |
| Star Deck | Balanced deckbuilder on a plank platform in a void | Fights on a level patch of the real castle lawn | Yes (new) | Keep |
| Last Star Standing | Battle royale on real BoB, nearly nothing happened for 2 minutes | ~4 min matches, eliminations from minute one (Easy no longer slows bot fights), streak and final-field calls | Yes | Keep |
| Summit Race | Rivals stuck or warping; a lone rival needed 4 minutes | A real race: Pro field finishes in 60-78 s, classes, splits, unlocks | Yes | Keep |
| Relic Descent | Deep ARPG in procedural box dungeons, harsh first minutes | Contextual first-run lessons; floors in real OoT dungeons (follow-up) | Yes (follow-up) | Keep |
| Sunstone Shindig | Wide board-party mode, mostly tap/tile minigames on flat slabs | Unchanged | No | Rebuild or cut |
| The Painted Halls | 16-room metroidvania in box rooms | Unchanged | No | Cut |

## Ranking after this pass (most fun first)

1. **Hold the Gate**: the clearest goal and the best escalation (15 waves, bosses every fifth, lives at
   stake), and Mario matters (punch, dive, ground-pound, rallying towers). Now on Peach's real lawn.
2. **Star Deck**: the most decisions per minute and the best-tested balance (bot win rates 33-67% per hero,
   daily seed, best score). Real lawn behind the cards now.
3. **Last Star Standing**: real BoB, real physics fights, loot; after the pacing fix it has the right arc
   (drop, loot, herding, final duel) in about four minutes.
4. **Summit Race**: after the AI rewrite it is a genuine race up BoB with a field that strings out and a ladder of
   classes. Short and replayable; one route only.
5. **Relic Descent**: the deepest systems (loot, skill tree, elites, a three-phase boss, meta stash), let down by
   anonymous brick boxes and a punishing start.
6. **Sunstone Shindig**: lots of content but little of it uses what this project is good at (Mario's physics on
   real courses); most minigames are UI taps on a blue slab.
7. **The Painted Halls**: competent design data, but it is grey boxes, it is Mario-only, and the real game
   already is a better metroidvania (the castle itself).

**Recommended cuts:** move **The Painted Halls** out of the GAMES menu (to EXPERIMENTS, or delete it) unless it is
rebuilt inside the real castle (`castle_inside`). **Sunstone Shindig** should either be rebuilt around physics
minigames on real course spots (see its section) or be moved to EXPERIMENTS; as it stands it reads as a tech demo.

---

## Hold the Gate (`gate*`)

**Core loop.** Build and upgrade six tower kinds on a tile board, call waves early for coins, and fight beside the
towers as Mario (punch/dive/pound hurt foes, standing near a tower rallies it, a pound triggers its overdrive).
15 waves, a boss every fifth, 20 lives, two maps, best wave saved per map.

**Before.** Mechanically the strongest of the seven: real decisions (placement, targeting modes, selling,
when to call a wave), a wave preview, a good HUD, shake on leaks and bosses. But it played on a made-up board:
a checkered meadow in a stone pen floating far from any course. The builder bot held all 20 lives through
wave 7, so the early game is gentle; the late game bites (from wave 10 with 1500 coins the bot lost 8 lives by
wave 15 on the new Twin Fords site, and the core tests `the_final_waves_are_not_free` and
`every_map_can_be_lost_too` cover it).

**Changed.**
- The boards now lie on the **real castle grounds** (`COURSE`, `SITES` in `gate.rs`). The sites were chosen by a
  scan of the course collision (`board_fit` / `tile_ground`: level, walkable, no water, no wall or low ceiling,
  no tree object on a tile) and are re-checked by `the_boards_fit_the_real_castle_grounds`. Castle Lawn
  keeps all 131 build tiles; Twin Fords loses 2 to trees, which become rock in the rules.
- `BoardFrame` (in `gate_core.rs`, unit-tested) turns board-local points into course coordinates (any of four
  quarter turns); `BoardGround` samples the lawn once per map, so foes, towers, projectiles, pickups and the cursor
  follow the real ground height and the road, the build checker and the map's rock plazas are draped over it.
- Mario walks the course's own collision; taps go through the bridge's frame; the camera frames the turned board.

**Character support.** All four (the hero's attacks are read from the shared action ids).

**Open.** The gate itself is a mesh (no collision) and the course's own objects still live around the board.
Early waves are easy for a builder; a "hard" setting or an earlier first boss would help returning players.

## Star Deck (`deck*`)

**Core loop.** A Slay-the-Spire style run: a branching map of fights, shops, rest sites, events and a boss; cards
are Mario moves, energy is *momentum* that grows when you chain Jump, Dive, Pound. Each hero (Mario, Link, Young
Link, Banjo) has its own deck.

**Before.** The best-balanced mode (policy bot wins 33-67% per hero over 600 seeds; an act is about 7 stops),
with a daily seed and best score. Its weakness was presentation: the fights played on a plank platform floating
in a purple void with a ring of trees, which looked like a placeholder.

**Changed.** The stage now lies on a **level patch of the real castle grounds**, found from the collision by
`stage_on` (every actor mark within a few units of the stage height, no water, no tree on the fighting line;
checked by `the_stage_is_level_real_ground`). The course's own sky, lawn, fences and castle are the backdrop; the
custom platform, collision box, fake grass plane and dusk palette are gone.

**Open.** A run is one act; a second act (or ascension-style difficulty) would extend its life. Different fights
could use different real spots (a meadow for early rows, the summit for the boss); the `stage()` indirection makes
that a small step.

## Last Star Standing (`lss*`)

**Core loop.** Glide onto Bob-omb Battlefield with up to 39 AI Marios, loot camps, chests and airdrops (shells,
bombs, flowers, star bursts, caps), fight with Mario's own moves while the Blight closes in; last one standing
wins. Records: best place, wins, difficulty.

**Before.** The match timeline was 18 s of drop plus 334 s of zone phases (almost 6 minutes), and in the real
game the field barely moved at first: a headless run counted 40, 40, 39, 38, 33 fighters alive at 15, 30, 45, 90,
120 s. Two minutes of a 40-player battle royale with five eliminations is what "too light" looks like.

Two causes: a long first hold, and the difficulty setting. New players start on Easy (the records' default),
and Easy scaled every bot's aim, reaction and damage, including bots fighting *each other*, so an Easy match was
also a slow match.

**Changed.**
- The difficulty now sets how dangerous bots are **to you** (`Lss::diff_of`): a bot whose target is the player
  uses the chosen level, bots fighting each other always fight at Normal.
- A tighter Blight schedule (`PHASES`): a 30 s first hold instead of 50, every later phase shorter, slightly
  harsher damage. The match is about 4 minutes. In the game (same seed, idle player) the field now reads
  33, 29, 21, 14 alive at 30, 60, 90, 120 s (it was 40, 36, 33, 30). In the core on the real island the Easy and
  Normal matches both end at 229 s (`a_full_field_on_the_real_island_thins_out`); flat-ground pacing is covered by
  `a_full_field_thins_out_steadily`.
- Stakes that rise: DOUBLE / TRIPLE KNOCKOUT and UNSTOPPABLE toasts for your streaks (6 s window), and
  "TOP 10", "FINAL FIVE", "FINAL DUEL" calls with a sound as the field shrinks.

**Open.** No camera shake or hit-stop on the player's own hits (the mode uses the default follow camera); the
bots near the player run on full physics and loot more slowly than the kinematic ones, so the action near the
player is still lighter than the field average. Worth a hands-on check.

## Summit Race (`race.rs`)

**Core loop.** You and seven rivals race up Bob-omb Battlefield's mountain along Koopa the Quick's route.

**Before.** Broken in practice. Every rival ran the crowd's generic racer brain at full stick toward sparse
checkpoints; they cut corners off ledges, stomach-slid back down slopes and recovered by warping. In a headless
race no rival got past 31% of the route in 86 s, and the existing test's lone racer needed 250 s (a good run is
about 70 s). There was no rubber band toward the front, no difficulty, no reason to race twice.

Root causes found on the real collision:
- The route planner's walkability check ignored the physics' surface classes: BoB's grass ramps are "noise
  slippery" (0x2a) and the mountain foot is "slippery" (0x14), which Mario slides down past 20 degrees, not 38.
  A probe Mario running straight up the foot stalls and slides back every time. `crowd_route::walkable_normal`
  now uses the same classes as `mario_get_floor_class` (this also fixes plans for other crowd users).
- With correct classes there is **no walkable connection** between the meadow and the mountain path in this
  port's static collision, so no planner can produce a runnable route there.

**Changed.**
- A dedicated race brain (`RivalBrain`, engine-free and unit-tested) drives the crowd bodies through a new
  `CrowdAgent::manual` input: pure pursuit on a densified rail, long jumps on straight level stretches, shoving
  past racers alongside, a jump at walls, a Lakitu-style pick-up when a rival falls off or stalls (it loses time;
  a rival that fails the same ledge three times is set past it), and a soft rubber band toward the player.
- **Lakitu lift**: the one unclimbable stretch (meadow to mountain path) is marked by the planner; whoever reaches
  its foot, player or rival, is carried to the top and loses the same 1.7 s. Fair, deterministic, and the race
  stays on the real course.
- **Classes**: Rookie, Pro, Champion (speed spread, long-jump habit, Champions shove and throw punches). Winning
  a class unlocks the next (saved), best time, best place and best splits are kept per class.
- **Feedback**: split times at each quarter against your best, "UP TO 3RD" / "DOWN TO 5TH" toasts, the leader's
  gap on the status line, countdown beeps, "X TAKES THE SUMMIT", podium sounds. Rivals keep racing after you
  finish so the final order is real.
- Results on the real course (`a_pro_field_climbs_the_real_bob_route`, `rookie_fields_are_slower_than_champion_fields`):
  a Pro field finishes between 59.5 and 77.9 s; Rookie winner 77.9 s (mean 83.8), Champion winner 57.1 s (mean 61.0).
  A headless game with the player on the Champion brain (`JUMPMAN_RACE_AUTO=1`, Pro class) finished 4th in 69.1 s,
  1.5-3 s behind the podium.

**Character support.** The player can be any of the four; rivals are always Marios.

**Stuck rivals (follow-up).** A Pro rival was found jumping at the mountainside near the summit (route ~46,100 of 52,818)
forever: its pure pursuit cut a sharp corner of the route into the mountain wall, the jump bonked it back, and the bonk
(a knockback, so "stunned") reset the stall counter, so Lakitu never came. Three more spots cost every rival time: the race line
grazed the tip of a wall wedge at ~31,700 (Mario's 50-unit body cannot pass a line 26 units off the tip), every rival cut the
corner at ~36,400 off the narrow mountain path and ran along the slope below until Lakitu came (~4 s each), and the summit
approach at ~51,700. Fixes:
- Stall is measured on route progress over a window (150 units of progress within 4 s), not reset by any motion, knockbacks
  included; the pick-up check runs even while the rival is knocked about. A rival standing on a layer of the mountain more
  than 600 below the line counts as off the route.
- The race line keeps 100 units from walls (`clear_walls`: each densified point is pushed out of the walls around it at a
  step's and a body's height, only onto the same floor).
- Pure pursuit looks 300 ahead on straights, 240/200 into bends (no more corner cutting off the path); a rival pressed against
  a wall off the line aims back at the line (60 ahead) instead of jumping at the wall; one stuck on something on the line
  sidesteps it.
- Rookie and Pro stick magnitudes trimmed by 3 (the fixed field got about 6 s quicker; this keeps the ladder where it was).

`every_rival_of_every_class_finishes_within_200_s` (3 seeds by default, `RACE_SEEDS=n`) checks the whole field. Six seeds
before: 7 of 84 Pro/Champion rivals and one Rookie never finished (one stuck at 46,100, others at the wedge or at the
summit); after: all 126 finish, slowest 95.2 s. Winners after: Rookie 77.1 s (mean 83.9), Pro 52-65 s (field means
58-62), Champion 50-54 s (field means 56-59). Diagnostics: `RACE_PICKS=1` prints every pick-up, `RACE_TRACE=1 RACE_IDX=n
RACE_EVERY=15` traces one rival, `probe_collision` (ignored) lists the walls near a spot.

**Open.** (The lift is now animated: Lakitu on a cloud carries the racer along an arc that clears the ground, same 50-tick ride for everyone.) One route only (THI's race route would need
the other course loaded).

## Relic Descent (`relic*`)

**Core loop.** A Diablo-style crawler: three procedural floors of rooms whose packs wake when you enter, loot
with rarities and affixes, a twelve-node skill tree with four skills, elites, a three-phase Bomb King, a gold
stash that makes later runs a little stronger.

**Assessment.** The deepest ruleset of the seven, with good juice (sparks per hit, puffs per kill, shake on slams
and bosses, rarity bursts) and a strong test suite (a scripted player wins 29 of 30 seeds). Its problems:
anonymous brick-and-flagstone boxes, and a start that punishes: the in-game demo bot, which swings but never
dodges, died in nine seconds in its first fight at level 1. A new player meets the same pack with only a 14 s
hint line on screen.

**Real levels, argued.** The genre's point is a new layout every run; the rules lean on that (rooms, doors, packs
that wake on entry, a farthest-room exit, tile walls for foe movement and knockback). A real-dungeon version is the
right next step (Deku Tree, Dodongo's Cavern and the Forest Temple are all in the data), but it needs what Gem
Clash got: a region/ground seam, room detection on the real collision and foe movement against real walls. That
is a port of its own, so this pass keeps the procedural floors.

**Changed.** Contextual first-run lessons (`next_lesson`, unit-tested): the first time foes wake you are told how
to attack and dodge, the first hit teaches that the dive is invulnerable, low health with a potion teaches the
potion button, the first skill point and the first item point at the tree and the pack. Each lesson shows once,
four seconds apart, with the right prompt for keyboard, touch or pad, and veterans (a win, or floor 2 reached)
never see them.

**Open.** A gentler first room on floor 1; the demo bot does not explore.

**Follow-up: real dungeons (`relic_real.rs`).** The floors now play in Ocarina of Time's own dungeons, from the player's
ROM import. A run draws one site per floor from the seed: floor 1 in the Deku Tree (the great hollow, the roots below,
the lower halls), floor 2 in Dodongo's Cavern, Jabu-Jabu's Belly or the Forest Temple (two sites each), floor 3 in a boss
lair (Gohma, King Dodongo, Barinade, Phantom Ganon) where the Bomb King waits. A site is an entrance plus a cap on the
scene rooms it takes in; a flood fill over a 110-unit grid on the scene's real collision keeps every standable,
walkable spot (slopes, steps of 70, wall clearance, head room, no wall across a step), groups the tiles into the
scene's own rooms (`roomBounds`; slivers merged, giants halved, 2-8 rooms), puts the exit in the room farthest in
doors and the treasure in a dead end. The rules keep their tile seam (`Dungeon` now carries its size, tile, origin,
per-tile heights and rooms), so packs, chests, knockback, shots and the minimap work unchanged; foes and drops stand
at the floor height of their tile. Mario walks the scene's real collision inside an invisible fence along the edge
of the flood fill (closed doors stay closed), the scene's real model is drawn, and the camera drops lower and closer
under the ceilings (rays against the collision). On a real floor the golden light opens only once the exit room's
guards are beaten (a real exit room can sit close to the entrance). Arrival toasts name the place and the goal; the
first-run lessons are unchanged. Without the import (or with `JUMPMAN_RELIC_PROCEDURAL=1`) the generated floors come
back with a toast saying why. `JUMPMAN_RELIC_SITES=deku_hollow,forest_cellar,phantom_lair` forces the sites.

Tests: every site builds on the imported data, is connected along the foes' tile walk, has open room centres and at
least 20 of 400 sampled spawn spots per room on its own floor, puts the exit and the boss in reachable rooms and builds
the same twice; a run plan is deterministic per seed; toy scenes check ramps, pits, sealed rooms, the room cap, the
fence and the exit lock with the rules running on a built floor. Not verified: a human playthrough of every site, and
Mario's own reach (the flood fill approximates it; the fence keeps him on the plan). Floor sizes run from 434 to 2205
tiles (King Dodongo's lair has two rooms, Gohma's five); some sites are one big scene room halved, since many doors are
walls in the static collision.

## Sunstone Shindig (`party/`)

**Core loop.** A four-seat board game (three boards with a gimmick each), items, duels, a Keeper selling the
Sunstone, bonus awards, and 15 mini-games after each round, against bots or local players.

**Assessment.** Broad and well-built (seeded core, bots per mini-game, practice menu), but most mini-games are
UI games (mash, quick draw, power meter, star count, odd one out, echo pads) on a flat blue slab, and the
boards are coloured discs on a flat green plane. Very little uses Mario's physics or a real level, which is what
makes the other modes feel like this game. A full 12-round party is long for what it offers solo.

**Changed.** Nothing in this pass (time went to the modes that could be fixed most).

**Recommendation.** Either rebuild it around the physics mini-games (Bumper Brawl, Crumble Floor, Coin Scramble,
Shell Lane, Plank Balance) staged on real course spots, with the board laid on a real place (Hourglass Keep is a
castle courtyard: `castle_courtyard` exists), or move it to EXPERIMENTS. The `BoardFrame` / `BoardGround` pattern
from Hold the Gate is the template for laying a board on a course.

## The Painted Halls (`halls*`)

**Core loop.** A 16-room, six-region metroidvania: Mario's moves are the keys (long jump, wall kick, ground
pound, three caps), 13 star fragments, bosses, a map, save rooms. A solver proves every gate needs its ability.

**Assessment.** The design data is careful, but on screen it is grey and brick boxes with a dark void under the
room edges, it is Mario-only, and Super Mario 64's own castle is the better version of the same idea. It is the
weakest fit for "real levels".

**Changed.** Nothing.

**Recommendation.** Cut it from the GAMES menu. If it comes back, it should be the real castle (`castle_inside`)
with its doors and painting rooms as the regions, and the same ability gating.

---

## Tests added or changed

- `race.rs`: classes and records, rubber band, pick-up, a Pro field on the real BoB route, Rookie vs Champion fields.
- `crowd_route.rs`: surface classes match the physics (exercised by the race tests).
- `lss.rs` / `lss_core.rs`: pacing on the real island (Easy and Normal) and on flat ground; the schedule length
  bound; the looting check now expects at least two armed bots (the tighter first hold moves them sooner).
- `gate_core.rs`: `BoardFrame` round trips; `gate.rs`: the boards fit the real castle grounds, the draped mesh.
- `deck_tests.rs`: the stage is level real ground.
- `relic.rs`: lessons come once each, most urgent first.

Tests that need the game data skip themselves when `assets/courses` is absent.

Test run for this pass: `cargo test --release -p jumpman -p jumpman-sim` with the game data linked in. All
experiments tests pass except four that need exports this worktree did not have (`public/mk64` for the kart
tests, Banjo-Kazooie enemy models for `foes_view`); outside the experiments, audio, character and content tests
failed for the same reason (missing exported data). None of those files were touched.

## Follow-up (approved cuts and the Shindig rebuild)

**Menus.** The Painted Halls is hidden from every menu (hub doors, the classic mode cards, the in-game dock list and its
number keys); its code, tests and `JUMPMAN_EXPERIMENT=halls` hook stay (`ExperimentId::listed`, `MENU_IDS`). GAMES now run
in the ranking of both audits: Gem Clash, Star Swarm, Hold the Gate, Star Deck, Last Star Standing, Summit Race, Endless Run,
Sunstone Shindig, Relic Descent, Kart (Endless Run moved up later; the reasoning is in `MINIGAMES_FUN_2.md`, "Menu order").

**Sunstone Shindig, rebuilt on the real Bob-omb Battlefield.** The board core (spaces, items, the Keeper, duels, formats,
awards) is kept; everything around it moved onto the real course, following Hold the Gate's board-on-a-real-course pattern:

- The three boards lie on the battlefield floor (`party/sites.rs`): Meadow Gates on the meadow where Mario starts, Windy Hills
  on the eastern hills, Bob-omb Flats on the flats by the start. Every space is on level, open ground with no tree, crate or
  post on it, every path is walkable and the ceremony spots are clear (a scan picked the sites, a test re-checks them).
- The fifteen UI mini-games (taps, tiles, meters on a blue slab) are gone. Five physics mini-games replace them, each played
  by four real Marios (`CrowdMario`, full SM64 physics) on a real spot, and each built on that spot: **Coin Scramble** on the
  eastern hills' slopes and ledges, **Summit Crown** (king of the hill with punches, the summit's real edges throw you off)
  on King Bob-omb's summit, **Bob-omb Rain** in the field of wooden posts, **Star Tag** on the floating island (fall off and
  the star goes back), **Goomba Stomp** on the high north-west meadow with chained stomps.
- Rules are engine-free and deterministic (`minis/`): a game reads the four bodies and asks for knockbacks, pick-ups and
  bounces; the app plays them on the physics, the tests on a kinematic stand-in, and one test plays all five on the real
  course with full physics.
- Rounds: 6 by default (5 / 6 / 8). With bots and fast board animations a whole game ran 270 s; with normal animations and a
  person's decisions the estimate is 8-12 minutes.

**Honest fun assessment.** Clearly better than before and now a real party game: every mini-game is Mario moving on a place
you recognise, the five ask for different things (collect, shove, dodge, keep away, time a jump), the bots play them
credibly (they fight for the crown, steal the star, chain stomps, flee blasts) and the session is short. It goes to GAMES
after Summit Race. Its limits: solo, the board half is still mostly watching three CPUs roll; five mini-games repeat within a
session (never twice in a row); Summit Crown is often contested (nobody scores while two Marios share the circle); the crowd
Marios run through course objects such as trees; nothing was played by a person with a pad. (The first four are addressed in
the follow-up below.) It is at its best with people on
gamepads, which is untested here.

## Sunstone Shindig follow-up (solo pacing, trees, the crown, variety)

- **Trees.** The crowd Marios step on the course collision only, and BoB's trees (and the Chain Chomp field's wooden posts)
  are objects, so they ran through them. Each arena now carries the solid objects standing in it (`Field::solids`, from
  `sites::solids`: trees with their own 80 x 500 hitbox, posts ~60 x 300): every piece is pushed out of a trunk's cylinder
  after its physics step (as SM64 pushes Mario out of an object, onto floor only, its run checked), on the board too, and the
  bots bend their run round a trunk just ahead (`steer_around`). Starts and coin spots are kept off the trunks. Bob-omb Rain
  has a tree in the middle of its field; Coin Scramble and Star Tag one each. Tests: a kinematic soak of every game with four
  trees never leaves a body in one; the real-physics test of every game asserts no Mario is ever inside a trunk.
- **Solo board.** While a person watches a CPU's turn its animations play at half length (walk steps, think pauses too),
  and a `Fast >>` button (key F) fast-forwards them (0.15). Before each CPU roll the person may call it: HIGH or LOW
  (buttons, keys H / L, an even split of the die: 6-10 / 1-5, Dash 8-10 / 5-7); a right call pays a coin, the CPU waits
  up to 1.6 s for the call. Measured (`cpu_turns_play_quicker_and_a_person_can_call_their_rolls`): 4.0 s per CPU turn
  paced including the call window, 1.4 s fast-forwarded; every CPU roll offers a call.
- **Summit Crown.** A shared circle no longer scores for nobody: the Mario who has stood in it longest keeps the crown
  and scores at half rate (the circle blinks white, the chip shows "(1/2)"); pushing in is how you take it. Twist on half
  the games: the Roaming Crown moves the circle to another spot of the summit every 7 s.
- **Variety.** Mini-games come from a shuffle bag (every game before any repeats, never twice in a row; tested over 40 seeds),
  and Coin Scramble has a Gold Rush twist one game in three. The twist is named on the intro card and the banner.
- Not done: a person has not played any of it; the call-it timing and the paced CPU turns are judged from tests and stills.

## Unverified

- No hands-on play with a controller, keyboard or touch; nothing was run on the iOS simulator or a phone in this pass.
- Hold the Gate: the late waves on the real lawn were only watched with the builder bot; camera framing on phone
  aspect ratios with the turned boards; whether any of the castle grounds' own objects wander onto a board.
- Last Star Standing: the in-game pacing was checked on one seed with an idle player; how the action feels near
  a player who is actually fighting is untested.
- Summit Race: the Lakitu lift is a plain teleport; how it feels to a player, and whether a human finds a faster
  way up than the lift, is untested.
- Star Deck: the stage's camera framing on narrow screens; the course objects near the stage.
