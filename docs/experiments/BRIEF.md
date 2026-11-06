# Project Brief: Jumpman Experiments

Four experimental game modes built on top of Jumpman, the Three.js recreation of the Super Mario 64 engine.

## Vision

Transform Super Mario 64 into a playground for emergent gameplay, procedural worlds, and unconventional game mechanics.

Rather than recreating the original game, use its existing movement, physics, animations, and level geometry as the foundation for four distinct experiments.

Core constraints: **Browser-based, desktop-first, no VR/XR, no LLMs, no external APIs.** Everything runs locally using Three.js, JavaScript/TypeScript, deterministic algorithms, and game logic.

These are future experiments. The current engine-fidelity work remains tracked separately in [the SM64 porting plan](../SM64-PORTING-PLAN.md).

## The four experiments

## 01 — Mario Across Dimensions

Perspective-shifting platformer

Switch between 3D, 2D side-scrolling, and top-down perspectives while playing the same level. Each mode changes not just the camera but how Mario moves and interacts with the environment.

**Signature moment:** Mario is running across Bob-omb Battlefield in 3D. You press a button, and the entire world becomes a 2D platformer. Another press transforms it into a top-down adventure.

**Mechanics:** Instant camera transitions, axis-constrained movement, mode-specific controls, persistent world state.

**Technical challenge:** Mapping 3D collision geometry into playable 2D planes without trapping Mario or breaking existing physics.

## 02 — Mario vs. God

Interactive world manipulation

Play Mario normally while modifying the world around him using the mouse. Move platforms, resize enemies, create ramps, change gravity, and manipulate obstacles in real time.

**Signature moment:** Mario leaps toward a platform, but you drag it away mid-jump. You then create a bridge beneath him just before he falls.

**Mechanics:** Object picking and dragging, live geometry manipulation, gravity controls, object spawning, undo/reset.

**Technical challenge:** Keeping physics and collision surfaces synchronized with modified geometry.

## 03 — A Hundred Marios

Emergent behavior simulation

Populate a level with dozens or hundreds of autonomous Marios, each following a simple set of rules and goals. Watch them explore, chase, flee, race, and interact.

**Signature moment:** One hundred Marios flood Bob-omb Battlefield. Some race toward the summit, others chase coins, and some follow the crowd or fight over territory.

**Mechanics:** Personality presets, autonomous movement, goal selection, crowd avoidance, simulation speed, agent inspector.

**Technical challenge:** Efficiently simulating many characters while preserving the feel of Mario 64 movement. Simplified physics or collision may be necessary for high agent counts.

## 04 — Mario's Infinite Dream

Procedural infinite platformer

Generate an endless Mario 64-style world as the player explores. Floating islands, towers, ramps, bridges, and surreal structures appear continuously, with increasingly unusual architecture.

**Signature moment:** Mario runs across an endless landscape of floating islands. As he progresses, familiar Mushroom Kingdom architecture gives way to impossible towers, spiral bridges, and gravity-defying structures.

**Mechanics:** Seeded generation, streaming world chunks, procedural platform placement, difficulty progression, biome variations.

**Technical challenge:** Guaranteeing traversability while producing interesting levels. Use connectivity constraints and reachability tests based on Mario's actual jump capabilities.

## 05 — Diorama (new concept from the user)

Poly-pocket diorama. A miniature real-asset board is viewed from far away with a free-fly god camera. Real Marios and Goombas move autonomously; clicking one makes it jump or chase. See [the diorama issue](issues/05-diorama.md).

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

## Proposed development order

1. **Mario Across Dimensions** — Low–medium complexity. Build the camera and movement abstractions first. This is the quickest visually distinctive prototype.
2. **Mario vs. God** — Medium complexity. Add a runtime world manipulation layer that can later be reused for procedural generation.
3. **Mario's Infinite Dream** — Medium–high complexity. Build on the runtime geometry system to generate increasingly complex traversable environments.
4. **A Hundred Marios** — High complexity. Introduce autonomous controllers, agent behaviors, and scalable simulation.

## Definition of success

Each experiment should produce a compelling, self-contained interaction within 30 seconds of launching it, with minimal UI and no setup.

The goal is not four complete games. It's four small, polished, surprising toys that demonstrate entirely new ways of interacting with Mario 64.

An especially compelling longer-term possibility is combining them: an infinite procedural world populated by autonomous Marios that the player can manipulate, while switching between 3D, 2D, and top-down views.
