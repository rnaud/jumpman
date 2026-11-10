//! Port of `scripts/export-oot-icons.py`: the 2D item / quest icons of `icon_item_static` (32x32 RGBA32, 0x1000 apart) and
//! `icon_item_24_static` (24x24 RGBA32, 0x900 apart) as `assets/oot-icons/<id>.png` plus `index.json`. The id order is the decomp's
//! `assets/xml/textures/icon_item_static.xml` / `icon_item_24_static.xml`; the ids must stay identical to the Python script's lists
//! (a test below checks that when the script is next to the crate).
use super::model::png;
use super::Ctx;
use crate::pyc::R;
use std::path::Path;

pub const ITEM_IDS: &str = "deku_stick deku_nut bomb bow arrow_fire dins_fire slingshot ocarina_fairy ocarina_time bombchu hookshot longshot \
arrow_ice farores_wind boomerang lens_of_truth magic_bean hammer arrow_light nayrus_love bottle_empty \
bottle_potion_red bottle_potion_green bottle_potion_blue bottle_fairy bottle_fish bottle_milk_full \
bottle_rutos_letter bottle_blue_fire bottle_bug bottle_big_poe bottle_milk_half bottle_poe weird_egg chicken \
zeldas_letter mask_keaton mask_skull mask_spooky mask_bunny_hood mask_goron mask_zora mask_gerudo mask_truth \
sold_out pocket_egg pocket_cucco cojiro odd_mushroom odd_potion poachers_saw broken_gorons_sword prescription \
eyeball_frog eye_drops claim_check bow_fire bow_ice bow_light sword_kokiri sword_master sword_biggoron \
shield_deku shield_hylian shield_mirror tunic_kokiri tunic_goron tunic_zora boots_kokiri boots_iron boots_hover \
bullet_bag_30 bullet_bag_40 bullet_bag_50 quiver_30 quiver_40 quiver_50 bomb_bag_20 bomb_bag_30 bomb_bag_40 \
gorons_bracelet silver_gauntlets golden_gauntlets scale_silver scale_golden broken_giants_knife adults_wallet \
giants_wallet deku_seeds fishing_pole";
pub const QUEST_IDS: &str = "medallion_forest medallion_fire medallion_water medallion_spirit medallion_shadow medallion_light \
kokiri_emerald goron_ruby zora_sapphire stone_of_agony gerudos_card gold_skulltula heart_container heart_piece \
boss_key compass dungeon_map small_key magic_jar_small magic_jar_big";

/// (segment, ids, edge in pixels, bytes per icon).
const FILES: [(&str, &str, usize, usize); 2] = [("icon_item_static", ITEM_IDS, 32, 0x1000), ("icon_item_24_static", QUEST_IDS, 24, 0x900)];

/// Writes the icon PNGs and `index.json` into `dir` (`<stage>/assets/oot-icons`).
pub fn export(ctx: &Ctx, dir: &Path) -> R<()> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut index = Vec::new();
    for (segment, ids, edge, step) in FILES {
        let data = ctx.segment(segment)?;
        for (i, name) in ids.split_whitespace().enumerate() {
            let pixels = data.get(i * step..i * step + edge * edge * 4).ok_or_else(|| format!("{segment} is too short for {name}"))?;
            std::fs::write(dir.join(format!("{name}.png")), png(edge, edge, pixels)).map_err(|e| e.to_string())?;
            index.push(format!("\"{name}\":{{\"file\":\"{name}.png\",\"w\":{edge},\"h\":{edge}}}"));
        }
    }
    std::fs::write(dir.join("index.json"), format!("{{\"version\":1,\"icons\":{{{}}}}}\n", index.join(","))).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_ids(src: &str, name: &str) -> Option<String> {
        let start = src.find(&format!("{name} = ("))?;
        let body = &src[start..];
        let body = &body[..body.find(".split()")?];
        let mut out = String::new();
        for line in body.lines().skip(0) {
            if let (Some(a), Some(b)) = (line.find('\''), line.rfind('\'')) {
                if b > a {
                    out.push_str(&line[a + 1..b]);
                }
            }
        }
        Some(out)
    }

    #[test]
    fn ids_match_the_python_exporter() {
        let Ok(src) = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/export-oot-icons.py")) else { return };
        for (name, ids) in [("ITEM_IDS", ITEM_IDS), ("QUEST_IDS", QUEST_IDS)] {
            let py = python_ids(&src, name).unwrap();
            assert_eq!(py.split_whitespace().collect::<Vec<_>>(), ids.split_whitespace().collect::<Vec<_>>(), "{name}");
        }
    }

    #[test]
    fn counts_and_uniqueness() {
        let all: Vec<&str> = ITEM_IDS.split_whitespace().chain(QUEST_IDS.split_whitespace()).collect();
        assert_eq!((ITEM_IDS.split_whitespace().count(), QUEST_IDS.split_whitespace().count()), (90, 20));
        let set: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(set.len(), all.len());
    }
}
