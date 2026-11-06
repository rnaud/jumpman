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

- [x] Switch 3D → 2D side → top-down at any time on Bob-omb Battlefield (Tab, or 1/2/3). Each switch is a dolly zoom: the lens narrows to 7° while the camera pulls back, so the world flattens without a cut.
- [x] 2D is a real cross-section: geometry between the camera and Mario's lane is clipped, and the cut face is drawn as earth with a grass edge, sampled from collision floors along the lane.
- [x] Mario keeps his own physics: in 2D the stick runs along the lane (any direction, snapped to 45° from the camera) and drift across it is removed every step; top-down uses screen-relative input and slices away everything above his head.
- [x] Switching mid-jump or next to a wall is safe because Mario never moves when the view changes.
- [x] The authored Mushroom Kingdom 1-1 level opens directly in the 2D view.

## Known limits

- The 2D cut follows the topmost floor near Mario's height; overhangs above the lane are not drawn as separate slices.
