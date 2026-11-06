# SM64 fidelity roadmap

Updated November 5, 2026. Finish the missing decomp systems before adding original game concepts. See [COMPLETENESS.md](COMPLETENESS.md) for the current verified/prototype/missing audit.

## Current baseline

Jumpman has core movement, ROM-derived models and animation clips, basic normal-cap swimming, Goombas, selected moving platforms, castle-door transitions, an 8-wedge health/damage pipeline with soft/normal/hard knockback and coin healing, and a first object-interaction layer: Bob-ombs (patrol/chase, fuse, carry, throw, explosion, coin, respawn), breakable boxes (large breakable crates, grabbable small crates, explosion rewards), the Chain Chomp with its wooden posts and gate (spawn/chain constraint, lunge, post release, gate break, contact damage), the purple switch/openable cage grill, the pit bowling balls, a simplified King Bob-omb three-throw fight, a simplified Koopa the Quick race, normal Koopa/shell interactions, and Bob-omb Buddy advice dialogue. The renderer is Three.js and the simulation runs at 30 Hz. Several actions, interactions, effects, and progression rules remain simplified. Extracted assets alone do not implement their original gameplay.

## Port order

1. **Object interactions and behavior execution.** Port hitboxes, interaction flags, object lifecycle/spawning, grabbing, carrying, throwing, breakable crates, switches, and enemy damage. Start with `interaction.c`, `object_list_processor.c`, object helpers, and relevant `behaviors/` implementations. First deliverable: Bob-ombs and breakable crates with correct carry, throw, explosion, and reward behavior. **Status:** the first deliverable is implemented in `src/sim/object.ts`, `src/sim/objectManager.ts` and `src/sim/behaviors/`; King Bob-omb's full cutscene, Koopa's path collision, water-bomb cannon barrel bubbles/audio, the remaining bowling-ball spawners and the remaining behaviors are still open.
2. **Remaining Mario actions and animation timing.** Implement sleeping idle, slow ledge climbing, ceiling hanging, pole climbing, held-object actions, and animation-frame-driven punches. Follow `mario_actions_*.c` and `mario.c`; keep animation transitions, collision, and sound events synchronized with the simulation. **Status:** sleeping idle/wake, slow ledge climbing, ceiling hanging, pole climbing, held-object actions and the animation-frame-driven punch/fast-punch/kick/breakdance sequence are implemented.
3. **Health, damage, and death.** Port damage amounts, knockback, invincibility, healing, drowning/death sequences, lava, and quicksand. Replace immediate respawns where the original uses action sequences; expose the health meter. **Status:** the health meter, damage amounts, soft/normal/hard knockback, invincibility, coin healing and the standing/on-back/on-stomach death sequences, the two-part drowning sequence, lava boost/burn and quicksand sinking/death are implemented; a poison-gas area behavior exists but is not yet placed in a playable area. The HUD shows the source-style 8-wedge meter.
4. **Level scripts, warps, missions, and persistence.** Port mission-dependent object activation, painting entrances, area transitions, star spawning/collection, castle returns, and save data. Use the original level scripts, `level_update.c`, `area.c`, and `save_file.c` as the behavioral reference. **Status:** four local save slots now persist collected stars per Bob act and best coin counts, hide already-collected stars, remember the active slot and can be reset from the pause menu. Bob-omb Battlefield has an Act 1–4 mission selector (including the 8-red-coin mission), act-specific star locations, mission-event star gating and per-act star persistence; the castle lobby's BoB/WF painting collision surfaces and the Princess’s Secret Slide entrance warp now load their courses; Big Boo's Haunt terrain, Boos, Big Boo, flames and crates are loadable with an event-gated star, though most of its objects are not ported; other painting destinations, source save-file slots and remaining entrances remain.
5. **Original camera behavior.** Port Lakitu modes, course camera regions, swimming/interior behavior, and scripted camera sequences from `camera.c`. The current custom orbit camera exposes Lakitu/close/top-down/side presets an auto-follow yaw after manual input expires, plus boss-focus, underwater, slide, interior and flight distance/pitch contexts but is not yet an implementation of `camera.c`.
6. **Music and complete sound behavior.** Support original sequences and instrument playback, spatial sounds, surface footsteps, and source-timed action sounds. Extracted WAV samples are only part of the audio system; refer to `src/audio/` and the game sound integration. Preserve the user's mute preference. *Source-derived note loops for grass/castle/slide are in; full instrument sequencing remains.*
7. **Rendering and environmental effects.** Support animated water/moving textures, coin animation, dust, splashes, particles, native shadows, and environmental effects. Check GeoLayout switch cases, texture-coordinate conventions, transparency, and source material state before approximating effects. **Status:** native shadows, coin animation, dust/sparks, water splash particles, an underwater fog pass and wave-animated translucent water surfaces are implemented; source water textures and the remaining source particle sets are pending.
8. **Special mechanics.** Cannons, Koopa shells, Wing/Metal/Vanish Caps, flight, and metal-cap underwater walking. Pull forward any prerequisite required by the first complete course. *Wing-cap flight (`Y`), Metal Cap (`U`) and Vanish Cap (`I`) prototypes are in; source takeoff animations remain; vanish now suppresses enemy aggro and metal reduces knockback; the BOB cannon barrel now launches Mario into flight and a sliding Koopa shell can be ridden and jumped off.*

## First end-to-end target: Bob-omb Battlefield

Use one faithful course to validate the shared systems before broadening level coverage:

- Bob-ombs, carrying/throwing, explosions, crates, and rewards.
- Chain Chomp, stakes, gate, rolling balls, and their interactions.
- Cannons and the required flight mechanics.
- King Bob-omb and Koopa the Quick, including dialogue and encounter/race state.
- Original mission selection, mission-specific spawns, red coins, coin totals, and star conditions.
- Star collection, castle return, re-entry, and persistent progress. *Per-act stars, Act 4 red coins, castle return from the win overlay and the Koopa race HUD are in.*

Completion means the original course objectives work through gameplay, not merely that their models appear.

## Implementation and verification

- Read the corresponding local decomp routines before implementing each behavior. Record deliberate approximations rather than silently substituting custom rules.
- Keep simulation state independent of Three.js presentation; use source action IDs, constants, and timing where practical.
- Add focused tests for state transitions, hitbox interactions, timing, damage, and persistence. Compare representative sequences with the reference behavior.
- Validate visuals and gameplay in-browser, including course switching and return visits; keep browser audio muted unless requested otherwise.
- Run tests, typecheck, production build, and asset coverage validation before publishing. Do not describe a Vite-only build as passing the full typechecked build.
- Extract and retain the complete asset library, not just assets currently referenced by playable courses.

## Next implementation

Continue object interactions with the rest of Bob-omb Battlefield: Chain Chomp and stakes, the
chain gate and purple switch, cannons, and the remaining grabbables/damage behaviors. After that,
move to the remaining Mario actions and animation timing. No course editor, ghost races, or other
original concepts until the core fidelity work above is sufficiently complete.
