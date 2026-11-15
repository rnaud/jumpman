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

## Playable fighters

The twelve fighters are playable as skins on Mario's sim, like the playable Bowser (`native/src/characters/smash.rs`;
Master Hand is not playable):

- **One implementation, data-driven**: `Character::Smash(Fighter)`. Each frame Mario's action picks a `smash::Move`
  (the table at the top of `smash.rs`: Wait, WalkSlow / WalkMiddle / WalkFast / Run by speed, JumpF then Fall,
  JumpAerialF (Kirby / Jigglypuff JumpAerialF1-3) for the double / triple jump and backflip, LandingLight, SquatWait,
  Attack11 / Attack12 / Attack13 (else AttackS3) down Mario's punch sequence, AttackAirN for the jump kick, AttackAirF for
  the dive, AttackDash for the slide kick, AttackAirLw / LandingAirLw for the ground pound, DamageN1, DownBounceU / D,
  DamageFlyN / Roll, DamageE, WallDamage, Appeal for the star dance, CliffWait / CliffClimb*, LightGet / LightThrowF;
  swimming plays Fall tilted forward, flight JumpF tilted). A move is a list of decomp motion names; the fighter's
  `fighters.json` motion table resolves the first one it has to its clip, so nothing is written per fighter. The glb is
  driven by the object views' `AnimRig` in seek mode on the module's own clock.
- **Size**: one common scale (Smash Mario = SM64 Mario's 160 units), so the fighters keep Smash's relative sizes; Donkey
  Kong is trimmed to 0.72 (about 1.8x Mario). The hitbox stays Mario's. The follow camera backs off for the big ones
  (`Fighter::camera_scale`, up to 1.4x); in Gem Clash everyone is drawn 0.8x-1.25x Mario.
- **Speeds and jumps** stay Mario's (`ATTRIBUTE_SPEEDS` is off: the courses are built around Mario's numbers).
- **Where**: SM64 courses and every mode that takes more than Mario (Swarm, Deck, Relic, Gem Clash, Gate, Last Stand map
  them to their Mario hero); Kart, Halls and Party stay Mario-only. Gem Clash maps its hero states straight to motions
  (`smash::gem_clash_clip`). `JUMPMAN_CHARACTER=ssb-mario|ssb-luigi|ssb-dk|ssb-link|ssb-samus|ssb-yoshi|ssb-kirby|ssb-fox|
  ssb-pikachu|ssb-jigglypuff|ssb-falcon|ssb-ness`, the V cycle and the character select's SMASH group; a fighter whose glb
  is missing is left out. `JUMPMAN_SMASH_CLIP=<motion or clip>` holds one clip for headless checks.
- **Sounds**: no voices are exported; the fighters use the `silent` voice set (SM64's hit sound, the star jingle).
- **Looks**: Link's sword and shield and Samus's arm cannon are joints of their models and come with them. Projectiles
  and special effects are particles (see Specials); items, hand / face swaps are not drawn.

Not done: per-fighter move feel (Gem Clash heroes from `attributes`), voices, Master Hand as a boss.

## Specials

Every fighter's neutral, up and down special works on Mario's sim (`native/sim/src/smash.rs`, engine-free, 30 Hz, SM64
units; app side `native/src/characters/smash.rs` (clips, input), `smash_fx.rs` (particles, sound messages, goombas) and
`native/src/experiments/smash_targets.rs` (minigame enemies)).

- **Input**: the special button is the N64 R: `Action::RButton` (H on the keyboard, rebindable; R2 on a pad). Special
  alone = neutral, with the stick pushed up = up special, down = down special (A held also means up, Z held down: a 3D
  stick is also the walking direction). Neutral and up specials turn to the stick first. iOS / touch: **no special
  button** (the HUD has none for these fighters; Banjo's R button is Banjo-only), so touch players get Mario's moves only.
  Test hook: `JUMPMAN_STICK="3:r"` presses R at 3 s.
- **How it runs**: `MarioRuntime::smash` (`SmashExt`). While a special runs it owns Mario's frame like `step_banjo`
  does, moving him with the sim's own ground / air steps (walls, ceilings, floors, ledge grabs stay SM64's); a knockback,
  death or warp (anything that changes Mario's action from outside) cancels it. Recoveries end in the helpless fall
  (`FallSpecial`: drift only, no specials, ledges grab) and its landing lag. Kirby's Stone is armour
  (`take_damage` refuses), Jigglypuff's Rest is a real sleep (no input for the rest of the clip).
- **Numbers**: decomp headers (`ft<name>.h`, `wpdef.h`, `wpvars.h`, `itvars.h`) converted with `K` = 0.348 (SSB64 units
  to SM64: Smash Mario is 460 tall, SM64 Mario 160), `KV` = 2K per tick for speeds (60 -> 30 Hz) and `KA` = 4K for
  accelerations. Moves that SSB64 drives by root motion (TRANSN clips: Super Jump Punch, Falcon Punch / Dive / Kick,
  Final Cutter, Pound) follow their clip's root travel (peak height / distance / frame measured once from the import);
  the view holds `joint_00` still in those clips (`root_motion_clips`) so the model does not travel twice. Damage comes
  from the weapon tables where the decomp has them (Charge Shot 3-26%, boomerang return 8%, Kirby's spit 10%) and is
  otherwise an approximate SSB64 value: the per-frame hitboxes live in the `*MainMotion` event scripts, which the import
  does not read yet.
- **Animation**: each phase names its decomp motions (`SmashExt::motion`, e.g. Giant Punch `SpecialNStart` /
  `SpecialNLoop` / `SpecialNEnd` / `SpecialNFull`, `SpecialAir*` in the air, Kirby's copies `Copy<Name>SpecialN`); the
  import already has every one of them (a test checks them all against `fighters.json`).
- **Hits**: hitboxes (spheres, this tick) and projectiles hit `SmashTargets`: the course's SM64 objects (attacked like
  a punch, inside the sim), the walking goombas (defeated), Hold the Gate's foes (1% = 2.2 health, electric / sleep
  stun) and Star Swarm's swarm (1% = 1/6 of the run's weapon hit, sleep freezes). Kirby and Yoshi swallow what they
  catch (bosses and elites resist). Gem Clash, Relic Descent, Deck, Last Stand and Party are not wired: Gem Clash plays
  its own fighter sim (the specials do not run there), the others only see the SM64-object and goomba hits.
- **Sound hook**: `characters::smash_fx::SmashSpecialSfx { fighter, mv, slot, phase }` (a Bevy message) at each sound
  moment; `phase` is `SfxPhase::{Start, Charge, Full, Release, Projectile, Hit, Explode, Special, Land, End}`. Nothing
  plays yet: a sound layer reads the messages.

| Fighter | Neutral | Up | Down |
|---|---|---|---|
| Mario | Fireball: bounces (50, -5 degrees, gravity 1.2, 140 frames), 7% fire | Super Jump Punch: +613 / 338 forward (root motion), 1% coins, 6% top; helpless (drift 0.6, lag 25) | Mario Tornado: 88 frames, stick steers (0.025, 17 max), special taps lift (22, max 40), 2% multi + 6% |
| Luigi | Green fireball: straight (36, no gravity, 80 frames), 6% | Super Jump Punch, 25% fire sweet spot on the first frames | Luigi Cyclone (as the Tornado) |
| Donkey Kong | Giant Punch: 10 charge loops (13 frames each), stored when full or cancelled (A / Z), 10% + 1.8 per level (full ~28%), lunge | Spinning Kong: ground travel (26 max), air lift 20.3 at 0.07 gravity, multi-hit, helpless | Hand Slap (ground only): two shock-wave slaps per loop (12%), loops while special is pressed again |
| Link | Boomerang: out decelerating, back homing, caught; one at a time (`Empty` clip); 9% out / 8% back | Spin Attack: ground multi-hit spin; air lift 69 at 0.23 gravity, helpless (0.6, lag 0.65) | Bomb: lobbed, bursts on a body / wall / landing / fuse (300 frames) |
| Samus | Charge Shot: 7 levels every 20 frames, stored when full / cancelled, fires the level (3-26%, speed 60-74, size 100-260); air fires the stored level, recoil | Screw Attack: lift 62 (10 forward), multi-hit, helpless (0.66, lag 0.4) | Bomb: morph-ball hop in the air, bomb with a 100-frame fuse, 180-wide blast |
| Yoshi | Egg Lay: tongue (frames ~8-20) swallows a body (removed) | Egg Throw: arc (gravity 2.7), 340-wide burst, stick tilts, one air hop | Yoshi Bomb: hop, slam at -150, 15% landing blast, two stars sliding front and back |
| Kirby | Inhale: suction cone while held; mouthful: special spits a star (10%), Z swallows (a fighter's neutral special is copied and then played with Kirby's `Copy*` clips) | Final Cutter: +665 (root motion), straight down, a shock wave on landing (100 speed, 20 frames) | Stone: armour, falls at -140, 18%; special after 18 frames or 160 frames turns back |
| Fox | Blaster: laser at 160, 3%, repeated presses fire again | Fire Fox: 35 frames of flames (gravity after 15), 30 frames at 115 toward the stick (up without one), helpless (1.0, lag 0.34) | Reflector: 5% shine, held while special is held (18 frames min), sends enemy projectiles back |
| Pikachu | Thunder Jolt: crawls along floors (55), drops at -45 degrees from the air, 8% electric | Quick Attack: zip toward the stick (up without one), a second zip on another press, helpless (0.4) | Thunder: bolt from the sky (-450), 8% on its way, 17% burst when it strikes Pikachu (+20 hop) |
| Jigglypuff | **Pound** (SSB64's neutral special; Rollout is Melee's, there is no Rollout clip): 167-unit lunge (root motion), 12% | Sing: 3 s of song, sleep in a 230 radius | Rest: 20% on the first frames, then asleep for the 251-frame clip |
| Captain Falcon | Falcon Punch: wind-up, lunge 129 (root motion), 25% fire on frames 53-60 | Falcon Dive: +561 / 141 forward (root motion), grabs a body on the way up, 10% blast and a jump off, helpless (0.72) | Falcon Kick: ground dash of 988 (root motion), 15% then 9%; from the air a dive forward and down to the floor and `LandingDownSpecial` |
| Ness | PK Fire: spark (73 / 95, -3.6 / -38 degrees) that becomes a pillar of fire (multi-hit) on a body or the floor | PK Thunder: steered ball (6 degrees a frame: stick left / right turns, up / down pitches; 160 frames, 8%); steered into Ness: PK Thunder 2 launches him along its heading (25%), helpless | PSI Magnet: held while special is held (30 frames min), absorbs enemy energy shots and heals |

Not done: the ROM's weapon / effect models (fireball, Charge Shot, PK Thunder... are particles), hitbox timing and damage
from the motion event scripts, knockback and percent on the fighters themselves, shields, the specials in Gem Clash, a
touch special button.

## Not done yet

- Costumes other than 0, face (texture part) swaps, hand model parts (open / closed fists), low-poly models, the polygon /
  metal / giant variants (NMario, MMario, GDonkey), accessories (Pikachu's hat, Jigglypuff's bow), hitboxes and the
  motion event scripts (sounds, effects, hitbox timing in `*MainMotion` files), item and weapon models, stages.
- The `TraI` spline track is ignored (no exported motion uses it visibly).
