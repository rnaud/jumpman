//! The real MK64 minimap of the race HUD: the ROM's course outline texture (`gCourseOutlineTextures`, exported as
//! `hud-minimap-<course>.png`) and the placement constants of `track_minimap_settings` (`code_8006E9C0.c`) and
//! `render_mini_map_finish_line` / `draw_minimap_character` (`render_objects.c`): a kart at `(x, z)` is drawn at
//! `(mapX + x * markerScale, mapY + z * markerScale)` in the texture's pixels, the finish line at `(mapX, mapY)`
//! (+ a few per-course pixels). Engine-free.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    /// Texture size (`D_800E5548`).
    pub w: f32,
    pub h: f32,
    /// `gMiniMapMarkerScale`.
    pub scale: f64,
    /// `gMiniMapX` / `gMiniMapY`.
    pub map_x: f64,
    pub map_y: f64,
    /// The finish line's per-course nudge in y (`render_mini_map_finish_line`).
    pub finish_dy: f64,
}

const fn s(w: f32, h: f32, scale: f64, map_x: f64, map_y: f64, finish_dy: f64) -> Spec {
    Spec { w, h, scale, map_x, map_y, finish_dy }
}

/// The 16 race tracks by decomp directory name.
pub fn spec(dir: &str) -> Option<Spec> {
    Some(match dir {
        "mario_raceway" => s(64., 64., 0.022, 6., 28., -2.),
        "choco_mountain" => s(64., 64., 0.022, 19., 37., -16.),
        "bowsers_castle" => s(64., 64., 0.0174, 12., 48., 0.),
        "banshee_boardwalk" => s(64., 64., 0.016, 55., 39., 0.),
        "yoshi_valley" => s(64., 64., 0.018, 61., 38., 0.),
        "frappe_snowland" => s(64., 64., 0.016, 36., 40., 0.),
        "koopa_troopa_beach" => s(64., 64., 0.014, 40., 21., 0.),
        "royal_raceway" => s(64., 64., 0.014, 37., 50., 0.),
        "luigi_raceway" => s(64., 96., 0.0155, 45., 60., 0.),
        "moo_moo_farm" => s(64., 64., 0.0155, 18., 36., 0.),
        "toads_turnpike" => s(128., 64., 0.013, 57., 44., 0.),
        "kalimari_desert" => s(64., 96., 0.015, 55., 27., 4.),
        "sherbet_land" => s(64., 64., 0.015, 52., 33., 0.),
        "rainbow_road" => s(64., 96., 0.0103, 39., 55., 0.),
        "wario_stadium" => s(64., 64., 0.0155, 53., 35., 0.),
        "dks_jungle_parkway" => s(64., 64., 0.0155, 29., 47., 0.),
        _ => return None,
    })
}

impl Spec {
    /// A kart at course position `(x, z)` as fractions (0..1) of the texture; a mirrored course flips `mapX`
    /// (`gMiniMapX = width - gMiniMapX`) and the kart's x.
    pub fn uv(&self, x: f64, z: f64, mirrored: bool) -> (f32, f32) {
        let (map_x, x) = if mirrored { (self.w as f64 - self.map_x, -x) } else { (self.map_x, x) };
        (((map_x + x * self.scale) / self.w as f64) as f32, ((self.map_y + z * self.scale) / self.h as f64) as f32)
    }
    /// The finish line marker (top-left of its 8 x 8 picture sits at the texture position `(mapX, mapY)`; pictures are
    /// placed by their centre in the HUD, so add half the icon).
    pub fn finish_uv(&self, mirrored: bool) -> (f32, f32) {
        let map_x = if mirrored { self.w as f64 - self.map_x } else { self.map_x };
        (((map_x + 4.) / self.w as f64) as f32, ((self.map_y + self.finish_dy + 4.) / self.h as f64) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_race_track_has_a_spec_and_the_arenas_do_not() {
        for t in super::super::kart::TRACKS {
            assert!(spec(t.dir).is_some(), "{}", t.dir);
        }
        assert!(spec("big_donut").is_none());
    }

    #[test]
    fn the_origin_is_the_texture_offset_and_scale_moves_karts() {
        let m = spec("mario_raceway").unwrap();
        let (u, v) = m.uv(0., 0., false);
        assert!((u - 6. / 64.).abs() < 1e-6 && (v - 28. / 64.).abs() < 1e-6);
        let (u, _) = m.uv(1000., 0., false);
        assert!((u - (6. + 22.) / 64.).abs() < 1e-5, "0.022 pixels per unit: {u}");
        // Toad's Turnpike is the 128 x 64 one.
        assert_eq!(spec("toads_turnpike").unwrap().w, 128.);
        // Mirror mode flips the layout.
        let (um, _) = m.uv(0., 0., true);
        assert!((um - 58. / 64.).abs() < 1e-6);
        // The finish line of Choco Mountain sits 16 pixels higher than the others.
        let c = spec("choco_mountain").unwrap();
        assert!((c.finish_uv(false).1 - (37. - 16. + 4.) / 64.).abs() < 1e-6);
    }
}
