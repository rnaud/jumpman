## 02 — Mario vs. God

Interactive world manipulation

Play Mario normally while modifying the world around him using the mouse. Move platforms, resize enemies, create ramps, change gravity, and manipulate obstacles in real time.

**Signature moment:** Mario leaps toward a platform, but you drag it away mid-jump. You then create a bridge beneath him just before he falls.

**Mechanics:** Object picking and dragging, live geometry manipulation, gravity controls, object spawning, undo/reset.

**Technical challenge:** Keeping physics and collision surfaces synchronized with modified geometry.

## Constraints and sequencing

Browser-based, desktop-first; no VR/XR, LLMs, or external APIs. Reuse Jumpman's physics, inputs, animation, and assets. This is a future experiment after the current engine-fidelity milestone.

Development order: **2 of 4**. Estimated complexity: **Medium**. Add a runtime world manipulation layer that can later be reused for procedural generation.

## Acceptance criteria

- [x] Pick, move, scale, spawn, and remove supported world objects with the mouse. (Platforms and selected actors/coins scale; spawned actors/coins can be removed by returning to a fresh course/run.)
- [x] Update collision with geometry changes, including platforms beneath Mario.
- [x] Expose gravity controls and undo/reset.
- [x] Deliver the signature interaction within 30 seconds of launch. (Title-screen GOD MODE button.)
