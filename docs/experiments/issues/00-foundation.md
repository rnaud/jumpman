# Project Brief: Jumpman Experiments

Four experimental game modes built on top of Jumpman, the Three.js recreation of the Super Mario 64 engine.

## Vision

Transform Super Mario 64 into a playground for emergent gameplay, procedural worlds, and unconventional game mechanics.

Rather than recreating the original game, use its existing movement, physics, animations, and level geometry as the foundation for four distinct experiments.

Core constraints: **Browser-based, desktop-first, no VR/XR, no LLMs, no external APIs.** Everything runs locally using Three.js, JavaScript/TypeScript, deterministic algorithms, and game logic.

These are future experiments. The current engine-fidelity work remains tracked separately in [the SM64 porting plan](../SM64-PORTING-PLAN.md).

## The four experiments

## Shared technical foundation

All four experiments should live within the same Jumpman codebase, sharing the original engine's capabilities.

| System | Purpose |
| --- | --- |
| Game mode framework | Switch between experiments without duplicating the engine |
| Camera controller | Support 3D, side-scrolling, and top-down views |
| Runtime world editor | Spawn, move, scale, and remove objects |
| Agent controller | Drive Mario characters through simulated inputs |
| Procedural geometry | Generate platforms, terrain, and obstacles |
| Debug interface | Inspect physics, navigation, agents, and world state |

Prefer simulation through Jumpman's existing physics and input interfaces rather than rewriting Mario's movement or animation systems.

## Delivery order

- [ ] 1. Mario Across Dimensions — Build the camera and movement abstractions first. This is the quickest visually distinctive prototype.
- [ ] 2. Mario vs. God — Add a runtime world manipulation layer that can later be reused for procedural generation.
- [ ] 3. Mario's Infinite Dream — Build on the runtime geometry system to generate increasingly complex traversable environments.
- [ ] 4. A Hundred Marios — Introduce autonomous controllers, agent behaviors, and scalable simulation.

## Definition of success

Each experiment should produce a compelling, self-contained interaction within 30 seconds of launching it, with minimal UI and no setup.

The goal is not four complete games. It's four small, polished, surprising toys that demonstrate entirely new ways of interacting with Mario 64.

An especially compelling longer-term possibility is combining them: an infinite procedural world populated by autonomous Marios that the player can manipulate, while switching between 3D, 2D, and top-down views.
