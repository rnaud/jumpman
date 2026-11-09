//! Port of `scripts/export-oot-enemies.py`: enemy models (`assets/levels/oot_enemies/<kind>.glb`) and the per-course
//! placement files (`<course>.json`: enemies, spawners, collectibles, environment). The Rust damage tables the Python
//! script also writes into `native/sim` are a developer artefact and not part of an import.
use super::courses::courses;
use super::model::*;
use super::*;
use crate::pyc::{dumps, re};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

struct Model {
    kind: &'static str,
    bank: &'static str,
    skel: &'static str,
    idle: &'static str,
    scale: f64,
    textures: &'static [&'static str],
    clips: &'static [(&'static str, &'static str)],
}

const fn m(kind: &'static str, bank: &'static str, skel: &'static str, idle: &'static str, scale: f64, textures: &'static [&'static str], clips: &'static [(&'static str, &'static str)]) -> Model {
    Model { kind, bank, skel, idle, scale, textures, clips }
}

const MODELS: [Model; 27] = [
    m("deku_baba", "object_dekubaba", "gDekubabaHeadSkel", "gDekubabaChompAnim", 0.01, &[], &[("Wait", "gDekubabaIdleAnim")]),
    m(
        "goma_larva",
        "object_gol",
        "gObjectGolSkel",
        "gObjectGolStandAnim",
        0.01,
        &[],
        &[("Run", "gObjectGolRunningAnim"), ("Jump", "gObjectGolJumpHeadbuttAnim"), ("Prepare", "gObjectGolPrepareJumpAnim"), ("Land", "gObjectGolLandFromJumpAnim"), ("Damaged", "gObjectGolDamagedAnim"), ("Death", "gObjectGolDeathAnim"), ("Walk", "gObjectGolWalkAnim")],
    ),
    m("skulltula", "object_st", "object_st_Skel_005298", "object_st_Anim_000304", 0.04, &[], &[("Anim55A8", "object_st_Anim_0055A8"), ("Anim5B98", "object_st_Anim_005B98")]),
    m("peahat", "object_peehat", "gPeehatSkel", "gPeehatFlyingAnim", 0.036, &[], &[("Landing", "gPeehatLandingAnim"), ("Recoil", "gPeehatRecoilAnim"), ("Rising", "gPeehatRisingAnim")]),
    m(
        "tektite",
        "object_tite",
        "object_tite_Skel_003A20",
        "object_tite_Anim_0012E4",
        0.01,
        &["object_tite_Tex_001300", "object_tite_Tex_001700", "object_tite_Tex_001900"],
        &[("Attack", "object_tite_Anim_00083C"), ("Walk", "object_tite_Anim_000A14"), ("Land", "object_tite_Anim_000C70"), ("Anim69C", "object_tite_Anim_00069C"), ("Anim4F8", "object_tite_Anim_0004F8")],
    ),
    m("red_tektite", "object_tite", "object_tite_Skel_003A20", "object_tite_Anim_0012E4", 0.01, &["object_tite_Tex_001B00", "object_tite_Tex_001F00", "object_tite_Tex_002100"], &[]),
    m("octorok", "object_okuta", "gOctorokSkel", "gOctorokFloatAnim", 0.01, &[], &[("Shoot", "gOctorokShootAnim"), ("Die", "gOctorokDieAnim"), ("Hide", "gOctorokHideAnim"), ("Hit", "gOctorokHitAnim"), ("Appear", "gOctorokAppearAnim")]),
    m(
        "mad_scrub",
        "object_dekunuts",
        "gDekuNutsSkel",
        "gDekuNutsStandAnim",
        0.01,
        &[],
        &[("Spit", "gDekuNutsSpitAnim"), ("Damage", "gDekuNutsDamageAnim"), ("Burrow", "gDekuNutsBurrowAnim"), ("Unburrow", "gDekuNutsUnburrowAnim"), ("Die", "gDekuNutsDieAnim"), ("Run", "gDekuNutsRunAnim"), ("Look", "gDekuNutsLookAroundAnim")],
    ),
    m(
        "wolfos",
        "object_wf",
        "gWolfosGraySkel",
        "gWolfosIdleAnim",
        0.0075,
        &["gWolfosGrayEyeOpenTex"],
        &[("Run", "gWolfosRunAnim"), ("Attack", "gWolfosAttackAnim"), ("Backflip", "gWolfosBackflipAnim"), ("Block", "gWolfosBlockAnim"), ("Damaged", "gWolfosDamagedAnim"), ("Side", "gWolfosSideStepAnim"), ("Howl", "gWolfosHowlAnim")],
    ),
    m("guay", "object_crow", "gGuaySkel", "gGuayFlyAnim", 0.01, &[], &[]),
    m("baby_dodongo", "object_dodojr", "object_dodojr_Skel_0020E0", "object_dodojr_Anim_0009D4", 0.02, &[], &[("Crawl", "object_dodojr_Anim_0005F0")]),
    m("armos", "object_am", "gArmosSkel", "gArmosHopAnim", 0.01, &[], &[("Damaged", "gArmosDamagedAnim"), ("Ricochet", "gArmosRicochetAnim")]),
    m("king_dodongo", "object_kingdodongo", "object_kingdodongo_Skel_01B310", "object_kingdodongo_Anim_00F0D8", 0.01, &[], &[("Death", "object_kingdodongo_Anim_002D0C"), ("LayDown", "object_kingdodongo_Anim_004E0C")]),
    m("stinger", "object_ei", "gStingerSkel", "gStingerIdleAnim", 0.01, &[], &[("Dive", "gStingerDiveAnim"), ("PopOut", "gStingerPopOutAnim"), ("Hit", "gStingerHitAnim")]),
    m("barinade", "object_bv", "gBarinadeBodySkel", "gBarinadeBodyAnim", 0.1, &[], &[]),
    m("phantom_ganon", "object_gnd", "gPhantomGanonSkel", "gPhantomGanonNeutralAnim", 0.01, &[], &[("Throw", "gPhantomGanonThrowAnim"), ("Block", "gPhantomGanonBlockAnim")]),
    m("shell_blade", "object_sb", "object_sb_Skel_002BF0", "object_sb_Anim_00004C", 0.006, &[], &[]),
    m("beamos", "object_vm", "gBeamosSkel", "gBeamosAnim", 0.014, &[], &[]),
    m("redead", "object_rd", "gRedeadSkel", "gGibdoRedeadIdleAnim", 0.01, &[], &[("Grab", "gGibdoRedeadGrabAttackAnim"), ("Death", "gGibdoRedeadDeathAnim")]),
    m("lizalfos", "object_zf", "gLizalfosSkel", "gZfIdleAnim", 0.015, &[], &[("Walk", "gZfWalkAnim"), ("Slash", "gZfSlashAnim"), ("Jump", "gZfJumpAnim")]),
    m("flame_skull", "object_Bb", "object_Bb_Skel_001A30", "object_Bb_Anim_000444", 0.01, &[], &[]),
    m("torch_slug", "object_bw", "gTorchSlugSkel", "gTorchSlugEyestalkWaveAnim", 0.013, &[], &[("Raise", "gTorchSlugEyestalkRaiseAnim"), ("Flail", "gTorchSlugEyestalkFlailAnim")]),
    m("wallmaster", "object_wallmaster", "gWallmasterSkel", "gWallmasterWaitAnim", 0.01, &[], &[("Walk", "gWallmasterWalkAnim"), ("Jump", "gWallmasterJumpAnim")]),
    m("stalfos", "object_sk2", "gStalfosSkel", "gStalfosMiddleGuardAnim", 0.015, &[], &[("Block", "gStalfosBlockWithShieldAnim"), ("Advance", "gStalfosFastAdvanceAnim"), ("Flinch", "gStalfosFlinchFromHitBehindAnim")]),
    m("iron_knuckle", "object_ik", "gIronKnuckleSkel", "gIronKnuckleRunAnim", 0.012, &[], &[("Block", "gIronKnuckleBlockAnim"), ("Vertical", "gIronKnuckleVerticalAttackAnim"), ("Horizontal", "gIronKnuckleHorizontalAttackAnim"), ("FrontHit", "gIronKnuckleFrontHitAnim")]),
    m("biri", "object_bl", "gBiriSkel", "gBiriMainAnim", 0.01, &[], &[]),
    m("keese", "object_firefly", "gKeeseSkel", "gKeeseFlyAnim", 0.01, &[], &[]),
];

fn leever() -> Model {
    m("leever", "object_reeba", "object_reeba_Skel_001EE8", "object_reeba_Anim_0001E4", 0.04, &[], &[])
}

fn actor_kind(actor: &str) -> Option<&'static str> {
    Some(match actor {
        "ACTOR_EN_DEKUBABA" => "deku_baba",
        "ACTOR_EN_KAREBABA" => "withered_baba",
        "ACTOR_EN_GOMA" => "goma_larva",
        "ACTOR_EN_ST" => "skulltula",
        "ACTOR_EN_SW" => "gold_skulltula",
        "ACTOR_EN_PEEHAT" => "peahat",
        "ACTOR_EN_TITE" => "tektite",
        "ACTOR_EN_OKUTA" => "octorok",
        "ACTOR_EN_DEKUNUTS" => "mad_scrub",
        "ACTOR_EN_WF" => "wolfos",
        "ACTOR_EN_CROW" => "guay",
        "ACTOR_EN_FIREFLY" => "keese",
        "ACTOR_EN_DODOJR" => "baby_dodongo",
        "ACTOR_EN_BILI" => "biri",
        "ACTOR_BOSS_DODONGO" => "king_dodongo",
        "ACTOR_EN_AM" => "armos",
        "ACTOR_EN_BUBBLE" => "shabom",
        "ACTOR_EN_EIYER" | "ACTOR_EN_WEIYER" => "stinger",
        "ACTOR_EN_SB" => "shell_blade",
        "ACTOR_EN_VM" => "beamos",
        "ACTOR_EN_RD" => "redead",
        "ACTOR_EN_ZF" => "lizalfos",
        "ACTOR_EN_BB" => "flame_skull",
        "ACTOR_EN_BW" => "torch_slug",
        "ACTOR_EN_FZ" => "freezard",
        "ACTOR_EN_RR" => "like_like",
        "ACTOR_EN_TEST" => "stalfos",
        "ACTOR_EN_IK" => "iron_knuckle",
        "ACTOR_EN_NY" => "spike",
        "ACTOR_EN_WALLMAS" => "wallmaster",
        "ACTOR_BOSS_VA" => "barinade",
        "ACTOR_BOSS_GANONDROF" => "phantom_ganon",
        "ACTOR_EN_REEBA" => "leever",
        _ => return None,
    })
}

fn npc_like(actor: &str) -> bool {
    matches!(actor, "ACTOR_EN_HINTNUTS" | "ACTOR_EN_SHOPNUTS" | "ACTOR_EN_SKJ" | "ACTOR_EN_ENCOUNT2")
}

fn kind_for(actor: &str, param: i64) -> &'static str {
    match actor {
        "ACTOR_EN_DEKUBABA" => {
            if param == 1 {
                "big_deku_baba"
            } else {
                "deku_baba"
            }
        }
        "ACTOR_EN_TITE" => {
            if param == -1 {
                "red_tektite"
            } else {
                "tektite"
            }
        }
        "ACTOR_EN_WF" => "wolfos",
        "ACTOR_EN_PEEHAT" => {
            if param == 1 {
                "peahat_larva"
            } else {
                "peahat"
            }
        }
        _ => actor_kind(actor).unwrap(),
    }
}

/// `actor_categories()`: overlay id -> category from the `ActorProfile` of every overlay source.
fn actor_categories(ctx: &Ctx) -> HashMap<String, String> {
    let mut cats = HashMap::new();
    let pat = re(r"/\*\*/ (ACTOR_\w+),\s*/\*\*/ (ACTORCAT_\w+)");
    for sub in ["src/overlays/actors", "src/overlays/bosses"] {
        let Ok(rd) = std::fs::read_dir(ctx.decomp.join(sub)) else { continue };
        let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            let Ok(files) = std::fs::read_dir(&d) else { continue };
            let mut files: Vec<_> = files.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("z_") && n.to_string_lossy().ends_with(".c"))).collect();
            files.sort();
            for f in files {
                if let Ok(bytes) = std::fs::read(&f) {
                    if let Some(c) = pat.captures(&String::from_utf8_lossy(&bytes)) {
                        cats.insert(c[1].to_string(), c[2].to_string());
                    }
                }
            }
        }
    }
    cats
}

/// `scene_environment(scene_data)`
fn scene_environment(scene: &[u8]) -> Value {
    let (mut light_mode, mut skybox_id, mut skybox_config, mut hour, mut min, mut time_speed) = (0u8, 0u8, 0u8, 255u8, 255u8, 255u8);
    let mut lights: Vec<Value> = Vec::new();
    let mut music = json!({"seq": 255, "nature": 255});
    let mut pos = 0;
    while pos + 8 <= scene.len() {
        let (op, count, a, b, c) = (scene[pos], scene[pos + 1] as usize, scene[pos + 4], scene[pos + 5], scene[pos + 6]);
        let pointer = (u32_(scene, pos + 4) & MASK) as usize;
        if op == 0x14 {
            break;
        }
        if op == 0x10 {
            hour = a;
            min = b;
            time_speed = c;
        } else if op == 0x11 {
            skybox_id = a;
            skybox_config = b;
            light_mode = c;
        } else if op == 0x15 {
            music = json!({"seq": scene[pos + 7], "nature": scene[pos + 6], "spec": count});
        } else if op == 0x0F {
            for i in 0..count {
                let q = pointer + 22 * i;
                let u = |o: usize| scene[q + o] as i64;
                let s = |o: usize| scene[q + o] as i8 as i64;
                let packed = i16_(scene, q + 18) as i64;
                lights.push(json!({
                    "ambient": [u(0), u(1), u(2)],
                    "light1Dir": [s(3), s(4), s(5)],
                    "light1Color": [u(6), u(7), u(8)],
                    "light2Dir": [s(9), s(10), s(11)],
                    "light2Color": [u(12), u(13), u(14)],
                    "fogColor": [u(15), u(16), u(17)],
                    "blendRate": (packed >> 10) & 0x3F,
                    "fogNear": packed & 0x3FF,
                    "zFar": i16_(scene, q + 20),
                }));
            }
        }
        pos += 8;
    }
    json!({"lightMode": light_mode, "skyboxId": skybox_id, "skyboxConfig": skybox_config, "hour": hour, "min": min, "timeSpeed": time_speed, "lights": lights, "music": music})
}

pub fn export_all(ctx: &Ctx, stage: &Path) -> R<()> {
    let assets = stage.join("assets");
    let out = assets.join("levels/oot_enemies");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let table = ctx.read_decomp("include/tables/actor_table.h")?;
    let ids: HashMap<usize, String> = re(r"/\* (0x[0-9A-Fa-f]+) \*/ DEFINE_ACTOR\w*\(\w+, (\w+),").captures_iter(&table).map(|c| (hex(&c[1]), c[2].to_string())).collect();
    let cats = actor_categories(ctx);
    let mut enemyish: HashSet<String> = cats.iter().filter(|(_, v)| *v == "ACTORCAT_ENEMY" || *v == "ACTORCAT_BOSS").map(|(k, _)| k.clone()).collect();
    enemyish.extend(["ACTOR_EN_SW", "ACTOR_EN_GOMA", "ACTOR_EN_REEBA"].map(String::from));
    let names = ctx.names().to_vec();
    for (course, scene, _spawn) in courses() {
        let scene_data = ctx.segment(&format!("{scene}_scene"))?;
        let header = commands(&scene_data)?;
        let col = header.get(&3).ok_or("KeyError: 3")?.1 as usize;
        let (xmin, zmin, xmax, zmax) = (i16_(&scene_data, col) as i64, i16_(&scene_data, col + 4) as i64, i16_(&scene_data, col + 6) as i64, i16_(&scene_data, col + 10) as i64);
        let (cx, cz) = ((xmin + xmax) as f64 / 2.0, (zmin + zmax) as f64 / 2.0);
        let tf = |x: f64, y: f64, z: f64| [round_to((x - cx) * 4.4, 2), round_to(y * 4.4, 2), round_to((z - cz) * 4.4, 2)];
        let (mut enemies, mut spawners, mut collectibles): (Vec<Value>, Vec<Value>, Vec<Value>) = (vec![], vec![], vec![]);
        let mut skipped: Map<String, Value> = Map::new();
        let environment = scene_environment(&scene_data);
        // Everything else the rooms place (doors, switches, blocks, chests...), and the scene's transition actors (doors).
        let mut mechanisms: Vec<Value> = Vec::new();
        if let Some(&(dcount, dptr)) = header.get(&14) {
            for i in 0..dcount as usize {
                let q = dptr as usize + 16 * i;
                let id = u16_(&scene_data, q + 4) as usize;
                let name = ids.get(&id).cloned().unwrap_or_else(|| format!("{id:#x}"));
                let (x, y, z) = (i16_(&scene_data, q + 6) as f64, i16_(&scene_data, q + 8) as f64, i16_(&scene_data, q + 10) as f64);
                mechanisms.push(json!({"actor": name, "param": i16_(&scene_data, q + 14), "pos": tf(x, y, z), "rot": [0, i16_(&scene_data, q + 12), 0], "transition": [scene_data[q], scene_data[q + 2]], "index": i}));
            }
        }
        for name in &names {
            if !name.starts_with(&format!("{scene}_room_")) {
                continue;
            }
            let room = ctx.segment(name)?;
            let (count, actor_list) = commands(&room)?.get(&1).copied().unwrap_or((0, 0));
            for i in 0..count as usize {
                let q = actor_list as usize + 16 * i;
                let actor_id = u16_(&room, q) as usize;
                let (ax, ay, az, rx, ry, rz, param) = (i16_(&room, q + 2) as i64, i16_(&room, q + 4) as i64, i16_(&room, q + 6) as i64, i16_(&room, q + 8) as i64, i16_(&room, q + 10) as i64, i16_(&room, q + 12) as i64, i16_(&room, q + 14) as i64);
                let actor = ids.get(&actor_id).cloned().unwrap_or_else(|| format!("{actor_id:#x}"));
                let pos = tf(ax as f64, ay as f64, az as f64);
                if actor_kind(&actor).is_some() {
                    enemies.push(json!({"kind": kind_for(&actor, param), "actor": actor_id, "param": param, "pos": pos, "yaw": (ry * 180) as f64 / 32768.0, "rot": [rx, ry, rz], "room": name, "index": i}));
                } else if actor == "ACTOR_EN_ITEM00" {
                    collectibles.push(json!({"type": param & 0xFF, "flag": (param >> 8) & 0x3F, "pos": pos, "param": param, "room": name, "index": i}));
                } else if actor == "ACTOR_EN_ENCOUNT1" {
                    spawners.push(json!({"kind": "encount1", "actor": actor_id, "param": param, "pos": pos, "spawnType": (param >> 11) & 31, "maxLive": (param >> 6) & 31, "total": param & 63, "range": 120 + 40 * rz, "room": name}));
                } else if !enemyish.contains(&actor) && !npc_like(&actor) {
                    mechanisms.push(json!({"actor": actor, "param": param, "pos": pos, "rot": [rx, ry, rz], "room": name, "index": i}));
                } else if enemyish.contains(&actor) || npc_like(&actor) {
                    let n = skipped.get(&actor).and_then(Value::as_i64).unwrap_or(0) + 1;
                    skipped.insert(actor, n.into());
                }
            }
        }
        let doc = json!({"enemies": enemies, "spawners": spawners, "skipped": skipped, "collectibles": collectibles, "mechanisms": mechanisms, "environment": environment});
        std::fs::write(out.join(format!("{course}.json")), dumps(&doc, None) + "\n").map_err(|e| e.to_string())?;
    }
    // Models; a kind that fails is skipped (the game's loader copes with a missing model).
    let all: Vec<Model> = MODELS.into_iter().chain([leever()]).collect();
    for md in &all {
        let r = Resident { scale: md.scale, textures: md.textures.iter().map(|s| s.to_string()).collect() };
        // A skeleton the xml names without an offset (`object_x_Skel_01B310`) is at the hex number in its name.
        let skeleton: Sym = match (ctx.object_defs(md.bank).ok().and_then(|d| d.try_get(md.skel).map(|a| a.has("Offset"))), md.skel.rsplit_once("_Skel_")) {
            (Some(false), Some((_, hex_))) | (None, Some((_, hex_))) => usize::from_str_radix(hex_, 16).map(Sym::Off).unwrap_or_else(|_| md.skel.into()),
            _ => md.skel.into(),
        };
        let a = Actor { asset: md.kind, body: md.bank, skeleton, animation: md.idle.into(), resident: Some(&r), output_folder: Some("oot_enemies"), extra_clips: Some(md.clips.to_vec()), ..Default::default() };
        if let Err(e) = actor_model(ctx, &a, &assets) {
            eprintln!("MODEL FAILED {} {e}", md.kind);
        }
    }
    // Shabom: a single static display list (gBubbleDL), no skeleton.
    {
        let bank = "object_bubble";
        let specs = ctx.object_defs(bank)?;
        let data = ctx.segment(bank)?;
        let segs: Segs = HashMap::from([(6, data.as_slice())]);
        let mut mesh = SceneModel::new();
        let tr = |x: f64, y: f64, z: f64| [x * 4.4, y * 4.4, z * 4.4];
        match (Mesh { name: bank, segments: &segs, transform: &tr, layer: 1, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }).into_model(&mut mesh, 0x0600_0000 + specs.offset("gBubbleDL")? as u32) {
            Ok(()) => mesh.write(&out.join("shabom.glb"))?,
            Err(e) => eprintln!("MODEL FAILED shabom {e}"),
        }
    }
    // Freezard: the intact statue display list (gFreezardIntactDL), actor scale 0.008.
    {
        let bank = "object_fz";
        let specs = ctx.object_defs(bank)?;
        let data = ctx.segment(bank)?;
        let segs: Segs = HashMap::from([(6, data.as_slice())]);
        let mut mesh = SceneModel::new();
        let tr = |x: f64, y: f64, z: f64| [x * 0.008 * 4.4 * 100.0 / 100.0, y * 0.008 * 4.4, z * 0.008 * 4.4];
        match (Mesh { name: bank, segments: &segs, transform: &tr, layer: 0, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }).into_model(&mut mesh, 0x0600_0000 + specs.offset("gFreezardIntactDL")? as u32) {
            Ok(()) => mesh.write(&out.join("freezard.glb"))?,
            Err(e) => eprintln!("MODEL FAILED freezard {e}"),
        }
    }
    // Like Like: the body display list (gLikeLikeDL), actor scale 0.01.
    {
        let bank = "object_rr";
        let specs = ctx.object_defs(bank)?;
        let data = ctx.segment(bank)?;
        let segs: Segs = HashMap::from([(6, data.as_slice())]);
        let mut mesh = SceneModel::new();
        let tr = |x: f64, y: f64, z: f64| [x * 0.01 * 4.4, y * 0.01 * 4.4, z * 0.01 * 4.4];
        match (Mesh { name: bank, segments: &segs, transform: &tr, layer: 0, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }).into_model(&mut mesh, 0x0600_0000 + specs.offset("gLikeLikeDL")? as u32) {
            Ok(()) => mesh.write(&out.join("like_like.glb"))?,
            Err(e) => eprintln!("MODEL FAILED like_like {e}"),
        }
    }
    // Spike: the rock body display list (gEnNyRockBodyDL), actor scale 0.01.
    {
        let bank = "object_ny";
        let specs = ctx.object_defs(bank)?;
        let data = ctx.segment(bank)?;
        let segs: Segs = HashMap::from([(6, data.as_slice())]);
        let mut mesh = SceneModel::new();
        let tr = |x: f64, y: f64, z: f64| [x * 0.01 * 4.4, y * 0.01 * 4.4, z * 0.01 * 4.4];
        match (Mesh { name: bank, segments: &segs, transform: &tr, layer: 0, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }).into_model(&mut mesh, 0x0600_0000 + specs.offset("gEnNyRockBodyDL")? as u32) {
            Ok(()) => mesh.write(&out.join("spike.glb"))?,
            Err(e) => eprintln!("MODEL FAILED spike {e}"),
        }
    }
    // Deku Baba stem pieces and the crown of leaves (EnDekubaba_Draw).
    let bank = "object_dekubaba";
    let specs = ctx.object_defs(bank)?;
    let data = ctx.segment(bank)?;
    let segs: Segs = HashMap::from([(6, data.as_slice())]);
    for piece in ["Stem0", "Stem1", "Stem2", "BaseLeaves"] {
        let mut mesh = SceneModel::new();
        let tr = |x: f64, y: f64, z: f64| [x * 0.01 * 4.4, y * 0.01 * 4.4, z * 0.01 * 4.4];
        Mesh { name: bank, segments: &segs, transform: &tr, layer: 0, joint: None, matrices: &[], terrain_light: None, texture_defs: None, skin_bones: None }.into_model(&mut mesh, 0x0600_0000 + specs.offset(&format!("gDekubaba{piece}DL"))? as u32)?;
        mesh.write(&out.join(format!("deku_baba_{}.glb", piece.to_lowercase())))?;
    }
    Ok(())
}
