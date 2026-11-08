//! Engine-free translation layer between the GameController shim's plain events and Bevy's gamepad
//! model: element tables, trigger/stick cleanup and player-slot assignment.
use bevy::input::gamepad::{GamepadAxis, GamepadButton};

/// Button elements as numbered by `gc_shim.m` (`sample`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Element {
    A,
    B,
    X,
    Y,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    Menu,
    Options,
    Home,
    LeftThumb,
    RightThumb,
}

impl Element {
    pub fn from_id(id: i32) -> Option<Self> {
        use Element::*;
        Some(match id {
            0 => A,
            1 => B,
            2 => X,
            3 => Y,
            4 => LeftShoulder,
            5 => RightShoulder,
            6 => LeftTrigger,
            7 => RightTrigger,
            8 => DpadUp,
            9 => DpadDown,
            10 => DpadLeft,
            11 => DpadRight,
            12 => Menu,
            13 => Options,
            14 => Home,
            15 => LeftThumb,
            16 => RightThumb,
            _ => return None,
        })
    }

    /// Positional layout (A is the bottom face button on every pad, whatever its label). `Home` is
    /// deliberately unmapped: the Guide/PS button must never reach the game (it would act as Start/exit).
    pub fn bevy(self) -> Option<GamepadButton> {
        use Element::*;
        Some(match self {
            A => GamepadButton::South,
            B => GamepadButton::East,
            X => GamepadButton::West,
            Y => GamepadButton::North,
            LeftShoulder => GamepadButton::LeftTrigger,
            RightShoulder => GamepadButton::RightTrigger,
            LeftTrigger => GamepadButton::LeftTrigger2,
            RightTrigger => GamepadButton::RightTrigger2,
            DpadUp => GamepadButton::DPadUp,
            DpadDown => GamepadButton::DPadDown,
            DpadLeft => GamepadButton::DPadLeft,
            DpadRight => GamepadButton::DPadRight,
            Menu => GamepadButton::Start,
            Options => GamepadButton::Select,
            LeftThumb => GamepadButton::LeftThumb,
            RightThumb => GamepadButton::RightThumb,
            Home => return None,
        })
    }

    pub fn is_trigger(self) -> bool {
        matches!(self, Element::LeftTrigger | Element::RightTrigger)
    }
}

/// Axis ids: 0 left X, 1 left Y, 2 right X, 3 right Y (GameController reports Y up, as Bevy does).
pub fn axis_from_id(id: i32) -> Option<GamepadAxis> {
    Some(match id {
        0 => GamepadAxis::LeftStickX,
        1 => GamepadAxis::LeftStickY,
        2 => GamepadAxis::RightStickX,
        3 => GamepadAxis::RightStickY,
        _ => return None,
    })
}

/// Stick noise floor; the game applies its own radial deadzone on top (`input::gamepad_stick`).
pub const STICK_SNAP: f32 = 0.04;
/// Analog triggers below this read as released (they rest around 0.01-0.03 on worn pads).
pub const TRIGGER_SNAP: f32 = 0.05;

pub fn clean_axis(v: f32) -> f32 {
    if v.abs() < STICK_SNAP {
        0.0
    } else {
        v.clamp(-1.0, 1.0)
    }
}

pub fn clean_trigger(v: f32) -> f32 {
    if v < TRIGGER_SNAP {
        0.0
    } else {
        v.min(1.0)
    }
}

/// Value forwarded to Bevy for `e`: triggers stay analog, everything else is digital 0/1 or kept as is.
pub fn clean_button(e: Element, v: f32) -> f32 {
    if e.is_trigger() {
        clean_trigger(v)
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// Maximum simultaneous controllers (Sunstone Shindig / Gem Clash cap out at 4 players; spare slots are free).
pub const MAX_PADS: usize = 8;

/// Player slot per connected controller: the lowest free slot, so connect order is player order and a
/// reconnecting pad takes the seat that freed up.
#[derive(Default, Debug)]
pub struct Slots {
    uids: [Option<u32>; MAX_PADS],
}

impl Slots {
    pub fn connect(&mut self, uid: u32) -> Option<usize> {
        if let Some(s) = self.slot(uid) {
            return Some(s);
        }
        let free = self.uids.iter().position(Option::is_none)?;
        self.uids[free] = Some(uid);
        Some(free)
    }

    pub fn disconnect(&mut self, uid: u32) -> Option<usize> {
        let s = self.slot(uid)?;
        self.uids[s] = None;
        Some(s)
    }

    pub fn slot(&self, uid: u32) -> Option<usize> {
        self.uids.iter().position(|u| *u == Some(uid))
    }

    pub fn count(&self) -> usize {
        self.uids.iter().flatten().count()
    }
}

/// One shim event after decoding.
#[derive(Clone, Debug, PartialEq)]
pub enum PadEvent {
    Connect { uid: u32, name: String },
    Disconnect { uid: u32 },
    Button { uid: u32, element: Element, value: f32 },
    Axis { uid: u32, axis: GamepadAxis, value: f32 },
}

/// Decode a raw shim event (`kind`: 0 connect, 1 disconnect, 2 button, 3 axis).
pub fn decode(kind: i32, uid: i32, id: i32, value: f32, name: impl FnOnce() -> String) -> Option<PadEvent> {
    let uid = u32::try_from(uid).ok()?;
    match kind {
        0 => Some(PadEvent::Connect { uid, name: name() }),
        1 => Some(PadEvent::Disconnect { uid }),
        2 => Some(PadEvent::Button { uid, element: Element::from_id(id)?, value }),
        3 => Some(PadEvent::Axis { uid, axis: axis_from_id(id)?, value }),
        _ => None,
    }
}

/// Scripted source for `JUMPMAN_FAKE_PAD=<count>`: pads connect at 0.5 s; every pad walks forward, taps
/// A every two seconds, pulls the right trigger briefly, and (pad 1 only) presses Menu at 20 s.
pub fn fake_events(count: u32, t: f32, prev_t: f32) -> Vec<PadEvent> {
    let mut out = Vec::new();
    let crossed = |at: f32| prev_t < at && t >= at;
    for uid in 1..=count {
        if crossed(0.5) {
            out.push(PadEvent::Connect { uid, name: format!("Fake Pad {uid}") });
            out.push(PadEvent::Axis { uid, axis: GamepadAxis::LeftStickY, value: 0.9 });
        }
        // Tap A every two seconds from 1 s, held for 0.2 s.
        let cycle = |x: f32| ((x - 1.0) / 2.0).floor();
        if cycle(t) > cycle(prev_t) {
            out.push(PadEvent::Button { uid, element: Element::A, value: 1.0 });
        } else if t > 1.0 && (t - 1.0) % 2.0 >= 0.2 && (prev_t - 1.0) % 2.0 < 0.2 && prev_t >= 1.0 {
            out.push(PadEvent::Button { uid, element: Element::A, value: 0.0 });
        }
        if uid == 1 && crossed(20.0) {
            out.push(PadEvent::Button { uid, element: Element::Menu, value: 1.0 });
        }
        if uid == 1 && crossed(20.1) {
            out.push(PadEvent::Button { uid, element: Element::Menu, value: 0.0 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_table_is_positional_and_drops_home() {
        let all: Vec<_> = (0..17).map(|i| Element::from_id(i).unwrap()).collect();
        assert!(Element::from_id(17).is_none() && Element::from_id(-1).is_none());
        assert_eq!(all[0].bevy(), Some(GamepadButton::South));
        assert_eq!(all[1].bevy(), Some(GamepadButton::East));
        assert_eq!(all[2].bevy(), Some(GamepadButton::West));
        assert_eq!(all[3].bevy(), Some(GamepadButton::North));
        assert_eq!(Element::Menu.bevy(), Some(GamepadButton::Start));
        assert_eq!(Element::Options.bevy(), Some(GamepadButton::Select));
        assert_eq!(Element::DpadLeft.bevy(), Some(GamepadButton::DPadLeft));
        assert_eq!(Element::LeftShoulder.bevy(), Some(GamepadButton::LeftTrigger));
        assert_eq!(Element::RightTrigger.bevy(), Some(GamepadButton::RightTrigger2));
        assert_eq!(Element::Home.bevy(), None, "the Guide button must not reach the game");
        // Every other element maps to a distinct button.
        let mut seen: Vec<_> = all.iter().filter_map(|e| e.bevy()).collect();
        let n = seen.len();
        seen.sort_by_key(|b| format!("{b:?}"));
        seen.dedup();
        assert_eq!(seen.len(), n);
        assert_eq!(n, 16);
    }

    #[test]
    fn axes_map_and_clean() {
        assert_eq!(axis_from_id(0), Some(GamepadAxis::LeftStickX));
        assert_eq!(axis_from_id(3), Some(GamepadAxis::RightStickY));
        assert_eq!(axis_from_id(4), None);
        assert_eq!(clean_axis(0.03), 0.0);
        assert_eq!(clean_axis(-0.03), 0.0);
        assert_eq!(clean_axis(-0.5), -0.5);
        assert_eq!(clean_axis(1.3), 1.0);
    }

    #[test]
    fn triggers_stay_analog_above_the_snap() {
        assert_eq!(clean_button(Element::LeftTrigger, 0.02), 0.0);
        assert_eq!(clean_button(Element::RightTrigger, 0.4), 0.4);
        assert_eq!(clean_button(Element::RightTrigger, 1.2), 1.0);
        assert_eq!(clean_button(Element::A, 1.0), 1.0);
        assert_eq!(clean_button(Element::DpadUp, -0.5), 0.0);
    }

    #[test]
    fn slots_follow_connect_order_and_reuse_free_seats() {
        let mut s = Slots::default();
        assert_eq!(s.connect(10), Some(0));
        assert_eq!(s.connect(11), Some(1));
        assert_eq!(s.connect(12), Some(2));
        assert_eq!(s.connect(11), Some(1), "idempotent");
        assert_eq!(s.disconnect(10), Some(0));
        assert_eq!(s.disconnect(10), None);
        assert_eq!(s.connect(13), Some(0), "lowest free seat");
        assert_eq!(s.count(), 3);
        for uid in 100..105 {
            assert!(s.connect(uid).is_some());
        }
        assert_eq!(s.connect(999), None, "full");
    }

    #[test]
    fn decode_rejects_garbage() {
        assert_eq!(decode(0, 3, 0, 0.0, || "Pad".into()), Some(PadEvent::Connect { uid: 3, name: "Pad".into() }));
        assert_eq!(decode(1, 3, 0, 0.0, String::new), Some(PadEvent::Disconnect { uid: 3 }));
        assert_eq!(decode(2, 3, 12, 1.0, String::new), Some(PadEvent::Button { uid: 3, element: Element::Menu, value: 1.0 }));
        assert_eq!(decode(3, 3, 1, 0.5, String::new), Some(PadEvent::Axis { uid: 3, axis: GamepadAxis::LeftStickY, value: 0.5 }));
        assert_eq!(decode(2, 3, 99, 1.0, String::new), None);
        assert_eq!(decode(9, 3, 0, 1.0, String::new), None);
        assert_eq!(decode(2, -1, 0, 1.0, String::new), None);
    }

    #[test]
    fn fake_pad_script_connects_each_pad_once() {
        let mut connects = 0;
        let mut a_down = 0;
        let mut a_up = 0;
        let mut t = 0.0;
        while t < 6.0 {
            let next = t + 1.0 / 60.0;
            for e in fake_events(2, next, t) {
                match e {
                    PadEvent::Connect { .. } => connects += 1,
                    PadEvent::Button { element: Element::A, value, .. } if value > 0.5 => a_down += 1,
                    PadEvent::Button { element: Element::A, .. } => a_up += 1,
                    _ => {}
                }
            }
            t = next;
        }
        assert_eq!(connects, 2);
        assert!(a_down >= 6 && a_up >= 6, "{a_down} {a_up}");
    }
}
