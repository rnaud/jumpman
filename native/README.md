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
| `JUMPMAN_COURSE/ACT/ENTRY`, `JUMPMAN_EXPERIMENT` (`dimensions god runner hundred diorama race shrink planet swarm relic gate halls kart`), `JUMPMAN_CHARACTER` | start state |
| `JUMPMAN_EXIT_AFTER=<s>`, `JUMPMAN_SCREENSHOT=<png>`, `JUMPMAN_WALK=1` | headless smoke / screenshot (saved 1 s before exit) |
| `JUMPMAN_SCREEN=title\|pause\|courses\|settings\|controls\|slots\|gameover\|play`, `JUMPMAN_TAB=modes\|levels`, `JUMPMAN_SHELL_SCRIPT` | force a menu (and the hub tab) / inject menu commands (`start`, `back`, `tab:levels`, `xp:<mode key>`, `course:<id>`, `open:<screen>`, `move:<dir>`, ...) |
| `JUMPMAN_FLOW=selector\|result\|gameover\|bonus\|death\|star` | progression screens |
| `JUMPMAN_DEBUG=1`, `JUMPMAN_DEBUG_LOG=<n>`, `JUMPMAN_FRAME_LOG=1` | debug panel / state log |
| `JUMPMAN_MUTE=1`, `JUMPMAN_NO_AUDIO=1`, `JUMPMAN_AUDIO_DUMP=f.raw` | audio |
| `JUMPMAN_TELEPORT=x,y,z[,yaw]`, `JUMPMAN_TAP=a@3.0,b@5` | scripted input |
| `JUMPMAN_SAFE_AREA=left,right,bottom,top` | desktop stand-in for iOS safe-area insets (logical px), e.g. `59,59,21,0` with `JUMPMAN_WINDOW=844x390` |
| `JUMPMAN_SHOW_FPS=1`, `JUMPMAN_UNCAP=1` (alias `JUMPMAN_NOVSYNC`), `JUMPMAN_RT=off\|enhanced\|rt`, `JUMPMAN_FPS=1` | performance overlay / uncapped frame rate / lighting tier (`rt` asks for hardware ray tracing, logs why it is unavailable, runs `enhanced`) / print FPS to stdout. They override Settings and are never saved |
| `JUMPMAN_ZELDA_FLAGS/POS/PRESS/AIM`, `JUMPMAN_NO_TRAVEL` | Zelda scene states |

More are documented in the module headers and PORTING.md.

## Menus (title screen and pause menu)
Both open the same *hub*: a header (logo or PAUSED + RESUME, SAVE FILE / SETTINGS / CONTROLS), a tab bar and the
hero picker (PLAY AS, reachable from both tabs).
* **MODES** (default): one card per mode with its thumbnail (`assets/ui/modes/<key>.jpg`, 512x320, `key` =
  `experiments::ExperimentId::key()`; a missing file shows a flat accent tile), the title and a one-line tagline
  (`shell::ExperimentId::tagline`). Tapping a card **always starts that mode fresh**: any running mode exits, the course
  the mode needs (else the current one) is reloaded from scratch through the shell's `StartCourse` fade (Mario, lives,
  health, enemies, objects, camera rig), and only then is the mode entered (`experiments::process_requests`, state
  machine `LoadWait`, with timeouts). The in-game dock menu and `X` menu take the same path (`ExperimentSlot::request`);
  choosing a course ends a running mode first, and LEAVE MODE reloads the current course so nothing is left behind.
* **LEVELS**: the course picker (all courses, scrolling), missions, header art `assets/ui/modes/levels.jpg` and START
  (the old START PLAYING / START COURSE behaviour).
* Navigation: arrows / d-pad move spatially across tabs and cards; `Tab`, `[` `]`, PageUp/Down or the shoulder buttons
  switch tabs; the focused button scrolls into view.
* Layout: `shell::menu_layout` picks a *compact* design (windows under 960x560, i.e. phones; 760x360 of usable space inside the
  safe area, scale kept near 1.0, 44 px tap targets, 12+ px text, 2-column cards) or the desktop design (1040x680,
  3 columns). Lists scroll with the wheel and by touch drag with a little inertia (`shell/scroll.rs`; taps activate
  on release so a drag over buttons only scrolls). Settings / Controls / Save File are scrolling pages with a BACK button
  at the top on phones.
Phone check: `JUMPMAN_WINDOW=874x402 JUMPMAN_TOUCH=1 JUMPMAN_SAFE_AREA=59,59,21,0 JUMPMAN_SCREEN=title [JUMPMAN_TAB=levels]`.

## Controls
WASD / arrows move, Space jump (hold for height), Shift or J punch/grab/throw, Ctrl or K crouch/ground pound,
Alt walk, Q/E or mouse drag orbit, wheel zoom, C camera behind, R reset, V swap character, M mute,
H aim the slingshot (Link), X leave an experiment. Gamepads work (left stick, A/B/Z, triggers, Start pauses).
Esc / Start pauses; quit from the menu or Cmd+Q.

**Banjo-Kazooie** (`JUMPMAN_CHARACTER=banjo` or V; full table and move status in `BANJO_CONTROLLER.md`): Space jump / flap, J claw / peck / barge,
K crouch / Beak Buster (in the air), **F** egg forward, **G** egg backwards, **T** Talon Trot, **Y** Wonderwing, **U** equip the next ability pad
(Shock Spring, Flight, Turbo Trainers, Wading Boots). Gamepad: D-pad = C buttons (up egg, down egg back, left trot, right wonderwing), R1 = U.
Touch: EGG / BACK / TROT / WING / POWER buttons left of the A/B grid.

**Link / Young Link** (`src/link/`, `LINK_CONTROLLER.md`): L toggles the Z-target lock (hold mode in the item screen's SETTINGS tab);
C-left / C-down / C-right are `1 2 3` (or `U I O`, or the D-pad left / down / right; on touch the `C<` `Cv` `C>` buttons above the A/B grid);
`7` / `Y` / the `C^` button opens the ocarina; `Tab` / `N` / D-pad up / the ITEMS button opens the item screen. Hold a bow, slingshot
or hookshot C key to aim (stick), release to fire. B swings the sword (stick direction picks stab / side slash, three in a row finish
the combo, hold B after a slash and release for the spin attack, B in the air is the jump slash). Test hook: `JUMPMAN_LINK_DEMO="30:lock,40:stick=up,60:b,90:c1:20,120:shot=bow"`
(entries `sim tick:what[:hold]`, `JUMPMAN_LINK_SHOTS=dir` for the PNGs) scripts inputs headlessly.

**Epona** (adult Link; `src/link/epona_view.rs`, `epona_hud.rs`, `sim/src/link/epona.rs`; details in `LINK_CONTROLLER.md`): play her song
(Up Left Right Up Left Right on the ocarina), pick EPONA on the item screen, or turn on EPONA FOLLOWS in its SETTINGS tab (she then waits
beside you at the start of every course). Walk up to her side and press **A (MOUNT)**. Riding: the stick steers her relative to her heading
(up = forward, sideways = turn, back = brake and rein in), a full stick pushes through walk, trot and gallop; **A = carrot** (six, a
dash to speed 14; one returns after about 2 s) while she moves and **A = DISMOUNT** when she stands (**B** also gets off). She jumps fences
by herself (a fence 19-40 high at a gallop, higher with a carrot) and gaps of about 250 units; water, a cliff or a steep slope make her rear up.
Touch: the round A button is relabelled MOUNT / CARROT xN / DISMOUNT. Keyboard `Space`, gamepad South, touch A. Items other than the
ocarina's songs are unavailable while mounted. Hooks: `JUMPMAN_EPONA=1` starts mounted, `near` starts her beside Link; `JUMPMAN_HORSE_CAM=<distance in Link heights>,<yaw deg>,<pitch deg>` moves the riding camera for screenshots.

**Touch** (`src/touch.rs`, port of `TouchControls.ts`): when touches arrive (iOS/Android builds, touchscreens) a bevy_ui
overlay appears while playing: virtual stick bottom-left (108 px, N64 64-unit stick math identical to the TS), L/Z/B/A
buttons bottom-right (each is the same action as its key/pad binding), and dragging anywhere else swipes the camera.
Touch merges into `PlayerInput` after keyboard/pad, so it coexists with them. Debug: `JUMPMAN_TOUCH=1` shows the overlay,
`JUMPMAN_TOUCH_SIM=1` replays synthetic touches (stick up-right, A held, camera swipe); combine with `JUMPMAN_SCREENSHOT=out.png`.

## The first eight experiments: goals and how they are proven completable
Every one has a defined win/result state, a touch-reachable restart (dock button) and a test that plays it to that state with a scripted player on the real `MarioRuntime` (`cargo test --lib experiments::`).

| Experiment | Goal / result | Restart | Proof |
|---|---|---|---|
| Across Dimensions (`dimensions`) | reach the summit (end of the mountain route) in any dimension; status shows time and switches | `R` / Climb again | `race::tests::a_racer_completes_the_real_bob_route` (shared `race::at_finish`) |
| Mario vs. God (`god`) | grab the star on the top block of the floating staircase; "STAR GET" with time and piece count | F9 / Reset | `god::tests::mario_can_climb_the_starter_staircase_to_the_star` |
| Endless Run (`runner`) | endless; a fall ends the run with score / record, A (touch, pad) or Space starts the next | A / Space / Run again | `runner::run_tests::a_scripted_player_runs_generated_tracks`, `scoring_counts_whole_metres_and_records` |
| A Hundred Marios (`hundred`) | you reach the summit through the crowd; shows how many Marios made it first | `R` / Start over | `race::tests::a_racer_completes_the_real_bob_route` |
| Diorama (`diorama`) | the Marios collect 30 coins | `N` / Start over | `diorama::tests::the_crowd_completes_the_coin_goal_on_the_real_battlefield` |
| Summit Race (`race`) | first to the summit of seven rivals; place, time, best | `R` / Race again | `race::tests::a_racer_completes_the_real_bob_route` |
| Shrink Ray (`shrink`) | reach the summit at any size (each size ticked off) | `R` / Start over | `race::tests::a_racer_completes_the_real_bob_route` |
| Tiny Planet (`planet`) | run all the way around once, then keep lapping | `R` / New planet | `planet::goal_tests::mario_runs_all_the_way_around_the_planet` |

Touch: God's `Place` button drops the selected tool's piece in front of Mario (no pointer needed); the Diorama has Turn / Tilt / Zoom / Drop coin buttons. Both phone-layout docks were checked with `JUMPMAN_TOUCH=1 JUMPMAN_WINDOW=844x390 JUMPMAN_SAFE_AREA=59,59,21,0`.

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
clears small foes. Survive until the dawn (8:00) to win ("YOU SURVIVED!"); otherwise the run ends at 0 health. The summary card offers Retry / Main Menu and the best survival time
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
* **Phones**: the camera frames the gate, the last three road tiles before it and the castle top inside the free screen
  rectangle (clear of the safe-area insets, the HUD strip and, with the touch overlay, the BUILD / WAVE / x1 row and the A/B grid;
  `gate.rs::frame_gate`, unit-tested for 874x402, 844x390 and 932x430 on both maps). The Prism Star is a flat model, so it is
  billboarded to the camera and pinwheels in its own plane instead of showing edge-on.
* **Sound**: every tower kind has its own shot sound (Bombard cannon thump, Cinder Bloom whoosh, Prism zap, Crush Slab slam,
  Coin Spout coin) rate-limited per kind, shell splashes explode, and a boss spawn / kill, wave start / clear and a leak each have a
  cue; the UI has cues for picking a tile or tower, build failure, upgrade, sell, target mode, speed and pause. All of it reuses
  the SM64 sound bank through `host.fx` / `SfxCue::Effect` (kinds added in `audio/mod.rs`).
* **Meta**: the best wave cleared per map is kept in `gate.json` next to the save files (`progress::save::data_dir`, so it
  works in the iOS container). The summary card shows waves cleared, kills and coins earned with RETRY / OTHER MAP / MAIN MENU.

Code: `experiments/gate_core.rs` (engine-free, seeded, unit-tested: path following, targeting rules, damage/armor/splash/slow/
burn/stun/pierce, economy cost/refund/upgrade, wave table and scaling, hero assist, win/lose, a bot beating a map, full-run
determinism, tick cost with 150 foes and 30 towers), `gate.rs` (arena, Mario-action glue, build-menu state machine, publishing),
`gate_view.rs` (bridge, pooled foe/tower models, HUD, build bar, tower card, preview, map picker, summary, tap-to-ground ray).
Env hooks: `JUMPMAN_GATE_SEED`, `_MAP=0|1` (skips the picker), `_WAVE=<n>`, `_COINS=<n>`, `_LIVES=<n>`, `_GOD=1`, `_BOT=1` (automatic
builder), `_SPEED=2`, `_TOWERS="kind:level@x,y;..."`, `_FOES="kind:offset;..."`, `_FREEZE=1`, `_MENU=build@x,y|card@x,y` (+ `_SEL=<kind>`),
`_END=won|lost` (summary), `_ZOOM`/`_FOCUS=x,y` (camera), `_SCRIPT="2:tap@5,3;3:pick2;4:pick2;6:upgrade"` (UI commands at run-relative
seconds: tap@x,y, pick<k>, confirm, upgrade, sell, mode, close, speed, pause, wave, interact, map0/map1, retry, othermap; for end-to-end checks without a pointer). Runs using them never touch the saved best wave. Balance report (bot, no hero help, 12 seeds): `cargo test --release balance_report -- --ignored --nocapture` (Meadow Pass 12/12 wins with ~8 lives left, Twin Fords 12/12 with ~12; `a_simple_bot_wins_every_map_without_the_hero` guards it).

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
* **Daily seed**: a **DAILY** toggle sits in the top bar of a fresh map (it re-rolls the map at once) and beside RETRY on the
  summary card; the choice is kept in `stardeck.json`. With it on, every run (and RETRY) uses the shared seed of the UTC day, so
  everyone gets the same map, shops and rewards. `JUMPMAN_DECK_SEED=daily` still forces it.
* **Controls**: tap a card to select (it grows), tap it again to play, or drag it up; tap a foe to aim a targeted card; END TURN,
  piles, DECK and potions are buttons (all 44px+ and inside the safe area, clear of the pause button). Keyboard: left/right + Enter (on a targeted card with several foes alive, Enter starts **aiming**: left/right walk the foes left to right with a pulsing ring and a white-bordered plate, Enter strikes, Backspace goes back to the hand),
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
`_TEMPO=<k>` (animation time scale; 0.12 with `_FAST=1`), script words `aim` / `aim.l` / `aim.r` / `aim.go` and `daily`, plus the global `JUMPMAN_WINDOW=874x402` for phone-sized screenshots.
Runs using the hooks never touch the saved bests. The campfire is a ring of stones, logs, a ground glow and four layers of additive flame tongues with rising embers; shop, event and treasure props stand in the strips the panels leave free at phone size. Balance: the policy bot wins about 45% of 1200 runs at the default difficulty (Mario 41, Link 39, Young Link 43, Banjo 57; `every_hero_has_a_healthy_bot_win_rate` keeps each hero between 33% and 67%; `cargo test --release win_rate_report -- --ignored --nocapture`).

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
  respawn. Dying returns to the last pedestal with full health; pickups, bosses and opened doors stay done. A broken floor plate is a one-way shortcut and **re-seals whenever its room is entered again** (so a hole can never split a room for someone who comes back without the move, and the shortcut can be taken again). Falling into a pit costs
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
cracks, ink, save round trip, map state), `halls.rs` (glue: sound cues for pickups (fragments climb in pitch, 1-up for hearts), gates opening (bars, locks, floors), doors and boss hits / phases / defeat; collision and meshes per room, the controller gating, wing flutter,
cap worn, fades, camera, plus **physical tests**: the real controller on each room's collision long-jumps the Hedge Run and Gear
Foyer gaps, wall-kicks up the Greenhouse and Clock Shaft columns, flutters onto the Foyer ledge and the Clock Shaft slab, and
fails every one of them without its ability), `halls_view.rs` (bridge, pooled models, touch / pad input) and `halls_ui.rs`
(HUD, map, prompts on `bevy_ui`).
Env hooks (any of them makes the run ephemeral): `JUMPMAN_HALLS_NEW=1`, `_ROOM=<index|name>`, `_ABILITIES=all|LJ,WK,GP,WG,MT,VN`,
`_CAP=wing|metal|vanish`, `_GOD=1`, `_POS=x,y,z` (room units), `_FRAGMENTS=<n>`, `_KEY=1`, `_BOSSES=<mask>`, `_DISCOVER=all`,
`_SCREEN=map|prompt|dead|win`, `_SHOWCASE=1` (one of each foe), `_DEMO=wallkick|flutter|longjump|pound|north` (an autopilot that
performs the move, for screenshots).

## Gem Clash (experiment)
An original **arena brawler**, native-only (no browser version): two to four fighters (you as Mario plus up to three bots, or extra
local gamepads) in six small multi-level arenas, with throwable props, weapons, three gems per round and a timed power form. It is
a genre-inspired homage to the "grab everything, collect the gems, transform and fight" arena brawlers; the rules, arenas, items,
names and art direction are our own and nothing comes from any commercial game. Pick **Gem Clash** from the green experiment
buttons on the title or pause menu, or run `cargo run --release -- bob --experiment brawl`. A setup screen opens first (mode, arena,
opponents, bot level).

* **Fighters**: everyone (the player too) runs on one engine-free simulation at 30 Hz with SM64-flavoured moves: run, jump and a
  second jump in the air, a jab-jab-kick chain (the kick knocks down), a dive (B in the air), a ground pound with a shockwave
  (Z in the air), grab / throw (Z on the ground). The player is drawn as the real Mario model whose animation is driven from the
  fighter's state (it is **not** the full `MarioRuntime`: one shared rule set keeps player and bots fair and unit-testable); bots
  are Goomba, Bob-omb, Koopa, Bully, Chuckya and the boss King Bob-omb (SM64 actor models through `spawn_model`, clips playing).
  Knock-outs are by health bar (100 hp); knockback grows as health falls; a solid hit pops a gem out, a knock-down shakes them all
  loose; falling off an open arena costs 18 hp and puts you back on a spawn.
* **Items** (8 kinds, dropped from the sky every few seconds and scattered at the start): crate, barrel, rock (thrown for 11 / 16 /
  14 damage), Bob-omb (lit when picked up, explodes after about 2.6 s wherever it is: throw it), hammer (4 swings, 22 damage, big
  knockback), boomerang (flies out, returns, hits both ways), blaster (12 shots), 1-Up mushroom (heals 32). At most two weapons exist at once.
* **Gems and the power form**: three gems (Ember, Tide, Leaf) fall each round; hold all three and you transform for 14 s: bigger
  (x1.5), faster (x1.3), x1.5 damage dealt and x0.7 taken, plus **Nova** (an area blast, 24 damage, costs 2.5 s of form) and the
  **Seeker beam** (locks the nearest foe within 1500, 22 damage, costs 1.6 s). The gems are used up; a fresh batch falls a few
  seconds after the form ends. Bots chase gems, chase whoever carries them and use the specials.
* **Arenas** (each with its own set piece; open arenas have no walls): **Gearworks** (two lifts to long gantries, sliding platforms,
  open drops), **Ember Basin** (a lava tide rises every 26 s with a warning; the rocks stay safe), **Bumper Garden** (five bumper
  pillars fling fighters and items), **Splitrock Bridge** (seven planks crumble 0.5 s after you stand on them and return),
  **Gale Summit** (gusts from a random side, telegraphed, over a stepped tower and open edges), **Cannon Court** (the cannon lobs a
  telegraphed shell at the healthiest fighter every 5 s, faster in the last 35 s).
* **Modes**: **Versus** (best of three rounds, 100 s each; time-out goes to the healthier fighter, ties replay) and the single-player
  **Gauntlet**: six stages with escalating bots (Goomba on Bumper Garden up to a Hard Bully / Chuckya / Koopa trio on Gearworks) then
  King Bob-omb (340 hp, 2.3x size, hard to knock down, stomps) with two Goombas on Cannon Court; one round win clears a stage, you
  have three lives. Bot levels Easy / Normal / Hard differ in reaction time, combo use, dodging, item and gem greed, hazard
  awareness and aim lead.
* **Bots** (`brawl_ai.rs`): a goal state machine (wander, item, gem, chase, flee, heart, high ground) with a per-tick fight layer
  (punch / dive / pound / throw / hammer / blaster / boomerang / specials), edge, lava, bumper, plank and shell avoidance, no
  allocation per tick.
* **Camera**: a dynamic arena camera frames every fighter (feet and heads, with margins for the HUD and the touch buttons) by solving
  the distance from the field of view and the window aspect each frame, smoothed (zooms out fast, in slowly); a steeper pitch on
  phones and on Gearworks.
* **Controls**: stick moves; A jump (again in the air); B hit (jab-jab-kick, dive in the air, throw / use the held item); Z grab /
  throw on the ground, ground pound in the air. Keyboard: Space / Shift / Ctrl as ever, `F` grab / throw anywhere, `G` Nova, `H`
  Seeker beam; touch has JUMP, HIT and GRAB (labelled through `ButtonLabels`), L as an explicit grab / throw and NOVA / BEAM buttons;
  a gamepad adds North = Nova, RB = Seeker beam, LB = grab. Extra connected pads join as players 2-4 in Versus (South jump, West hit,
  East grab, North Nova, RB Beam, triggers pound) and replace bots. Menu: arrows / WASD + Enter or the on-screen buttons.
* **HUD**: a card per fighter (name, health bar, three gem slots, round pips, a power bar), round and clock between the cards, a big
  banner (ROUND / FIGHT / K.O. / power-up), hazard warnings and a hint line; compact on phones (smaller fonts, safe-area insets,
  room kept for the pause button).
* **Sound**: reuses the SM64 effects (punch / dive / pound / explosion / break box / splash / cannon), plus new effect kinds in
  `audio/mod.rs`: `gem1`..`gem3` (rising pings), `power_up` and `round_end`.
* **Persistence**: `brawl.json` beside the save files keeps the best Gauntlet stage, Gauntlet clears and Versus match wins; runs using
  test hooks never write it.

Code: `experiments/brawl_arena.rs` (seeded RNG, vector helpers, the six arenas and their set pieces), `brawl_core.rs` (rules:
fighters, moves, hits, items, projectiles, gems, power form, hazards, rounds, Versus / Gauntlet flow), `brawl_ai.rs` (bots),
`brawl_tests.rs` (about 45 engine-free tests: damage and knockback, combo, dive, pound, grab / throw, bomb fuse, hammer wear-out,
blaster ammo, boomerang return, heals, gem transform and timing, nova / beam, gem drops, round flow and best of three, time-out
and draws, void fall, lava, bumpers, cannon, wind, crumbling planks, lifts, bot sanity, determinism by seed, a bot-vs-bot soak of
every arena that always terminates, a full Gauntlet run, ally rules, drop caps and the **balance report**), `brawl.rs` (glue:
input, sound, camera, arena entities, Mario pose, flow, summary, record) and `brawl_view.rs` (`BrawlBridge`, pooled models, HUD,
menu, summary, touch / pad input). Balance report (`cargo test --release balance -- --nocapture`, one-round duels, same fighter
kind, seats swapped, 6 arenas x 10 seeds = 60 duels per row): Hard beats Normal 38-22, Normal beats Easy 48-12, Hard beats Easy 48-12, Normal mirror 34-26 (seat noise); four equal Normal bots, 48 matches: seat wins 11 / 11 / 12 / 14. About 55 fps with four fighters on an M-series Mac (measured while compiling, display-capped).
Env hooks (any state-changing one keeps the record untouched): `JUMPMAN_BRAWL_SEED=<n>`, `_MODE=versus|gauntlet`, `_ARENA=gear|ember|
bumper|bridge|gale|cannon|<0-5>` (also skips the setup screen), `_BOTS=<1-3>`, `_DIFF=easy|normal|hard`, `_STAGE=<0-5>`, `_ROUNDS=<n>`,
`_START=1`, `_DEMO=1` (a Hard bot plays your fighter), `_GEMS=<0-3>`, `_POWER=1`, `_ITEMS=hammer,bomb,blaster,...`, `_FIGHT=1` (skip the
countdown), `_T=<secs>` (arena clock; 16 is high tide, 7 a gust), `_HP=<slot>:<hp>[,...]`, `_GOD=1`, `_SPEED=<k>` (core ticks per step).
Phone-size screenshots: `JUMPMAN_WINDOW=874x402 JUMPMAN_TOUCH=1 JUMPMAN_SAFE_AREA=59,59,21,0`.

## Sunstone Shindig (experiment)
An original **board-game party mode**, native-only (no browser version): four players (you plus three CPUs, or up to four
people hot-seat / on gamepads) race around a themed board for 10 to 15 rounds, a **mini-game** is played after every round, and the
Sunstone, bought from a roaming Keeper, decides the winner. It is a genre-inspired homage to the "dice-roll board plus
mini-game collection" party games, but the rules, boards, spaces, items, all 15 mini-games, names and art direction are our own;
nothing is copied from any commercial game. Pick **Sunstone Shindig** from the green experiment buttons on the title or pause
menu, or run `cargo run --release -- bob --experiment party` (or `JUMPMAN_EXPERIMENT=party`). The pieces are the real SM64 Mario
rig (four caps / overalls colours) with its animation clips, driven by the same `CrowdMario` runtime as the crowd modes.

* **Turn**: optionally use one item, then **ROLL**: a 1-10 number block spins and you tap **STOP** to freeze it (a timing roll;
  CPUs draw it from the seeded RNG). Walk that many spaces; at a fork the choices glow and are lettered **A / B** on the map (the
  buttons say what lies ahead and how far the Keeper is). Passing **Home** pays 3 coins; passing the Keeper with 20 coins stops you
  to offer the Sunstone.
* **Spaces** (the colour tells the team in the next mini-game: sun-tinted blue / green / gold / cyan, moon-tinted red / purple /
  pink): **Coins** +3, **Pit** -3, **Event** (10 outcomes: Coin Rain, ghost thief, gift, Gust of Fortune (3 more spaces), Swap Spots,
  Robin Hood, Tax Toad, Lucky Charm (+2 next roll), Party Favor, Keeper rumor), **Gift** (a random item), **Shop** (item shop),
  **Warp** (jumps to another pad), **Duel** (pick a rival, play a two-player mini-game now: the winner takes up to 5 coins),
  **Hex** (Slow Boots: next roll capped at 3, Leaky Pockets -5, Glum: lose an item) and **Keeper** sites.
* **Items** (two slots, one use per turn, before the roll): **Dash Die** (roll 5-10), **Twin Dice** (2d6, an even total pays a coin),
  **Keeper Whistle** (warp to the Keeper), **Pickpocket** (steal up to 6 coins from a rival). Item shop prices 6 / 5 / 10 / 7.
* **The Sunstone** costs 20 coins at the Keeper; after each sale (or a rumor) the Keeper moves to another site and the beam of light
  follows. **Mini-game after every round**: how it is played depends on the space tints the four of you ended on: all the same =
  free-for-all (1st +8 coins, 2nd +4), two and two = **2v2** (winners +6 each), one against three = **1v3** (the lone player +10,
  the trio +5 each); team score is the members' average. A duel space plays a two-player game on the spot.
* **End**: three bonus awards, each worth a Sunstone: **Coin Hoarder** (most coins), **Mini-game Master** (most wins) and one
  space-kind award chosen by the seed (Coin Collector, Unlucky Star, Event Fan, Window Shopper or Duelist); ties share. Most
  Sunstones wins, coins break ties. The **results ceremony** reveals the awards one by one then the podium (real Marios on
  pedestals, winners hop).
* **Boards** (fixed layouts, each with its own mechanic):
  * **Hourglass Keep** (30 spaces): a castle courtyard with two **drawbridges** over the middle; only one is down at a time and
    they swap every round. Crossing is a ten-space shortcut.
  * **Cloudtop Isles** (34): floating isles on a cloud ring, with three **updraft shortcuts** that exist only while the wind blows
    (the wind is re-rolled each round).
  * **Cinder Crown** (42): a volcano of three rings with stairs up and slides down; the **lava tide** floods the outer ring after
    round 3 and the middle ring after round 7 (flooded spaces burn 4 coins, standing there when it rises costs 3, a flooded Keeper
    site is abandoned).
* **Mini-games** (10-30 s, one big button, tiles or a single stick; every one has an intro card with instructions and a practice
  mode from the **Mini-games** list on the title screen; the list keeps your best score for each):
  Barrel Hop (tap to jump rolling barrels in your lane), Mash Mountain (tap-mash up a pole, tiring if you mash too hard), Echo Pads
  (memory sequence), Coin Scramble (stick, grab coins, gold is 3), Bomb Drizzle (stick, dodge bombs, last one standing), Pound Grid
  (tap the glowing pad first), Plank Balance (stick against the tilt), Shell Lane (stick up/down to swap rows: dodge rocks, hit pads),
  Odd One Out (tap the different tile), Bumper Brawl (stick + A dash, shove rivals off a shrinking ring), Crumble Floor (stick,
  tiles drop away), Quick Draw (tap when the flag goes green, early = out), Block Stack (tap to drop the sliding block),
  Star Count (count flitting stars, tap the number), Power Meter (stop the marker in the gold zone).
* **Controls**: everything is a big button or tile (44 px and up, inside the safe area, clear of the pause button at the top right;
  on the board the touch pad is hidden, in a stick mini-game it shows). Keyboard: arrows / Tab + Enter or Space press the focused
  button (the first button is always the primary action, e.g. ROLL / STOP), in games WASD / arrows move, Space / Enter is the big
  button or A, `1`-`9` pick tiles. Gamepad: d-pad + South for menus, left stick + South in games; seats 2-4 of a multi-person game
  use the other gamepads in mini-games (set **People** on the title screen; on the board the people take turns on one screen).
* **CPU difficulty** (Easy / Normal / Hard, a title-screen setting): on the board it changes fork choices (look-ahead path value,
  Keeper-seeking), item use, shop reserve and duel targets; in mini-games it changes reaction time, aim error and speed (each
  game has its own bot policy).
* **Save**: `party.json` beside the other saves (`progress::save::data_dir`, the app container on iOS): games, wins, best Sunstones
  per board and the best score per mini-game as played by a person. Test-hook runs never write it.
* **Audio** reuses the shared effect cues (`SfxCue::Effect` kinds): die land, step, coin / hurt for coin changes, star + confetti
  for a Sunstone, one-up for gifts and the win fanfare, select-mission for a mini-game start, move-menu ticks for the countdown,
  teleport for warps and a Keeper move, explosion for the tide. Nothing new in `audio/`; not verified by ear.

Code (`native/src/experiments/party/`): `common.rs` (seeded RNG, bot `Level`, `PadIn`, the stage draw list, ranking),
`minis/*.rs` (one engine-free module per mini-game behind the `MiniGame` trait: deterministic core, bot policy, scene; `Mini`
catalogue and `Session` in `minis/mod.rs`), `board.rs` (the three boards as graphs with `Conditions` for bridge / wind / tide),
`game.rs` + `game/` (turn flow, spaces, events, items, shop, Keeper, mini-game formats and rewards, awards, bots, headless
autoplay), `app.rs` (state, experiment shell, save, hooks), `flow.rs` (UI commands, event animation queue, CPU turns, the mini-game
runner), `view.rs` (board props, a pooled primitive stage for mini-games, the pieces, cameras), `ui.rs` (all screens on `bevy_ui`).
`PartyApp` is a plain resource inserted on enter (no host field). The core is tested without the engine: every board reaches the end
under autoplay for 10 / 12 / 15 rounds and 25 seeds, determinism by seed, Sunstones conserved between sales and bonuses, the 20-coin
price and the Keeper moving, passing the Keeper, items, shop limits, die range and Slow Boots, mini-game formats from tints and
their rewards, duels, the bridge flipping each round, wind shortcuts, the tide flooding the lowest ring, bonus awards and ties, bot
fork choices; per mini-game: all 15 terminate under bot-vs-bot soak (12 seeds x 3 levels x 3 formats), are deterministic by seed,
keep avatars on stage, an idle player never wins half the time, and Hard bots beat Easy ones.
Env hooks (runs using `_SEED`, `_SKIP`, `_AUTOPLAY`, `_SCRIPT` or `_START` never touch the saved records):
`JUMPMAN_PARTY_BOARD=0|1|2` (start that board), `_MINIGAME=barrelhop|mash|echo|coins|bombs|pound|plank|lane|odd|bumper|crumble|draw|stack|stars|meter`
(open its intro card for practice), `_SEED=<n>`, `_TURNS=10..15`, `_LEVEL=easy|normal|hard`, `_AUTOPLAY=1` (bots play every seat,
including yours, with short animations), `_SKIP=<rounds>` (fast-forward with bots), `_START=title|minis|overview|results|shop|keeper|duel|branch|mini|miniresult`
(`results` plays a whole bot game; the others fast-forward until you face that decision), `_REVEAL=<s>` (results reveal time),
`_SCRIPT=1.0=roll,1.5=stop,4=choose.12,6=keeper.1,...` (UI commands at times: `roll stop choose.<space> keeper.<0|1> buy.<0-3> leave
duel.<seat> item.<slot> target.<slot>.<seat> ack go again finish next overview tap tile.<n> back start board.<n> minis practice.<key>`),
`_FAST=1`, `_HUMANS=<n>`, plus the global `JUMPMAN_WINDOW=874x402` for phone-sized screenshots.
Balance (bots, no human): `cargo test --release --lib party -- --ignored --nocapture` prints both reports. Board games, 400 seeds
of 12 rounds, four Normal CPUs: seat win rates 21-31% (fair share 25%); about 7.1 / 6.4 / 5.0 Sunstones are bought per game on
Hourglass Keep / Cloudtop Isles / Cinder Crown, nearly every game sees four or more purchases; a Hard CPU against three Easy ones
wins 96%. Mini-games, 120 seeds each: seat shares 14-35% (sampling noise is about 4 points; the four most uneven games re-checked at 600 seeds sit at 21-27% for every seat); a Hard
bot beats three Easy ones 39-100% of the time (Shell Lane is the least skill-sensitive; Bomb Drizzle always ties among Normal CPUs that
all survive, so only mistakes decide it).
Not verified: real touch and gamepad input (the controls are driven through the same commands in scripted runs and by the mouse),
audio by ear, and multi-person mini-games with more than one gamepad.

## Game controllers (iOS and macOS)

`src/controller/` bridges GameController.framework (`gc_shim.m`, compiled by `build.rs`) into Bevy `Gamepad`
entities, so play input, menus, rebinding and every experiment read pads unchanged. Xbox, DualSense/DualShock 4,
Switch Pro and MFi pads work on iPhone and Mac (pair in Settings > Bluetooth; hold the pairing button until it flashes).
Layout is positional (A = bottom face button), Menu = Start (pause), View/Share/Create = Select, triggers are analog, the
Guide/PS/Home button is ignored. Several pads take player seats in connect order (the pad's LED shows the seat).
Losing a pad while playing pauses; the first connect hides the touch overlay on iPhone (touching the screen brings it back).
`JUMPMAN_FAKE_PAD=<1-4>` scripts virtual pads (walk forward, tap A every 2 s) through the same pipeline.
The old gilrs backend is available as `--features gilrs` (generic HID pads on desktop; it double-registers pads GameController also sees).

## Performance settings and "ray tracing"
Settings > AUDIO AND PERFORMANCE (saved in `settings.json`, all apply live, defaults off):
* **SHOW FPS** - a small pill at the top centre (below the course title, inside the safe area, clear of the HUD corners and touch
  controls): frames per second, average frame ms over 1 s, and the simulation tick rate. Works in menus, courses and every experiment
  (`src/perf.rs`; `JUMPMAN_FPS=1` prints the same numbers to stdout).
* **UNCAP FPS** - off = vsync (`PresentMode::AutoVsync`), on = `AutoNoVsync` (Immediate, else Mailbox) and the unfocused-window
  throttle is lifted (`WinitSettings`). On iPhone wgpu cannot turn display sync off (`CAMetalLayer.displaySyncEnabled` is macOS only),
  so the toggle instead asks for 80-120 Hz through a `CADisplayLink.preferredFrameRateRange` (`ios/Sources/refresh.m`; `Info.plist`
  has `CADisableMinimumFrameDurationOnPhone`); off asks for 30-60 Hz. The 30 Hz simulation is a fixed step with interpolated
  rendering, so frame rate never changes game speed (test `perf::tests::fixed_step_count_is_independent_of_frame_rate`).
  Measured on this Mac (Apple M5 Pro, 120 Hz display, windowed): uncapped runs at exactly 120.0 fps in menus, Castle Grounds and with
  enhanced lighting, i.e. windowed macOS still presents at the display rate; the app-side limiter is gone, the compositor's is not.
* **Enhanced lighting** (Off / On) - screen-space ambient occlusion (SSAO, high quality), Gaussian-filtered soft shadows with a 4096
  shadow map (2048 when off), prepasses and FXAA (SSAO requires MSAA off). The effect is **subtle** on the flat-lit N64 materials
  (visible as contact darkening on Mario and soft shadow edges) and it is **not ray tracing**. No frame-time change was measurable on
  the M5 Pro (both stay display-bound at 8.3 ms, i.e. 120 fps uncapped). Capped, an unfocused window runs at ~55 fps because Bevy
  throttles unfocused windows; a focused capped window follows the display refresh (not measured: the test runs were unfocused).

### Ray tracing: not possible in this build
Hardware ray tracing was investigated and is **not available**, so there is no ray-tracing option (a disabled note is shown in Settings):
* Bevy 0.16.1 has no `bevy_solari` (it first ships in a later Bevy); `~/.cargo/registry/src/*/bevy-0.16.1/Cargo.toml` has no such feature.
* The pinned wgpu 24.0.5 / wgpu-hal 24.0.4 Metal backend stubs out `build_acceleration_structures` and never exposes
  `EXPERIMENTAL_RAY_QUERY` (`wgpu-hal/src/metal/adapter.rs` reports `ray_tracing_scratch_buffer_alignment: 0`). Apple Silicon and A17 Pro+ have
  Metal ray tracing in hardware, but nothing in this dependency stack can reach it.
* Getting real RT means upgrading Bevy and wgpu to versions whose Metal backend implements ray queries, then building BLAS/TLAS for
  the course meshes and animated actors; the iOS Simulator has no RT hardware either. That is a separate, large piece of work.
Screen-space reflections (`ScreenSpaceReflections`) were also left out: they need the deferred prepass and deferred materials, but the
course, sky and water materials here are forward-rendered custom materials.

## Banjo-Kazooie worlds (geometry, collision, enemies, collectibles)
Banjo-Kazooie worlds are playable courses in the LEVELS tab (orange chip border, `Series::Banjo`, `bk_*` ids): Spiral
Mountain, Mumbo's Mountain, Treasure Trove Cove, Clanker's Cavern, Bubblegloop Swamp, Gobi's Valley, Freezeezy Peak, Rusty
Bucket Bay, Mad Monster Mansion and the four Click Clock Wood seasons. Mario, Link, Young Link and Banjo all walk them
with the SM64 physics. The worlds are static terrain plus the enemies and collectibles of their setup files (below); there
are no doors, warps, music or moving parts, and the worlds are "exploration only" (no star).
- `scripts/export-bk-levels.py` (stdlib only; shares the rarezip reader of `export-bk-banjo.py`; wired into the BYOR
  pipeline's `bk` step) reads each world's two level models from the ROM asset table (`*_OPA` and `*_XLU`, listed in the
  decomp's `mapModel.c`). Display lists are walked through the geo layout (nearest LOD, camera-area branches all on,
  selectors off) into one glTF per world (`assets/levels/bk_*/terrain.glb`: one primitive per texture/wrap, CI4/CI8/RGBA16/
  RGBA32 decoded to PNG, prelit vertex colours, cut-out textures `MASK`, the XLU model `BLEND`, unlit). The `BKCollisionList`
  triangles (deduplicated across cells) become the course JSON's `vertices`/`triangles`; two-sided flagged triangles are
  doubled; water-flagged triangles (`0x1E0000`) become `waterBoxes` (id 45, rasterised in 128-unit cells) so Mario swims
  while the model draws its own water (`fx/water` skips id 45). World units are scaled by 1.25 (the Banjo rig's scale).
- The course JSON also carries a `sky` object (gradient stops, fog colour and range) that `render/course_view.rs` and
  `fx/sky.rs` apply; courses without it keep the daylight dome.
- Spawns: Spiral Mountain is hand-picked; the others use `pick_spawn`, the lowest broad flat dry clearing. They are open
  ground, not the game's entry warps. Surface flags are not mapped to SM64 types yet (everything is "default"; steep BK
  slopes slide through SM64's slope rules); death is the SM64 height rule (lowest vertex - 500).
- Tests: `src/game/bk_tests.rs` (spawn floor exists, Mario stands, walks 50+ units in each direction, glb budgets).

### Enemies and collectibles of the BK worlds (all four characters fight them)
- `scripts/export-bk-setup.py` parses each map's **setup file** (ROM asset `map + 0x71C`; `gsworld_load` -> `cubeList_fromFile` ->
  `code7AF80_initCubeFromFile`: a grid of 1000-unit cubes holding 20-byte `NodeProp`s and 12-byte props) for the 14 exported
  worlds. Category-6 nodes are actors (`cube_spawnActors`: actor id, position, yaw in degrees, scale in percent); the sprite
  props are the notes (0x6D6), blue eggs (0x6D7), red (0x580) and gold (0x6D1) feathers. Enemies are written into each
  `assets/courses/bk_*.json` as `objects` entries `{"behavior": "bkEnemy", "param": actorId | scale% << 16}`; the pickups as
  `bkPickups` `[kind, x, y, z]`; `assets/courses/bk_setup_report.json` lists every placed actor id per world with the
  implemented / skipped split that `src/bk_enemies/tests.rs` checks. Positions are x1.25 like the terrain.
- `scripts/export-bk-enemies.py` exports 12 rigged models (`public/models/bk/enemies/*.json`, 1.1 MB: the Banjo rig format plus
  `states[]`, the decomp's `ActorAnimationInfo` table indexed by actor state, read from the C sources). The hood's clothes follow the
  Click Clock Wood season (`chgrublinhood_draw` appendages). Both exporters run in the BYOR `bk` step after the levels.
- Behavior: `sim/src/bk_enemy.rs`, ONE `ObjectBehaviorKind::BkEnemy` for all species, keeping the decomp's actor state numbers
  (`SimObject::action` is the BK state, so the animation table indexes it). Species and source functions: Grublin, Grublin Hood and
  Limbo (`humanoidBaddie_update`: idle/random chase/turn/alert counts/chase, speeds from each `*_initialize`), Ticker, Bigbutt
  (snort x3, charge 13 to 30 u/frame, slide, gore, knock-down 4 s), Snippet / Mutie Snippet (hit flips it for 3 s, the next hit kills),
  Sir Slush + snowball (tracks at 6 deg/frame, throws from 500..2750 units, 32-frame lob with the decomp's lead), Bat (roost,
  dive at 800 units, flap away, fall), Shrapnel (water mine, grows faster, explodes), Snare-bear (snap window) and the Beehive
  (breaks, heals one wedge). Not ported: see the skip list in `export-bk-setup.py` (Buzzbomb, Flibbit, Clucker, Caterpillar, Grille
  Chompa, Grimlet, Flotsam, Scarab: their models use the skeletal-animation path; bosses and minigame actors).
- Everything goes through the shared interaction pass, so **Mario** (punch, kick, stomp, ground pound, dive), **Link** (sword reach,
  arrows, bombs, hammer; `link::moves::hit_object`; the Z-target picks any `INTERACT_DAMAGE` object) and **Banjo** (claw, peck, Beak
  Buster, barge, eggs) all damage them; punches/kicks/eggs/sword are the decomp's `ow` (weak), the rest its `die` (strong). Contact
  hurts through `take_damage` (wedges and knockback for Mario and Banjo, hearts for Link's bridge). Defeat: puff, loot coins, a 25%
  health wedge; Banjo also gets notes, eggs or red feathers (`bk_enemies::enemy_drops`).
- App side `src/bk_enemies/`: skins the models on the CPU like Banjo (views only for the nearest 18 enemies within 6500 units,
  everything beyond 7000 is frozen and hidden), and streams the real collectibles into Banjo's pickup list (within 2600 units,
  at most 120 live), replacing the sandbox scatter in these worlds.
- Tests: `sim/tests/bk_enemies.rs` (22 engine-free), `src/bk_enemies/tests.rs` (setup counts per world, every placed actor
  implemented or skipped, models, pickup floors/streaming, scripted Mumbo's Mountain scenarios per character, Z-target).

## Ocarina of Time enemies in the OoT courses

`scripts/export-oot-enemies.py` reads the room actor lists of every exported OoT scene (the same
collision-bounds transform as `export-oot-courses.py`) and writes `assets/levels/oot_enemies/<course>.json`
(placements: kind, position, yaw, params, room; `EnEncount1` spawners; the enemy-like actors it skips),
one skinned glb per kind (`actor_model` from the course exporter, ROM skeleton + clips) and the Rust tables
`native/sim/src/oot_enemy/{tables,drop_table}.rs` (every overlay's `DamageTable`, `sItemDropIds`,
`sDropQuantities`, parsed from the decomp C). The BYOR `do_oot` runs it after the courses.

* `native/sim/src/oot_enemy/` (engine-free, 24 unit tests): hit points, damage rows by weapon, drop rolls
  (`Item_DropCollectibleRandom`, age-aware), AI per kind (Deku Baba / Big / Withered, Gohma larva, Skulltula,
  Gold Skulltula, Peahat, Tektite blue/red, Octorok, Mad Scrub, Wolfos, Guay, Leever + the Leever spawner),
  Octorok/Scrub shots with shield reflection, active-radius sleeping (9000 units).
* `native/src/zelda_enemies/` (Bevy glue, 7 course tests): spawns from the placement file, steps the field after the
  player's tick, applies the player's blows, hurts the player, rolls drops (Link: hearts, rupees, ammo, magic;
  Mario/Banjo: wedges and coins), draws models (Baba stems are separate pieces), shots and drops, and publishes the
  enemies to Link's Z-focus. Link's projectiles reach them through `LinkEvent::HitExtra` (ids from `TARGET_BASE`).
* Who hits them: Link's `swing_damage` rows (sword, stick, hammer, spin, jump slash) and `Dmg` of arrows, seeds,
  boomerang, hookshot, bombs and magic; Mario and Banjo through the shared SM64 attack states (punch/kick/dive and so
  Beak Barge and Beak Bomb, slide kick, ground pound and so Beak Buster, stomping from above, Wonderwing) mapped to
  `Dmg` rows, plus Banjo's eggs. They hurt everyone through `take_damage` (Link's hearts lose exactly the C damage).
* Test hooks: none needed; walk to the placements (`assets/levels/oot_enemies/<course>.json`), e.g. `jumpman deku_tree`
  with `JUMPMAN_ZELDA_POS`, `JUMPMAN_CHARACTER=link|mario|banjo`.

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
borderless fullscreen so Bevy sees the real size; menus use the compact phone layout (`shell::menu_layout`, see Menus).
Touch controls use fixed safe-area margins (`touch.rs`). Saves/settings live in the app container.

Universal app (iPhone + iPad, `TARGETED_DEVICE_FAMILY = "1,2"`): iPhone stays landscape-only; iPad rotates freely
(`UISupportedInterfaceOrientations~ipad`, all four). `UIRequiresFullScreen` stays YES, so Split View / Slide Over /
Stage Manager resizing is opted out (the window is always the full screen; rotation still resizes it and the menus,
touch controls and cameras re-layout from the window size). iPads use the desktop menu layout in landscape and the
compact one in portrait (width < 960). The single 1024 AppIcon covers the iPad icon sizes. Layout check without a
device: `JUMPMAN_WINDOW=1180x820 JUMPMAN_SAFE_AREA=0,0,20,0 JUMPMAN_TOUCH=1` on the desktop build. App Store Connect
(not TestFlight) needs iPad screenshots at submission.

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
also works. **F** or the dock's **Get out** button parks the kart
and switches to Mario's full native walking/jumping simulation. Return nearby
and press it again to get in. **R** or the **Reset** button resets both to the
starting line. **X** leaves the experiment.

Luigi Raceway geometry and textures and the occupied kart's directional sprites
come from MK64. Driving is a port of MK64's decompiled player physics (`kart_physics.rs`, matching
`kartPhysics.ts` via a shared golden trace in `kart_physics_trace.txt`), run at 60 Hz as two updates per sim tick;
**Space/A** hops and drifts. Ground contact follows the decomp (`func_8003F46C`/`func_8003E9EC` floor reaction,
`func_8003F734`/`func_8003FBAC`/`func_8002C954` wall bounces with the per-normal restitution values, `slopeAccel`
from the four tyre heights of `func_80029B4C`, the brake-hold force). Falling off the course triggers a Lakitu
rescue (`kart_rescue.rs`): fade to black, back on the nearest path node facing down the track, fade in. The empty
parked kart is a procedural mesh.

On phones (touch overlay showing, window under 520 px tall) the title card is compact and finger-worded and steps
aside after 12 s; the A button is relabelled **HOP/DRIFT** (gold while a drift charges, orange on the mini-turbo),
B **ITEM** (**GET IN** on foot), Z **BRAKE** (`touch::ButtonLabels`, set through `Host::labels`), the dock sits between the stick and
the A/B grid above the home indicator (`dock::touch_dock_insets`), and the status line shows `DRIFT [##] RELEASE` /
`MINI-TURBO!`. Verify at phone size with
`JUMPMAN_WINDOW=874x402 JUMPMAN_TOUCH=1 JUMPMAN_EXPERIMENT=kart JUMPMAN_SCREENSHOT=out.png JUMPMAN_EXIT_AFTER=7`. `cargo test --lib experiments::kart::tests` verifies driving and transfers;
the extracted-track test also runs when the local ROM export is present.

### Items, camera, effects and sound

You arrive in free roam **with item boxes** (the `Phase::Practice` of `kart_race.rs`: the player alone, six box stations along
the lap, no countdown, laps or CPUs; `R` restarts it). Drive through a box for the roulette in the item window (top right,
`KartHud`, icons from `public/mk64/item-*.png`), then **E**, **Shift**, gamepad **B** or the touch item button uses the mushroom,
banana or green shell; the gold **Race · T** dock button starts the race below. Test hooks for screenshots:
`JUMPMAN_KART_ITEM=mushroom|banana|shell` hands the free-roam player that item 0.5 s in, `JUMPMAN_KART_ITEM_USE=1` uses it at 1.5 s.

The chase camera is the one from MK64's `camera.c` (`kart_camera.rs`, the twin of `kartCamera.ts`, tested with the same numbers):
eye 50 behind and 9.5 up, look-at 70 ahead, 0.4/0.15 follow per 60 Hz frame, a lagging heading with the drift swing, vertical FOV 40
(48 mushroom, 58 spinout), mushroom push-in and wall-bump kick; two camera updates per sim tick, blended by `host.alpha`.
Kart presentation follows `render_player.c` / `kart_dma.c` / `code_80057C60.c`:

* **Sprites** (`kart_sprite.rs` engine-free tables + tests, `kart_sprite_view.rs` placement). Each racer is a 64 x 64 frame chosen
  from 321 per character (`<racer>-kart-frames.png`, 16 x 21 cells, exported for all 8 racers): `func_8002934C` picks the
  slope group (`func_80029200`, `slopeAccel` thresholds 2/3/7/10 degrees, 9 groups) and the view angle
  `camera heading + kart heading + unk_0C0` (the drift slip angle turns the sprite, there are no steering-lean frames) in 2.86 degree
  steps up to 57 degrees, then 9 degree steps (35 frames, frame 20 is never used for driving), mirrored for the other side;
  spin-outs use the `Table1` groups in 9 degree steps. The quad is 18 x 18 course units x 0.75 (`gCharacterSize`) = 108 sim units
  (it was 144), a vertical billboard (tilted to the camera only in the air) 4.5 course units towards the camera, rolled by the
  road's cross slope and pitch (`unk_0CC` / `unk_0D4`). Tumble frames 289-320 are exported but not used: the native race has no tumble.
* **Shadow**: `gTextureKartShadow` (64 x 64 kart-shaped I8, `mk64-kart-shadow.png`) on an 18 x 18 unit quad x 0.75, turned with
  the displayed heading, lying on the floor 1 unit up and 2 units towards the camera, scaled `1 - 0.02 * height` while airborne
  and dropped beyond 50 units (`render_player_shadow`, `render_player`). MK64 has no item shadows.
* **Particles** (`kart_puffs.rs`, ticked at 30 Hz as ring pools of 10 slots like the game, positions re-evaluated each frame from
  the interpolated kart): exhaust puffs behind the tailpipe on every kart (`func_80060504`: 3 in 5 chance per tick with the
  throttle, 3 in 14 without; 12 ticks; scale 0.5 + 0.07 per tick, alpha 112 minus 3 per tick; yellow and 128 under a mushroom),
  drift smoke from the outside back tyre every 3 ticks (white, yellow, orange by drift tier; 16 x 16 texture, 32 x 32 after 50
  drift ticks), tyre grass from the back tyres above 10 km/h, mushroom sparks circling the player. Asphalt kicks up no dust in
  MK64. `kart_fx.rs` keeps the stars of a spun-out kart and the item-box shards.
  Test hook: `JUMPMAN_KART_FX=drift0|drift1|drift2|boost|grass|spin`.

Sound is synthesised (the MK64 samples are not in the repository): `kart_audio.rs` streams a `KartSynth` as a Bevy audio source
while the experiment is active, playing the layer tables of `assets/audio/kart-voices.json` (shared with `kartAudio.ts`) plus a
looped engine note whose pitch follows the speed and a skid layer; it follows the global mute (`GameAudio::is_muted`). The
experiment reaches it through module functions (`kart_audio::set_engine/play/set_active`), so `Host` did not grow a field. Cue
logic is `kart_sound.rs`. `JUMPMAN_NO_AUDIO=1` skips the stream. Not verified by ear.

### Race mode

**T** (or the dock's **Race** button) starts a three-lap race against three CPU karts; **E**, **Shift**, gamepad **B** or the
round touch button (the first extra button above the A/B grid, labelled with the held item) uses the item, **R**
puts you back on the last checkpoint, **T** retries, **N** returns to free roam. Rules live in
`kart_race.rs` (engine-free: ordered gates derived from the track `path`, laps, ranking, CPU `Driver` on
`kart_physics`, items; twin of `kartRace.ts`), presentation in `kart_race_view.rs` (CPU sprites on their own ROM atlases - Luigi,
Peach, Toad - pooled item boxes/bananas/shells, and the `KartHud` resource), with the MK64-style HUD in `kart_hud.rs`: the item window with
the ROM icons and roulette, the big `1st`..`4th`, `LAP 2/3`, the clock in MK64's yellow number font, a racer strip with the face portraits,
a minimap (the ROM outline with a marker per racer), Lakitu with the start lights / lap signs / chequered flag, and a results card with
faces and times, plus the touch item button through `ModeButtons`. All pictures are ROM textures exported by `scripts/export-mk64.py`
into `public/mk64` (`hud-*`, `lakitu-*`, `mk64-*`, `<name>-kart.png`). More hooks: `JUMPMAN_KART_AUTO=1|drift` lets the CPU driver steer
the player, `JUMPMAN_KART_AUTO_ITEM=<ticks>` uses the held item then, `JUMPMAN_KART_ITEM=props` drops bananas and a shell on the track. Bests are saved to `kart.json` in the data dir (`JUMPMAN_SAVE_DIR`). Test hooks:
`JUMPMAN_KART_RACE=1|item|banana|shell|finish` starts a race at launch (`item`/`banana`/`shell` hand the player that
item 0.5 s after GO, `finish` runs the player through all laps 1.5 s after GO to show the results card; use
`JUMPMAN_SAVE_DIR=<tmp>` so the fake times are not saved), `JUMPMAN_XP_KEYS="4.0:KeyE"` presses E,
`JUMPMAN_WINDOW=874x402 JUMPMAN_TOUCH=1` for the phone layout. `cargo test --lib kart_race` runs the rules plus the real-track
tests (three CPUs lap Luigi Raceway on the asphalt, replay per seed).

### Mario Kart 64 courses: kart tracks and walkable levels

`scripts/export-mk64.py` (with the table in `scripts/mk64_courses.py`) exports **all 20 courses** of the decomp's `courses/`
tree (16 race tracks and the 4 battle arenas Big Donut, Block Fort, Double Deck, Skyscraper) from the same ROM:

* `assets/courses/mk64_<dir>.json` + `assets/levels/mk64_<dir>/terrain.glb`: the course as an ordinary walkable course.
  They appear in the LEVELS tab as the **Mario Kart** series (red chips) and work with every hero (Mario, Link, Young Link,
  Banjo). Collision is what the game itself builds at load time (`course_generate_collision_mesh`): the triangles of the
  display lists named by the course's `TrackSections` table (plus the few per-course extra lists in `course_generate_collision_mesh`),
  honouring the section flags (wall-only / floor-only lists, vertex flag 4 = no collision). The MK64 surface type maps to an
  SM64 surface kind: snow is slippery, Sherbet Land ice is very slippery, `OUT_OF_BOUNDS` is a death plane, everything else
  is not slippery. Floors are made upright, steep or downward triangles get a ceiling twin and walls are solid from both sides,
  so the guard walls of the tracks stop Mario from either side while the real edges (Rainbow Road, cliffs) still drop him.
  Scale is 8 world units per course unit (`mk64.unit`, the same `UNIT` as the kart): a track is about 12-20 Marios wide.
  Spawn is the first lap-path node, facing down the track (battle arenas: the item box nearest the middle). The visual is one
  glTF primitive per material (texture, wrap/mirror/clamp mode, translucent or not), textures embedded as PNG, vertex colours
  baked in, unlit; coplanar overdraw (road kerbs, lane paint, sand over grass) is resolved at export by stacking decal layers
  0.9 world units apart; the water / glass lists whose vertex alpha the game sets at load (`find_vtx_and_set_colours`) are blended.
  `explorationOnly` is set (no star), sky colours and fog come from the `mk64` block of the json (`fx/sky.rs` `CourseLook`);
  the palettes are hand-picked, the game computes its sky at run time.
* `public/mk64/<dir>.json`: kart-mode data. Luigi Raceway keeps the original display-list groups and PNG textures
  (byte-identical to the earlier export); the other 15 tracks carry only the lap path, the item boxes and a `model` pointing at
  their glb, plus `hud-minimap-<dir>.png` drawn from the path. Their kart collision is the walkable course's collision.
* The Mario Kart 64 mode has a **track picker**: **V** (dock button **Track**) cycles the 16 tracks in cup order (Mushroom, Flower,
  Star, Special); the mode restarts on the new track. `JUMPMAN_KART_COURSE=<dir>` (e.g. `mario_raceway`) picks the first track.
  The four battle arenas have no lap path and are walk-only (LEVELS tab). Race rules, grid, item boxes and Lakitu rescue derive
  from the lap path, so every track can be driven and raced by the player. The three CPU karts (a custom path follower, not the
  decomp's) lap 7 of the 16 tracks (Luigi Raceway, Moo Moo Farm, Koopa Troopa Beach, Kalimari Desert, Toad's Turnpike, Mario
  Raceway, Wario Stadium; `Track.cpu`, checked by `cpu_karts_drive_every_track`); on the other 9 (jumps, forks, narrow bridges,
  no rails) T shows "free roam only" and the mode stays in free roam with item boxes.

Checks: `cargo test --release --lib game::mk64` (all 20 courses load; Mario dropped on the start line lands, stands and walks 8
compass directions), `cargo test --release --lib cpu_karts_drive_every_track`. Screenshots:
`JUMPMAN_COURSE=mk64_mario_raceway JUMPMAN_CHARACTER=banjo JUMPMAN_SCREENSHOT=out.png JUMPMAN_EXIT_AFTER=8`.
Not ported: course props and objects (trees, signs animation, cows, trains, traffic, thwomps, boats, bats), water as a swimmable
volume (Koopa Troopa Beach, DK's Jungle Parkway water is walkable ground or a visual only), lava as a hazard, the in-game minimaps
for the other 15 tracks, MK64 music.
