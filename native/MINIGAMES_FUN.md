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
| Relic Descent | Deep ARPG in procedural box dungeons, harsh first minutes | Contextual first-run lessons | No (argued below) | Keep, port later |
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

**Open.** The lift is a teleport with a toast, not an animated Lakitu. One route only (THI's race route would need
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

**Open.** The real-dungeon port; a gentler first room on floor 1; the demo bot does not explore.

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

## Unverified

- No hands-on play with a controller, keyboard or touch; nothing was run on the iOS simulator or a phone in this pass.
- Hold the Gate: the late waves on the real lawn were only watched with the builder bot; camera framing on phone
  aspect ratios with the turned boards; whether any of the castle grounds' own objects wander onto a board.
- Last Star Standing: the in-game pacing was checked on one seed with an idle player; how the action feels near
  a player who is actually fighting is untested.
- Summit Race: the Lakitu lift is a plain teleport; how it feels to a player, and whether a human finds a faster
  way up than the lift, is untested.
- Star Deck: the stage's camera framing on narrow screens; the course objects near the stage.
