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

- [x] Play Mario normally while the mouse reshapes the world. Tools 1–7: Grab, Bridge, Block, Ramp, Spring, Bob-omb, Coin.
- [x] Grab drags pieces, Bob-ombs, Koopas, Goombas and coins on the ground plane (Shift lifts vertically). The wheel resizes whatever is hovered and right-click deletes it. A highlight box, drop line and landing shadow show depth while dragging.
- [x] Collision is rebuilt the instant a piece moves; a platform carries Mario if it moves under his feet.
- [x] Signature moment: drag the floating starter blocks away mid-jump, then paint a plank bridge *at Mario's height* so it catches him (verified in the smoke).
- [x] Springs launch Mario into a triple jump. Gravity presets: Moon, Floaty, Normal, Heavy. Slow motion (T) and undo/redo (⌘/Ctrl+Z) are available.
