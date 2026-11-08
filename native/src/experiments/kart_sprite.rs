//! Which pre-rendered kart frame MK64 draws, and how big and where it is drawn. Engine-free.
//!
//! The game never draws a kart model: every racer is a 64 x 64 sprite (two 64 x 32 textures stacked) picked from
//! 321 frames per character that were rendered offline. `func_8002934C` (`src/player_controller.c`) picks
//! `animGroupSelector` (the slope of the road under the kart, 9 groups) and `animFrameSelector` (the view angle:
//! `unk_048 + rotation[1] + unk_0C0`, i.e. camera heading plus kart heading plus the drift slip angle), and
//! `src/kart_dma.c` (`DECLARE_KART_GROUP`, `Table0`, `Table1`) maps both to a frame number. `render_kart`
//! (`src/render_player.c`) draws that frame on the 18 x 18 quad `gPlayerOneVtx` scaled by `gCharacterSize` (0.75).
//! The frames are exported by `scripts/export-mk64.py` into `<racer>-kart-frames.png`.

use std::f64::consts::PI;

/// Racers in `characterId` order (`gCharacterSize`).
pub const CHARACTERS: [&str; 8] = ["mario", "luigi", "yoshi", "toad", "donkeykong", "wario", "peach", "bowser"];
/// Atlas layout of `<racer>-kart-frames.png` (see `export_kart_frames`).
pub const COLUMNS: usize = 16;
pub const ROWS: usize = 21;
pub const FRAMES: usize = 321;
/// The quad is 18 units wide and tall (`gPlayerOneVtx`: x -9..9, y 0..18), scaled by `gCharacterSize` = 0.75;
/// course units are 8 sim units.
pub const SPRITE_UNITS: f64 = 18. * 0.75 * 8.;
/// The sprite hangs 6 * 0.75 course units in front of the kart (`z = -6` in the quad, towards the camera).
pub const TOWARDS_CAMERA: f64 = 6. * 0.75 * 8.;
/// `render_kart` puts the quad `sp144 - 2.0` = -0.5 course units from the tyres' contact height.
pub const BASE_DROP: f64 = 0.5 * 8.;

pub fn frames_file(character: usize) -> String {
    format!("{}-kart-frames.png", CHARACTERS[character.min(7)])
}

/// MK64's `DEGREES(n)`: s16 angle units.
pub const fn degrees(n: i32) -> i32 {
    n * 0x10000 / 360
}

/// `func_80029200`: the pitch group of the road under the kart (0 = steep downhill ... 4 = flat ... 8 = steep uphill
/// in the game's sign, `slopeAccel`).
pub fn slope_group(slope_accel: i32) -> usize {
    let s = slope_accel;
    let mut group = 4;
    if s < -0x71B {
        group = 0;
    }
    if s < -0x4F9 && s > -degrees(10) {
        group = 1;
    }
    if s <= -degrees(3) && s > -degrees(7) {
        group = 2;
    }
    if s <= -degrees(2) && s > -degrees(3) {
        group = 3;
    }
    if s < degrees(2) && s > -degrees(2) {
        group = 4;
    }
    if s >= degrees(2) && s < degrees(3) {
        group = 5;
    }
    if s >= degrees(3) && s < degrees(7) {
        group = 6;
    }
    if s >= degrees(7) && s < degrees(10) {
        group = 7;
    }
    if s >= degrees(10) {
        group = 8;
    }
    group
}

/// The view angle as the game's u16: `unk_048 + rotation[1] + unk_0C0` for a kart seen at `relative` radians
/// (camera heading to the kart minus the kart's displayed heading; 0 = seen from behind).
pub fn view_angle(relative: f64) -> u16 {
    ((relative / (2. * PI) * 65536.).round() as i64).rem_euclid(65536) as u16
}

/// Result of `func_8002934C`: `animFrameSelector` (0..=34) and whether the frame is drawn mirrored (`unk_002`
/// side bit, the `flipOffset` vertices of `render_player`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub index: usize,
    pub mirrored: bool,
}

/// `animFrameSelector` for a view angle. `spinning` is `BANANA_SPINOUT_EFFECT` / `DRIVING_SPINOUT_EFFECT`: the
/// spin uses 9 degree steps everywhere (frames 0..=19, anything past that wraps to 0).
pub fn view(angle: u16, spinning: bool) -> View {
    let mut first = (angle / 128) as i32;
    let mirrored = first >= 0x101;
    if mirrored {
        first = 0x201 - first;
    }
    let (div, add) = if spinning {
        (degrees(9), 0)
    } else if first < 0x51 {
        (0x208, 0)
    } else {
        (degrees(9), 0xF)
    };
    let mut a = angle as i32;
    if a >= 0x7FF9 {
        a = (0x10000 - a) & 0xFFFF;
        if spinning {
            a /= div;
            if a == 0 {
                a = 1;
            }
            return finish(a + add, mirrored, spinning);
        }
    }
    finish(a / div + add, mirrored, spinning)
}

fn finish(index: i32, mirrored: bool, spinning: bool) -> View {
    let mut index = index.min(0x22);
    if spinning && index >= 0x14 {
        index = 0;
    }
    View { index: index as usize, mirrored }
}

/// `DECLARE_KART_GROUP`: frame number of entry `i` of group `g` (0..=17).
pub fn group_frame(g: usize, i: usize) -> usize {
    if g <= 8 {
        if i < 21 {
            21 * g + i
        } else {
            [195, 195, 215, 235, 235, 235, 255, 275, 275][g] + (i - 21)
        }
    } else {
        [189, 189, 209, 229, 229, 229, 249, 269, 269][g - 9] + i.min(19)
    }
}

/// `gKartTextureTable0[selector][index]`: normal driving frames (`Table0 = Group8 ... Group0`).
pub fn table0_frame(selector: usize, index: usize) -> usize {
    group_frame(8 - selector.min(8), index)
}

/// `gKartTextureTable1[selector][index]`: spin-out frames (`Table1 = Group17 ... Group9`).
pub fn table1_frame(selector: usize, index: usize) -> usize {
    group_frame(17 - selector.min(8), index)
}

/// What the kart is doing, as far as the sprite choice goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pose {
    /// `slopeAccel` (s16 units).
    pub slope_accel: i32,
    /// Spun out by a banana or shell (`BANANA_SPINOUT_EFFECT`).
    pub spinning: bool,
}

/// The frame to draw (0..=288) and whether to mirror it, from the view angle in the game's u16 units.
pub fn pick(angle: u16, pose: Pose) -> (usize, bool) {
    let v = view(angle, pose.spinning);
    let selector = slope_group(pose.slope_accel);
    let frame = if pose.spinning && v.index != 0 { table1_frame(selector, v.index) } else { table0_frame(selector, v.index) };
    (frame, v.mirrored)
}

/// First frame of the tumble animation (`gKartTextureTumbles`, 32 frames).
pub const TUMBLE_FIRST: usize = 289;

/// UV scale and offset on the atlas for `frame`, mirrored by flipping the horizontal scale.
pub fn uv(frame: usize, mirrored: bool) -> ([f32; 2], [f32; 2]) {
    let (col, row) = ((frame % COLUMNS) as f32, (frame / COLUMNS) as f32);
    let (w, h) = (1. / COLUMNS as f32, 1. / ROWS as f32);
    if mirrored {
        ([-w, h], [(col + 1.) * w, row * h])
    } else {
        ([w, h], [col * w, row * h])
    }
}

/// Rotation of the sprite about the view axis (`unk_050` before smoothing), radians: the road's cross slope `unk_0CC`
/// seen from behind plus its pitch `unk_0D4` seen from the side. `pitch` / `roll` are s16 units (see
/// `Kart::view_tilt`), `angle` the view angle.
pub fn screen_roll(pitch: f64, roll: f64, angle: u16) -> f64 {
    let a = angle as f64 / 65536. * 2. * PI;
    (pitch * a.sin() + roll * a.cos()) * PI / 32768.
}

/// `render_player_shadow`: the shadow quad is `SPRITE_UNITS` wide, scaled by `1 - 0.02 * height` (course units) in the
/// air and not drawn from 50 units up (`render_player`: `surfaceDistance[2] <= 50`). `height` in sim units.
pub fn shadow_scale(height: f64, airborne: bool) -> Option<f64> {
    let h = height / 8.;
    if h > 50. {
        return None;
    }
    Some(if airborne { (1. - h * 0.02).clamp(0., 1.) } else { 1. })
}

/// The shadow centre is shifted 2 course units towards the camera (`spB0` / `spAC`), and sits 1 unit over the road.
pub const SHADOW_SHIFT: f64 = 2. * 8.;
pub const SHADOW_LIFT: f64 = 8.;

#[cfg(test)]
mod tests {
    use super::*;

    fn deg(d: f64) -> u16 {
        view_angle(d.to_radians())
    }

    #[test]
    fn straight_behind_is_frame_zero_and_straight_ahead_is_the_last_frame() {
        assert_eq!(view(0, false), View { index: 0, mirrored: false });
        assert_eq!(view(deg(180.), false).index, 34);
    }

    #[test]
    fn the_35_angles_are_2_86_degree_steps_up_to_57_degrees_then_9_degree_steps() {
        // 0x208 = 520 units = 2.857 degrees
        assert_eq!(view(deg(2.9), false).index, 1);
        assert_eq!(view(deg(10.), false).index, 3);
        assert_eq!(view(deg(56.), false).index, 19);
        // past 0x51 * 128 units (56.9 degrees) the steps are DEGREES(9) and start at frame 21
        assert_eq!(view(deg(58.), false).index, 21);
        assert_eq!(view(deg(90.), false).index, 25);
        assert_eq!(view(deg(171.), false).index, 34);
        // frame 20 is never used for driving
        assert!((0..=65535u16).all(|a| view(a, false).index != 20));
    }

    #[test]
    fn the_other_side_is_the_same_frame_mirrored() {
        for d in [5., 30., 60., 90., 120., 170.] {
            let (right, left) = (view(deg(d), false), view(deg(360. - d), false));
            assert!(!right.mirrored && left.mirrored, "{d}");
            assert!((right.index as i32 - left.index as i32).abs() <= 1, "{d}: {} vs {}", right.index, left.index);
        }
    }

    #[test]
    fn spin_outs_use_9_degree_steps_and_wrap_to_frame_zero() {
        assert_eq!(view(deg(10.), true).index, 1);
        assert_eq!(view(deg(100.), true).index, 11);
        assert_eq!(view(deg(179.), true).index, 19);
        assert_eq!(view(deg(180.), true).index, 0, "frames past 19 wrap to 0");
        assert_eq!(view(deg(359.), true).index, 1, "the mirrored side never gives 0 (the game bumps it to 1)");
    }

    #[test]
    fn slope_groups_follow_func_80029200() {
        assert_eq!(slope_group(0), 4);
        assert_eq!(slope_group(degrees(1)), 4);
        assert_eq!(slope_group(degrees(2)), 5);
        assert_eq!(slope_group(degrees(5)), 6);
        assert_eq!(slope_group(degrees(8)), 7);
        assert_eq!(slope_group(degrees(12)), 8);
        assert_eq!(slope_group(-degrees(2)), 3);
        assert_eq!(slope_group(-degrees(5)), 2);
        assert_eq!(slope_group(-degrees(8)), 1);
        assert_eq!(slope_group(-degrees(12)), 0);
    }

    #[test]
    fn frame_tables_match_kart_dma() {
        // Table0 = Group8 ... Group0: selector 4 is Group4 (frames 84..104 then the shared 235..248)
        assert_eq!(table0_frame(4, 0), 84);
        assert_eq!(table0_frame(4, 20), 104);
        assert_eq!(table0_frame(4, 21), 235);
        assert_eq!(table0_frame(4, 34), 248);
        // selector 0 is Group8 (168..188, 275..288); selector 8 is Group0 (0..20, 195..208)
        assert_eq!(table0_frame(0, 0), 168);
        assert_eq!(table0_frame(0, 34), 288);
        assert_eq!(table0_frame(8, 0), 0);
        assert_eq!(table0_frame(8, 21), 195);
        // groups 2 and 6 have their own back views
        assert_eq!(group_frame(2, 21), 215);
        assert_eq!(group_frame(6, 21), 255);
        // Table1 = Group17 ... Group9: selector 4 is Group13 = 229..248
        assert_eq!(table1_frame(4, 0), 229);
        assert_eq!(table1_frame(4, 19), 248);
        assert_eq!(table1_frame(8, 0), 189);
        assert_eq!(table1_frame(0, 19), 288);
        for g in 0..=17 {
            for i in 0..35 {
                assert!(group_frame(g, i) < TUMBLE_FIRST, "group {g} entry {i}");
            }
        }
    }

    #[test]
    fn pick_combines_slope_and_angle() {
        assert_eq!(pick(0, Pose::default()), (84, false));
        assert_eq!(pick(deg(90.), Pose::default()), (table0_frame(4, 25), false));
        let steep_up = Pose { slope_accel: degrees(12), ..Pose::default() };
        assert_eq!(pick(0, steep_up), (0, false));
        let spin = Pose { spinning: true, ..Pose::default() };
        assert_eq!(pick(deg(100.), spin).0, table1_frame(4, 11));
        assert_eq!(pick(deg(300.), spin).1, true);
    }

    #[test]
    fn uv_cells_cover_the_atlas_and_mirror_by_negative_scale() {
        let ((sx, sy), (ox, oy)) = {
            let (s, o) = uv(17, false);
            ((s[0], s[1]), (o[0], o[1]))
        };
        assert!((sx - 1. / 16.).abs() < 1e-6 && (sy - 1. / 21.).abs() < 1e-6);
        assert!((ox - 1. / 16.).abs() < 1e-6 && (oy - 1. / 21.).abs() < 1e-6);
        let (s, o) = uv(0, true);
        assert!(s[0] < 0. && (o[0] - 1. / 16.).abs() < 1e-6);
        assert!(FRAMES <= COLUMNS * ROWS);
    }

    #[test]
    fn sprite_size_matches_the_quad_and_character_size() {
        assert_eq!(SPRITE_UNITS, 108.);
        assert_eq!(TOWARDS_CAMERA, 36.);
    }

    #[test]
    fn the_shadow_shrinks_with_height_and_vanishes_at_50_units() {
        assert_eq!(shadow_scale(0., false), Some(1.));
        assert_eq!(shadow_scale(8. * 20., false), Some(1.), "driving over a bump keeps the full shadow");
        assert!((shadow_scale(8. * 10., true).unwrap() - 0.8).abs() < 1e-9);
        assert_eq!(shadow_scale(8. * 50., true), Some(0.));
        assert_eq!(shadow_scale(8. * 51., true), None);
    }

    #[test]
    fn tilt_is_roll_from_behind_and_pitch_from_the_side() {
        assert!((screen_roll(1000., 500., 0) - 500. * PI / 32768.).abs() < 1e-9);
        assert!((screen_roll(1000., 500., 0x4000) - 1000. * PI / 32768.).abs() < 1e-9);
    }
}
