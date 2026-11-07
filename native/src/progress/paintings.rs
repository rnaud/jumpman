//! Port of `painting.ts` (`paintingDestination`) and the gameplay half of `castlePaintings.ts`
//! (the ripple state machine and the Dire Dire Docks painting sliding away after its star).
use super::data::V3;
use super::save::{SaveFile, FLAG_DDD_MOVED};
use std::collections::HashMap;

/// `get_painting_warp_node` indexes the current area's table by floor type.
pub fn painting_destination(kind: i32, height: f64, nodes: &HashMap<i32, String>) -> Option<String> {
    let index = kind - 0xd3;
    if !(0..0x2d).contains(&index) || (index >= 0x2a && height >= 80.0) {
        return None;
    }
    nodes.get(&index).cloned()
}

/// One painting's ripple parameters (from `paintingArt.layouts`).
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PaintingSpec {
    pub name: String,
    pub index: i32,
    pub id: i32,
    pub pos: V3,
    pub rotation: [f64; 2],
    pub size: f64,
    pub dimensions: [f64; 2],
    pub opacity: f64,
    pub environment: bool,
    pub magnitude: [f64; 2],
    pub decay: [f64; 2],
    pub rate: [f64; 2],
    pub dispersion: [f64; 2],
    pub parts: Vec<PaintingPart>,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct PaintingPart {
    pub shape: String,
    pub texture: String,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct PaintingShape {
    /// x, y, movable, u, v
    pub vertices: Vec<[f64; 5]>,
    pub triangles: Vec<u32>,
}

#[derive(serde::Deserialize, Clone, Debug, Default)]
pub struct PaintingArt {
    #[serde(default)]
    pub shapes: HashMap<String, PaintingShape>,
    #[serde(default)]
    pub layouts: Vec<PaintingSpec>,
}

/// Ripple state shared by every painting (only one is ever active).
#[derive(Debug, Default)]
pub struct PaintingAnim {
    pub specs: Vec<PaintingSpec>,
    floor_type: i32,
    /// Index into `specs` of the rippling painting.
    pub active: Option<usize>,
    pub frame: i32,
    pub magnitude: f64,
    pub phase: usize,
    pub origin: [f64; 2],
    /// DDD painting x offset (slides away once the DDD star is collected).
    pub docks_x: Option<f64>,
    /// Set for one tick when `active` changed to flat (view must reset geometry).
    pub dirty: bool,
}

pub const DDD_PAINTING_HOME: f64 = 3456.0;
pub const DDD_PAINTING_MOVED: f64 = 5529.6;

impl PaintingAnim {
    pub fn new(specs: Vec<PaintingSpec>, save: &SaveFile) -> Self {
        let mut p = Self { specs, floor_type: -1, ..Default::default() };
        p.reset(save);
        p
    }

    pub fn reset(&mut self, save: &SaveFile) {
        if self.active.take().is_some() {
            self.dirty = true;
        }
        self.floor_type = -1;
        self.docks_x = self.specs.iter().any(|s| s.name == "ddd").then(|| if save.has_flag(FLAG_DDD_MOVED) { DDD_PAINTING_MOVED } else { DDD_PAINTING_HOME });
    }

    /// `step`: Mario's floor type (or -1) and position drive the ripple.
    pub fn step(&mut self, floor_type: i32, mario: V3, save: &mut SaveFile) {
        if let Some(x) = &mut self.docks_x {
            if save.has_star("ddd", 1) && *x < DDD_PAINTING_MOVED {
                *x = (*x + 20.0).min(DDD_PAINTING_MOVED);
                if *x == DDD_PAINTING_MOVED {
                    save.set_flag(FLAG_DDD_MOVED);
                }
            }
        }
        if floor_type != self.floor_type {
            self.floor_type = floor_type;
            for (i, spec) in self.specs.iter().enumerate() {
                let wobble = floor_type - (0xa6 + 3 * spec.id);
                let enter = floor_type - (0xd3 + 3 * spec.id);
                let entering = (0..3).contains(&enter);
                if !entering && !(0..3).contains(&wobble) {
                    continue;
                }
                if self.active.is_some() && self.active != Some(i) {
                    self.dirty = true;
                }
                if self.active != Some(i) || !spec.environment {
                    self.frame = 0;
                }
                self.active = Some(i);
                self.phase = usize::from(entering);
                self.magnitude = spec.magnitude[self.phase];
                let slot = f64::from(if entering { enter } else { wobble } + 1) / 4.0;
                let flat_wall = spec.environment && !entering;
                self.origin = [
                    if flat_wall { 0.5 * spec.size } else { slot * spec.size },
                    if flat_wall { spec.size / 2.0 } else { (mario[1] - spec.pos[1] + 50.0).clamp(0.0, spec.size) },
                ];
                if spec.rotation[0] != 0.0 && entering {
                    self.origin = [(mario[0] - spec.pos[0]).clamp(0.0, spec.size), (spec.pos[2] - mario[2]).clamp(0.0, spec.size)];
                }
                break;
            }
        }
        let Some(i) = self.active else { return };
        self.frame += 1;
        let spec = &self.specs[i];
        self.magnitude *= spec.decay[self.phase];
        if spec.environment && self.phase == 1 && self.magnitude <= spec.magnitude[0] {
            self.magnitude = spec.magnitude[0];
            self.phase = 0;
        } else if self.magnitude <= 1.0 {
            self.active = None;
            self.dirty = true;
        }
    }

    /// z displacement of a movable vertex at (x, y) (in painting units) for the current frame.
    pub fn displacement(&self, x: f64, y: f64) -> f64 {
        let Some(i) = self.active else { return 0.0 };
        let spec = &self.specs[i];
        let scale = spec.size / 614.0;
        let distance = jumpman_sim::math::js_hypot(x * scale - self.origin[0], y * scale - self.origin[1]);
        let elapsed = f64::from(self.frame - 1) - distance / spec.dispersion[self.phase];
        if elapsed < 0.0 {
            0.0
        } else {
            (self.magnitude * (spec.rate[self.phase] * 2.0 * std::f64::consts::PI * elapsed).cos()).round()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> HashMap<i32, String> {
        HashMap::from([(0, "bob".to_string()), (1, "bob".to_string()), (0x2a, "hmc".to_string())])
    }

    #[test]
    fn floor_types_map_to_courses() {
        assert_eq!(painting_destination(0xd3, 0.0, &table()).as_deref(), Some("bob"));
        assert_eq!(painting_destination(0xd4, 500.0, &table()).as_deref(), Some("bob"));
        assert_eq!(painting_destination(0xd2, 0.0, &table()), None);
        assert_eq!(painting_destination(0xd3 + 0x2d, 0.0, &table()), None);
        // The HMC painting (index >= 0x2a) only warps when Mario is below 80 units.
        assert_eq!(painting_destination(0xd3 + 0x2a, 79.0, &table()).as_deref(), Some("hmc"));
        assert_eq!(painting_destination(0xd3 + 0x2a, 80.0, &table()), None);
    }

    #[test]
    fn ripple_starts_on_wobble_floor_and_decays() {
        let spec = PaintingSpec {
            name: "bob".into(),
            index: 0,
            id: 0,
            pos: [0.0; 3],
            rotation: [0.0, 0.0],
            size: 614.0,
            dimensions: [1.0, 1.0],
            opacity: 1.0,
            environment: false,
            magnitude: [20.0, 80.0],
            decay: [0.9, 0.95],
            rate: [0.1, 0.1],
            dispersion: [10.0, 10.0],
            parts: vec![],
        };
        let mut save = SaveFile::in_memory(1);
        let mut p = PaintingAnim::new(vec![spec], &save);
        p.step(0xa6, [100.0, 0.0, 0.0], &mut save);
        assert_eq!(p.active, Some(0));
        assert_eq!(p.phase, 0);
        assert!(p.displacement(0.0, 0.0) != 0.0 || p.magnitude > 1.0);
        for _ in 0..200 {
            p.step(0xa6, [0.0; 3], &mut save);
        }
        assert_eq!(p.active, None, "ripple dies out");
        p.step(0xd3, [100.0, 0.0, 0.0], &mut save);
        assert_eq!(p.phase, 1, "entering uses the strong phase");
    }

    #[test]
    fn docks_painting_slides_after_the_star() {
        let mut spec = PaintingSpec { name: "ddd".into(), index: 0, id: 3, pos: [0.0; 3], rotation: [0.0; 2], size: 614.0, dimensions: [1.0; 2], opacity: 1.0, environment: false, magnitude: [1.0; 2], decay: [0.9; 2], rate: [0.1; 2], dispersion: [1.0; 2], parts: vec![] };
        spec.id = 3;
        let mut save = SaveFile::in_memory(1);
        let mut p = PaintingAnim::new(vec![spec], &save);
        assert_eq!(p.docks_x, Some(DDD_PAINTING_HOME));
        save.collect_star("ddd", 1, 0);
        for _ in 0..200 {
            p.step(-1, [0.0; 3], &mut save);
        }
        assert_eq!(p.docks_x, Some(DDD_PAINTING_MOVED));
        assert!(save.has_flag(FLAG_DDD_MOVED));
    }
}
