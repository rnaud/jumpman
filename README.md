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

With the exported SM64 library in `assets/`, Jumpman opens on the **Castle Grounds**
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
falls back to the procedural movement playground and character. Music and gameplay effects run through the US audio engine.


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

Run `pnpm export:courses` with the local `reference/sm64` checkout available to generate native collision and placement JSON into `assets/courses/`. The exporter and deploy check skip meshless geo-only decor models. Then run `pnpm dev`:

- `/` or `?level=castle_grounds` starts Mario outside the castle.
- `?level=bob` opens Bob-omb Battlefield directly.
- `?level=wf`: Whomp’s Fortress with six selectable missions. The Whomp King occupies the summit in act 1. Later acts add the tower, sliding steps and rider-operated elevator. A cannon shot breaks the wall concealing star 6.
- `?level=bbh`: Big Boo’s Haunt terrain exploration with 11 Boos, 2 Big Boos, 6 red flame hazards, two crates and four moving platforms; most mansion objects are still unported.
- `?level=ccm`: Cool, Cool Mountain includes the cabin slide and Li’l Penguin Lost. Carry the mountaintop baby to its mother beside the lower cabin for the second star. From act 2 onward the racing penguin waits inside the chimney. Accept with A, beat it down the main slide, and meet it at the bottom for star 3. In act 5, lead the rolling snowball past the checkpoint to its head, then speak to the assembled snowman.
- `?level=jrb`: Jolly Roger Bay includes the sunken ship, its water-drain puzzle and star box (act 1), and the ocean cave treasure chests (act 3).
- `?level=lll`: Lethal Lava Land terrain/lava exploration with five charging Bullies and a Big Bully boss; most fire objects are not ported.
- `?level=ssl`: Shifting Sand Land terrain exploration with two circling Klepto birds and four Pokey cacti; most quicksand features and pyramid objects are not ported.
- `?level=wdw`: Wet-Dry World terrain exploration; the water-level puzzle and most town objects are not ported.
- `?level=sl`: Snowman’s Land terrain exploration with Mr. Blizzard; most snow objects are not ported.
- `?level=ttm`: Tall, Tall Mountain terrain exploration; most mountain objects and the slide are not ported.
- `?level=thi`: Tiny-Huge Island terrain exploration; size-changing pipes and most objects are not ported.
- `?level=ttc`: Tick Tock Clock terrain exploration; clock hands, conveyors and most objects are not ported.
- `?level=rr`: Rainbow Ride terrain exploration; carpets and most objects are not ported.
- `?level=bitdw|bitfs|bits` opens a Bowser obstacle course. Its eight red coins award a star and its exit leads to the corresponding arena. Approach Bowser’s tail, press B to grab, rotate the stick (or cycle W, D, S, A), and press B again to throw him at a mine. The first two arenas each take one hit and award a key. The third takes three hits and awards the Grand Star.
- `?level=pss`: Princess’s Secret Slide, with a finish star and optional coins.
- `?level=mushroom2d`: the authored side-scrolling **Mushroom Kingdom 1-1** experiment level.
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
materials are retained, wing-cap-only geometry is shown while the wing cap is active, and billboard objects
face the current camera.

This is a playable asset integration, not a full port of the level behaviors.
Imported courses use original collision triangles and surface classifications generated from the local reference source. Static object collision uses the corresponding native collision files where available. The playground retains its procedural collision. Moving platforms carry Mario and rebuild their collision each physics tick. Goombas chase, hit Mario, and can be stomped; the coins/star are an authored Jumpman objective.
Castle entrance/exit door warps are implemented. Boss fights, player cannons and course-specific warps are described below.

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

Every push to `main` runs [Deploy Worker](.github/workflows/deploy.yml). GitHub Actions
validates the assets, runs tests, builds the game, and deploys that commit to the existing
`jumpman` Worker. Set `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` in the repository's
Actions secrets. Missing credentials fail the deployment step. After adding a secret, rerun the
failed workflow to publish the same commit.
For local testing, put both values in gitignored `.dev.vars`, run `pnpm build`, then
`pnpm worker:dev`. Verify with `SITE_PASSWORD=your-password node scripts/worker-smoke.mjs`.

The native GLBs and generated course JSON are checked in with the project. Deployment uploads the
built assets to the password-protected Worker; `pnpm deploy` checks the exported native maps before
building to prevent accidentally replacing the deployed game with the fallback playground.

Browser regression checks (Chrome required):

```bash
URL=http://127.0.0.1:5173/ node scripts/courses-smoke.mjs
URL=http://127.0.0.1:5173/ node scripts/chests-smoke.mjs
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
- ledge grab and fast/slow ledge climb
- hangable-ceiling grab, hand-over-hand movement and drop
- tree-pole grab, climbing, top-of-pole and jump-off
- grab, carry, jump with, throw and place Bob-ombs and small crates
- idle head turns, falling asleep after ten cycles, and waking up on input
- fast or slow ledge climbing based on input
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

## Ocarina of Time courses

Choose Mario or Adult Link and start **Link’s House** to play the village route. Walk through the curtained doorway onto the porch. Use **Space / gamepad A** at the ladder, near Saria, or beside a sign. The same button advances dialogue. Follow the path out of the clearing and cross the village to the **Kokiri Shop**. Walk back through its entrance to return to the forest outside the shop.

The house doors also lead to the Know-It-All Brothers’ house, the twins’ house, Mido’s house and Saria’s house. The opening-day village has all 13 Kokiri children, Saria, Mido and the shopkeeper. Conversations use the US ROM font, message backgrounds, highlighted words and choice prompts. Use W/S or the gamepad stick to choose a reply, then Space / A to advance. The interiors use their original paintings, fixed camera data, collision, and exit floors. Saria, the signs, the shopkeeper and shelf stock use ROM models and placements. Collect green and blue rupees in the village. At the shop counter, approach the shield shelf for a Deku Shield (40 rupees) or the heart shelf for one health wedge (10 rupees). Space / A confirms a purchase and J / B cancels. Both characters share the wallet and shield within each save slot. **Kokiri Forest** and **Lon Lon Ranch** also remain directly selectable as exploration courses.

To rebuild the maps from the tracked US 1.2 ROM and OoT definitions:

```sh
git submodule update --init reference/oot
python3 scripts/export-oot-courses.py
```

## Characters and animation

### Adult Link from Ocarina of Time

Choose **Adult Link** under **Character** on the main menu and select **Start Playing**, or open `?character=link`. The N64 model includes the Master Sword and Hylian Shield, 21 articulated limbs and 19 ROM animation clips.

| Action | Keyboard |
| --- | --- |
| Move, auto-jump | WASD, run over a ledge |
| Roll | Space while moving |
| Sword slash / follow-up | J or Shift, tap again during recovery for a finishing slash |
| Shield | Hold K or Ctrl |
| Focus / strafe | Hold L (left bumper on a gamepad). A gold marker identifies the opponent and the camera frames both fighters. |
| Side hop / backflip | While focused, press Space with a side or backward direction |

Link's analog movement and roll follow the OoT player code, adapted to the scale and fixed timestep of these courses. Course-specific interactions, swimming physics and scripted sequences use the shared game runtime.

To rebuild Link from a fresh checkout, run these commands from the repository root:

```sh
git submodule update --init reference/oot
python3 scripts/export-oot-link.py
```

The exporter reads the tracked US 1.2 ROM at `reference/roms/oot-us-1.2.z64` and the pinned OoT asset definitions in `reference/oot`. It checks the ROM checksum before writing `public/models/oot/link.json`. To use your own inputs, pass the ROM path and the OoT checkout path as the two positional arguments.

### Banjo-Kazooie

Choose **Banjo-Kazooie** under **Character** on the main menu (or in the pause menu), or open `?character=banjo`. Banjo plays every Mario and Zelda course. The high-poly N64 model (`ASSET_34E`) has 63 bones, its CI4 textures and prelit vertex colours. Kazooie's head, wings, legs and tail are separate appendage parts. They appear only during the moves that use her, as in the original game. 37 of Rare's keyframe animations are evaluated with the decomp's Catmull-Rom sampler and bone-matrix order.

Banjo has his own moveset, ported from the `src/core2/bs/*.c` states (velocities and gravity converted to 30 Hz frames at his 1.25 model scale):

| Move | Keyboard (pad) | Source |
| --- | --- | --- |
| Jump, higher while held | Space (A) | `bsjump.c`: 710 up, gravity −1350 held / −2700 released |
| Feathery Flap | Space again in the air, hold | `bsbflap.c`: 280 per flap, −1100 gravity, up to 2.5 s |
| Claw Swipe | J or Shift (B) standing or walking | `bsclaw.c`: three swipes over 1.3 s; picks up Bob-ombs and other grabbables |
| Forward Roll | J while running | `bstwirl.c` (BS_31_ROLL): 600 units/s for 0.9 s |
| Rat-a-tat Rap | J in the air | `bsbpeck.c`: pecks with small hops, −1400 gravity |
| Crouch | Hold K or Ctrl (Z) | `bscrouch.c` |
| Flap Flip | Space while crouched; hold Space to slow the fall | `bsbflip.c`: 920 up, −1200 gravity |
| Beak Barge | J while crouched; hold J for the full charge | `bsbbarge.c`: 850 (held) or 500, slowing by 80 per frame |
| Beak Buster | K in the air | SM64's ground pound timing with BK's animation |

Each move borrows the SM64 action with the same role (punch, jump kick, ground pound, …), so enemies, boxes and grabbable objects react through their native interactions. Walking, swimming, poles, ledges, slides and carrying still use Mario's controller with BK animations. Talon Trot, eggs and Wonderwing are not ported yet.

To rebuild Banjo from a fresh checkout, run these commands from the repository root:

```sh
git submodule update --init reference/banjo-kazooie
python3 scripts/export-bk-banjo.py
```

The exporter reads the US 1.0 ROM at `reference/roms/bk-us-1.0.z64` and the asset enums in `reference/banjo-kazooie`. It checks the ROM checksum, unpacks the rarezip asset table and writes `public/models/bk/banjo.json`. `node scripts/banjo-smoke.mjs` checks every move, claws a Battlefield Goomba, and loads Banjo into every course.

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

## Experiments

Eight toys built on the real engine (`src/game/experiments/`). Start one from the title screen or the pause menu, or open `?experiment=dimensions|god|runner|hundred|diorama|race|shrink|planet`. Esc → *Leave experiment* returns to normal play, and Esc → *Main menu* returns to the title screen from anywhere. The pause menu also swaps between Mario and Adult Link mid-level. Details: [the experiments brief](docs/experiments/BRIEF.md).

- **Across Dimensions** — on Bob-omb Battlefield, **Tab** (or 1–4) cycles 3D → 2D side → top-down → 2.5D path. 2D is a dolly-zoom into a lane-locked cross-section of the level; top-down slices away everything above Mario; the 2.5D path holds Mario on the race route to the summit with a side-on camera that swings around the mountain.
- **Mario vs. God** — keep playing while the mouse moves the world: tools 1–7 grab/drag/resize/delete, paint plank bridges at Mario's height, drop blocks, ramps and springs, spawn Bob-ombs and coins. Portals (tool 8) keep his speed through any wall or floor. Gravity presets, slow motion (T) and undo (⌘/Ctrl+Z).
- **Endless Run** — Mario auto-runs down a seeded track (`?runSeed=N`); steer with A/D, jump gaps and hurdles, dodge pillars. Dodge rolling boulders too. One fall ends the run; Space runs again. Daily mode shares a seed per day and races your best run's ghost.
- **A Hundred Marios** — 25/50/100 animated, full-physics Marios in five personalities (racer, coin hunter, follower, brawler, explorer). Click one to inspect it, Tab to watch it, P to change its personality.
- **Diorama** — the course inside a plastic compact case under a sky dome, with a tilt-shift lens and free-fly camera. Click Marios for tricks, Goombas to make them charge, the ground to drop a coin; the course's own objects react to the Mario you poked.
- **Summit Race** — you against seven rivals up the Battlefield's race route, with a countdown, live standings and best times.
- **Shrink Ray** — keys 1–4 rescale the whole course around Mario (×8, ×3, ×1, ×¼) with matching collision; same physics, different world.
- **Tiny Planet** — a spherical mini-world where gravity points at the core; run all the way around it.

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
- Loose coins and coin formations use the level’s macro placements. Red coins add two to the coin counter.
- The Chain Chomp and its four standalone wooden posts are exported from the level macros, along
  with the chain gate. The Chomp spawns its pivot post and five metallic-ball chain segments when
  Mario comes within 3000 units, lunges, and can be released by ground-pounding the pivot post. It
  then charges the gate and destroys it. Touching it deals three damage wedges through the shared
  damage pipeline.
- The purple switch at the cage, the openable bars grill and the two pit bowling balls are exported
  too. Stepping on the switch presses it, opens the cage doors and removes the grill barrier for
  the rest of the run. Pit bowling balls are camera-facing source billboards with the 130-unit
  graph offset; they roll down the slope and deal two damage wedges.
- King Bob-omb is exported at the summit. Mario can grab and throw him; each landing costs one of
  his three health points, and the third landing unlocks the summit star. King now grabs and
  throws Mario when he gets too close, with an intro dialogue; the full boss cutscene remains simplified.
- Koopa the Quick and the race flag are exported. Approaching Koopa starts a countdown and a
  scripted run along the source BoB trajectory; reaching the flag first unlocks the star. The race
  proposition dialogue is implemented and source path collision is approximated; the
  countdown/timer HUD is implemented.

Approximations are recorded here rather than hidden: Bob-ombs are invisible while held in the
original, but Jumpman shows the model just in front of Mario because the articulated model has no
hand attachment. Object-versus-object collision is limited to explosion chains. Large crates push
Mario out horizontally but do not yet act as standable floors. The Chain Chomp's five chain
segments are constrained along a line rather than simulated with per-segment physics, its lunge
speed is 40 u/f instead of 140 u/f, the closed chain gate is visual only, and the cage grill uses
one removable barrier instead of two per-door collision models. Pit bowling balls roll straight
down the slope instead of following the source waypoint tables. Coin and star rewards still use
the existing Jumpman objective layer rather than the original mission script.

## What is not ported

The goal was the controller and movement physics, not a full game. Omissions and approximations:

- **Object interactions are partial.** Bob-ombs, breakable boxes, the Chain Chomp, wooden posts,
  the chain gate, the purple switch/openable grill, pit bowling balls, climbable trees, a simplified
  water bomb cannon, a simplified King Bob-omb fight and a simplified Koopa the Quick race are
  implemented, along with Bob-omb Buddy advice NPCs, a normal Koopa that loses its shell when
  punched/stomped, a shell that can be kicked into a sliding hazard and ridden before jumping off; standalone pole actors,
  other grabbables and most enemy hitboxes are not. King Bob-omb's full cutscene and Koopa's source path collision remain approximated. The starting cannon in act 1 stays quiet. The mountain cannon sweeps across its firing arc and shoots intangible water bombs while Mario is nearby. Three separate ambush zones drop water bombs ahead of Mario, with a shadow marking their approach. Bob-omb Battlefield exposes all six missions in the pause menu and via `?act=N`.
  King Bob-omb and Koopa the Quick appear in acts 1 and 2 respectively. The island box awards star 3,
  the eight red coins award star 4, and the five flying-ring centers award star 5. Pounding the
  Chain Chomp’s post three times releases it to break the gate in front of star 6. The win overlay can return to the castle,
  and the Koopa race displays a live countdown/timer HUD. Collected stars persist per act, and
  best coin counts persist, in one of four local save slots shown in the HUD and selectable from
  the pause menu.
- **Cap mechanics are prototypes.** `Y` grants a 30-second wing cap; winged triple jumps and cannons use a source-derived pitch/yaw model. `U` grants Metal Cap (heavier gravity, fall-damage immunity, metallic tint) and `I` grants Vanish Cap (damage immunity and translucent tint). Metal-cap water mode sinks to the bottom and walks (simplified); full Vanish interaction rules remain. Surface types for ice, slippery,
  slow, lava and quicksand are supported; remaining surfaces are constants only.
- **Damage and death are partial.** Health is a source-style 8-wedge meter with hurt/heal
  counters, damage amounts, invincibility during knockback, soft/normal/hard knockback and coin
  healing. Losing the last wedge plays the source standing/on-back/on-stomach death animation
  before respawning; underwater it plays the two-part drowning sequence instead. Lava boost/burn
  and deep/instant quicksand sinking now use the source surface rules; a poison-gas area behavior
  exists but is not yet placed in a playable area.
- **No SM64 animation data.** Mario animations live in the ROM, not the decompilation repository.
  With local assets, the native Mario view plays the exported original clips.
  The procedural and CC0 fallback rigs still approximate those poses.
- **JavaScript numbers are f64.** The original uses f32 arithmetic. The algorithms and constants are
  the same, and the tests pin the important results (242-unit jump apex, 32-unit run speed), but
  individual frames are not bit-identical to an N64.
- **Simplified actions**: forward/backward rollouts are omitted; the air wall-bonk path maps
  backward knockback to the soft-bonk state. Sleeping, slow ledge climbing, ceiling hanging,
  tree-pole climbing, lava boost, quicksand sinking and the animation-frame-driven
  punch/fast-punch/kick/breakdance sequence now follow the source state machines.
- **Audio and environmental effects are partial but growing.** Terrain footstep samples, water splash/plunge
  samples, underwater fog, and water surfaces are implemented. The original music sequences run on the browser audio thread with the source instrument banks, channel dynamics and reverb presets.
- **The camera is not fully Lakitu yet.** It is a custom orbit camera with Lakitu/close/top-down/side
  presets (`C` cycles them, `X` snaps behind), and it eases behind Mario after manual orbit input
  expires while moving, adds speed-based look-ahead focus and uses low slide, interior and flight
  contexts. It feeds the same yaw value into `update_mario_joystick_inputs`; the full `camera.c`
  modes and scripted sequences remain.

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

`pnpm test` runs 165 deterministic simulation and asset tests:

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
- Chain Chomp spawning, chain links, post release, gate break and contact damage
- purple-switch activation, grill barrier removal and pit-bowling-ball contact damage
- idle sleep/wake transitions, the slow ledge climb and the three death sequences
- dynamic billboard registration and two-sided grill materials
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
node scripts/painting-smoke.mjs # enter all four castle lobby paintings
node scripts/warp-smoke.mjs     # castle front doors, secret slide and Bowser arena warps
node scripts/missions-smoke.mjs # per-act stars, red coins, and no-exit 100-coin stars
node scripts/keys-smoke.mjs     # Bowser keys, basement/upstairs unlocks, and return nodes
node scripts/water-smoke.mjs    # underwater fog, splash particles and water surfaces
node scripts/experiments-smoke.mjs # every experiment's signature interaction, character swap, main menu
node scripts/wing-smoke.mjs   # grant a wing cap and fly
node scripts/caps-smoke.mjs   # grant metal + vanish caps
node scripts/vanish-smoke.mjs # vanish-cap enemy aggro
node scripts/mountain-slide-smoke.mjs # castle painting through CCM slide, star and castle return
node scripts/cannon-smoke.mjs # launch from the Bob-omb Battlefield cannon
node scripts/metal-water-smoke.mjs # metal-cap water walking
node scripts/shell-smoke.mjs  # ride a Koopa shell and jump off
node scripts/king-grab-smoke.mjs # King Bob-omb grab and throw
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
The manifest preserves sample rates and sustain-loop markers. The in-game audio engine plays jumps, terrain-dependent footsteps, impacts, swimming strokes, coin sounds and Peach’s voice by their original US sound IDs. Each red coin has its own pitch, and the coin counter plays a sound every two simulation ticks until it reaches the earned total.

Background music runs the US sequence interpreter and portable audio mixer from sm64-port in an AudioWorklet. The checked-in WebAssembly module includes the extracted sequences, recompressed samples using their original ADPCM codebooks, instrument banks, envelopes and loop points. Level scripts supply the track and reverb preset for each area. The castle grounds play outdoor ambience. Mario's location controls the original dynamic layers in JRB, DDD, HMC, WDW and BBH.

To rebuild the module, use WASI SDK 34, a native C compiler, and the sm64-port checkout at commit `2b17d081c9798b31b91dc71f37994b0da28cffc9`. Run `python3 scripts/render-music.py --port /path/to/sm64-port --wasi-sdk /path/to/wasi-sdk`. The browser and GitHub deployment use the checked-in module.

## Native Rust port

`native/` holds a Rust/Bevy port: a bit-exact simulation (`native/sim`, checked against golden fixtures
generated from this TypeScript) plus a Metal-rendered app with menus, saves, audio and experiments.

```sh
cd native
cargo run --release                 # play from the checkout (uses ../assets)
cargo test --workspace --locked     # goldens + headless course ticks
./package-mac.sh && open target/Jumpman.app
```

Details (asset lookup, `JUMPMAN_*` hooks, controls, architecture, fixture regeneration):
[native/README.md](native/README.md) and [native/PORTING.md](native/PORTING.md).

## Mario Kart 64 free roam

Select **Mario Kart 64** in the Experiments menu, or open
`?level=playground&experiment=kart`. Luigi Raceway uses the original course
vertices/display lists, 40 ROM textures, and Mario's directional kart sprites.
**W/S** accelerate, brake and reverse; **A/D** steer. The touch stick and gamepad
left stick work too. Press **F**, gamepad **B**, or the **Get out** button to park
and walk/jump with Mario's existing SM64 controls. Return within reach and press
it again to drive. **R** or **Reset** returns both to the starting line.

The pinned decompilation is the `reference/mk64` submodule. Regenerate assets with:

```bash
git submodule update --init reference/mk64
pnpm export:mk64 reference/roms/mk64-us-1.0.z64
```

The US 1.0 ROM is tracked at `reference/roms/mk64-us-1.0.z64` (this repository is private), next to the Banjo-Kazooie and Ocarina of Time ROMs, so a fresh clone has every input. The generated `public/mk64/` assets are committed too (CI and the iOS/Mac packaging scripts read them directly); `git clone --recurse-submodules` and the export above are only needed to regenerate them.

The exporter verifies the US ROM checksum, also accepts byte-swapped `.v64` and
`.n64` input, and writes `public/mk64/` (committed, so deploys need no ROM). It reads the ROM in place;
it does not copy the ROM into the project. Generated assets are included in
production/offline builds. They are committed; re-run the export only to regenerate them.

This is an exploration integration, not the complete MK64 game or an emulator.
Driving uses a custom fixed-step controller with acceleration, reverse,
off-road slowdown and track collision. Walking uses the existing Mario runtime.
The parked empty kart is a procedural model, since MK64's sprites contain the
kart and driver together. Races, items, opponents, native MK64 audio, and animated
course props are not included. Source: [n64decomp/mk64](https://github.com/n64decomp/mk64).

The Rust app exposes the same mode in its experiment menu or via
`cargo run --manifest-path native/Cargo.toml -- playground --experiment kart`.
Mac/iOS packages include the same generated MK64 assets.

`node scripts/kart-smoke.mjs` tests driving, dismounting, walking, re-entry and
cleanup against the dev server on port 5174 (`URL` overrides it).
