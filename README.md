# Jumpman

Porting priorities and the next implementation milestone: [SM64 fidelity roadmap](docs/SM64-PORTING-PLAN.md).

[Play Jumpman](https://jumpman.alexandria-rnaud.workers.dev) — password-protected Cloudflare Worker.

**Super Mario 64's controller and physics, ported from the decompilation into Three.js.**

Jumpman is a small, playable Three.js playground built around a faithful re-implementation of
Mario's movement code from [n64decomp/sm64](https://github.com/n64decomp/sm64). The analog-stick
input pipeline, action state machine, 30 Hz quarter-step integration, slope handling and triangle
collision are ported; the camera and game shell run in Three.js and Web Audio. Bob-ombs and
breakable boxes use source-derived hitboxes, carrying, throwing, explosions, coin rewards and
respawns.

With the exported SM64 library in `assets/`, Jumpman opens in **Bob-omb Battlefield**
with the original **Mario model and 209 native animation clips**. Press **V** to
swap to the procedural character. The original movement playground and CC0 rig
remain available.

![Mario in Bob-omb Battlefield](screenshots/23-sm64-battlefield.png)

```bash
pnpm install
pnpm dev          # http://127.0.0.1:5173
pnpm test         # physics, collision, object interactions, assets, and authentication tests
pnpm build        # typecheck + production bundle
```

The copied `assets/` library is loaded directly as GLB; Jumpman does not run an emulator
or extract a ROM. Without that library, startup
falls back to the procedural movement playground and character. Audio uses the locally converted ROM samples when available, with synthesized fallbacks while samples load.


Future game modes: [Jumpman Experiments brief](docs/experiments/BRIEF.md).

## Local SM64 asset integration

The reproducible exporter is `scripts/export-sm64.py` (Python standard library only).
Place the US decomp source in `reference/sm64` and your matching ROM at
`reference/sm64/baserom.us.z64`, then run:

```bash
(cd reference/sm64 && python3 extract_assets.py us)
python3 scripts/export-courses.py
python3 scripts/export-sm64.py
python3 scripts/export-audio.py
python3 scripts/check-sm64-library.py
```

The default exports **all** actor/level GeoLayouts, additional switch-case variants,
embedded PNG textures, and compatible native animation tracks—not only the current
courses. `--required-only` is available for a quick development export.
`assets/extracted/` preserves every extracted US ROM resource, while
`assets/animations.json` and `assets/layouts.json` preserve every source animation
table and layout command. `assets/export-report.json` records coverage and exceptions.
The ROM, extracted resources and generated GLBs remain gitignored.

GLBs select the first switch case and nearest LOD by default; alternative switch
cases are separate models under each actor/level's `variants/` folder. Mario's eye
alternatives are also retained for the runtime blink callback. Procedural callbacks
(water rendering, shadows, skyboxes, face deformation, etc.) are preserved as node
metadata, not baked geometry or executable behavior. Geometryless layouts are
explicitly marked. Source animation/skeleton mismatches are recorded in the report;
only available joint tracks are attached, and the full original table is retained.
This is an asset conversion pipeline, not an emulation of the N64 graphics processor.

Run `pnpm export:courses` with the local `reference/sm64` checkout available to generate native collision and placement JSON into `assets/courses/`. Then run `pnpm dev`:

- `/` or `?level=bob`: Bob-omb Battlefield with exported Mario.
- `?level=castle_grounds`: Castle Grounds exploration.
- `?level=wf`: Whomp’s Fortress exploration with timed moving platforms.
- `?level=pss`: Princess’s Secret Slide, with a finish star and optional coins.
- `?level=playground`: the original movement course with exported Mario.
- `?level=playground&character=procedural`: start with the procedural character.
- `?level=playground&character=cc0`: use the bundled Quaternius character.
- `?character=local` and custom `?character=/models/example.glb&clips=...` still work.

The start screen and **Pause** menu link to all six destinations. Each uses the same movement controls, victory, restart, and character switching. The three new exploration courses have an immediately available goal star; they do not reproduce the original star missions. The
battlefield challenge has 13 placed coins and a star on the mountain summit.
Its trees, coins, star, Goombas, and tilting seesaw use the exported library. Trees use their original static placements.

`src/game/sm64Assets.ts` indexes every copied actor/level GLB. Use
`loadSm64Asset(relativePath)` and `instantiateSm64(gltf)` to place another model;
`sm64AssetPaths` lists the available paths. Vite supplies URLs in development
and emits the library as separate files in production. Only requested GLBs are
downloaded, and textures remain embedded. A production build includes these
local assets, so its `dist/` is a private build when the library is present.

SM64’s export scale is reversed from metadata (0.01 meters per source unit →
100×) for compatibility with the physics. Mario uses a dedicated native clip
map instead of generic humanoid fitting or additional flip rotations. Exported
materials are retained, wing-cap-only geometry is hidden, and billboard objects
face the current camera.

This is a playable asset integration, not a full port of the level behaviors.
Imported courses use original collision triangles and surface classifications generated from the local reference source. Static object collision uses the corresponding native collision files where available. The playground retains its procedural collision. Moving platforms carry Mario and rebuild their collision each physics tick. Goombas chase, hit Mario, and can be stomped; the coins/star are an authored Jumpman objective.
Castle entrance/exit door warps are implemented. Original boss fights, cannons, other warps, and procedural effects are not implemented.

Validation:

```bash
pnpm test                  # includes asset integration tests when assets/ exists
pnpm build
# With the dev server running (Chrome required):
URL=http://127.0.0.1:5173/ node scripts/sm64-smoke.mjs
```


| Triple jump | Wall kick |
| --- | --- |
| ![Triple jump](screenshots/06-triple-jump.png) | ![Wall kick](screenshots/09-wallkick.png) |

| Dive | Course clear |
| --- | --- |
| ![Dive](screenshots/07-dive.png) | ![Course clear](screenshots/10-win.png) |

| Procedural Mario | CC0 glTF rig |
| --- | --- |
| ![Procedural Mario](screenshots/22-procedural-front.png) | ![CC0 rig](screenshots/12-cc0-idle.png) |

---

## Display settings

Press **Esc** during play to pause, switch courses, or adjust display settings. Press **Esc** again to resume.
Course links and castle doors replace the level inside the running engine without reloading the page. Preferences persist on this device across course changes and reloads.

- **Resolution:** 320×240, 640×480, or native. Widescreen preserves the selected height and adapts the width without stretching.
- **Aspect ratio:** centered 4:3 with borders, or fit the window.
- **Classic preset:** 640×480, 4:3, simple lighting, blob shadow, 45° field of view, and smooth movement between 30 Hz physics ticks. Choose **30 Hz steps** to render on physics ticks.
- **Modern preset:** native resolution, fit-window framing, filmic lighting and dynamic shadows, smooth movement interpolation.
- **Upscaling:** soft or sharp pixels. These are approximations, not emulation of the N64 video pipeline. These options do not change the audio or physics.

## Cloudflare Worker deployment

`worker/index.ts` authenticates every request before forwarding it to the static asset binding.
`run_worker_first: true` is required: model URLs, JavaScript, and HTML all pass through the gate.
The password and a random session-signing secret are Cloudflare secrets, never Vite variables.
Sessions expire after 24 hours and use Secure, HttpOnly, SameSite=Strict cookies. Changing either
secret invalidates existing sessions. Login attempts are rate limited per IP at the edge.

```bash
pnpm exec wrangler login
pnpm export:courses                # local reference checkout required
pnpm exec wrangler secret put SITE_PASSWORD
pnpm exec wrangler secret put SESSION_SECRET  # use a random high-entropy value
pnpm deploy
```

For a first deployment, secrets can instead be supplied together using Wrangler's
`--secrets-file` option with a local, gitignored JSON file. Missing secrets fail closed.

The [Deploy Worker](.github/workflows/deploy.yml) workflow runs on pushes to `main` when the
repository has `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` secrets; without them it reports
that it skipped and leaves the current deployment alone. It uses the checked-in native assets and
the same validation/build steps as `pnpm deploy`.
For local testing, put both values in gitignored `.dev.vars`, run `pnpm build`, then
`pnpm worker:dev`. Verify with `SITE_PASSWORD=your-password node scripts/worker-smoke.mjs`.

The native GLBs and generated course JSON are checked in with the project. Deployment uploads the
built assets to the password-protected Worker; `pnpm deploy` checks for all five native maps before
building to prevent accidentally replacing the deployed game with the fallback playground.

Browser regression checks (Chrome required):

```bash
URL=http://127.0.0.1:5173/ node scripts/courses-smoke.mjs
URL=http://127.0.0.1:8787 SITE_PASSWORD=your-password node scripts/worker-smoke.mjs
```

## Controls

| Input | Action |
| --- | --- |
| `W A S D` | Move (camera-relative analog stick) |
| `Space` | Jump / double jump / triple jump — **hold for height** |
| `Shift` or `J` | Punch; grab a Bob-omb or small crate; throw while holding; dive or kick in the air |
| `Ctrl` or `K` | Crouch; ground pound in the air |
| `Alt` | Walk (half stick, useful for precision) |
| Mouse drag | Orbit camera |
| Mouse wheel | Zoom |
| `Q` / `E`, arrow keys | Orbit camera with the keyboard |
| `C` | Snap camera behind Mario |
| `V` | Swap the loaded character and the procedural Mario rig |
| `R` | Restart the run |
| `F3` / `` ` `` | Toggle the debug panel |
| `G` | Toggle collision wireframe |
| `F` | Fullscreen |

Gamepad: left stick moves, right stick orbits, `A` jumps, `B` punches, `X`/`Y`/triggers crouch and
ground pound.

### The moves

The port deliberately reproduces the whole common movement vocabulary:

- walk → run with analog acceleration and camera-relative turning
- single, double and triple jump (the triple requires speed on the double-jump landing)
- short hop by tapping A; full height by holding A
- backflip (crouch, then A), side flip (turn around, then A)
- long jump (crouch-slide while running, then A)
- dive (B at speed), jump kick, ground pound
- wall kick (A within the two-frame window after an air wall bonk)
- ledge grab and fast ledge climb
- grab, carry, jump with, throw and place Bob-ombs and small crates
- butt slide, stomach slide, dive slide, crouch slide, crawl
- slope acceleration, slope sliding, steep-floor push-off, quicksand-style surface classes
- friction differences for normal, slippery, very slippery and ice surfaces

---

## The course

The playground is built to exercise the movement system:

- **Jump Garden** — four ascending platforms and an arch for jump-height comparisons.
- **Wall Kick Canyon** — a 270-unit corridor and a 520-unit end wall for wall-kick chains.
- **Butt-Slide Rink** — ice ramp, ice sheet and a chute for slides.
- **Long Jump Gap** — a 350-unit gap with distance markers.
- **Star Tower** — a spiral of platforms around a 760-unit pillar; the star waits on top.
- **Stair & Pyramid** — 40-unit steps and a climbable four-tier pyramid.
- **Slope Lab** — 20°, 30°, 45° and 60° ramps with different surface classes.
- 13 coins and a star; collecting every coin wakes the star.

---

## Characters and animation

### Procedural Mario (fallback)

The fallback rig is a
**hand-keyed original**: a low-poly Mario with hip/knee/foot, shoulder/elbow/hand, torso, head and cap
joints. Poses are selected from the action ID and smoothed with exponential damping. The run cycle
is driven by actual speed, jumps blend takeoff/rise/fall, flips use the action timer, and landings
squash and recover. It has no external dependencies and weighs nothing at runtime.

This fallback uses controller timing with hand-authored keyframes. The native SM64 view instead
uses the animation clips in the locally supplied export.

### CC0 animated rig (`?character=cc0`)

The optional rig is [Quaternius' Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html),
released under **CC0 1.0**. The bundled GLB contains a rigged mannequin and 46 clips including idle,
walk, jog, sprint, jump start/loop/land, roll, crouch, punch and interact. It is loaded with
`GLTFLoader`, coloured red/blue to fit the scene, scaled to SM64's 160-unit height and driven through
an `AnimationMixer` with crossfades.

- Asset: `public/models/quaternius/AnimationLibrary.glb`
- Notice: `public/models/quaternius/LICENSE.txt`
- Loader and clip map: [GltfCharacterView.ts](src/game/GltfCharacterView.ts)
- Select `?character=cc0` to load this model; **V** swaps it with the procedural rig.

### Bring your own rig (option 1)

The CC0 rig is a ready-made demo, but the loader is generic. Put any compatible glTF/GLB under
`public/models/` and open:

```
http://127.0.0.1:5173/?character=/models/my-character.glb
```

The loader scales it to 160 units, centres it and drops its feet onto the root origin. If its clip
names match the aliases in `CLIP_ALIASES`, everything works with no code change; otherwise edit that
map. [public/models/README.md](public/models/README.md) lists the clip names.

The value of this path is that it separates **controller/physics** from **character art**. You can
commission a character and animations, use a different art style, or drop in a rig you already own
without touching the simulation. The procedural rig gives you a zero-asset Mario silhouette; the CC0
rig proves the animation pipeline end to end; the custom URL is the same pipeline pointed at your
own art.

### Local-only assets (gitignored)

For a private setup, keep the source material and converted files out of version control:

- `rom/`, `extracted/`, `local-assets/` and common ROM extensions are already gitignored;
- `public/models/local/` is the local character slot — everything except its README is ignored;
- put a `character.glb` there and open `http://127.0.0.1:5173/?character=local`;
- press **V** to swap between the procedural Mario and the local rig.

The loader starts from a glTF/GLB. It does **not** extract or convert assets from a ROM; that step is
intentionally outside this project. See [public/models/local/README.md](public/models/local/README.md).
A production build copies `public/` into `dist/`, so remove private files before sharing a build.

## Fidelity: what is actually ported

The reference is `n64decomp/sm64` at commit `9921382` (August 2023).

### Input

`MarioRuntime.updateMarioInputs` follows `update_mario_inputs` in `src/game/mario.c`:

1. clear `input`, reset particle flags and the high byte of `flags`
2. read A/B/Z edges and A/Z held state
3. convert the stick to `intendedMag = (stickMag / 64)² · 64 / 2` (max 32)
4. compute `intendedYaw = atan2s(-stickY, stickX) + cameraYaw`
5. run wall collisions, then find the floor, ceiling and floor angle
6. set `INPUT_NONZERO_ANALOG`, `INPUT_A_PRESSED`, `INPUT_ABOVE_SLIDE`, `INPUT_OFF_FLOOR`, `INPUT_UNKNOWN_5`, and decrement the double-jump and wall-kick timers

The camera yaw convention is SM64's: **yaw is the direction from Mario to the camera**, so pressing
"up" produces `cameraYaw + 0x8000`, i.e. away from the camera. A browser test rotates the camera
90° and verifies that Mario's facing matches `cameraYaw + 0x8000`.

### Action state machine

`set_mario_action` is split across `MarioRuntime` and `src/sim/actions/`:

| Action group | Source | Port |
| --- | --- | --- |
| Stationary | `mario_actions_stationary.c` | `actions/stationary.ts` |
| Moving | `mario_actions_moving.c` | `actions/moving.ts` |
| Airborne | `mario_actions_airborne.c` | `actions/airborne.ts` |
| Automatic (ledges) | `mario_actions_automatic.c` | `actions/automatic.ts` |
| Object (punch) | `mario_actions_object.c` | `actions/object.ts` |

The runtime's action loop matches `execute_mario_action`: when an action handler returns "cancel",
the new action executes in the **same 30 Hz frame**. This is why landing cancels, jump chains and
ledge transitions happen without an extra frame of delay.

The airborne transition velocities are verbatim:

| Action | Vertical velocity | Other |
| --- | --- | --- |
| Jump | `42 + forwardVel · 0.25` | `forwardVel · 0.8` |
| Double jump | `52 + forwardVel · 0.25` | `forwardVel · 0.8` |
| Triple jump | `69` | `forwardVel · 0.8` |
| Backflip | `62` | `forwardVel = -16` |
| Side flip | `62` | `forwardVel = 8`, face `intendedYaw` |
| Wall kick | `62` | minimum forward speed 24 |
| Long jump | `30` | `forwardVel · 1.5`, capped at 48 |
| Dive (speed kick) | `+20` | forward speed capped at 48 |
| Jump kick | `20` | |

### Integration

`src/sim/marioStep.ts` ports `perform_ground_step` and `perform_air_step`:

- **Four quarter-steps per frame** (`vel / 4` horizontally, with ground movement scaled by
  `floor.normal.y`).
- Walls are resolved **before** floor height, at two offsets/radii: `(30, 24)` for the lower body
  and `(60, 50)` for the upper body on the ground, and `(30, 50)` / `(150, 50)` in the air.
- A floor within 100 units above Mario is stepped onto; higher ledges are walls.
- `find_floor` has the original **78-unit buffer** and returns the first matching triangle in
  SM64's surface order, not necessarily the highest floor.
- Gravity is `-4` per frame with a terminal velocity of `-75`; long jumps and slide kicks use
  `-2`; the variable-jump-height path (`MARIO_UNKNOWN_08` + missing A + rising faster than 20)
  divides vertical velocity by 4, exactly as the decompilation does.
- Air control uses the same `0.35` drag, `1.5 · cos(dYaw)` forward acceleration and
  `10 · sin(dYaw)` sideways speed.

The resulting jump apex on flat ground is **242 units** with A held (the tests assert 235–250), and
a tapped short hop is under 100 units.

### Collision

`src/sim/collision.ts` mirrors `src/engine/surface_collision.c`:

- triangles are classified as floors (`ny > 0.01`), ceilings (`ny < -0.01`) or walls
- floors are sorted highest-first by `vertex1.y`; ceilings lowest-first; walls keep insertion order
- walls use the same projected point-in-triangle tests and the `X_PROJECTION` flag
- wall push is `radius - (normal · point + originOffset)` along the normal
- the 1024-unit spatial cells include a **50-unit border buffer**, matching
  `lower_cell_index` / `upper_cell_index`. This is required for wall collision radii at cell
  boundaries and was the source of a real bug during development.
- all collision is extracted from the rendered meshes, so visual and physical geometry cannot drift

---

## Interactive objects

Bob-omb Battlefield now spawns the 12 macro-placed Bob-ombs, three large crates and two small
crates from the decompilation's `macro.inc.c`. The object layer follows the source structure:

- `src/sim/object.ts` ports `object_step`, hitboxes, held states and interaction status flags.
- `src/sim/objectManager.ts` runs the object list, Mario interaction pass and explosion chain.
- `src/sim/behaviors/` ports `bobomb.inc.c`, `breakable_box.inc.c` and
  `breakable_box_small.inc.c`.
- Press `Shift`/`J` near a Bob-omb to punch it and pick it up. Press it again to throw. Holding a
  lit Bob-omb for too long drops and explodes it. Thrown Bob-ombs explode on landing and spawn a
  coin. Large crates break on a punch or ground pound; the three-coin crate drops its reward.
  Small crates can be carried and break when thrown into a wall.
- Dynamic coins are collectible but do not increase the course's 13-coin star requirement.

Approximations are recorded here rather than hidden: Bob-ombs are invisible while held in the
original, but Jumpman shows the model just in front of Mario because the articulated model has no
hand attachment. Object-versus-object collision is limited to explosion chains. Large crates push
Mario out horizontally but do not yet act as standable floors. Coin and star rewards still use the
existing Jumpman objective layer rather than the original mission script.

## What is not ported

The goal was the controller and movement physics, not a full game. Omissions and approximations:

- **Object interactions are partial.** Bob-ombs and breakable boxes are implemented; Chain Chomps,
  cannons, gates/switches, Koopa shells, poles, other grabbables and most enemy hitboxes are not.
  Coins and the star still use the authored Jumpman objective layer.
- **No metal-cap underwater walking, quicksand death, lava, caps, wing cap flight, or save files.**
  Surface types for ice/slippery/slow are supported; the rest are constants only.
- **Damage and death are partial.** Health is now a source-style 8-wedge meter with hurt/heal
  counters, damage amounts, invincibility during knockback, soft/normal/hard knockback and coin
  healing. Death cutscenes and source death animations are still replaced by an immediate respawn;
  lava, quicksand and poison-gas damage are not implemented yet.
- **No SM64 animation data.** Mario animations live in the ROM, not the decompilation repository.
  With local assets, the native Mario view plays the exported original clips.
  The procedural and CC0 fallback rigs still approximate those poses.
- **JavaScript numbers are f64.** The original uses f32 arithmetic. The algorithms and constants are
  the same, and the tests pin the important results (242-unit jump apex, 32-unit run speed), but
  individual frames are not bit-identical to an N64.
- **Simplified actions**: the punch combo is timer-driven instead of animation-frame driven;
  forward/backward rollouts and the slow ledge climb are omitted (ledge grab and fast climb are
  implemented); the air wall-bonk path maps backward knockback to the soft-bonk state; quicksand
  physics is inert.
- **The camera is not Lakitu.** It is a custom orbit camera, but it feeds the same yaw value into
  `update_mario_joystick_inputs`.

---

## Architecture

```
src/
  sim/                         pure TypeScript, no Three.js
    math.ts                    s16 angles, sins/coss, atan2s, approach
    state.ts                   action IDs/flags, input flags, Mario state
    surfaces.ts                SM64 surface types and classes
    collision.ts               triangle partition, find_floor/find_ceil/walls
    marioHelpers.ts            floor class, slope/slippery/steep, slope probes
    marioStep.ts               ground/air quarter steps, gravity, ledge grab
    MarioRuntime.ts            input pipeline, action transitions, 30 Hz loop
    object.ts                  object_step, hitboxes, held/interaction state
    objectManager.ts           object list, Mario interactions, explosions
    behaviors/                 bobomb, breakable box behaviors
    actions/                   stationary, moving, airborne, automatic, object, holding
  game/
    Game.ts                    render loop, fixed-step accumulator, coins/star
    world.ts                   course geometry, collision extraction, textures
    CharacterView.ts           shared render-character interface
    MarioModel.ts              hand-keyed procedural Mario rig
    GltfCharacterView.ts       CC0/custom glTF loader, mixer, action→clip map
    CameraRig.ts               orbit camera + camera collision
    Input.ts                   keyboard, mouse, gamepad, latched button edges
    Particles.ts               dust/spark sprite pool
    Sfx.ts                     synthesized Web Audio effects
    HUD.ts                     DOM HUD and debug panel
public/models/
  quaternius/                  CC0 animation library + license
  README.md                    how to use a custom glTF rig
tests/                         Vitest simulation tests
scripts/                       Puppeteer smoke/screenshot scripts
```

Physics never imports Three.js. The renderer consumes `MarioRuntime` and interpolates between the
last two 30 Hz states, so simulation stays deterministic while rendering runs at monitor refresh
rate.

### Run loop

```
render dt → accumulator
  while accumulator >= 1/30:
    controller = input.consumeStep()   // latched key/gamepad edges
    objects.step(mario)                // behaviors + Mario interaction targets
    mario.step(controller, camera.yawS16)
    objects.afterMarioStep(mario)      // held-object attachment
    process object events, coins, star
  render interpolation α = accumulator / (1/30)
```

Button presses are latched until the next physics step, so a tap between frames is never lost.

---

## Tests

`pnpm test` runs 92 deterministic simulation and asset tests:

- cardinal `atan2s` directions and s16 wrapping
- flat floors, box tops, 45° ramps, 78-unit floor buffer, wall push-out, surface ordering
- run speed converging on 32
- full jump apex ≈ 242 units, short hop < 100
- jump → double → triple chain
- backflip out of crouch, side flip out of a turn
- long jump from a crouch slide, dive at speed
- ground pound landing
- wall kick after an air bonk
- ledge grab + auto climb
- ice momentum and the very-slippery floor class
- object hitbox overlap, object gravity, grab/carry/throw, Bob-omb explosions and crate breaks
- health drain, knockback strength/direction, invincibility, healing and respawn
- camera-relative movement is covered by a browser script rather than Vitest

Browser verification scripts (require `pnpm dev` on port 5173 and a local Chrome; the scripts
default to the standard macOS Chrome path, so adjust `executablePath` if yours differs):

```bash
node scripts/smoke.mjs      # boot, walk, jump, screenshots
node scripts/camera.mjs     # mouse orbit + camera-relative movement
node scripts/play3.mjs      # dive, backflip, wall kick
node scripts/win.mjs        # all 13 coins + star + win overlay
node scripts/zones.mjs      # screenshots of every zone
node scripts/characters.mjs # loads the CC0 rig and checks V toggle + clip changes
node scripts/object-smoke.mjs # grab/carry/throw a Bob-omb, explode it, break a crate
```

Most screenshots in this README were produced by those scripts.

---

## Adding to the world

`src/game/world.ts` has `addBox`, `addRamp`, `addLabel`, `addCoin` and a `register` helper. Meshes
passed through `register` get:

1. added to the scene with shadows,
2. their world-space triangles extracted into the SM64 collision world,
3. added to the camera-blocker list.

Surface type is the fifth argument to `register` (or the `type` option to `addTriangle`); use the
constants in `src/sim/surfaces.ts` (`SURFACE_ICE`, `SURFACE_SLIPPERY`, …). Keep the cell border
buffer and surface sorting in mind if you add large triangles near cell boundaries.

---

## Credits and legal

- Physics, action IDs and input logic are ported from
  [n64decomp/sm64](https://github.com/n64decomp/sm64), which is released under **CC0 1.0**.
- Three.js is © three.js authors, MIT licensed.
- [Quaternius' Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html)
  is © Quaternius, released under CC0 1.0. The bundled file and notice are in
  `public/models/quaternius/`.
- Super Mario 64 and Mario are trademarks of Nintendo. This is a non-commercial fan project; it
  keeps locally supplied ROMs and exported game assets outside version control. The
  copied `assets/` library is not covered by the project’s code license.
- The Jumpman code in this repository is MIT licensed (see `LICENSE`).
- Do not commit a ROM or ROM-extracted assets. `rom/`, `extracted/`, `local-assets/`,
  `public/models/local/` (except its README), common ROM extensions and `private/` are excluded in
  `.gitignore` as a safety net.


## Extracted sound samples

After the ROM extraction, run `pnpm export:audio` to convert **all 219** extracted
AIFF samples to PCM WAV in `assets/audio/`, including unused instruments and voices.
The manifest preserves sample rates and sustain-loop markers. Mario's jump voices,
landings, footsteps, bonks, ground pounds, dives, coins and star celebration use these
samples through Web Audio. The coin sound uses the source four-note pattern.
Effects retain synthesized fallbacks while buffers load or if samples are unavailable.

This ports sample playback and selected sound events, not the complete N64 audio
engine: the `.m64` background-music sequences are preserved in `assets/extracted/`
but are not yet sequenced. Voice pitch bends, full ADSR envelopes and reverb are not
reproduced exactly. Converted audio stays local and gitignored with the other assets.
