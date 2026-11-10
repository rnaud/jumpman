# Gamepad controls

Every mode, menu and screen can be played with a gamepad alone. Names are the Xbox layout (A = South, B = East,
X = West, Y = North; PlayStation: cross, circle, square, triangle). Courses, Link, Banjo and the experiments all read
the same pad through Bevy's `Gamepad`, whether it comes from gilrs, GameController.framework (macOS/iOS) or Android.

## Last-used input device

`native/src/last_input.rs` keeps one shared resource, `LastInput` (`KeyboardMouse`, `Touch` or `Gamepad`), updated every
frame from the latest key, mouse button / movement, touch or pad button / stick deflection. While it says `Gamepad`:

- the touch overlay (stick, A/B/Z/L, mode and Banjo / Relic extra buttons) is hidden, except the pause button;
- the experiment dock bar is hidden (open it with Select, below) and its labels lose their key hints;
- `hints::show()` is false, so every `ControlHint` node (key hint cards, "press X" lines) stays hidden;
- a few status lines name the pad button instead of a key.

A key, a mouse press or move, or a touch shows everything again, and so does unplugging the pad.
`JUMPMAN_INPUT=pad|keyboard|touch` pins the device (off-screen screenshots, tests). Gameplay HUD (scores, timers,
meters) is never affected.

## Everywhere

| Input | Action |
| --- | --- |
| D-pad / left stick | Move focus (menus) |
| A | Activate |
| B | Back (menus), resume (pause menu) |
| LB / RB | Previous / next tab (title, level select) |
| Start | Open / close the pause menu (OPTIONS, CONTROLS, HOW TO PLAY, SAVE FILE and LEAVE are ordinary focusable buttons) |
| CONTROLS page | A on a row, then press the new button (Start cancels); camera and restart rows included |
| ROM import screen | A extract, B back to the game; phones: X choose files, Y scan Documents |

## SM64 courses, Banjo, Link (no mode running)

| Input | Action |
| --- | --- |
| Left stick | Move / walk (partial deflection = walk) |
| A / B | Jump / punch, dive, grab |
| X, Y, LT, RT | Crouch / ground pound (Z) |
| Right stick | Orbit camera, look up |
| L3 / R3 | Cycle camera / snap behind (rebindable) |
| Select | Restart the run (courses only, rebindable) |
| LB | Focus / Link lock-on |
| Link | D-pad left / down / right = C-left / down / right, R3 = C-up, d-pad up = item menu (menu: d-pad, LB/RB tabs, A use, X / Y / RT assign to a C slot, B closes) |
| Banjo | D-pad = C buttons, RB = R |
| Zelda courses (Mario / Banjo) | RB (hold) aims the slingshot, d-pad up equips it |

## Experiment dock (every mode with a dock bar)

Dock bars carry the actions a keyboard reaches with keys and a finger with chips (Reset, Undo, Place, Next, Mix,
speed, track / class / racer, Daily / Random, lens, zoom ...). With a pad:

| Input | Action |
| --- | --- |
| Select | Open / close the dock cursor (the bar appears; the game ignores the pad meanwhile) |
| D-pad left / right, LB / RB | Move the cursor |
| A | Press the chip |
| B, Select, Start | Close |
| RB (cursor closed) | The mode's primary action (marked per mode below) |

## Modes

| Mode | Gamepad controls |
| --- | --- |
| Dimensions | Stick + A / B as Mario. Select dock: the four dimensions, Climb again (RB after the summit) |
| Hold the Gate | Map pick: d-pad / LB / RB + A, B leaves. Playing: X build / interact, Y call the next wave, d-pad up speed, d-pad down pause; in the build / tower card d-pad left / right + A (type, upgrade, sell, target mode, close), B closes. Result: A retry, X other map, B leave |
| God Mode | Stick + A / B. Select dock: tools 1-8, Place (RB), Undo, Redo, Reset, Gravity, Slow, Remove nearest (pad only: deletes the piece closest to where Place drops one). The Grab tool's dragging, resizing and the plank brush need a pointer; every tool can still be placed |
| Runner | Left stick steers, A jumps / dives; on the results A (or RB) runs again; Select dock: Daily / Random |
| Hundred Marios | RB next Mario; Select dock: counts, mix, speed, Watch, Personality, Start over |
| Diorama | Left stick orbits, right stick up / down zooms, RB shakes the case; Select dock: lens, reset view, turn, tilt, zoom, Drop coin, Start over |
| Race | Stick + A / B; RB races again (Select dock too) |
| Shrink | Stick + A / B; Select dock: sizes 1-4, Start over (RB) |
| Planet | Stick + A / B; RB new planet (Select dock too) |
| Star Swarm | Stick moves; level-up cards d-pad left / right + A; summary A retry, B leave |
| Mario Kart 64 | Stick steer, A hop / drift, X / Y / LT / RT brake, B item (on foot: get in / out, opens the kart menu); front end and result panels: stick / d-pad + A / B; Select dock: Get in / out, Reset, Track, Class, Racer, Menu, race start / leave / retry |
| Last Star Standing | Stick + A / B; d-pad left / up / right = item slots 1 / 2 / 3; result card A play again, X bot difficulty, Y spectate, B leave; spectating LB / RB switch player, B leaves |
| Relic Descent | Stick moves, B attacks (run + B = dodge, Z in the air = slam), RB attack (hold), d-pad left / right / down = skills 2 / 3 / 4, d-pad up potion, Select pack, L3 skill tree. Pack: d-pad walks the bag then worn gear, A equip / unequip, X sell, Y show / hide commons, LB / RB sell commons / up to uncommon, B close. Tree: d-pad walks the nodes, A spends a point, B closes. Result: A retry, B leave |
| Star Deck | D-pad left / right + A pick a card (A again plays; with several foes d-pad picks the target), Y end turn, B cancel; X deck, Select help, LT draw pile, RT discard pile, RB exhaust pile, LB first potion; map / reward / shop / rest / event screens and modals: d-pad walks the buttons, A presses, B closes |
| The Painted Halls | Stick + A / B as Mario; RB or Y cycle the cap, Select map (B closes it); start prompt A continue, X new game, B leave; win card A keep playing, B leave |
| Gem Clash (Brawl) | Stick + A jump, X attack, B grab, LT / RT ground pound, Y / RB / LB specials, Select HOW TO PLAY; setup: d-pad up / down choose a row, left / right change it, A start, B back to the hub; summary A next / rematch, B menu; extra pads join as players 2-4 |
| Party | Stick + A; tile games: d-pad moves the cursor, A picks; menus: d-pad + A, B back |

## Not verified

Everything above is covered by unit tests that inject synthetic `Gamepad` state (menu navigation, dock cursor, per-mode
cards, pickers, trees, the touch overlay and dock hiding) or by reading the input code. A physical controller, the iOS /
Android controller bridges and a real display were not used. R3 (snap camera) also reads as Link's C-up on his pad scheme.
