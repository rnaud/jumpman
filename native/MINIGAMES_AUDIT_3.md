# Minigames audit, round 3: the research checklists

Six modes measured against the per-mode checklists in `MINIGAMES_RESEARCH.md` (Gem Clash, Hold the Gate, Star Deck, Last
Star Standing, Endless Run, Kart), the worst failures fixed, and the fixed criteria pinned by tests where that was
practical. Everything was measured headless: bots on the deterministic cores, the cores' own tests, and the new report
tests listed below (`cargo test --release <name> -- --nocapture` prints the tables quoted here). Bots are not players:
where a number depends on how a person plays, it says so. Nothing here was played with a pad, on a phone or in the iOS
simulator.

New report tests (all run in the normal suite; most also assert the criteria they measure):

| Test | Mode | What it prints / pins |
| --- | --- | --- |
| `brawl_region_tests::gem_clash_pace_report` | Gem Clash | per region and overall: first contact, distance to the nearest foe, props at hand, gem swaps, power form length and outcome, round and match length |
| `brawl_core::tests::a_thrown_prop_is_worth_a_three_hit_combo` | Gem Clash | a thrown crate / rock / barrel does at least a punch-punch-kick |
| `lss_core::tests::last_star_standing_pace_report` | LSS | the player's first fight, longest stretch with no rival in view, elimination curve, airdrops, final-five spread |
| `lss::tests::the_players_blows_earn_hit_stop_and_a_camera_kick` | LSS | hit-stop / camera kick table for the player's fights |
| `gate_core::tests::hold_the_gate_pace_report` | Gate | builder bot with and without a fighting hero: wins, lives, leaks per wave, first foe, run length |
| `deck_tests::chains_escalate_and_landed_cards_hit_stop`, `star_deck_first_fight_pace_report` | Star Deck | chain feedback steps, card hit-stop; first fight turns and cards per turn |
| `runner::run_tests::a_retry_is_quick_and_the_first_obstacle_comes_soon` | Endless Run | death-to-running time, first obstacle after GO over 60 seeds |
| `kart_cpu_tests::kart_pack_report` | Kart | per track at 100cc: time within 3 s of another kart, item hits on the leader |

---

## Gem Clash (`brawl*`)

Measured on all 26 shipped real-course regions, Normal bots, 4 matches each 1v1 and four-way (about 220 and 280 rounds).
"Before" is the state after the ~46 % speed-up; the pace (760 u/s, 4-tick reversal) was not touched.

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | Every landed hit: hitstop + sound + particle + victim shake, scaled with damage; heavy hits kick the camera | Hit: core hitstop 1-6 ticks scaled by damage (both fighters), `bonk`/`stomp`, sparks, white flash, shake 0.08-0.45; heavy adds a ring, coloured chips and 2 ticks of view hit-stop. **No victim shake** | Same plus Sakurai's victim shake: the fighter who took the blow trembles side to side through the freeze (5 + 3 x freeze-ticks units) | Fixed (view; not pinned by a test) |
| 1b | Power-form attacks visibly bigger | Nova / seek beam have their own rings, flash, burst, shake; power fighters are 1.5x size | Unchanged | Pass |
| 2 | First contact within 3 s of GO; nobody more than ~2 s from a foe | 1v1 p50 2.5 s, p90 4.0 s; 4-way p50 1.7 s. Nearest foe p95 1.1 s, max 4.7 s | 1v1 p50 2.5 s, p90 4.5 s; 4-way p50 1.7 s. p95 1.1 s | Pass at the median (pinned p50 <= 3 s, p95 <= 2 s). Bots wander to gems first; a player rushing meets the foe in ~1.3 s. Courtyard (1v1 p50 5.6 s) and Ranch (3.9 s) are the slow regions |
| 3 | A throwable or weapon within ~1 s of every fighter in the first 30 s | 84 % of fighter-ticks (1v1), 83 % (4-way) | 85 % / 82 % | Pass overall (pinned >= 75 %). Bob-omb Summit and Courtyard sit at 51-60 % |
| 4 | Gem holder readable; gems knocked loose produce chases (gems change hands in most rounds) | Gems change hands in 81 % / 92 % of rounds | 79 % / 90 % | Pass (pinned >= 60 %). Readability at widest zoom not checked by a person |
| 5 | Power form lasts 8-15 s; wins its fight most of the time but ends the round outright in no more than about half | Median 6.2 s (1v1), 7.2 s (4-way): specials burned it (nova 2.5 s, beam 1.6 s). Owner won 79 % of rounds; 47 % / 38 % of knockouts by a powered fighter | Specials cost 1.6 s / 1.0 s, power blows 1.35x (was 1.5x). Median 7.9 s / 8.2 s; owner won 87 % / 74 %; 51 % / 45 % of knockouts | Fixed for length (pinned p50 >= 7 s, knockouts <= 55 %). 1v1 owner win rate is high; the window is a comeback but a strong one |
| 6 | Round 40-90 s median; best of three under 5 min | Round p50 28 s (p90 53); match p50 95 s, max 166 s (1v1) / 255 s (4-way) | Round p50 27-28 s; match p50 82 s / 112 s, max 175 s / 214 s | Match pass (pinned <= 300 s). **Rounds are short of 40 s** with bots that never stop fighting; not changed, to keep the pace the user asked for (a human round will be longer; check with a pad) |
| 7 | Every set piece telegraphs >= 1 s with a visual and a sound | Covered by `brawl_hazard_tests` (telegraph timings); a telegraph tick sound on every hazard start | Unchanged | Pass by code/tests; off-camera knockouts not measured |
| 8 | Camera keeps all fighters and the gem in frame on phone aspect | Previous round's camera work (all four fighters, Sand Pit view yaw) | Unchanged | Not re-measured |
| 9 | A thrown prop that connects is worth at least a 3-hit combo | Crate 11, barrel 16, rock 14 vs punch-punch-kick 24 (in engine, with combo bonus) | Crate 24, barrel 27, rock 25 | Fixed, pinned (`a_thrown_prop_is_worth_a_three_hit_combo`) |
| 10 | Gauntlet curve | `the_gauntlet_can_be_won_and_lost`, `gauntlet_stages_escalate...` | Unchanged | Not re-measured per stage |

Balance after the changes (existing tests): Hard vs Normal 47-13, Normal vs Easy 46-14, Hard vs Easy 54-6; novice vs two
bots Easy 83 % (was 83 %), Normal 35 % (42 %), Hard 15 % (19 %); novice with monsters Easy 90 %, Normal 44 %, Hard 17 %.
Stronger props help the bots too, so the novice drifted down a little on Normal; still inside the test's bands.

## Hold the Gate (`gate*`)

Builder bot (`bot_step`, calls waves early) with and without a stand-in fighter (runs at the ground foe nearest the gate,
punches every 12 ticks and pounds every 45 while in reach), 8 seeds per map.

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | First decision within 15 s, first foe within 30 s | 400 coins at once, building open from the first frame; first foe at **40 s** | First foe at 25 s (`FIRST_WAVE_SECS`); calling early still pays (`FIRST_WAVE_SECS / 3`) | Fixed (pinned <= 30 s) |
| 2 | Mario measurable but not mandatory (fighter keeps 3+ more lives, builder alone still wins) | Builder only: won 8/8 per map, 6.0 / 12.0 lives left. With fighter: 20.0 / 20.0 | Builder 5.6 / 11.9, fighter 20.0 / 20.0 | Pass (pinned). The stand-in fighter is perfect at chasing; it says Mario matters, not by how much for a person |
| 3 | A new thing every 1-2 waves in waves 1-10 | A new foe kind in every wave 1-10 (Goomba, Bob-omb, Tektite, Bully, King, Boo+Bat, Spiny+Wolfos, Koopa, Lakitu+Peahat, Chomp) | Unchanged | Pass |
| 4 | Early waves not free (a passive builder loses a life before wave 7 on the harder map, or a harder setting) | 0/8 runs lost a life before wave 7 on either map; leaks only in waves 10-11 | Tried bigger waves 4-5: no early leak, and the extra bounty made the late game *easier* (6.0 -> 9.0 lives); reverted. Still 0/8 | **Not fixed.** Needs a harder setting (a menu/HUD change, out of scope here) or faster early foes |
| 5 | Wave preview shows composition and path | `preview()` (kinds, counts, boss, air) on the HUD; tested | Unchanged | Pass (code/tests) |
| 6 | Every tower kind picked (pick rates) | The bot builds a fixed list, so its picks say nothing | - | Not measurable without a choosing policy |
| 7 | Overdrive visible and satisfying | Pound on a tower: shake 0.6, `star_sound`, burst | Unchanged | Pass (code) |
| 8 | Calling early pays | Bonus = timer / 3 s in coins; `calling_the_wave_early_pays_a_bonus_once` | Unchanged | Pass (value not measured against a no-call bot) |
| 9 | Boss waves are spikes | Shake 1.0 and `select_mission` / star stings on bosses; leaks concentrate in waves 10-11 (the Chomp block), not on wave 5 | Unchanged | Partial |
| 10 | Session 15-25 min with fast-forward | Bot runs 12 min (calls every wave early); 8-10 min with the fighter | ~0.3 min shorter (first wave sooner) | Pass for a player who does not call early (15 waves at 24 s + ~40 s fights); not measured on a person |

## Star Deck (`deck*`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | Foe intent visible every turn; momentum predictable | Intent icon, colour and amount over every foe (`deck_ui` `intent_label`), momentum and chain on the HUD; `enemy_intents_are_telegraphed_and_resolve` | Unchanged | Pass (code/tests) |
| 2 | First reward within 90 s; 3 options + skip | Every reward 3 cards + skip (pinned in the new report). First fight: 2.5 turns on average (longest 6), 3.1 cards a turn | Unchanged | Pass by estimate: 2.5 turns x ~20-30 s for a person is 50-75 s; not timed with a person |
| 3 | Chains feel different (bigger feedback, pitch rises along the chain) | Every link: "CHAIN xN" float, 10 sparks, the same `coin` chime | Chime climbs `gem1` > `gem2` > `gem3` > `star`, sparks 10/16/22/28, stage shake 0/0.15/0.3/0.45 | Fixed, pinned (`chains_escalate_and_landed_cards_hit_stop`) |
| 4 | A turn takes 10-30 s; speed setting | `_FAST`/tempo plumbing exists; 3.1 cards a turn at ~0.5 s of animation each | Unchanged | Pass by estimate |
| 5-7 | Pick/win rate per card, engines, map divergence | Policy bot picks by a fixed rule, so pick rates are not meaningful; win rates 33-67 % per hero (`every_hero_has_a_healthy_bot_win_rate`) | - | Not measured |
| 8-9 | Run length, a reason to win twice | One act, daily seed + best score | Unchanged | Open (second act / ascension) |
| 10 | Card hits land on the 3D stage with hitstop and a hit spark | Sparks on the foe, shake 0.35, `StompGoomba`, damage float; **no hit-stop** | A landed card holds the fight clock 50-150 ms (by damage) | Fixed, pinned |

## Last Star Standing (`lss*`)

Core measurement: 39 bots + the player on the bot brain, real elimination rules, 6 seeds (flat island; the real-island
pacing test `a_full_field_on_the_real_island_thins_out` is unchanged). "In view" = a living rival within 3000 units.

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | Player's first fight within 45-90 s of landing | 2, 3, 11, 18, 2 s after landing (one seed: the autopilot never traded blows before going out) | Same (rules unchanged) | Pass (pinned <= 45 s) |
| 2 | Never 30 s without seeing a rival after minute 1 | Longest stretch 0, 16, 0, 28, 0, 0 s | Same | Pass, barely on one seed (pinned <= 30 s) |
| 3 | Player hits have hitstop + shake + sound; a knockout gets a distinct sound and toast | Player's blow: a white puff only. Taken: `bonk` + red puff. KO: `break_box` + toast. **No hit-stop, no shake** | Landed: `bonk`/`stomp` + gold pop + 2-3 ticks of whole-match hit-stop + camera kick; taken: kick 0.28+ and 2-3 ticks; your KO: `explosion` + 4 ticks + kick 0.55 (plus the toast) | Fixed (`the_players_blows_earn_hit_stop_and_a_camera_kick` pins the table; the in-game freeze and kick were not watched on screen) |
| 4 | Field halves by ~2 min, final five by 3-3.5 min | Half out at 83-113 s; final five at 154-203 s; match 200-218 s | Same | Pass |
| 5 | Landing choice matters | - | - | Not measured (needs drop-zone policies) |
| 6 | Zone readable | Toasts for "closing" / "new safe zone", minimap ring, Blight wall (existing) | Unchanged | Pass (code) |
| 7 | Final five fit on one screen | Spread at FINAL FIVE 1578-2946 units | Same | Pass at the camera's ~3000-unit range, marginal on two seeds |
| 8 | Placement rewarded | End card: place, knockouts, best place, wins, NEW BEST | Unchanged | Pass (code) |
| 9 | Spectate or requeue under 5 s | Eliminated card after `CARD_DELAY`, Spectate / Again buttons | Unchanged | Not timed |
| 10 | An airdrop every 45-60 s | Airdrops at 48, 104, 154, 196 s (gaps 56, 50, 42 s) | Same | Pass (pinned <= 60 s) |

Engine changes behind the LSS fix: `CameraOverride::shake` (an offset along the default follow camera's right/up axes,
cleared on exit) and `sim_plugin`: a held step (`freezes_player` or `time_scale() == 0`) now copies `curr` into `prev`, so
the renderer stops replaying the last tick's motion while nothing moves (this also steadies other frozen modes).

## Endless Run (`runner*`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | Death to running again under 2 s (one button); first obstacle within 3 s of the restart | Fall called 1300 under the track (~0.85 s of free fall) + A locked 20 ticks + 45-tick countdown: ~2.9 s. Segments 0-1 forced plain: first obstacle ~5 s after GO | Fall at 800 (~0.6 s) + 8-tick lock + 12-tick retry countdown (first run keeps 45): ~1.3 s. Segment 1 may hold an obstacle: median 2.7 s after GO (worst 6.8 s over 60 seeds) | Fixed, pinned (`a_retry_is_quick_and_the_first_obstacle_comes_soon`) |
| 2 | Difficulty visibly rises every 20-30 s | Ramp over 40 segments, themes at 12 and 26 (round 2) | Unchanged | Pass by design (not re-timed) |
| 3 | Coins feed the score or a line choice | Coins counted and shown, not in the score | Unchanged | **Not fixed** (changes what a record means; needs a decision) |
| 4 | Every death readable | - | - | Not measured |
| 5 | Near-miss feedback | None | Unchanged | Not fixed |
| 6 | Median first run 45-90 s | Scripted player runs 282-304 m in 32-46 s over 14 segments without falling | Same | Not measured on a new player |
| 7 | Ghost visible and beatable | Daily ghost + "GHOST BEATEN - PAST YOUR BEST" | Unchanged | Pass (code) |
| 8-9 | Missions; speed sensation | None | Unchanged | Not fixed |

Side effect: changing segment 1 changes every seed's track from segment 1 on, so a daily ghost recorded today before this
change no longer matches today's track (tomorrow's is fine).

## Kart (`kart*`)

| # | Criterion | Before | After | Status |
| --- | --- | --- | --- | --- |
| 1 | Mini-turbo works and is felt; 2+ s a lap | Drift charge / sparks / burst ported and tested (`drift_indicator_shows_charge_and_the_mini_turbo`, `kart_fx` tests) | Unchanged | Feel pass by tests; the lap gain was **not measured** (needs a drifting vs non-drifting time-trial driver) |
| 2 | Items by position match the decomp; 1st place hit in most races | `roulette_curves_are_the_roms_tables`, `curves_match_the_exported_json`. Leader hit by an item in **16/16** races at 100cc | Unchanged | Pass |
| 3 | Pack racing: within 3 s of another kart most of the race (100cc) | 62 % of race time overall; 73-96 % on most tracks, 46-60 % on Toad's Turnpike, Frappe, Wario, Yoshi, Banshee; Rainbow Road 17 % (the scripted player did not finish it) | Unchanged | Pass overall; the scripted player is not a person |
| 4 | More than one track and driver | 16 race tracks and 8 drivers raced in `every_track_can_be_raced_for_three_laps` / `racers_drive_differently`; track select in the kart menu | Unchanged | Pass |
| 5 | Start boost, Lakitu rescue | Tested (`StartBoost`, `falling_off_the_course_is_rescued_gracefully...`) | Unchanged | Pass (tests) |
| 6 | Time trial with ghost and records | `time_trial_benches_the_cpus_records_and_keeps_only_better_ghosts`, ghost store | Unchanged | Pass |
| 7 | Battle: first pop within 30 s, rounds 1-3 min | Battle tests exist; timing not measured | - | Not measured |
| 8 | Touch controls | - | - | Not checked |

No kart change: it is a port of MK64 and passed the measurable items.

---

## Not verified / limits

- Headless offscreen runs of the built game (1280x720) after the changes: Gem Clash (demo bot vs Chuckya on Bob-omb
  Summit: hits, gems, power form, hit flashes on screen, no errors), Last Star Standing (seed 3 from 20 s: runs, no
  errors, but no player blow landed, so the in-game freeze and camera kick were **not seen**), Endless Run (daily track:
  slalom pillars by 38 m; the idle runner did not fall, so the in-game retry was **not timed**; the retry time is from the
  unit test's arithmetic). Hold the Gate, Star Deck and Kart were not run in the game. No pad, phone, iOS simulator or
  desktop window.
- All "player" numbers come from bots; the human-facing ones (first contact, round length, turn time, session length) are
  estimates.
- Hold the Gate's early game is still free for a builder: the fix needs a harder setting.
- Endless Run coins, near-misses, missions and speed sensation are untouched.
