//! `bhvToadMessage`: the castle Toads (`src/game/mario_misc.c`: `bhv_toad_message_init`,
//! `bhv_toad_message_loop`, `toad_message_*`). Three of them hold a Power Star behind a star count
//! (12 / 25 / 35 stars, dialogs 082 / 076 / 083, `bhv_spawn_star_no_level_exit(STAR_INDEX_ACT_n)`);
//! the others just talk. `bhv_toad_message_init` reads the save file: the presentation layer
//! publishes `stars` (total star count) and `collected` (the SAVE_FLAG_COLLECTED_TOAD_STAR_n flag of
//! this Toad's star) as vars on the object (`progress::castle_npcs`); until then the Toad waits.
use super::npc::*;
use super::*;
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::hitbox;
use crate::sm64_dialogs::dialog;

pub const TOAD_STAR_1_DIALOG: u32 = 82;
pub const TOAD_STAR_2_DIALOG: u32 = 76;
pub const TOAD_STAR_3_DIALOG: u32 = 83;
pub const TOAD_STAR_1_DIALOG_AFTER: u32 = 154;
pub const TOAD_STAR_2_DIALOG_AFTER: u32 = 155;
pub const TOAD_STAR_3_DIALOG_AFTER: u32 = 156;
pub const TOAD_STAR_1_REQUIREMENT: f64 = 12.0;
pub const TOAD_STAR_2_REQUIREMENT: f64 = 25.0;
pub const TOAD_STAR_3_REQUIREMENT: f64 = 35.0;

pub const TOAD_MESSAGE_FADED: i32 = 0;
pub const TOAD_MESSAGE_OPAQUE: i32 = 1;
pub const TOAD_MESSAGE_OPACIFYING: i32 = 2;
pub const TOAD_MESSAGE_FADING: i32 = 3;
pub const TOAD_MESSAGE_TALKING: i32 = 4;

/// `toad_seg6_anims_0600FB58` order (the glb lists them in this order).
pub const ANIM_NAMES: [&str; 8] = [
    "toad_seg6_anim_06008F7C", "toad_seg6_anim_06009310", "toad_seg6_anim_060099F0", "toad_seg6_anim_0600A0D0",
    "toad_seg6_anim_0600B66C", "toad_seg6_anim_0600CE78", "toad_seg6_anim_0600E414", "toad_seg6_anim_0600FB40",
];
/// ANIMATE(6): `toad_seg6_anim_0600E414` (`assets/animations.json`: flags 1, 100 frames).
const IDLE: AnimSpec = spec(0, 0, 100, true);

/// `DIALOG_xxx` the Toad starts with, from its `BPARAM1` (`oBhvParams >> 24`).
pub fn star_index(dialog_id: u32) -> Option<i32> {
    match dialog_id {
        TOAD_STAR_1_DIALOG => Some(1),
        TOAD_STAR_2_DIALOG => Some(2),
        TOAD_STAR_3_DIALOG => Some(3),
        _ => None,
    }
}

pub fn init(o: &mut SimObject) {
    // SET_HITBOX(80, 100), INTERACT_TEXT (the type bit is irrelevant here: `interact` is a hook).
    o.hitbox = hitbox(0, 0.0, 0.0, 0.0, 0.0, 80.0, 100.0, 80.0, 100.0);
    o.interact_subtype = 0;
    o.visible = false;
    o.set_var("opacity", 81.0);
    anim::init(o, 6, IDLE);
}

/// `bhv_toad_message_init`: stars and flags are the save file's (published by the app).
fn gate(o: &mut SimObject) -> bool {
    if o.has_var("gated") {
        return !o.has_var("deleted");
    }
    if !o.has_var("stars") {
        return false; // waiting for the save file
    }
    o.set_var("gated", 1.0);
    let stars = o.var("stars");
    let collected = o.var("collected") != 0.0;
    let mut dialog_id = o.param as u32;
    let enough = match dialog_id {
        TOAD_STAR_1_DIALOG => {
            if collected { dialog_id = TOAD_STAR_1_DIALOG_AFTER; }
            stars >= TOAD_STAR_1_REQUIREMENT
        }
        TOAD_STAR_2_DIALOG => {
            if collected { dialog_id = TOAD_STAR_2_DIALOG_AFTER; }
            stars >= TOAD_STAR_2_REQUIREMENT
        }
        TOAD_STAR_3_DIALOG => {
            if collected { dialog_id = TOAD_STAR_3_DIALOG_AFTER; }
            stars >= TOAD_STAR_3_REQUIREMENT
        }
        _ => true,
    };
    if enough {
        o.set_var("dialogId", dialog_id as f64);
        o.set_var("recentlyTalked", 0.0);
        o.action = TOAD_MESSAGE_FADED;
        o.set_var("opacity", 81.0);
        true
    } else {
        o.set_var("deleted", 1.0);
        o.deactivate(-1.0);
        false
    }
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    if !gate(&mut m.objects[id]) {
        return;
    }
    // GRAPH_RENDER_ACTIVE: the Toad is only updated while drawn (always, in range of the camera).
    m.objects[id].interact_subtype = 0;
    let dist = m.objects[id].distance_to_mario;
    match m.objects[id].action {
        TOAD_MESSAGE_FADED => {
            // toad_message_faded
            let o = &mut m.objects[id];
            if dist > 700.0 { o.set_var("recentlyTalked", 0.0); }
            if o.var("recentlyTalked") == 0.0 && dist < 600.0 { o.action = TOAD_MESSAGE_OPACIFYING; }
        }
        TOAD_MESSAGE_OPAQUE => {
            // toad_message_opaque
            let o = &mut m.objects[id];
            if dist > 700.0 {
                o.action = TOAD_MESSAGE_FADING;
            } else if o.var("recentlyTalked") == 0.0 {
                o.interact_subtype = INT_SUBTYPE_NPC;
                if o.interact_status & INT_STATUS_INTERACTED != 0 {
                    o.interact_status = 0;
                    o.action = TOAD_MESSAGE_TALKING;
                    o.set_var("dialogState", 0.0);
                    let pos = o.pos;
                    m.objects.sound(pos, "TOADS_JINGLE"); // play_toads_jingle
                }
            }
        }
        TOAD_MESSAGE_OPACIFYING => {
            // toad_message_opacifying
            let o = &mut m.objects[id];
            let a = o.var("opacity") + 6.0;
            o.set_var("opacity", a);
            if a == 255.0 { o.action = TOAD_MESSAGE_OPAQUE; }
        }
        TOAD_MESSAGE_FADING => {
            // toad_message_fading
            let o = &mut m.objects[id];
            let a = o.var("opacity") - 6.0;
            o.set_var("opacity", a);
            if a == 81.0 { o.action = TOAD_MESSAGE_FADED; }
        }
        _ => {
            // toad_message_talking: cur_obj_update_dialog_with_cutscene(MARIO_DIALOG_LOOK_DOWN, DIALOG_FLAG_TURN_TO_MARIO, ...)
            let dialog_id = m.objects[id].var("dialogId") as u32;
            if update_dialog_with_cutscene(m, id, "dialogState", MARIO_DIALOG_LOOK_DOWN, true, dialog_id) {
                let o = &mut m.objects[id];
                o.set_var("recentlyTalked", 1.0);
                o.action = TOAD_MESSAGE_FADING;
                let (after, index) = match dialog_id {
                    TOAD_STAR_1_DIALOG => (Some(TOAD_STAR_1_DIALOG_AFTER), 1),
                    TOAD_STAR_2_DIALOG => (Some(TOAD_STAR_2_DIALOG_AFTER), 2),
                    TOAD_STAR_3_DIALOG => (Some(TOAD_STAR_3_DIALOG_AFTER), 3),
                    _ => (None, 0),
                };
                if let Some(after) = after {
                    o.set_var("dialogId", after as f64);
                    // bhv_spawn_star_no_level_exit(STAR_INDEX_ACT_n): the star pops out above Toad.
                    let pos = [o.pos[0], o.pos[1] + 160.0, o.pos[2]];
                    m.objects.castle_star(pos, index);
                }
            }
        }
    }
    let o = &mut m.objects[id];
    // The glb only needs the material alpha: the renderer reads `opacity` (255 = opaque, 81 = faded).
    o.visible = true;
    anim::advance(o);
    end(o);
}

/// `interact_text` for `INT_SUBTYPE_NPC` Toads (the hook runs while Mario overlaps the hitbox).
pub fn interact(id: ObjId, m: &mut MarioRuntime) -> bool {
    if !m.objects[id].has_var("gated") {
        return false;
    }
    if m.objects[id].interact_subtype & INT_SUBTYPE_NPC != 0 {
        return check_npc_talk(m, id);
    }
    push_mario_out_of_object(m, id, 2.0);
    true
}

/// The dialog pages of a Toad dialog (diagnostics / tests).
pub fn dialog_text(id: u32) -> Option<&'static str> {
    dialog(id).map(|d| d.text)
}
