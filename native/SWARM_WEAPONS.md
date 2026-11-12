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
