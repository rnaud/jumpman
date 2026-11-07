use bevy::prelude::*;
use serde::Deserialize;
use std::f32::consts::{PI, TAU};

#[derive(Deserialize)]
pub struct Prop {
    pub model: String,
    pub pos: [f32; 3],
    pub yaw: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spawn {
    pub pos: [f32; 3],
    pub face_yaw: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub model: String,
    pub objects: Vec<Prop>,
    pub spawn: Spawn,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[i32; 5]>,
    pub water_boxes: Vec<[f32; 6]>,
}

struct Triangle {
    points: [Vec3; 3],
    normal: Vec3,
}

impl Triangle {
    fn contains(&self, point: Vec3) -> bool {
        let [a, b, c] = self.points;
        [(a, b), (b, c), (c, a)]
            .into_iter()
            .all(|(start, end)| (end - start).cross(point - start).dot(self.normal) >= -0.1)
    }
}

#[derive(Resource)]
pub struct Terrain {
    floors: Vec<Triangle>,
    walls: Vec<Triangle>,
    spawn: Vec3,
    spawn_yaw: i16,
}

impl Terrain {
    pub fn new(course: &Course) -> Self {
        let mut terrain = Self {
            floors: vec![],
            walls: vec![],
            spawn: course.spawn.pos.into(),
            spawn_yaw: course.spawn.face_yaw as i16,
        };
        for &[a, b, c, kind, _] in &course.triangles {
            // Skip intangible and camera-only surfaces.
            if matches!(kind, 0x12 | 0x72) {
                continue;
            }
            let points = [a, b, c].map(|i| Vec3::from(course.vertices[i as usize]));
            let normal = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize_or_zero();
            let tri = Triangle { points, normal };
            if normal.y > 0.01 {
                terrain.floors.push(tri);
            } else if normal != Vec3::ZERO && normal.y >= -0.01 {
                terrain.walls.push(tri);
            }
        }
        terrain
    }

    fn floor(&self, pos: Vec3, top: f32) -> Option<f32> {
        self.floors
            .iter()
            .filter_map(|tri| {
                let p = tri.points[0];
                let n = tri.normal;
                let h = p.y - (n.x * (pos.x - p.x) + n.z * (pos.z - p.z)) / n.y;
                (h <= top && tri.contains(pos.with_y(h))).then_some(h)
            })
            .max_by(f32::total_cmp)
    }

    fn push_walls(&self, pos: &mut Vec3) {
        for tri in &self.walls {
            let probe = *pos + Vec3::Y * 60.;
            let offset = tri.normal.dot(probe - tri.points[0]);
            if offset.abs() <= 50. && tri.contains(probe - offset * tri.normal) {
                *pos += tri.normal.with_y(0.) * (50. - offset);
            }
        }
    }
}

#[derive(Default, Resource)]
pub struct Controls {
    pub stick: Vec2,
    pub jump_pressed: bool,
    pub jump_held: bool,
    pub long_jump: bool,
    pub orbit: f32,
}

#[derive(Resource)]
pub struct Mario {
    pub pos: Vec3,
    pub previous: Vec3,
    pub speed: f32,
    pub yaw: i16,
    pub grounded: bool,
    pub long_jump: bool,
    vy: f32,
}

impl Mario {
    pub fn spawn(terrain: &Terrain) -> Self {
        let mut pos = terrain.spawn;
        pos.y = terrain.floor(pos, pos.y + 78.).unwrap_or(pos.y);
        Self {
            pos,
            previous: pos,
            speed: 0.,
            yaw: terrain.spawn_yaw,
            grounded: true,
            long_jump: false,
            vy: 0.,
        }
    }

    pub fn angle(&self) -> f32 {
        f32::from(self.yaw) * PI / 32768.
    }

    // Source counterparts: updateWalkingSpeed, updateAirWithoutTurn and setMarioActionAirborne.
    pub fn tick(&mut self, input: &Controls, terrain: &Terrain) {
        self.previous = self.pos;
        let moving = input.stick != Vec2::ZERO;
        let target = ((input.stick.x.atan2(input.stick.y) + input.orbit) / TAU * 65536.).round()
            as i32 as i16;
        let delta = i32::from(target.wrapping_sub(self.yaw));
        if self.grounded {
            if moving {
                if self.speed < 1. {
                    self.yaw = target;
                } else {
                    self.yaw = self.yaw.wrapping_add(delta.clamp(-0x800, 0x800) as i16);
                }
                self.speed += if self.speed <= 32. {
                    1.1 - self.speed / 43.
                } else {
                    -1.
                };
            } else {
                self.speed = (self.speed - 2.).max(0.);
            }
            if input.jump_pressed {
                self.long_jump = input.long_jump && self.speed > 8.;
                if self.long_jump {
                    self.vy = 30.;
                    self.speed = (1.5 * self.speed).min(48.);
                } else {
                    self.vy = 42. + 0.25 * self.speed;
                    self.speed *= 0.8;
                }
                self.grounded = false;
            }
        }
        let mut side = 0.;
        if !self.grounded {
            self.speed -= self.speed.clamp(-0.35, 0.35);
            if moving {
                let diff = delta as f32 * PI / 32768.;
                self.speed += 1.5 * diff.cos();
                side = 10. * diff.sin();
            }
            if self.speed > if self.long_jump { 48. } else { 32. } {
                self.speed -= 1.;
            }
            if self.speed < -16. {
                self.speed += 2.;
            }
        }

        let (s, c) = self.angle().sin_cos();
        let step = Vec3::new(
            self.speed * s + side * c,
            self.vy,
            self.speed * c - side * s,
        ) / 4.;
        for _ in 0..4 {
            let old = self.pos;
            self.pos += step.with_y(if self.grounded { 0. } else { step.y });
            terrain.push_walls(&mut self.pos);
            if let Some(h) = terrain.floor(self.pos, old.y + if self.grounded { 78. } else { 0.1 })
            {
                if self.grounded && old.y - h <= 40. || self.vy <= 0. && self.pos.y <= h {
                    self.pos.y = h;
                    self.vy = 0.;
                    self.grounded = true;
                    self.long_jump = false;
                    continue;
                }
            }
            self.grounded = false;
        }
        if !self.grounded {
            if !input.jump_held && !self.long_jump && self.vy > 20. {
                self.vy *= 0.25;
            }
            self.vy = (self.vy - if self.long_jump { 2. } else { 4. }).max(-75.);
        }
        if self.pos.y < -3000. {
            *self = Self::spawn(terrain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_ground() -> Terrain {
        Terrain::new(&Course {
            model: String::new(),
            objects: vec![],
            water_boxes: vec![],
            spawn: Spawn {
                pos: [0.; 3],
                face_yaw: 0,
            },
            vertices: vec![[-5000., 0., -5000.], [0., 0., 5000.], [5000., 0., -5000.]],
            triangles: vec![[0, 1, 2, 0, 0]],
        })
    }

    #[test]
    fn running_long_jump_travels_farther_and_lands() {
        let terrain = flat_ground();
        let range = |long_jump| {
            let mut m = Mario::spawn(&terrain);
            let mut input = Controls {
                stick: Vec2::Y,
                jump_held: true,
                long_jump,
                ..default()
            };
            for _ in 0..40 {
                m.tick(&input, &terrain);
            }
            let start = m.pos;
            input.jump_pressed = true;
            m.tick(&input, &terrain);
            assert!(!m.grounded && m.pos.y > start.y);
            input.jump_pressed = false;
            for _ in 0..100 {
                m.tick(&input, &terrain);
                if m.grounded {
                    assert_eq!(m.pos.y, 0.);
                    return m.pos.z - start.z;
                }
            }
            panic!("missed landing");
        };
        assert!(range(true) > range(false));
    }

    #[test]
    fn releasing_jump_early_reduces_height() {
        let terrain = flat_ground();
        let peak = |held| {
            let mut mario = Mario::spawn(&terrain);
            let mut input = Controls {
                jump_held: held,
                jump_pressed: true,
                ..default()
            };
            let mut max: f32 = 0.;
            for _ in 0..40 {
                mario.tick(&input, &terrain);
                input.jump_pressed = false;
                max = max.max(mario.pos.y);
            }
            assert!(mario.grounded);
            max
        };
        assert!(peak(true) > peak(false) + 100.);
    }
}
