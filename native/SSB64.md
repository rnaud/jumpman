# Super Smash Bros. (SSB64) fighters

The Smash import turns the user's own Super Smash Bros. (US) ROM into rigged, animated fighters: one skinned glTF per
fighter with every motion as a named clip, plus an index (`fighters.json`). Nothing from the ROM is in the repository or
in an app bundle; the decomp only supplies names and tables (three files plus thirteen headers, see the manifest below).

- Decomp: `reference/ssb64` = [VetriTheRetri/ssb-decomp-re](https://github.com/VetriTheRetri/ssb-decomp-re) pinned at
  `572a747d8171137718fad472c5d7473dc7309ffd` (2026-10-20). Only its sources and headers are read; nothing is built.
- ROM: Super Smash Bros. (U) [!], SHA-1 `e2929e10fccc0aa84e5776227e798abc07cedabf` (MD5 `f7c52568a31aadf26e14dc2b6416b2ed`),
  the US version the decomp builds byte-identically (its README). A developer copy goes in git-ignored
  `reference/roms/ssb64-us.z64`.
- Exporters: `scripts/export-ssb64.py` (reference, standard library only) and its byte-identical Rust port
  `native/byor-rs/src/ssb64.rs` (what the apps run). `byor-rs --ssb64 ROM` / `pipeline.py --ssb64 ROM` import it like the
  other games; the setup screen lists it as "Super Smash Bros. (US) [fighters]".
- Output (data dir): `public/models/ssb64/<fighter>.glb` for mario, fox, donkey, samus, luigi, link, yoshi, captain, kirby,
  pikachu, purin (Jigglypuff), ness and boss (Master Hand), and `public/models/ssb64/fighters.json`. About 49 MB in all.
- Viewer: `native/examples/ssb64_view.rs` loads one glb on a floor, plays clips and saves off-screen stills (headless).

## File system: relocData

| What | Where in the decomp | Format |
|---|---|---|
| File table | `src/lb/lbreloc.c`, `src/lb/lbtypes.h` (`LBTableEntry`), `lLBRelocTableAddr` = ROM `0x1AC870` (`smashbrothers.us.yaml`) | 2132 entries of 12 bytes + an end entry: `u32 is_compressed:1, data_offset:31; u16 reloc_intern_offset; u16 compressed_size; u16 reloc_extern_offset; u16 decompressed_size` (offsets and sizes in words). File data starts right after the table (`0x1B2C6C`). |
| Compression | `src/sys/dma.c` `syDmaDecodeVpk0` | "vpk0": size, method, two Huffman trees (offsets, lengths), LZ copies; method 1 has the 2-sample offset form. |
| Pointers | `lbRelocLoadAndRelocFile` | Each pointer slot holds `next_slot << 16 \| target_word`: an intern chain (targets in the same file) and an extern chain whose target file ids are a `u16` list stored after the file's data in the ROM. The exporter resolves every slot to `(file id, byte offset)`. |
| File names | `tools/relocFileDescriptions.us.txt` (`-NNN: Name`), also the `src/relocData/<id>_<Name>.c` names | `MarioMain` = 203, `MarioModel` = 296, `FTMarioAnimWait` = 499, ... |

## Fighters

| What | Where | Notes |
|---|---|---|
| Fighter table | `src/ft/ftdata.c` `FTData dFT<Name>Data` (`src/ft/fttypes.h`) | main file, main motion (event scripts) file, model file, special files, `o_attributes` (offset of `FTAttributes` in the main file, 0x428 for Mario), the motion descriptor array. |
| Attributes | `FTAttributes` (`src/ft/fttypes.h`, typed per fighter in `src/relocData/*Main.c`) | physics (walk/dash/run speeds, jump heights, gravity, weight, `size` = model scale, 1.12 for Mario), hurtboxes, `setup_parts` (+0x29C, which joints exist), `commonparts_container` (+0x2D4), model parts, texture parts (faces). |
| Skeleton | `FTCommonPartContainer.commonparts[detail].dobjdesc` -> `DObjDesc[]` (`src/sys/objtypes.h`), built by `lbCommonSetupFighterPartsDObjs` (`src/lb/lbcommon.c`) | 44-byte entries `{s32 id; Gfx *dl; Vec3f translate, rotate, scale}` ending with id 18. `id & 0xFFF` is the depth: a joint's parent is the last joint one level up. Detail 0 = high poly (exported). |
| Transform | `lbCommonFighterPartsFuncMatrix` -> `syMatrixTraRotRpyR(Sca)` (`src/sys/matrix.c`) | `T * Rz(rot.z) * Ry(rot.y) * Rx(rot.x) * S`, radians. |
| Display lists | the joint DLs, drawn in tree order by `ftDisplayMainDrawDefault` (`src/ft/ftdisplaymain.c`) | F3DEX2. The renderer sets 2-cycle mode and `G_LIGHTING` before the joints. Vertices stay in the RSP cache between joints, so a child's triangles reuse its parent's vertices: SSB64's soft skin, exported as glTF skin weights (each vertex bound to the joint whose DL loaded it). `FTCommonPart.flags & 0xF == 1` (Yoshi, Master Hand): `dl` is a `Gfx *[2]` whose first list is drawn with the parent's matrix. |
| Materials | `MObjSub` (`src/sys/objtypes.h`), per joint in `p_mobjsubs`; costume scripts in `p_costume_matanim_joints`; drawn by `gcDrawMObjForDObj` (`src/sys/objdisplay.c`) as segment `0x0E` | Palette, texture image, tile size, prim/env colours and light colours (Mario's shoes are coloured by `LIGHT_1`). The costume script (AObjEvent32 at frame = costume) picks the costume: costume 0 is exported. |
| Motion table | `FTMotionDesc dFT<Name>MotionDescs[]` (`ftdata.c`) | `{anim file id, event script offset, flags}` indexed by motion id. Flags: `ANIMJOINT` (an AObjEvent32 animation, e.g. the entry `Appear` motions: skipped), `SHIELDPOSE` (a pose from the shield-pose file: skipped), TRANSN/XROTN/YROTN (root-motion helper joints). |
| Motions ("figatree") | `src/ft/ftanim.c` `ftAnimParseDObjFigatree`, `src/sys/objanim.c` `gcPlayDObjAnimJoint`, `lbCommonAddFighterPartsFigatree` | An anim file is an array of per-joint pointers (joint tree order) to `AObjEvent16` scripts: `u16 {opcode:5, flags:10, toggle:1}`, an optional `u16` frame count when `toggle`, then one `s16` per flagged track (RotXYZ, TraI, TraXYZ, ScaXYZ; scales 1/512 rad, 1/4 unit, 1/4096, rates 1/512, 1/32, 1/8192). Opcodes set linear, Hermite (value + rate) and step keys, wait (`Block`), `Loop` and `End`. The exporter runs the interpreter frame by frame (60 fps) and writes every frame as a LINEAR glTF key. |

### Motion ids (src/ft/ftdef.h `FTCommonMotion`, same for every fighter)

0 EntryNull, 1-3 Rebirth{Down,Stand,Wait}, **4 Wait**, 5-7 Walk{Slow,Middle,Fast}, 8 WalkEnd, **9 Dash**, **10 Run**,
11 RunBrake, 12 Turn, 13 TurnRun, **14 KneeBend** (jump squat), 15 GuardKneeBend, **16-17 Jump{F,B}**, 18-19 JumpAerial{F,B},
**20 Fall**, 21 FallAerial, 22-24 Squat{,Wait,Rv}, **25-26 Landing{Light,Heavy}**, 27-28 Pass, 29-30 Ottotto (teeter),
**31-39 Damage{Hi,N,Lw}{1,2,3}**, 40-42 DamageAir{1,2,3}, 43 DamageE, **44-48 DamageFly{Hi,N,Lw,Top,Roll}**, 49 WallDamage,
50 DamageFall, 51-52 FallSpecial / LandingFallSpecial, 53 Twister, 54-56 Dokan (pipes), 57 StopCeil, 58-71 Down* (knockdown,
get-up, tech), 72-87 Cliff* (ledge), 88-111 item get / throw, 112-127 Sword / Bat / Harisen / StarRod swings, 128-133
ray gun, fire flower, hammer, 134-145 Guard (shield) / Escape (roll) / ShieldBreak / FuraFura (dizzy), 146-149 Catch /
CatchPull / Throw{F,B}, 150-163 being grabbed / thrown, **164 Appeal** (taunt), **165-166 Attack11/12** (jab), 167
AttackDash, 168-172 AttackS3 (forward tilt, 5 angles), 173-175 AttackHi3, 176 AttackLw3, **177-181 AttackS4** (forward
smash, 5 angles), **182 AttackHi4**, **183 AttackLw4**, **184-188 AttackAir{N,F,B,Hi,Lw}**, 189-194 LandingAir*, **195 =
SpecialStart**: from here the fighter's own enum (`src/ft/ftchar/ft<name>/ft<name>.h`), e.g. Mario 195 Attack13, 196-197
Appear{R,L}, 198 SpecialN (fireball), 199 SpecialAirN, 200-201 Special{,Air}Hi, 202-203 Special{,Air}Lw.

Not every id has an animation (an empty entry means the fighter cannot do it, e.g. Kirby and Jigglypuff have no
JumpAerialF; their multi-jumps are their own JumpAerialF1-5).

## Exported data

`<fighter>.glb`:
- node 0: the skinned mesh (one primitive per material, `JOINTS_0`/`WEIGHTS_0` with weight 1 on the loading joint);
- node 1: the armature root named after the fighter, scaled by `UNIT * FTAttributes.size` (UNIT = 0.004: in the bind pose
  Mario is 1.84 units tall, Kirby 1.17, Pikachu 1.41, Link 2.38, Donkey Kong 4.67 standing upright); faces +Z, feet at y = 0;
- nodes 2..: `joint_NN` (NN = DObjDesc index), in tree order;
- materials: the colour combiner is evaluated per texel (prim, env, light colour as shade) into the base-colour texture or
  factor (metallic 0, roughness 1); environment-mapped parts (Link's blade) get the texture's mean colour; alpha cut-outs are
  MASK. Wrap modes follow the tile's clamp / mirror / mask.
- animations: one clip per distinct animation file, named after the first common motion that plays it (`Wait`, `WalkSlow`,
  `Run`, `KneeBend` (also the landing), `JumpF`, `Fall`, `Attack11`, `AttackS4`, `DamageHi1`, ...), fighter-specific
  motions after the file (`FireballGround`, `SuperJumpPunchAir`, ...). 60 fps, LINEAR, every joint keyed on translation,
  rotation and scale (held channels with two keys). One pass of looping motions. Root translation is left in the clips (SSB64 moves
  the fighter in code; only the TRANSN-flagged motions carry root motion).

`fighters.json`: per fighter `label`, `file`, `joints`, `triangles`, `size`, `unit`, `clips` (`frames`, `seconds`,
source file id and name) and `motions`: motion id -> `{clip, motion}` with the decomp's motion name (common or the
fighter's own, e.g. `"198": {"clip": "FireballGround", "motion": "SpecialN"}`). This is the table a controller maps
SM64 actions to. `attributes` holds the `FTAttributes` numbers a controller tunes with (walk / dash / run speed, jump
heights, air control, gravity, terminal velocities, `jumps_max`, `weight`), in SSB64 units per frame.

Counts (costume 0, high poly): 22-29 joints, 256-474 triangles, 139-186 clips per fighter (Master Hand: 22).

## Checks

- `python3 -m unittest byor/test_ssb64.py` (from `native/`): vpk0, relocation chains, figatree interpreter, combiner, tile
  axes, decomp tables, ROM registration (synthetic data only).
- `cargo test -p byor-rs` (`src/ssb64.rs` unit tests, `tests/ssb64_rom.rs`): with `BYOR_TEST_SSB64_ROM` the export runs
  and every fighter must have the core motions; with `BYOR_TEST_SSB64_REF` (a `scripts/export-ssb64.py` output) all 14
  files must be byte-identical (they are).
- `byor/check-manifest.py --trace --ssb64 ROM --strict-unused`: the import reads exactly the 16 manifest files.

## Making them playable (plan, not started)

1. **Movement**: drive a fighter with the existing SM64 sim (`jumpman_sim`, Mario's actions and physics), exactly as
   `characters/gltf_view.rs` drives the Quaternius rig: it already resolves SM64 action ids to a `ClipKey` and the
   `ClipKey` to a clip name through `ClipAliases`. A Smash fighter is a `GltfRig` whose aliases come from `fighters.json`
   (motion name -> clip): Idle = Wait, Walk = WalkMiddle (WalkSlow / WalkFast by speed, the clips are 90 / 60 / 40-frame
   cycles that scale with stick), Jog = Dash, Sprint = Run, JumpStart = KneeBend then JumpF (JumpAerialF for the double
   jump, Kirby / Jigglypuff JumpAerialF1-5), JumpLoop = Fall, JumpLand = the LandingLight clip (it is KneeBend), Roll =
   EscapeF, CrouchIdle = SquatWait, PunchJab = Attack11, PunchCross = Attack12 (Attack13 for the third hit), Interact =
   LightGet. New keys worth adding to `ClipKey`: Kick -> AttackS3 / AttackHi4, GroundPound -> AttackAirLw, Dive ->
   AttackDash, Hurt -> DamageN1, KnockedDown -> DownBounceD, GetUp -> DownStandD, Taunt -> Appeal, Ledge -> CliffCatch
   / CliffWait, Swim -> (none: SSB64 has no swimming; use Fall). Scale: fit the bind-pose height to Mario's 160 SM64 units
   (`gltf_view::visual_adjust`); feet are at y = 0 and the rig faces +Z, so no rotation offset. Speeds stay Mario's (the
   sim), optionally scaled per fighter from `attributes` (run_speed / Mario's 44, jump_height_base / 26, gravity / 2.4).
2. **Character slot**: add `Character::Smash(Fighter)` (or one variant per fighter), gated in `content.rs` by a
   `Game::Ssb64` probe of `../public/models/ssb64/fighters.json` (a new probe is safe: `full_build_has_everything` must
   then skip SSB64 or the checkout needs the export). Sounds: SSB64 voices are not exported yet (they are in the FGM bank,
   `src/sys/audio.c`, `MUSIC_AND_SFX_DISCOVERIES.md`).
3. **Gem Clash** (`experiments/brawl*.rs`): add `Hero::Smash(id)` with a `HeroStyle` from `attributes` (hp ~100,
   speed = run_speed / 44, weight from `weight`, scale from the height) and map `brawl_core::Act` to clips: Idle Wait,
   Air JumpF / Fall, Punch(0..2) Attack11 / Attack12 / AttackS3 (or Attack13), Dive AttackDash (air: AttackAirF), PoundWind
   / Pound / PoundLand AttackAirLw / LandingAirLw, Swing AttackS4, Spin AttackHi4 (Link SpecialHi), Shoot SpecialN, Toss
   ThrowF, Nova SpecialLw, Beam SpecialN, Hurt DamageN1, Down DownBounceD, GetUp DownStandD, Dead DamageFlyRoll. Timing:
   the clips are the game's (an Attack11 is 15-20 frames at 60 fps), so `HeroStyle.combo` windows can be taken from the
   clip lengths. Master Hand fits as a Gauntlet boss (its 22 clips: Slap, Poke, Walk, GunShoot, Punch, Dying...).
4. Effort: the generic rig + alias table is one piece of work (~1-2 days including the scale/feet tuning and tests);
   after that each fighter is data (~1-2 hours: check the aliases, specials and sounds); Gem Clash heroes ~0.5 day each for
   the move feel. Kirby and Jigglypuff need the multi-jump mapping, Link and Samus their item / weapon models (not exported).

## Not done yet

- Not playable (plan above).
- Costumes other than 0, face (texture part) swaps, hand model parts (open / closed fists), low-poly models, the polygon /
  metal / giant variants (NMario, MMario, GDonkey), accessories (Pikachu's hat, Jigglypuff's bow), hitboxes and the
  motion event scripts (sounds, effects, hitbox timing in `*MainMotion` files), item and weapon models, stages.
- The `TraI` spline track is ignored (no exported motion uses it visibly).
