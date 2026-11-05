## 01 — Mario Across Dimensions

Perspective-shifting platformer

Switch between 3D, 2D side-scrolling, and top-down perspectives while playing the same level. Each mode changes not just the camera but how Mario moves and interacts with the environment.

**Signature moment:** Mario is running across Bob-omb Battlefield in 3D. You press a button, and the entire world becomes a 2D platformer. Another press transforms it into a top-down adventure.

**Mechanics:** Instant camera transitions, axis-constrained movement, mode-specific controls, persistent world state.

**Technical challenge:** Mapping 3D collision geometry into playable 2D planes without trapping Mario or breaking existing physics.

## Constraints and sequencing

Browser-based, desktop-first; no VR/XR, LLMs, or external APIs. Reuse Jumpman's physics, inputs, animation, and assets. This is a future experiment after the current engine-fidelity milestone.

Development order: **1 of 4**. Estimated complexity: **Low–medium**. Build the camera and movement abstractions first. This is the quickest visually distinctive prototype.

## Acceptance criteria

- [ ] Switch between 3D, side-scrolling, and top-down during a single run.
- [ ] Adapt movement constraints and controls to each perspective; preserve world state.
- [ ] Handle transitions near walls, edges, and airborne states without trapping Mario.
- [ ] Deliver the signature interaction within 30 seconds of launch.
