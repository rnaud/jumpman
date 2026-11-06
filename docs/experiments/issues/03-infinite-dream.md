## 03 — Mario's Infinite Dream

Procedural infinite platformer

Generate an endless Mario 64-style world as the player explores. Floating islands, towers, ramps, bridges, and surreal structures appear continuously, with increasingly unusual architecture.

**Signature moment:** Mario runs across an endless landscape of floating islands. As he progresses, familiar Mushroom Kingdom architecture gives way to impossible towers, spiral bridges, and gravity-defying structures.

**Mechanics:** Seeded generation, streaming world chunks, procedural platform placement, difficulty progression, biome variations.

**Technical challenge:** Guaranteeing traversability while producing interesting levels. Use connectivity constraints and reachability tests based on Mario's actual jump capabilities.

## Constraints and sequencing

Browser-based, desktop-first; no VR/XR, LLMs, or external APIs. Reuse Jumpman's physics, inputs, animation, and assets. This is a future experiment after the current engine-fidelity milestone.

Development order: **3 of 4**. Estimated complexity: **Medium–high**. Build on the runtime geometry system to generate increasingly complex traversable environments.

## Acceptance criteria

- [x] Seeded generation (`?dreamSeed=`) of a continuous chain of set pieces: island hops, stairs, stepping pillars, curving plank bridges, spiral stairs around towers, floating checkerboards, tumbling pastel cubes and rainbow ribbons, with checkpoints every four pieces.
- [x] Pieces stream in ahead and retire behind. The dream runs on its own `CollisionWorld` far from the course, so nothing in the hidden course can reach Mario.
- [x] Reachability: every route hop's edge gap and rise is clamped to a fraction of Mario's single jump, measured through `MarioRuntime` (841 units far, 335 high). `tests/dreamGen.test.ts` checks 40 seeds.
- [x] Difficulty rises with distance; three biomes (Mushroom Meadow → Sky Fortress → Dreamscape) change architecture, sky, fog and music, and background decor grows stranger (towers and ruins, then floating doors and upside-down islands).
- [x] Falling wakes Mario on the last route platform he stood on. The HUD shows distance, best distance (saved), biome and coins.
