//! Ports of `scripts/oot-scenery.py` (child-day scenery variants selected by the actor overlays) and
//! `scripts/oot-residents.py` (room actor ids -> object symbols and greetings).
use super::libm;
use super::R;
use std::f64::consts::PI;

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub name: String,
    pub bank: String,
    pub lists: String,
    pub scale: f64,
    pub collision: String,
    pub translucent: bool,
}

impl Prop {
    pub fn new(name: &str) -> Prop {
        Prop { name: name.into(), bank: String::new(), lists: String::new(), scale: 0.1, collision: String::new(), translucent: false }
    }
    pub fn full(name: &str, bank: &str, lists: &str, scale: f64, collision: &str, translucent: bool) -> Prop {
        Prop { name: name.into(), bank: bank.into(), lists: lists.into(), scale, collision: collision.into(), translucent }
    }
    /// Hashable identity (`lru_cache` on the NamedTuple).
    pub fn key(&self) -> (String, String, String, u64, String, bool) {
        (self.name.clone(), self.bank.clone(), self.lists.clone(), self.scale.to_bits(), self.collision.clone(), self.translucent)
    }
}

pub fn recovery_heart() -> Prop {
    Prop::full("recovery_heart", "object_gi_heart", "gGiRecoveryHeartDL", 0.32, "", false)
}
pub fn drawbridge_chain() -> Prop {
    Prop::full("drawbridge_chain", "object_spot00_objects", "gHyruleFieldCastleDrawbridgeChainsDL", 1.0, "", false)
}

fn statics(actor: i64) -> Option<Prop> {
    let f = Prop::full;
    Some(match actor {
        0xbf => f("crater_smoke", "object_spot17_obj", "gCraterSmokeConeDL", 0.1, "", true),
        // Bg_Hidan_Dalm: the hammerable totem of the Fire Temple (no collision here: it cannot be removed when it breaks)
        0x40 => f("fire_totem", "object_hidan_objects", "gFireTempleHammerableTotemBodyDL gFireTempleHammerableTotemHeadDL", 0.1, "", false),
        0x4c => f("bomb_flower", "object_bombf", "gBombFlowerLeavesDL gBombFlowerBaseLeavesDL gBombFlowerBombAndSparkDL", 0.01, "", false),
        0x4a => f("castle_drawbridge", "object_spot00_objects", "gHyruleFieldCastleDrawbridgeDL", 1.0, "gHyruleFieldCastleDrawbridgeCol", false),
        0x9d => f("gravestone", "object_haka", "gGravestoneStoneDL gGravestoneEarthDL", 0.1, "gGravestoneCol", false),
        0x102 => Prop::new("kakariko_village/windmill_sails"),
        0x107 => f("milk_crate", "object_spot15_obj", "gLonLonMilkCrateDL", 0.1, "gLonLonMilkCrateCol", false),
        0xf8 => f("corral_fence", "object_spot15_obj", "gLonLonCorralFenceDL", 0.1, "gLonLonCorralFenceCol", false),
        0x11f => f("river_waterfall", "object_spot03_object", "object_spot03_object_DL_000B20 object_spot03_object_DL_000BC0 object_spot03_object_DL_001580", 0.1, "", true),
        0x190 => f("wasteland_gate", "object_spot12_obj", "gGerudoFortressWastelandGateDL", 0.1, "gGerudoFortressWastelandGateCol", false),
        0x191 => f("training_gate", "object_spot12_obj", "gGerudoFortressGTGShutterDL", 0.1, "gGerudoFortressGTGShutterCol", false),
        0x15c => f("goron_vase", "object_spot18_obj", "gGoronCityVaseDL", 0.1, "gGoronCityVaseCol", false),
        0x1c4 => f("goron_door", "object_spot18_obj", "gGoronCityDoorDL", 0.1, "gGoronCityDoorCol", false),
        0x19f => f("colossus_wall", "object_spot11_obj", "gDesertColossusBombableWallDL", 1.0, "gDesertColossusBombableWallCol", false),
        0x1a8 => f("fountain_wall", "object_spot08_obj", "gZorasFountainBombableWallDL", 1.0, "gZorasFountainBombableWallCol", false),
        0x1a9 => f("crater_wall", "object_spot17_obj", "gCraterBombableWallDL", 0.1, "gCraterBombableWallCol", false),
        0xcd => f("cavern_boulder", "object_spot16_obj", "gDodongosCavernRockDL", 0.4, "", false),
        0x127 => f("bombable_boulder", "object_bombiwa", "object_bombiwa_DL_0009E0", 0.1, "", false),
        0x1d2 => f("bronze_boulder", "gameplay_field_keep", "gSilverRockDL", 0.4, "", false),
        0x19e => f("beehive", "gameplay_field_keep", "gFieldBeehiveDL", 0.1, "", false),
        0x1a0 => f("kakariko_village/crate", "", "", 0.1, "gLargeCrateCol", false),
        0x141 => Prop::new("kokiri_forest/sign"),
        _ => return None,
    })
}

/// Dynapoly actors that move at run time: the sim kind (`jumpman_sim::oot_movers::Kind`) and the model/collision prop (`None` for
/// the Jabu water-level changer, which only moves a water box).
pub fn mover_for(actor: i64, p: i64) -> Option<(&'static str, Option<Prop>)> {
    let f = Prop::full;
    Some(match actor {
        0x87 => ("forest_elevator", Some(f("mori_elevator", "object_mori_objects", "gMoriElevatorDL", 1.0, "gMoriElevatorCol", false))),
        0xe3 if p & 0xff == 0 => ("forest_pillars", Some(f("mori_pillars", "object_mori_objects", "gMoriHashiraPlatformsDL", 1.0, "gMoriHashira1Col", false))),
        0xe3 => ("forest_gate", Some(f("mori_gate", "object_mori_objects", "gMoriHashiraGateDL", 1.0, "gMoriHashira2Col", false))),
        0xc8 => match p & 0xff {
            1 => ("jabu_elevator", Some(f("jabu_elevator", "object_bdan_objects", "gJabuElevatorPlatformDL", 0.1, "gJabuElevatorCol", false))),
            2 => ("jabu_water", None),
            3 => ("jabu_lowering", Some(f("jabu_lowering", "object_bdan_objects", "gJabuFallingPlatformDL", 0.1, "gJabuLoweringPlatformCol", false))),
            _ => return None,
        },
        0xe6 => match p & 0xff {
            0 => ("jabu_switch", Some(f("jabu_switch_blue", "object_bdan_objects", "gJabuBlueFloorSwitchDL", 0.1, "gJabuFloorSwitchCol", false))),
            1 | 2 => ("jabu_switch", Some(f("jabu_switch_yellow", "object_bdan_objects", "gJabuYellowFloorSwitchDL", 0.1, "gJabuFloorSwitchCol", false))),
            _ => return None,
        },
        // Bg_Haka: the stone comes out of its grave when Link pulls it (the earth patch stays: placed as a static prop)
        0x9d => ("grave", Some(f("grave_slab", "object_haka", "gGravestoneStoneDL", 0.1, "gGravestoneCol", false))),
        0x157 => ("jya_lift", Some(f("jya_lift", "object_jya_obj", "gLiftDL", 0.1, "gLiftCol", false))),
        0x18e => ("jya_1flift", Some(f("jya_1flift", "object_jya_obj", "g1fliftDL", 0.1, "g1fliftCol", false))),
        0x64 => match (p >> 12) & 15 {
            0 => ("water_float", Some(f("water_float_0", "object_mizu_objects", "gWaterTempleFloatingPlatformOutsideCentralPillarDL", 0.1, "gWaterTempleFloatingPlatformOutsideCentralPillarCol", false))),
            1 => ("water_float", Some(f("water_float_1", "object_mizu_objects", "gWaterTempleFloatingPlatformWestDL", 0.1, "gWaterTempleFloatingPlatformWestCol", false))),
            2 => ("water_float", Some(f("water_float_2", "object_mizu_objects", "gWaterTempleFloatingPlatformInsideCentralPillarDL", 0.1, "gWaterTempleFloatingPlatformInsideCentralPillarCol", false))),
            3..=6 => ("water_statue", Some(f("water_statue", "object_mizu_objects", "gWaterTempleDragonStatueDL", 0.1, "gWaterTempleDragonStatueCol", false))),
            _ => return None,
        },
        _ => return None,
    })
}

/// `prop_for(actor, p)`
pub fn prop_for(actor: i64, p: i64) -> R<Option<Prop>> {
    let f = Prop::full;
    if actor == 0x9b && p & 0x300 == 0 {
        return Ok(Some(f("grotto_entrance", "gameplay_field_keep", "gGrottoDL", 0.01, "", true)));
    }
    if actor == 0x9c && p & 255 < 3 {
        let (dl, col) = [("012A50", "012BA4"), ("0127C0", "0128D8"), ("0130B0", "0133EC")][(p & 255) as usize];
        return Ok(Some(f(&format!("grave_monument_{}", p & 255), "object_spot02_objects", &format!("object_spot02_objects_DL_{dl}"), 0.1, &format!("object_spot02_objects_Col_{col}"), false)));
    }
    if actor == 0x6e && (1..=6).contains(&p) {
        let emblem = ["flash", "dark", "flame", "ice", "soul", "wind"][(p - 1) as usize];
        return Ok(Some(f(&format!("warp_pad_{emblem}"), "object_mjin", "gWarpPadBaseDL", 1.0, "gWarpPadCol", false)));
    }
    if actor == 0x12c {
        return Ok(Some(f("collapsing_platform", "object_d_lift", "gCollapsingPlatformDL", if p & 2 != 0 { 0.05 } else { 0.1 }, "gCollapsingPlatformCol", false)));
    }
    if actor == 0x50 && p & 255 == 2 {
        return Ok(Some(f("deku_platforms", "object_ydan_objects", "gDTRisingPlatformsDL", 0.1, "gDTRisingPlatformsCol", false)));
    }
    if actor == 0x12d {
        let part = if p & 3 == 2 { "Target" } else { "Post" };
        return Ok(Some(f(&format!("hookshot_{}", part.to_lowercase()), "object_d_hsblock", &format!("gHookshot{part}DL"), 0.1, &format!("gHookshot{part}Col"), false)));
    }
    if actor == 0x51 {
        if p >> 8 == 1 {
            return Ok(Some(f("deku_ladder", "object_ydan_objects", "gDTFallingLadderDL", 0.1, "gDTFallingLadderCol", false)));
        }
        return Ok(Some(f("deku_spike_log", "object_ydan_objects", "gDTRollingSpikeTrapDL", 0.1, "", false)));
    }
    if actor == 0xb8 && p == 2 {
        return Ok(Some(f("valley_bridge", "object_spot09_obj", "gValleyBridgeChildDL", 1.0, "gValleyObjects2Col", false)));
    }
    if actor == 0xb9 && matches!(p & 15, 0 | 1) {
        let part = ["Statue", "StatueSpear"][(p & 15) as usize];
        return Ok(Some(f(&format!("goron_{}", part.to_lowercase()), "object_spot18_obj", &format!("gGoronCity{part}DL"), 0.1, &format!("gGoronCity{part}Col"), false)));
    }
    if actor == 0xd5 && p >> 8 < 3 {
        let part = ["WaterTempleGate", "WaterTempleKey", "HighWater"][(p >> 8) as usize];
        return Ok(Some(f(&format!("lake_{}", part.to_lowercase()), "object_spot06_objects", &format!("gLakeHylia{part}DL"), if p >> 8 == 2 { 1.0 } else { 0.1 }, if p >> 8 == 0 { "gLakeHyliaWaterTempleGateCol" } else { "" }, p >> 8 == 2)));
    }
    if actor == 0x120 && (p == 0 || p == 1) {
        return Ok(Some(f(&format!("domain_falls_{p}"), "object_spot07_object", ["object_spot07_object_DL_000460", "object_spot07_object_DL_000BE0"][p as usize], 0.1, "", true)));
    }
    if actor == 0x18d {
        let (dl, col) = [("000840", "000918"), ("001190", "0012C0")][(p & 1) as usize];
        return Ok(Some(f(&format!("meadow_gate_{}", p & 1), "object_spot05_objects", &format!("object_spot05_objects_DL_{dl}"), 0.1, &format!("object_spot05_objects_Col_{col}"), false)));
    }
    if actor == 0x5e {
        let style = *["Golden", "Timed", "Wooden"].get((p >> 12) as usize).ok_or("IndexError: torch style")?;
        return Ok(Some(f(&format!("{}_torch", style.to_lowercase()), "object_syokudai", &format!("g{style}TorchDL"), 1.0, "", false)));
    }
    if actor == 0x111 {
        return Ok(Some(if p & 256 != 0 { Prop::new("kakariko_village/pot") } else { f("dungeon_pot", "gameplay_dangeon_keep", "gPotDL", 0.15, "", false) }));
    }
    if actor == 0x14e {
        return Ok(Some(if p & 1 != 0 { f("silver_rock", "gameplay_field_keep", "gSilverRockDL", 0.4, "", false) } else { Prop::new("kakariko_village/rock") }));
    }
    if actor == 0x125 {
        return Ok(Some(if p & 3 != 0 { f("wild_grass", "object_kusa", "object_kusa_DL_000140", 0.4, "", false) } else { Prop::new("kakariko_village/shrub") }));
    }
    if actor == 0x14f && p == 0 {
        return Ok(Some(f("flowers", "gameplay_field_keep", "gHanaDL", 0.01, "", false)));
    }
    if actor == 0x39 && matches!(p & 255, 9 | 10) {
        return Ok(Some(Prop::new(if p & 255 == 10 { "kakariko_village/arrow_sign" } else { "kokiri_forest/sign" })));
    }
    if actor == 0x77 {
        let kind = p & 255;
        if kind < 5 {
            return Ok(Some(Prop::new("kakariko_village/tree")));
        }
        if kind < 10 {
            return Ok(Some(f("oval_tree", "object_wood02", "object_wood02_DL_007CA0 object_wood02_DL_007D38", 1.0, "", false)));
        }
        if (11..=16).contains(&kind) {
            return Ok(Some(f("woodland_bush", "object_wood02", "object_wood02_DL_000090", 1.0, "", false)));
        }
    }
    Ok(statics(actor))
}

pub fn cluster_offsets(kind: i64) -> Vec<(f64, f64)> {
    if kind == 1 {
        let scattered = [(40, 0x666), (40, 0x2ccc), (40, 0x5999), (40, 0x8666), (20, 0xc000), (80, 0x1333), (80, 0x4000), (80, 0x6ccc), (80, 0x9333), (80, 0xaccc), (80, 0xc666), (60, 0xe000)];
        return scattered.iter().map(|&(r, a)| (r as f64 * libm::cos_(a as f64 * PI / 32768.0), -(r as f64) * libm::sin_(a as f64 * PI / 32768.0))).collect();
    }
    let ring: Vec<(f64, f64)> = (0..8).map(|i| (80.0 * libm::sin_(i as f64 * PI / 4.0), 80.0 * libm::cos_(i as f64 * PI / 4.0))).collect();
    if kind == 0 {
        let mut v = vec![(0.0, 0.0)];
        v.extend(ring);
        v
    } else {
        ring
    }
}

/// `wood_offsets(kind, yaw)`: (dx, dz, heading).
pub fn wood_offsets(kind: i64, yaw: i64) -> Vec<(f64, f64, i64)> {
    let points = [(707, 0x1fff), (525, 0x4c9e), (510, 0x77f5), (500, 0xa5c9), (566, 0xd6c3), (141, 0xa000)];
    points
        .iter()
        .map(|&(radius, heading): &(i64, i64)| {
            let angle = (heading + yaw + if kind == 15 { 0x4000 } else { 0 }) as f64 * PI / 32768.0;
            (radius as f64 * libm::sin_(angle), radius as f64 * libm::cos_(angle), heading)
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ residents

#[derive(Clone, Debug)]
pub struct Resident {
    pub label: &'static str,
    pub bank: &'static str,
    pub skeleton: &'static str,
    pub idle: &'static str,
    pub greeting: i64,
    pub textures: &'static [&'static str],
    pub scale: f64,
    pub motion_bank: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn r(label: &'static str, bank: &'static str, skeleton: &'static str, idle: &'static str, greeting: i64, textures: &'static [&'static str], scale: f64, motion_bank: &'static str) -> Resident {
    Resident { label, bank, skeleton, idle, greeting, textures, scale, motion_bank }
}

const GUARD: Resident = r("Hyrulean Soldier", "sd", "gEnHeishiSkel", "gEnHeishiIdleAnim", 0x7002, &[], 0.01, "");
const ZORA: Resident = r("Zora", "zo", "gZoraSkel", "gZoraIdleAnim", 0x4006, &["gZoraEyeOpenTex"], 0.01, "");
const GORON: Resident = r("Goron", "oF1d_map", "gGoronSkel", "gGoronAnim_0029A8", 0x3002, &["gGoronCsEyeOpenTex", "gGoronCsMouthNeutralTex"], 0.01, "");
const SCARECROW: Resident = r("Bonooru", "ka", "object_ka_Skel_0065B0", "object_ka_Anim_000214", 0x40A0, &[], 0.01, "");
const SCRUB: Resident = r("Deku Scrub", "hintnuts", "gHintNutsSkel", "gHintNutsLookAroundAnim", 0x109B, &[], 0.01, "");

pub fn resident(actor: i64) -> Option<Resident> {
    Some(match actor {
        0x3C => r("Ranch Horse", "horse_normal", "gHorseNormalSkel", "gHorseNormalIdleAnim", 0, &[], 0.01, ""),
        0x9A => r("Epona", "horse_link_child", "gChildEponaSkel", "gChildEponaIdleAnim", 0, &[], 0.005, ""),
        0xCB => r("Ingo", "in", "gIngoSkel", "gIngoStandingAnim", 0x2030, &["gIngoEyeOpenTex", "gIngoHeadGradient2Tex"], 0.01, ""),
        0xE7 => r("Malon", "ma1", "gMalonChildSkel", "gMalonChildIdleAnim", 0x2041, &["gMalonChildEyeOpenTex", "gMalonChildNeutralMouthTex"], 0.01, ""),
        0x14D => r("Kaepora Gaebora", "owl", "gOwlPerchingSkel", "gOwlPerchAnim", 0x2064, &["gObjOwlEyeOpenTex"], 0.025, ""),
        0x178 | 0xB3 | 0x142 => GUARD,
        0x1BC => r("Carpenter", "daiku", "object_daiku_Skel_007958", "object_daiku_Anim_007DE0", 0x506A, &[], 0.01, ""),
        0x167 => r("Man on the Roof", "ani", "gRoofManSkel", "gRoofManIdleAnim", 0x5050, &["gRoofManEyeOpenTex"], 0.01, ""),
        0x13C => r("Cucco Lady", "ane", "gCuccoLadySkel", "gObjOsAnim_07D0", 0x5036, &["gCuccoLadyEyeOpenTex"], 0.01, "object_os_anime"),
        0x132 => r("Master Craftsman", "toryo", "object_toryo_Skel_007150", "object_toryo_Anim_000E50", 0x5028, &[], 0.01, ""),
        0x16C => r("Graveyard Boy", "cs", "gGraveyardKidSkel", "gGraveyardKidIdleAnim", 0x2022, &["gGraveyardKidEyesOpenTex"], 0.01, ""),
        0xED => r("Frog", "fr", "object_fr_Skel_00B498", "object_fr_Anim_001534", 0, &["object_fr_Tex_0059A0", "object_fr_Tex_0059A0"], 0.015, ""),
        0x13E => r("Bean Salesman", "ms", "gBeanSalesmanSkel", "gBeanSalesmanEatingAnim", 0x405E, &[], 0.015, ""),
        0x164 => r("King Zora", "kz", "gKzSkel", "gKzIdleAnim", 0x401A, &["gKzEyeOpenTex"], 0.01, ""),
        0xA1 => r("Princess Ruto", "ru1", "gRutoChildSkel", "gRutoChildWaitHandsOnHipsAnim", 0x402C, &["gRutoChildEyeOpenTex", "gRutoChildMouthClosedTex"], 0.01, ""),
        0x124 | 0x1CE => ZORA,
        0x1AE | 0x13D => GORON,
        0x1CA | 0x17B => SCARECROW,
        0x138 => r("Gerudo", "ge1", "gGerudoWhiteSkel", "gGerudoWhiteIdleAnim", 0x6000, &["gGerudoWhiteEyeOpenTex"], 0.01, ""),
        0x186 => r("Gerudo Guard", "gla", "gObjectGlaSkel", "gObjectGlaIdleAnim", 0x6004, &["gObjectGlaEyeOpenTex"], 0.01, ""),
        0x16A => r("Carpet Merchant", "js", "gCarpetMerchantSkel", "gCarpetMerchantIdleAnim", 0x6074, &[], 0.01, ""),
        0x84 => r("Talon", "ta", "gTalonSkel", "gTalonSleepAnim", 0x204B, &["gTalonEyeOpenTex", "gTalonHeadSkinTex"], 0.01, ""),
        0x98 => r("Darunia", "du", "gDaruniaSkel", "gDaruniaIdleAnim", 0x301A, &["gDaruniaEyeOpenTex", "gDaruniaMouthSeriousTex", "gDaruniaNoseSeriousTex"], 0.01, ""),
        0x1C6 => r("Cow", "cow", "gCowBodySkel", "gCowBodyChewAnim", 0, &[], 0.01, ""),
        0x19 => r("Cucco", "niw", "gCuccoSkel", "gCuccoAnim", 0, &[], 0.01, ""),
        0x5A => r("Lord Jabu-Jabu", "jj", "gObjectJjSkel", "gObjectJjAnim", 0, &["gObjectJjEyeOpenTex"], 0.087, ""),
        0x115 => r("Skull Kid", "skj", "gSkullKidSkel", "gSkullKidPlayFluteAnim", 0x10BC, &[], 0.01, ""),
        0x195 => r("Business Scrub", "shopnuts", "gBusinessScrubSkel", "gBusinessScrubLookAroundAnim", 0x10A0, &[], 0.01, ""),
        0x192 | 0x1A3 => SCRUB,
        0x1BF => r("Poe Guide", "po_field", "gPoeFieldSkel", "gPoeFieldFloatAnim", 0, &[], 0.01, ""),
        _ => return None,
    })
}

/// `visible_actor(actor, params)`: the frog conductor, invisible Ocarina stumps and Jabu's collision actors have no model.
pub fn visible_actor(actor: i64, params: i64) -> bool {
    !((actor == 0xED && params == 0) || (actor == 0x115 && (params & 0xffff) >> 10 > 2) || (actor == 0x5A && params != -1))
}

/// `greeting_for(actor, params, course)`
pub fn greeting_for(actor: i64, params: i64, course: &str) -> R<i64> {
    Ok(match actor {
        0x14D => match (params >> 6) & 63 {
            1 => 0x2064,
            2 => 0x2068,
            3 => 0x206C,
            4 => 0x206F,
            5 => 0x2071,
            6 => 0x4002,
            7 => 0x4003,
            8 => 0x4004,
            10 => 0x6079,
            11 => 0x10C0,
            12 => 0x10C4,
            _ => 0x2064,
        },
        0x1AE => match params & 31 {
            0 => 0x3011,
            4 => 0x300A,
            5 => 0x3009,
            6 => 0x3008,
            7 => 0x3014,
            8 => 0x3016,
            9 => 0x3017,
            10 => 0x300E,
            11 => 0x3024,
            12 => 0x3064,
            // a sign variant without a known text: the generic one
            _ => 0x3011,
        },
        0x1CE => match params & 63 {
            6 => 0x4020,
            7 => 0x4021,
            _ => 0x4006,
        },
        0x192 => {
            let base = (params >> 8) & 255;
            if base != 0 {
                0x1000 + base
            } else {
                0
            }
        }
        0x178 => 0x5063,
        0xB3 => {
            if course == "kakariko_village" {
                0x7029
            } else {
                0x7002
            }
        }
        0x124 => 0x4053,
        0x17B => 0x4076,
        0x13D => 0x3049,
        0x142 => 0x702D,
        0x16A => 0x6077,
        0xCB => 0x203F,
        a => resident(a).ok_or_else(|| format!("KeyError: {a:#x}"))?.greeting,
    })
}
