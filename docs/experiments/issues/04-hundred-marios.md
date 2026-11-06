## 04 — A Hundred Marios

Emergent behavior simulation

Populate a level with dozens or hundreds of autonomous Marios, each following a simple set of rules and goals. Watch them explore, chase, flee, race, and interact.

**Signature moment:** One hundred Marios flood Bob-omb Battlefield. Some race toward the summit, others chase coins, and some follow the crowd or fight over territory.

**Mechanics:** Personality presets, autonomous movement, goal selection, crowd avoidance, simulation speed, agent inspector.

**Technical challenge:** Efficiently simulating many characters while preserving the feel of Mario 64 movement. Simplified physics or collision may be necessary for high agent counts.

## Constraints and sequencing

Browser-based, desktop-first; no VR/XR, LLMs, or external APIs. Reuse Jumpman's physics, inputs, animation, and assets. This is a future experiment after the current engine-fidelity milestone.

Development order: **4 of 4**. Estimated complexity: **High**. Introduce autonomous controllers, agent behaviors, and scalable simulation.

## Acceptance criteria

- [x] Drive autonomous Marios through the existing input/physics interfaces. (Eight agents use `MarioRuntime` controllers; the rest use lightweight sprite physics for crowd scale.)
- [x] Provide personality presets, goal selection, and crowd avoidance.
- [x] Expose simulation speed and an agent inspector.
- [x] Profile dozens through 100 agents; document any physics simplifications and measured performance. (Sprites with simplified ground physics; ~62 FPS headless at 100 agents / 1.25×.)
- [x] Deliver the signature interaction within 30 seconds of launch. (Title-screen 100 MARIOS button; O respawns 100.)
