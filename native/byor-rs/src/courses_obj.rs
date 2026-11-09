//! Second half of the `export-courses.py` port: the object placement chains, extra collision and the group-2 additions.
use crate::courses::{act_list, i, model_of, obj, p3, push, star_act, Courses, M};
use crate::pyc::{args, clean, float, int, int0, ints, read, re, round, R};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

fn o5(a: &[String]) -> R<(Value, i64)> {
    Ok((p3(a, 1)?, int(&a[5])?))
}

impl Courses {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn objects(&self, level: &str, area_index: i64, script: &str, models: &HashMap<String, String>, folder: &Path, terrain_path: &Path, data: &mut M) -> R<()> {
        let frozen: Vec<(&str, &str)> = if level == "wf" {
            vec![
                ("bhvWFRotatingWoodenPlatform", "rotating_wooden_platform"),
                ("bhvWFSlidingPlatform", "sliding_platform"),
                ("bhvRotatingPlatform", "rotating_platform"),
                ("bhvSmallBomp", "small_bomp"),
                ("bhvLargeBomp", "large_bomp"),
                ("bhvWFBreakableWallRight", "breakable_wall_right"),
                ("bhvWFBreakableWallLeft", "breakable_wall_left"),
            ]
        } else {
            vec![]
        };
        let frozen_of = |b: &str| frozen.iter().find(|(k, _)| *k == b).map(|(_, v)| *v);
        let lvl_model = |k: &str| -> R<String> { Ok(format!("levels/{level}/{}.glb", model_of(models, k)?)) };
        // Act-dependent script objects.
        for a in args(script, "OBJECT_WITH_ACTS") {
            let acts = act_list(&a[a.len() - 1]);
            let b = a[8].as_str();
            let (pos, yaw) = o5(&a)?;
            let simple = |data: &mut M, model: &str, behavior: &str| obj(data, json!({"model":model,"pos":pos,"yaw":yaw,"behavior":behavior,"acts":acts}));
            if level == "bob" && (b == "bhvBoBBowlingBallSpawner" || b == "bhvTTMBowlingBallSpawner") {
                let path = match a[7].as_str() {
                    "BBALL_BP_STYPE_BOB_UPPER" => 0,
                    "BBALL_BP_STYPE_BOB_LOWER" => 1,
                    o => return Err(format!("unknown ball path {o}")),
                };
                if !data.contains_key("mountainBallPaths") {
                    let source = read(&folder.join("areas/1/trajectory.inc.c"))?;
                    let mut paths = Vec::new();
                    for k in 0..2 {
                        let part = source.split(&format!("bob_seg7_metal_ball_path{k}[]")).nth(1).ok_or("ball path")?.split("TRAJECTORY_END").next().unwrap().to_string();
                        paths.push(Value::Array(args(&part, "TRAJECTORY_POS").iter().map(|r| ints(&r[1..]).map(Value::Array)).collect::<R<_>>()?));
                    }
                    data.insert("mountainBallPaths".into(), json!(paths));
                }
                push(data, "mountainBallSpawners", json!({"pos":p3(&a, 1)?,"path":path,"period":if b == "bhvBoBBowlingBallSpawner" { 128 } else { 64 },"acts":acts}));
            } else if level == "bob" && b == "bhvPitBowlingBall" {
                obj(data, json!({"model":"actors/bowling_ball/bowling_ball_geo.glb","behavior":"pitBowlingBall","pos":pos,"yaw":yaw,"acts":acts}));
            } else if b == "bhvSunkenShipPart2" || b == "bhvShipPart3" {
                let rotation = if b == "bhvSunkenShipPart2" { json!([0xE958, 0xEE6C, 0x0C80]) } else { json!([0, 0, 0]) };
                obj(data, json!({"model":lvl_model(&a[0])?,"pos":pos,"yaw":0,"rotation":rotation,"acts":acts}));
            } else if b == "bhvInSunkenShip" || b == "bhvInSunkenShip2" || b == "bhvInSunkenShip3" {
                let name = match b {
                    "bhvInSunkenShip" => "in_sunken_ship",
                    "bhvInSunkenShip2" => "in_sunken_ship_2",
                    _ => "in_sunken_ship_3",
                };
                let rotation = if b.ends_with('3') { json!([0, 0, 0]) } else { json!([0xE958, 0xEE6C, 0x0C80]) };
                push(data, "shipCollision", json!({"name":name,"pos":pos,"rotation":rotation,"acts":acts}));
            } else if level == "wf" && (b == "bhvTower" || b == "bhvBulletBillCannon") {
                obj(data, json!({"model":format!("levels/wf/{}.glb", model_of(models, &a[0])?),"pos":pos,"yaw":yaw,"acts":acts}));
            } else if b == "bhvTowerPlatformGroup" {
                let source = clean(&read(&self.refd.join("src/game/behaviors/tower_platform.inc.c"))?);
                let tail = source.split_once("void spawn_wf_platform_group(void)").ok_or("tower")?.1.to_string();
                let children = args(&tail, "spawn_and_init_wf_platforms");
                let kinds = |k: &str| match k {
                    "bhvWFSolidTowerPlatform" => "tower_solid",
                    "bhvWFSlidingTowerPlatform" => "tower_slide",
                    _ => "tower_elevator",
                };
                let (x, y, z) = (int(&a[1])?, int(&a[2])?, int(&a[3])?);
                for (index, c) in children.iter().enumerate() {
                    let angle = index as f64 * std::f64::consts::PI / 4.0;
                    obj(data, json!({"model":format!("levels/wf/{}.glb", model_of(models, &c[0])?),"yaw":index as i64 * 45,"param":index,"pos":[json!(x as f64 + 704.0 * angle.sin()), json!(y + index as i64 * 100), json!(z as f64 + 704.0 * angle.cos())],"collision":kinds(&c[1]),"acts":acts}));
                }
            } else if b == "bhvCannonClosed" {
                push(data, "cannons", json!({"pos":pos,"yaw":yaw,"param":int0(&a[7])?,"acts":acts}));
            } else if b == "bhvKingBobomb" {
                simple(data, "actors/king_bobomb/king_bobomb_geo.glb", "kingBobomb");
            } else if level == "bob" && b == "bhvHiddenStar" {
                data.insert("secretStar".into(), json!({"act":star_act(&a[7])?,"pos":pos,"triggers":[]}));
            } else if b == "bhvStar" {
                push(data, "placedStars", json!({"act":star_act(&a[7])?,"pos":pos,"acts":acts}));
            } else if b == "bhvCCMTouchedStarSpawn" {
                let source = clean(&read(&self.refd.join("src/game/behaviors/spawn_star_exit.inc.c"))?);
                let p: Vec<f64> = args(&source, "spawn_default_star")[0].iter().map(|v| float(v.trim_end_matches('f'))).collect::<R<_>>()?;
                data.insert("secretStar".into(), json!({"act":1,"pos":p,"triggers":[],"origin":[2780, int(&a[2])? + 100, 4666]}));
            } else if b == "bhvHiddenRedCoinStar" {
                data.insert("redCoinAct".into(), json!(star_act(&a[7])?));
                data.insert("redCoinStar".into(), p3(&a, 1)?);
            } else if b == "bhvWhompKingBoss" {
                simple(data, "actors/whomp/whomp_geo.glb", "whompKing");
            } else if b == "bhv1Up" {
                simple(data, "actors/mushroom_1up/mushroom_1up_geo.glb", "oneUp");
            } else if b == "bhvKlepto" {
                simple(data, "actors/klepto/klepto_geo.glb", "klepto");
            } else if b == "bhvSnowmansBottom" || b == "bhvSnowmansHead" {
                let body = b == "bhvSnowmansBottom";
                obj(data, json!({"model":format!("levels/ccm/{}.glb", model_of(models, &a[0])?),"pos":pos,"yaw":yaw,"behavior":if body { "snowmanBody" } else { "snowmanHead" },"acts":acts}));
                if body {
                    let t = read(&folder.join("areas/1/trajectory.inc.c"))?;
                    let traj = t.split("TRAJECTORY_END").next().unwrap();
                    data.insert("snowmanPath".into(), json!(args(traj, "TRAJECTORY_POS").iter().map(|r| ints(&r[1..]).map(Value::Array)).collect::<R<Vec<_>>>()?));
                } else {
                    data.insert("snowmanStar".into(), json!({"act":star_act(&a[7])?,"pos":[-4700, -1024, 1890]}));
                }
            } else if b == "bhvTuxiesMother" {
                obj(data, json!({"model":"actors/penguin/penguin_geo.glb","pos":pos,"yaw":yaw,"behavior":"tuxiesMother","param":0,"acts":acts}));
                data.insert("penguinStar".into(), json!({"act":star_act(&a[7])?,"pos":[3167, -4300, 5108]}));
            } else if b == "bhvRacingPenguin" {
                let waypoints = args(&read(&folder.join("areas/2/trajectory.inc.c"))?, "TRAJECTORY_POS");
                let mut markers: Vec<(String, Value)> = Vec::new();
                for o in args(script, "OBJECT") {
                    let k = o[o.len() - 1].clone();
                    let v = p3(&o, 1)?;
                    if let Some(e) = markers.iter_mut().find(|(n, _)| *n == k) {
                        e.1 = v;
                    } else {
                        markers.push((k, v));
                    }
                }
                let m = |k: &str| markers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).ok_or_else(|| format!("no marker {k}"));
                data.insert("penguinRace".into(), json!({"path":waypoints.iter().map(|r| ints(r).map(Value::Array)).collect::<R<Vec<_>>>()?,"finish":m("bhvPenguinRaceFinishLine")?,"shortcut":m("bhvPenguinRaceShortcutCheck")?}));
                data.insert("penguinStar".into(), json!({"act":star_act(&a[7])?,"pos":[-7339, -5700, -6774]}));
                simple(data, "actors/penguin/penguin_geo.glb", "racingPenguin");
            } else if b == "bhvBalconyBigBoo" || b == "bhvGhostHuntBigBoo" {
                simple(data, "actors/boo/boo_geo.glb", "bigBoo");
            } else if b == "bhvBoo" || b == "bhvGhostHuntBoo" {
                simple(data, "actors/boo/boo_geo.glb", "boo");
            } else if b == "bhvKoopa" && a[7].contains("KOOPA_THE_QUICK_BOB") {
                simple(data, "actors/koopa/koopa_with_shell_geo.glb", "koopaTheQuick");
            } else if b == "bhvKoopa" && a[7].contains("KOOPA_BP_NORMAL") {
                simple(data, "actors/koopa/koopa_with_shell_geo.glb", "koopa");
            } else if b == "bhvBobombBuddy" || b == "bhvBobombBuddyOpensCannon" {
                let opens = b == "bhvBobombBuddyOpensCannon";
                let param = if opens { 1 } else { int(a[7].strip_prefix("DIALOG_").unwrap_or(&a[7]))? };
                obj(data, json!({"model":"actors/bobomb/bobomb_buddy_geo.glb","pos":pos,"yaw":yaw,"behavior":if opens { "bobombBuddy" } else { "bobombBuddyAdvice" },"param":param,"acts":acts}));
            } else if b == "bhvWaterBombCannon" {
                obj(data, json!({"model":"actors/cannon_base/cannon_base_geo.glb","pos":pos,"yaw":yaw,"behavior":"waterBombCannon","param":int0(&a[7])?,"acts":acts}));
            } else if b == "bhvKoopaRaceEndpoint" {
                simple(data, "actors/koopa_flag/koopa_flag_geo.glb", "koopaFlag");
            }
        }
        // second chain
        let script_behaviors = [("bhvChainChompGate", "chainChompGate")];
        let actor_behaviors = [("bhvFloorSwitchGrills", "actors/purple_switch/purple_switch_geo.glb", "purpleSwitch"), ("bhvPitBowlingBall", "actors/bowling_ball/bowling_ball_geo.glb", "pitBowlingBall")];
        let bbh_moving = [("bhvMeshElevator", "mesh_elevator"), ("bhvMerryGoRound", "merry_go_round"), ("bhvHauntedBookshelf", "moving_bookshelf"), ("bhvBBHTiltingTrapPlatform", "tilting_trap_platform")];
        let objects = args(script, "OBJECT");
        for a in &objects {
            let last = a[a.len() - 1].as_str();
            let (pos, yaw) = o5(a)?;
            let simple = |data: &mut M, model: &str, behavior: &str| obj(data, json!({"model":model,"pos":pos,"yaw":yaw,"behavior":behavior}));
            if last == "bhvTHIBowlingBallSpawner" {
                // `bhv_thi_bowling_ball_spawner_loop` (Tiny-Huge Island): every 64 frames, when Mario is within 12000 (and not within 800, and
                // not above the spawner), a ball rolls the huge / tiny trajectory of `bowling_ball.inc.c` with odds 1.5 (Rust-only data).
                if !data.contains_key("mountainBallPaths") {
                    let source = read(&folder.join("../../src/game/behaviors/bowling_ball.inc.c"))?;
                    let mut paths = Vec::new();
                    for name in ["sTHIHugeMetalBallTraj", "sTHITinyMetalBallTraj"] {
                        let part = source.split(&format!("{name}[]")).nth(1).ok_or("thi ball path")?.split("TRAJECTORY_END").next().unwrap().to_string();
                        paths.push(Value::Array(args(&part, "TRAJECTORY_POS").iter().map(|r| ints(&r[1..]).map(Value::Array)).collect::<R<_>>()?));
                    }
                    data.insert("mountainBallPaths".into(), json!(paths));
                }
                let (path, speed, scale) = match a[7].as_str() {
                    "BBALL_BP_STYPE_THI_LARGE" => (0, 25, 1.0),
                    "BBALL_BP_STYPE_THI_SMALL" => (1, 10, 0.3),
                    o => return Err(format!("unknown THI ball {o}")),
                };
                push(data, "mountainBallSpawners", json!({"pos":p3(a, 1)?,"path":path,"period":64,"maxDistance":12000,"minDistance":800,"odds":1.5,"speed":speed,"scale":scale}));
            } else if last == "bhvWhirlpool" {
                obj(data, json!({"behavior":"whirlpool","pos":pos,"yaw":yaw}));
            } else if last == "bhvSmallPenguin" {
                obj(data, json!({"model":"actors/penguin/penguin_geo.glb","pos":pos,"yaw":yaw,"behavior":"babyPenguin","param":int0(&a[7])?}));
            } else if script_behaviors.iter().any(|(k, _)| *k == last) && models.contains_key(&a[0]) {
                let b = script_behaviors.iter().find(|(k, _)| *k == last).unwrap().1;
                simple(data, &lvl_model(&a[0])?, b);
            } else if last == "bhvOpenableGrill" && models.contains_key("MODEL_BOB_BARS_GRILLS") {
                simple(data, &lvl_model("MODEL_BOB_BARS_GRILLS")?, "openableGrill");
            } else if last == "bhvBowserBomb" {
                obj(data, json!({"model":"actors/bomb/bowser_bomb_geo.glb","behavior":"bowserMine","pos":pos,"yaw":yaw}));
            } else if last == "bhvTiltingBowserLavaPlatform" || last == "bhvFallingBowserPlatform" {
                let platform = if level == "bowser_2" { "tilting_platform".to_string() } else { format!("falling_platform_{}", int(&a[7])?) };
                obj(data, json!({"model":lvl_model(&a[0])?,"pos":pos,"yaw":yaw,"collision":format!("{platform}/collision.inc.c")}));
            } else if last == "bhvBowsersSub" || last == "bhvBowserSubDoor" {
                let part = if last == "bhvBowsersSub" { "submarine" } else { "sub_door" };
                obj(data, json!({"model":lvl_model(&a[0])?,"pos":pos,"yaw":yaw,"collision":format!("{part}/collision.inc.c"),"submarine":true}));
            } else if bbh_moving.iter().any(|(k, _)| *k == last) && models.contains_key(&a[0]) {
                let c = bbh_moving.iter().find(|(k, _)| *k == last).unwrap().1;
                obj(data, json!({"model":format!("levels/bbh/{}.glb", model_of(models, &a[0])?),"pos":pos,"yaw":yaw,"collision":format!("{c}/collision.inc.c")}));
            } else if last == "bhvBigBully" || last == "bhvBigBullyWithMinions" {
                simple(data, "actors/bully/bully_boss_geo.glb", "bigBully");
            } else if last == "bhvSmallBully" {
                simple(data, "actors/bully/bully_geo.glb", "bully");
            } else if last == "bhvSmallWhomp" {
                simple(data, "actors/whomp/whomp_geo.glb", "whomp");
            } else if last == "bhvMrBlizzard" {
                simple(data, "actors/snowman/mr_blizzard_geo.glb", "mrBlizzard");
            } else if last == "bhvFlame" {
                simple(data, "actors/flame/red_flame_geo.glb", "flame");
            } else if let Some((_, model, behavior)) = actor_behaviors.iter().find(|(k, _, _)| *k == last) {
                simple(data, model, behavior);
            } else if level == "bob" && last == "bhvCheckerboardElevatorGroup" {
                let (size, travel) = a[7].split_once('|').ok_or("checkerboard param")?;
                if size.trim() != "first_0" {
                    return Err("checkerboard size".into());
                }
                let t = int0(travel.trim())?;
                let travel = if t == 0 { 65 } else { t };
                let (x, y, z) = (int(&a[1])?, int(&a[2])?, int(&a[3])?);
                for (child, offset) in [-145i64, 145].into_iter().enumerate() {
                    let p = [x + round(f64::from(yaw as i32).to_radians().sin() * offset as f64), y + child as i64 * travel * 10, z + round(f64::from(yaw as i32).to_radians().cos() * offset as f64)];
                    obj(data, json!({"model":"actors/checkerboard_platform/checkerboard_platform_geo.glb","pos":p,"yaw":yaw,"collision":"checkerboard_lift","param":if child == 0 { travel } else { -travel }}));
                }
            } else if level == "ccm" && last == "bhvPlatformOnTrack" {
                let t = read(&folder.join("areas/1/trajectory.inc.c"))?;
                let trajectory = t.split_once("ccm_seg7_trajectory_0701669C[]").ok_or("trajectory")?.1.to_string();
                let track: Vec<Value> = args(&trajectory, "TRAJECTORY_POS").iter().map(|r| ints(&r[1..]).map(Value::Array)).collect::<R<_>>()?;
                obj(data, json!({"model":lvl_model(&a[0])?,"pos":track[0],"yaw":yaw,"collision":"ropeway_lift/collision.inc.c","track":track}));
            } else if last == "bhvStaticObject" && models.contains_key(&a[0]) {
                obj(data, json!({"model":lvl_model(&a[0])?,"pos":pos,"yaw":yaw}));
            }
        }
        for a in &objects {
            let last = a[a.len() - 1].as_str();
            if let Some(c) = frozen_of(last).filter(|_| models.contains_key(&a[0])) {
                let (pos, yaw) = o5(a)?;
                let mut spec = json!({"model":lvl_model(&a[0])?,"pos":pos,"yaw":yaw,"param":self.platform_param(&a[7], last)?,"collision":format!("{c}/collision.inc.c")});
                if last == "bhvWFBreakableWallLeft" || last == "bhvWFBreakableWallRight" {
                    spec["behavior"] = json!("blastWall");
                    spec["param"] = json!(i64::from(last == "bhvWFBreakableWallRight"));
                }
                obj(data, spec);
            }
        }
        let terrain_text = read(terrain_path)?;
        for a in args(&terrain_text, "SPECIAL_OBJECT(?:_WITH_YAW)?") {
            let preset = a[0].as_str();
            if preset == "special_bowser" {
                obj(data, json!({"model":"actors/bowser/bowser_geo_no_shadow.glb","pos":p3(&a, 1)?,"yaw":0,"behavior":"bowser","param":level[level.len() - 1..].parse::<i64>().map_err(|e| e.to_string())?}));
            } else if preset == "special_mine" {
                obj(data, json!({"model":"actors/bomb/bowser_bomb_geo.glb","behavior":"bowserMine","pos":p3(&a, 1)?,"yaw":0}));
            } else if preset == "special_bubble_tree" {
                obj(data, json!({"model":"actors/tree/bubbly_tree_geo.glb","pos":p3(&a, 1)?,"yaw":0,"behavior":"tree"}));
            } else if let Some(rest) = preset.strip_prefix("special_level_geo_") {
                if let Some(model) = models.get(&format!("MODEL_LEVEL_GEOMETRY_{rest}")) {
                    let path = if model == "bowser_1_yellow_sphere_geo" { format!("actors/yellow_sphere/{model}.glb") } else { format!("levels/{level}/{model}.glb") };
                    let yaw = if a.len() > 4 { int0(&a[4])? } else { 0 } as f64 * 360.0 / 256.0;
                    obj(data, json!({"model":path,"pos":p3(&a, 1)?,"yaw":yaw}));
                }
            }
        }
        let macro_path = folder.join(format!("areas/{area_index}/macro.inc.c"));
        let macro_behaviors = [
            ("macro_bobomb", "actors/bobomb/black_bobomb_geo.glb", "bobomb", 0),
            ("macro_bobomb_stationary", "actors/bobomb/black_bobomb_geo.glb", "bobomb", 1),
            ("macro_breakable_box_no_coins", "actors/breakable_box/breakable_box_geo.glb", "breakableBox", 0),
            ("macro_breakable_box_three_coins", "actors/breakable_box/breakable_box_geo.glb", "breakableBox", 1),
            ("macro_breakable_box_five_coins", "actors/breakable_box/breakable_box_geo.glb", "breakableBox", 2),
            ("macro_breakable_box_small", "actors/breakable_box/breakable_box_small_geo.glb", "breakableBoxSmall", 0),
            ("macro_chain_chomp", "actors/chain_chomp/chain_chomp_geo.glb", "chainChomp", 0),
            ("macro_koopa_shell_underwater", "actors/koopa_shell/koopa_shell_geo.glb", "koopaShellUnderwater", 0),
            ("macro_wooden_post_1", "actors/poundable_pole/wooden_post_geo.glb", "woodenPost", 0),
        ];
        if macro_path.exists() {
            let mt = read(&macro_path)?;
            for a in args(&mt, "MACRO_OBJECT_WITH_BHV_PARAM") {
                if a[0] == "macro_wooden_signpost" {
                    let yaw = (((int(&a[1])? as f64 * 16.0 / 45.0) as i64 & 127) << 9) as f64 * 360.0 / 65536.0;
                    let mut sign = json!({"model":"actors/wooden_signpost/wooden_signpost_geo.glb","pos":p3(&a, 2)?,"yaw":yaw,"collision":"signpost","pages":self.sign_pages(&a[5])?});
                    if level != "bob" {
                        // Rust importer only: loaded outside the TypeScript differential fixtures (`request.g2`).
                        sign["g2"] = json!(true);
                    }
                    obj(data, sign);
                }
                if a[0] == "macro_water_bomb_spawner" {
                    push(data, "waterBombSpawners", json!({"pos":p3(&a, 2)?,"param":int0(&a[5])?}));
                }
                if a[0] == "macro_cannon_closed" {
                    push(data, "cannons", json!({"pos":p3(&a, 2)?,"yaw":int(&a[1])?,"param":int0(&a[5])?}));
                }
            }
            for a in args(&mt, "MACRO_OBJECT") {
                let n = a[0].as_str();
                let (pos, yaw) = (p3(&a, 2)?, int(&a[1])?);
                if let Some((_, model, behavior, param)) = macro_behaviors.iter().find(|(k, _, _, _)| *k == n) {
                    obj(data, json!({"model":model,"pos":pos,"yaw":yaw,"behavior":behavior,"param":param}));
                } else if n == "macro_water_bomb_cannon_1" || n == "macro_water_bomb_cannon_2" {
                    obj(data, json!({"model":"actors/cannon_base/cannon_base_geo.glb","behavior":"waterBombCannon","param":0,"pos":pos,"yaw":yaw}));
                } else if n.starts_with("macro_bobomb_buddy_opens_cannon") {
                    obj(data, json!({"model":"actors/bobomb/bobomb_buddy_geo.glb","behavior":"bobombBuddy","param":1,"pos":pos,"yaw":yaw}));
                } else if n == "macro_pokey_1" || n == "macro_pokey_2" {
                    obj(data, json!({"model":"actors/pokey/pokey_head_geo.glb","pos":pos,"yaw":yaw,"behavior":"pokey"}));
                } else if n == "macro_goomba" || n == "macro_goomba_triplet_spawner" {
                    push(data, "goombas", json!({"pos":pos,"triplet":n.ends_with("spawner")}));
                } else if n == "macro_hidden_star_trigger" && data.contains_key("secretStar") {
                    data.get_mut("secretStar").unwrap()["triggers"].as_array_mut().unwrap().push(pos);
                } else if n == "macro_box_wing_cap" {
                    obj(data, json!({"model":"actors/exclamation_box/exclamation_box_geo.glb","behavior":"wingBox","pos":pos,"yaw":yaw}));
                } else if (level == "bob" || level == "jrb") && n.starts_with("macro_box_star_act_") {
                    obj(data, json!({"model":"actors/exclamation_box/variants/exclamation_box_geo/geo_switch_anim_state_3.glb","behavior":"starBox","pos":pos,"yaw":yaw,"param":int(&n[n.len() - 1..])?}));
                } else if data.contains_key("redCoinStar") && n == "macro_red_coin" {
                    push(data, "redCoins", pos);
                } else if n.starts_with("macro_coin_") {
                    let shape = n.trim_start_matches("macro_coin_");
                    let shape = shape.strip_suffix("_flying").unwrap_or(shape);
                    let yaw = ((int(&a[1])? as f64 * 16.0 / 45.0) as i64 & 127) << 9;
                    push(data, "coinFormations", json!({"shape":shape,"pos":pos,"yaw":yaw,"flying":n.ends_with("_flying")}));
                } else if n == "macro_yellow_coin_1" || n == "macro_yellow_coin_2" {
                    push(data, "coins", pos);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn extra_collision(&self, level: &str, script: &str, folder: &Path, terrain_path: &Path, data: &mut M) -> R<()> {
        let mut extras: Vec<String> = match level {
            "bob" => vec!["seesaw_platform/collision.inc.c".into()],
            "wf" => vec!["areas/1/10/collision.inc.c".into(), "areas/1/11/collision.inc.c".into()],
            "castle_grounds" => vec!["areas/1/7/collision.inc.c".into(), "areas/1/8/collision.inc.c".into()],
            _ => vec![],
        };
        if level == "wf" {
            let mut f: Vec<&str> = ["rotating_wooden_platform", "sliding_platform", "rotating_platform", "small_bomp", "large_bomp", "breakable_wall_right", "breakable_wall_left"].to_vec();
            f.sort_unstable();
            f.dedup();
            extras.extend(f.into_iter().map(|p| format!("{p}/collision.inc.c")));
        }
        if level == "bbh" {
            extras.extend(["mesh_elevator", "merry_go_round", "moving_bookshelf", "tilting_trap_platform"].map(|d| format!("{d}/collision.inc.c")));
        }
        for o in data["objects"].as_array().unwrap() {
            if let Some(c) = o.get("collision").and_then(Value::as_str).filter(|c| c.ends_with("/collision.inc.c")) {
                extras.push(c.to_string());
            }
        }
        let mut seen = Vec::new();
        let mut ec = M::new();
        for p in extras {
            if seen.contains(&p) {
                continue;
            }
            ec.insert(p.clone(), self.collision(&folder.join(&p), None)?);
            seen.push(p);
        }
        data.insert("extraCollision".into(), Value::Object(ec));
        let objs = data["objects"].as_array().unwrap().clone();
        let mut set = |data: &mut M, k: &str, v: Value| {
            data.get_mut("extraCollision").unwrap().as_object_mut().unwrap().insert(k.into(), v);
        };
        if objs.iter().any(|o| o["collision"] == "warp_pipe") {
            set(data, "warp_pipe", self.collision(&self.refd.join("actors/warp_pipe/collision.inc.c"), None)?);
        }
        if data.get("cannons").and_then(Value::as_array).is_some_and(|c| !c.is_empty()) {
            set(data, "cannonLid", self.collision(&self.refd.join("actors/cannon_lid/collision.inc.c"), None)?);
        }
        let ships: Vec<String> = data.get("shipCollision").and_then(Value::as_array).map(|s| s.iter().map(|x| x["name"].as_str().unwrap().to_string()).collect()).unwrap_or_default();
        for name in ships {
            set(data, &name, self.collision(&folder.join("wooden_ship/collision.inc.c"), Some(&format!("jrb_seg7_collision_{name}")))?);
        }
        if level != "bob" && data.get("objects").and_then(Value::as_array).is_some_and(|o| o.iter().any(|x| x["collision"] == "signpost")) {
            // Signs outside BoB (the Rust importer only; the frozen Python exporter places BoB's signs alone).
            set(data, "signpost", self.collision(&self.refd.join("actors/wooden_signpost/collision.inc.c"), None)?);
        }
        if level == "bob" {
            set(data, "signpost", self.collision(&self.refd.join("actors/wooden_signpost/collision.inc.c"), None)?);
            set(data, "checkerboard_lift", self.collision(&self.refd.join("actors/checkerboard_platform/collision.inc.c"), None)?);
            set(data, "chainGate", self.collision(&folder.join("chain_chomp_gate/collision.inc.c"), None)?);
            let mut post = self.collision(&self.refd.join("actors/poundable_pole/collision.inc.c"), None)?;
            for v in post["vertices"].as_array_mut().unwrap() {
                for x in v.as_array_mut().unwrap() {
                    // int(axis / 2): truncation toward zero
                    *x = json!((x.as_i64().unwrap() as f64 / 2.0).trunc() as i64);
                }
            }
            set(data, "woodenPost", post);
        }
        if level == "wf" {
            let platform = self.collision(&folder.join("extending_platform/collision.inc.c"), None)?;
            for k in ["tower_solid", "tower_slide", "tower_elevator"] {
                set(data, k, platform.clone());
            }
        }
        if data.get("wingSwitch").is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty())) {
            for (name, symbol) in [("switchTop", "capswitch_collision_050033D0"), ("switchBase", "capswitch_collision_05003448")] {
                set(data, name, self.collision(&self.refd.join("actors/capswitch/collision.inc.c"), Some(symbol))?);
            }
        }
        if objs.iter().any(|o| matches!(o["behavior"].as_str(), Some("starBox" | "wingBox"))) {
            let mut b = self.collision(&self.refd.join("actors/exclamation_box_outline/collision.inc.c"), None)?;
            for v in b["vertices"].as_array_mut().unwrap() {
                for x in v.as_array_mut().unwrap() {
                    *x = json!(2 * x.as_i64().unwrap());
                }
            }
            set(data, "starBox", b);
        }
        if args(script, "OBJECT").iter().any(|o| o[o.len() - 1] == "bhvDDDWarp") {
            for name in ["ddd_warp", "ddd_warp_2"] {
                set(data, name, self.collision(terrain_path, Some(&format!("inside_castle_seg7_collision_{name}")))?);
            }
        }
        let has_doors = data.get("doors").is_some_and(|d| d.as_array().is_some_and(|a| !a.is_empty())) || data.get("roomDoors").is_some_and(|d| d.as_array().is_some_and(|a| !a.is_empty()));
        if has_doors {
            set(data, "door", self.collision(&self.refd.join("actors/warp_collision/collision.inc.c"), Some("door_seg3_collision_0301CE78"))?);
        }
        if data.get("roomDoors").and_then(Value::as_array).is_some_and(|d| d.iter().any(|x| x["sliding"] == true)) {
            set(data, "star_door", self.collision(&folder.join("star_door/collision.inc.c"), None)?);
        }
        Ok(())
    }
}
