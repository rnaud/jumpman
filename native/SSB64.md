# Super Smash Bros. (SSB64) fighters

The Smash import turns the user's own Super Smash Bros. (US) ROM into rigged, animated fighters: one skinned glTF per
fighter with every motion as a named clip, plus an index (`fighters.json`). Nothing from the ROM is in the repository or
in an app bundle; the decomp only supplies names and tables (three files plus fourteen headers, see the manifest below).

- Decomp: `reference/ssb64` = [VetriTheRetri/ssb-decomp-re](https://github.com/VetriTheRetri/ssb-decomp-re) pinned at
  `572a747d8171137718fad472c5d7473dc7309ffd` (2026-10-20). Only its sources and headers are read; nothing is built.
- ROM: Super Smash Bros. (U) [!], SHA-1 `e2929e10fccc0aa84e5776227e798abc07cedabf` (MD5 `f7c52568a31aadf26e14dc2b6416b2ed`),
  the US version the decomp builds byte-identically (its README). A developer copy goes in git-ignored
  `reference/roms/ssb64-us.z64`.
- Exporters: `scripts/export-ssb64.py` (reference, standard library only) and its byte-identical Rust port
  `native/byor-rs/src/ssb64.rs` (what the apps run). `byor-rs --ssb64 ROM` / `pipeline.py --ssb64 ROM` import it like the
  other games; the setup screen lists it as "Super Smash Bros. (US) [fighters, stages, music]" (import stamp version 3).
- Output (data dir): `public/models/ssb64/<fighter>.glb` for mario, fox, donkey, samus, luigi, link, yoshi, captain, kirby,
  pikachu, purin (Jigglypuff), ness and boss (Master Hand), and `public/models/ssb64/fighters.json`. About 49 MB in all.
  Plus the voices (see Sounds), the 41 stages (`assets/courses/ssb64_*`, `assets/levels/ssb64_*`, about 5 MB, see Stages)
  and the music (`assets/audio/music/ssb64`, about 1.3 MB, see Music).
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

## Sounds

The import also renders the twelve fighters' voices and their jump / special-move effects from the ROM:
`scripts/export-ssb64-sounds.py` (reference) and its byte-identical port `native/byor-rs/src/ssb64_sounds.rs` (run by
`ssb64::run`, so `byor-rs --ssb64 ROM` and the pipeline write them with the fighters). Output (data dir):
`assets/audio/voices/ssb64/<Name>_0.wav` (mono 16-bit, 32000 Hz; `Name` = the `gmFGMVoiceID` name without `nSYAudio`,
e.g. `VoiceMarioJump`, `FGMCaptainSpecialNPunch`) and `manifest-ssb64.json` (id, frames, duration, peak, cut). 237 clips,
about 14 MB: every `nSYAudioVoice<Fighter>*` of the twelve fighters (197) plus the 41 effects in `EXPORT_FGM` (jumps,
specials, the KO star), minus `VoiceLinkNull` (its script plays nothing). Python takes ~5 s, Rust ~0.3 s.

| What | Where in the decomp | Format |
|---|---|---|
| Sample bank | `B1_sounds2.ctl` ROM `0xC6B650`, `.tbl` `0xC7B1F0` (`smashbrothers.us.yaml`, `MUSIC_AND_SFX_DISCOVERIES.md`) | libultra `ALBankFile` (`include/PR/libaudio.h`): one bank, one instrument whose `soundArray` holds every effect sample, VADPCM (order 2) with optional `ALADPCMloop`. |
| Sound ids | `src/gm/gmsound.h` `gmFGMVoiceID` (US branch of the `REGION_US` conditionals) | 695 entries; the id indexes `fgm.ucd`. |
| Voice scripts | `fgm.ucd` `0xF5A9C0`: `{s32 count; u32 offsets[]; data}`, run by `func_80026B90_27790` (`src/libultra/n_audio/n_env.c`) once per 60 Hz audio frame | `0xD0` stop, `0xD1` articulation, `0xD2` staccato flag, `0xD4` duration table, `0xD5`/`0xD6` volume, `0xD9` fork a script, `0xDA`/`0xDB` loop, `0xDE` priority group, `0xDF`/`0xE0` -2400/-4800 cents; notes `(pitch_code << 3) \| dur_code`: `pitch_code * 100 - 1300` cents (0 = release). The first note spawns the articulation, later notes only re-pitch it (legato). |
| Articulations | `fgm.tbl` `0xF57BF0`, same package shape, run by `func_80027460_28060` | `0x00` vol, `0x10` pan, `0x20` pitch (cents), `0x30` fx, `0x40` attach LFO, `0x50` detach, `0x60` trigger sample N of the bank, `0x70` end (silence), `0x80`/`0x90` loop; a 4-bit wait (extended form for longer waits) after each. |
| LFOs | `fgm.unk` `0xF573D0`: `{u32 count; {u8 shape, target, postproc, phase; f32 period, amplitude, offset}[]}` | sine (`gSYSinTable`, here `sin`), square, saws, one-shot ramps, sample-and-hold / interpolated random (the engine's LCG, seed 1); targets volume, pitch, pan, scratch registers or another LFO's parameters. |
| Playback | `n_alSynStartVoiceParams(wave, alCents2Ratio(pitch + note), vol)` with `unityPitch = 0`; output rate 32000 (`dSYAudioPublicSettings`) | a sample plays at `32000 * 2^(cents / 1200)` Hz; volume `vol * (set_volume * 127 >> 7) * 127 >> 7` of 32768. |

The exporter runs those interpreters offline exactly as the engine schedules them (scripts, then articulations, then
one frame of mixing with linear interpolation; mono, pan and reverb ignored) and stops a sound that is still playing
after 3.5 s (charge loops: `FGMSamusSpecialNCharge0`, `FGMKirbySpecialNStart`, `FGMNessPKThunderLoop`, the Screw
Attack) with a 10 ms fade. Note: the decomp's enum names around ids 183-189 are partly guesses; `FGMCaptainSpecialNStart`
/ `NPunch` fork into ids 186 / 187, which the enum calls `FGMFoxSpecialHiStart` / `Fly` (both moves are fire effects).

In the game (`native/src/characters/smash_audio.rs`): `smash_sfx(fighter, SmashSfx::X)` names the clips of a sound,
`smash_audio::play(&mut sfx, fighter, X)` or `SfxCue::Smash { fighter, sfx: X }` plays them. `SmashSfx`: `Jump`,
`DoubleJump`, `AttackLight` / `AttackStrong` / `AttackSmash` (the fighter's `Smash1-3` grunts), `HeavyLift`, `Hurt`,
`Ko` (star KO scream + twinkle), `Death`, `Dizzy`, `Taunt`, and the specials `NeutralSpecialStart` (wind-up: "FALCON..."),
`NeutralSpecialCharge` (Samus / DK charge loop), `NeutralSpecial` (release: "...PUNCH!", fireball, blaster, PK Fire,
Thunder Jolt, Charge Shot, boomerang, Inhale, Egg Lay, Giant Punch, Pound), `NeutralSpecialEnd` (boomerang return, egg,
Kirby's spit), `UpSpecialStart` (Fire Fox take-off, "PK Thunder!"), `UpSpecial`, `DownSpecial`, `DownSpecialEnd` (Rest
wake-up, Yoshi Bomb landing, reflect, Samus's bomb blast, Pikachu's bolt). Each fighter's voice set
`ssb_<stem>` in `assets/audio/character-sounds.json` maps the SM64 triggers to these: jump, double jump / backflip / wall
kick, punches (Mario's `MARIO_PUNCH_*` grunts answered by name) and kicks, hurt, death, the star dance (taunt).

| Fighter | Jump / double | Neutral special | Up special | Down special |
|---|---|---|---|---|
| Mario | voice / voice | fireball | Super Jump Punch + coin | Mario Tornado voice |
| Luigi | voice / voice | fireball (Mario's sound) | Super Jump Punch | Luigi Cyclone voice |
| Donkey Kong | - / - | charge, Giant Punch yell | Spinning Kong | Hand Slap |
| Link | voice / voice | boomerang throw, return | Spin Attack voice + swing | bomb pull |
| Samus | jump / aerial effects | charge loop, Charge Shot | Screw Attack | bomb, explosion |
| Yoshi | voice / voice | tongue, egg lay | Egg Throw voice + throw | Yoshi Bomb jump, fall |
| Kirby | puff jump / inflate | Inhale loop, spit | Final Cutter voice | Stone voice + effect |
| Fox | - / voice | Blaster | "Fire!" + take-off, flight | Reflector, reflect |
| Pikachu | - / - | "Pika!" + Thunder Jolt | Quick Attack voice + effect | Thunder call, bolt |
| Jigglypuff | puff jump / inflate | Pound | Sing | Rest, wake-up |
| Captain Falcon | - / voice | "FALCON..." + flame, "...PUNCH!" + blast | Falcon Dive voice + effect | Falcon Kick voice |
| Ness | jump effect / - | "PK Fire!" + PSI | "PK Thunder!", thunder loop | PSI Magnet |

## Motion event scripts (hitboxes)

Every fighter motion has an event script in the fighter's `*MainMotion` file: when each hitbox comes on and off and with
what damage / angle / knockback, when sounds and voices play, effects, hurtbox state (intangible / invincible), throws,
flags the status code reads. `scripts/export-ssb64-motions.py` (reference) and its byte-identical port
`native/byor-rs/src/ssb64_motions.rs` (run by `ssb64::run`, so `byor-rs --ssb64 ROM` and the pipeline write it with the
fighters) decode them for the 13 fighters into `public/models/ssb64/motion-events.json` (about 2.5 MB; Python 1.5 s).

| What | Where in the decomp | Format |
|---|---|---|
| Script per motion | `FTMotionDesc.offset` (`src/ft/ftdata.c`), symbolic offsets from `include/ft/motiondesc_offsets.h`; `0x80000000` = none; `FTANIM_FLAG_SUBMOTION_SCRIPT` = in the SubMotion file (no exported fighter uses it) | byte offset into the fighter's MainMotion file (Mario 202, Captain 235...) |
| Events | `enum FTMotionEvent` (`src/ft/ftdef.h`, 52 opcodes), layouts `FTMotionEvent*` (`src/ft/fttypes.h`, MSB-first bit fields), builder macros `ftMotionCommand*` (`ftdef.h`), the decompiled scripts in `src/relocData/*MainMotion.c` | 32-bit words, `opcode:6` on top. `MakeAttackColl` (5 words): `id:3 group:3 joint:7s damage:8 can_rebound:1 element:4`, `size:16 off_x:16s`, `off_y:16s off_z:16s`, `angle:10s knockback_scale:10 knockback_weight:10 hit_ground:1 hit_air:1`, `shield_damage:8s fgm_level:3 fgm_kind:4 knockback_base:10`. Pointers (subroutine, goto, parallel script, throw descriptor) go through the file's relocation chains. |
| Interpreter | `ftMainParseMotionEvent` / `ftMainUpdateMotionEventsAll` (`src/ft/ftmain.c`), started by `ftMainSetStatus` with `script_wait = 1` and run once per 60 Hz frame | `SyncWait n` adds n frames, `AsyncWait n` waits for animation frame n, loops / subroutines share one 5-slot stack, `Goto` back = a repeating script, `SetParallelScript` starts a second thread, `PauseScript` stops it. |
| Hitbox placement | `ftMainUpdateAttackStatFighter` (`ftmain.c`), `gmCollisionGetFighterPartsWorldPosition` | centre = joint world matrix x offset; TopN is scaled by `FTAttributes.size`, `MakeAttackCollScaled` divides the offset by it; radius = `size / 2`. Joint id 0 = TopN (feet), `4 + n` = `DObjDesc` n (the glb's `joint_NN`), -2 = the light-item hand. |

The exporter runs the interpreter offline exactly as the game schedules it (frame 0 = the frame the motion starts, so
Mario's first jab hits on frames 2-3, Falcon Punch on 42-46) and writes per fighter (`size`) and motion id:
`motion` (decomp name), `script` (offset), `end` (frame of `End`, null when the script loops or pauses), `loops`
(`[{thread, from, period}]`: a goto back into code that already ran; the events from `from` repeat every `period` frames
and the simulation stops there), `root_motion`, `events` and `hitboxes`:

- `events`: every event in firing order with its `frame` and `op` (the decomp's `nFTMotionEvent*` name) and its decoded
  fields: hitboxes (above), `SetAttackColl{Offset,Damage,Size,SoundLevel}`, `ClearAttackColl{ID,All}`,
  `RefreshAttackCollID`, sounds (`PlayFGM`, `PlayVoiceStoreInfo`, `PlayFGMStoreInfo`, `PlayLoop*`, `fgm` id + `name` from
  `gmFGMVoiceID`: the exported clip is `assets/audio/voices/ssb64/<name>_0.wav` when it is in the sound export),
  `PlaySmashVoice` (a random `Smash1-3` grunt), `Effect` (joint, effect id, offset, scatter), hurtboxes
  (`SetHitStatusAll` / `PartAll` / `PartID`: 0 none, 1 normal, 2 invincible, 3 intangible; `SetDamageCollPartID`),
  `SetThrow` (the `FTThrowHitDesc`: status, damage, angle, knockback scale / weight / base, element), flags 0-3, model /
  texture part swaps, colour animations, rumble, after-images, slope contour. `thread: 1` marks the parallel script.
- `hitboxes`: the attack collisions (`fp->attack_colls[4]`) replayed into windows of constant values: `id`, `from`, `to`
  (exclusive; null = on until the motion ends), `set` (hit records: a target is hit at most once per set; a new hitbox
  shares the records of an active one of its group, `RefreshAttackCollID` starts new ones), `group`, `joint`, `offset`,
  `size` (diameter), `scaled`, `damage`, `angle` (361 = Sakurai angle), `knockback_scale` (growth), `knockback_weight`
  (fixed / set knockback), `knockback_base`, `element` (0 normal, 1 fire, 2 electric, 3 slash, 4 coin, 5 freezing,
  6 sleep), `shield_damage`, `hit_ground`, `hit_air`, `can_rebound`, `fgm_kind` / `fgm_level` (hit sound), and
  `positions`: the centre on every frame of the window, **on the animated skeleton** (the motion's figatree pose of the
  joint at that frame, the same interpreter as the glb clips), in fighter space: SSB64 units from the feet, +Z forward,
  +Y up. Root-motion clips (those whose `joint_00` starts at the origin, the view's `root_motion_clips` rule; in the game
  TransN is unlinked from the body and its translation drives the fighter) hold `joint_00` at the origin, as the view
  draws them. `on_joint: false` when the joint is not in the model (Yoshi's tongue joint 31, items): the offset from the
  feet is used.

Counts: 30-236 scripts per fighter, 2030 hitbox events in all (Mario 250, Samus 246, Donkey Kong 198...). A check
against the decompiled scripts in `src/relocData/*MainMotion.c` (every `ftMotionCommandMakeAttackColl` of 1887 script
arrays, walked linearly) matches all of them; the two "differences" are angles the C source spells unsigned (954 = -70).

### On the sim (`native/sim/src/smash_hitbox.rs`, engine-free)

`MotionEvents::from_json(text)` loads the file; the app does it once (`characters::smash::register_motion_events`, when
the fighter index loads) and `smash_hitbox::register`s it; `SmashExt::scripts` overrides it per runtime (tests). API:

- `ev.fighter("Captain")?.by_name("SpecialN")` / `.by_id(203)` / `.first(&names)` -> `Motion`.
- `Motion::active(frame60)` -> `Vec<ActiveHit>` (the window, the frame, the centre in fighter space);
  `active_in_tick(t)` = frames `2t` and `2t + 1` (the sim is 30 Hz, SSB64 60 Hz); `active_in_frames(a, b)`;
  `active_span()`; `script_frame(f)` (repeating scripts wrap); `sounds_between(a, b)`; `hit_status_at(f)`.
- `ActiveHit::world(feet, yaw, K)` / `smash_hitbox::to_world`: fighter space -> world (SM64 s16 yaw, `smash::K` = 0.348 SM64
  units per SSB64 unit); `HitWindow::radius()` (SSB64 units).
- **Knockback descriptor** (`smash_hitbox::Knockback`, on every scripted `Hitbox` and on the `SmashHit` a target receives):
  `angle` (degrees, 361 = Sakurai), `growth` (`knockback_scale`), `fixed` (`knockback_weight`, 0 = none), `base`
  (`knockback_base`), `element` (`Element`), `damage` (percent, before staling), `shield_damage`, `hit_ground`, `hit_air`,
  `can_rebound`, `sfx_kind`, `sfx_level`. `SmashHit` also has `motion` (the SSB64 motion id) and `mv: Option<SpecialMove>`
  (`None` for a normal attack). This is the input for SSB64's knockback (`ftParamGetCommonKnockback` family in
  `src/ft/ftparam.c`) and the percent layer: see "Knockback and percent".

Use in the moves (`native/sim/src/smash.rs`):

- **Specials**: each tick, when the motion the view plays (`SmashExt::motion`: names, seconds) has scripted hitboxes, the
  special's own damaging volumes (normal / fire / electric / explosion) are replaced by the script's for frames `2t`,
  `2t + 1` of that motion: damage, timing, size, element, angle, knockback and the centre on the animated joint, turned to
  the fighter's facing around Mario's feet. Capture / grab / sleep volumes and projectiles stay the move's. Catch moves
  (`ftParamSetCatchParams`: Falcon Dive, Kirby's inhale, Yoshi's egg lay) are left alone: their attack collisions are grab
  detectors. With the user's import, 17 specials take the script's hitboxes: Super Jump Punch, Mario / Luigi Tornado,
  Giant Punch, Spinning Kong, Hand Slap, Spin Attack, Screw Attack, Final Cutter, Stone, Fire Fox, Reflector, Thunder,
  Pound, Sing, Rest, Falcon Punch, Falcon Kick (the projectile specials have no melee hitbox in their scripts: fireball,
  Charge Shot, boomerang, bombs, eggs, blaster, Thunder Jolt, PK Fire / Thunder keep the weapon numbers).
- **Normal attacks**: while no special runs, Mario's punch sequence (`Attack11` / `Attack12` / `Attack13`, else
  `AttackS3`, at the view's 1.15x), jump kick (`AttackAirN`), dive (`AttackAirF`), slide kick (`AttackDash`) and ground
  pound (`AttackAirLw` at 1.2x, `LandingAirLw`) put out that motion's scripted hitboxes at the clip's time
  (`smash::normal_attack_motion`), as `Hitbox { normal: true }` / `SmashHit { mv: None }`. The course's SM64 objects and
  the modes that already score Mario's own punches (Hold the Gate, Star Swarm) ignore them (SM64 hits those); the walking
  goombas take them. They are the data the fighter-vs-fighter knockback / percent layer reads.
- Sounds still follow `SfxPhase` cues (`smash_fx::special_cues`): the script's sound timing is exported
  (`sounds_between`) but not yet played from it.

## Checks

- `python3 -m unittest byor/test_ssb64.py` (from `native/`): vpk0, relocation chains, figatree interpreter, combiner, tile
  axes, decomp tables, ROM registration (synthetic data only).
- `cargo test -p byor-rs` (`src/ssb64.rs` unit tests, `tests/ssb64_rom.rs`): with `BYOR_TEST_SSB64_ROM` the export runs
  and every fighter must have the core motions; with `BYOR_TEST_SSB64_REF` (a `scripts/export-ssb64.py` output) all 14
  files must be byte-identical (they are).
- `cargo test -p byor-rs --test ssb64_rom ssb64_sounds`: with `BYOR_TEST_SSB64_ROM` every fighter's voices and every
  effect are exported and audible; with `BYOR_TEST_SSB64_SOUNDS_REF` (a `scripts/export-ssb64-sounds.py --out <dir>`
  folder) all 238 files are byte-identical (they are). `src/ssb64_sounds.rs` unit tests and `byor/test_ssb64.py`
  `Sounds` cover the interpreters without a ROM.
- `cargo test -p jumpman smash_audio` (`native/src/characters/smash_audio.rs`): the table covers every fighter, names only
  exported clips, equals the `ssb_*` sets of `character-sounds.json`; with the clips in `assets/audio/voices/ssb64` they
  decode and are audible (skipped without them).
- `python3 -m unittest byor/test_ssb64_motions.py`: the motion script decoder (hand-assembled `ftMotionCommand*` words),
  interpreter (waits, loops, subroutines, goto loops), hitbox windows and hit-record sets, the decomp tables.
- `cargo test -p byor-rs --test ssb64_rom ssb64_motion_events`: with `BYOR_TEST_SSB64_ROM`, Mario's jab and Falcon Punch
  have the decomp's numbers and frames; with `BYOR_TEST_SSB64_MOTIONS_REF` (a `scripts/export-ssb64-motions.py --out <dir>`
  folder) `motion-events.json` is byte-identical (it is). `src/ssb64_motions.rs` unit tests cover the decoder and windows.
- `cargo test -p jumpman-sim smash_hitbox` and `--test smash_specials` (`scripted_*`, `normal_attacks_use_the_scripted_jab`):
  windows, 30 Hz ticks, wrap, Falcon Punch active only on ticks 21-23 with the script's knockback; with
  `JUMPMAN_SMASH_MOTION_EVENTS=<motion-events.json>` (`real_scripts_drive_the_specials`) every special's scripted hitbox
  belongs to the motion the view plays and the real Falcon Punch comes out on tick 21 with 24-26%.
- `byor/check-manifest.py --trace --ssb64 ROM --strict-unused`: the import reads exactly the manifest files (18 with
  `include/ft/motiondesc_offsets.h`).

- `python3 -m unittest byor/test_ssb64_stages.py` (from `native/`): the AObjEvent32 interpreter, the extrusion's
  winding per line kind, the pass-through flag, the stage table, the ALSeqFile reader and the BGM names (synthetic data).
- `cargo test -p byor-rs --test ssb64_stages_rom`: with `BYOR_TEST_SSB64_ROM` all 41 stages and the 47 sequences export
  (four player starts on every VS stage, spawn inside the blast zones, a floor line); with `BYOR_TEST_SSB64_STAGES_REF` /
  `BYOR_TEST_SSB64_MUSIC_REF` (the `--out` folders of the Python exporters) every file is byte-identical (they are).
- `cargo test -p jumpman game::ssb64 ssb64_music` (with the import in `assets/` or `JUMPMAN_SMASH_TEST_ASSETS`): every stage
  loads, Mario lands on the spawn's floor, walks left / right and never leaves the depth band; the moving platforms of
  Congo Jungle, Saffron City, Peach's Castle and Planet Zebes move and carry him; the music renders.

- `python3 -m unittest byor/test_ssb64_weapons.py`, `cargo test -p byor-rs weapons` and `--test ssb64_weapons_rom`: with
  `BYOR_TEST_SSB64_ROM` every model / item sound is written; with `BYOR_TEST_SSB64_WEAPONS_REF` (a
  `scripts/export-ssb64-weapons.py --out <dir>/models/ssb64 --sound-out <dir>/sounds` folder) all 84 files are
  byte-identical (they are). `cargo test -p jumpman-sim items` (pick-up, swing, throw, food, Star, Bob-omb, Ray Gun,
  spawner) and `cargo test -p jumpman smash_models`.
- `byor/check-manifest.py --trace --ssb64 ROM --strict-unused`: the import reads exactly the 17 manifest files.

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
- **Sounds**: each fighter has its own SSB64 voices and special-move effects ("Sounds" above): voice set `ssb_<stem>` for
  the SM64 action triggers, `characters::smash_audio` for the specials.
- **Looks**: Link's sword and shield and Samus's arm cannon are joints of their models and come with them. Effects,
  projectiles, items and hand / face swaps are not drawn.

Not done: per-fighter move feel on the courses (speeds / jumps from `attributes`), Master Hand as a boss.

## Specials

Every fighter's neutral, up and down special works on Mario's sim (`native/sim/src/smash.rs`, engine-free, 30 Hz, SM64
units; app side `native/src/characters/smash.rs` (clips, input), `smash_fx.rs` (particles, sound messages, goombas) and
`native/src/experiments/smash_targets.rs` (minigame enemies)).

- **Input**: the special button is the N64 R: `Action::RButton` (H on the keyboard, rebindable; R2 on a pad). Special
  alone = neutral, with the stick pushed up = up special, down = down special (A held also means up, Z held down: a 3D
  stick is also the walking direction). Neutral and up specials turn to the stick first. X on a pad is the special too
  (unless the player bound X). iOS / touch: the **SPECIAL** button (`touch::Control::Smash`, left of B) for a Smash
  fighter outside the modes and in Gem Clash (beside its SHOCK WAVE / SEEK BEAM buttons, `TouchControls::smash_with_mode`;
  the other modes keep their own buttons and read R2 / H only). Test hook: `JUMPMAN_STICK="3:r"` presses R at 3 s.
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
  from the weapon tables where the decomp has them (Charge Shot 3-26%, boomerang return 8%, Kirby's spit 10%); melee
  hitboxes come from the motion event scripts when the import has them ("Motion event scripts" above: damage, frames,
  size, angle, knockback, placed on the animated joint) and are otherwise the approximate values below.
- **Animation**: each phase names its decomp motions (`SmashExt::motion`, e.g. Giant Punch `SpecialNStart` /
  `SpecialNLoop` / `SpecialNEnd` / `SpecialNFull`, `SpecialAir*` in the air, Kirby's copies `Copy<Name>SpecialN`); the
  import already has every one of them (a test checks them all against `fighters.json`).
- **Hits**: hitboxes (spheres, this tick) and projectiles hit `SmashTargets`: the course's SM64 objects (attacked like
  a punch, inside the sim), the walking goombas (defeated), Hold the Gate's foes (1% = 2.2 health, electric / sleep
  stun) and Star Swarm's swarm (1% = 1/6 of the run's weapon hit, sleep freezes). Kirby and Yoshi swallow what they
  catch (bosses and elites resist). Relic Descent, Last Star Standing and Star Deck have adapters too (see "Minigame
  adapters"); Gem Clash runs the specials on a seat per fighter ("Gem Clash"). Party is not wired (Mario-only).
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

The table's damage numbers are the fallback when the import has no `motion-events.json`.

Not done: shields; Party.

## Knockback and percent (`native/sim/src/smash_knockback.rs`)

SSB64's formulas from the decomp, engine-free:

| What | Decomp | Here |
|---|---|---|
| Knockback | `ftParamGetCommonKnockback` (`src/ft/ftparam.c`): `((((p/10 + p*d/20) * w * 1.4) + 18) * growth/100) + base`, `p` = percent before the hit + the damage queued this frame (this hit), set knockback `(((1 + 10*f/20) * w * 1.4) + 18) * growth/100 + base`, capped at 2500; damage ratio / handicaps 1 | `knockback(percent, damage, &Knockback, weight)` |
| Weight | `FTAttributes.weight` (`src/relocData/2xx_<Name>Main.c`, a knockback multiplier): Mario / Luigi / Fox 1.0, DK 0.83, Samus 0.92, Yoshi 0.93, Link / Falcon 0.96, Ness 1.1, Pikachu 1.16, Kirby 1.19, Jigglypuff 1.3 | `weight(fighter)` |
| Angle | `ftCommonDamageGetKnockbackAngle`: 361 = 43 degrees in the air, on the ground 0 below 32 knockback else 42.5 | `launch_angle` |
| Launch | `ftCommonDamageInitDamageVars`: `kb` units / frame along the angle (`vel_damage_air`), losing 1.7 a frame along itself (`ftmain.c`) | `Launch::of` (SM64 units / tick: `kb * K * 2`, decay `1.7 * K * 4`), `Launch::decay_step` |
| Hitstun / level | `ftParamGetHitStun` = kb / 1.875 frames; `ftCommonDamageGetDamageLevel` 12 / 24 / 32 (level 3 = launched) | `hitstun_frames`, `damage_level` |
| Hitlag | `ftParamGetHitLag` (US): `d/3 + 5` frames, electric x1.5 | `hitlag_frames` |
| Weapons | `WPAttributes` of `*Special1.c` / `*Main.c` / `251_ITCommonData.c` (fireball 361 / 25 / base 10, boomerang 70 / 30 / 55, PK Fire 80 / 50 / 40, Thunder 70 / 50 / 80, Thunder Jolt 361 / 20 / 10...) | `weapon(mv, kind, damage)`; `resolve` now fills `SmashHit::knockback` for every hit (script, weapon table, items, else `approximate`) |
| Percent | `percent_damage` | `Percent`; minigame foes: missing health as the percent (`minigame_launch`) |
| Blast zones | `map_bound_*` (the stages' `blast` block, `game::ssb64::Stage`) | `BlastZone` (a box; Gem Clash builds it from its arena) |

DI: SSB64 has no trajectory DI (only Smash DI during hitlag); not implemented.

Where it applies: **Gem Clash** (every Smash hit, special, projectile, scripted normal attack and SSB64 item on any
fighter, with the victim's SSB64 weight; a Gem Clash hero's weight is the inverse of its Gem Clash weight), the
**minigames' foes** through their adapters (`minigame_launch`: Relic Descent's push, Last Star Standing's knock per
tick, Gem Clash's monsters), and the course sim's own targets keep their behaviour.

## Gem Clash (`native/src/experiments/brawl_smash.rs`, `brawl_items.rs`)

A Smash fighter (the player's character, `Config::smash`, or a bot in tests / the balance harness) fights with its
SSB64 moveset:

- **Specials**: each Smash fighter has a **seat**, a private `MarioRuntime` on a flat floor whose `smash` layer runs the
  special exactly as on the courses. The seat is anchored at the arena floor under the fighter (re-anchored when that
  changes, the projectiles kept in place), stepped with the fighter's input while the special runs (`Act::Special`;
  camera yaw 0, so world (x, z) = stick (x, -y)), and its motion is fed back through Gem Clash's collision
  (`integrate`: walls, floors, the void, the blast zones). Projectiles keep flying after the special (the seat coasts
  them). Hits resolve against the other fighters (SSB64 launch, Kirby copies a fighter he swallows, Sing puts to sleep,
  Stone is armour, the Reflector sends Gem Clash's shots back, the PSI Magnet absorbs bullets and heals) and the
  monsters. A blow cancels a special. Input: R (H, R2 / X, touch SPECIAL); stick up or A held = up, stick down or Z held
  = down. H is still SEEK BEAM in the power form (it wins that tick).
- **Normal attacks**: the punch chain, the dive, the ground pound and its landing play `Attack11` / `Attack12` /
  `Attack13`, `AttackAirF`, `AttackAirLw`, `LandingAirLw` with the motion scripts' hitboxes, damage and knockback (the
  punch chain's length from the script's end), when the import has them; else Mario's blows. A scripted normal deals at
  least the Gem Clash blow it stands in for (`normal_damage_floor`: Mario's punch chain 6 / 6 / 10, dive 9, ground pound
  13; harder SSB64 moves keep their own damage, the knockback stays the script's). At SSB64's own 2-5% jabs a Smash
  player's B button dealt a third of everyone else's (`human_experience` with the import's scripts: 167 damage a minute as
  Smash Mario against Easy bots, 366 as Gem Clash Mario; now 379), which made the bots look unbeatable.
- **Knockback**: the victim's percent is its lost health (`Fighter::percent`); a launch sets `Fighter::kvel` (decaying
  1.7 a frame), hitstun (Hurt, or Down when launched), hitlag on both. A launched fighter flies over the invisible rim and
  is knocked out beyond the blast zones (the rim / floor half size + 520, 2600 above the floor, the void below). Health
  still knocks out at 0, as for everyone in Gem Clash; no HUD change (the cards keep the health bar; percent is not
  shown, there is no slot for it).
- **Bots**: Smash bots use their specials by range (`ai_use`: projectiles at range, rushes and slams close, Rest only on
  a stunned foe), recover with their up special when off the floor with the jumps spent, release charges (Giant Punch,
  Charge Shot) at the foe, reflect / absorb incoming shots. The app's Gem Clash opponents stay its own cast (Goombas,
  Koopas...); Smash bots exist in the core (tests, balance) but the app does not draw them.
- **Fair play against a human** (`brawl_ai.rs` `hold_fire_on_human`, all bots, only when the foe is a human, so the
  AI-vs-AI balance is untouched): a bot does not start a blow, shot, throw or special on a human who is still reeling (hurt,
  knocked down, getting up), and leaves a human that another bot just hit alone for a moment (Easy 0.8 s, Normal 0.67 s,
  Hard 0.33 s; `human_grace`), so two bots do not trade the player between them. Measured by `brawl_human_tests.rs` (a scripted ordinary / passive player against the
  app's line-up of two bots with monsters; `human_experience` prints the table, `bots_leave_the_player_room` guards it):
  against Easy bots the bots' damage a minute drops from 202 to 151 (Gem Clash Mario) and their hits on a reeling player
  from 12% to 5%, hits right after another bot's from 16% to 10%; Normal and Hard keep their numbers (novice wins: Easy
  83% -> 94%, Normal 46% -> 45%, Hard 23% -> 24%). The Smash combat itself did not make Gem Clash's bots more aggressive
  (a Mario player meets the same numbers with or without the SSB64 items); the weak scripted normals above did.
- **Names**: cards and banners say the fighter's name ("JIGGLYPUFF POWER!", "PLAYING AS C. FALCON") instead of "You" /
  "MARIO".
- **SSB64 items** (`brawl_items.rs`, on in the app when the import has the object models; `JUMPMAN_BRAWL_SMASH_ITEMS=0|1`):
  55% of the drops (`SMASH_ITEM_SHARE`) are SSB64 items from `SMASH_ITEM_TABLE` (Beam Sword, Home-Run Bat, Fan, Star
  Rod, Hammer, Ray Gun, Fire Flower, Maxim Tomato, Heart Container, Star, Bob-omb, Green / Red Shell, Motion-Sensor
  Bomb). Each rides a Gem Clash mechanic (swords / bat / fan / rod / hammer = the hammer's swing, Ray Gun / Fire Flower
  = the blaster's shots, shells = a throwable, bombs = the bomb, food = the heart, Star = the bubble) so **every
  character** picks them up and the bots use them; hits use SSB64 numbers (`item_knockback`: weapon tables for the
  shots, the swing values, the item's `ITAttributes` damage when thrown) through `smash_hit`. Maxim Tomato heals half,
  the Heart Container all, the Star makes the toucher invincible for `ITSTAR_INVINCIBLE_TIME`. Drawn with the import's
  models (`BrawlBridge::smash_items` -> `characters::smash_models`). The arenas' own item spots are used; Gem Clash does
  not run on the SSB64 stages, so their item points are not used.

## Minigame adapters (`native/src/experiments/smash_targets.rs`)

| Mode | Targets | Damage | Knockback / effects |
|---|---|---|---|
| Hold the Gate | the marching foes | 1% = 2.2 health | electric / sleep stun, swallow |
| Star Swarm | the swarm | 1% = 1/6 of a weapon hit | sleep freezes; a swallow finishes a foe of up to four weapon hits (else 12%) |
| Relic Descent (`RelicTargets`) | the dungeon's foes (not hidden ones) | 1% = 1/7 of the hero's swing | SSB64 push (`minigame_launch`), electric / sleep stun, swallow non-bosses |
| Last Star Standing (`LssTargets`) | the other Marios | 1% = 1 hit point | SSB64 launch through the mode's knock |
| Star Deck (`DeckTargets`) | the stage's foes | 1% = 0.5 attack, card modifiers | at most 2 special hits a turn (`SMASH_HITS_PER_TURN`); the hero plays specials on the stage between cards (`deck::deck_controller`) |

Normal attacks are left to the modes (they score Mario's own punches). Every adapter multiplies by the fighter's
`minigame_scale` (Balance). Star Swarm also runs the SSB64 items for a Smash fighter (`swarm_item_rates`: a drop every
8 s, four at most, no containers; food heals the run's hero, the Star protects it; item hits go through `SwarmTargets`).
Items there need the Smash fighter's item layer (non-Smash characters get none in Swarm).

## Balance (`native/src/experiments/brawl_balance_tests.rs`)

Harness: Gem Clash AI-vs-AI duels (Normal bots, first to one round, 4 min cap), every pair of the 16 characters (the
12 Smash fighters and Mario, Link, Young Link, Banjo), 6 seeds x the 6 box arenas x both seat orders = 1080 duels per
character, on 3 threads (about 2 s in `--release`); with the import's motion scripts
(`JUMPMAN_SMASH_MOTION_EVENTS`), with and without SSB64 items. Minigame score: the fighter alone against a Star Swarm
pack of 24 unkillable Skitters closing in at 240 units / s for 30 s, spamming one special at the nearest foe; the score
is its best special's SSB64 percent per second through `SwarmTargets`.

Tuning: Gem Clash `smash::tuning` (health, damage dealt): Donkey Kong 96 hp / 0.77, Kirby 95 hp / 0.81, Ness 104 hp /
1.12, Mario / Luigi / Samus / Fox 1.08 damage, the rest SSB64's own (knockback dealt and speed stay 1.0). Minigames `smash_targets::minigame_scale`: Mario
1.25, Luigi 1.1, Donkey Kong 0.5, Link 1.3, Samus 3.6, Yoshi 0.42, Kirby 1.05, Fox 1.2, Pikachu 0.38, Jigglypuff 0.95,
Captain Falcon 0.8, Ness 0.85 (area multi-hitters down: Hand Slap, Yoshi Bomb, Thunder; single-target shooters up).

| Fighter | win % before (no items / items) | after (no items / items) | swarm score before (best special) | after (x median) |
|---|---|---|---|---|
| Mario | 42 / 43 | 43 / 45 | 194 | 247 (0.92) |
| Luigi | 42 / 47 | 44 / 48 | 232 | 256 (0.96) |
| Donkey Kong | **63** / **61** | 54 / 54 | **576** | 289 (1.08) |
| Link | 46 / 49 | 48 / 48 | 184 | 239 (0.89) |
| Samus | 42 / 46 | 47 / 49 | 65 | 234 (0.88) |
| Yoshi | 53 / 48 | 50 / 51 | **693** | 291 (1.09) |
| Kirby | **66** / **61** | 56 / 53 | 247 | 259 (0.97) |
| Fox | 42 / 43 | 43 / 44 | 209 | 251 (0.94) |
| Pikachu | 47 / 47 | 47 / 46 | **735** | 280 (1.05) |
| Jigglypuff | 53 / 52 | 55 / 54 | 291 | 277 (1.04) |
| Captain Falcon | 46 / 46 | 44 / 46 | 365 | 292 (1.09) |
| Ness | 40 / 42 | 49 / 48 | 327 | 275 (1.03) |
| Mario (hero) | 53 / 53 | 55 / 52 | - | - |
| Link (hero) | 56 / 57 | 55 / 58 | - | - |
| Young Link (hero) | 53 / 50 | 54 / 51 | - | - |
| Banjo (hero) | 55 / 55 | 55 / 55 | - | - |

(Before: 4 seeds, all knobs 1.0; after: 6 seeds. Swarm before: one seed, the median then ~270.) Every character is in
43-58% after, every swarm score within 0.88-1.09x of the median. Blast-zone KOs: 0 in these bot duels (health runs out
first; the knockback grows only up to ~100% here), they are covered by tests. `smash_balance_regression` (always on)
runs each Smash fighter against Mario and another fighter (12 duels each) and the swarm scores: no fighter below 8% or
above 92%, no swarm score above 1.35x the median with the scripts (2.5x on the approximate fallback hitboxes).
`balance_full` / `balance_swarm_slots` (`--ignored --nocapture`) print the tables. After the normal-attack damage floor ("Gem Clash"
above), with the scripts: 42-56% without items, 42-54% with them (Fox lowest, Kirby highest; the four heroes 49-52%).

## Stages

`scripts/export-ssb64-stages.py` (reference; it reuses the fighter exporter's relocData reader, F3DEX2 / RDP interpreter
and glTF writer) and its byte-identical port `native/byor-rs/src/ssb64_stages.rs` (run by `ssb64::run`) export all 41
grounds of `dMPCollisionGroundFileInfos` (`src/mp/mpcollision.c`): the nine VS stages (Peach's Castle, Sector Z, Congo
Jungle, Planet Zebes, Hyrule Castle, Yoshi's Island, Dream Land, Saffron City, Mushroom Kingdom), Final Destination, Meta
Crystal, Duel Zone (the Battlefield-style polygon stage), Race to the Finish, How to Play, the 1P mini Yoshi's Island, the
beta Dream Land and the test stage, and the twelve Break the Targets and twelve Board the Platforms stages. Output (data
dir, ~1.9 s in Rust, ~80 s in Python): `assets/courses/ssb64_<stage>.json` (a regular course document),
`assets/levels/ssb64_<stage>/stage.glb`, `part_<n>.glb` for each moving platform, `assets/courses/ssb64_stages.json`.

| What | Where in the decomp | Format |
|---|---|---|
| Stage file | `GR<Name>Map` (`tools/relocFileDescriptions.us.txt`), `MPGroundData` (`src/mp/mptypes.h`) at 0x14 in the VS maps (after their item weights; found as the offset whose +0x40 map geometry and +0x84 item weights are pointers) and 0 in the others | four `MPGroundDesc` layers, `map_geometry`, `layer_mask`, fog colour, light angle, camera bounds, `map_bound_*` (blast zones), `bgm_id`, `map_nodes`, `alt_warning`. |
| Geometry | `grDisplayMakeGeometryLayer` (`src/gr/grdisplay.c`), `gcSetupCustomDObjs` (`src/sys/objanim.c`) | Each layer is a DObjDesc tree (`id & 0xFFF` = depth) with `MObjSub` lists (materials, `costume` = frame 0 of the material animations) and AObjEvent32 joint animations; a layer whose `layer_mask` bit is set carries `DObjDLLink {list_id, dl}` lists (opaque 0, translucent 1) instead of one display list. 63-451 triangles a stage. |
| Joint animation | `gcParseDObjAnimJoint` / `gcPlayDObjAnimJoint` (`src/sys/objanim.c`) | AObjEvent32: `{opcode:7, flags:10, payload:15}` then one f32 per flagged track (two for the rate forms); SetVal (linear), SetValRate / SetVal0Rate (Hermite), SetValAfter (step), Wait, Jump / SetAnim (loop). Run frame by frame like the figatree interpreter; one period is kept (up to 7200 frames: Peach's Castle's platform loops every 4800). |
| Collision | `MPGeometryData`, `mpCollision*` (`src/mp/mpcollision.c`), `src/mp/mpdef.h` | `yakumono_count` groups (`MPLineInfo`: id + {first line, count} for floor, ceiling, right wall, left wall); a line is a polyline of vertex ids (`MPVertexLinks {first, count}` into the id array); a vertex is `{s16 x, y; u16 flags}`: `0x4000` pass-through, `0x8000` ledge, low byte = material. Yakumono *i* moves with DObj *i* of layer 1 (`mpCollisionAllocYakumono`) when that DObj is animated: its lines are relative to the DObj's translation. |
| Map objects | `MPMapObjData {u16 kind; s16 x, y}`, `MPMapObjKind` (`src/mp/mpdef.h`) | 0-3 player starts, 4 item spawns, 0x20 respawn point, ... |

**3D play.** SSB64 stages are 2D lines (x right, y up, the camera looks down -z). Each line segment is extruded into a
quad over a depth band `z` in `[-360, 360]` SM64 units (about 1030 SSB64 units, a little wider than a fighter's stance and
inside most stages' visible depth), wound so floors face up, ceilings down and right / left walls +x / -x: Mario's sim
(and anything using `CollisionWorld`) walks the stage's real surfaces. Pass-through platforms are just floor lines with no
ceiling or wall, so they are one-way like in SSB64 (jump through from below, land on top; dropping through on down is
not done); their triangles are listed in `passTriangles` (ledges in `ledgeTriangles`). Two invisible walls (surface `0x76`,
no camera collision) close the band at `z = +-360` across the whole blast zone, so a fighter knocked or walking toward the
camera or the background stays in the plane, also off stage. Scale: `K` = 0.348 SM64 units per SSB64 unit, the fighters'
common scale (Smash Mario 460 tall = SM64 Mario 160). The spawn is player 1's start dropped onto the floor below it,
facing the camera side (`faceYaw` 0x8000), so the follow camera starts in front of the stage like SSB64's.

**Blast zones / death.** `deathHeight` = the bottom blast line and `horizontalBoundary` = the nearer side blast line, so
falling or walking out kills through the sim's own void check (the top blast line only matters with knockback). The
`ssb64` block of the course json exposes `blast`, `camera`, `warning` (the whistle height), `players`, `items`, `rebirth`
(SM64 units, `z` = 0), `bgm`, `fog`, the yakumono list and the moving `parts`; `game::ssb64::Stage` reads it.

**Moving parts.** Every DObj whose animation changes its pose is cut out of the static mesh with its subtree:
- a moving yakumono of layer 1 (a platform with collision) becomes a surface object: its lines go to `extraCollision`
  (relative to the DObj), its model to `part_<id>.glb` (relative to its frame-0 translation) and its translation, one row per
  30 Hz tick, to the `track` of a `ssb64Yakumono` solid (`native/sim/src/solids/ssb64.rs`), which replays it in a loop and
  carries a rider (`apply_platform_displacement`). Working: Peach's Castle's sliding platform under the stage and its
  two moving blocks, Congo Jungle's two floating platforms, Planet Zebes's rising platform, Saffron City's two elevators,
  the test stage's two platforms and every moving platform of Board the Platforms (2-8 a stage).
- the others (scenery) stay in `stage.glb` under a `parts` node, one looping glTF clip each, played by
  `render::ssb64_stage`: Sector Z's passing ships, Yoshi's Island's turning and drifting background pieces, Mushroom Kingdom's
  background, Final Destination's spinning backdrop, the beta Dream Land's swaying trees.

Not done (code-driven in SSB64, `src/gr/grcommon/*.c`): Peach's Castle's bumper, Sector Z's Arwing and its lasers, Congo
Jungle's barrel cannon, Planet Zebes's acid (the rising and falling lake), Hyrule Castle's tornado, Yoshi's Island's
cloud platforms vanishing, Dream Land's Whispy wind, Saffron City's Pokémon door, Mushroom Kingdom's POW block / Piranha
Plants / scale platforms, the Break the Targets targets and the Board the Platforms goal platforms (the course is there,
the objects are not), the wallpaper (the 2D background image behind each stage; the sky uses the fog colour instead).

**In the game.** A SMASH group in the level select lists the imported stages (the retro LEVELS page: "SUPER SMASH BROS.",
"BREAK THE TARGETS", "BOARD THE PLATFORMS"; the hub's course grid too); any character, the Smash fighters included, can be
played there (`JUMPMAN_COURSE=ssb64_pupupu`). Other modes load one by id: `game::ssb64::load(assets, "pupupu")` (or
`"ssb64_pupupu"`) returns a `LoadedCourse`; `game::ssb64::stage(assets, id)` the `Stage` block; `STAGES`,
`battle_stages()`, `installed()` list them.

## Music

`scripts/export-ssb64-music.py` and its byte-identical port `native/byor-rs/src/ssb64_music.rs` write
`assets/audio/music/ssb64/`: `seq_NN.bin` (the 47 BGM sequences), `bank.ctl` / `bank.tbl` (the music instrument bank) and
`music.json` (id, name from `gmMusicID` in `src/gm/gmsound.h`, division, channels, volume; `menu` = main menu, character /
stage select, results, training, data, opening, credits, stage clear, game over; `victory` = each fighter's fanfare).

| What | Where | Format |
|---|---|---|
| Sequences | `S1_music.sbk` ROM `0xB277B0` (`smashbrothers.us.yaml`, `MUSIC_AND_SFX_DISCOVERIES.md`) | libultra `ALSeqFile` ('S1'): 47 compact-MIDI files (`ALCMidiHdr`: 16 track offsets + division 480), played by `n_alCSPlay`; BGM id = sequence index (`syAudioPlayBGM`). |
| Bank | `B1_sounds1.ctl` `0xB4E5C0`, `.tbl` `0xB54CE0` | `ALBankFile` ('B1'), 43 instruments + percussion, VADPCM, 32 kHz. |

It is the same format Banjo-Kazooie uses, so the game plays it with BK's software sequence player
(`bk_music::synth`, given SSB64's 32 kHz output rate): `native/src/ssb64_music.rs` plays each stage's `bgm` (from its map
header: Dream Land's theme on Dream Land, ...; Final Destination's header names the Master Hand theme) while a Smash stage
is loaded, stepping the SM64 music aside. The menu / victory themes are exported and named but nothing plays them yet (the
menus have no per-game music). Not modelled: reverb, the Mushroom Kingdom hurry theme, the hammer / star themes.
`cargo test -p jumpman ssb64_music` renders 15 themes (stage, menu, victory) and checks they are audible;
`JUMPMAN_SSB64_MUSIC_WAV=<dir>` writes 20 s WAVs of them.

Not done: weapon / effect animations (the ROM models are drawn static, see "Weapons, effects and items"), shields.
(Hitboxes from the motion scripts, knockback / percent, the specials in Gem Clash and the touch SPECIAL button are
done: see the sections above.)

## Weapons, effects and items

The import also writes the specials' weapon models, the main special effects and the item models from the ROM, and the
item sounds: `scripts/export-ssb64-weapons.py` (reference) and its byte-identical port `native/byor-rs/src/ssb64_weapons.rs`
(`ssb64::weapons`, run by `ssb64::run` and the pipeline after the sounds). It draws with the fighters' `Mesh` (display
lists, MObj materials, the combiner), so the material rules are the fighters'. Output (data dir):
`public/models/ssb64/objects/<name>.glb` (48 static models, about 0.4 MB), `public/models/ssb64/objects.json` and 34
item sounds `assets/audio/voices/ssb64/FGM*_0.wav` + `manifest-ssb64-items.json`. Python ~2 s, Rust < 0.5 s.

| What | Where in the decomp | Format |
|---|---|---|
| Weapons | `WPDesc` (`src/wp/wp<fighter>/*.c`, flags: 1 = DObjDesc tree, 2 = DLLinks) -> `WPAttributes` (`src/wp/wptypes.h`) in the fighter's `Main` / `Special1` file | `{data, p_mobjsubs, anim_joints, p_matanim_joints, ...}`: `data` is a DObjDesc tree or one display list (`wpManagerMakeWeapon`). Offsets from the `@ 0x....` notes of `src/relocData/2xx_*.c` (Mario / Luigi `Special1` + 0, Samus / Link / Ness / Pikachu / Yoshi `Main` + 0xC / 0x40, Kirby `Main` + 8, Pikachu `Special1` + 0 / 0x34). |
| Items | `ITAttributes` (`src/it/ittypes.h`), `itManagerMakeItem` (`src/it/itmanager.c`) | the same four pointers first; the common items in `ITCommonData` (file 251, the `ItemAttributes @` offsets of `251_ITCommonData.c`), models in `ITCommonObject` (86); Link's bomb `LinkMain` + 0x40, PK Fire's pillar `NessSpecial1` + 0x34. |
| Effects | `EFDesc` (`src/ef/eftypes.h`), `efManagerMakeEffect` (`src/ef/efmanager.c`) | file + offsets of the DObjDesc tree (flags & 4) or display list, the `MObjSub ***`, AnimJoint and MatAnimJoint tables (the `ll*` symbols: their offsets read from the `src/relocData/<file>.c` layouts and checked against the data). |
| Trees | `gcSetupCustomDObjs` / `gcSetupCustomDObjsWithMObj` (`src/sys/objanim.c`) | 44-byte `DObjDesc` like the fighters' (`id & 0xFFF` = depth, end id 18); a DObj's `dl` may be a `DObjDLLink` array (`{list_id, dl}` up to id 4, the XLU renderers). One `MObjSub **` list per DObj; the material script (MatAnimJoint) is applied at frame 0. |
| Renderer state | `itDisplayColAnimOPA`, `wpYoshiEggThrowProcDisplay` | they clear the environment colour first (`env0` in the table: Hammer, Bob-omb, Link's bomb, Yoshi's egg); `palette`: the Luigi fireball (palette 1, `wpMarioFireballMakeWeapon`) and the green shell (1) vs the red (0) share one model. |

Models (48): weapons `fireball`, `fireball_luigi`, `blaster`, `charge_shot`, `samus_bomb`, `boomerang`, `spin_attack`,
`final_cutter`, `pk_fire`, `pk_thunder`, `pk_thunder_trail`, `thunder_head`, `thunder_jolt`, `thunder_jolt_air`,
`yoshi_egg`, `yoshi_star` (also the Star Rod's star), `ray_gun_shot`; items `capsule`, `maxim_tomato`, `heart`, `star`,
`beam_sword`, `bat`, `fan`, `ray_gun`, `fire_flower`, `hammer`, `motion_bomb`, `bob_omb`, `star_rod`, `green_shell`,
`red_shell`, `crate`, `barrel`, `bumper`, `poke_ball`, `egg`, `link_bomb`, `pk_fire_pillar`; effects `falcon_punch`,
`falcon_kick`, `reflector`, `psi_magnet`, `sing`, `spin_attack_trail`, `thunder_shock`, `egg_lay`, `hit_slash`.

`<name>.glb`: one node, one mesh (a primitive per material), the bind pose baked in, positions in UNIT (0.004) per SSB64
unit, no skin and no animation. Many SSB64 items and weapons are sprite quads (`plane` in the index: the axis the model is
flat along). Vertex colours keep their alpha only where the combiner reads the shade alpha (sprites carry alpha 0 there).
Effects whose material script fades in from alpha 0 (`solid`: Sing, Spin Attack) are exported at full alpha.
`objects.json`: per model `kind` (weapon / item / effect), `file`, `source`, `joints`, `triangles`, `materials`,
`billboard` (the game draws it facing the camera), `plane`, `min` / `max`; `hands`: per fighter the joints that hold
items (`FTAttributes.joint_itemlight_id` +0x33C / `joint_itemheavy_id` +0x334, the `joint_NN` nodes of the fighter glb).

Not exported: joint animations (AnimJoint: the Reflector's spin-up, Bob-omb's walk, the egg-shell pieces sit at their
bind pose), texture flip-books after frame 0 (Falcon Punch's three frames), the particle-bank effects (explosions, fire
flower flames, sparks, smoke: `efcommon` `.scb` / `.txb`, see the decomp's `PARTICLE_BANK_DISCOVERIES.md`), Kirby's copy
hats, the Pokémon.

**In the game** (`native/src/characters/smash_models.rs`): the projectiles of `sim.mario.smash.projectiles` are drawn
with their model (camera-facing for the sprites, the boomerang spinning, Thunder's bolt as stacked heads, PK Thunder with
its tail, the Charge Shot sized by level), the running special's effect (Falcon Punch's bird and Falcon Kick's fire turned
about the facing toward the camera, the Reflector hexagon and PSI Magnet pulsing, Sing's notes, Link's spin, Pikachu's
Thunder), and an SSB64 hit spark on each hit. Weapons, effects and sprite items are unlit (pre-lit in SSB64), the solid
items lit. Anything without a model (no import, explosions, Fire Flower flames, blasts) keeps the particles of
`smash_fx.rs` (`SmashModels::covers`).

### Items (play)

`native/sim/src/smash_items.rs` (`jumpman_sim::smash::items`, engine-free, hooked like the specials: `step_smash` runs it
when no special runs, `after_step` steps the items; numbers from `src/it/itvars.h` and `251_ITCommonData.c`):

- **Spawning**: on the SM64 courses a Smash fighter plays (`Items::enabled`, set by the app; `JUMPMAN_SMASH_ITEMS=0`
  turns it off), an item drops from 300 units above a random floor point 200-700 units from the fighter every 11 s, up to
  4 on the field; untouched items blink and vanish after 30 s. `JUMPMAN_SMASH_ITEM=<name>` drops one in front at start.
- **Pick up**: B (attack) standing next to an item (`LightGet` / `HeavyGet`). Maxim Tomato (+4 wedges, SSB64 heals 100%)
  and Heart Container (full) are eaten; the Star is taken on touch: 10 s of invincibility (`ITSTAR_INVINCIBLE_TIME`,
  `SmashExt::armor`), sparkles. The Hammer is worn: 12 s (`ITHAMMER_TIME`) of `HammerWait` / `HammerWalk` with a hitbox
  coming down in front every 12 ticks (10%), B does nothing else.
- **Use** (B, stick near neutral): Beam Sword / Home-Run Bat / Fan / Star Rod swing (`*Swing1`, `*Swing3` while walking,
  `*Swing4` with Z held; damage 10 / 14 / 22 sword, 12 / 15 / 30 bat (the home run knocks hard), 2-4 fan, 6-9 star rod,
  approximate: the swing hitboxes are in the motion event scripts); the Star Rod's tilt / smash also shoot a star (8% /
  12%, 20 stars); Ray Gun shoots (`LGunShoot`, 16 shots, 10%); Fire Flower breathes fire while B is held (60 frames of
  fuel, 3% fire, particles).
- **Throw** (B with the stick pushed hard; heavy items, bombs, shells, Bumper, Poké Ball and containers on any B): the
  item flies toward the stick (`LightThrowF` / `LightThrowAirF` / `HeavyThrowF`) and hits the first body it meets with its
  ITAttributes damage. Bob-omb: explodes on contact / landing (30%, `ITAttackEvent` size 350); left on the ground it walks
  after 3 s and blows up on contact or after 5 s. Motion-Sensor Bomb: sticks where it lands, arms, explodes on the first
  body in its 180-unit sensor. Green / red shell: slides (18% / 10%), bounces off walls and bodies, stops. Bumper: stays
  and bumps (4%, every 10 ticks). Poké Ball: bursts open (12% burst; the Pokémon are not exported). Capsule / Crate /
  Barrel: break and drop another item.
- **Hits** go through the specials' path: thrown items, armed bombs, shells and the Bumper are `ProjKind::Item`
  projectiles kept on the item, swings and the hammer `Hitbox`es, shots projectiles, all with `SpecialMove::Item`;
  `smash::resolve` applies them to every `SmashTargets` (SM64 objects, goombas, Hold the Gate, Star Swarm), so a mode's
  knockback / percent applies to items unchanged.
- **Looks and sounds**: items on the field (sprite items face the camera, thrown ones spin), the held item on the
  fighter's item joint (`hands` of `objects.json`), the item motions (`LightGet`, `SwordSwing4`, `LGunShoot`,
  `HammerWalk`...) through `SmashExt::motion`; the item FGM clips on the item events (`smash_models::item_sounds`: get,
  throw, swings, shots, empty, bat / fan / shell hits, landing, explosion, container smash, Poké Ball, Motion-Sensor Bomb,
  heal, Star). No HUD change. Not done: the Star and Hammer music, items in Gem Clash / the minigames' own spawners, the
  red shell's homing, Bob-omb's flashing, per-fighter throw strength, smash throws, catching thrown items.

## Not done yet

- Costumes other than 0, face (texture part) swaps, hand model parts (open / closed fists), low-poly models, the polygon /
  metal / giant variants (NMario, MMario, GDonkey), accessories (Pikachu's hat, Jigglypuff's bow), playing the motion scripts' effects and sound timing (exported, not
  used yet), the throw / damage-thrown scripts and hurtbox events on the sim; music, stage / item / menu
  sounds and the announcer, item and weapon models, stages.

  metal / giant variants (NMario, MMario, GDonkey), accessories (Pikachu's hat, Jigglypuff's bow), hitboxes and the
  motion event scripts (when each sound plays, effects, hitbox timing in `*MainMotion` files); stage / item / menu
  sounds and the announcer, item and weapon models, the code-driven stage hazards (see Stages).

  metal / giant variants (NMario, MMario, GDonkey), accessories (Pikachu's hat, Jigglypuff's bow), hitboxes and the
  motion event scripts (when each sound plays, effects, hitbox timing in `*MainMotion` files); music, stage / menu
  sounds and the announcer, the weapon / effect / item joint animations and particle effects (see "Weapons, effects and
  items"), stages.
- The `TraI` spline track is ignored (no exported motion uses it visibly).
