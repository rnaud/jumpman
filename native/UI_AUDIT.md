# UI audit: screens in the v3 look

"v3" = the N64-era front end of `shell/retro` (navy framed panels, gold chosen row, red BACK / green ENTER, chunky 5x7 pixel font).
Before this pass only the title, hub room, pause menu and its pages (options, controls, how to play, save file) were v3; every
other screen was built from `shell/theme.rs` (glossy blue bubble panels, rounded display font) or from ad-hoc `bevy_ui` styling.

## How it was fixed

* `shell/retro/font.rs`: the title's bitmap font as a generated TrueType face (no asset, nothing ROM-derived). `theme::load_ui_assets`
  uses it for `UiAssets::display/body`. It is **not** Bevy's default font any more (hud-revert): menu code names it explicitly
  (`UiAssets`, or `theme::pixel_font(size)` where no `UiAssets` is at hand), so `Text` that names no font, i.e. every in-game HUD
  readout, is back on the original default font.
* HUD revert: the in-game heads-up displays are back exactly as at `6646ebf2` (Mario/Link/Banjo HUDs, touch buttons, Gem Clash
  in-match HUD, lab status line, swarm/lss bars, KART button; `hud_kit.rs` and `HUD_AUDIT.md` deleted). Theme widgets used by
  heads-up elements are drawn by `UiAssets::hud()`, a second asset set with `classic: true` (pre-v3 rounded glossy pills, the old
  Lilita/Nunito/rounded face with Bevy's built-in font as fallback); menus use the v3 set. See "HUD decisions" below.
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
| Mode dock: in-play chip bar (Next/Watch, Place/Undo ...) | experiments/dock.rs `rebuild_dock` | themed | classic (HUD, unchanged by design) |
| Mode dock: experiment intro card, GAMES list | experiments/dock.rs `rebuild_card`, `rebuild_menu` | themed | v3 |
| Link item screen | link/ui.rs | ad-hoc | v3 |
| Zelda "you got" card | zelda/ui.rs | ad-hoc | unchanged (appears in play: reverted to the pre-pass look) |
| Zelda textbox / shops | zelda/ui.rs DialogueArt | ROM-derived art (user's own import) | unchanged, does not match |
| NPC dialogue box (SM64) | objects/dialog.rs | ad-hoc | unchanged (appears in play: reverted to the pre-pass look) |
| Act select, course result, bonus prompt, game over | progress/ui.rs | ad-hoc, ROM font | v3 |
| Cutscene dialogue, end credits text | progress/cutscene_glue.rs, credits.rs | ROM font | pixel font |
| Castle cake "Thank you" image | progress/credits.rs | ROM image | unchanged |
| ROM setup / import / re-import / "Leaving the game" | byor.rs | plain text | v3 frame, pixel font, v3 bars |
| Toasts, tooltips, control hints, HUD chips | hud.rs, hints.rs ... | HUD | unchanged by design |
| Loading screens, error/panic screens | n/a | do not exist (kart only toasts "Loading...") | n/a |
| Android startup error text | android.rs | plain | unchanged |

## HUD decisions (hud-revert; the user asked for menus only)

| Element | Decision |
|---|---|
| Mario HUD (meter, lives/coins/stars with the SM64 textures, course title, action line, race badge, toast), touch buttons (A/B/Z/L, pause, mode, KART, stick), Link HUD (hearts, magic bar, rupee/keys text, C buttons, ITEMS), Banjo HUD and buttons, Gem Clash in-match cards/bars/clock/banner/hint/`?`, lab status line, Swarm/LSS bars | unchanged by design: files restored to `6646ebf2` (later non-UI fixes kept, there were none in these files) |
| Link item SCREEN (`LinkMenu`) | menu: stays v3 |
| Gem Clash menu text sizes | menu: stays |
| Mode dock chip bar while playing | HUD: classic look (`rebuild_dock` uses `ui.hud()`) |
| Experiment intro card, GAMES list | pre-game menus: v3 |
| Gate, Halls, Relic, Swarm, Last Star `spawn_hud`, Gate banner, Gem Clash `spawn_hud` and `?` button | in-play overlays: classic look |
| Gate preview/menu/maps/summary, Halls map/prompt/dead/win, Relic pack/tree/summary, Swarm cards/summary, Last Star card, Gem Clash menu/help/summary | menus or interrupting screens: v3 |
| Star Deck (all screens incl. combat, toast/banner) and Party (board, minigame frames) | judged full-screen game screens built like menus: v3 (not a heads-up display over a 3D scene) |
| Kart HUD art, kart results card | HUD art is imported images (unchanged); results/podium card is a results screen: v3 |
| NPC dialogue box, Zelda "you got" card | appear during play: reverted to the pre-pass look |
| Credits / cutscene dialogue text, SM64 progress screens, byor setup | menus/cutscenes: v3 pixel font kept |
| Safe-area placement of the Mario/Link/Banjo HUDs (added by the HUD pass) | dropped with the restyle: the pre-pass fixed margins are back (it is in commit `718f3697` if wanted again) |
| `fb.rs` glyph/punctuation additions | kept (menus use them) |

## Still not matching

* Zelda textbox and cake image stay ROM-derived runtime art.
* Screens only reviewed at landscape 874x402 / portrait 402x874 / desktop 1280x720 for the main set; see the report for the list.
* Menus are layout-compatible with the old sizes, but the pixel font is wider than the old rounded one: some long lines wrap.
* The Gem Clash menu panel is a fixed 560-600 px wide and still overflows a 402 px wide portrait phone (it did before too).
* End-credit staff text and cutscene dialogue boxes were converted but their captures did not show text at the sampled time: unverified.
* The text shadow / inks of the classic widgets use the pre-v3 navy, but a few heads-up helpers that name `theme::NAVY` directly (Gem Clash `txt`, Deck) now get the darker v3 navy: a very small colour difference.
* Never run on a device (iOS/Android code paths in byor.rs were edited but not compiled for those targets).
