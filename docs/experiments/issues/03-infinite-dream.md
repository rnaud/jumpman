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

- [x] Reproduce world generation from a seed.
- [x] Stream chunks in and retire distant chunks with bounded resource usage.
- [x] Guarantee connected playable routes using measured Mario jump capabilities. (Reachability test uses 240-unit edge gaps and bounded height change.)
- [x] Add difficulty progression and biome/architecture variations. (Palette/height bands, towers and bridges; further architecture polish remains.)
- [x] Deliver the signature interaction within 30 seconds of launch. (Title-screen INFINITE DREAM button.)
