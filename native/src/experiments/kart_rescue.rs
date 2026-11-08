//! Lakitu-style rescue for the MK64 free roam (twin of `src/game/experiments/kartRescue.ts`).
//!
//! When the kart falls off the course the screen fades to black, the kart is put back on the nearest path node to
//! where it last had ground under it, facing down the track, and the screen fades back in. Engine-free so it can be
//! tested without a window. Positions are world units (course units x 8), one `tick` is one 30 Hz sim step.
use jumpman_sim::math::Vec3 as V3;

/// Ticks of fade to black, of holding black (the teleport happens at its start), and of fade back in.
pub const FADE_OUT: u32 = 12;
pub const HOLD: u32 = 4;
pub const FADE_IN: u32 = 14;
/// How far below the lowest path node (world units) the kart may fall before it is fished out.
pub const FALL_MARGIN: f64 = 1500.;
/// Nodes ahead of the nearest one that the kart is turned towards (the path has one node every ~160 units).
const LOOKAHEAD: usize = 3;

#[derive(Clone, Debug)]
pub struct Lakitu {
    nodes: Vec<V3>,
    floor_limit: f64,
    safe: Option<V3>,
    /// Ticks since the rescue started; `None` while driving normally.
    timer: Option<u32>,
}

impl Lakitu {
    /// `path` in world units; the export's sentinel node (x = -32768 * scale) is skipped.
    pub fn new(path: &[V3]) -> Self {
        let nodes: Vec<V3> = path.iter().copied().filter(|p| p[0] > -30000.).collect();
        let floor_limit = nodes.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) - FALL_MARGIN;
        Self { nodes, floor_limit, safe: None, timer: None }
    }
    /// Remember the last place the kart had ground under it.
    pub fn note_grounded(&mut self, pos: V3) {
        self.safe = Some(pos);
    }
    pub fn forget(&mut self) {
        self.safe = None;
        self.timer = None;
    }
    pub fn should_rescue(&self, pos: V3) -> bool {
        self.timer.is_none() && pos[1] < self.floor_limit && !self.nodes.is_empty()
    }
    pub fn start(&mut self) {
        self.timer = Some(0);
    }
    pub fn active(&self) -> bool {
        self.timer.is_some()
    }
    /// Where to put the kart: the path node nearest to the last grounded position (or `from`), and the yaw
    /// (browser convention, forward = (sin y, cos y)) towards the node a few steps ahead.
    pub fn destination(&self, from: V3) -> Option<(V3, f64)> {
        let origin = self.safe.unwrap_or(from);
        let n = self.nodes.len();
        let distance = |p: &V3| (p[0] - origin[0]).hypot(p[2] - origin[2]) + 0.5 * (p[1] - origin[1]).abs();
        let (i, node) = self.nodes.iter().enumerate().min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))?;
        let ahead = self.nodes[(i + LOOKAHEAD) % n];
        Some((*node, (ahead[0] - node[0]).atan2(ahead[2] - node[2])))
    }
    /// Advance one tick. Returns true on the tick the kart must be teleported (the screen is fully black).
    pub fn tick(&mut self) -> bool {
        let Some(t) = self.timer else { return false };
        let next = t + 1;
        self.timer = if next >= FADE_OUT + HOLD + FADE_IN { None } else { Some(next) };
        next == FADE_OUT
    }
    /// 0 = clear, 1 = black.
    pub fn alpha(&self) -> f32 {
        match self.timer {
            None => 0.,
            Some(t) if t < FADE_OUT => (t + 1) as f32 / FADE_OUT as f32,
            Some(t) if t < FADE_OUT + HOLD => 1.,
            Some(t) => 1. - ((t - FADE_OUT - HOLD + 1) as f32 / FADE_IN as f32).min(1.),
        }
    }
    /// The kart is held (no driving) while the screen is fading out or black.
    pub fn holding(&self) -> bool {
        self.timer.is_some_and(|t| t < FADE_OUT + HOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring() -> Vec<V3> {
        // 40 nodes round a circle of radius 4000 (world units), plus the export's sentinel.
        let mut v: Vec<V3> = (0..40)
            .map(|i| {
                let a = i as f64 / 40. * std::f64::consts::TAU;
                [a.sin() * 4000., -400., a.cos() * 4000.]
            })
            .collect();
        v.push([-32768. * 8., -32768. * 8., -32768. * 8.]);
        v
    }

    #[test]
    fn picks_the_nearest_node_to_the_last_grounded_spot_and_faces_forward() {
        let mut l = Lakitu::new(&ring());
        l.note_grounded([4000., -400., 20.]);
        let (node, yaw) = l.destination([0., -9000., 0.]).unwrap();
        assert!((node[0] - 4000.).abs() < 200. && node[2].abs() < 600., "{node:?}");
        // Node 10 sits at +x; the next nodes run towards -z, so the kart faces -z (yaw ~ pi).
        assert!(yaw.cos() < -0.5, "{yaw}");
    }

    #[test]
    fn only_triggers_well_below_the_course_and_ignores_the_sentinel() {
        let l = Lakitu::new(&ring());
        assert!(!l.should_rescue([0., -400. - 100., 0.]));
        assert!(!l.should_rescue([0., -1000., 0.]));
        assert!(l.should_rescue([0., -400. - FALL_MARGIN - 1., 0.]));
        assert!(!Lakitu::new(&[]).should_rescue([0., -1e9, 0.]));
    }

    #[test]
    fn fades_out_teleports_once_and_fades_back_in() {
        let mut l = Lakitu::new(&ring());
        l.start();
        let (mut peak, mut teleports, mut ticks) = (0f32, 0, 0);
        let mut last = 0f32;
        while l.active() {
            if l.tick() {
                teleports += 1;
                assert!((l.alpha() - 1.).abs() < 1e-6, "black at the teleport: {}", l.alpha());
            }
            ticks += 1;
            peak = peak.max(l.alpha());
            if ticks == FADE_OUT {
                last = l.alpha();
            }
        }
        assert_eq!(teleports, 1);
        assert_eq!(ticks, FADE_OUT + HOLD + FADE_IN);
        assert_eq!(peak, 1.);
        assert!(last >= 0.99);
        assert_eq!(l.alpha(), 0.);
        assert!(!l.holding());
    }
}
