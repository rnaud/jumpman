//! Link's side of Epona: mounting, riding, dismounting (`Player_Action_8084CC98` and friends).
use super::combat::TICKS_PER_FRAME;
use super::epona::{Anim, RideInput, Rider};
use super::moves::{LinkEvent, LinkExt, Mv, PoseHint, ARG_RIDE};
use crate::mario_runtime::MarioRuntime;
use crate::state::*;

impl LinkExt {
    /// Called every tick from `LinkExt::run`. Steps Epona and, when Link is on her (or climbing on / off), takes
    /// the whole frame over (`Some(true)`); `None` leaves the frame to the normal controller.
    pub(super) fn run_epona(&mut self, m: &mut MarioRuntime, pressed: u32) -> Option<bool> {
        if !self.epona.present {
            return None;
        }
        let c = m.controller.clone();
        let input = RideInput { stick_x: c.stick_x, stick_y: c.stick_y, a_pressed: pressed & A_BUTTON != 0, b_pressed: pressed & B_BUTTON != 0 };
        let was = self.epona.rider;
        let link_pos = m.pos;
        let world = m.world.clone();
        self.epona.step(&world, link_pos, &input);
        for e in self.epona.take_events() {
            self.events.push(LinkEvent::Epona(e));
        }
        match self.epona.rider {
            Rider::None => {
                if let Rider::Dismounting { side, .. } = was {
                    // Off: Link stands beside her.
                    m.pos = self.epona.dismount_point(&world, side);
                    m.vel = [0.0; 3];
                    m.set_action(ACT_IDLE, 0.0);
                    return Some(true);
                }
                // On foot: A next to her climbs up.
                let ground = matches!(m.action, ACT_IDLE | ACT_WALKING) && m.action & ACT_FLAG_AIR == 0;
                if self.age == super::items::Age::Adult && self.mv == Mv::Free && ground && pressed & A_BUTTON != 0 && m.held_obj.is_none() && m.riding_shell.is_none() {
                    if let Some(side) = self.epona.mount_side(m.pos) {
                        self.epona.begin_mount(side);
                        for e in self.epona.take_events() {
                            self.events.push(LinkEvent::Epona(e));
                        }
                        self.attach(m);
                        self.ride_pose();
                        return Some(true);
                    }
                }
                None
            }
            _ => {
                self.attach(m);
                self.ride_pose();
                Some(true)
            }
        }
    }

    /// Link's body follows the saddle (the app places the model on the real saddle bone).
    fn attach(&mut self, m: &mut MarioRuntime) {
        m.pos = self.epona.rider_point();
        m.vel = [0.0; 3];
        m.forward_vel = 0.0;
        m.face_angle[1] = self.epona.yaw;
        if m.action != ACT_IDLE || m.action_arg != ARG_RIDE {
            m.set_action(ACT_IDLE, ARG_RIDE);
        }
    }

    /// The `uma_*` clip Link plays (`D_80854944` / `D_80854968` / `D_8085498C`, mount / dismount animations).
    fn ride_pose(&mut self) {
        let e = &self.epona;
        let hint = |clip: &'static str, frame: f64, looping: bool| PoseHint { clip, frame: frame * TICKS_PER_FRAME, looping };
        self.pose = Some(match e.rider {
            Rider::Mounting { side, t } => hint(if side < 0 { "uma_mountLeft" } else { "uma_mountRight" }, t, false),
            Rider::Dismounting { side, t } => hint(if side < 0 { "uma_dismountLeft" } else { "uma_dismountRight" }, t, false),
            _ => {
                let whip = e.boost;
                match e.anim {
                    Anim::Idle | Anim::Whinny => {
                        let (n, f) = e.rider_idle;
                        hint(["uma_wait3", "uma_wait1", "uma_wait2"][n as usize], f, false)
                    }
                    Anim::Stopping => hint(if whip { "uma_stopMuti" } else { "uma_stop" }, e.frame, false),
                    Anim::Rearing => hint("uma_stand", e.frame, false),
                    Anim::Walk => hint(if whip { "uma_walkMuti" } else { "uma_walk" }, e.frame, false),
                    Anim::Trot => hint(if whip { "uma_slowrunMuti" } else { "uma_slowrun" }, e.frame, false),
                    Anim::Gallop => hint(if whip { "uma_fastrunMuti" } else { "uma_fastrun" }, e.frame, false),
                    Anim::LowJump => hint("uma_jump100", e.frame, false),
                    Anim::HighJump => hint("uma_jump200", e.frame, false),
                }
            }
        });
    }
}
