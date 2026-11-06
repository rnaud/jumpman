## 03 — Endless Run

Replaces *Mario's Infinite Dream*. A seeded endless track that Mario runs down using his real SM64 physics.

**Signature moment:** Mario sprints down a sky track. He leaps a gap, clears a hurdle and weaves between pillars. Grass gives way to brick fortresses and then a rainbow sky.

## Acceptance criteria

- [x] Mario auto-runs at full speed along +X; the player steers (A/D) and keeps every button (jump, long jump, dive).
- [x] Seeded (`?runSeed=`), continuous track of segments: runs, gaps, hurdles, pillar slaloms, narrow bridges, stairs, split lanes and island hops. Segments stream in ahead and retire behind, on their own collision world.
- [x] Every gap and step is clamped to a fraction of Mario's measured running jump (841 far, 335 high). `tests/runnerGen.test.ts` checks 40 seeds.
- [x] Difficulty grows with distance. Three themes (Grass Run → Brick Fortress → Rainbow Sky) change textures, side decor, sky, fog and music.
- [x] A fall ends the run and shows the distance, coins and best (saved). Space starts a fresh seed.
