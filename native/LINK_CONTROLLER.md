# Link (Ocarina of Time) controller

Adult Link and Young Link play on the SM64 engine (`jumpman-sim`, 30 Hz) with a faithful OoT move and
item layer on top. Where the decomp gives a number it is used and cited (`z_player.c`, `z_en_arrow.c`,
`z_en_boom.c`, `z_arms_hook.c`, `z_en_bom.c`, `z_en_bom_chu.c`, `z_magic_fire.c`, `z_camera_data.inc.c`,
`z_kaleido_scope.c`, `z_inventory.c`, `audio/game/general.c`). Unit conversion: OoT runs at 20 Hz, Jumpman
ticks at 30 Hz (1 OoT frame = 1.5 ticks) and the Link rig is drawn 4.4 x the actor scale
(`link/combat.rs` `speed()/accel()/len()/ticks()`).

## Code map

| Piece | File |
| --- | --- |
| Inventory, age tables, ammo caps, bottles, equipment | `sim/src/link/items.rs` |
| Damage types (the 32 `DamageTable` rows), swing table, spin charge | `sim/src/link/combat.rs` |
| Arrow / seed / nut / boomerang / hookshot / bomb / bombchu / Din's Fire physics | `sim/src/link/projectiles.rs` |
| Ocarina notes, 12 songs, recorder | `sim/src/link/ocarina.rs` |
| The state machine (swings, combos, spin, jump slash, aim, items, hookshot pull, ocarina, boots, hearts) | `sim/src/link/moves.rs` |
| Plugin, C buttons, lock-on toggle, hit application, event handling | `src/link/mod.rs` |
| Projectile / spell drawing | `src/link/view.rs` |
| HUD, touch C buttons, item screen, ocarina staff, night/rain overlays | `src/link/ui.rs` |
| Z-target camera + Navi | `src/link/zcam.rs` |
| Animation choice (jump arc, landing, pose hints) | `src/characters/link_pose.rs` |
| Epona: `z_en_horse.c` state machine (gaits, carrots, jumps, braking, summon) | `sim/src/link/epona.rs` |
| Link on Epona: mount / ride / dismount, the `uma_*` pose hints | `sim/src/link/epona_rider.rs` |
| Epona model (CPU-skinned), blink, Link on the saddle | `src/link/epona_view.rs`, `scripts/export-oot-epona.py` (`public/models/oot/epona.json`) |
| Carrot HUD, MOUNT / CARROT / DISMOUNT button labels | `src/link/epona_hud.rs` |
| Horse chase camera | `src/link/zcam.rs` (`ZMode::Horse`) |
| Extra animation clips | `scripts/export-oot-link.py` `EXTRA_CLIPS` (regenerates `public/models/oot/*.json`; the BYOR pipeline runs the same script) |

The legacy controller (`characters::step_link`: walk, roll, guard, backflip/hops, ledge hang) is still what runs
plain movement, and its golden tests (`link_free`, `link_focus`) are untouched: the faithful layer is inert
unless `MarioRuntime::link.enabled` (the app sets it for Link / Young Link, never in tests of the legacy path).

## Controls

| Action | Keyboard | Gamepad | Touch |
| --- | --- | --- | --- |
| Move | WASD | left stick | virtual stick |
| A: roll (moving) / jump forward (Z-target + stick forward) / backflip + side hops (Z-target) | Space | South | A |
| B: sword (stick forward = stab, sideways = side slash, none = horizontal / forward slash); chain 3 for the finisher; hold B after a slash to charge the spin attack, release to spin; B in the air = jump slash (flip slash after the Z-targeted jump) | Shift / J | East | B |
| Shield (guard) | Ctrl / K | West, North, LT2, RT2 | Z |
| Z-target (toggle by default; hold mode in the item screen SETTINGS) | L | LB | L |
| C-left / C-down / C-right | 1 2 3 or U I O | D-pad left / down / right | on-screen `C<` `Cv` `C>` above the A/B grid |
| C-up = ocarina | 7 / Y | right stick click | on-screen `C^` |
| Item screen | Tab / N | D-pad up | ITEMS button |
| Epona: mount next to her / carrot while she moves / dismount when she stands (A relabelled MOUNT, CARROT xN, DISMOUNT) | Space (A); Shift / J (B) also dismounts | South; East | A; B |
| Epona: steer | WASD (relative to her heading, not the camera) | left stick | virtual stick |
| Epona: call | ocarina song (`^ < > ^ < >`), item screen EPONA, SONGS tab (Epona's Song) | same | same |
| Bow, slingshot, hookshot | hold the C key (aim with the stick, a Z-target locks the aim on), release to fire, A cancels | same | hold the C button |
| Ocarina notes | A, then C-down, C-right, C-left, C-up (`v > < ^`) | same | A button + the four C buttons; B puts it away |

Item screen: arrows / D-pad move, Enter / South selects (auto-assigns to a free C button), `1 2 3` or West / North / RT2
assign to C-left / down / right, Q / E (LB / RB) change tab, Tab / East close. Touch: tap cells, then tap the
`C <  C v  C >` assign buttons.

## Status

Legend: **done** = implemented and covered by a test; **partial** = works with the stated gap; **missing**.

### Movement and combat

| Feature | Status | Notes |
| --- | --- | --- |
| Run / walk, curved stick speed | done (legacy) | `step_link` |
| Z-targeting (candidate choice, strafe, backflip, side hops) | done (legacy) | `camera/link_focus.rs`; lock is now a toggle with a hold option |
| Lock-on toggle / hold | done | `link::lock_toggle`, setting persisted to `link.json` |
| Z-target camera (Battle / Parallel placement, fov 50 / 45, shoulder swing, ease in/out, wall pull-in) | partial | `zcam.rs` pure placement tested; `Camera_Battle1`'s exact yaw-swing interpolation is approximated; KEEP1 (friendly target) uses the Parallel numbers |
| Navi follows Link and flies to the locked target | partial | glowing orb, no model / dialogue / hints |
| Shield / guard | done (legacy) | blocks frontal hits (`take_damage`); Deku / Hylian / Mirror have no behavioural difference (no fire / reflect) |
| Roll | done (legacy) | invincibility frame 6, 10 ticks |
| Sword swings: forward, right, left, stab by stick direction | done | hit windows from `D_80854190` |
| 3-hit combo finisher | done | |
| Charged spin attack (hold B, release; big spin at charge >= 0.85 or a stick circle) | done | 0.02 per OoT frame; magic cost of the big spin not modelled |
| Jump slash (B in the air) and flip slash (Z-jump then B) | done | landing shockwave not modelled |
| Weapons: Kokiri / Master / Biggoron (reach), Deku Stick, Megaton Hammer | done | stick does not burn or break (event exists, never raised); hammer smashes breakables, not floor switches |
| Damage per weapon (`Dmg` table of a typical enemy) | partial | applied to goombas (any damage kills) and the SM64 object arena through `attack_object`; no per-enemy hit points exist in the SM64 objects |
| Ledge hang / climb | done (legacy) | SM64 ledge grab |
| Ladders, vines, crawl spaces | partial | pole climb and crawl exist from SM64; no OoT ladder |
| Swimming / diving | partial | SM64 swim; Zora Tunic keeps the meter full, no Zora dive |
| Iron Boots | done | walk on the bottom (metal-cap water step); no sinking in non-swim states |
| Hover Boots | done | 2 s hover after leaving the ground while A is held |
| Goron Tunic | missing | no fire / lava immunity yet |
| Hearts (16 units), damage, death | done | Mario's wedges become hearts (1 per wedge); at 0 the SM64 death plays |
| Epona | partial | adult Link rides her (`z_en_horse.c` ported, see below); child Link cannot, as in the game |

### Items

| Item | Status | Notes |
| --- | --- | --- |
| Inventory, C-slot assignment, age rules, ammo caps, wallet, upgrades | done | sandbox loadout by default (everything owned); `Inventory::starter` models the real start |
| Item screen (items, equipment, songs, settings) | done | keyboard, gamepad, touch |
| HUD: hearts, magic, rupees, keys, C boxes with ammo | done | |
| Fairy Slingshot | done | aim, lock-on assist, 80 u/frame, 15 frames |
| Fairy Bow | done | 150 u/frame, 12 frames, gravity in the last 7.2 |
| Fire / Ice / Light Arrows | partial | magic cost (4 / 4 / 8), element events; ignite lights nothing yet, ice does not freeze enemies / water |
| Boomerang | done | 20-frame out leg, steering missing, fetches coins |
| Hookshot / Longshot | partial | 13 / 26 frames, pulls Link; latches onto any wall (SM64 surfaces have no OoT material) and on `Hook` targets, none are placed in the courses |
| Bombs | done | 70-frame fuse, bounce, chain reaction, hurts Link, breaks boxes / bomb walls; no lifting / throwing |
| Bombchu | partial | runs along the floor, blows on contact; does not climb walls |
| Deku Nut | partial | flash event and effect, stun not applied to enemies |
| Deku Stick | partial | see above |
| Megaton Hammer | done | |
| Din's Fire | done | 20 + 30 + 25 + 15 frame sphere, scorches once per target, 12 magic |
| Farore's Wind | partial | sets a point, second cast returns to it (no menu, same course only) |
| Nayru's Love | done | 60 s invulnerability shield |
| Lens of Truth | partial | toggles and drains magic; nothing is hidden in the courses to reveal |
| Magic Beans | partial | plants (event + toast), no beanstalk platform |
| Bottles | done | red / green / blue potion, milk, fairy, poe; fish / bug / blue fire release with no world effect |
| Masks, trade quest, Keaton, etc. | missing | |
| Ocarina (12 songs, staff UI, tones) | done | recorder tested for every song; notes synthesised |
| Song effects | partial | five warp songs go to the matching exported course; Sun's Song = night tint; Storms = rain + lightning; Saria = +1 heart; Time, Lullaby, Prelude (Temple of Time not exported) only toast; **Epona's Song summons her** (see Epona) |

### Epona

Unit conversion as for the rest of Link (20 Hz to 30 Hz ticks, x4.4): every per-frame quantity of `z_en_horse.c` is scaled by 2/3 per tick in
`sim/src/link/epona.rs` and speeds by `4.4 / 1.5` (gallop 8 = 23.5 units per tick).

| Feature | Status | Notes |
| --- | --- | --- |
| Model, textures, blink, nine animations (idle, whinny, refuse = skid, rearing, walk, trot, gallop, low / high jump) | done | `object_horse` skin limbs exported with their per-vertex limb weights (`Skin_ApplyLimbModifications`), CPU-skinned like Link; the 3 eye textures are swapped as in `EnHorse_OverrideLimbDraw`. Checked on desktop screenshots (idle, gallop, side view) |
| Gaits and steering (`EnHorse_UpdateSpeed`: base 3 / 6 / 8, stick-driven 0.5 per frame ramp, traction-limited turn 400 / 0x190, brake 0.3 on a pulled-back stick, idle 0.06) | done | `epona.rs` tests; the stick is relative to her heading like the original's `curStick` |
| Carrots (6, `boostSpeed` 14 reached over 16 frames, +1 after 60 frames then every 11, all six back after 140) | done | HUD pips, A button label |
| Jumping: low (12) / high (13) jump arcs from the real clips' root height, triggers from `EnHorse_UpdateBgCheckInfo` (obstacle 19-40 at a gallop, 19-72 / up to 112 when boosted), take-off only when the obstacle is about one jump away | done | the original does not delay the take-off; see Not verified |
| Gap jump (galloping at a missing floor with ground 150-330 units ahead) | partial | **extension**, the generalised Gerudo Valley bridge jump; the original makes her stop |
| Stopping and obstruction (`EnHorse_Stopping`, `CheckFloors`: water, no floor, slope over 35 degrees, a drop of 40+ ahead: restore the pose, rear up at a gallop, reverse) | done | |
| Mount / dismount (A next to her flank within 75 units; A when she stands, B too); climb clips `uma_*_up` / `_down` | partial | the facing-the-horse condition is dropped and the flank test widened (anyone near her side can mount); the dismount spot is a fixed 60 units to the side |
| Link on the saddle, `uma_*` riding clips synced to her animation (whip variants while a carrot is active) | partial | the origin hangs 66 below the saddle point (the original's `riderPos.y - 27` left him floating with this model, checked on screenshots) |
| Epona's Song, EPONA item, SONGS tab: she appears 380-580 units away in front of the camera and gallops / trots / walks to Link (`EnHorse_SetFollowAnimation`, `FollowPlayer`) | done | works in every course; needs open ground (a spawn spot with a flat line to Link) else a toast says so |
| 'Epona follows' setting (SETTINGS tab, default off) | done | she stands beside Link at the start of each course |
| Horse sounds (`NA_SE_EV_HORSE_*`: walk, run, neigh, groan, jump, land, land2, slip, carrot chime) | partial | samples decoded from Soundfont 0 instruments by `export-character-voices.py` (pitch and length from the channel scripts, envelopes not applied); not listened to |
| Camera behind the horse | partial | wide chase placement (3.2 Link heights, 14 degrees) eased in and out; not the numbers of `CAM_SET_HORSE` |
| Items while mounted | missing | bow, slingshot, boomerang and the C buttons are disabled on horseback (the ocarina is too); the faithful aim state machine needs a rider mode |
| Epona vs other actors (pushing goombas, `colliderJntSph` hits), Lon Lon Ranch Ingo / race / HBA, cutscene horses, Hylian Loach-style spawns | missing | |

### Lock-on, jump and camera feedback

| Item | Status | Notes |
| --- | --- | --- |
| Lock-on toggle (default) with hold option | done | `LockOn` + `LinkSettings`; reachable from touch (L), gamepad (LB) and the item screen; the lock also drops when its target is gone |
| Jump look | partial | `normal_run_jump` (13 frames, one-shot) used to loop mid-air; it is now frame-synced to the arc and held, the long fall is `normal_fall_wait`, the Z-targeted forward jump uses `fighter_front_jump`, and a landing clip plays after touchdown. Checked on desktop screenshots as adult Link (rise, flip, landing) and Young Link idle; the ledge-walk-off and forward roll jump were not screenshotted |
| Z-target camera | partial | see the table above |
| Voices | done | swings (`kick` = SWORD_N, heavy = SWORD_L through `groundPoundDrop`), jump / backflip / hop / forward jump, roll (`dive`), ledge climb (`wallKick` = CLIMB_END), long fall, hurt and death come out of the same `MarioEvent` path; confirmed with `JUMPMAN_VOICE_LOG=1` as Link and Young Link (swing, roll, forward jump; the death cry `NA_SE_VO_LI_DOWN` was not extracted) |

### Tests

* `sim/src/link/*` unit tests (inventory, tables, projectile physics, ocarina, combat) and `sim/tests/link_controller.rs`
  (27 scenarios: swings, combo, stab, charged spin, jump / flip slash, bow, fire arrow, slingshot, boomerang, bombs,
  bombchu, Din's, Farore's, Nayru's, hearts / potion / death, ocarina song, hookshot pull, hover boots, stick, hammer,
  legacy path untouched). Run `cd native && cargo test --release -p jumpman-sim`.
* `src/link/ui.rs`, `zcam.rs`, `demo.rs` tests (touch layout, item screen model, camera placement, demo grammar).
* The golden tests `link_free` / `link_focus` (legacy controller) still pass.
* `JUMPMAN_LINK_DEMO` scripts the same actions in the real app for screenshots and the voice log.

Epona tests: `sim/tests/epona.rs` (13 scenarios on flat, fenced, gapped, watered and sloped worlds: speed tiers, carrots, stopping, jumps,
mount / ride / dismount through `MarioRuntime`, the song summoning her and her arrival), `src/link/epona_view_tests.rs` (model size and saddle point),
`src/link/epona_scenarios.rs` (a ride on Bob-omb Battlefield and Kokiri Forest).

### Not verified

Epona: how the gaits, jump timing and camera feel (the take-off delay and the saddle height are tuned by eye against screenshots), the horse sounds
by ear, the mount / dismount animations in motion (only seen as stills), the touch labels on a real device.

Real touch and gamepad input (driven through the same code paths by the scripted demo and unit tests only), iOS builds, the ocarina
tones by ear, the item screen on screen (its model is unit-tested, the layout was not screenshotted), and the doc-test target
of `jumpman_game` (it fails in the shared build directory when another worktree has overwritten the `jumpman-sim` rlib, not for a code reason).
