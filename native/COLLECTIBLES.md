# Currency collectibles and the collectible theme

The small currency pickups follow the CHARACTER, not the world (`sim/src/collectible_theme.rs`, `Character::theme()`):
Mario = coins, Link / Young Link = rupees, Banjo = notes. The world's object stays what it was (it still feeds the world's rules);
only what is drawn, heard and which counter is credited changes. Eggs, feathers, Jiggies, honeycombs are NOT part of this.

## Value tables

Worth (coin-equivalents) of what a world defines: SM64 yellow 1, red 2, blue 5; OoT rupee green 1 / blue 5 / red 20 / purple 50 / silver 100 / gold 200 = 1 / 5 / 2 / 10 / 20 / 40; BK note 1.

| world defines | Mario (coin counter) | Link (wallet) | Banjo (note chip) |
|---|---|---|---|
| SM64 yellow coin | coin 1 | green rupee, +1 | note, +1 |
| SM64 red coin | red coin, +2 | red rupee, +20 | note, +2 |
| SM64 blue coin | blue coin, +5 | blue rupee, +5 | note, +5 |
| OoT green / blue rupee | yellow / blue coin, +1 / +5 | the rupee, +1 / +5 | note, +1 / +5 |
| OoT red rupee (20) | red coin, +2 | +20 | note, +2 |
| OoT purple / silver / gold | blue coin, +10 / +20 / +40 | +50 / +100 / +200 | note, +10 / +20 / +40 |
| BK note | yellow coin, +1 | green rupee, +1 | note, +1 |

World rules are untouched: `GameSim::coins` always counts the SM64 value (100-coin star, course score), `red_coins` the red-coin star,
`BanjoExt::notes` every BK note (note doors). Link's wallet is capped by his wallet upgrade. Banjo's HUD sum is `notes + coins`.

## Inventory

| collectible | world | spawn | draw | collect | counter |
|---|---|---|---|---|---|
| yellow / red coins, coin formations, rings | SM64 | `game/runner.rs` `GameSim::new` (`course.coins`, `red_coins`, `coin_formations`) | `render/pickup_view.rs` (+ `render/currency_skin.rs` skin) | `GameSim::collect_pickups` -> `credit_coins` | `coins`, `red_coins`; Link `inv.rupees` |
| loot coins (goomba, bob-omb, boxes) | SM64 | `spawn_coin` (`ObjectEvent::Coin`) | `pickup_view.rs` | same | same |
| moving yellow / blue coins, moneybag coins, blue coin switch | SM64 | sim objects (`behaviors/moving_yellow_coin.rs`, `part_b/blue_coin.rs`) | `render/object_view.rs` (coin glb + skin) | `ObjectEvent::MovingCoinCollected` / `ValueCoinCollected` -> `add_coins` | same |
| Link kills a goomba / boomerang fetch | SM64 | `link/mod.rs` | - | `credit_coins` | same |
| rupee drops (grass, pots, enemies, placed collectibles) | OoT | `zelda_enemies` `Drop`s | `zelda_enemies::sync_drops` (`themed_drop_look`) | `zelda_enemies::collect` | Link wallet; others `coins` |
| village rupees, chests | OoT (kokiri_forest) | `zelda/scene.rs` | `zelda/view.rs` (real rupee glb for Link, `sync_rupee_skins` for the others) | `zelda/world.rs`, `visit.rs` through the lent counter (`zelda/mod.rs`) | wallet for Link, `coins` for others (one counter each) |
| notes | BK | `bk_enemies/pickups.rs` -> `BanjoExt::pickups` | `banjo_ctl::sync_pickups` (`themed_look_of`) | `banjo_ext::collect_pickups` (Banjo) / `collect_guest_pickups` (guests; `guest_coins` drained by `GameSim`) | `banjo.notes` always; Link wallet; Mario `coins` |

## Presentation

* Drawing: Mario keeps the real SM64 coin glbs everywhere coins exist. Rupees and notes have NO model in the imported data: they are the procedural shapes of `fx/pickup_fx.rs` (`Look::Rupee*`, `Look::Note`), and Mario's coins in OoT / BK worlds are the procedural coin disc (`Look::Coin*`), not the glb. The OoT village keeps its real rupee glbs for Link. All views read the live `Character`, so a switch re-skins pickups already out.
* Collect burst: Mario the SM64 three-spark burst; Link / Banjo the shaped-pickup pop + sparkles (`PickupFx::collect`); `fx/juice.rs` tints by theme.
* Sound (`audio::themed_collect_sound`): SM64 coin sound (Mario), OoT `SY_GET_RUPY` (Link), BK `collect.note` (Banjo).
* Text: collect toast `COIN n` / `RUPEE n` / `NOTE n` (`hud::collect_toast`), village counter line "n coins / notes". No HUD layout change.

Not themed: the shop dialog strings in the village still say "rupees"; the red-coin number sprites and red-coin sound stay (star hunt hints).

## Placement (anchor)

`fx/anchor.rs` decides where a themed skin sits (engine-free, unit tested). Every skin mesh (`pickup_fx::shape_tris`: rupee, note, disc) is centred on its origin with a half-height of one unit; the skin spins about the world's vertical axis through its centre (the billboard rotation of an SM64 coin view is counter-rotated in `render/currency_skin.rs`).

| world / item | what `pos` is | skin centre | size (half-height) | bob |
|---|---|---|---|---|
| SM64 coins (placed, loot, moving, bag, blue, rings) | bottom of the coin model (it is 64 tall) | `pos` + 32 = the real coin's centre | 26 (lowest point 6 above `pos`) | none |
| OoT village rupees | origin of the real rupee model (y 7..92 above it) | `pos` + 49.5 = its centre | 36 | none |
| OoT drops / placed collectibles | centre, 20 / 30 above the floor | `pos`, lifted so the lowest point is 6 above the floor | the look's own size (19-24) | up only, 4 |
| BK notes | centre; the floor under it is looked up at spawn | `pos`, lifted the same way | the look's own size | up only, 4 |

A skin therefore never dips below 6 sim units above the floor (tested at every spin angle and bob phase, and against a floor that rises 20 degrees around the item). Debug hooks for floor-level captures: `JUMPMAN_SIDECAM_PICKUP=1` (with `JUMPMAN_SIDECAM` and `JUMPMAN_COLLECT_DEMO`) frames the demo's pickup from the side, `JUMPMAN_DEMO_DROP=<s>` tosses three rupee drops next to the player in an OoT course.
