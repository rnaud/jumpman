//! Super Smash Bros. moving stage parts as surface objects: a yakumono (collision group) whose layer-1 DObj runs an
//! AObjEvent32 animation in the game (`mpCollisionPlayYakumonoAnim`, `src/mp/mpcollision.c`: the group's lines move with
//! the DObj's translation). The exporter (`scripts/export-ssb64-stages.py`) samples that translation once per 30 Hz tick
//! into `track` rows `[tick, x, y, z]` (one period of the loop, SM64 units); this behavior replays it, and the framework
//! carries a rider along (`apply_platform_displacement` with `oVel` = this tick's step).
use super::*;

pub fn make(spec: &SolidSpec) -> Option<Box<dyn Behavior>> {
    match spec.behavior.as_str() {
        "ssb64Yakumono" => Some(Box::new(Yakumono { track: spec.track.clone() })),
        _ => None,
    }
}

pub struct Yakumono {
    track: Vec<[f64; 4]>,
}

impl Yakumono {
    fn at(&self, tick: i64) -> Option<Vec3> {
        if self.track.is_empty() {
            return None;
        }
        let r = self.track[tick.rem_euclid(self.track.len() as i64) as usize];
        Some([r[1], r[2], r[3]])
    }
}

impl Behavior for Yakumono {
    fn init(&mut self, o: &mut Obj) {
        if let Some(p) = self.at(0) {
            o.pos = p;
        }
    }

    fn update(&mut self, o: &mut Obj, _cx: &Cx) {
        // `oTimer` counts the ticks since the load; row `timer + 1` is where the part is at the end of this tick.
        if let Some(p) = self.at(o.timer + 1) {
            o.vel = [p[0] - o.pos[0], p[1] - o.pos[1], p[2] - o.pos[2]];
            o.pos = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_the_track_in_a_loop() {
        let spec = SolidSpec { behavior: "ssb64Yakumono".into(), track: vec![[0.0, 0.0, 0.0, 0.0], [1.0, 10.0, 5.0, 0.0], [2.0, 20.0, 0.0, 0.0]], ..Default::default() };
        let mut b = make(&spec).unwrap();
        let mut o = Obj::new(&spec);
        b.init(&mut o);
        let m = MarioRuntime::with_default_death_height(std::sync::Arc::new(crate::collision::CollisionWorld::default()), crate::mario_runtime::SpawnPoint { pos: [0.0; 3], face_yaw: 0.0, action: None });
        let cx = Cx { m: &m, ttc_speed: 0, riding: false, index: 0, floor_switch_ticking: false };
        let mut xs = vec![];
        for _ in 0..4 {
            o.begin_frame();
            b.update(&mut o, &cx);
            o.end_frame();
            xs.push((o.pos[0], o.vel[0]));
        }
        assert_eq!(xs, vec![(10.0, 10.0), (20.0, 10.0), (0.0, -20.0), (10.0, 10.0)]);
        assert!(make(&SolidSpec { behavior: "bhvOther".into(), ..Default::default() }).is_none());
    }
}
