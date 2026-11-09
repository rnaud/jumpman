//! The current room of an OoT scene. The game keeps `play->roomCtx.curRoom.num` through its room-transition actors; this port
//! loads every room of a scene at once, so the room is derived from where Link stands: each room's mesh has an axis-aligned
//! bounding box (`roomBounds` of the course file, exported with the mesh) and the current room is the smallest box that
//! contains him, kept while he stays inside it (hysteresis, so overlapping boxes do not flicker).
use crate::math::Vec3;

#[derive(Clone, Debug)]
pub struct RoomBox {
    pub room: i32,
    pub min: Vec3,
    pub max: Vec3,
}

impl RoomBox {
    fn contains(&self, p: &Vec3, slack: f64) -> bool {
        (0..3).all(|k| p[k] >= self.min[k] - slack && p[k] <= self.max[k] + slack)
    }

    fn volume(&self) -> f64 {
        (0..3).map(|k| (self.max[k] - self.min[k]).max(1.0)).product()
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoomTracker {
    pub boxes: Vec<RoomBox>,
    pub current: Option<i32>,
}

impl RoomTracker {
    pub fn new(boxes: Vec<RoomBox>) -> Self {
        Self { boxes, current: None }
    }

    /// Update from Link's position (`y` is his feet; a room box is tested with 150 units of headroom above and below).
    pub fn update(&mut self, p: Vec3) -> Option<i32> {
        let p = [p[0], p[1] + 50.0, p[2]];
        if let Some(cur) = self.current {
            if self.boxes.iter().filter(|b| b.room == cur).any(|b| b.contains(&p, 0.0)) {
                return self.current;
            }
        }
        let best = self.boxes.iter().filter(|b| b.contains(&p, 150.0)).min_by(|a, b| a.volume().total_cmp(&b.volume()));
        if let Some(b) = best {
            self.current = Some(b.room);
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> RoomTracker {
        RoomTracker::new(vec![
            RoomBox { room: 0, min: [-1000.0, -100.0, -1000.0], max: [1000.0, 800.0, 1000.0] },
            RoomBox { room: 1, min: [500.0, -100.0, -300.0], max: [3000.0, 500.0, 300.0] },
        ])
    }

    #[test]
    fn the_room_is_the_smallest_box_around_link_and_sticks_while_he_stays_inside() {
        let mut t = tracker();
        assert_eq!(t.update([0.0, 0.0, 0.0]), Some(0));
        // the overlap belongs to the smaller room only when Link has left the first one's box
        assert_eq!(t.update([700.0, 0.0, 0.0]), Some(0), "still inside room 0: keeps it");
        assert_eq!(t.update([2000.0, 0.0, 0.0]), Some(1));
        assert_eq!(t.update([700.0, 0.0, 0.0]), Some(1), "inside room 1 too: keeps it");
        assert_eq!(t.update([-500.0, 0.0, 0.0]), Some(0));
        // outside every box: the last room stays
        assert_eq!(t.update([9000.0, 0.0, 0.0]), Some(0));
    }
}
