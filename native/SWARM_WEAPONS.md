# Star Swarm: weapons (research note and design)

## What was researched

Megabonk (2025, a 3D roguelite survivor) was read about through public guides and tier lists (standard web
search; nothing was copied, the wording and numbers below are ours). What the genre does that matters here:

- A run is built from a handful of **weapon slots**. Each weapon fires by itself with its own pattern and
  levels up on its own. About 29 weapons exist, grouped loosely by how they hit: swings in front of you
  (sword, katana, big executioner blade), projectiles (bow and sniper that pierce, revolver and shotgun
  spreads, thrown axes and bones that bounce, boomerangs that return, homing rockets), fire (staff fireballs
  that explode, a trail of flames while you walk, a dragon-breath cone), ice (a walker that freezes),
  poison (thrown flasks, a lingering cloud), lightning (strikes that jump between foes), area (an aura
  around you, orbiting rocks, a black hole that drags foes in, a tornado, mines), and defensive ones (a
  shield that blocks and releases shockwaves).
- Separate from weapons are **tomes** (and items): passive stat boosts such as damage, cooldown,
  projectile count, area size, XP, luck, health. They level up as well and apply to every weapon at once.
- Level-up cards mix "new weapon", "weapon level" and "tome level"; the power curve comes from stacking
  tomes on two or three favourite weapons. Weapons have a level cap, and some weapons only shine with
  certain tomes (area for auras, quantity for projectile weapons, cooldown for lightning).
- Status effects create combos between weapons: frozen foes are easy to crush, burning and poison keep
  working after the weapon has moved on, gravity wells herd foes into area weapons.

## Our design (Star Swarm)

Inspired by the genre, built from our own numbers and our own effects.

- **Slots.** Five weapon slots. A run starts with the character's signature weapon: Mario the star shot
  (plus his dive and ground pound techniques), Link the sword, Banjo the boomerang.
- **Cards.** While slots are free and the build is small, most level-up cards offer new weapons. When the
  slots are full only levels of owned weapons, tomes and evolutions are offered. Chests lean toward rare
  cards and a ready evolution.
- **Weapons** (each 6 levels, level changes pattern and effect, not just damage):
  Star Shot, Hero Sword, Spin Slash, Star Boomerang, Nova Aura, Orbit Blades, Thunder Strike,
  Flamewalker, Fire Staff, Frost Ring, Poison Flask, Black Hole, Homing Missiles.
  Techniques (no slot): Dive Strike, Ground Quake.
- **Tomes** (apply to all weapons): Power, Haste (cooldowns), Quantity (extra projectiles), Area, Piercing,
  Luck (crit), Swift Boots, Spring Legs, Hearty, Mending, Gem Magnet, Quick Study, Thick Hide, Second Chance.
- **Status effects:** burn (damage over time, spread by fire weapons), poison (damage over time and a slow),
  freeze (the foe cannot move; bosses and elites are only slowed).
- **Synergies:** frozen foes take extra damage from everything (shatter), burning foes take extra damage
  from lightning, black holes drag foes into every area weapon, fireballs leave flames after Inferno.
- **Evolutions** (need two things at level): Star Storm, Supernova, Cyclone, Thunder God, Comet Return,
  Earthshaker, Master Sword (sword plus luck: every swing throws a piercing beam), Inferno (fire staff plus
  flamewalker: bigger, longer fires, fireballs leave flames), Singularity (black hole plus magnet: huge
  well and a violent implosion).

Everything is deterministic (`swarm_core.rs`, seeded RNG), pooled in fixed slot arrays, and drawn through
the existing pooled render lists (bolts, rings, zones, boomerangs, orbit stars).

Unsourced numbers (per-level scaling of the original game) were not copied; balance is tuned by the
scripted kiting runs in `swarm_core.rs` tests (`hero_survival`).

## Balance checks and tooling

- `cargo test --release -p jumpman --lib hero_survival -- --ignored --nocapture` runs eight seeds per hero with a
  scripted kiting player (a skilled one and a casual half-speed one) and prints survival, kills at 2:00 and the
  share of runs that reach the Tyrant. `link_survives_and_kills_as_well_as_mario_early` keeps Link within reach of
  Mario's kills and survival through 3:00.
- Headless hooks: `JUMPMAN_SWARM_UPGRADES=sword:4,flame:3` (names or indices, slots enforced),
  `JUMPMAN_SWARM_AUTOPICK=1` (first card of every offer), `JUMPMAN_SWARM_START`, `_LEVEL`, `_GOD`, `_CAP`,
  `_PROFILE=1`. A finished run logs `swarm run over: ...` with hero, time, kills and level.
- Added effects stay pooled: bolts (kind per slot), rings (sword arcs, frost ring), zones (flame, poison,
  black hole; appended to the ring list) and the existing boomerang and orbit lists. No list grows during a run
  (`weapon_effects_stay_in_their_pools`).

## Exploring the whole level (camera, chests, no arena)

- **Camera** (`swarm_cam.rs`). A third-person orbit camera behind the hero at about 29 degrees of pitch. The player turns it
  with the right stick, a touch drag on the right half or the mouse; the hero moves relative to it (the shell camera rig supplies the
  yaw, pitch and zoom). The distance grows by up to a quarter with the crowd, walls and hills pull the camera in
  (never through them), and the eye is kept off side walls and above the ground. All weapons aim at the nearest foes (not at the
  hero's facing), so the pattern reads from any angle.
- **No arena.** The whole real course is the playfield. The ground around the player is sampled in 16x16-cell chunks as he walks
  (`Terrain` in `swarm.rs`, filling the core's streamed `WalkMask`); a cell is walkable when its floor is continuous and no wall
  crosses it. Foes spawn 1600-2200 units from the player on walkable ground joined to him in a straight line, chase him, and are
  recycled past 3000 units (bosses and elites are moved instead), so the live count stays under the cap (300 desktop, 190 mobile).
  The pressure that replaces the storm is the clock: the Star Tyrant arrives at 7:00, and from then on foes get tougher, faster and
  more numerous every minute (`dawn()`), so lingering in the far corners of the level is not free.
- **Falls and hazards.** A fall (below the last safe spot, or onto a death plane) puts the player back on a safe spot from about a
  second earlier with a scratch; lava burns; water is swum.
- **Hidden chests** (`swarm_world.rs`, drawn by `swarm_chest_view.rs`). `plan` floods the course's own floors (up to three layers
  per 100-unit cell, so roofs and caves count; walls, slippery or steep slopes, narrow ridges and water are not standable) from the
  start: first by walking, then with one jump up a ledge or a drop of the same height (190, so no chest lies at the bottom of a pit that cannot be left). Every reached spot is scored for being hidden (an
  enclosed nook, a ledge, a covered cave, a far dead end); a seeded draw with spacing picks 10 to 36 sites by reachable area, three
  quarters hidden first, the rest left in the open. The best hidden ones are golden, the next rare, the rest common.
  Same seed, same layout (unit-tested), and every chest is checked to be reachable on all eight real courses.
- **Finding them.** A chest shows a light beam only within 1300 units; hidden ones sparkle through walls within 2300 and chime close
  up; once seen, a chest stays on the round compass (top right, the camera's forward is up) with a count.
- **Contents.** Touching a chest opens it: a card pick (weapon, weapon level, tome or a ready evolution), more for the better ones,
  plus healing and gold (paid out as star bits after the run). Golden chests need a key (elites drop one) or a 20-second fight
  against a guardian and waves while you stay within range; leaving cancels it and the chest resets.
- **Test hooks.** `JUMPMAN_SWARM_TP=chest:<n>[:<dist>]` or `<x>,<z>`, `JUMPMAN_SWARM_CAM=<yaw>,<pitch>,<dist>`,
  `JUMPMAN_SWARM_CHESTS=1` (lists the chests), `JUMPMAN_SWARM_BOT=1` (a scripted walker visits every chest over the planner's route),
  `cargo test ... a_scripted_walker_reaches_chests` steps the real simulation to the
  chests of every course (`SWARM_BOT_LEVELS=bob,island`, `SWARM_BOT_CHESTS=<n>`, `SWARM_BOT_TRACE=1`).

