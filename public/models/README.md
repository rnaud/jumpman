# Character models

## Bundled CC0 rig

`quaternius/AnimationLibrary.glb` is the **Universal Animation Library** by
[Quaternius](https://quaternius.com/packs/universalanimationlibrary.html), released under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).

File SHA-256: `1b7bf67866360665426bb99e4c71bd619f19b408453c24e30f0c3071601eee5c`

It contains a rigged mannequin mesh and 46 humanoid animation clips, including:

- `Idle_Loop`, `Walk_Loop`, `Jog_Fwd_Loop`, `Sprint_Loop`
- `Jump_Start`, `Jump_Loop`, `Jump_Land`
- `Roll`, `Roll_RM`
- `Crouch_Idle_Loop`, `Crouch_Fwd_Loop`
- `Punch_Jab`, `Punch_Cross`
- `Interact`, `Swim_*`, `Sit_*`, `Push_Loop`

The original notice is in `quaternius/LICENSE.txt`. No paid Source-tier models are included.

## Using your own rig (option 1)

The loader reads any glTF/GLB that exposes the clip names below.  Put your file under
`public/models/` and open the page with:

```
http://127.0.0.1:5173/?character=/models/my-character.glb
```

The game still starts with the procedural Mario rig; once your model loads, press **V** to toggle.
A custom file should be roughly humanoid, face **+Z**, and be authored in metres (the loader scales
it to 160 SM64 units and drops its feet onto the ground).

Clip names used by `GltfCharacterView`:

| Mario state | Clip |
| --- | --- |
| idle, land stops | `Idle_Loop` |
| walk / jog / sprint | `Walk_Loop`, `Jog_Fwd_Loop`, `Sprint_Loop` |
| jump start | `Jump_Start` |
| jump loop, freefall | `Jump_Loop` |
| landing | `Jump_Land` |
| flips, dive, slides, ground pound | `Roll` / `Roll_RM` |
| crouch / crawl | `Crouch_Idle_Loop`, `Crouch_Fwd_Loop` |
| punch | `Punch_Jab`, `Punch_Cross` |
| ledge | `Interact` |

If your clip names differ, edit `CLIP_ALIASES` in `src/game/GltfCharacterView.ts`.  Models with
different proportions work, but animations authored for another skeleton may need retargeting in
Blender before export.

## Local-only character slot

`public/models/local/` is gitignored except for its README.  Put a private `character.glb` there and
open:

```
http://127.0.0.1:5173/?character=local
```

The loader starts from a glTF/GLB; it does not extract or convert assets from a ROM.  Use only models
and animations you created, commissioned, or are licensed to use, and keep ROM-extracted assets out
of commits and shared builds.
