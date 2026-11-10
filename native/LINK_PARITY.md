# Link movement parity with Ocarina of Time (SM64 worlds and OoT scenes)

Audit of how Link moves in the SM64 courses against `reference/oot/src/overlays/actors/ovl_player_actor/z_player.c`
(plus `z_player_lib.c` for the boot tables and `z_actor.c` for `Player_GetHeight`). Code: `sim/src/link/physics.rs` (numbers),
`sim/src/characters.rs` `step_link` / `oot_air` / `oot_slide` / `oot_stun` (the controller), tests in `sim/tests/link_parity.rs`
(synthetic worlds) and `src/link/parity_tests.rs` (real courses).

## Unit conversion

One controller serves every world. OoT frames are 20 Hz, ticks 30 Hz: 1 frame = 1.5 ticks. The OoT scenes are exported x4.4
(`WORLD_SCALE`) and the Link rig is drawn at 0.044 = 4.4 x the actor scale, so in an SM64 course (SM64 units, Mario 160 tall) Link is
about 300 tall (`Player_GetHeight` 68 x 4.4) and every OoT number is converted the same way as in an OoT scene:
speed x 4.4 / 1.5, acceleration x 4.4 / 1.5^2, length x 4.4 (`link/combat.rs` `speed / accel / len`). Consequence checked on the real
courses (`parity_tests::full_stick_is_the_oot_run_speed...`): bob, wf, ccm, castle_grounds, castle_inside and kokiri_forest all give
17.6 units per tick at full stick, 528 units in 30 ticks. Link crosses the same *body lengths* per second everywhere.
Mario-sized geometry is therefore small for Link (a 40 unit stair is a 0.13 body step); this is the faithful reading, not a bug.

## Table

Status: **fixed** (this pass, with a test), **adapted** (SM64 surface needing a defined OoT-style response), **missing** (not done), **unverified**.

| Behaviour | OoT truth (decomp) | Before | After | Status |
| --- | --- | --- | --- | --- |
| Run speed | `Player_CalcSpeedAndYawFromControlStick` (z_player.c:4053): `(( 1 - cos((mag-20)*450))^2 * 30 + 7) * 0.14`, stick clamped to 60, cap `R_RUN_SPEED_LIMIT` 6.0 (Kokiri; 3.0 Iron, 5.5 Hover, `sBootData`) | `x 0.56`, cap 32 (Mario's own run, 1.8 x too fast) | 17.6 per tick at full stick, 3.09 at stick 32 (curve exact) | fixed |
| Uphill slowdown | same function: `- 8 * clamp(sin(floorPitch), 0, 0.6)^2` | none | in `walk_target` (`uphill_sin`) | fixed |
| Accel / decel | `func_8083DF68`: `Math_AsymStepToF(speed, target, REG(19)/100 = 2.0, 1.5)` per frame | 4 up / 6 down per tick | 3.91 / 2.93 per tick (stops from a run in 6 ticks = 4 frames) | fixed |
| Turning while running | `func_8083DF68`: `Math_ScaledStepToS(yaw, target, REG(27) = 2000)` per frame | 0x1600 per tick (4.2 x faster) | 1333 per tick | fixed |
| Stick pushed back | `func_8083C484`: > 0x6000 from the heading: `Player_DecelerateToZero` (8/frame), then pivot | U-turn at speed | stops dead, restarts from 0 the other way | fixed |
| Standing turn | `Player_SetupTurnInPlace` `turnRate = 1200`; idle start snaps below ~165 degrees (`func_8083FC68`) | snap | snap (the slow turn-in-place above 165 degrees is not ported) | missing |
| Roll | `Player_Action_Roll`: speed target = 1.5 x walk target, at least 3.0, up 2.0 / down 1.5 per frame; animation frame 20 at 1.875 frames per game frame (16 ticks); invulnerable from frame 8 for 10 frames; bonk only at speed >= 7 (`Player_Action_Roll` "Must have a speed of 7") | `max(1.5 x target, 12)` = 48 per tick, 744 units, invulnerable 10 ticks, any wall stops | top 26.4 per tick, 417 units (decomp: ~410), invulnerable 15 ticks from tick 6, only a wall at >= 7 units/frame stops the roll | fixed |
| Backflip / side hops (Z-target + A + stick) | `func_8083BCD0`: backflip speed 6.0, vy 5.8; side hops 8.5, vy 3.5; Z-target gravity -1.2 | 27 / 38 and 27 / 28 per tick: apex 200 / 112, reach 540 / 405 | apex 53 / 18, reach 238 / 193 (OoT: 62 / 22, 255 / 193) | fixed |
| Z-target + A + forward | `Player_ActionHandler_10`: with a weapon the jump slash take-off (5.0, 5.0), without one a roll | speed 27, vy 30 (apex 128, reach 432) | 14.7 / 14.7 (apex 39, reach 169) | fixed (the no-weapon roll is not ported: Link always has the sword in this build) |
| Gravity, terminal speed | `REG(68) = -100` (-1.0 per frame), -1.2 when Z-targeting a hostile, `minVelocityY = -20` | Mario's -4 per tick, terminal -75 | -1.96 per tick^2 (-2.35 locked), terminal -58.7 | fixed |
| Air control | `func_8083DFE0`: speed clamped to the run cap, 0.05 up / 0.1 down per frame toward the stick, yaw 200 per frame, a reversed stick brakes 1.0 | none (ballistic) | the same, for plain falls and ledge jumps (the hops stay ballistic: `PLAYER_STATE2_19`) | fixed |
| Jump off a ledge | `func_8083A4A8` (z_player.c:5704): on `GROUND_LEAVE` with speed > 3.0, facing within 0x2000, no sword drawn, 20+ units above the floor: vy = 7.5 when speed > 5.9, else 1.25 + 0.2 x speed (`IREG(66..69)`) | any `LEFT_GROUND` with 12+ per tick: vy 38 (apex 200 at any speed) | vy 22 at the run cap (apex 113; decomp 124), a slow walk (speed 3.09 < 8.8) just steps off | fixed |
| Ledge threshold | `sYDistToFloor > 20` OoT units (88) | SM64's `LEFT_GROUND`: drops over 100 | unchanged (14 % apart, a coincidence of the 4.4 scale) | adapted |
| Landing, fall damage | `func_80843E64` / `Player_Action_8084411C`: fall distance from the take-off height: <= 80 short landing; 80..800 with the stick forward rolls out without damage; >= 400 damage 8 (half a heart) + 10 frames stagger, >= 800 damage 16 + 20 frames, 40 frames of intangibility | nothing: no fall damage, no roll-out | all of it (x 4.4; `Landing`, `ARG_STUN`; the hurt cue goes through `MarioEvent::Hurt`) | fixed (no stagger clip: he plays idle) |
| Slopes | `Player_HandleSlopes` / `Player_Action_SlideOnSlope` (z_player.c:7254, 14255): he slides **only** on `FLOOR_EFFECT_1` surfaces, at `(1 - ny) * 40` (max 10 units/frame), stick ignored; every other floor (normal.y > 0.5) is walked | walked up SM64 slides and ice slopes at full speed, never slid | SM64 steepness stands for the flag: very slippery floors above 15 degrees, slippery above 20, everything else above 45 slides (`steep_for_link`, `oot_slide`); a 55 degree floor cannot be climbed | adapted |
| Ledge grab (hang / climb) | `Player_Action_8084411C`: while falling against a wall, `yDistToLedge` up to 150 OoT units | SM64's `AIR_STEP_CHECK_LEDGE_GRAB` | unchanged; the lower OoT jumps reach fewer ledges than Mario-scale ones did | adapted (the 150 unit reach is not ported) |
| Walking into a wall | `unk_880`: speed cap `0.00008 x angle to the wall`, 0.1 when head-on | pushes at full speed | unchanged | missing |
| Stairs | OoT walks floor steps within its wall-check height | walked | walked (`he_walks_up_stairs...`: 8 steps of 25, no hop; the real castle stairs are slanted collision) | verified for steps under 30 |
| Step-up height, wall radius | `sAgeProperties` adult `wallCheckRadius` 18 (79 world units); the step height was not decoded in this pass | SM64's 30 unit wall-ignore height and radius 50 (Link is 300 tall but as thin as Mario) | unchanged: a 40 unit riser blocks Link as it blocks Mario | missing |
| Water | surface and dive (`Player_Action_8084DC48`), Zora Scale depth, swim speeds from the water actions | SM64 surface swim + `dive.rs` | unchanged | missing (swim speeds, Z-target swim) |
| Ice / slippery flat floors | `FLOOR_TYPE`-dependent friction and A restrictions (`sFloorType`) | full traction | unchanged | missing |
| Lava / death floors / void | burn knock-back and damage; `Player_HandleExitsAndVoids` | SM64 lava boost, death floors, `check_void` | unchanged | adapted |
| Moving platforms, doors, warps, paintings, stars, cannons, signs, NPCs | actor / `DynaPoly` interactions | shared Mario interaction code | unchanged; not re-checked for Link in this pass | unverified |
| Poles, trees, ladders | climb walls with `WALL_FLAG_1/3` | SM64 pole grab | unchanged | unverified |
| Shield, sidestep and backwalk speed while Z-targeting | `Player_Action_8084227C` (linear speed mode, 0.9 x target, 2.0 / 3.0 steps) | curved speed | curved speed (now the slower OoT numbers) | missing |
| Slide / stagger / hang animations for the new states | `down_slope_slip`, `landing` clips | n/a | `ACT_WALKING` / `ACT_IDLE` with `ARG_SLIDE` / `ARG_STUN` play the walk / idle clips | missing |

## Evidence

* `cd native && cargo test --release -p jumpman-sim --test link_parity` (15 tests) pins every "after" number above against the decomp
  stepped at 20 Hz (`oot_arc`), including the slide, the stairs, the fall damage and the take-offs. `JUMPMAN_LINK_LEGACY_MOVE=1`
  switches the old numbers back on (used for the before captures only).
* `cargo test --release -p jumpman --lib link::parity_tests` runs full-stick Link on six real courses (needs the imported courses).
* Contact sheets (`linkparity_<behaviour>_before.png` / `_after.png`, 480x300 frames labelled with the sim tick, side camera
  `JUMPMAN_SIDECAM`) were made headless with `JUMPMAN_OFFSCREEN` and `JUMPMAN_LINK_DEMO` scripts; they are not committed (public repo).

## Not verified

Never run on a device, never played by hand: feel was checked from numbers and headless frames only. The audio cues of the new landings
(`MarioEvent::Hurt`) were not listened to. SM64 geometry was authored for a Mario 160 tall; with an OoT-scaled Link (300 tall, 17.6 per tick)
open courses feel slower to cross than with Mario's 32 per tick and some stairs / steps are small. If that is judged wrong, the single knob is
`physics::run_cap` and `OOT_SCALE`, not the structure.

## Next pass (in order)

1. A hostile-lock walk, sidestep and backwalk with the OoT numbers (`Player_Action_8084227C`), then the standing turn-in-place.
2. Walking into a wall slows him (`unk_880`), and the roll's bonk knock-back and clip.
3. Real clips for the slide, the stagger and the landing roll (`link_pose.rs`, outside this pass).
4. Swim speeds and the Z-target swim; ice friction by `FLOOR_TYPE`.
5. Check doors, signs, poles, cannons and warps one by one as Link on bob, wf, ccm, castle_inside.
