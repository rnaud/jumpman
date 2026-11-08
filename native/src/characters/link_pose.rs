//! Engine-light port of the animation logic of `OotLinkView.ts`: clip choice from the sim state,
//! frame interpolation and the limb hierarchy -> bone matrices. Vertices are stored in the local
//! space of their limb, so a vertex's world position is `bone_world[limb] * v`.
use super::assets::LinkAsset;
use bevy::math::{EulerRot, Mat4, Quat, Vec3};
use jumpman_sim::characters::{link_finishing, link_guarding, link_rolling, LINK_BACKFLIP, LINK_HOP_LEFT, LINK_HOP_RIGHT};
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;

pub const ANGLE_TO_RAD: f64 = std::f64::consts::PI * 2.0 / 65536.0;

/// Which clip to play and how.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkChoice {
    pub clip: &'static str,
    /// Absolute frame (otherwise the clip loops with its own phase).
    pub frame: Option<f64>,
    /// Playback speed multiplier on 30 fps.
    pub speed: f64,
}

/// `OotLinkView.update`'s clip selection. `clip_len` gives the frame count of a clip.
pub fn choose_clip(m: &MarioRuntime, clip_len: impl Fn(&str) -> usize) -> LinkChoice {
    let mut clip = "idle";
    let mut speed = 30.0;
    let mut frame = None;
    let a = m.action;
    // Poses of the faithful controller (swings, item use, aiming, ocarina...): OoT clips run at
    // 20 Hz, the sim at 30 Hz.
    if let Some(h) = m.link.pose.filter(|_| m.link.enabled) {
        let len = clip_len(h.clip);
        if len > 1 {
            let f = h.frame * 20.0 / 30.0;
            let f = if h.looping { f % len as f64 } else { f.min(len as f64 - 1.0) };
            return LinkChoice { clip: h.clip, frame: Some(f), speed };
        }
    }
    if link_rolling(m) {
        clip = "roll";
        frame = Some(m.action_timer * 1.25);
    } else if a == ACT_PUNCHING {
        let finishing = link_finishing(m);
        let strike = if finishing { "finish" } else { "slash" };
        let wind_down = clip_len(strike) as f64;
        clip = if m.action_timer < wind_down { strike } else if finishing { "finishRecover" } else { "recover" };
        frame = Some(if m.action_timer < wind_down { m.action_timer } else { m.action_timer - wind_down });
    } else if link_guarding(m) {
        clip = "guard";
    } else if a == ACT_CRAWLING {
        clip = "crawl";
        frame = Some(m.anim_frame.min(clip_len("crawl") as f64 - 1.0));
    } else if a == ACT_CLIMBING_POLE {
        clip = "climb";
    } else if a == ACT_LEDGE_GRAB {
        clip = "hang";
    } else if a & ACT_FLAG_SWIMMING != 0 {
        clip = "swim";
    } else if a == ACT_FREEFALL && [LINK_BACKFLIP, LINK_HOP_LEFT, LINK_HOP_RIGHT].contains(&m.action_arg) {
        clip = if m.action_arg == LINK_BACKFLIP {
            "backflip"
        } else if m.action_arg == LINK_HOP_LEFT {
            "leftHop"
        } else {
            "rightHop"
        };
        frame = Some((clip_len(clip) as f64 - 1.0).min(m.action_timer));
    } else if a & ACT_FLAG_AIR != 0 {
        // `normal_run_jump` is a one-shot (13 frames) and the legacy loop restarted it in mid-air:
        // tie its frame to the arc instead (take-off at +38 -> apex -> descent) and hold the last
        // frame; the long fall is the looping `normal_fall_wait`. A Z-targeted forward jump uses the
        // front flip (`fighter_front_jump`).
        let flip = m.link.enabled && m.link.hop_forward && clip_len("frontFlip") > 1;
        let name = if flip { "frontFlip" } else { "jump" };
        let len = clip_len(name) as f64;
        let slow_walk_off = m.vel[1] <= 0.0 && m.forward_vel.abs() < 12.0 && m.action_timer < 3.0;
        if m.vel[1] > -14.0 && !slow_walk_off {
            let frac = ((38.0 - m.vel[1]) / 52.0).clamp(0.0, 1.0);
            clip = name;
            frame = Some(frac * (len - 1.0));
        } else {
            clip = "fall";
        }
    } else if m.forward_vel.abs() > 1.0 {
        clip = if m.forward_vel.abs() > 16.0 { "run" } else { "walk" };
        if m.controller.focus_yaw.is_some() {
            clip = "strafe";
        }
        speed *= 0.35f64.max(m.forward_vel.abs() / if clip == "run" { 32.0 } else { 16.0 });
    }
    if clip == "idle" && m.controller.focus_yaw.is_some() {
        clip = "focus";
    }
    LinkChoice { clip, frame, speed }
}

/// Whether the drawn-sword meshes (vs the sheathed ones) are shown.
pub fn sword_drawn(m: &MarioRuntime) -> bool {
    m.action == ACT_PUNCHING || link_guarding(m) || m.controller.focus_yaw.is_some()
}

#[derive(Default)]
pub struct LinkAnimator {
    pub phase: f64,
    pub active_clip: String,
    was_air: bool,
    landing_left: f64,
}

impl LinkAnimator {
    /// Advance by `dt` seconds and return the 21 bone world matrices (model space, unscaled).
    pub fn update(&mut self, asset: &LinkAsset, young: bool, m: &MarioRuntime, dt: f64, smooth: bool) -> (LinkChoice, Vec<Mat4>) {
        let mut choice = choose_clip(m, |name| asset.clips.get(name).map_or(1, Vec::len));
        // Landing: a short squash clip after touching down (`normal_landing`), when the asset has it.
        let airborne = m.action & ACT_FLAG_AIR != 0;
        if self.was_air && !airborne && m.link.enabled {
            self.landing_left = 0.35;
        }
        self.was_air = airborne;
        if self.landing_left > 0.0 {
            self.landing_left -= dt;
            let calm = matches!(choice.clip, "idle" | "focus" | "walk");
            if calm && asset.clips.contains_key("landing") {
                let len = asset.clips["landing"].len() as f64;
                choice = LinkChoice { clip: "landing", frame: Some(((0.35 - self.landing_left) * 20.0).min(len - 1.0)), speed: choice.speed };
            }
        }
        if !asset.clips.contains_key(choice.clip) {
            choice = LinkChoice { clip: "idle", frame: None, speed: choice.speed };
        }
        let frames = &asset.clips[choice.clip];
        self.phase = match choice.frame {
            Some(f) => f,
            None if choice.clip == self.active_clip => (self.phase + dt * choice.speed) % frames.len() as f64,
            None => 0.0,
        };
        self.active_clip = choice.clip.to_string();
        let bones = pose_bones(asset, frames, self.phase, young, choice.clip == "crawl", smooth);
        (choice, bones)
    }
}

/// Interpolated frame -> world matrix per limb.
pub fn pose_bones(asset: &LinkAsset, frames: &[Vec<f64>], phase: f64, young: bool, crawl: bool, smooth: bool) -> Vec<Mat4> {
    let index = phase.min(frames.len() as f64 - 1.0);
    let a = &frames[index.floor() as usize];
    let b = &frames[(index.floor() as usize + 1) % frames.len()];
    let t = if smooth { index % 1.0 } else { 0.0 };
    let n = asset.limbs.len();
    let mut local = Vec::with_capacity(n);
    for j in 0..n {
        let mut pos = Vec3::new(asset.limbs[j].pos[0] as f32, asset.limbs[j].pos[1] as f32, asset.limbs[j].pos[2] as f32);
        if j == 0 {
            let lerp = |i: usize| a[i] + (b[i] - a[i]) * t;
            pos = Vec3::new(lerp(0) as f32, lerp(1) as f32, lerp(2) as f32);
            if young {
                pos *= 0.64;
            }
            if crawl {
                pos.x = 0.0;
                pos.z = 0.0;
            }
        }
        let angle = |axis: usize| {
            let i = 3 * j + 3 + axis;
            let delta = (b[i] - a[i] + 98304.0).rem_euclid(65536.0) - 32768.0;
            ((a[i] + delta * t) * ANGLE_TO_RAD) as f32
        };
        let rot = Quat::from_euler(EulerRot::ZYX, angle(2), angle(1), angle(0));
        local.push(Mat4::from_rotation_translation(rot, pos));
    }
    // Walk child/sibling links from the root.
    let mut world = vec![Mat4::IDENTITY; n];
    let mut stack = vec![(0usize, Mat4::IDENTITY)];
    while let Some((i, parent)) = stack.pop() {
        world[i] = parent * local[i];
        let mut c = asset.limbs[i].child;
        while c != 255 && c < n {
            stack.push((c, world[i]));
            c = asset.limbs[c].sibling;
        }
    }
    world
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::assets::load_link;

    fn assets() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn mario() -> MarioRuntime {
        crate::characters::test_support::mario()
    }

    #[test]
    fn idle_pose_stands_on_the_ground() {
        let asset = load_link(&assets(), "link").unwrap();
        let (choice, bones) = LinkAnimator::default().update(&asset, false, &mario(), 0.0, true);
        assert_eq!(choice.clip, "idle");
        // Every skinned vertex ends up inside Link's body bounds (units are N64 model units).
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for batch in asset.batches.iter().filter(|b| b.mode == "base") {
            for v in &batch.vertices {
                let p = bones[v[8] as usize].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
                min_y = min_y.min(p.y);
                max_y = max_y.max(p.y);
            }
        }
        assert!(min_y > -400.0 && min_y < 400.0, "feet at {min_y}");
        assert!(max_y > 3000.0 && max_y < 6500.0, "head at {max_y}");
    }

    #[test]
    fn clip_selection() {
        let len = |_: &str| 20usize;
        let mut m = mario();
        m.forward_vel = 20.0;
        m.set_action(ACT_WALKING, 0.0);
        assert_eq!(choose_clip(&m, len).clip, "run");
        m.forward_vel = 8.0;
        let walk = choose_clip(&m, len);
        assert_eq!(walk.clip, "walk");
        assert!((walk.speed - 15.0).abs() < 1e-9);
        m.controller.focus_yaw = Some(0.0);
        assert_eq!(choose_clip(&m, len).clip, "strafe");
        m.forward_vel = 0.0;
        assert_eq!(choose_clip(&m, len).clip, "focus");
        m.set_action(ACT_FREEFALL, LINK_BACKFLIP);
        m.action_timer = 99.0;
        let c = choose_clip(&m, len);
        assert_eq!((c.clip, c.frame), ("backflip", Some(19.0)));
    }

    #[test]
    fn slash_then_recover() {
        let len = |n: &str| if n == "slash" { 6 } else { 10 };
        let mut m = mario();
        m.set_action(ACT_PUNCHING, 0x53 as f64);
        m.action_timer = 2.0;
        assert_eq!(choose_clip(&m, len).clip, "slash");
        m.action_timer = 8.0;
        let c = choose_clip(&m, len);
        assert_eq!((c.clip, c.frame), ("recover", Some(2.0)));
        assert!(sword_drawn(&m));
    }
}
