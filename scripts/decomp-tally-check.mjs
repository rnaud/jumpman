#!/usr/bin/env node
// Re-counts the mechanical numbers behind /DECOMP_TALLY.md.
//
//   node scripts/decomp-tally-check.mjs            # human-readable report
//   node scripts/decomp-tally-check.mjs --json     # same numbers as JSON
//   MK64_DIR=/path/to/mk64 OOT_DIR=/path/to/oot BK_DIR=/path/to/banjo-kazooie node scripts/decomp-tally-check.mjs
//
// Decomp checkouts default to reference/{sm64,mk64,oot,banjo-kazooie}; a missing one is reported as
// "unavailable" and the matching tally rows keep their previous numbers (git submodule update --init reference/<name>).
// Dependency free (Node >= 18). Read-only: it never writes anything.
//
// Caveat on "name matches": a decomp function counts as matched when its snake_case or camelCase name appears as a
// whole word anywhere in the project sources. That is an UPPER bound on "ported" (a comment can mention a function
// that was only approximated) and a LOWER bound where a port renamed things. The tally explains which is which.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const wantJson = process.argv.includes('--json');
const out = {};
const lines = [];
const say = (s = '') => { lines.push(s); };
const pct = (a, b) => (b ? `${((100 * a) / b).toFixed(1)}%` : 'n/a');

const exists = (p) => fs.existsSync(p);
const read = (p) => (exists(p) ? fs.readFileSync(p, 'utf8') : '');
function walk(dir, exts, acc = []) {
  if (!exists(dir)) return acc;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === 'node_modules' || e.name === 'target' || e.name.startsWith('.git')) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, exts, acc);
    else if (exts.some((x) => e.name.endsWith(x))) acc.push(p);
  }
  return acc;
}
const dirOf = (envName, rel) => {
  const d = process.env[envName] ? path.resolve(process.env[envName]) : path.join(ROOT, rel);
  // A submodule that is not checked out is an empty directory.
  return exists(d) && fs.readdirSync(d).length > 0 ? d : null;
};
const camel = (s) => s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

// ---- project sources ---------------------------------------------------------------------------------------------
const projectFiles = [
  ...walk(path.join(ROOT, 'src'), ['.ts']),
  ...walk(path.join(ROOT, 'native/sim/src'), ['.rs']),
  ...walk(path.join(ROOT, 'native/src'), ['.rs']),
  ...walk(path.join(ROOT, 'scripts'), ['.py', '.mjs']),
].filter((f) => !f.endsWith('.test.ts') && !f.endsWith('decomp-tally-check.mjs'));
const projectText = projectFiles.map(read).join('\n');
const simText = [...walk(path.join(ROOT, 'src/sim'), ['.ts']), ...walk(path.join(ROOT, 'native/sim/src'), ['.rs'])].map(read).join('\n');
const words = (text) => new Set(text.match(/[A-Za-z_][A-Za-z0-9_]*/g) || []);
const projectWords = words(projectText);
const simWords = words(simText);

// Top-level C function names of a decomp file.
function cFunctions(file) {
  const t = read(file).replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  const names = new Set();
  const re = /^(?:static\s+)?(?:[A-Za-z_]\w*[\s*]+)+(\w+)\s*\([^;{}]*\)\s*\{/gm;
  for (let m; (m = re.exec(t)); ) if (!['if', 'while', 'for', 'switch', 'return'].includes(m[1])) names.add(m[1]);
  return [...names];
}
function matchRatio(names, wordSet) {
  const hit = names.filter((n) => wordSet.has(n) || wordSet.has(camel(n)));
  return { total: names.length, matched: hit.length };
}

// ---- Super Mario 64 ----------------------------------------------------------------------------------------------
const SM64 = dirOf('SM64_DIR', 'reference/sm64');
if (!SM64) {
  out.sm64 = 'unavailable';
} else {
  const sm = {};
  const header = read(path.join(SM64, 'include/sm64.h'));
  const decAct = new Map();
  for (const m of header.matchAll(/^#define (ACT_[A-Z0-9_]+)\s+(0x[0-9A-Fa-f]+)/gm)) {
    if (/^ACT_(FLAG_|GROUP_|ID_MASK)/.test(m[1])) continue;
    decAct.set(m[1], parseInt(m[2], 16));
  }
  const notFlag = (n) => !/^ACT_(FLAG_|GROUP_|ID_MASK)/.test(n);
  const tsState = read(path.join(ROOT, 'src/sim/state.ts'));
  const tsAct = new Set([...tsState.matchAll(/export const (ACT_[A-Z0-9_]+)\s*=/g)].map((m) => m[1]).filter(notFlag));
  const rsAct = new Set(
    walk(path.join(ROOT, 'native/sim/src'), ['.rs']).flatMap((f) => [...read(f).matchAll(/pub const (ACT_[A-Z0-9_]+)\s*:/g)].map((m) => m[1])).filter(notFlag),
  );
  // "handled" = referenced somewhere in the TS sim other than state.ts (a case label / table key / setAction call).
  const handleWords = words(walk(path.join(ROOT, 'src/sim'), ['.ts']).filter((f) => !f.endsWith('state.ts')).map(read).join('\n'));
  const groups = { 0x000: 'stationary', 0x040: 'moving', 0x080: 'airborne', 0x0c0: 'submerged', 0x100: 'cutscene', 0x140: 'automatic', 0x180: 'object' };
  const byGroup = {};
  const missing = [];
  for (const [name, val] of decAct) {
    const g = groups[val & 0x1c0];
    byGroup[g] ??= { decomp: 0, defined: 0, handled: 0 };
    byGroup[g].decomp++;
    if (tsAct.has(name)) byGroup[g].defined++;
    else missing.push(name);
    if (handleWords.has(name)) byGroup[g].handled++;
  }
  sm.actions = {
    decomp: decAct.size,
    tsDefined: [...decAct.keys()].filter((n) => tsAct.has(n)).length,
    rsDefined: [...decAct.keys()].filter((n) => rsAct.has(n)).length,
    handled: [...decAct.keys()].filter((n) => handleWords.has(n)).length,
    projectOnlyNames: [...tsAct].filter((n) => !decAct.has(n)),
    byGroup,
    missing,
  };

  const files = {
    'mario.c': 'src/game/mario.c', 'mario_step.c': 'src/game/mario_step.c', 'interaction.c': 'src/game/interaction.c',
    'mario_actions_stationary.c': 'src/game/mario_actions_stationary.c', 'mario_actions_moving.c': 'src/game/mario_actions_moving.c',
    'mario_actions_airborne.c': 'src/game/mario_actions_airborne.c', 'mario_actions_submerged.c': 'src/game/mario_actions_submerged.c',
    'mario_actions_cutscene.c': 'src/game/mario_actions_cutscene.c', 'mario_actions_automatic.c': 'src/game/mario_actions_automatic.c',
    'mario_actions_object.c': 'src/game/mario_actions_object.c', 'surface_collision.c': 'src/engine/surface_collision.c',
    'surface_load.c': 'src/engine/surface_load.c', 'platform_displacement.c': 'src/game/platform_displacement.c',
    'object_helpers.c': 'src/game/object_helpers.c', 'obj_behaviors.c': 'src/game/obj_behaviors.c', 'obj_behaviors_2.c': 'src/game/obj_behaviors_2.c',
    'object_collision.c': 'src/game/object_collision.c', 'camera.c': 'src/game/camera.c', 'hud.c': 'src/game/hud.c',
    'ingame_menu.c': 'src/game/ingame_menu.c', 'level_update.c': 'src/game/level_update.c', 'paintings.c': 'src/game/paintings.c',
    'mario_misc.c': 'src/game/mario_misc.c', 'envfx_snow.c': 'src/game/envfx_snow.c', 'envfx_bubbles.c': 'src/game/envfx_bubbles.c',
    'file_select.c': 'src/menu/file_select.c', 'star_select.c': 'src/menu/star_select.c', 'title_screen.c': 'src/menu/title_screen.c',
    'save_file.c': 'src/game/save_file.c', 'rendering_graph_node.c': 'src/game/rendering_graph_node.c',
    'behavior_script.c': 'src/engine/behavior_script.c', 'geo_layout.c': 'src/engine/geo_layout.c', 'math_util.c': 'src/engine/math_util.c',
  };
  sm.functions = {};
  for (const [label, rel] of Object.entries(files)) {
    // The sim ports keep the decomp's snake_case names in Rust (file for file), so the sim-only word set is the right
    // haystack there; app-layer files (camera, hud, menus...) are re-implemented, so their ratio is just name overlap.
    const simFile = label.startsWith('mario') || /surface|platform|interaction|math/.test(label);
    sm.functions[label] = matchRatio(cFunctions(path.join(SM64, rel)), simFile ? simWords : projectWords);
  }

  // Behaviors.
  const bhvData = read(path.join(SM64, 'data/behavior_data.c'));
  const bhvScripts = [...bhvData.matchAll(/^const BehaviorScript (bhv\w+)\[\]/gm)].map((m) => m[1]);
  const inc = fs.readdirSync(path.join(SM64, 'src/game/behaviors')).filter((f) => f.endsWith('.inc.c'));
  const kindUnion = read(path.join(ROOT, 'src/sim/object.ts')).match(/export type ObjectBehaviorKind =([^;]*);/);
  const tsKinds = kindUnion ? [...kindUnion[1].matchAll(/'(\w+)'/g)].map((m) => m[1]) : [];
  sm.behaviors = {
    behaviorScripts: bhvScripts.length,
    incFiles: inc.length,
    incFilesNamedInProject: inc.filter((f) => projectText.includes(f)).length,
    simObjectKinds: tsKinds.length,
    tsSimBehaviorFiles: walk(path.join(ROOT, 'src/sim/behaviors'), ['.ts']).length,
    rustSimBehaviorFiles: walk(path.join(ROOT, 'native/sim/src/behaviors'), ['.rs']).filter((f) => !f.endsWith('mod.rs')).length,
  };

  // Levels and placements.
  const levelDefs = read(path.join(SM64, 'levels/level_defines.h'));
  const levels = [...levelDefs.matchAll(/^\s*DEFINE_LEVEL\(\s*"[^"]*",\s*\w+,\s*\w+,\s*(\w+)/gm)].map((m) => m[1].toLowerCase());
  const exportedLevels = new Set();
  for (const f of fs.readdirSync(path.join(ROOT, 'assets/courses'))) {
    const stem = f.replace(/\.\d+\.json$/, '').replace(/\.json$/, '');
    if (exists(path.join(SM64, 'levels', stem))) exportedLevels.add(stem);
  }
  const exporterSrc = read(path.join(ROOT, 'scripts/export-courses.py'));
  const placed = new Map();
  for (const lvl of exportedLevels) {
    for (const line of read(path.join(SM64, 'levels', lvl, 'script.c')).split('\n')) {
      if (!/\bOBJECT(_WITH_ACTS)?\(/.test(line)) continue;
      const b = line.match(/\b(bhv\w+)/g);
      if (b) placed.set(b.at(-1), (placed.get(b.at(-1)) || 0) + 1);
    }
  }
  const perLevel = {};
  for (const lvl of [...exportedLevels].sort()) {
    let total = 0;
    let handled = 0;
    for (const line of read(path.join(SM64, 'levels', lvl, 'script.c')).split('\n')) {
      if (!/\bOBJECT(_WITH_ACTS)?\(/.test(line)) continue;
      const b = line.match(/\b(bhv\w+)/g);
      if (!b) continue;
      total++;
      if (exporterSrc.includes(b.at(-1))) handled++;
    }
    perLevel[lvl] = { placements: total, explicitlyHandled: handled };
  }
  const named = [...placed.keys()].filter((b) => exporterSrc.includes(b));
  sm.levels = {
    decompLevels: levels.length,
    exportedCount: exportedLevels.size,
    notExported: levels.filter((l) => !exportedLevels.has(l)),
    perLevel,
    placementsInExportedLevels: [...placed.values()].reduce((a, b) => a + b, 0),
    distinctBhvInPlacements: placed.size,
    distinctBhvNamedByExporter: named.length,
    placementsWithExplicitExporterHandling: named.reduce((a, b) => a + placed.get(b), 0),
  };

  // Camera, animation, audio, assets.
  const camH = read(path.join(SM64, 'src/game/camera.h'));
  sm.camera = {
    modes: [...camH.matchAll(/^#define CAMERA_MODE_\w+/gm)].length,
    cutscenes: [...camH.matchAll(/^#define CUTSCENE_\w+/gm)].length,
    functions: cFunctions(path.join(SM64, 'src/game/camera.c')).length,
  };
  const animIds = new Set([...read(path.join(SM64, 'include/mario_animation_ids.h')).matchAll(/\/\* (0x[0-9A-Fa-f]+) \*\/ MARIO_ANIM_/g)].map((m) => parseInt(m[1], 16)));
  const animSrc = read(path.join(ROOT, 'src/game/sm64Animation.ts'));
  const a0 = animSrc.indexOf('export const SM64_ANIM');
  const mapped = new Set([...animSrc.slice(a0, animSrc.indexOf('};', a0)).matchAll(/0x[0-9a-fA-F]+/g)].map((m) => parseInt(m[0], 16)));
  sm.animations = { decompMarioAnims: animIds.size, mappedInSm64AnimTable: [...mapped].filter((i) => animIds.has(i)).length };
  const manifest = JSON.parse(read(path.join(ROOT, 'assets/audio/manifest.json')) || '{}');
  const sounds = JSON.parse(read(path.join(ROOT, 'assets/audio/music/sounds.json')) || '{}');
  sm.audio = {
    decompSequenceIds: [...(read(path.join(SM64, 'include/seq_ids.h')).match(/enum SeqId \{([^}]*)\}/)?.[1] || '').matchAll(/^\s*SEQ_\w+/gm)].length,
    extractedSampleWavs: Object.keys(manifest).length,
    soundNamesInSoundsJson: Object.keys(sounds).length,
  };
  const surfaceDefs = [...read(path.join(SM64, 'include/surface_terrains.h')).matchAll(/^#define (SURFACE_\w+)\s+0x/gm)].map((m) => m[1]);
  sm.surfaces = { decompSurfaceTypes: surfaceDefs.length, namedInSim: surfaceDefs.filter((s) => simWords.has(s) || simWords.has(s.toLowerCase())).length };
  const dirs = (p) => fs.readdirSync(p, { withFileTypes: true }).filter((e) => e.isDirectory()).length;
  sm.actors = { decompActorDirs: dirs(path.join(SM64, 'actors')), exportedActorDirs: dirs(path.join(ROOT, 'assets/actors')) };
  out.sm64 = sm;
}

// ---- Mario Kart 64 -----------------------------------------------------------------------------------------------
const MK = dirOf('MK64_DIR', 'reference/mk64');
if (!MK) {
  out.mk64 = 'unavailable';
} else {
  const mk = {};
  const kartText = [...walk(path.join(ROOT, 'src/game/experiments'), ['.ts']), ...walk(path.join(ROOT, 'native/src/experiments'), ['.rs'])]
    .filter((f) => /kart/i.test(path.basename(f)))
    .map(read).join('\n');
  const kartWords = words(kartText);
  const courseDirs = fs.readdirSync(path.join(MK, 'courses'), { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name);
  const battle = ['big_donut', 'block_fort', 'double_deck', 'skyscraper'];
  const exported = exists(path.join(ROOT, 'public/mk64'))
    ? fs.readdirSync(path.join(ROOT, 'public/mk64')).filter((f) => f.endsWith('.json')).map((f) => f.replace('.json', '').replace(/-/g, '_'))
    : [];
  mk.courses = { dirs: courseDirs.length, raceCourses: courseDirs.filter((c) => !battle.includes(c)).length, battleCourses: battle.length, exported, notExported: courseDirs.filter((c) => !exported.includes(c)) };
  mk.functions = {};
  for (const [label, rel] of Object.entries({
    'player_controller.c': 'src/player_controller.c', 'camera.c': 'src/camera.c', 'racing/collision.c': 'src/racing/collision.c',
    'code_80057C60.c (items/objects)': 'src/code_80057C60.c', 'update_objects.c': 'src/update_objects.c',
    'cpu_vehicles_camera_path.c': 'src/cpu_vehicles_camera_path.c', 'racing/race_logic.c': 'src/racing/race_logic.c', 'racing/actors.c': 'src/racing/actors.c',
    'effects.c': 'src/effects.c', 'menus.c': 'src/menus.c', 'menu_items.c': 'src/menu_items.c', 'replays.c': 'src/replays.c', 'save.c': 'src/save.c',
  })) mk.functions[label] = matchRatio(cFunctions(path.join(MK, rel)), kartWords);
  const defs = read(path.join(MK, 'include/defines.h'));
  mk.items = { decompItems: [...defs.matchAll(/^\s*\/\* 0x[0-9A-F]+ \*\/ ITEM_(?!NONE)\w+/gm)].length, implementedInProject: 3, note: 'mushroom, banana, green shell (kartRace.ts / kart_race.rs); this count is manual' };
  mk.actorTypes = [...read(path.join(MK, 'include/actor_types.h')).matchAll(/^\s*(?:\/\* 0x[0-9A-F]+ \*\/\s*)?ACTOR_\w+\s*(?:=|,)/gm)].length;
  out.mk64 = mk;
}

// ---- Ocarina of Time ---------------------------------------------------------------------------------------------
const OOT = dirOf('OOT_DIR', 'reference/oot');
if (!OOT) {
  out.oot = 'unavailable';
} else {
  const oot = {};
  const scenes = [...read(path.join(OOT, 'include/tables/scene_table.h')).matchAll(/^\s*(?:\/\*[^*]*\*\/\s*)?DEFINE_SCENE\(\s*(\w+)_scene/gm)].map((m) => m[1]);
  const exportedScenes = [...read(path.join(ROOT, 'scripts/export-oot-courses.py')).matchAll(/'[\w.]+':\s*\('(\w+)',\s*\d+\)/g)].map((m) => m[1]);
  oot.scenes = { decomp: scenes.length, exported: new Set(exportedScenes).size, exportedNames: [...new Set(exportedScenes)].sort() };
  const actorDir = path.join(OOT, 'src/overlays/actors');
  const ovls = exists(actorDir) ? fs.readdirSync(actorDir).filter((f) => f.startsWith('ovl_')) : [];
  oot.actors = {
    overlays: ovls.length,
    bosses: ovls.filter((f) => f.startsWith('ovl_Boss_')).length,
    enemyOverlays: ovls.filter((f) => f.startsWith('ovl_En_')).length,
    bgOverlays: ovls.filter((f) => f.startsWith('ovl_Bg_')).length,
  };
  const playerC = read(path.join(OOT, 'src/overlays/actors/ovl_player_actor/z_player.c'));
  const playerActions = [...new Set([...playerC.matchAll(/\bvoid (Player_Action_\w+)\(/g)].map((m) => m[1]))];
  oot.player = { actionFunctions: playerActions.length, namedInProject: playerActions.filter((n) => projectWords.has(n)).length, linkControllerLines: read(path.join(ROOT, 'src/sim/linkController.ts')).split('\n').length };
  out.oot = oot;
}

// ---- Banjo-Kazooie -----------------------------------------------------------------------------------------------
const BK = dirOf('BK_DIR', 'reference/banjo-kazooie');
if (!BK) {
  out.bk = 'unavailable';
} else {
  const bk = {};
  const states = [...new Set([...read(path.join(BK, 'include/enums.h')).matchAll(/^\s*(BS_[0-9A-Fa-f]+_\w+)/gm)].map((m) => m[1]))];
  const bsFiles = fs.readdirSync(path.join(BK, 'src/core2/bs')).filter((f) => f.endsWith('.c'));
  const banjoSrc = [read(path.join(ROOT, 'src/sim/banjoController.ts')), read(path.join(ROOT, 'native/sim/src/characters.rs'))].join('\n');
  bk.states = { decompBsStates: states.length, bsSourceFiles: bsFiles.length, bsFilesNamedInPort: bsFiles.filter((f) => banjoSrc.toLowerCase().includes(`bs${f.toLowerCase()}`)).length };
  const chDir = path.join(BK, 'src/core2/ch');
  bk.characters = { chFiles: exists(chDir) ? fs.readdirSync(chDir).filter((f) => f.endsWith('.c')).length : 0 };
  bk.worlds = ['BGS', 'CC', 'CCW', 'FP', 'GV', 'MM', 'MMM', 'RBB', 'SM', 'TTC', 'lair'].filter((d) => exists(path.join(BK, 'src', d)));
  bk.worldsExported = 0; // Banjo plays SM64/OoT courses; no BK level geometry is exported.
  out.bk = bk;
}

// ---- other references --------------------------------------------------------------------------------------------
out.referenceDir = exists(path.join(ROOT, 'reference'))
  ? fs.readdirSync(path.join(ROOT, 'reference')).map((n) => {
      const p = path.join(ROOT, 'reference', n);
      return fs.statSync(p).isDirectory() ? `${n}${fs.readdirSync(p).length ? '' : ' (empty submodule)'}` : n;
    })
  : [];

// ---- report ------------------------------------------------------------------------------------------------------
if (wantJson) {
  console.log(JSON.stringify(out, null, 2));
  process.exit(0);
}
const sm = out.sm64;
if (typeof sm === 'object') {
  say('== Super Mario 64 (reference/sm64) ==');
  const a = sm.actions;
  say(`Actions: decomp ${a.decomp}; defined in TS sim ${a.tsDefined} (${pct(a.tsDefined, a.decomp)}); defined in Rust sim ${a.rsDefined}; referenced by a handler/transition ${a.handled} (${pct(a.handled, a.decomp)})`);
  for (const [g, v] of Object.entries(a.byGroup)) say(`  ${g.padEnd(11)} decomp ${String(v.decomp).padStart(3)}  defined ${String(v.defined).padStart(3)}  handled ${String(v.handled).padStart(3)}`);
  say(`  project-only ACT names: ${a.projectOnlyNames.join(' ') || '(none)'}`);
  say(`  not defined in the sim (${a.missing.length}): ${a.missing.join(' ')}`);
  say('Function-name matches (decomp function name found in project source; see caveat at top of this script):');
  for (const [k, v] of Object.entries(sm.functions)) say(`  ${k.padEnd(28)} ${String(v.matched).padStart(4)} / ${String(v.total).padEnd(4)} ${pct(v.matched, v.total)}`);
  const b = sm.behaviors;
  say(`Behaviors: ${b.behaviorScripts} BehaviorScripts, ${b.incFiles} behaviors/*.inc.c files (${b.incFilesNamedInProject} named in project sources), sim ObjectBehaviorKinds ${b.simObjectKinds}, behavior files TS ${b.tsSimBehaviorFiles} / Rust ${b.rustSimBehaviorFiles}`);
  const l = sm.levels;
  say(`Levels: ${l.decompLevels} decomp levels, ${l.exportedCount} exported as courses (${pct(l.exportedCount, l.decompLevels)}); not exported: ${l.notExported.join(' ')}`);
  say(`  OBJECT placements in the exported levels' script.c: ${l.placementsInExportedLevels} (${l.distinctBhvInPlacements} distinct behaviors); behaviors the exporter names explicitly: ${l.distinctBhvNamedByExporter} (${l.placementsWithExplicitExporterHandling} placements, ${pct(l.placementsWithExplicitExporterHandling, l.placementsInExportedLevels)})`);
  say('  per level (OBJECT placements / explicitly named by the exporter): ' + Object.entries(l.perLevel).map(([k, v]) => `${k} ${v.placements}/${v.explicitlyHandled}`).join(', '));
  say(`Camera: ${sm.camera.modes} CAMERA_MODEs, ${sm.camera.cutscenes} CUTSCENE_ ids, ${sm.camera.functions} functions in camera.c`);
  say(`Animations: ${sm.animations.decompMarioAnims} Mario anims; ${sm.animations.mappedInSm64AnimTable} mapped in SM64_ANIM`);
  say(`Audio: ${sm.audio.decompSequenceIds} sequence ids; ${sm.audio.extractedSampleWavs} sample WAVs in assets/audio/manifest.json; ${sm.audio.soundNamesInSoundsJson} names in sounds.json`);
  say(`Surfaces: ${sm.surfaces.decompSurfaceTypes} SURFACE_ types, ${sm.surfaces.namedInSim} named in the sim`);
  say(`Actor dirs: decomp ${sm.actors.decompActorDirs}, exported ${sm.actors.exportedActorDirs}`);
} else say('== Super Mario 64: reference/sm64 not found ==');
const mk = out.mk64;
say();
if (typeof mk === 'object') {
  say('== Mario Kart 64 (reference/mk64) ==');
  say(`Courses: ${mk.courses.dirs} dirs (${mk.courses.raceCourses} race + ${mk.courses.battleCourses} battle); exported: ${mk.courses.exported.join(', ') || 'none'}`);
  for (const [k, v] of Object.entries(mk.functions)) say(`  ${k.padEnd(34)} ${String(v.matched).padStart(4)} / ${String(v.total).padEnd(4)} ${pct(v.matched, v.total)}`);
  say(`Items: ${mk.items.decompItems} in the decomp, ${mk.items.implementedInProject} in the project (${mk.items.note}); actor types ${mk.actorTypes}`);
} else say('== Mario Kart 64: reference/mk64 not checked out (git submodule update --init reference/mk64 or set MK64_DIR) ==');
const oot = out.oot;
say();
if (typeof oot === 'object') {
  say('== Ocarina of Time (reference/oot) ==');
  say(`Scenes: ${oot.scenes.decomp} in the decomp, ${oot.scenes.exported} exported (${pct(oot.scenes.exported, oot.scenes.decomp)}): ${oot.scenes.exportedNames.join(' ')}`);
  say(`Actor overlays: ${oot.actors.overlays} (${oot.actors.enemyOverlays} En_, ${oot.actors.bgOverlays} Bg_, ${oot.actors.bosses} Boss_)`);
  say(`Player: ${oot.player.actionFunctions} Player_Action_* functions, ${oot.player.namedInProject} named in the project; src/sim/linkController.ts is ${oot.player.linkControllerLines} lines`);
} else say('== Ocarina of Time: reference/oot not checked out (git submodule update --init reference/oot or set OOT_DIR) ==');
const bk = out.bk;
say();
if (typeof bk === 'object') {
  say('== Banjo-Kazooie (reference/banjo-kazooie) ==');
  say(`Banjo states: ${bk.states.decompBsStates} BS_ ids, ${bk.states.bsSourceFiles} core2/bs/*.c files (${bk.states.bsFilesNamedInPort} named in the port); ${bk.characters.chFiles} core2/ch/*.c enemy/NPC files; world dirs: ${bk.worlds.join(' ')}; worlds exported: ${bk.worldsExported}`);
} else say('== Banjo-Kazooie: reference/banjo-kazooie not checked out (git submodule update --init reference/banjo-kazooie or set BK_DIR) ==');
say();
say(`reference/: ${out.referenceDir.join(', ')}`);
console.log(lines.join('\n'));
