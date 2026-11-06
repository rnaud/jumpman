## 07 — Shrink Ray

Same Mario, same SM64 physics — a different world size.

- [x] Sizes: Tiny (world ×8), Small (×3), Normal and Giant (×¼), on keys 1–4.
- [x] The experiment builds its own copy of the course (level geometry and scenery) and a collision world scaled to match, far from the original, so the hidden course objects never meet Mario.
- [x] Resizing scales the world smoothly around Mario, then rebuilds collision. Camera range, fog and sky follow the scale.
- [x] Per-world ceiling limit (`CollisionWorld.ceilingLimit`), so a ×8 mountain isn't capped by SM64's 20000-unit "no ceiling" height.
