# Banjo-Kazooie controller

Inventory of the Banjo-Kazooie (N64, US 1.0) player controller from the decomp
(`reference/banjo-kazooie/src/core2/bs/*.c`, the "bs" = Banjo state machine; inputs in `core2/bainput.c`,
`bs/crouch.c`), and what the native engine implements. Code: `sim/src/characters.rs` (the original nine moves),
`sim/src/banjo_ext.rs` (everything added later), `src/banjo_ctl.rs` (input, HUD, eggs, pickups),
`src/characters/banjo_pose_ext.rs` (animation choice). Tests: `sim/tests/banjo_moves.rs`.

Engine model: Banjo is the SM64 `MarioRuntime` with `as_banjo` set. Each BK move is an SM64 action plus an
`action_arg` code (`BANJO_*`), so SM64 course actors (goombas, breakable boxes, bob-ombs...) see the
move through their normal interaction checks. Rare units per second become 30 Hz frames at a 1.25 model scale
(`speed()` / `accel()`), the convention the first nine moves already used.

Inputs. BK uses A, B, Z, the four C buttons and R. The sim carries the real N64 masks (`C_UP 0x8`, `C_DOWN 0x4`,
`C_LEFT 0x2`, `C_RIGHT 0x1`, `R_TRIG 0x10` in `state_consts.rs`), so a recorded N64 input could be fed in unchanged.

| N64 | BK meaning (`bainput.c`) | keyboard | gamepad | touch |
|-----|--------------------------|----------|---------|-------|
| A | jump; in the air Feathery Flap; Flap Flip from crouch | Space | south | A |
| B | Claw Swipe / Roll (fast); in the air Rat-a-tat Rap; crouched Beak Barge; in flight Beak Bomb | J, Shift | east | B |
| Z | crouch (hold); in the air Beak Buster | K, Ctrl | west, north, L2, R2 | Z |
| C-up | egg forward (Z + C-up; C-up alone is first person) | F | D-pad up | EGG |
| C-down | egg backwards | G | D-pad down | BACK |
| C-left | Talon Trot (Z + C-left) | T | D-pad left | TROT |
| C-right | Wonderwing (Z + C-right) | Y | D-pad right | WING |
| R | tight turn in flight; **port-only**: equip the next ability pad | U | R1 | POWER |

Port deviations (all because there are no BK worlds, pads or phone buttons for chords): C-up/down/left/right work
without Z held. A trot or Wonderwing started without Z is latched on (touch has no way to hold two buttons while
steering); press Z or the same C button again to leave it. Ability pads (Shock Spring, Flight, Turbo Trainers,
Wading Boots) cannot be stood on in an SM64 course, so R cycles the equipped one instead (`Power`).

## Status

Done = implemented with the decomp's numbers and covered by a test. Partial = works but a part is borrowed from SM64
or missing. Missing = not implemented.

| Move / system | Decomp | Status | Notes |
|---------------|--------|--------|-------|
| Idle, walk, run, creep, turn, skid | `stand.c walk.c turn.c` | Partial | SM64 ground physics with Banjo animations; BK's 30..500 u/s stick zones and the walk-cycle/velocity map are not used on the ground (only as the air-speed cap) |
| Crouch | `crouch.c` | Partial | SM64 crouch; BK's turn-in-place and the Z-held move chords are in |
| Jump (hold for height) | `jump.c` | Done | 710 u/s, gravity -2700, -1350 while A held (apex checked in a test) |
| Feathery Flap | `bFlap.c` | Done | once per air time, 2.5 s, impulse every 9 frames; needs no feathers in BK either |
| Flap Flip | `bFlip.c` | Done | crouch + A, 920 u/s, slow fall while A held |
| Rat-a-tat Rap | `bPeck.c` | Done | B in the air, 0.5 s of pecks with hops |
| Claw Swipe / Roll | `claw.c twirl.c` | Done | three hit windows; roll when walking fast |
| Beak Barge | `bBarge.c` | Done | crouch + B, 850/500 u/s |
| **Beak Buster** | `bbuster.c` | Done | Z in the air: 0.36 s hover, dive 2300 u/s with -20000 gravity, 3-frame impact freeze, rebound hop 730 u/s / -2110 (apex tested). Terminal speed capped at 150 units/frame (decomp 5000 u/s) so a 30 Hz step cannot tunnel through a floor. The decomp stores +2300 for the dive velocity; applied downward. Hits through SM64's ground-pound interaction (`ATTACK_GROUND_POUND_OR_TWIRL`); the 150-unit shockwave `func_8032728C` is just the ground-pound landing hitbox |
| **Talon Trot** | `bTrot.c` | Done | 30..700 u/s on the stick (1000 turbo), jump 693.5 / -1200 and landing back into the trot, ends when Z is released. Not ported: climbing steep slopes, the slow "trudge" on dangerous ground (300 u/s), slide state |
| **Shock Spring Jump** | `bShock.c` | Done | A charges 31 frames (anim 0.3637 of 2.8 s); released early cancels; else 1250 u/s, gravity -1200 (apex ~650 BK units, tested). Needs the Shock pad equipped (R) because there is no pad actor |
| **Blue egg, forward** | `bEggHead.c`, `code_CC1E0.c fxegg_head_*` | Done | 1.0 s state, egg leaves at anim 0.4704 (frame 14), 800 u/s straight, 2.0 s life, shatters on walls/floors, queues up to 3 presses; hits SM64 objects (damage / breakable / grabbable: `attack_object(ATTACK_PUNCH)`). Spawn: player +(0, 80, 0) + 70 forward |
| **Blue egg, backward** | `bEggAss.c fxegg_ass_*` | Done | dropped 18 behind at +60, 200 u/s back, ~710 u/s up, gravity -2200; walking over it picks it back up. Does not hurt enemies (neither does BK's) |
| Egg ammo | `ITEM_D_EGGS` | Done | start 50, cap 100 (BK: 100 / 200 upgrade), refill +5 per egg pile pickup; "NO EGGS" feedback |
| **Wonderwing** | `bWhirl.c` | Done | invincible; 80..425 u/s, jump 693.5 / -1200, one gold feather per 2.0 s, Z released or no feathers ends it. Invincibility is `invinc_timer`; enemies are not damaged by touching (matches BK) |
| Gold feathers | `ITEM_10_GOLD_FEATHER` | Done | start 10, cap 10, pickups |
| **Flight** | `bFly.c` | Partial | take-off (lift 1600 u/s), glide at 600 u/s, stick Y pitches (dive up to 80 deg trades speed for fall rate), stick X turns (R tighter), A spends a red feather for a 400 u/s flap. The exact `pitch_*`/`yaw_*` angular-velocity smoothing and the BA_FLAG_9 hover are approximated; no flight camera (the follow camera is used) |
| **Beak Bomb** | `bFly.c bsbfly_bomb_*` | Done | B in flight, costs a red feather, 0.7 s wind-up, 4200 u/s along the facing and pitch for 4000 units, attacking hitbox (`ACT_DIVE`), wall crash costs a honeycomb, floor crash lands, otherwise recovers into flight; 1 s cooldown after take-off |
| Red feathers | `ITEM_F_RED_FEATHER` | Done | start 50, cap 100, pickups |
| Turbo Talon Trainers | `bTrot.c` turbo | Partial | R equips for 30 s: trot 80..1000 u/s and Z need not stay held. The pickup/pad and its sound clock are not ported |
| Wading Boots | `bLongLeg.c` | Partial | only the low hop (400 u/s, -800) and the cycle slot; the stilt model, mud/lava immunity and 80..500 stilt walk are missing |
| Swimming / diving | `swim.c bSwim.c code_BEF20.c` | Partial | **eggs under water** (C-up fires a blue egg from the swim state, 12-frame cooldown, `banjo_ext::water_tick`; eggs keep flying and pickups stay collectable while submerged); SM64 swim physics with Banjo's swim/dive clips, plus the BK **air meter** (`ITEM_17_AIR`, `banjo_ext::water_tick`): 3600 units = 6 bubbles = 60 s, 2 units per 30 Hz frame under water, +220 per frame on the surface (refills in 17 frames), polluted water (Rusty Bucket Bay, Click Clock Wood winter) drains 4 under water and 2 on the surface and delays the refill 2 s, sandcastle infinite air flag, `CatchBreath` event on surfacing, HUD line `AIR [OOOOOO]`; at zero Banjo drowns (SM64 `ACT_DROWNING`, health below 0x100). SM64's health drain / regen in water is off for Banjo. Missing: Beak Dive (`bsbdive`), dive-B burst, underwater eggs, bubble/gasp sounds |
| **Climbing** | `climb.c bs/climb.c ch/climbBase.c` | Done | The BK climb poles (`ACTOR_26_CLIMB_BASE` base + `ACTOR_27/28` top node, 75 placed in 15 maps, `bkClimbs`): walking or falling into a pole's column (radius + 25 units, above the base, not above the top) grabs it when the Climb move is learned (`BANJO_CLIMB`); stick Y maps 0.03..1.0 to 100..300 u/s, A jumps off backwards, down at the base lets go, a 0.6 s regrab delay; exit poles (`ACTOR_28`) hop onto the top (60 u/s forward, 710 up, -1350, `bsjump_climbExit`). Port simplifications: Banjo hangs at the pole's radius on its axis (no orbiting), the climb idle / move clips are the SM64 pole clips' replacements `climbIdle` / `climb`, no climb sounds |
| Ledge grab | none in BK | n/a | BK has no ledge grab; SM64's is still active in the air step (kept so courses stay traversable) |
| Slopes / slide | `slide.c` | Partial | SM64 slide actions |
| Damage / knockback | `ow.c rebound.c die.c` | Partial | SM64 knockback and 8-wedge health shown as 8 honeycomb pieces (BK: 5 + upgrades); no fall-damage "splat" (`splat.c`) |
| Honeycomb pickup | `ITEM_14_HEALTH` | Done | heals; the HUD draws the pieces |
| Musical notes | `ITEM_C_NOTE` | Partial | HUD shows notes + SM64 coins; notes are real pickups in the 14 worlds; the lair's 12 note doors count them (see Locks) |
| Jiggies | `ITEM_E_JIGGY` | Partial | pickups + HUD count; the lair's world doors take them (jiggy puzzles, see Locks); no podium / cutscene (Banjo does the `jiggy` clip on SM64 star dance) |
| Pickups in any course | `ch/collectible.c` | Done (sandbox) | 12 pickups scattered around the spawn of each course; `BanjoExt::drop_pickup` places more |
| Mumbo's transformations | `croc.c walrus.c pumpkin.c ant.c bee*.c`, `ch/mumbo.c` | Partial | termite, crocodile, walrus, pumpkin and bee (`banjo_ext::Form`): the skull maps' Mumbo (`bkMumbo`: MM termite, BGS croc, FP walrus, MMM pumpkin, CCW bee) transforms Banjo when he walks up (400 units) and pays the first-time Mumbo token cost (5 / 10 / 15 / 20 / 25, `__transformation_getCost`), and changes him back on the next visit after leaving the spot. In a form Banjo has no BK moves: Mario's walk / jump under the form's top speed (500 / 375 / 400 / 500 / 375 u/s), the bee flutters up while A is held (120 frames per air time, simplified `beeFly.c`), the croc lunges with B (`bscroc_bite_init`: 500 u/s forward, up, jaws out as an SM64 jump kick, a weak blow), the walrus drops into a slide with Z while running (SM64 butt slide stands in for `walrus.c` sled), the pumpkin's walk speed pulses with its hop cycle. The five rigged models and their idle / walk / jump / ow / die / special clips are exported (`form_*.json`, drawn by `bk_enemies/forms.rs`). Not ported: Mumbo's actor and the pad itself, the transformation cutscene, the termite wall walk and the bee sting, the exact bite / sled / hop numbers and physics, the detransform timer and Mumbo's warnings, the lair crypt pumpkin. Test hook: `JUMPMAN_BANJO_FORM=bee` |
| Carry / throw, sled, surf | `carry.c throw.c sled.c surf.c` | Partial | SM64 carry/throw with Banjo clips; no sled/surf |
| Gruntilda's lair flight (BS_76 etc.), drones, cutscene locks | `droneenter.c`, ... | Missing | cutscene/lair-specific |
| First-person aim | none in BK64 | n/a | BK's first person is look-only; eggs fly along the facing (the game has no aimed eggs; aimed/fire/ice/grenade eggs are Banjo-Tooie) |
| Fire / ice / grenade eggs, Clockwork Kazooie, Breegull Blaster | Banjo-Tooie | n/a | not in BK64 |

## Progression and locks

Off by default: every course is a sandbox with all moves and no locks. `JUMPMAN_BK_PROGRESSION=1` starts a progression
game on the first BK map (`BanjoExt::begin_progression`): no moves learned, no eggs, no notes. State that follows Banjo
across map changes (`BanjoExt::carried`, in `RunCarry`): item counters, learned moves, solved puzzles and opened note doors.

| System | Decomp | Status | Notes |
|--------|--------|--------|-------|
| Ability progression | `core2/abilityprogress.c` | Done | `Ability` = `enum ability_e` bits; every move start checks `has()`: Barge, Beak Bomb, Beak Buster, Claw, Climb, Eggs, Feathery Flap, Flap Flip, Flight pad, hold-A high jump, Rat-a-tat, Roll, Shock Spring pad, Talon Trot, Turbo, Wonderwing, Wading Boots pad. Not gated: Camera Control and Dive (no use in the port), swimming |
| Bottles' molehills | `ch/mole.c`, `SM/ch/smbottles.c` | Partial | the 8 Spiral Mountain lessons (`ACTOR_12B`, ids 1..8: camera, dive, attack = Claw + Roll + Rat-a-tat, barge, jump = high jump + flap + flip, climb) and the world moles (`ACTOR_37A`, ids 9..0x12: Beak Bomb, Eggs, Beak Buster, Talon Trot, Shock Spring, Flight, Wonderwing, Wading Boots, Turbo Trainers, Note Doors; `bkMoles`). Standing within 260 units teaches the move at once (a `Learned` event / toast); no mole actor, dialogs, refreshers or camera. Pads still cycle with R, skipping unlearned ones |
| Mumbo tokens | `ch/mumbotoken.c`, `ITEM_1C_MUMBO_TOKEN` | Partial | `ACTOR_2D_MUMBO_TOKEN` setup actors (75) are pickups with a counter and HUD entry. No Mumbo's skull, transformations or token signs |
| Jiggy puzzle locks | `lair/jigsawpicture.c` `PICTURE_INFO` | Partial | the lair's world doors (a `bk_gl_*` warp into a world map) open when the world's picture can be completed: 1, 2, 5, 7, 8, 9, 10, 12, 15 jiggies for Mumbo's Mountain .. Click Clock Wood, taken from the stock (the podium is implicit). Not ported: the podium, the picture cutscenes, the DOG / double-health puzzles, door models |
| Note doors | `lair/lairspawnqueue.c` `chnotedoor_update` | Partial | the 12 `ACTOR_203_NOTE_DOOR` actors (`bkNoteDoors`, 7 in the lair maps) block a 660 x 700 slab (the model is not extracted) until the Note Doors lesson is learned and the notes reach 50, 180, 260, 350, 450, 640, 765, 810, 828, 846, 864, 882; walking within 290 units opens one. Drawn as a purple slab; the HUD shows `NOTE DOOR n: have/need` within 500 units. Notes are one global counter (BK counts per world) |
| Switches and their doors | `GV/ch/sunswitch`, `ACTOR_13F/140`, `ACTOR_1E2/16C/16A/168` + `1E3/16D/16B/169` | Partial | Gobi's sun switch -> sun door and the Click Clock Wood season switches -> season doors (`bkSwitches`, `bkSwitchDoors`; `banjo_ext::tend_switches`): a ground pound or Beak Buster landing within 160 units presses a switch for good (carried across maps), its door is a note-door slab in a progression game. The door and switch models are not extracted. Not placed: Gobi's Kazooie doors / star / honeycomb switches, the RBB, MMM and BGS switches (they need the object scripts that react to them) |
| Breakable doors and hatches | `core2/code_4C020.c` breakable markers; `ACTOR_109/10A/10B/10E/13E/21C/21D/265/2E7/CB/191` | Partial | MMM's shack and mansion doors and cellar barrel tops, RBB's engine room and cabin closet doors, TTC's Blubber hatch and lighthouse door, the CCW whipcrack room doors (20 actors, `bkBreakDoors`): a blue egg shattering next to one or a Beak Buster landing breaks it for good (`banjo_ext::tend_break_doors`, carried across maps); closed ones block a slab in a progression game. No door models, no debris. The RBB / MMM doors that open through scripts (church door, dining-room switches, water-level switches, propeller switches) are not ported |

## Enemies and music (not part of the player controller)

Clucker, Grille Chompa and red Flibbit were added to the BK enemies on 2026-11-09 (`sim/src/bk_enemy.rs` `plant` / `flibbit`; tests
`src/bk_enemies/tests.rs`): the two biters hurt only while their bite is out and die to a strong blow (`bk_enemy::blow_of`: dives, ground pound and the like;
punch / kick class hits only make them sound) then, Flibbit hurts on touch and dies to a strong blow.
Each BK course plays its map's theme through `src/bk_music/` (a software libultra sequencer over the extracted sound font); see
`DECOMP_TALLY.md`, Audio.

## Animations

Base clips are in `public/models/bk/banjo.json`. The new moves use `public/models/bk/banjo-extra.json`
(`python3 scripts/export-bk-banjo.py --extras <rom>`; merged by `characters/assets.rs::load_banjo`): `eggHead`,
`eggAss`, `shockCharge`, `shockJump`, `wonderEnter/Idle/Walk/Jump`, `flyEnter`, `fly`, `bombStart`, `bombCrash`,
`bombEnd`, `trotEnter`, `trotTurbo`. Kazooie's visible parts per move come from the decomp's
`modelAppendages_setKazooies*Visibility` calls. `trot`/`trotIdle` were already exported but unused.

## Test hooks

`JUMPMAN_CHARACTER=banjo`, `JUMPMAN_BANJO_SCRIPT="2:z+left@3,2:stick@3,6:a"` (time:buttons@hold; buttons `a b z up down
left right r stick`) drives the C buttons headlessly; combine with `JUMPMAN_SCREENSHOT=out.png JUMPMAN_EXIT_AFTER=<s>`.
