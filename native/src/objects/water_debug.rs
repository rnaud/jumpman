//! Headless hooks for checking the swimming set by eye (`actions/water.rs`).
//!
//! * `JUMPMAN_HOLD=a@2-6,b@4,stick=0:64@3-5` holds A (press edge at the start) / presses B / holds the stick
//!   (`x:y`, -64..64) between app-clock seconds. Combine with `JUMPMAN_TELEPORT`.
//! * `JUMPMAN_METAL=1` gives Mario a metal cap for an hour.
//! * `JUMPMAN_WATER_EVENT=hurt@4,shock@6,health=300@2,idle@0.5` hurts Mario from the front, shocks him (water only), sets his health, or
//!   puts him into `ACT_WATER_IDLE` where he stands (a teleport below the surface from a dry action plunges to `waterLevel - 100`).
use crate::game::runner::GameSim;
use crate::input::{self, PlayerInput};
use bevy::prelude::*;
use jumpman_sim::state::{A_BUTTON, B_BUTTON};

pub struct WaterDebugPlugin;

impl Plugin for WaterDebugPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(v) = std::env::var("JUMPMAN_HOLD") {
            app.insert_resource(Holds(parse_holds(&v))).add_systems(PreUpdate, hold.after(input::read_input));
        }
        if std::env::var("JUMPMAN_METAL").is_ok() {
            app.add_systems(PostStartup, |mut sim: ResMut<GameSim>| sim.mario.grant_metal_cap(30.0 * 3600.0));
        }
        if let Ok(v) = std::env::var("JUMPMAN_WATER_EVENT") {
            app.insert_resource(Events(parse_events(&v))).add_systems(Update, event);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hold {
    A,
    B,
    Stick(f32, f32),
}

#[derive(Resource)]
struct Holds(Vec<(Hold, f32, f32)>);

#[derive(Resource)]
struct Events(Vec<(String, f64, f32)>);

/// `a@2-6`, `b@4`, `stick=0:64@3-5` (a missing end means one frame).
pub fn parse_holds(s: &str) -> Vec<(Hold, f32, f32)> {
    s.split(',')
        .filter_map(|t| {
            let (what, span) = t.split_once('@')?;
            let (from, to) = match span.split_once('-') {
                Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
                None => {
                    let a: f32 = span.trim().parse().ok()?;
                    (a, a + 0.04)
                }
            };
            let what = what.trim().to_ascii_lowercase();
            let hold = if what == "a" {
                Hold::A
            } else if what == "b" {
                Hold::B
            } else if let Some(xy) = what.strip_prefix("stick=") {
                let (x, y) = xy.split_once(':')?;
                Hold::Stick(x.parse().ok()?, y.parse().ok()?)
            } else {
                return None;
            };
            Some((hold, from, to))
        })
        .collect()
}

/// `hurt@4`, `shock@6`, `health=300@2` -> (name, value, time).
pub fn parse_events(s: &str) -> Vec<(String, f64, f32)> {
    s.split(',')
        .filter_map(|t| {
            let (what, time) = t.split_once('@')?;
            let (name, value) = what.split_once('=').map_or((what, 0.0), |(n, v)| (n, v.parse().unwrap_or(0.0)));
            Some((name.trim().to_string(), value, time.trim().parse().ok()?))
        })
        .collect()
}

fn hold(holds: Res<Holds>, mut input: ResMut<PlayerInput>, time: Res<Time>) {
    let now = time.elapsed_secs();
    for (what, from, to) in &holds.0 {
        if now < *from || now > *to {
            continue;
        }
        match what {
            Hold::A => {
                input.down |= A_BUTTON;
                // The press edge on the first frame of the span only.
                if now - *from < 0.034 {
                    input.tap(true, false);
                }
            }
            Hold::B => {
                input.down |= B_BUTTON;
                if now - *from < 0.034 {
                    input.tap(false, true);
                }
            }
            Hold::Stick(x, y) => {
                input.stick = Vec2::new(*x, *y);
                input.stick_mag = Vec2::new(*x, *y).length().min(64.0);
            }
        }
    }
}

fn event(mut events: ResMut<Events>, mut sim: ResMut<GameSim>, time: Res<Time>) {
    let now = time.elapsed_secs();
    let mut due = vec![];
    events.0.retain(|e| {
        if now >= e.2 {
            due.push(e.clone());
            false
        } else {
            true
        }
    });
    for (name, value, _) in due {
        let m = &mut sim.mario;
        match name.as_str() {
            "hurt" => {
                let yaw = m.face_angle[1];
                let from = [m.pos[0] + jumpman_sim::math::sins(yaw) * 200.0, m.pos[1], m.pos[2] + jumpman_sim::math::coss(yaw) * 200.0];
                m.take_damage(1.0, Some(from));
            }
            "shock" => {
                jumpman_sim::actions::water::shock_in_water(m);
            }
            "health" => m.health = value,
            // A teleport from a dry action below the surface plunges to waterLevel - 100; this starts Mario treading in place.
            "idle" => {
                if let Some(y) = std::env::var("JUMPMAN_TELEPORT").ok().and_then(|v| v.split(',').nth(1).and_then(|p| p.trim().parse::<f64>().ok())) {
                    m.pos[1] = y;
                }
                m.vel = [0.0; 3];
                m.forward_vel = 0.0;
                m.set_action(jumpman_sim::state::ACT_WATER_IDLE, 0.0);
            }
            other => warn!("JUMPMAN_WATER_EVENT: unknown event {other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hold_script() {
        let h = parse_holds("a@2-6, b@4,stick=0:64@3-5,x@1");
        assert_eq!(h, vec![(Hold::A, 2.0, 6.0), (Hold::B, 4.0, 4.04), (Hold::Stick(0.0, 64.0), 3.0, 5.0)]);
        assert_eq!(parse_events("hurt@4,health=300@2"), vec![("hurt".into(), 0.0, 4.0), ("health".into(), 300.0, 2.0)]);
    }
}
