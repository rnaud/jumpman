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

- [x] 25/50/100 Marios rain in around the player. Each is a full `MarioRuntime` driven only by synthetic stick and button input, rendered with the real SM64 rig and its native animation clips (distant Marios animate at a reduced rate).
- [x] Five colour-coded personalities. Racers run the Koopa race to the summit, using a route re-planned so Mario can actually walk it (grid A* over collision with SM64's slipperiness thresholds). Coin hunters chase respawning coins. Followers flock around you and echo your jumps in a wave. Brawlers guard turf and punch intruders with real knockbacks. Explorers wander and long- and triple-jump.
- [x] Crowd avoidance, wall jumping, re-routing when a goal stops getting closer, and a warp past checkpoints a racer keeps failing.
- [x] Mixes (Mixed, Race day, Gold rush, Parade, Brawl) and sim speed ½×–3×. Click a Mario to inspect its goal, action, speed and stats; Tab watches it; P changes its personality. You can punch the crowd or bounce on their heads.

- [x] Rendering: every body part of every Mario is an instance in one `BatchedMesh` per Mario material, so 100 animated Marios cost about 45 draw calls instead of more than 3,000.

## Known limits

- The racers' planner can't find a walking route over a few summit stretches; racers rely on jumping and the warp there.
