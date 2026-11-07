# Native port audit

Method: every catalogue course launched headless in release (`JUMPMAN_COURSE=<id> JUMPMAN_SCREENSHOT=... JUMPMAN_EXIT_AFTER=7`, Mario skin, audio off), logs grepped for panic/warn/error (none), screenshots read for black screens, missing geometry/actors, scale, sky/fog/water, HUD. Then every act (`JUMPMAN_ACT`) and every `warpSpawns` entry (`JUMPMAN_ENTRY`) of the Mario courses (147 runs, all clean), castle_inside areas 2 and 3, the 8 experiments, scripted playthroughs (teleport to the star -> dance -> iris wipe -> castle -> save dialog -> save; castle door; painting -> act selector -> course), and a frame-time pass (`JUMPMAN_FPS=1 JUMPMAN_NOVSYNC=1`).

## Defects found and fixed
| Defect | Root cause | Fix |
|--------|-----------|-----|
| After ANY course switch (star exit, door, painting, death warp to another course) Mario, the whole HUD, wipes and menus disappeared | `progress::tag_new` tagged every entity spawned between `mark_entities` and `tag_course_entities` as `CourseScoped`; unordered Startup systems (HUD, Mario view, camera, overlays, dialog UI, shell, audio, ...) landed in that window and were despawned by the first switch | `progress::PersistentSpawn` system set; all persistent Startup spawners are in it and `mark_entities` runs after it |
| Water surfaces vanished after a switch | spawned in Startup, tagged course-scoped, never rebuilt | `fx/water.rs` registers a `CourseRebuild` hook |
| Playground: star dance fires at the start (star sat on Mario's spawn and was ready immediately) | no `starLocations` entry, `requireCoins` not modelled | star at the TS position, gated on all meadow coins (`flow.rs`) |
| HUD title stayed on the previous course | the HUD swaps the curly apostrophe, the switch compared the raw name | compare normalised titles |

Test hooks added: `JUMPMAN_DEBUG_LOG=n` also logs `FLOW a -> b` transitions and the flow in the Mario line, `JUMPMAN_NOVSYNC=1`, `JUMPMAN_MSAA=off|2`.

## Course x status
Legend: OK = loads, no log warnings, screenshot plausible (matches the TS build where compared). Acts/entries = all listed acts and warp entries load.

| Course | Start | Acts/entries | Notes |
|--------|-------|--------------|-------|
| bob | OK | OK (6 acts, 1 entry) | |
| castle_grounds | OK | OK (8 entries) | castle door warp scripted: OK |
| castle_inside | OK | OK, areas 2/3 OK | painting -> act selector -> bob scripted: OK |
| wf, bbh, ccm, jrb, ddd, hmc, lll, ssl, wdw, sl, ttm, thi, ttc, rr | OK | OK (acts 1-6) | ddd/sa start in water (TREAD WATER) by design; rr start on a slope (butt slide, camera close) is the same in the web build |
| sa | OK | n/a | |
| bitdw, bitfs, bits | OK | OK (entries 0A/0C) | |
| bowser_1/2/3 | OK | OK | Bowser + mines render |
| pss | OK | OK | star -> dance -> castle -> save: OK (save written to `save_slot1.json`) |
| totwc | OK | OK | flying-warp start is intended |
| mushroom2d | OK | n/a | opens Dimensions (2D side) |
| playground | fixed (see above) | n/a | |
| links_house, kokiri_shop, midos_house, sarias_house, twins_house, know_it_all_house | OK | n/a | fixed-camera photograph rooms |
| kokiri_forest, deku_tree, graveyard, lost_woods, sacred_forest_meadow, hyrule_field, kakariko_village, lon_lon_ranch, zoras_river/domain/fountain, lake_hylia, gerudo_valley, gerudos_fortress, haunted_wasteland, desert_colossus, hyrule_castle, death_mountain_trail/crater, goron_city | OK | n/a | death_mountain_trail's pale floor is snow (same in web) |
| Experiments dimensions, god, runner, hundred, diorama, race, shrink, planet | OK | n/a | dock/card render |

Characters: Mario verified on every course; Link/Young Link/Banjo are covered by the `characters`/`zelda` unit tests and were not screenshotted per course.

## Performance
Release, 2080x1360 window, uncapped, shared machine (other jobs were using the GPU, so numbers are noisy): 11-19 ms/frame; the heaviest are the large SM64 outdoor levels (bob, castle_grounds, wf, bbh ~18.6 ms) versus 11-13 ms for small courses. Turning shadows off did not change bob; MSAA off gave ~14 ms in one noisy run, so cost is dominated by fill/MSAA at native resolution rather than CPU. No CPU hot spot was found; the 240p/480p render-target scaling in `PORTING.md` would be the lever if more headroom is wanted.
