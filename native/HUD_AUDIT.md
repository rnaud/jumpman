# HUD audit (in-game heads-up displays, v3 look)

The v3 look is the N64-style menu: navy framed panels with a light inner edge and a dark outline, the chunky 5x7 bitmap
font with outline and drop shadow, gold highlights, red/green/blue slabs, pixel icons (see `docs/menu-v3/*.png`).
The shared kit is `native/src/shell/retro/hud_kit.rs` (`PixelText`, `Framed` plates, `Icon`s, `HudKit`, `bar`, `BarFill`);
one HUD pixel is `HudScale` layout px (2 on a phone, 3 from 600 px short side). All art is procedural (no ROM data).

Status: DONE = rebuilt on the kit, PARTLY = frame/bars restyled but text still in the shell theme font, KEPT = intentionally
unchanged (user-imported art or owned by the screens agent), TODO = not touched in this pass.

| Element | File | Before | After | Status |
|---|---|---|---|---|
| Mario power meter | `hud.rs` | 9 flat green squares, no frame | 9 ink-bordered cells in the navy plate, same colour/flash/throb logic (`segment_color`) | DONE |
| Mario lives / coins / stars | `hud.rs` | SM64 font textures (ROM-derived) "x 4" in a column | pixel icon (cap, coin, star) + `x N` in the HUD font, one navy plate with the meter; counter punch and life-loss shake kept | DONE |
| Course title (hint) | `hud.rs` | SM64 menu font, no frame | gold pixel text in a navy chip (hidden unless hints on) | DONE |
| Action / speed readout (hint) | `hud.rs` | Bevy default font, light blue | pixel text in a dark chip | DONE |
| Centred toast (COIN n, OUCH, mode toasts) | `hud.rs` | SM64 menu font, gold | gold 2x pixel text with outline, pop-in/shrink-out kept | DONE |
| Race badge (Koopa / slide / penguin) | `hud.rs` | green pill, default font | green slab chip with pixel text | DONE |
| Safe-area placement of the Mario HUD | `hud.rs` | fixed 16/12 px, ignored the notch | `inset_edge` pushes the panel, title, badge and action chip in by the safe area (test `hud_clears_the_safe_area`) | DONE |
| Debug panel (backquote) | `hud.rs` | green default-font text | unchanged (asked to leave alone) | KEPT |
| Touch stick | `touch.rs` | translucent grey disc, white rim | navy disc, light-blue rim, gold knob (still round: it is an analogue stick) | DONE |
| Touch A / B / Z / L | `touch.rs` | round translucent buttons, default font | slab plates (A red, B blue, Z/L navy), big pixel letters, held = gold plate; hit rects unchanged | DONE |
| Touch pause button | `touch.rs` | round grey, "II" default font | navy slab with gold pixel "II" | DONE |
| Mode buttons (kart item/L, gem clash SHOCK WAVE / SEEK BEAM, LSS slots) | `touch.rs` | round, tinted fill | slabs, tint mapped to red/green/blue/gold/navy plate (`plate_for_tint`), cooldown shade kept, pixel label fitted to the plate | DONE |
| Relic Descent buttons (skills, ATK, POT, BAG, TREE) | `touch.rs` | purple/green/orange/red circles | blue/green/red/navy slabs, cooldown shade kept | DONE |
| KART call button | `kart_anywhere/mod.rs` | round, default font | navy slab with pixel "KART", gold when driving | DONE |
| Link hearts | `link/ui.rs` | red rounded squares | pixel hearts (full / half / empty), bump and low-health throb kept | DONE |
| Link magic bar | `link/ui.rs` | green rounded bar | ink-framed green bar with sheen, width still follows the magic cap | DONE |
| Link rupees / keys / lens / nayru | `link/ui.rs` | green blob + default font | rupee icon + `x N`, gold `KEYS n` pixel text, rupee bump kept | DONE |
| Link C buttons + ITEMS | `link/ui.rs` | brown circles, default font | slab plates (navy; gold held; grey empty slot; blue ITEMS), pixel labels, item pictures (loaded at run time) unchanged | DONE |
| Ocarina staff line | `link/ui.rs` | default font | gold pixel text | DONE |
| Link item screen, Epona/aim HUD | `link/ui.rs` `link/epona_hud.rs` `link/aimcam.rs` | n/a | not part of this pass (item screen owned by the screens agent; Epona and aim readouts untouched) | TODO / KEPT |
| Banjo honeycombs | `banjo_ctl.rs` | 8 rounded amber blobs | pixel honeycomb icons (full / empty) in a navy chip, bump kept | DONE |
| Banjo notes / eggs / feathers / jiggies / tokens | `banjo_ctl.rs` | translucent pills with colour blobs | navy chips with pixel icons and `x N`, bump and token-chip rule kept | DONE |
| Banjo lives / stars / air lines | `banjo_ctl.rs` | default font | pixel text (multi-line) | DONE |
| Banjo touch buttons (POWER BACK WING TROT EGG) | `banjo_ctl.rs` | brown circles | navy slabs, pixel labels, gold when held | DONE |
| Banjo HUD hide rules | `banjo_ctl.rs` | `mode_hud_hidden` | unchanged | KEPT |
| Gem Clash fighter cards | `experiments/brawl_view.rs` | rounded navy pills, theme font | dark plate with the player-colour outline kept (hit flash/knock kept), pixel name and win pips | DONE |
| Gem Clash HP / ghost / power bars, gem pips | `experiments/brawl_view.rs` | rounded pills | square ink-framed bars with sheen | DONE |
| Gem Clash clock + round | `experiments/brawl_view.rs` | rounded pill, theme font | navy plate, gold pixel clock | DONE |
| Gem Clash banner / sub / warn / hint | `experiments/brawl_view.rs` | theme font, shadow | pixel text (banner 3-4x, punch-in and fade kept) | DONE |
| Gem Clash `?` help button | `experiments/brawl_view.rs` | round | navy slab, gold `?` | DONE |
| Gem Clash menu / how-to card / summary | `experiments/brawl_view.rs` | theme | not touched: screens agent | KEPT |
| Generic mode status line (Summit Race, Runner, Hundred, Dimensions, Shrink, Planet, Diorama, ...) | `experiments/mod.rs` | yellow default-font text floating at left:380 | dark plate centred above the dock, pixel text, wraps at 44 chars | DONE |
| Mode dock buttons (Next/Watch, Place/Undo, Run again, ...) | `experiments/dock.rs` | theme pills | shell theme (screens agent) | KEPT |
| Star Swarm HP / XP bars, level pill | `experiments/swarm_view.rs` | rounded pills | square ink-framed bars, square gold level slab; timer/kill/hint text still the theme font | PARTLY |
| Last Star Standing HP / shield / tag bars | `experiments/lss_view.rs` | rounded pills | square ink-framed bars; counters, map and slots unchanged | PARTLY |
| Kart HUD (lap, time, rank, item box, racer strip, minimap, lakitu, speed lines, results) | `experiments/kart_hud.rs`, `kart_minimap.rs` | the MK64 look from the user's imported art | unchanged on purpose: those are run-time loaded imported images and must stay as they are; only the touch buttons and the pause button around it moved to the v3 slabs | KEPT |
| Relic Descent HUD, Hold the Gate, Halls, Deck, Party in-mode HUDs | `relic_ui.rs`, `gate_view.rs`, `halls_ui.rs`, `deck_ui.rs`, `party/ui.rs` | rounded theme pills and theme-font text | frames and bars NOT converted (theme-based, rounded pills); after the merge with `ui-screens` their text is the bitmap font | TODO (PARTLY via the font) |
| Control hints (`hints` module) | `hints.rs` | marker system only | unchanged: it only hides/shows the chips above; the hint chips themselves (course title, action line, how-to text) are restyled where they are built | KEPT |
| FPS / perf panel | `perf.rs` | debug text | unchanged (debug) | KEPT |
| Item-get cards, dialogue boxes, Zelda cards | `zelda/ui.rs`, `zelda/visit.rs`, `objects/dialog.rs` | n/a | borderline: left to the screens agent | KEPT |

## Notes after merging `origin/ui-screens`

* The screens branch turns the 5x7 bitmap font into a TrueType face (`shell/retro/font.rs`), so the remaining theme-based
  `bevy_ui` `Text` (Relic Descent, Star Deck, Hold the Gate, Halls, Party in-mode HUDs, Star Swarm / Last Star text) now draws in
  the same letters as the kit's `PixelText`. Their frames and bars are still the old rounded pills, so they are PARTLY, not DONE.
* Wrapping: `PixelText` wraps to a character count, or to a fraction of the window width (`WrapFrac`) for the long hint / status lines.
* Scale: `HudScale` is chosen once at start-up from the short side of the window (2 below 600 px, 3 from 600). A window resized across
  that line keeps the first choice until restart.
* Touch plates keep the tested layout rects (48 / 44 layout px), so at scale 3 the single-letter labels drop to 1x and longer
  labels are shrunk to the plate (`PixelText::fit_width`). The smallest labels (SHOCK / WAVE, WHIRL, STARS) end up about 9-10
  layout px tall, the size the old 10-11 px default-font labels had.
* The kart HUD artwork (lap / time / rank glyphs, item box, racer strip, minimap, Lakitu) is the user's imported art, loaded at
  run time; it stays as it is.
