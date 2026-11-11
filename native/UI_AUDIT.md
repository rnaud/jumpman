# UI audit: screens in the v3 look

"v3" = the N64-era front end of `shell/retro` (navy framed panels, gold chosen row, red BACK / green ENTER, chunky 5x7 pixel font).
Before this pass only the title, hub room, pause menu and its pages (options, controls, how to play, save file) were v3; every
other screen was built from `shell/theme.rs` (glossy blue bubble panels, rounded display font) or from ad-hoc `bevy_ui` styling.

## How it was fixed

* `shell/retro/font.rs`: the title's bitmap font as a generated TrueType face (no asset, nothing ROM-derived). `theme::load_ui_assets`
  uses it for `UiAssets::display/body` **and installs it as Bevy's default font**, so `Text` that names no font gets it too
  (this also changes the in-game HUD text, which is the other agent's area).
* `shell/theme.rs`: palette, `button`, `panel`, `tray`, `kicker`, `label`, `outlined`, focus cursor (the gold chevron), checker backdrop
  re-skinned to the `retro::skin` colours: square slabs with a 1 px bevel and dark outline ring, navy gradient dialog frames, gold chosen
  row, hard pixel text shadows. Every screen built from these (mode menus, cards, banners, deck/halls/gate/party/relic/swarm/lss UIs,
  dock) inherits it. Layout, sizes and `nav.rs` focus logic are unchanged.
* `retro/pages.rs`: new `courses` page: the LEVELS row of the title opened the classic bevy_ui hub (with a wrong "PAUSED" header);
  it is now a v3 list page (START, chosen course, missions, series headings, Banjo sub-area switch).
* `byor.rs` (UI only), `link/ui.rs` (item screen), `zelda/ui.rs` (item card), `objects/dialog.rs` (NPC box), `progress/ui.rs`
  (act select, course result, bonus prompt, game over), `progress/cutscene_glue.rs` + `credits.rs` (dialogue box, staff text: now
  pixel `Text` instead of the ROM-derived SM64 font): v3 frames and font.
* `lib.rs`: an off-screen run (`JUMPMAN_OFFSCREEN=WxH`) now sizes its window like the image (`JUMPMAN_WINDOW` still overrides).
  Before, the window stayed 1040x680, so layout was computed for a different size than the picture and captures were cropped or
  mis-scaled. Phone captures also need `JUMPMAN_TOUCH=1` (the phone layouts key on it). Default screens: `JUMPMAN_SCREEN=title`
  (v3 title), never `JUMPMAN_MENU=classic`.

## Inventory (landscape captures `ui_before_/ui_after_<name>_land.png` in the scratchpad)

Legend: before / after. v3 = already or now in the v3 look. "themed" = built from `theme.rs`, so v3 after the re-skin.

| Screen | File | Before | After |
|---|---|---|---|
| Title / file select (hub list) | shell/retro/mod.rs | v3 | v3 |
| Hub room (GAMES / LAB) | shell/retro/scene.rs | v3 | v3 |
| Character select (PLAY AS row) | retro/model.rs (title row) | v3 | v3 |
| Level select (LEVELS) | shell/screens.rs -> retro/pages.rs | classic hub | v3 page |
| Banjo sub-area list | retro/pages.rs | classic | v3 |
| Pause, options, controls, how to play, save file | retro/pages.rs | v3 | v3 |
| Game over (shell) | shell/screens.rs | classic | themed v3 (not a skin page) |
| Classic title/hub (`JUMPMAN_MENU=classic` only) | shell/screens.rs | classic | themed v3 look (dev fallback, not default) |
| Gem Clash menu, how to play card, summary | experiments/brawl_view.rs | themed | v3 |
| Kart menu, results, podium, pause | experiments/kart_hud.rs (HUD agent file) | themed | v3 via theme (file untouched) |
| Halls prompt / map / dead / win | halls_ui.rs | themed | v3 |
| Star Deck map, battle, shop, rest, event, reward, won/lost | deck_ui.rs | themed | v3 |
| Relic, Last Star, Swarm, Gate, Party menus | relic_ui, lss_view, swarm_view, gate_view, party/ui | themed | v3 |
| Race, Runner, God, Diorama, Dimensions, Shrink, Planet, Hundred | experiments/*.rs | themed / HUD only | v3 via theme |
| Mode dock (pause entry list, info card) | experiments/dock.rs | themed | v3 |
| Link item screen | link/ui.rs | ad-hoc | v3 |
| Zelda "you got" card | zelda/ui.rs | ad-hoc | v3 |
| Zelda textbox / shops | zelda/ui.rs DialogueArt | ROM-derived art (user's own import) | unchanged, does not match |
| NPC dialogue box (SM64) | objects/dialog.rs | ad-hoc | v3 frame |
| Act select, course result, bonus prompt, game over | progress/ui.rs | ad-hoc, ROM font | v3 |
| Cutscene dialogue, end credits text | progress/cutscene_glue.rs, credits.rs | ROM font | pixel font |
| Castle cake "Thank you" image | progress/credits.rs | ROM image | unchanged |
| ROM setup / import / re-import / "Leaving the game" | byor.rs | plain text | v3 frame, pixel font, v3 bars |
| Toasts, tooltips, control hints, HUD chips | hud.rs, hints.rs ... | other agent | not touched here |
| Loading screens, error/panic screens | n/a | do not exist (kart only toasts "Loading...") | n/a |
| Android startup error text | android.rs | plain | unchanged |

## Still not matching

* Zelda textbox and cake image stay ROM-derived runtime art.
* Screens only reviewed at landscape 874x402 / portrait 402x874 / desktop 1280x720 for the main set; see the report for the list.
* Menus are layout-compatible with the old sizes, but the pixel font is wider than the old rounded one: some long lines wrap.
* The Gem Clash menu panel is a fixed 560-600 px wide and still overflows a 402 px wide portrait phone (it did before too).
* End-credit staff text and cutscene dialogue boxes were converted but their captures did not show text at the sampled time: unverified.
* Kart menu/results (kart_hud.rs) and Relic/Swarm in-game chips are HUD-agent files: they only changed through the theme and font.
* Never run on a device (iOS/Android code paths in byor.rs were edited but not compiled for those targets).
