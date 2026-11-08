//! Which MK64 sequence belongs to which course: the `switch` of `play_music_for_current_track`
//! (`src/racing/race_logic.c`) and the `SeqId` enum of `include/seq_ids.h`.

pub const SEQ_TITLE: u32 = 1;
pub const SEQ_MENU: u32 = 2;
pub const SEQ_RACEWAY: u32 = 3;
pub const SEQ_FARM: u32 = 4;
pub const SEQ_MOUNTAIN: u32 = 5;
pub const SEQ_BEACH: u32 = 6;
pub const SEQ_SCARY: u32 = 7;
pub const SEQ_SNOW: u32 = 8;
pub const SEQ_CASTLE: u32 = 9;
pub const SEQ_DESERT: u32 = 10;
pub const SEQ_RACE_STARTING: u32 = 11;
pub const SEQ_FINAL_LAP: u32 = 12;
pub const SEQ_FINISH_FIRST: u32 = 13;
pub const SEQ_FINISH_OTHER: u32 = 14;
pub const SEQ_FINISH_LOSE: u32 = 15;
pub const SEQ_RESULTS_WIN: u32 = 16;
pub const SEQ_STAR: u32 = 17;
pub const SEQ_RAINBOW: u32 = 18;
pub const SEQ_JUNGLE: u32 = 19;
pub const SEQ_TURNPIKE: u32 = 21;
pub const SEQ_RESULTS_LOSE: u32 = 24;
pub const SEQ_BATTLE: u32 = 25;
/// Sequences in the archive (`SEQ_COUNT`); id 0 is the sound-effect player.
pub const SEQ_COUNT: u32 = 30;

/// The 20 courses (decomp directory name, which is the id after `mk64_`) and the theme the game plays on each.
pub const COURSE_THEMES: [(&str, u32); 20] = [
    ("mario_raceway", SEQ_RACEWAY),
    ("luigi_raceway", SEQ_RACEWAY),
    ("royal_raceway", SEQ_RACEWAY),
    ("wario_stadium", SEQ_RACEWAY),
    ("toads_turnpike", SEQ_TURNPIKE),
    ("yoshi_valley", SEQ_FARM),
    ("moo_moo_farm", SEQ_FARM),
    ("choco_mountain", SEQ_MOUNTAIN),
    ("block_fort", SEQ_MOUNTAIN),
    ("double_deck", SEQ_MOUNTAIN),
    ("kalimari_desert", SEQ_DESERT),
    ("koopa_troopa_beach", SEQ_BEACH),
    ("bowsers_castle", SEQ_CASTLE),
    ("banshee_boardwalk", SEQ_SCARY),
    ("frappe_snowland", SEQ_SNOW),
    ("sherbet_land", SEQ_SNOW),
    ("rainbow_road", SEQ_RAINBOW),
    ("dks_jungle_parkway", SEQ_JUNGLE),
    ("skyscraper", SEQ_BATTLE),
    ("big_donut", SEQ_BATTLE),
];

/// The course theme for a decomp directory name (`mario_raceway`, with or without the `mk64_` prefix).
pub fn theme_for(course: &str) -> Option<u32> {
    let dir = course.strip_prefix(crate::game::mk64::PREFIX).unwrap_or(course);
    COURSE_THEMES.iter().find(|(d, _)| *d == dir).map(|(_, s)| *s)
}

/// Rank for the decomp's finish logic (`func_800C76C0`: 0 = first and gets the winner's fanfare, ranks
/// 1-3 the "other" fanfare and the results theme, 4 and worse the losing fanfare) from a place among
/// `racers`. The game races eight karts; a four-kart field is spread over the same thresholds.
pub fn decomp_rank(place: usize, racers: usize) -> u8 {
    if racers <= 1 {
        return 0;
    }
    (place * 8 / racers).min(7) as u8
}
