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

Gamepad names: Z = R1 (`RightTrigger`), L = L2 (`LeftTrigger2`), R = R2 (`RightTrigger2`); L1 is free in play (menus use L1 / R1 for tabs, L1 + R1 held half a second calls the free-driving kart). Migrated from the old LB / trigger defaults by `bindings.rs` (`controls.json` version 2).
| The state machine (swings, combos, spin, jump slash, aim, items, hookshot pull, ocarina, boots, hearts) | `sim/src/link/moves.rs` |
| Plugin, C buttons, lock-on toggle, hit application, event handling | `src/link/mod.rs` |
| World props Link's items act on (grass, pots, crates, rocks, boulders, hammer rocks, cracked walls, webs, torches, eye / crystal / floor / rusty switches; stick fire) | `sim/src/link/props.rs` (engine-free), `src/link/prop_world.rs` (Bevy glue: drops, hiding models, torch flames, Deku puzzle roles); placed from `props` in the course json by `scripts/export-oot-courses.py` |
| OoT sound effects (`NA_SE_IT_*`, `SY_*`, `PL_*`, `EV_*`, `EN_*`) | `scripts/export-oot-sfx.py` -> `assets/audio/voices/oot_sfx/*.wav` + `manifest-oot-sfx.json`; `src/audio/oot_sfx.rs` (names, per-enemy tables, tests) |
| Projectile / spell drawing (real item models, billboarded bomb) | `src/link/view.rs`, `src/link/items3d.rs` (models from `scripts/export-oot-items.py` -> `public/models/oot/items/*.json`) |
| First-person aim camera, reticle, aim input (right stick, arrows, mouse / touch drag) | `src/link/aimcam.rs`, `LinkExt::aim` / `aim_turn` in `sim/src/link/moves.rs` |
| Combat and item effects: hit sparks, bomb fireball / smoke / shockwave, blade trail, colour filters, hit-stop, shake, hurt flash / blink | `src/link/combat_fx.rs` (events from `WorldFx::queue`, `oot_enemy::Ev`) |
| Held items in the hand (bow, slingshot, hookshot, boomerang, hammer) | `scripts/export-oot-link.py` (`hold:*` batches), `characters::link_pose::batch_visible` |
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
| Shield (guard) | Ctrl / K | R1 (Z); also West, North | Z |
| Z-target (the N64 L button; toggle by default, hold mode in the item screen SETTINGS) | L | L2 | L |
| C-left / C-down / C-right | 1 2 3 or U I O | D-pad left / down / right | on-screen `C<` `Cv` `C>` above the A/B grid |
| C-up = ocarina | 7 / Y | right stick click | on-screen `C^` |
| R (the N64 R button: hold to aim the village slingshot) | H | R2 | - |
| Item screen | Tab / N | D-pad up | ITEMS button |
| Epona: mount next to her / carrot while she moves / dismount when she stands (A relabelled MOUNT, CARROT xN, DISMOUNT) | Space (A); Shift / J (B) also dismounts | South; East | A; B |
| Epona: steer | WASD (relative to her heading, not the camera) | left stick | virtual stick |
| Epona: call | ocarina song (`^ < > ^ < >`), item screen EPONA, SONGS tab (Epona's Song) | same | same |
| Bow, slingshot, hookshot: first-person aim (camera at the eye, fov 45, reticle) | hold the C key; aim with WASD, the arrow keys or a mouse drag; a Z-target locks the aim on; release to fire (or B), A cancels | hold the C button; aim with the left or the right stick (a Z-target locks on) | hold the C button; drag a finger anywhere that is not a button to aim, or slide the finger that holds the C button (the left virtual stick also aims) |
| Invert the vertical aim | `invert_aim_y` in `link.json` (stick, arrows and drag) | same | same |
| Ocarina notes | A, then C-down, C-right, C-left, C-up (`v > < ^`) | same | A button + the four C buttons; B puts it away |

Item screen: arrows / D-pad move, Enter / South selects (auto-assigns to a free C button), `1 2 3` or West / North / R2
assign to C-left / down / right, Q / E (L1 / R1) change tab, Tab / East close. Touch: tap cells, then tap the
`C <  C v  C >` assign buttons.

## Status

Legend: **done** = implemented and covered by a test; **partial** = works with the stated gap; **missing**.

### Movement and combat

| Feature | Status | Notes |
| --- | --- | --- |
| Run / walk, curved stick speed, turning, air physics, jumps, landings, slopes | done | OoT numbers in `sim/src/link/physics.rs` (run cap 6.0 units per frame, accel 2.0 / 1.5, turn 2000, gravity -1.0, ledge jump, hops, fall damage, slope slide); audit table and evidence in `LINK_PARITY.md` |
| Z-targeting (candidate choice, strafe, backflip, side hops) | done (legacy) | `camera/link_focus.rs`; lock is now a toggle with a hold option |
| Lock-on toggle / hold | done | `link::lock_toggle`, setting persisted to `link.json` |
| Z-target camera (Battle / Parallel placement, fov 50 / 45, shoulder swing, ease in/out, wall pull-in) | partial | `zcam.rs` pure placement tested; `Camera_Battle1`'s exact yaw-swing interpolation is approximated; KEEP1 (friendly target) uses the Parallel numbers |
| Navi follows Link and flies to the locked target | partial | glowing orb, no model / dialogue / hints |
| Shield / guard | done (legacy) | blocks frontal hits (`take_damage`); Deku / Hylian / Mirror have no behavioural difference (no fire / reflect) |
| Roll | done | 1.5 x walk speed for 16 ticks (~410 units), invulnerable from tick 6 for 15 ticks, bonk at 7+ units per frame |
| Sword swings: forward, right, left, stab by stick direction | done | `func_80837818`: stick read against Link's facing at 55/60 of full; forward = stab only while Z-targeting (else overhead slash), left / back = right slash, right = left slash; hit windows and clip frame counts from `D_80854190` / `link_animetion.xml`; after the clip the `_end` clip (Z-target `_endR`) plays at 30 fps and the sword stays out (`swordIdle` / `swordWalk` / `swordRun`). Biggoron's two-handed rows reuse the 1H clips |
| 3-hit combo finisher | done | the same slash three times (`unk_845`). OoT forgets the count 8 frames (400 ms) after a swing starts and only reads B on the swing's last frame or after; for a finger on a glass B the count lives `COMBO_KEEP_TICKS` (24 ticks, 800 ms) and any B press during a swing (also on its first ticks or its last tick) is kept for its end, so taps 150-600 ms apart give 1-2-3 (`sim/tests/link_combo_taps.rs`, `link::sword_tests` through the touch layer). The spin-charge window stays the original 8 frames. `JUMPMAN_LINK_OLD_CLIPS=1` drops the 26 clips added with the `_end` recoveries to check the data of a device that has not re-imported (the swing logic does not depend on them); `JUMPMAN_LINK_LOG=1` also prints `EnemyHurt` / `EnemyDied` |
| Charged spin attack (hold B, release; big spin at charge >= 0.85 or a stick circle) | done | 0.02 per OoT frame; magic cost of the big spin not modelled |
| Jump slash (B in the air) and flip slash (Z-jump then B) | done | blade live from frame 7 until landing, landing plays the `FINISH` hit clip and `_end`; OoT's own trigger (Z-target + A with a sword = immediate jump slash) is not ported |
| Weapons: Kokiri / Master / Biggoron (reach), Deku Stick, Megaton Hammer | done | stick does not burn or break (event exists, never raised); hammer smashes breakables, not floor switches |
| Damage per weapon (`Dmg` table of a typical enemy) | partial | applied to goombas (any damage kills) and the SM64 object arena through `attack_object`; no per-enemy hit points exist in the SM64 objects |
| Ledge hang / climb | done (legacy) | SM64 ledge grab |
| Ladders, vines, crawl spaces | partial | pole climb and crawl exist from SM64; no OoT ladder |
| Swimming / diving | partial | SM64 swim at the surface; Zora Tunic keeps the meter full; A at the surface dives (`link/dive.rs`, `Player_Action_8084DC48`: sinks 2 units a frame while A is held and the Zora Scale's depth (120 / 240 / 360) is not reached, the head comes back up over 15 frames, then rises at `depth * 0.018 + 4` (at most 8) in strokes that push only between frames 10 and 20 of the 29-frame swim animation, and hands back to the surface swim at 68 units (48 as a child)); the surface swim's own buoyancy and Z-target swim are still SM64's |
| Iron Boots | done | walk on the bottom (metal-cap water step); no sinking in non-swim states |
| Hover Boots | done | 2 s hover after leaving the ground while A is held |
| Goron Tunic | done | lava floors (`SURFACE_BURNING`) and King Dodongo's fire breath do nothing while it is worn (`MarioRuntime::fire_immune`); no heat meter (the Fire Temple is not exported) |
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
| First-person aim (bow, slingshot, hookshot) | done | `Camera_Subj3` numbers of the aim modes, reticle that closes and reddens over a target, ammo count, head hidden, `invert_aim_y`; tested in `sim/tests/link_controller.rs` and `aimcam.rs` |
| Item models | done | bomb (billboarded body + cap, `flashIntensity` replay), bombchu, boomerang, arrow, hookshot tip + chain, nut; the hand holds the bow / slingshot / hookshot / boomerang / hammer. Seeds, the Deku Stick and Din's / Nayru's spheres are still primitives; the stick model is exported but not drawn |
| Combat and item effects | done | see the effect list in the report of the pass: hit marks, clank sparks, death burst, bomb fuse sparks / smoke / fireball / ring / quake, blade trail, spin glow, jump-slash shock, roll dust, enemy flash / frozen / burning / stun, hit-stop, Link hurt flash and blink, pickup glints, Navi trail, nut flash. Audio now uses the extracted OoT clips (see the Sound table below); the old note: (no `NA_SE_IT_*` extraction) |
| Fire / Ice / Light Arrows | partial | magic cost (4 / 4 / 8), element events and flame / ice / light trails and hit effects; the `oot_enemy` rows ignite (burn timer) and freeze enemies; ignite lights no web or torch, ice does not freeze water |
| Boomerang | done | 20-frame out leg, steering missing, fetches coins |
| Hookshot / Longshot | done (surfaces), partial (targets) | 13 / 26 frames, pulls Link; real tip + chain models. **Latches only onto surfaces with `SurfaceType_CanHookshot`** (bit 17 of surface type word 1, parsed from the scene collision by `export-oot-courses.py` into `hookable` triangle indices, flag `SURFACE_FLAG_HOOKABLE`; e.g. Deku Tree 61, Kokiri Forest 110, Goron City 156 marked triangles); other surfaces reflect it (`IT_HOOKSHOT_REFLECT`). SM64 courses carry no marks and keep the old any-wall rule. The first-person reticle closes and turns green over a hookable surface. No `Hook` target actors exist in the exported courses; enemies take the hookshot row of their damage table (stun) but are not pulled |
| Bombs | done | 70-frame fuse, bounce, chain reaction, hurts Link, breaks boxes / bomb walls / boulders / cracked walls; **A next to a resting bomb lifts it overhead, A throws, B sets it down** (the fuse keeps burning, it can go off in the hands; lift animation is the `lightBomb` clip, the throw is `normal_throw`) |
| Bombchu | partial | runs along the floor, blows on contact; does not climb walls |
| Deku Nut | partial | model, white flash pulse; stuns `oot_enemy` enemies through the nut damage row (`react 1`); the SM64 objects are not stunned |
| Deku Stick | done (no model change) | catches fire at a lit torch or flame, burns for 10 s then is used up (`StickBroke`), and as a flame lights torches and burns webs (`props.rs`); the held stick has no lit model (flame is a prop effect only) |
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
| Items while mounted | partial | the bow from the saddle (`epona_rider.rs`, `func_80834380` / `func_80836AB8`): any equipped arrow flies as `ARROW_NORMAL_HORSE` for free, the aim is limited to 3500 up / down and 19114 to either side of the horse, the stick aims instead of steering; the other items and the ocarina are still disabled on horseback |
| Epona vs other actors (pushing goombas, `colliderJntSph` hits), Lon Lon Ranch Ingo / race / HBA, cutscene horses, Hylian Loach-style spawns | missing | |

### Lock-on, jump and camera feedback

| Item | Status | Notes |
| --- | --- | --- |
| Lock-on toggle (default) with hold option | done | `LockOn` + `LinkSettings`; reachable from touch (L), gamepad (L2) and the item screen; the lock also drops when its target is gone |
| Z-target on SM64 courses | done | `jumpman_sim::link::targeting` is the one classification shared by the lock (`camera/link_focus.rs`), the blade's reach (`object_manager`) and the projectiles: opponents include the enemies that only bounce or are hit from below (spindrift, scuttlebug, swoop, snufit) and the grabbable / pushing ones; hazards that merely hurt (flames, bowling balls, crushers, amps) are never targets; people (bob-omb buddies, Toad, penguins, signs) lock from 700 units and lose to any opponent. Z pressed again on a target moves to the next one, or releases when there is none. With nothing to target the toggled lock only lasts while Z is held (0.35 s at least, for the camera recentre) so Link is not left strafing with A turned into hops |
| Sword on SM64 objects | done | the reach extension covers every opponent and the jump / flip slashes; a blade hit raises `LinkEvent::HitObject` (same impact ring / sparks / hit-stop / sound as OoT enemies); Link is not thrown back by Mario's punch recoil and takes no contact damage from something only his blade touches; the spin attack hits all around |
| Jump look | partial | `normal_run_jump` (13 frames, one-shot) used to loop mid-air; it is now frame-synced to the arc and held, the long fall is `normal_fall_wait`, the Z-targeted forward jump uses `fighter_front_jump`, and a landing clip plays after touchdown. Checked on desktop screenshots as adult Link (rise, flip, landing) and Young Link idle; the ledge-walk-off and forward roll jump were not screenshotted |
| Z-target camera | partial | see the table above |
| Voices | done | swings (`kick` = SWORD_N, heavy = SWORD_L through `groundPoundDrop`), jump / backflip / hop / forward jump, roll (`dive`), ledge climb (`wallKick` = CLIMB_END), long fall, hurt and death come out of the same `MarioEvent` path; confirmed with `JUMPMAN_VOICE_LOG=1` as Link and Young Link (swing, roll, forward jump; the death cry `NA_SE_VO_LI_DOWN` was not extracted) |

### Link in Banjo worlds

Adult and Young Link play every `bk_*` map on the same engine. The BK maps are exported at 1.25 x the BK units (`bk_enemy::S`) and Link keeps his own numbers there:
he runs 17.4 units per tick (520 per second, about 3 body heights per second) where Banjo and Mario run 25.3, so a map takes half as long again to cross; A rolls (no
standing jump: a jump comes from the Z-target forward jump or from running off an edge) and the ledge hang climbs what Banjo jumps onto. Checked on real BK data by
sim tests (`src/bk_enemies/tests/link.rs`) and by headless captures through the touch layer at 874x402 (Mumbo's Mountain, Treasure Trove Cove, Spiral Mountain,
Bubblegloop Swamp).

| What | Status | Notes |
| --- | --- | --- |
| Spawn and walking, the 104 `bk_*` maps | works | Link stands on the floor of every spawn (a few maps start high above it, e.g. Clanker's belly, and he drops) and walks away on at least one of eight bearings wherever Banjo can. Two maps are closed rooms from their default spawn (`bk_mmm_inside_loggo`, `bk_ccw_spring_zubba_hive`) and trap Mario, Banjo and Link alike (they are entered through a warp). Blind walks off a cliff-edge start kill every character |
| Camera, Z-target camera, lock arrows, Navi | works | the lock takes a Banjo baddie like any opponent; arrows, Navi and the Battle camera show |
| Sword combo through the touch layer | works | 3 finger taps 150-450 ms apart, held 30-100 ms: three swings, the finisher on the third (Z-locked `Forward, Forward, Combo(Forward)`); each swing's blade window lands one `HitObject` on a baddie in reach. `JUMPMAN_LINK_LOG=1` also prints `BkEnemyHurt` / `BkEnemyDied` |
| Hit reactions per species | works, decomp rules | Grublin: dies to any blow; Ticker and Snippet: 2 hit points, one per sword blow; Bigbutt: slashes only make it reel (`OW`), a strong blow knocks it over; Grublin Hood: slashes make it flinch, a strong blow kills it; Limbo collapses; Nipper only takes strong blows in its open windows. **Strong blows** are the adult's spin attack and jump / flip slash (`Dmg::is_strong`: Master and Biggoron spin and jump rows, hammer, explosions): the interaction pass hands every blade hit over as a weak punch / kick, `bk_enemy_behavior` upgrades it with `LinkExt::blade_is_strong`. Young Link's Kokiri blows are all weak: he cannot fell a Hood or a Bigbutt with the sword |
| Z-target of Banjo baddies | fixed | a Ticker, or any baddie in a harmless state, is only a breakable body to the interaction pass (`INTERACT_BREAKABLE`) and was never a target. `targeting::bk_enemy_class` now classifies by species and state: everything up and about is an opponent; hives, Mumbo, podiums, snowballs, corpses, collapsed Limbos and buried biters are not |
| Stab, side and back slashes, spin (also behind him), jump and flip slash, B buffer in the knock-back | works | sim tests on a Ticker / Snippet / Hood / Bigbutt; a B pressed in the last 9 ticks of the knock-back a Grublin's bite causes swings when Link can act |
| Lunge and step | fixed | the finisher's and the stab's step carried Link into, or through, a baddie. Ground baddies are now solid to Link (`link_is_blocked_by`: the core of the hitbox, 55 % of its radius, pushes him out; the blade still reaches). A forward jump goes over a tall one |
| Damage taken | works | a baddie's touch costs Link hearts (`inv.health`, 1 per SM64 wedge) and knocks him back. The defeat heart (25 %) and the honeycomb heal one heart of **Link's** hearts (`bk_enemy::heal_a_heart`; `heal_counter` heals the SM64 meter, which Link does not use) |
| Defeat effects, loot, sounds | works | puff and coins from `bk_enemy::finish`; the BK hit and defeat sounds (`audio::bk_events::enemy_sounds`) play for any character in a BK world next to the OoT sword strike; not listened to |
| HUD | works | Link keeps hearts and rupees; the Banjo HUD is Banjo's |
| Notes, Jiggies, honeycombs | added | the BK pickups were only streamed in, drawn and collected for Banjo. In a BK world Link (and Mario) now see and take them: a note is a rupee on Link's counter, a Jiggy and a honeycomb toast and sparkle, the honeycomb heals a heart; the counters are the shared `banjo` ones. Eggs, feathers (Banjo's ammo) and Mumbo tokens are not placed for others |
| Transformation pads, Mumbo, moles, note doors, switches, poles, flight, the other Banjo moves | not for Link | Banjo-only systems (`step_banjo`, `tend_*`): Link walks over a Mumbo pad unchanged, cannot climb a BK pole or flap, and the lair's note doors do not stop him (they are Banjo's progression); warps work for everyone (`game/bk_warp_tests.rs`) |
| Swimming | as in SM64 worlds | surface swim and A to dive (`link/dive.rs`); the polluted / winter BK water only hurts Banjo |

Capture hooks added: `JUMPMAN_LINK_DEMO` `tp=x/y/z/yaw` (put Link somewhere at a tick) and `JUMPMAN_LINK_LOCKSTEP=1` (the sim waits until each `shot=` still has been
rendered; without it a busy machine runs several ticks inside one frame and the stills of the others are dropped), `JUMPMAN_LINK_LOG=2` also prints `BKNEAR` (baddies within
1000 units). The demo runs its steps in list order: sort them by tick. Baddies are awake from the first tick, also while the app is still loading, so place Link where
a chasing one will meet him.

Not verified: a real device (a real finger's cadence, performance), the sounds by ear, the BK pickups drawn for Link in a screenshot (a walk over notes in Mumbo's Mountain was
captured, the notes are too small to tell in the stills; the pick-up itself is covered by a sim test), tall BK maps by hand (the walk test is eight blind bearings per map); the Nipper,
Tanktup, Grabba, Sir Slush, Snarebear and Shrapnel fights were only run as far as hit detection (Sir Slush's hitbox, radius 190, throws Link back at a 150-unit
approach; the Shrapnel mines float in the water, where Link has no sword swing); Clucker and Grille Chompa hits (they pop up and bite on their own timer) have no Link scenario beyond the exploration run.

### Tests

* `sim/src/link/*` unit tests (inventory, tables, projectile physics, ocarina, combat) and `sim/tests/link_controller.rs`
  (27 scenarios: swings, combo, stab, charged spin, jump / flip slash, bow, fire arrow, slingshot, boomerang, bombs,
  bombchu, Din's, Farore's, Nayru's, hearts / potion / death, ocarina song, hookshot pull, hover boots, stick, hammer,
  legacy path untouched). Run `cd native && cargo test --release -p jumpman-sim`.
* `src/link/ui.rs`, `zcam.rs`, `demo.rs` tests (touch layout, item screen model, camera placement, demo grammar).
* The golden tests `link_free` / `link_focus` (legacy controller) still pass.
* `JUMPMAN_LINK_DEMO` scripts the same actions in the real app for screenshots and the voice log; `JUMPMAN_TICK_HZ=6` slows the sim so a `shot=` per tick lands on the intended tick (steps run in list order: sort them by tick).

Epona tests: `sim/tests/epona.rs` (13 scenarios on flat, fenced, gapped, watered and sloped worlds: speed tiers, carrots, stopping, jumps,
mount / ride / dismount through `MarioRuntime`, the song summoning her and her arrival), `src/link/epona_view_tests.rs` (model size and saddle point),
`src/link/epona_scenarios.rs` (a ride on Bob-omb Battlefield and Kokiri Forest).

### Not verified

Epona: how the gaits, jump timing and camera feel (the take-off delay and the saddle height are tuned by eye against screenshots), the horse sounds
by ear, the mount / dismount animations in motion (only seen as stills), the touch labels on a real device.

Real touch on a physical iPhone / iPad and a real gamepad (the touch layer was driven by real taps in the iOS *simulator* only, see "iOS simulator taps" below), iOS device builds, the ocarina
tones by ear, the item screen on screen (its model is unit-tested, the layout was not screenshotted), and the doc-test target
of `jumpman_game` (it fails in the shared build directory when another worktree has overwritten the `jumpman-sim` rlib, not for a code reason).


### Link in SM64 worlds: sword

Link (adult and Young) swings in the SM64 courses through Mario's own object interaction pass (`object_manager::interact_with_object`): the live blade
is `MARIO_PUNCHING` / `MARIO_KICKING` plus a reach (`Weapon::reach`), enemies see an ordinary punch (`attack_object`), so what an SM64 enemy does with it is
what its behavior does with `INT_STATUS_WAS_ATTACKED`. Tests: `src/link/mario_combo.rs` (real courses, human tap cadences; `cargo test -p jumpman link::mario_combo`).

| Piece | State |
| --- | --- |
| Three-hit combo on SM64 enemies | **done and tested**: taps 200-450 ms apart (held 60 / 100 ms, three phases) give Right, Right, Combo(Right) and, on a health-3 enemy, exactly three blows, one per swing (`LinkEvent::HitObject` each), then it dies: Big Boo (bbh), Pokey (ssl), Big Bully (lll), King Whomp (wf), both Links. King Whomp walks at Link, so the test looks for a standing distance from which the blade reaches him each time |
| One-hit enemies | Bob-omb (bob), Whomp (wf), Spindrift (ccm), Scuttlebug (bbh): swing 1 kills, swings 2 and 3 still play (finisher kind kept) and never strike the corpse |
| One blow per swing per object | `swing_hit` by object id (also checked with two enemies stacked: each takes one blow per swing); walking goombas (not sim objects) die on the first blow |
| Blow strength by blade | `LinkExt::blow_power` and `SimObject::take_blow`: Kokiri and Master 1 hit point (OoT 1 and 2 half-hearts, rounded up), Biggoron 2, Master jump slash 2, Biggoron jump 4; Pokey, Big Boo, Big Bully, Whomp and King Whomp read it, so Biggoron kills a health-3 enemy in two blows. Mario's punch stays 1 |
| Knock-back | Link's blows push Big Boo (30), Big Bully (50), King Whomp (20) and Pokey (30) away from him. Mario's blows keep the TS port's behaviour (parity tests compare it: Big Boo fed an s16 angle to `Math.sin`, Big Bully / King Whomp stepped towards Mario, Pokey stood still) |
| Impact feedback | the same `HitObject` mark, `IT_SWORD_STRIKE` and hit-stop as the OoT enemies. Walking goombas used to die silently: they now report a `HitObject` (id `EXTRA_TARGET_BASE + i`) |
| Spin attack | cuts all round for sim objects and now for walking goombas too (their cone check ignored `all_round`) |
| Z-lock rows | Z: Forward x2 + Combo(Forward); stick left / right: Right / Left slashes with their finishers; stick forward: Stab, Stab, Combo(Stab) (sim test on a Bob-omb; through the touch layer a stab was captured on a Big Boo, but Link's own lunge brought him into its contact knock-back before the 2nd and 3rd stab); stick back: the horizontal slash |
| Jump / flip slash | B after a Z + forward jump gives FlipSlash and defeats a Bob-omb ahead (tested, both Links); the plain jump slash (B in an A jump) is only in the never-crash sweep. Plain A with the stick is a roll, as in OoT, not a jump |
| Crates | the sword splits the SM64 small box (corkbox) and the big breakable box, with their coins; Mario's punch and kick still do not break the corkbox. A Bob-omb Buddy is not struck |
| Walls and ledges | 16 stick directions x Z / free, taps every 8 ticks on Bob-omb Battlefield, Whomp's Fortress, Cool Cool Mountain, Big Boo's Haunt and the castle grounds: Link never ends a tick inside a wall, below the floor or with a non-finite position (the lunge and slashes go through `step_ground`) |
| Never crash | every object kind found on 11 courses (bob wf ccm bbh jrb lll ssl hmc thi castle) swung at with slashes, Z-stab, spin and flip slash, both Links, one blow per swing per object |

Not covered or not changed:
* Only the enemies above were fought with checked outcomes. Chain Chomp, Mr Blizzard, Scuttlebug (beyond one blow), Klepto, Snufit, Swoop, King Bob-omb, Bowser, Eyerok, Wiggler and the rest were swung at in the never-crash test (no panic, Link on the floor, one blow per swing) but their reaction was not examined.
* No invulnerability window exists for these enemies, so nothing swallows a combo hit. The ones that walk at Link (Pokey, King Whomp, Big Bully, Boos) do knock him out of the combo when they touch him
  (soft knock-back, ~20 ticks); presses made during it are lost beyond the 4-tick B buffer, as in OoT's damage animation. Combat tests stand Link where the blade reaches first.
* The finisher's blade window opens 3 ticks after its swing starts, so an enemy that reaches Link before then costs the third blow.
* One blow takes whole hit points: the OoT per-sword damage differences below one point (Kokiri 1 vs Master 2 half-hearts) are rounded away; only Biggoron and the jump slashes hit harder.
* Touch: the stab and the side slashes read the stick at 58.7 of 64 (`STICK_DIR_MAG`, 92% of the pad radius), so a thumb has to push to the rim of the pad (dragging past the rim saturates at 64). The headless `stick=` of `JUMPMAN_TOUCH_SCRIPT`
  moved the finger only 40 px (a deflection of 47, never a stab); it now moves 80 px. Nothing was tried with a thumb on a device.
* Captures (874x402, real touch events, Big Boo's Haunt and Whomp's Fortress) are in the final report of this pass; nothing ran on a device or in the iOS simulator.

### Sounds, props and the Deku Tree (this pass)

**Sounds** (`scripts/export-oot-sfx.py`): a channel-script interpreter over `seq_0.prg.seq` plus the Soundfont 0 / 1 samples
(VADPCM and the 2-bit `CODEC_SMALL_ADPCM`), envelopes (`1 tick = 1/180 s`, approximate release), looped samples, mixed at 16 kHz and peak-normalised.
Decoded 223 of 230 requested ids (see `manifest-oot-sfx.json`, `clips` and `skipped`); not decoded: `EN_DEKU_SCRAPE`, `EN_GERUDOFT_DAMAGE / DEAD`,
`PL_BODY_BOUND / BODY_HIT` and a few more whose scripts use only random-choice / unsupported ops. Not modelled: vibrato, portamento, pan, reverb, per-channel
volume; nothing was listened to. Mapping: swings, shots, hits, explosions, hammer, hook, spells, pickups (`SY_GET_RUPY` / `HP_RECOVER`), Link hurt
(`PL_DAMAGE`), enemy hurt and death (`oot_sfx::enemy_hurt / enemy_dead` per kind + `EN_EXTINCT`, for every character), webs, torches, switches, pots,
grass. SM64 cues remain for Mario and Banjo blows.

**Props** (placed from the room actor lists): Deku Tree 26 grass, 2 floor + 5 wall webs, 11 torches, 3 floor + 1 eye switch, 1 cracked wall; Kokiri Forest 16 grass + 4 rocks;
Lon Lon 7 pots + 1 crate; Goron City 16 boulders, 5 hammer rocks, 29 large rocks, 8 torches, 9 pots, 5 cracked walls; Graveyard 24 torches; ... 348 props in the area-1 courses.
Rules (see `props.rs` header): webs and torches need fire (fire arrow, Din's Fire, burning stick), eye switches seeds / arrows / boomerang, crystal switches any weapon,
rusty switches the hammer, floor switches weight or hammer, boulders and cracked walls bombs. Cut grass and broken pots roll `Item_DropCollectibleRandom` rows.
Torch groups (`count`) need that many torches burning at once. Switch flags are stored (`Props::flags`) but only the Deku Tree puzzle parts act on them: the hand-built lobby web (`web`),
the lower web (`lowerWeb`) and the eye (`eye`) are bound to the nearest web / eye props, so fire burns them and seeds / arrows press the eye.
Not done: doors / shutters opening from flags or cleared rooms (9 `DOOR_SHUTTER`s are plain geometry), rise-blocks, the 5 other wall webs reveal nothing, wall webs have no
collision to walk through, `BG_YDAN_HASI`, bean plants, a flame trail from Link, pots / rocks cannot be lifted and thrown, crate / rock models of Hyrule Field are static decorations that
only vanish where a model of the right name stands within 60 units of the actor.

**Item polish**: aim-invert toggle in the SETTINGS tab (`INVERT AIM`); hook reticle; bomb lift / throw; `JUMPMAN_LINK_C="firearrow,dins,stick"` sets the C buttons for scripted runs.
Still primitives: seeds, Din's / Nayru's spheres, Farore's marker, bottle / fairy models, the lit Deku Stick. Bombchu wall climbing: not done.

## iOS simulator taps (real touches, `JUMPMAN_TOUCH_LOG=1`)

Link's touch controls were checked with real taps injected into the iOS 27.2 simulator (iPhone 18 Pro, 874x402 points, scale 3, safe area 62 / 62 / 20 in landscape;
the iPhone 17 could not be driven: the simulator tool had no access to it), not with injected `TouchInput` messages. Set `JUMPMAN_TOUCH_LOG=1` (`SIMCTL_CHILD_JUMPMAN_TOUCH_LOG=1`
with `simctl launch --console`) to print every raw touch (logical px, frame, clock), the window size / scale factor / safe area, and which layer claimed each finger
(`Stick`, `Button(..)`, camera drag, `Link C button`, `ITEMS button`, `item screen <action>`, pause). Findings and fixes of that pass:

* `experiments::relic_view::cleanup` ran every frame while Relic Descent was *not* running and reset `TouchControls::suppress` every frame, so Link's item screen never
  swallowed touches: taps on the screen also pressed L / Z / A / B / the stick under it and turned the camera (Assign C > sits on top of L). Fixed (only on the frame the mode ends).
* The item screen's buttons used Bevy's `Interaction`, which reads `Touches::first_pressed_position()`, an arbitrary one of the fingers on the glass; they are now hit-tested at
  each new touch's own position (`menu_touch_taps`, `pick_menu_action`); `Interaction` still serves the mouse.
* A finger resting on a C button now also aims (bow / slingshot / hookshot: hold, slide, release), not only a second finger on empty screen (`LinkInput::claim_drag`);
  `touch_buttons` is ordered after `touch::drive_touch` so the C finger never leaves a camera-drag owner behind.
