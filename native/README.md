# Jumpman native (Rust / Bevy)

A Rust port of the TypeScript game. `jumpman-sim` (`sim/`) is the engine-free simulation (f64, bit-compatible
with the TS original); the `jumpman` crate (`src/`) is the Bevy 0.16 app that renders, plays audio and
runs menus/progression on top of it. Method, stage table and open TODOs: [PORTING.md](PORTING.md).

## Build and run

```sh
export PATH=/opt/homebrew/opt/rustup/bin:$PATH   # if rustup is from Homebrew
cd native
cargo run --release                      # title screen
cargo run --release -- bob --act 2 --entry WARP_NODE_0A
cargo run --release -- --list            # course ids
./package-mac.sh                         # -> target/Jumpman.app (open it)
```

CLI: `<course> [--act N] [--area N] [--entry NODE] [--push] [--experiment NAME] [--list]`; short names
`kokiri`, `dungeon`, `shop`, `link` map to the Zelda courses. A CLI course or any headless hook skips the title.

### Assets
The game reads the checked-in `../assets` (SM64 export, audio, UI art) and `../public/models`. The root is
resolved in order: `JUMPMAN_ASSETS`, `Jumpman.app/Contents/Resources/assets`, `assets/` beside the executable,
`../../../assets` from `target/{debug,release}`, the compile-time checkout, `./assets`.
`package-mac.sh` bundles the whole tree (~380 MB), `public/models` and an icon made from `public/icon-512.png`;
`JUMPMAN_BUNDLE_COURSES=bob,ccm` bundles only those courses; `JUMPMAN_ICON=x.icns` overrides the icon.
Saves and `settings.json` live in `~/Library/Application Support/Jumpman/` (`JUMPMAN_SAVE_DIR`,
`JUMPMAN_CONFIG_DIR` redirect them; Linux `~/.local/share/jumpman`, Windows `%APPDATA%\Jumpman`).

### Env hooks (selection)
| Variable | Effect |
|---|---|
| `JUMPMAN_COURSE/ACT/ENTRY`, `JUMPMAN_EXPERIMENT` (`dimensions god runner hundred diorama race shrink planet swarm relic gate halls`), `JUMPMAN_CHARACTER` | start state |
| `JUMPMAN_EXIT_AFTER=<s>`, `JUMPMAN_SCREENSHOT=<png>`, `JUMPMAN_WALK=1` | headless smoke / screenshot (saved 1 s before exit) |
| `JUMPMAN_SCREEN=title\|pause\|courses\|settings\|controls\|slots\|gameover\|play`, `JUMPMAN_SHELL_SCRIPT` | force a menu / inject menu commands |
| `JUMPMAN_FLOW=selector\|result\|gameover\|bonus\|death\|star` | progression screens |
| `JUMPMAN_DEBUG=1`, `JUMPMAN_DEBUG_LOG=<n>`, `JUMPMAN_FRAME_LOG=1` | debug panel / state log |
| `JUMPMAN_MUTE=1`, `JUMPMAN_NO_AUDIO=1`, `JUMPMAN_AUDIO_DUMP=f.raw` | audio |
| `JUMPMAN_TELEPORT=x,y,z[,yaw]`, `JUMPMAN_TAP=a@3.0,b@5` | scripted input |
| `JUMPMAN_ZELDA_FLAGS/POS/PRESS/AIM`, `JUMPMAN_NO_TRAVEL` | Zelda scene states |

More are documented in the module headers and PORTING.md.

## Controls
WASD / arrows move, Space jump (hold for height), Shift or J punch/grab/throw, Ctrl or K crouch/ground pound,
Alt walk, Q/E or mouse drag orbit, wheel zoom, C camera behind, R reset, V swap character, M mute,
H aim the slingshot (Link), X leave an experiment. Gamepads work (left stick, A/B/Z, triggers, Start pauses).
Esc / Start pauses; quit from the menu or Cmd+Q.

**Touch** (`src/touch.rs`, port of `TouchControls.ts`): when touches arrive (iOS/Android builds, touchscreens) a bevy_ui
overlay appears while playing: virtual stick bottom-left (108 px, N64 64-unit stick math identical to the TS), L/Z/B/A
buttons bottom-right (each is the same action as its key/pad binding), and dragging anywhere else swipes the camera.
Touch merges into `PlayerInput` after keyboard/pad, so it coexists with them. Debug: `JUMPMAN_TOUCH=1` shows the overlay,
`JUMPMAN_TOUCH_SIM=1` replays synthetic touches (stick up-right, A held, camera swipe); combine with `JUMPMAN_SCREENSHOT=out.png`.

## Star Swarm (experiment)
An original arena-survivor ("horde survivor") mode, native-only. Pick **Star Swarm** from the green experiment
buttons on the title or pause menu, or run `cargo run --release -- bob --experiment swarm`.

You run, jump, dive and ground-pound on Mario's real physics inside a walled plaza; your stars fire at the nearest
foes on their own. Goombas ("skitters"), thwomps ("brutes"), spinies ("dashers" that wind up, then lunge) and boos
("wisps" that shoot) swarm in from the edge with wave scaling over time (count, health, speed, damage), a ring of
skitters closes in every 40 s and a king bob-omb mini-boss arrives every 2 min. Kills drop gems (gold/blue/red by
value) and sometimes a heart; gems get magnetised, fill the XP bar, and every level-up pauses the game and offers
three upgrade cards (tap, keys 1/2/3, arrows + Enter, or gamepad d-pad + A): Star Power, Rapid Fire, Twin Stars, Big
Bang, Piercing Light, Swift Boots, Spring Legs (jump), Ground Quake (ground-pound shockwave), Hearty, Mending, Gem
Magnet, Lucky Star (crit) and the rare Nova Pulse, Orbit Stars and Second Chance. Contact has i-frames and a jump
clears small foes. The run ends at 0 health; the summary card offers Retry / Main Menu and the best survival time
is kept in `swarm.json` beside the save files (`progress::save::data_dir`, so it works in the iOS app container).

Code: `experiments/swarm_core.rs` (engine-free, seeded, unit-tested: XP curve, wave scaling, upgrade rolls and caps,
i-frames, determinism, peak-load tick cost), `swarm.rs` (arena, Mario glue, render lists), `swarm_view.rs` (pooled
actor models, HUD, cards, summary on `bevy_ui`; ~220 live foes on desktop, 160 on iOS, gems culled by distance).
Env hooks: `JUMPMAN_SWARM_SEED=<n>` (reproducible run), and for screenshots `JUMPMAN_SWARM_START=<secs>` (start with
that minute's waves), `JUMPMAN_SWARM_XP=<n>`, `JUMPMAN_SWARM_GOD=1`, `JUMPMAN_SWARM_SHOWCASE=1` (one of each foe);
runs using these never write the best time. Scripted card picks: `JUMPMAN_XP_KEYS="3:1"`.

## Last Star Standing (experiment)
An original last-one-standing match, native-only. Pick **Last Star Standing** from the green experiment buttons on the
title or pause menu, or run `cargo run --release -- bob --experiment laststar`.

You and 39 AI Marios (23 on iOS) glide down onto Bob-omb Battlefield from a launch height: steer with the stick
(any speed), hold A to dive down faster, and choose where to land. Then scavenge: loot camps, chests and falling
airdrops hold **Bounce Shells** (thrown, they roll and ricochet), **Lit Bob-ombs** (lobbed, fused, big blast),
**Fire Flowers** (rapid fireballs), **Star Bursts** (radial blast + a moment of invincibility), **Quake Charges**
(your next ground pound lands as a huge shockwave), **Shield Caps**, **Swift Boots**, **Hearts** and **Lucky Shrooms**,
in three rarities (damage and uses scale). The inventory is two weapon slots plus one consumable slot: walk over an
item to take it (a rarer one swaps out your worst). Mario's own moves are the skill ceiling: punch, **dive** (hit
at range, big knockback), **ground-pound** (stomp a head, or a shockwave that only hurts grounded rivals: jump it) and
wall kicks all matter. Hits do damage with brief i-frames and real knockback; knocking a Mario off the island or into
water counts as an elimination, credited to whoever hit them last. **The Blight** (a purple fog wall) closes in over
six phases (hold, warning, shrink) and drains health outside the safe circle faster each phase; the next circle is
marked on the minimap and by a gold beam. The match lasts about six minutes; the last Mario standing wins.

HUD (all `bevy_ui`, phone-first): health and shield, alive and knockout counts, the zone timer line, a minimap with
the safe and next circles and a rim pointer at the next centre, an elimination feed ("X knocked Y out"), name tags on
nearby rivals. Slots are keys `1` `2` `3` (hotbar buttons, clickable), d-pad left/up/right on a pad, and on touch
three round buttons above the A/B cluster inside the safe-area insets. After you are out the camera follows a rival
(`<` `>` / Q E / shoulder buttons switch) and the placement card shows "#N of M", knockouts and survival time with
Spectate / Retry / Main Menu; your best placement and win count live in `lss.json` beside the save files
(`progress::save::data_dir`, so it works in the iOS app container), as does the chosen difficulty.

Bots: each has an aggression (how readily it picks fights), lands near loot, scavenges, rotates into the next circle
before the Blight arrives, engages when it meets you (shells, flames and bombs at range, dives, pounds, punches), flees
when hurt, and uses consumables. Bots near the action (the nearest 14, 8 on iOS) run on the full SM64 physics as
`CrowdMario` bodies (A Hundred Marios' tech) and press real stick and buttons; the rest are moved by a cheap kinematic
mover in the core. The whole field costs about 30 microseconds per 30 Hz step on an M-series Mac.

Code: `experiments/lss_core.rs` (engine-free, seeded, unit-tested: zone schedule and damage, loot tables and rarity,
inventory, damage/shield/i-frames/knockback, melee shapes, projectiles, eliminations, placements, bot state
transitions, a whole simulated match for determinism, 40-bot tick cost), `lss.rs` (island measurement, terrain adapter,
bodies, glue), `lss_view.rs` (pickups/shots/blasts/Blight wall, HUD, minimap, feed, tags, cards on `bevy_ui`).
`touch.rs` gained `ModeButtons` (extra round touch buttons for a mode).
Env hooks (screenshots and tests; runs using the state-changing ones never write records): `JUMPMAN_LSS_SEED=<n>`,
`_BOTS=<n>`, `_DIFF=easy|normal|hard`, `_START=<secs>` (skip the drop, fast-forward the clock), `_PHASE=<n>` (start in
phase n's shrink), `_ALIVE=<n>`, `_GOD=1`, `_KIT=1`, `_DIE=<secs>`, `_SPECTATE=1`, `_FIGHT=1`, `_LOOT=1`, `_EDGE=1`,
`_TOPDOWN=<height>` (overview camera), `_LOG=1` (step cost every 5 s). Scripted item use: `JUMPMAN_XP_KEYS="4.0:1,4.4:2"`.
## Relic Descent (experiment)
An original action-RPG "looter dungeon crawler", native-only. Pick **Relic Descent** from the green experiment buttons
on the title or pause menu, or run `cargo run --release -- bob --experiment relic`. It plays as whichever character you
picked (Mario, Link, Young Link, Banjo) on that character's real physics.

* **Dungeon**: three seeded floors of 6-8 rooms joined by three-tile corridors on a tile grid (walls are merged boxes: one
  wall mesh + a few floor meshes per floor), low brick walls so the camera sees over them, torches, a fog/point-light mood,
  a minimap (rooms light up as you visit them, treasure gold, stairs, boss red) and a stairs ring. Packs sleep until you
  enter their room (so only one pack is ever awake; cap 80 live foes, 64 on iOS). The farthest room holds the stairs; floor 3
  ends in the **Bomb King** (three phases: stomp rings, then a call for help and bomb volleys, then enraged charges).
* **Combat on Mario's moves**: B (or the touch ATK button, `Enter` held) swings (cone in front: Mario's punch combo is wide and
  quick, Link's slash long, Young Link jabs fast, Banjo's claw heavy); **running + B is the dive = the dodge, with i-frames**
  (touch DODGE / `B` key forces a dive from standing); the ground-pound landing is the **Quake Slam** shockwave (the SLAM
  button leaps and slams for you). Skills with spark cost and cooldowns: Whirl, Star Volley (fan of stars), Aegis (shield);
  a 12-perk skill tree (11 nodes unlock/upgrade the four skills, plus Vigor) spends one point per level (two to start).
  Foes: Scamp (goomba swarmer), Bruiser (bully, telegraphed ground slam), Lancer (spiny: wind-up then charge), Wraith (boo:
  keeps range, shoots), Sapper (black bob-omb: lit fuse then blast), Seer (summoner), plus elite modifiers (Swift, Stout,
  Leeching, Volatile, Warded) with a ring under them. Hit numbers, crit gold numbers, flashes, knockback, health bars.
* **Loot**: kills and chests drop gear (weapon, armor, boots, two charms) in five rarity tiers with random affixes (damage, crit,
  attack speed, move speed, life steal, max health, cooldown cut, dodge frames, pickup radius, thorns; no duplicates), item
  level from floor and character level, six legendary effects (Sunsplit Blade, Quakebrand, Mantle of Ten Thorns, Skiff
  Treads, Lantern of Second Chances, Magpie's Knot), gold, potions. Auto-pickup; ground labels in rarity colours with
  beams from epic up; the loot filter (default on) turns commons into gold on the spot. The pack page (BAG button / `I`)
  shows worn gear, the bag, and a compare card with green/red deltas, EQUIP / SELL, SELL COMMONS and the filter toggle.
* **Meta**: gold from a run is banked in `relic.json` beside the save files (`progress::save::data_dir`, so it works in the
  iOS container): 60% on death, 100% on victory; every 500 banked gold gives +2% damage and health (max +20%). Also the
  deepest floor and the loot-filter choice.
* **Controls**: touch uses the stick, A/B/Z/L as ever plus ATK, DODGE, SLAM/WHIRL/STARS/AEGIS, POT, BAG and TREE (all inside the
  safe area and clear of the pause button; they dim while recharging). Keyboard: `Enter` attack (hold), `B` dodge, `1`-`4` skills,
  `H` potion, `I` pack, `T` skills, `Y` filter. Gamepad: d-pad left/right/down = skills, up = potion, RB attack, Select pack, L3
  skills (East still punches/dives).

Code: `experiments/relic_core.rs` (engine-free, seeded, unit-tested: damage and crit, armour, i-frames, XP and skill tree, pack
and boss scaling and phases, room wake-up, descending, pickups and bag, determinism, a bot clearing a room, tick cost with a
full crowd), `relic_loot.rs` (rarity weights, affixes, item-level scaling, compare, equipment totals), `relic_dungeon.rs`
(generator: connectivity, determinism, wall boxes), `relic.rs` (floor meshes/collision, Mario-action glue, publishing),
`relic_view.rs` (bridge, pooled models, overlays, input) and `relic_ui.rs` (HUD, pack, skill tree, summary on `bevy_ui`).
The touch layer has mode buttons (`touch.rs`: `Control::Extra`, `extra_layout`).
Env hooks: `JUMPMAN_RELIC_SEED`, `_FLOOR` (start floor), `_LEVEL`, `_GOD=1`, `_GIVE=legendary:weapon,epic:armor,...`, `_BAG=<n>`,
`_DROPS=<n>` (loot on the floor), `_TREE=2:1,3:2` (perk index:ranks), `_POINTS`, `_SCREEN=bag|tree`, `_SELECT=<bag index>|worn<slot>`,
`_TELEPORT=boss|stairs|treasure|combat|<room>`, `_BOT=1` (walks at foes and swings), `_HORDE=<n>`, `_SHOWCASE=1`, `_END=win|lose`
(summary screens). Runs using them never touch the banked gold / best floor.

## Hold the Gate (experiment)
An original 3D tower-defence mode with a "hero plus towers" feel, native-only. Pick **Hold the Gate** from the green
experiment buttons on the title or pause menu, or run `cargo run --release -- bob --experiment gate`. It builds its own
walled meadow (no course needed, so it also starts in-process on iOS) and uses the real SM64 actor models.

* **Maps** (picked on a start card, `1` / `2`): **Meadow Pass** (one long winding road; flyers cut straight across it) and
  **Twin Fords** (two roads from two spawn pipes merge into one long straight). Grass cells alternate shade so the grid reads;
  road and rock cells are not buildable. The castle gate at the end of the road has 20 lives (a leak costs 1, a boss up to 20).
* **Waves**: 15 hand-tuned waves (a boss every fifth: King Bob-omb on 5, Chain Chomp on 10, Whomp on 15), wave-scaled health,
  a next-wave timer you can beat with CALL (small coin bonus), a **wave preview** panel (what is coming, BOSS / flyers tag),
  x1 / x2 speed and a soft pause. Foes: Goomba (swarmer), Bob-omb (fast), Bully (armored, shoves and jams a tower), Spiny
  (slow, tough, fireproof), Boo (flies straight over the rocks), Lakitu (flying healer), Koopa (calls in goombas). Live foes are
  capped at 150 on iOS (180 elsewhere).
* **Towers** (6 kinds x 3 levels, sell for 70%): **Bombard** (lobbed shells, splash, ground only), **Crush Slab** (a thwomp that
  slams the road: damage and stun, ground only), **Cinder Bloom** (fireballs that burn, ignoring armor), **Dread Eye** (a slowing
  aura that also hits flyers; level 3 chips), **Prism Star** (a piercing beam), **Coin Spout** (pays coins over time). Targeting
  modes First / Last / Strongest / Closest. Armor trims each hit (never below 20%); bosses resist slow and stun.
* **Hero assist** (modest, never required): Mario's punch, dive and ground-pound hurt foes (flyers need him airborne), foes
  drop bonus coins he can scoop up, standing near a tower **rallies** it (faster fire, 1.5x Spout payout, repairs a jam four
  times as fast) and a ground-pound beside a tower triggers a 5 s **overdrive** (and clears its jam). Towers are solid.
* **Building**: tap (or click) a grass cell, or press the BUILD touch button / `B` / pad X to act on the cell in front of
  Mario; the cursor cell shows green or red. The build bar at the bottom picks a tower (the first tap shows its range ring
  and description, tapping it again builds); tapping a tower opens its card (stats, target mode, UPGRADE, SELL, rally/jam state).
  Keys: `1`-`6` pick/build, `U` upgrade, `V` sell, `T` target, `N` call wave, `F` speed, `P` pause. Gamepad: X = build,
  Y = call wave, d-pad up = speed, d-pad down = pause, d-pad left/right + A walk an open menu, B closes.
* **Meta**: the best wave cleared per map is kept in `gate.json` next to the save files (`progress::save::data_dir`, so it
  works in the iOS container). The summary card shows waves cleared, kills and coins earned with RETRY / OTHER MAP / MAIN MENU.

Code: `experiments/gate_core.rs` (engine-free, seeded, unit-tested: path following, targeting rules, damage/armor/splash/slow/
burn/stun/pierce, economy cost/refund/upgrade, wave table and scaling, hero assist, win/lose, a bot beating a map, full-run
determinism, tick cost with 150 foes and 30 towers), `gate.rs` (arena, Mario-action glue, build-menu state machine, publishing),
`gate_view.rs` (bridge, pooled foe/tower models, HUD, build bar, tower card, preview, map picker, summary, tap-to-ground ray).
Env hooks: `JUMPMAN_GATE_SEED`, `_MAP=0|1` (skips the picker), `_WAVE=<n>`, `_COINS=<n>`, `_LIVES=<n>`, `_GOD=1`, `_BOT=1` (automatic
builder), `_SPEED=2`, `_TOWERS="kind:level@x,y;..."`, `_FOES="kind:offset;..."`, `_FREEZE=1`, `_MENU=build@x,y|card@x,y` (+ `_SEL=<kind>`),
`_END=won|lost` (summary), `_ZOOM`/`_FOCUS=x,y` (camera), `_SCRIPT="2:tap@5,3;3:pick2;4:pick2;6:upgrade"` (UI commands at run-relative
seconds: tap@x,y, pick<k>, confirm, upgrade, sell, mode, close, speed, pause, wave, interact, map0/map1, retry, othermap; for end-to-end checks without a pointer). Runs using them never touch the saved best wave.

## Star Deck (experiment)
An original turn-based **deckbuilder roguelike**, native-only. Pick **Star Deck** from the green experiment buttons on the
title or pause menu, or run `cargo run --release -- bob --experiment stardeck`. The cards are Mario moves and the hero is
whichever character you picked (Mario, Link, Young Link, Banjo), animated through the real player model on a small 3D stage.

* **Momentum, not energy**: every card spends **momentum** (the vertical meter, 0-8). Moves chain in the order
  **Jump -> Dive -> Pound -> Jump...**: each link refunds 1 momentum and adds +2 damage per chain level to attacks (cards with a
  gold ring would extend the chain right now). Strike and Spin cards break a chain, Brace and Trick cards leave it alone, X-cost
  cards (Star Spin) spend the whole meter. Momentum fades by 1 per turn but never below your stride (3, +1 with Spring Boots).
  Hand of 5, draw / discard / exhaust piles (tap the pile buttons), Retain and Exhaust keywords, Powers last the fight.
* **Statuses** (original): Scorch (burn), Dizzy (-25% damage), Shaken (+50% taken), Pumped (+damage), Tough (+Shell), Spiky,
  Stride, Regen, Quick. Shell is the block resource and clears at the start of its owner's turn.
* **Foes telegraph**: every foe shows its next move above it (attack damage as it will land after statuses, guard, buff, hex, or `!!`
  for a warning turn). 10 regular foes (Scuttler, Fuse Pup, Rammer, Prickle, Gloomwisp, Slab, Snapvine, Swooper, Zapper, Shellback),
  2 elites (Tether Hound, Horn Captain: second phase at half health) and the three-phase **Fuse King** boss (calls guards, lobs
  Bruise bombs, winds up a 32-damage Boom Stomp). Foes use the SM64 actor models with their animation clips (pooled, lunge/flash on hits).
* **Run**: one act, a branching 15-node map (6 rows of 2-3 stops + the boss; battles, elites, shops, campfires, mystery events,
  treasure), seeded. 55 cards (shared starters, a signature pair per hero, common/uncommon/rare attacks, skills and 6 powers,
  each with a "+" upgrade), 11 relics, 5 potions (3 slots), 5 events, shop (5 cards, 2 relics, 2 potions, card removal), campfire
  (heal 30% or upgrade a card), card rewards (pick 1 of 3 or skip; rarity weights 60/33/7, elites 45/40/15).
* **Meta**: best stops cleared / best score / wins are kept in `stardeck.json` beside the save files (`progress::save::data_dir`, so
  it works in the iOS container). `JUMPMAN_DECK_SEED=daily` plays the same seed for everyone on a UTC day.
* **Controls**: tap a card to select (it grows), tap it again to play, or drag it up; tap a foe to aim a targeted card; END TURN,
  piles, DECK and potions are buttons (all 44px+ and inside the safe area, clear of the pause button). Keyboard: left/right + Enter,
  `1`-`9` pick a card, `E` end turn, Backspace cancels; gamepad: d-pad + South / East / North. Menus also take arrow keys + Enter.

Code: `experiments/deck_core.rs` (engine-free, seeded, unit-tested: draw/discard/reshuffle, damage and status math, momentum and chain
rules, X cost, retain/exhaust/powers, relics, potions, foe patterns and boss phases, a bot clearing fights, every card playable with
terminating loops), `deck_run.rs` (map generation: connectivity, node distribution; encounters, rarity-weighted rewards, shop
pricing, events, rest, full-run determinism and score), `deck.rs` (experiment shell, stage, hero choreography through the sim,
event timeline playback, foe models, props, save, env hooks), `deck_ui.rs` (all screens on `bevy_ui`), `deck_tests.rs` (command path
and event-stream consistency). `DeckGame` is a plain resource inserted on enter, so nothing was added to the experiment host.
Env hooks: `JUMPMAN_DECK_SEED=<n>|daily`, `_START=map|battle|elite|boss|shop|rest|event[:n]|treasure|reward|won|lost`,
`_FOES=scuttler,rammer`, `_HAND=dive,pound+`, `_GIVE=long_jump+,mega_quake`, `_RELICS=spring_boots,brick_helm`, `_HP`, `_COINS`,
`_DEPTH=<rows cleared>`, `_SCRIPT=2=play.0.0,4=end,6=node.1` (commands wait for the animation), `_AUTOPLAY=1` (a bot plays the run),
`_TEMPO=<k>` (animation time scale; 0.12 with `_FAST=1`), plus the global `JUMPMAN_WINDOW=874x402` for phone-sized screenshots.
Runs using the hooks never touch the saved bests.

## The Painted Halls (experiment)
An original **metroidvania**, native-only: one connected manor of sixteen rooms in six regions (Foyer, Hanging Garden, Cellars,
Flooded Vault, Clock Tower, Long Gallery) gated by abilities you find one by one, built on Mario's real controller. Pick
**The Painted Halls** from the green experiment buttons on the title or pause menu, or run
`cargo run --release -- bob --experiment halls`. A **New game / Continue** prompt opens first.

* **The moves are the keys.** Six unlocks, each with a visible "can't pass yet" cue: **Long Jump** (wide gaps; Hedge Run, Gear
  Foyer), **Wall Kick** (two tall columns 340 apart; Greenhouse, Clock Shaft), **Ground Pound** (cracked floor plates in the
  Crypt and the Wine Racks drop you into the Drain Hall), and the three power caps: **Wing Cap** (press A again in the air to
  flutter, three flaps per jump, hold A to glide: high ledges), **Metal Cap** (walk through the ink pools unharmed) and
  **Vanish Cap** (slip through iron bars). Wear a cap with `F` (touch CAP button, pad RB); until a move is found its input is
  simply ignored (no crouch on the ground, no Z pound in the air, no wall kick). Floating glyphs in the cap colours (red wing,
  green metal, blue vanish, orange / yellow / purple for the moves) mark each gate until you own the ability.
* **World**: hand-authored data (`halls_world.rs`: room boxes, doors, pickups, foes, gates), built into merged meshes and a fresh
  `CollisionWorld` per room (only the current room exists; walls are inward-facing quads so the camera sees into the "dollhouse").
  Doors fade through a short black; locked doors take the Garden Key, a boss kill, or 8 star fragments (the Foyer's star door);
  one shutter only opens from the far side. Rooms hold 13 **star fragments** and 3 **heart containers** (start with 3 hearts,
  max 7); framed procedural paintings hang in the halls.
* **Foes** (on `assets/actors`, pooled with `spawn_model` so their clips play): Goomba (chases), Bob-omb (fuse, blast), Spiny
  (patrols, charges, ignores stomps), Boo (only chases when your back is turned), Bully (charge) and Piranha Plant (pops up to
  bite). They respawn when you re-enter a room; contact costs half a heart with 1.5 s of i-frames; they drop hearts. Mario's own
  moves fight: stomp, punch / kick, dive, ground-pound landing. Two bosses: **the Warden** (Whomp: slams, then lies prone; stomp or
  pound him then; three phases, calls goombas, double slams) and **King Fuse** (King Bob-omb: bomb rain, charges into the wall and
  is stunned there; a pound does double). Bosses seal the doors, then drop the Ground Pound sigil / the Vanish Cap.
* **Save rooms and death**: pedestals (Foyer, Wine Racks, Drain Hall, Vault Heart, Gear Foyer, Heart Chamber) heal fully and set the
  respawn. Dying returns to the last pedestal with full health; pickups, bosses and broken floors stay done. Falling into a pit costs
  a heart and returns you to the last solid ground.
* **Map** (`Tab`, touch MAP, pad Select): grid of discovered rooms (region colours), the current room ringed in gold, save / boss /
  locked-door dots, "?" cells for unvisited exits, ability gates you have seen noted as coloured dots (faded once you own the
  ability), a legend and the abilities checklist. The HUD has hearts, star fragments, six ability chips (worn cap outlined), a
  corner minimap, a boss bar and a room banner.
* **Persistence**: `halls.json` beside the save files (`progress::save::data_dir`, the app container on iOS): abilities, keys, bosses,
  collected / broken / opened flags, discovered rooms, seen gates, last pedestal, worn cap, time and deaths. Saved on every pickup,
  boss, pedestal and door. Test-hook runs never touch it.
* **Controls**: stick, A / B / Z as ever; `Tab` map, `F` cap (`M` / `C` already mean mute / camera app-wide); touch has MAP and CAP buttons beside the A / B grid (inside the safe area).

Code: `experiments/halls_world.rs` (engine-free data, geometry queries, the room graph and **solver**: it proves the game is
completable, that every fragment and room is reachable, that removing any ability cuts exactly the places behind its gates, that
nothing soft-locks and that the intended order holds), `halls_core.rs` (rules: foes, bosses, damage / i-frames, pickups, doors,
cracks, ink, save round trip, map state), `halls.rs` (glue: collision and meshes per room, the controller gating, wing flutter,
cap worn, fades, camera, plus **physical tests**: the real controller on each room's collision long-jumps the Hedge Run and Gear
Foyer gaps, wall-kicks up the Greenhouse and Clock Shaft columns, flutters onto the Foyer ledge and the Clock Shaft slab, and
fails every one of them without its ability), `halls_view.rs` (bridge, pooled models, touch / pad input) and `halls_ui.rs`
(HUD, map, prompts on `bevy_ui`).
Env hooks (any of them makes the run ephemeral): `JUMPMAN_HALLS_NEW=1`, `_ROOM=<index|name>`, `_ABILITIES=all|LJ,WK,GP,WG,MT,VN`,
`_CAP=wing|metal|vanish`, `_GOD=1`, `_POS=x,y,z` (room units), `_FRAGMENTS=<n>`, `_KEY=1`, `_BOSSES=<mask>`, `_DISCOVER=all`,
`_SCREEN=map|prompt|dead|win`, `_SHOWCASE=1` (one of each foe), `_DEMO=wallkick|flutter|longjump|pound|north` (an autopilot that
performs the move, for screenshots).

## Offline
The native app is inherently offline; there is no service worker equivalent to `src/offline.ts` /
`scripts/build-offline.mjs` to port. The Rust code makes no network requests (no HTTP client dependency, no URL fetches;
Bevy's web asset sources are not enabled) and reads only files under the assets root (`JUMPMAN_ASSETS`, or the bundle's
`Contents/Resources/assets` + `public/models` made by `package-mac.sh`). Saves are local files.

## Architecture map
- `sim/src`: `math`, `surfaces`, `collision`, `mario_*`, `actions/*` (one file per TS action file), `object*`,
  `behaviors/*`, `characters`; no engine dependency.
- `src/game`: course/catalogue loading and `GameSim` (30 Hz tick). `sim_plugin.rs` drives it from `FixedUpdate`.
- `src/input.rs`, `camera/`, `render/`, `fx/`, `hud.rs`: presentation. `audio/`: SM64 sequence player (wasm in wasmi).
- `src/progress`: saves, warps, stars, doors, wipes. `src/shell`: title/pause/settings UI. `src/objects`, `zelda/`,
  `characters/`, `experiments/`: gameplay extras.

## Tests and golden fixtures
`cargo test --workspace --locked` (about 300 tests). Sim tests in `sim/tests/` compare against JSON fixtures
in `sim/tests/fixtures/` that are generated from the TypeScript code and committed; values must match bit-for-bit.
Regenerate from the repo root after changing TS sim code:

```sh
GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts
cd native && cargo test --workspace
```

CI: `.github/workflows/rust.yml` runs tests (blocking) and clippy (non-blocking) on macOS for changes under `native/`.

## iOS / TestFlight

`ios/` holds an Xcode wrapper around the game as a Rust static library (`jumpman_game`, entry point `main_rs`).

```sh
# one-time
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
# simulator build (no signing)
xcodebuild -project ios/Jumpman.xcodeproj -scheme Jumpman -configuration Release -sdk iphonesimulator \
  -derivedDataPath /tmp/jumpman-ios CODE_SIGNING_ALLOWED=NO build
codesign --force --sign - /tmp/jumpman-ios/Build/Products/Release-iphonesimulator/Jumpman.app
xcrun simctl install booted /tmp/jumpman-ios/Build/Products/Release-iphonesimulator/Jumpman.app
SIMCTL_CHILD_JUMPMAN_TOUCH=1 xcrun simctl launch booted com.anf.jumpman
# TestFlight
TEAM=ABCDE12345 BUNDLE_ID=com.you.jumpman UPLOAD=1 ./package-ios.sh
```

Notes: iOS 26+ requires the UIScene life cycle, which winit 0.30 does not use, so `ios/Sources/main.m`
swizzles `UIWindow initWithFrame:` to attach winit's window to the connected scene. The window is
borderless fullscreen so Bevy sees the real size; menus are scaled down to fit (`shell::fit_menu_scale`).
Touch controls use fixed safe-area margins (`touch.rs`). Saves/settings live in the app container.

## Mario Kart 64 free roam

Choose **Mario Kart 64** in the experiment menu, or launch directly:

```sh
cargo run --release -- playground --experiment kart
```

Uses the same `public/mk64/` export as the Three.js version. From the repository
root run `pnpm export:mk64 reference/roms/mk64-us-1.0.z64` once. The exporter
checks the US ROM checksum; the ROM is never copied or bundled. Mac and iOS
packaging copy the generated track, textures and sprite atlas.

**W/S** accelerate, brake and reverse; **A/D** steer. The gamepad or touch stick
also works. **F**, gamepad **B**, or the dock's **Get out** button parks the kart
and switches to Mario's full native walking/jumping simulation. Return nearby
and press it again to get in. **R** or the **Reset** button resets both to the
starting line. **X** leaves the experiment.

Luigi Raceway geometry and textures and the occupied kart's directional sprites
come from MK64. Driving is a custom 30 Hz controller matching `kart.ts`, with
quarter-step track collision and off-road slowdown. The empty parked kart is a
procedural mesh. Native MK64 audio, opponents, items and racing rules are not yet
ported. `cargo test --lib experiments::kart::tests` verifies driving and transfers;
the extracted-track test also runs when the local ROM export is present.
