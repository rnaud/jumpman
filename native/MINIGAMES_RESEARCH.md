# Minigames research: what the reference games do, and how to audit ours

This is a research companion to `MINIGAMES_FUN.md` and `MINIGAMES_FUN_2.md`. For every mode in GAMES (and the four
EXPERIMENTS) it lists the commercial games the genre comes from, the design principles that make those games fun (with
sources), and a **checklist an auditor can apply to our version**, ordered by expected impact. The checklists are meant
to be measured: most items can be checked with the existing hooks (`JUMPMAN_OFFSCREEN`, `JUMPMAN_XP_LOG=<secs>`, each
mode's bot/demo hooks, the deterministic cores' tests), the rest need a person with a pad.

No game code was changed for this document. Numbers quoted from the reference games come from the cited sources; where a
source is a fan wiki, a review or a secondary summary rather than the designer, it says so. Thresholds in the checklists
("within 20 s", "every 60-90 s") are design targets derived from those games, not laws: treat a miss as "look at this",
not as "broken".

---

## 0. Cross-cutting principles (apply to every mode)

These come up in almost every source below, so they are listed once.

1. **Game feel / juice.** Every player action and every hit should answer with several layered signals: a short freeze
   (hitstop), a sound, a particle or flash, a little camera shake, a knockback that reads. Sakurai uses hitstop heavily in
   Smash and tunes it to each game's speed, and has a video on eight ways to strengthen it (shake the victim, keep the
   hitbox in place, ...). Vlambeer's "Art of Screenshake" took a flat shooter and made it feel good with ~30 small
   additions; "Juice It or Lose It" made the same case: "maximum output for minimum input".
   - Sakurai on hitstop (Famitsu column, fan translation): https://sourcegaming.info/2015/11/11/thoughts-on-hitstop-sakurais-famitsu-column-vol-490-1/
   - Sakurai's hitstop video, summarised: https://www.gonintendo.com/contents/13581-sakurai-s-latest-game-dev-video-features-8-hit-stop-techniques
   - Smash hitlag reference: https://ssbwiki.com/Hitlag
   - Jan Willem Nijman, "The Art of Screenshake" (INDIGO 2013): https://www.youtube.com/watch?v=AJdEqssNZ-U ; write-up: https://gamedeveloper.com/design/vlambeer-co-founder-shares-advice-on-building-better-action-games
   - Jonasson & Purho, "Juice It or Lose It" (GDC Europe 2012): https://gdcvault.com/play/1016487/Juice-It-or-Lose ; https://gamedeveloper.com/design/video-is-your-game-juicy-enough-
2. **Fun before study.** The breakout run-based games give power and spectacle in the first minute and defer
   complexity until the player is hooked (Vampire Survivors is the textbook case).
   - https://birdor-blog.pages.dev/vampire-survivors-low-friction-success-case-study/ (secondary case study)
   - https://theconversation.com/vampire-survivors-how-developers-used-gambling-psychology-to-create-a-bafta-winning-game-203613
3. **Failure should feel like your fault.** Fall Guys' lead designer put the target difficulty as "you always feel like
   it was your fault that you failed". Unreadable deaths (off-screen hits, unseen hazards, camera behind a wall) are the
   biggest single fun killer in our camera-heavy 3D modes.
   - https://gamedeveloper.com/design/q-a-designing-for-seasons-in-bumbling-platform-royale-i-fall-guys-i-
4. **Short loops, instant retry, a reason to go again.** Daily seeds, ghosts, records and unlock ladders (Spelunky's
   Daily Challenge made a run social and nerve-wracking by giving everyone the same cave).
   - https://gamedeveloper.com/design/the-understated-genius-of-the-i-spelunky-i-daily-challenge
   - https://spelunky.fandom.com/wiki/Daily_Challenge_Mode_(HD)
5. **Measure it.** Mega Crit balanced Slay the Spire on two metrics per card: how often it is picked when offered and
   how often it is in a winning deck. Our deterministic cores make the same kind of measurement cheap.
   - https://gdconf.com/news/learn-slay-spires-successful-metrics-driven-approach-game-balancing-gdc-2019
   - https://gamedeveloper.com/design/how-i-slay-the-spire-i-s-devs-use-data-to-balance-their-roguelike-deck-builder

**Shared checklist (run on every mode first):**

- [ ] Every player hit on a foe and every hit taken has at least 3 of: hitstop (2-6 frames at 30 Hz is plenty), a sound,
      a particle/flash, a camera kick, a readable knockback. Count them on a screenshot sequence.
- [ ] Time from choosing the mode to first player input that matters is under 15 s (briefings skippable, no forced menus).
- [ ] Retry after a loss takes one button and under 5 s.
- [ ] No death or loss in 10 recorded runs is caused by something that was off-screen or hidden by the camera.
- [ ] The mode states its goal in one line on screen in the first 5 s.
- [ ] A record (best time / place / wave / score) is shown at the end and the moment it is beaten.
- [ ] The mode works and is legible at phone aspect (19.5:9) with touch controls; HUD text readable at arm's length.

---

## 1. Gem Clash (`brawl*`)

**Ours.** 2-4 fighters in small multi-level arenas cut from real courses, throwable props, three weapons, three gems that
trigger a timed power form, a closing rim, falling shells; Versus (best of three) and a six-stage Gauntlet.

**Reference games.** Power Stone 1/2 (the direct model: items, environmental interaction, gem transformation), Super
Smash Bros. (items, knockback readability, hitstop, camera), Spider-Man/Marvel-style 3D arena brawlers, Towerfall/Stick
Fight for "small arena, short rounds".

**What makes them fun.**
- **The objective is not only the opponent.** Power Stone matches are about controlling space and chasing items and
  stones while using the arena as cover; everyone starts equal and the advantage is found on the map. The stage itself is
  a weapon (boxes, barrels, poles, lamps you can swing round or throw). The producer said strategic depth needed
  interesting stage design and terrain features.
  - https://www.inkl.com/news/power-stone-the-dreamcast-brawler-that-foresaw-fortnite-and-overwatch (Guardian retrospective)
  - https://shmuplations.com/?p=18118 (1999 developer interview)
  - https://en.wikipedia.org/wiki/Power_Stone_(video_game)
- **The transformation is a comeback and a spectacle.** Collecting three stones flips the fight for a short window; the
  other fighters' job becomes running away or stealing the stones back, which creates chases. The window is short so it
  does not end the round on its own.
- **Hits read from far away.** Smash pulls the camera back, so Sakurai compensates with heavy hitstop, victim shake,
  hit marks and launch trails; he tunes hitstop to the speed of each game.
  - https://sourcegaming.info/2015/11/11/thoughts-on-hitstop-sakurais-famitsu-column-vol-490-1/
  - https://www.gonintendo.com/contents/23556-masahiro-sakurai-discusses-hit-marks-in-his-latest-dev-video
- **Pace.** A Power Stone arena is crossed in about two seconds; fights are dense because nobody can be far away. Our own
  measurement (`MINIGAMES_FUN_2.md`, Gem Clash pace) already moved Mario to 760 u/s for this reason.
- **Social moments.** Steals, last-second throws, a power form ending a hair before the knockout. Rounds short enough
  (30-90 s) that a loss is followed at once by another try.

**Audit checklist (by impact).**
1. [ ] **Every landed hit has hitstop + sound + particle + victim shake**, scaled with damage; heavy hits and throws
       also kick the camera. Power-form attacks are visibly bigger than normal ones.
2. [ ] **First contact within 3 s of "GO"** in a 1v1 and every arena; nobody is ever more than ~2 s of running from the
       nearest opponent (measure distances in a bot match).
3. [ ] **A pickable object or weapon is within ~1 s of every fighter at all times** in the first 30 s (Power Stone's
       "the map is the arsenal").
4. [ ] **Gem holder is readable at a glance** (glow / marker visible at the widest camera zoom), and a gem can be knocked
       loose by a hit, so the 3-gem race produces chases. Check: in 20 bot rounds, gems change hands at least once in most
       rounds.
5. [ ] **Power form lasts 8-15 s** and in bot matches wins its fight most of the time but does not end the round outright
       in more than about half of cases (comeback without auto-win).
6. [ ] **Round length 40-90 s** median; a match (best of three) under 5 min.
7. [ ] **Every set piece telegraphs at least 1 s** before it can hurt (lava tide, cannon, wind, crumbling bridge) with
       both a visual and a sound; no knockout from a set piece that was off camera.
8. [ ] **Camera keeps all fighters and the gem in frame** at all times on phone aspect; no wall between camera and fighters
       (the Shifting Sand Pit fix is the template).
9. [ ] **Throwing a prop at a fighter is worth it**: a throw that connects does at least as much as a 3-hit combo.
10. [ ] **Gauntlet difficulty curve**: a first-time player beats stage 1 on the first try and stage 3 within a few tries;
        King Bob-omb loses to a skilled player more often than not (novice/skilled bot win rates per stage in a test).

---

## 2. Star Swarm (`swarm*`)

**Ours.** An arena survivor on real course stretches: auto-firing weapons, gems to level up, three upgrade cards per
level with evolutions, chests, shrines (charge / challenge / greed / boss altar), a closing storm ring, survive to 7:00 and
kill the Star Tyrant; tiers, ranks and per-level bests between runs.

**Reference games.** Vampire Survivors (the genre's origin), Brotato (short timed waves, a shop between), Megabonk (3D
movement, shrines, which ours already borrows from), Halls of Torment, 20 Minutes Till Dawn.

**What makes them fun.**
- **Power early, build later.** The smallest repeated pleasure is a build growing absurdly strong while the player narrowly
  survives. Complexity comes after commitment.
  - https://birdor-blog.pages.dev/vampire-survivors-low-friction-success-case-study/ (secondary)
  - https://jboger.substack.com/p/the-secret-sauce-of-vampire-survivors
  - https://www.superjumpmagazine.com/vampire-survivors-a-study-of-an-indie-phenomenon/
- **Build choices matter as much as dodging**, and weapons behave in clearly different ways; evolutions reward planning
  two picks ahead. Variable rewards (chest openings, level-up rolls) are deliberately slot-machine-like: Galante came from
  slot-machine graphics, and The Conversation's researchers tie the loop to gambling reward structures.
  - https://theconversation.com/vampire-survivors-how-developers-used-gambling-psychology-to-create-a-bafta-winning-game-203613
  - https://pcgamer.com/vampire-survivors-saved-its-creator-from-working-on-mobile-gambling-games
  - https://indiecator.org/2022/01/29/indietail-vampire-survivors/ (evolution system)
- **Short waves let a stat spike be felt at once.** Brotato's waves are 20 s at the start and grow to 60-90 s; a run is
  20 waves, under 30 minutes. A big stat change shows up within minutes.
  - https://adrianhon.substack.com/p/brotato ; https://mssv.net/2023/08/29/brotato/
- **3D adds movement as a skill and risk-reward on the map.** Megabonk's reviews single out jumping (double/triple jumps,
  airborne damage) and shrines: free item shrines, shady dealers, and risk shrines (extra bosses, greed for difficulty,
  challenge waves for chests). Level design with drops, bottlenecks and "rewards tantalisingly out of reach".
  - https://pcgamer.com/games/roguelike/megabonk-review
  - https://www.destructoid.com/reviews/megabonk-review/
  - https://fullcleared.com/reviews/megabonk-review/
- **Pacing curve.** Waves of density spikes and lulls, bosses at fixed times, a final screen-filling crescendo; the end of
  a winning run should be the most spectacular minute of it.

**Audit checklist (by impact).**
1. [ ] **First level-up within 20-30 s**, and level-ups every 15-30 s through the first 3 minutes (log levels with
       `JUMPMAN_XP_LOG`). First evolution possible by about minute 4-5 for a focused build.
2. [ ] **A felt power spike every 60-90 s**: a new weapon, an evolution, a chest, or a shrine reward. Count events in a
       bot run; a 90 s gap with no spike is a flag.
3. [ ] **Each upgrade card changes what you see** (new projectile, bigger area, more count), not only a hidden number;
       at least 2 of 3 cards per level are visibly different picks.
4. [ ] **Kill feedback scales with the crowd**: individual kills are cheap (small pop, short sound with pitch variation,
       capped voices) but a big clear (quake, evolution nuke) gets a bigger flash/shake. The frame never drops on a clear.
5. [ ] **Density curve has lulls and spikes**: foes on screen over time rises with clear dips after bosses; the last 60 s
       before the Tyrant is the densest of the run.
6. [ ] **Risk shrines are taken by choice**: greed/challenge pay enough that a skilled bot policy that takes them wins at
       least as often as one that skips them, on NORMAL.
7. [ ] **Contact damage is readable**: the hero flashes and is briefly invulnerable after a hit; elites and bosses
       telegraph their charges for at least 0.7 s.
8. [ ] **Run length 7-10 min including the Tyrant**; a death ends in under 3 s with a summary that shows star bits earned
       and what to buy (meta progression visible at the end of every run, win or lose).
9. [ ] **Meta progression adds options, not only stats**: at least one new character/weapon/level unlock in the first
       3 runs.
10. [ ] **Chests feel like a jackpot**: an opening animation with escalation (1 / 3 / 5 rewards), sound build-up.

---

## 3. Hold the Gate (`gate*`)

**Ours.** Tower defence on the real castle lawn: six tower kinds with three levels, 15 waves with a boss every fifth, two
maps, 20 lives, call waves early for coins; Mario fights beside the towers (punch / dive / pound, rally, overdrive).

**Reference games.** Orcs Must Die! 1-3 (hero plus traps), Kingdom Rush (pacing, hero unit, tower upgrades), Bloons TD 6,
Sanctum and Dungeon Defenders (action TD), Plants vs. Zombies (onboarding one new thing per level).

**What makes them fun.**
- **The hero matters, but the build wins.** Orcs Must Die adds a mobile, armed hero to stationary defences; traps sit in
  the path without blocking it. A common criticism is when level geometry and pricing make most of the toolbox
  pointless.
  - https://ludusnovus.net/2015/05/06/orcs-must-die-blocking-strategies/
  - https://scientificgamer.com/thoughts-orcs-must-die
  - https://www.pcgamesn.com/orcs-must-die-3/gameplay-traps-war-machines
- **Pacing of arrivals against your upgrades.** Kingdom Rush: Origins is praised because the timing of enemy arrivals and
  how it interacts with your upgrades is "immaculate"; the series leans to macro decisions over micro.
  - https://www.gameinformer.com/games/kingdom_rush_origins/b/ios/archive/2014/12/09/kingdom-rush-origins-game-informer-review.aspx
  - https://www.gamedeveloper.com/design/kingdom-rush---the-wonderful-campaign-level-design
  - https://www.kodeco.com/2769-making-a-hit-tower-defense-game-a-top-app-dev-interview
- **Readable threat, then a test.** Wave previews let the player plan; each new foe type is introduced alone before it is
  mixed in; boss waves are the exam of the build.
- **Risk-reward on tempo.** Calling a wave early for coins is the genre's best risk-reward lever (Kingdom Rush's early call
  bonus); ours already has it.
- **Satisfying chain reactions.** Watching a well-placed maze shred a wave (traps combining) is the payoff moment.

**Audit checklist (by impact).**
1. [ ] **First meaningful decision within 15 s** (place the first tower) and first foe within 30 s of the start.
2. [ ] **Mario's contribution is measurable but not mandatory**: a builder-only bot and a builder+fighter bot both clear
       wave 5; by wave 15 the fighter bot keeps clearly more lives (e.g. 3+ more). If Mario does nothing useful the hero
       is decoration; if he is mandatory, building is decoration.
3. [ ] **A new thing every 1-2 waves** in waves 1-10: a new foe type, a new tower affordable, a boss. Log it per wave.
4. [ ] **Early waves are not free**: a passive builder loses at least one life before wave 7 on the harder map, or there is
       a harder setting (the first audit found the early game too gentle).
5. [ ] **Wave preview shows composition and path** before the call; a leak is never a surprise from an unseen lane.
6. [ ] **Every tower kind is picked**: in 50 bot-policy runs that choose from the cheapest effective set, no tower is
       never built (Slay-the-Spire-style pick rate), and no single kind is in every winning build.
7. [ ] **Overdrive/rally is visible and satisfying**: ground-pound on a tower gives a flash, sound and a clearly stronger
       burst; you can see why it mattered.
8. [ ] **Calling early pays**: the coin bonus for calling the next wave early is worth taking by wave 5 (a bot that calls
       early has more towers by wave 10 without losing more lives).
9. [ ] **Boss waves (5/10/15) are each a spike**: boss intro, health bar, music or sting, and the highest leak risk of
       their block of five.
10. [ ] **Session length 15-25 min** for 15 waves with a fast-forward; a mid-run save or a "continue" if longer.

---

## 4. Star Deck (`deck*`)

**Ours.** Slay-the-Spire-style run of fights, shops, rest sites, events and a boss on a branching map; cards are Mario
moves and energy is *momentum* that grows with chained Jump / Dive / Pound; a deck per hero; daily seed; one act.

**Reference games.** Slay the Spire (structure), Balatro (score-chasing combos and jokers), Monster Train, Inscryption,
Griftlands.

**What makes them fun.**
- **Every reward is a choice, including "skip".** Pick one of three cards after a fight; the deck is thinned as much as it
  is grown. Mega Crit balanced each card on pick rate and win rate, across an Early Access community.
  - https://gdconf.com/news/learn-slay-spires-successful-metrics-driven-approach-game-balancing-gdc-2019
  - https://gdcvault.com/play/1025731/
  - https://gamedeveloper.com/design/how-i-slay-the-spire-i-s-devs-use-data-to-balance-their-roguelike-deck-builder
- **Intent is shown.** Slay the Spire shows what every foe will do next turn, so each turn is a puzzle with full
  information, not a guess.
- **Synergy and the broken run.** Balatro's depth comes from jokers that bend the base rules and multiply each other; the
  pleasure is building an engine that produces absurd numbers. Its creator built it from Luck Be a Landlord's single-
  player loop, not from love of poker.
  - https://www.pcgamer.com/i-dont-play-poker-at-all-says-solo-developer-who-made-the-poker-roguelike-i-cant-stop-playing/
  - https://rogueliker.com/balatro-interview/
  - https://www.gamesradar.com/exploring-balatros-hype-its-ingenious-twists-on-poker-and-its-mysterious-creator
- **Map as a risk-reward planner.** Elites for relics vs. safe paths, rest vs. upgrade, shops vs. fights.
- **Escalation after a win.** Ascension levels (StS) and stakes (Balatro) give a winning player a reason to replay; ours
  has one act and no ascension yet (open item in the first audit).

**Audit checklist (by impact).**
1. [ ] **Foe intent is visible every turn** (attack amount, block, buff) and the momentum you will have next turn is
       predictable from the hand; no hidden information decides a fight.
2. [ ] **First card reward within 90 s** of starting a run; every reward offers 3 options plus skip.
3. [ ] **Chaining is the fun**: playing Jump > Dive > Pound feels different from three unrelated cards (bigger
       animation, momentum counter pops, sound pitch rises along the chain). Count the feedback on a chain.
4. [ ] **A turn takes 10-30 s** for a decisive player; animation speed setting exists (the `_TEMPO`/`_FAST` hooks show
       the plumbing).
5. [ ] **Pick-rate / win-rate table per card** from the autoplay bot over 600 seeds: no card under ~5% pick when offered
       (dead card) and none above ~80% (auto-pick), per hero.
6. [ ] **At least one "engine" per hero** that a player can discover and that wins with big numbers (a combo that scales
       with momentum); it should appear in a winning deck in at least 10% of bot wins.
7. [ ] **Map choices diverge**: the policy bot's win rate is noticeably different between elite-heavy and safe paths (a
       real trade-off, neither dominates).
8. [ ] **Run length 15-30 min** per act; daily seed shows a score to beat.
9. [ ] **A reason to win twice**: ascension/heat levels or a second act; unlocks visible after the first win.
10. [ ] **Card hits land on the 3D stage** with hitstop and a hit spark on the foe actor, so the cards feel like Mario's
        moves and not a spreadsheet.

---

## 5. Last Star Standing (`lss*`)

**Ours.** Up to 40 Marios glide onto real Bob-omb Battlefield, loot camps, chests and airdrops, fight with Mario's moves,
the Blight closes in, last one standing wins. About 4 minutes since the pacing fix; streak and field-size callouts.

**Reference games.** PUBG, Fortnite, Apex Legends (zone and loot pacing), Fall Guys and Tetris 99 (short, casual,
elimination-show framing), Super Smash "Stamina" free-for-alls for fighting on Mario's moves.

**What makes them fun.**
- **The zone is the pacing tool.** PUBG's own dev letter: stacking shrink, damage and combat pressure into the same windows
  hurt the late game; they lengthened late shrink times to give room for late fights and cut early downtime. Design
  writers call zone timing and damage the main levers on match pace.
  - https://pubg.com/news/10280 ; https://patched.gg/games/pubg-battlegrounds/dev-letter-blue-zone-revamp
  - https://gamedesignskills.com/game-design/battle-royale/
- **Fewer players, better chances.** Mediatonic cut Fall Guys from 100 to 60 because it played better and gave everyone a
  better chance of winning. For a solo mode, the feeling of "I could win this" matters more than the head count.
  - https://www.pcgamesn.com/fall-guys/player-count
- **Rounds that create stories.** Fall Guys' rounds are judged on being simple, replayable, and able to produce
  "one-in-a-million moments".
  - https://gamedeveloper.com/design/q-a-designing-for-seasons-in-bumbling-platform-royale-i-fall-guys-i-
- **Loot as a fast early arc.** Landing choice (hot spot vs quiet edge) is the first risk-reward choice; a good drop gives a
  weapon within seconds; loot quality tells you how bold to be.
- **The final circle is the climax.** Everyone remaining is visible and close; this is where tension and spectacle should
  peak. Placement matters even in a loss (top 10, top 5 feel like achievements).
- **Downtime is the enemy.** Long searches for the last opponents are a known complaint.
  - https://forums.ea.com/discussions/apex-legends-feedback-en/public-matches-need-better-pacing/13697223 (player feedback)

**Audit checklist (by impact).**
1. [ ] **The player's first fight within 45 s** of landing on a hot drop and within 90 s on a quiet one (log the player's
       first exchange of hits, not the field's).
2. [ ] **The player is never more than 30 s without seeing an opponent** after minute 1 (the first audit flagged that the
       action near the player is lighter than the field average).
3. [ ] **Player hits have hitstop + shake + sound** (first audit: no hit-stop or shake on the player's own hits). A
       knockout gets a distinct sound and a toast.
4. [ ] **Elimination curve**: the field halves by about minute 2 and the final five arrive by minute 3-3.5 in a 4-min match
       (already roughly true; re-check after any change).
5. [ ] **Landing choice matters**: a hot drop gives a weapon within 10 s but meets 2+ rivals; a quiet drop gives time but
       worse loot. Measure loot tier and first-fight time per drop zone over seeds.
6. [ ] **Zone is readable**: the next safe circle is shown before it shrinks, with a timer and a sound cue; Blight damage
       ramps so standing in it early is survivable and late is not.
7. [ ] **Final circle fits on one screen**: at "FINAL FIVE" the remaining fighters are within camera range of each other.
8. [ ] **Placement feels rewarded**: the end screen shows place, knockouts, and records (best place, wins) with a
       better-than-before callout; top 10 counts as progress.
9. [ ] **Spectate or instant requeue** after elimination, under 5 s to a new match.
10. [ ] **An airdrop or event every 45-60 s** that pulls fighters together.

---

## 6. Summit Race (`race.rs`)

**Ours.** You and seven full-physics rivals race up Bob-omb Battlefield along Koopa the Quick's route; classes Rookie /
Pro / Champion with unlocks; splits, place toasts, a Lakitu lift over the unclimbable stretch; a good run is about 70 s.

**Reference games.** Koopa the Quick races (SM64 itself), Mario Party race mini-games, Fall Guys race rounds (Door Dash,
Hit Parade), Getting Over It / Only Up (climbing with real stakes), SSX/Trackmania for "learn the line, shave seconds".

**What makes them fun.**
- **A route to master.** Racing games live on learning the line and finding faster routes; time trials with splits and
  ghosts turn one route into dozens of runs.
- **Pack racing and the jostle.** Fall Guys races open in a crowd and string out; the fun is in the bumps and the close
  finish. Rounds must be simple and replayable; failure should feel like your own fault.
  - https://gamedeveloper.com/design/q-a-designing-for-seasons-in-bumbling-platform-royale-i-fall-guys-i-
  - https://gamesbeat.com/?p=190067
- **Falling is the stake.** Foddy's Getting Over It makes the mountain one continuous space where a fall throws you back
  past ground you already climbed: tension comes from what you can lose. We do not want frustration as the aesthetic, but a
  short climb benefits from a fall costing real seconds and a risky shortcut saving them.
  - https://design.google/library/bennett-foddy-getting-over-it-interview
  - https://gamedeveloper.com/design/designer-interview-the-aesthetics-of-frustration-in-i-getting-over-it-i-
- **Rubber banding that is felt but not seen.** Keep the race close (soft rubber band) without the player catching rivals
  cheating; Mario Kart is the cautionary example (see Kart).

**Audit checklist (by impact).**
1. [ ] **Close finishes**: in Pro class with a competent player (the Champion-brain hook), the gap between 1st and 4th is
       under 5 s in most races, and the player's place changes at least 3 times per race.
2. [ ] **A risky shortcut exists and pays**: at least one line (long-jump chain, ledge skip) that saves 2+ s and costs
       more if missed. If the only way up is the rail, the race is a reflex test.
3. [ ] **Rivals visible most of the race**: at least one rival within ~15 m of the player 70%+ of the time in Pro.
4. [ ] **Splits against your best at every quarter** with green/red colour and a sound (in place; check it reads on phone).
5. [ ] **Contact is fun, not random**: a shove or punch staggers but never sends the player off the mountain more than
       once per race at Pro.
6. [ ] **The Lakitu lift does not feel like a cutscene**: under 2 s, the camera stays behind the racer, and rival
       positions are preserved so nobody gains a place from it.
7. [ ] **Instant restart** from the pause menu and from the results screen, under 3 s to countdown.
8. [ ] **Class ladder takes 3-10 races**: a new player wins Rookie within 3 tries; Champion needs a clean run with the
       fast lines.
9. [ ] **Ghost of your best run** (the Endless Run ghost tech exists) to race against in a time-trial option.
10. [ ] **Race length 55-90 s.** Longer than 2 min means something is wrong (stuck, lost the route).

---

## 7. Endless Run (`runner*`)

**Ours.** Auto-run along a generated track on real SM64 physics; steer, jump, long-jump, dive; one fall ends it; themes
change at segments 12 and 26; daily seed with a ghost; 100 m chimes and "PAST YOUR BEST". 1-3 minutes a run.

**Reference games.** Temple Run, Subway Surfers, Canabalt, Alto's Adventure, Super Mario Run (platformer auto-runner),
Geometry Dash (memorised daily/levels).

**What makes them fun.**
- **Escalating challenge.** Almost every endless runner ramps speed, density or hazard variety; Temple Run speeds up,
  Subway Surfers adds density; Temple Run's short view distance makes a wrong decision fatal in a split second.
  - https://dinogame.gg/blog/difficulty-curve-in-flappy-bird/
  - https://skich.app/blog/subway-surfers-vs-temple-run
- **Missions and multipliers give a run a purpose beyond distance.** Subway Surfers' missions (three tasks each) raise a
  persistent score multiplier (up to x30); a finite hoverboard is a safety net that keeps tension.
  - https://gameworldobserver.com/2016/06/24/subway-surfers-gameplay-analysis
  - https://medium.com/udonis/subway-surfers-lessons-from-the-record-breaking-mobile-game-8a0d26635945
- **Near misses and flow.** Rewarding the barely-avoided obstacle (bonus points, a whoosh) makes skill visible.
  - https://seanking11.github.io/gamebootcamp/modules/04-endless-runner (course material)
- **Same track for everyone today.** Daily seed + ghost is the Spelunky Daily idea applied to a runner.
  - https://gamedeveloper.com/design/the-understated-genius-of-the-i-spelunky-i-daily-challenge

**Audit checklist (by impact).**
1. [ ] **Death to running again under 2 s** (one button) and the first obstacle within 3 s of the restart.
2. [ ] **Difficulty rises every 20-30 s** in a way the player can name (denser, new obstacle, new theme). Since Mario's top
       speed is capped (round 2 audit), the ramp must come from density, gap length and new obstacle kinds, or a pressure
       element (the "lava rises behind you" idea).
3. [ ] **Coins matter**: coins feed the score (multiplier) or a currency; at least one route choice per theme between a
       safe line and a coin line (round 2 audit: coins do not feed the score).
4. [ ] **Every death is readable**: in 20 recorded deaths, the obstacle that killed was on screen for at least 0.8 s.
5. [ ] **Near-miss feedback**: clearing a gap edge or pillar by a small margin gives a sound and points.
6. [ ] **Median first-run length 45-90 s** for a new player; a skilled player's runs reach the third theme.
7. [ ] **Ghost visible and beatable**: the daily ghost is in view at the start and passing it is announced (in place).
8. [ ] **Missions**: three short goals per session ("long-jump 5 gaps", "reach 500 m in the brick fortress") give a
       reason to start a run that is not "beat my best".
9. [ ] **Speed sensation**: FOV / camera pull-back or wind lines grow with density so the run feels faster even at a fixed
       top speed.

---

## 8. Sunstone Shindig (`party/`)

**Ours.** A four-seat board game on real Bob-omb Battlefield (three boards), items, duels, a Keeper selling the Sunstone,
bonus awards; five physics mini-games (Coin Scramble, Summit Crown, Bob-omb Rain, Star Tag, Goomba Stomp) after each
round; 6 rounds by default, 8-12 minutes with a person.

**Reference games.** Mario Party (1-3 and Superstars especially), Mario Party Jamboree, Pummel Party, Fuzion Frenzy,
WarioWare for mini-game brevity.

**What makes them fun.**
- **Luck on the board, skill in the mini-games.** Critics describe the pillars as skill, strategy and luck; the boards lean
  on chance, the mini-games on skill, and the luck keeps a stronger player from winning every time.
  - https://brandonthegamedev.com/5-lessons-from-mario-party-for-board-game-developers/
  - https://dualshockers.com/super-mario-party-review-nintendo-switch/
- **Mini-games are the main source of fun.** "A Mario Party game is only as good as its minigames"; they break the
  monotony of the board.
  - https://gamingtrend.com/reviews/mario-party-10-review/
- **Catch-up mechanics are required in a social game**, even if they irritate skilled players; last-minute bonus stars and
  events produce comebacks and rivalries. Note that predictable bonus stars are not surprising, so the twist should be
  partly hidden.
  - https://brandonthegamedev.com/5-lessons-from-mario-party-for-board-game-developers/
- **Too much luck kills it.** Reviews of Mario Party 10 and some modes call outcomes that depend entirely on luck the main
  complaint.
  - https://gamingtrend.com/mario-party-10-review
- **Downtime is the solo problem.** Watching three CPUs take their turns is the weakest part (our own audit agrees).

**Audit checklist (by impact).**
1. [ ] **Solo downtime**: CPU turns take under 4 s each with fast animations on by default for CPU moves (skippable); the
       player acts at least every 15 s on average across a game.
2. [ ] **Mini-game every 60-90 s** of real time (one per round, so a round must take no more than about 90 s with board
       animations).
3. [ ] **Mini-game length 20-45 s**, with a 3 s rules card and a practice option; rules learnable from the card alone.
4. [ ] **Skill wins mini-games**: a skilled bot beats random-input bots in each mini-game 70%+ of the time; no mini-game is
       a coin flip (Summit Crown's "nobody scores while contested" needs this check).
5. [ ] **Comeback**: in bot games, the player in last after round 3 wins the game in 10-25% of cases (not zero, not
       half); a last-round event or hidden bonus award makes the final ranking uncertain until the reveal.
6. [ ] **Variety in a session**: in a 6-round game no mini-game repeats more than twice; aim for 8+ mini-games so most
       sessions have none repeated (round 1 follow-up: five repeat inside a session).
7. [ ] **Board decisions every turn**: at least one fork, item or shop choice in most of the player's turns.
8. [ ] **Social beats are staged**: steals, duels and the Sunstone purchase get a camera cut, sound and a reaction from the
       other characters.
9. [ ] **Session 8-12 min** for 6 rounds with one human; results screen with awards.
10. [ ] **Local multiplayer is tested with real gamepads** (never done so far).

---

## 9. Relic Descent (`relic*`)

**Ours.** Diablo-style crawler: three procedural floors of rooms whose packs wake on entry, rarity/affix loot, a twelve-
node skill tree with four skills, elites, a three-phase Bomb King, a gold stash for meta progression; dive = dodge with
i-frames, pound = Quake Slam; contextual first-run lessons.

**Reference games.** Diablo 1-4 (loot and the click-kill loop), Hades (run structure, dodge combat, boons, God Mode),
Zelda dungeons (layout, keys and locks, the item that opens the dungeon), Path of Exile, Torchlight.

**What makes them fun.**
- **Loot is the core loop.** Brevik: Diablo was always about loot and how to obtain it; the randomiser is compared to a
  slot machine and fed the loop from day one. He has also criticised newer ARPGs for fast levelling and throwaway loot:
  every drop should be worth looking at.
  - https://www.pcgamer.com/diablo-designer-david-breviks-full-gdc-post-mortem-is-now-online
  - https://gdcvault.com/play/1023469/
  - https://www.tweaktown.com/news/74666/diablos-loot-lottery-rng-is-like-slot-machine-says-david-brevik/index.html
  - https://www.pcgamer.com/games/rpg/blizzard-co-founder-and-diablo-designer-thinks-new-arpgs-have-cheapened-the-genre-with-fast-leveling-throwaway-loot-and-enemies
- **Every death moves you forward.** Hades keeps the death loop central; God Mode adds 20% damage resistance and 2% more
  per death (fan figures), so struggling players still progress; the Pact of Punishment lets strong players tune harder
  runs precisely. Replayability and reactivity were Supergiant's stated goals.
  - https://inverse.com/gaming/hades-god-mode-interview
  - https://www.vice.com/en/article/how-hades-made-a-genre-known-for-being-impossibly-hard-accessible
  - https://GamaSutra.com/design/roguelikes-and-narrative-design-with-i-hades-i-creative-director-greg-kasavin
- **Boons as mini skill trees with a few strong choices**, not a flood of small ones (too much choice dilutes agency).
  - https://benjaminglover.substack.com/p/why-hades-was-a-success (secondary)
- **Dungeons as a puzzle of keys and locks.** Mark Brown's Boss Keys: small keys, one boss key, and a dungeon item that
  opens many locks; a central hub room with long-term goals (Twilight Princess). Even a procedural floor benefits from one
  lock-and-key loop and a landmark room.
  - https://gameinformer.com/b/news/archive/2016/09/27/video-analyzes-the-dungeon-design-of-the-zelda-oracle-games
  - https://roomescapeartist.com/2017/09/10/boss-keys-analysis-zelda-dungeons/
- **Readable combat.** Hades-style: every enemy attack telegraphs, the dodge is generous, the camera never hides the threat.

**Audit checklist (by impact).**
1. [ ] **The first fight is winnable by a new player** who only attacks and dodges sometimes: a dodge-light bot survives
       floor 1 room 1 in 9 of 10 seeds (first audit: the demo bot died in 9 s).
2. [ ] **First item drop within 60 s**, first blue/rare within 3-5 min, and each rarity has a distinct beam/sound visible
       across the room.
3. [ ] **Every enemy attack telegraphs at least 0.5 s** (wind-up pose + flash or ground marker), elites and the Bomb King
       longer; the dive's i-frames cover the hit window reliably.
4. [ ] **Hits have hitstop and sparks, kills a puff and a loot pop** (in place; verify on the four heroes).
5. [ ] **A skill point within the first 3 minutes** and a visible change when it is spent (new skill or visible effect).
6. [ ] **Death progresses the meta**: every run, win or lose, ends with gold banked and an obvious thing to buy; a
       losing player is noticeably stronger by run 3 (a God-Mode-like safety valve).
7. [ ] **Floors have a goal shape**: at least one key/lock or landmark per floor so the player knows where the exit and the
       boss are; time to find the exit under 4 min per floor.
8. [ ] **Loot comparisons are one glance**: an up/down arrow on drops vs. equipped; a junk filter (exists: `Y`).
9. [ ] **Run length 20-35 min** for three floors and the boss; a mid-run save if longer.
10. [ ] **Real-dungeon port**: when the Deku Tree / Dodongo's Cavern version lands, the same checks apply plus "the
        player recognises the place in the first 10 s".

---

## 10. Mario Kart 64 (`kart*`)

**Ours.** A port of MK64 racing: kart physics, all 15 items with the decomp's roulette curves, CPU drivers, GP / Time
Trial / VS / Balloon Battle structures; the round-2 audit calls it "one track and Mario only" in practice.

**Reference games.** Mario Kart 64 itself, Mario Kart 8 Deluxe, Crash Team Racing, Diddy Kong Racing.

**What makes them fun.**
- **The power slide and mini-turbo are the skill.** The official player's guide calls power-sliding at every opportunity the
  most important technique; Kalimari Desert's first turn fits 4-5 mini-turbos. MK64 is often credited with the series'
  highest skill ceiling and most shortcuts.
  - https://www.mariowiki.com/Mini-Turbo ; https://www.mariowiki.com/Power_Slide
  - https://archive.org/download/MarioKart64OfficialPlayersGuide/Mario%20Kart%2064%20Official%20Player%27s%20Guide_text.pdf
  - https://www.thegamer.com/mario-kart-tricks-from-the-franchise-you-had-no-idea-about/
- **Position-weighted items are the comeback mechanic.** Item boxes draw from tables weighted by race position: leaders get
  bananas and green shells, trailing racers get stars and lightning; the Spiny Shell (introduced in MK64) hits the leader.
  The predictability is also a long-running complaint.
  - https://kotaku.com/either-im-paranoid-or-mario-kart-8-deluxes-ai-is-cheati-1795819005
  - https://nintendoworldreport.com/feature/16008
- **Rubber-band AI keeps the pack together** but is resented when it is visible (and it conflicts with monetisation in
  Mario Kart Tour).
  - https://medium.com/game-design-post/mario-kart-tours-rubber-banding-is-at-odds-with-its-monetisation-1f5b43909348
- **Structure gives the races meaning**: cups, points, trophies per CC, time-trial ghosts.

**Audit checklist (by impact).**
1. [ ] **Mini-turbo works and is felt**: the spark colour, sound and boost match MK64; a time-trial lap with mini-turbos is
       measurably faster (2+ s on a lap) than without.
2. [ ] **Item distribution by position matches the decomp tables** (unit test against the exported curves), and 1st place
       is hit by an item at least once in most 3-lap races.
3. [ ] **Pack racing**: the player in a 4-kart field is within 3 s of another kart for most of a race at 100cc.
4. [ ] **More than one track and driver** reachable from the menu (the round-2 audit's main limit). Without this the
       mode is a tech demo regardless of feel.
5. [ ] **Start boost and Lakitu rescue** behave like MK64 (rescue under 3 s, start boost timing window).
6. [ ] **Time trial with a ghost** and saved records per track.
7. [ ] **Battle mode**: first balloon pop within 30 s in a 4-kart arena; a round lasts 1-3 min.
8. [ ] **Touch controls**: steering and drift playable on a phone (drift hop on a separate button, auto-accelerate option).

---

## 11. Experiments

### 11a. Mario vs. God (`god.rs`)

**Ours.** Place blocks, ramps, springs, planks, portals and Bob-ombs on real Bob-omb Battlefield while playing Mario; a
ladder of five stars out of jump reach, scored by pieces used (gold/silver/bronze) and ladder time.

**References.** Super Mario Maker (build and play), Poly Bridge / Bridge Constructor (build with a budget, score by
economy), Garry's Mod and Minecraft creative (sandbox), Fez's "change the world to make the jump possible" feeling.

**Principles.** Mario Maker's guidance is to design for the player and avoid near-impossible layouts; Nintendo's World 1-1
teaches by playing, without text. A creation mode needs a goal with a score (budget puzzles score by pieces or cost),
otherwise it is a toy. Mario Maker also requires that a creator clear their own course before publishing.
- https://gamesradar.com/mario-makers-producer-has-few-level-building-tips
- https://mcvuk.com/development-news/video-miyamoto-shares-his-level-design-secrets/
- https://gamedeveloper.com/design/how-nintendo-draws-i-mario-maker-i-design-cues-from-an-snes-classic

**Checklist.**
1. [ ] The first star (tutorial) teaches placing, testing and undo in under 60 s without reading.
2. [ ] Each later star has at least two distinct solutions (e.g. springs vs. a staircase vs. a portal), checked by a human.
3. [ ] Gold par is reachable: the designer (or a build script) has a solution at or under every gold par.
4. [ ] Place-test-undo loop under 3 s; undo is one button on every input method, touch included.
5. [ ] Touch can place and move pieces (drag), not only "Place" (round-2 audit limit).

### 11b. Across Dimensions (`dimensions.rs`)

**Ours.** One Mario on real BoB with four cameras (3D, side cross-section, top-down, 2.5D path); timed summit with medals.

**References.** Fez (rotate the view, a jump that was impossible becomes easy), Super Paper Mario (flip to 3D), Crush,
Echochrome.

**Principles.** Fez started from the rotation mechanic and cut everything that did not serve it; its levels became tall
because rotation demands it. The fun is the moment a view switch turns an impossible gap into a path.
- https://gamedeveloper.com/design/the-dimensions-of-fez
- https://kotaku.com/going-indie-fez-creator-phil-fishs-moment-of-clarity-5219689
- https://www.engadget.com/2011-09-28-fez-was-inspired-by-icos-minimalism-and-lonely-isolation.html

**Checklist.**
1. [ ] At least 3 spots on the route where switching view is the fastest way through (a shortcut only visible or only
       walkable in one view); otherwise switching is cosmetic.
2. [ ] View switch under 0.5 s with Mario's momentum preserved.
3. [ ] Medal pars measured on a human (or a scripted route), not guessed (round-2 audit).
4. [ ] A "flat run" medal or score bonus for 2D-only climbs (idea from round 2).

### 11c. A Hundred Marios (`hundred.rs`)

**Ours.** A race to the summit through a crowd of up to 100 real-physics Marios with five personalities.

**References.** Fall Guys race rounds (crowd at the start, string out), Totally Accurate Battle Simulator (crowd
spectacle), Mario Party's mass races.

**Principles.** A crowd is fun when it is both obstacle and rival: the jostle at the start, a few rivals who can beat you.
Fall Guys tuned its head count for a better chance of winning, and wants failure to feel like your fault.
- https://www.pcgamesn.com/fall-guys/player-count
- https://gamedeveloper.com/design/q-a-designing-for-seasons-in-bumbling-platform-royale-i-fall-guys-i-

**Checklist.**
1. [ ] At least 3 crowd Marios can finish within 10 s of a good player's time (a "rival" personality; round 2 measured the
       crowd at about 250 s, so place never matters).
2. [ ] The start bottleneck produces visible jostling for 5-10 s, then the field strings out.
3. [ ] Knockbacks from the crowd are a nuisance, not a wall: under 1 lost second per 10 s on average.

### 11d. Shrink Ray (`shrink.rs`)

**Ours.** The same Mario on BoB scaled x8, x3, x1 and x1/4; best time per size.

**References.** Grounded, Micro Machines, It Takes Two's shrinking chapter, Katamari (scale as progression).

**Principles.** Scale is a novelty that lasts when each size has its own goal and route (Katamari turns growth into the
score; It Takes Two changes the challenge with the size). A size that is "a few steps by design" is a joke that should be
short and funny, not a mode.

**Checklist.**
1. [ ] Each size has a distinct challenge (Tiny: a long climb with new routes over grass and posts; Giant: something to do,
       e.g. a stomping score).
2. [ ] Time to a first "wow, that is a blade of grass" moment under 10 s.
3. [ ] If folded into Across Dimensions as a size axis (round-2 idea), switching size keeps the timer and position.

---

## 12. Top three criteria per mode (summary)

| Mode | 1 | 2 | 3 |
| --- | --- | --- | --- |
| Gem Clash | Every hit: hitstop + sound + particle + victim shake | First contact within 3 s of GO, nobody more than ~2 s from a foe | A throwable or weapon within ~1 s of every fighter early on |
| Star Swarm | First level-up in 20-30 s, then every 15-30 s | A felt power spike every 60-90 s | Every upgrade card changes what you see |
| Hold the Gate | First tower within 15 s, first foe within 30 s | Mario measurably helps (fighter bot keeps 3+ more lives) without being required | Something new every 1-2 waves |
| Star Deck | Foe intent and next-turn momentum always visible | First card reward within 90 s, always 3 + skip | Chains feel different (escalating feedback on Jump > Dive > Pound) |
| Last Star Standing | Player's first fight within 45-90 s of landing | Never 30 s without seeing an opponent after minute 1 | Hitstop + shake + sound on the player's hits |
| Summit Race | Close finish: 1st-4th under 5 s, 3+ place changes | A risky shortcut that saves 2+ s | A rival within ~15 m 70%+ of the time |
| Endless Run | Death to running again under 2 s | Difficulty visibly rises every 20-30 s (density/obstacles, not speed) | Coins feed the score or a choice of line |
| Sunstone Shindig | Solo: player acts every 15 s, CPU turns under 4 s | A mini-game every 60-90 s | Skill wins mini-games (skilled bot 70%+) |
| Relic Descent | First fight winnable by a dodge-light new player | First drop within 60 s, rarity readable across the room | Every enemy attack telegraphs at least 0.5 s |
| Kart | Mini-turbo felt and worth 2+ s a lap | Items weighted by position like the decomp | More than one track and driver reachable |
| Mario vs. God | Tutorial star teaches place/test/undo in 60 s | Two solutions per star | Gold pars proven reachable |
| Across Dimensions | 3+ spots where a view switch is the fast way | Switch under 0.5 s with momentum kept | Pars measured, not guessed |
| A Hundred Marios | 3+ crowd rivals within 10 s of a good time | Start jostle then string-out | Crowd knockbacks cost under 1 s per 10 s |
| Shrink Ray | A distinct challenge per size | First "wow" under 10 s | Keeps timer if merged into Dimensions |

## Source quality note

Primary or near-primary: Sakurai's column and videos, Vlambeer and Jonasson/Purho talks, Mega Crit's GDC talk, PUBG's dev
letter, Fall Guys designer Q&A, Brevik's GDC postmortem coverage, Kasavin interviews, Foddy's Design Notes interview,
LocalThunk interviews, the MK64 player's guide. Secondary (reviews, analyses, fan wikis): the Vampire Survivors case
studies, Brotato wave timings (sources disagree on late-wave length), Mario Party lessons, Subway Surfers analyses, Megabonk
reviews, Hades God Mode percentages, Mario Kart item-weighting descriptions. The checklist thresholds are our derivations
from these, to be validated by measurement on our modes.
