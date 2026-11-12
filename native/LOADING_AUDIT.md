# Loading audit: is a level loaded behind the menus?

Question from the player: "sometimes we load two levels at once, or a level is always loaded even on the start-game menus".

**Answer (measured, headless): yes to the second, no to the first.**

- *Before*: the app built the default course (`castle_grounds`: 927 collision surfaces, 52 sim objects, 11 course root entities,
  ~700 more entities of models and props, 290 meshes, 847 materials) at launch and kept it behind the title, hub and menus.
  The same happened after "leave to title" from any level (the level, 61 roots in Bob-omb Battlefield, stayed alive).
  The sim did **not** step (ticks stayed at 0: `SimSet` and the camera are gated on `AppState::Playing`) and the menus draw
  in their own private scene, so nothing was *visible* or *simulated*; it was memory, load time and entity count.
- *Two levels at once*: never two course roots in the world (the old views are despawned before the new ones are spawned), but
  during every switch **two `GameSim`s were alive** (the new one was built while the old one was still held: peak RSS 513 MB
  against 469 MB steady for title -> Bob-omb Battlefield), and several paths **loaded a course for nothing**: a mode started from
  the title (Gem Clash classic arena, Relic, ...) loaded `castle_grounds` at boot and then reloaded `castle_grounds` for the mode
  (2 loads, 1 needed); a repeat START while a load was queued was ignored only by accident, and a different second request made
  while the first was loading was stored but never consumed.

Measurements: `JUMPMAN_OFFSCREEN=800x450 JUMPMAN_RT=off JUMPMAN_NO_AUDIO=1 JUMPMAN_LOAD_LOG=1 JUMPMAN_STATE_LOG=1`, release build,
Apple M5 Pro, scripted with `JUMPMAN_SHELL_SCRIPT`. `JUMPMAN_LOAD_LOG=1` (new, `src/load_log.rs`) prints one `[load-log]` line per second and per course
load: state, screen, course id/act/area, `roots` (top-level `CourseScoped` entities), all entities, meshes / images / materials,
collision surfaces, sim objects, sim ticks (and the delta), frame ms, RSS and peak RSS, number of real course loads (`loads=`; the empty
menu course is printed as `LOAD course=void` but not counted). `JUMPMAN_LOAD_LOG=2` also prints a census of the top-level entities at 4 s.
RSS comes from `ps`, peak from `getrusage` (macOS). "before" = the commit before this work plus only the log; "after" = this branch.

## Results

| Scenario | Before | After |
|---|---|---|
| (a) cold start at the title, idle 10 s | course `castle_grounds` built (1 load), roots 11, entities 2491, meshes 290, images 136, materials 847, surfaces 927, objects 52, **RSS 490 MB**, ticks 0, frame 4.9 ms | **no course** (`void`: 2 surfaces, 0 objects, 0 props), roots 2 (persistent scene entities, not identified one by one), entities 1769, meshes 191, images 95, materials 749, **RSS 449 MB**, ticks 0, frame 5.4 ms, 0 course loads |
| (b) title -> hub -> back (only the character switch was scripted, hub navigation was not); (g) `SwitchCharacter` (link, banjo) | no load (loads stays 1), nothing changes | no load (loads stays 0), nothing changes |
| (c) title -> LEVELS -> Bob-omb -> START | loads 1 -> 2; **peak RSS 513 MB** (old + new `GameSim`); steady 469 MB; roots 11 -> 61 | loads 0 -> 1; **peak 465 MB**; steady 444 MB; roots 2 -> 61; old course released before the new one is built |
| (d) in level -> pause -> LEAVE to title | level stays loaded: `course=bob`, roots 61, entities 4357, surfaces 1216, RSS 477 MB | `course=void`, roots 2, entities 1770, surfaces 2, objects 0 (RSS 474 MB: the allocator keeps freed pages; meshes 285 -> 191, materials 862 -> 749 as assets are released) |
| (e) title -> Gem Clash (classic arena) | **2 loads** (`castle_grounds` at boot, `castle_grounds` again for the mode) | **1 load** (`castle_grounds`), peak 475 MB (was 497) |
| (e) title -> Relic | not measured before | 1 load (`castle_grounds`), peak 474 MB |
| (e) title -> Kart (needs `playground`) | boot `castle_grounds` + `playground` = 2 loads, peak 580 MB | `playground` only = 1 load, peak 576 MB (the Kart course itself dominates) |
| (f) leave a mode to the title, start a level | *inferred from the code and (d)/(e), not run before:* mode course stays alive behind the title; the level then loads with old + new alive | mode course unloaded (`void`), level loads alone: 1 load, `castle_grounds` -> `void` -> `ccm`, peaks 462 -> 486 MB (never two at once) |
| (h) door / warp / painting course switch in a level | `perform_switch`: new `GameSim` built while the old was alive | old released first, then the new one built; on a load error the old course is loaded back (test) |
| (i) paths with two `StartCourse` in a row | `experiments` reload after boot course (same id) | one request per start; a mode asks for its course exactly once |
| (j) 4 START taps in 0.5 s | 2 loads of Bob-omb (taps in the fade merge; a tap during the fade-in reloaded) | 1 load |

Not measured (see the end): audio streams / music (the headless run has no audio device; by code the music cues are sim
events and the sim never ticks in `Menu`; with `JUMPMAN_MUSIC_LOG=1` the title prints only `oot music: <stop> (course void)`), a real device.

## What was wrong and what changed

| Invariant | Change |
|---|---|
| No level behind the menus | New empty course `void` (`game/playground.rs::void`, `CourseRequest::void()`): two floor triangles, no props, objects or coins. A launch that starts at the title (`shell::detect_launch() == Menu`) builds `GameSim` on it (`lib.rs`), so every plugin that needs a `GameSim` finds one and runs on nothing. CLI courses, test hooks (`JUMPMAN_COURSE`, `JUMPMAN_EXIT_AFTER`, `JUMPMAN_SCREEN=pause`, ...) still start in their course. The first real course is built by `StartCourse` / `OpenExperiment` after the player chooses. Leaving to the title (`ReturnToMenu`, game over -> main menu) queues a swap back to `void` (`shell::unload_switch`). The BYOR first-run / re-import screens still have no `GameSim` at all (RSS 259 MB, `course=<none>`). |
| At most one course alive | `Progress::perform_switch` copies out what travels (lives, camera yaw, `as_link`/`as_banjo`, Link's pockets), **drops the old `GameSim`** (replaced by `void`), and only then builds the next one; if the load fails the old course is loaded back. `progress::despawn_course_entities` (shared by every rebuild) debug-asserts that no `CourseScoped` entity survives before the next course's views are spawned. |
| No redundant loads | `fresh_course(None, "void")` -> the default course (one load; a mode used to load it at boot and again for itself). `Transition::in_flight_is` drops a repeat of the course being faded to / loaded / faded in; a *different* request made during the load is kept and built after it (it used to be stored and never consumed). Character changes load nothing (verified). |
| Same level again is fast | Only the sim data is rebuilt (`LoadedCourse` loads in 0-4 ms here); the heavy part is the Bevy asset side, which stays cached by the asset server for the session. Nothing but the empty course is kept alive. |

Also in this branch (separate request): **"UPDATE AVAILABLE" at launch** (`byor.rs`): `launch_decision(setup_requested, assets_present, stamps, skipped, direct_start)`
is a pure function; a game imported earlier whose stamp is stale (older pipeline version or exporter hash, e.g. OoT `rust-oot-1` -> `rust-oot-2`)
opens the existing full-screen import screen with the heading `UPDATE AVAILABLE - re-import needed for OOT, BK`, the remembered ROMs listed (or the
"ROM files are gone, choose them again" message and the picker), `UPDATE` (Enter on desktop) and `SKIP, PLAY ANYWAY` (Esc on desktop). A skip is stored in
`<data>/update-skipped` keyed by the exporter set (`exporters_key()`), so it is not asked again until an app update changes a hash. Never-imported games do not
count; first run keeps the setup screen; launches that name a course/screen/hook (and a dev checkout with no stamps) never prompt;
`JUMPMAN_SKIP_UPDATE_PROMPT=1` forces that. When an import finishes the game starts at the title (empty course). No `GameSim` exists while the screen shows
(`course=<none>`, 259 MB). Capture: `scratchpad/load_update_screen.png`.
(Behaviour change: a stale `sm64` used to start its re-import by itself; now it waits for UPDATE like the others. `can_play()` no longer refuses stale-but-present data, since SKIP must start it.)

## Tests

- `game::playground` void course and `progress::flow::tests`: `unloading_to_the_menu_course_releases_the_level` (collision world freed, level start again works),
  `a_level_switch_holds_one_course_not_two`, `a_failed_load_keeps_the_old_course` (plus the existing `course_switch_keeps_link_and_banjo`).
- `progress::unload_tests`: no course entity (children included) survives the unload, resources are spared.
- `shell::tests`: `a_repeat_of_the_course_in_flight_is_not_started_twice`, `leaving_for_the_menu_swaps_the_level_for_the_empty_course_once`,
  `a_different_course_asked_during_the_load_is_built_next` (small Bevy app over `step_transition`).
- `experiments::fresh_start_tests::a_mode_started_from_the_title_loads_the_default_course_once`.
- `byor::tests`: `launch_decision_cases`, `a_skipped_update_is_remembered_per_exporter_set`, `the_update_screen_names_the_games_and_both_choices`.

## Not verified / limits

- Audio: no audio device in the headless runs, so streams and music tracks playing could not be counted; only the code path and the music log were checked.
- No device run (iOS / Android / Windows); the update prompt was captured on desktop only (macOS, fixture data dir with stale OoT and BK stamps), not on a phone layout.
- Real-region Gem Clash (needs the picker to move to another course), the in-game door / warp / painting switch and rapid taps on a real touch screen were not
  scripted; they go through the same `perform_switch` / `Transition` that the tests and the Bob-omb / Kart / Relic / Brawl runs exercised.
- A mode that needs no course (Gem Clash classic arena, Relic, ...) still loads `castle_grounds` for itself (as before, but once); running it on `void` would save more
  but needs each mode checked for a course dependency.
- RSS does not fall back after leaving a level (allocator keeps the pages); the asset / entity counts do. The title does not show less than the previous title in
  the picture (the menus never drew the level), only costs less.
- 25 data-dependent tests (audio clips, Banjo/Link rigs, MK64 music, full-build content) fail in this worktree for lack of extracted audio/model data; the rest (1476) pass.
