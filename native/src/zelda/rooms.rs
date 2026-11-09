//! The current room of the OoT scene Link is in (`jumpman_sim::oot_rooms`), from the `roomBounds` of the course file, and the
//! "room clear" flag (`Flags_GetClear`): no live enemy placed in that room is left.
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use bevy::prelude::*;
use jumpman_sim::oot_rooms::{RoomBox, RoomTracker};

#[derive(Resource, Default)]
pub struct SceneRooms {
    pub tracker: RoomTracker,
    key: String,
    /// The room Link is in now (`None` before the first update or in a scene without room data).
    pub current: Option<i32>,
    /// Every enemy placed in the current room is dead.
    pub cleared: bool,
}

/// `roomBounds` of a course file.
pub fn load_boxes(root: &std::path::Path, id: &str, area: i32) -> Vec<RoomBox> {
    let path = root.join("courses").join(if area > 1 { format!("{id}.{area}.json") } else { format!("{id}.json") });
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    let f = |a: &serde_json::Value| -> Option<[f64; 3]> { Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?]) };
    v["roomBounds"].as_array().map(|list| list.iter().filter_map(|b| Some(RoomBox { room: b["room"].as_i64()? as i32, min: f(&b["min"])?, max: f(&b["max"])? })).collect()).unwrap_or_default()
}

pub fn track_rooms(mut rooms: ResMut<SceneRooms>, sim: Res<GameSim>, root: Res<AssetRoot>, en: Option<Res<crate::zelda_enemies::OotEnemies>>) {
    let req = &sim.course.request;
    let key = format!("{}:{}", req.id, req.area);
    if rooms.key != key {
        rooms.key = key;
        rooms.tracker = RoomTracker::new(load_boxes(&root.0, &req.id, req.area));
    }
    rooms.current = rooms.tracker.update(sim.mario.pos);
    let Some(room) = rooms.current else { return };
    rooms.cleared = en.as_ref().is_none_or(|e| !e.field.enemies.iter().any(|x| x.room == room && x.alive()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forest_temples_rooms_have_boxes_and_enemies_know_their_room() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let boxes = load_boxes(&root, "forest_temple", 1);
        if boxes.is_empty() {
            return eprintln!("no OoT data imported; skipped");
        }
        assert!(boxes.len() >= 10, "{} rooms", boxes.len());
        assert!(boxes.iter().all(|b| b.room >= 0 && (0..3).all(|k| b.max[k] > b.min[k])));
        let p = crate::zelda_enemies::load_placements(&root, "forest_temple", 1).unwrap();
        assert!(p.enemies.iter().filter(|e| e.room.contains("_room_")).count() > 10, "enemies carry their room");
        // the spawn lies in some room
        let spawn: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("courses/forest_temple.json")).unwrap()).unwrap();
        let s = &spawn["spawn"]["pos"];
        let mut t = RoomTracker::new(boxes);
        assert!(t.update([s[0].as_f64().unwrap(), s[1].as_f64().unwrap(), s[2].as_f64().unwrap()]).is_some());
    }
}
