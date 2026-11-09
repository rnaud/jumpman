//! Object sounds derived from behavior state changes.
//!
//! The simplified behaviors (Bob-omb, Boo, Bully, Whomp, King Bob-omb...) and the faithful Bowser port do not
//! call `cur_obj_play_sound_*` themselves. [`Pre::of`] snapshots an object before its update and [`emit`] compares
//! it with the object after the update to produce the sounds the decomp behavior plays at the same moments
//! (state edges, animation frames, per-step timers), as `ObjectEvent::Sound` / `ObjectEvent::SoundLoop` with
//! the real `SOUND_*` name (`assets/audio/music/sounds.json` keys, `SOUND_` prefix removed).
//!
//! Behaviors that already emit their own events (koopa, penguin, racing penguin, snowman, water bomb, the
//! behaviors/g1 set) are not listed here. A behavior file can still call `ObjectManager::sound` / `sound_loop`
//! directly; this hook only adds what is missing.
use crate::object::*;
use crate::object_manager::{ObjectEvent, ObjectManager};

/// State captured before one behavior update.
#[derive(Clone, Copy, Debug)]
pub struct Pre {
    active: bool,
    forward_vel: f64,
    interact: u32,
    action: i32,
    flash: f64,
    anim_frame: f64,
}

impl Pre {
    pub fn of(o: &SimObject) -> Self {
        Self {
            active: o.active,
            forward_vel: o.forward_vel,
            interact: o.interact_status,
            action: o.action,
            flash: o.var("flash"),
            anim_frame: o.var("animFrame"),
        }
    }
}

/// `sBowserSoundStates` (bowser.inc.c): (animation id, frame 1, frame 2, sound) for the entries that play a sound.
/// A negative frame is skipped like in `exec_anim_sound_state`.
const BOWSER_ANIM_SOUNDS: [(i32, i32, i32, &str); 14] = [
    (8, 0, -1, "OBJ_BOWSER_WALK"),
    (9, 0, -1, "OBJ2_BOWSER_ROAR"),
    (10, 0, -1, "OBJ2_BOWSER_ROAR"),
    (13, 20, 40, "OBJ_BOWSER_WALK"),
    (14, 20, -1, "OBJ_BOWSER_WALK"),
    (15, 20, 40, "OBJ_BOWSER_WALK"),
    (16, 0, -1, "OBJ_BOWSER_TAIL_PICKUP"),
    (17, 0, -1, "OBJ_BOWSER_DEFEATED"),
    (18, 8, -1, "OBJ_BOWSER_WALK"),
    (19, 8, 17, "OBJ_BOWSER_WALK"),
    (20, 8, -10, "OBJ_BOWSER_WALK"),
    (22, 5, -1, "OBJ_FLAME_BLOWN"),
    (25, 0, -1, "OBJ_BOWSER_TAIL_PICKUP"),
    (26, 0, -1, "OBJ2_BOWSER_ROAR"),
];

/// `bhvBowser` actions of the faithful port (behaviors/bowser_sm64.rs).
const BOWSER_ACT_TELEPORT: i32 = 16;
const BOWSER_ACT_BREATH_FIRE: i32 = 15;
const BOWSER_ANIM_BREATH: i32 = 6;

fn moving_step(o: &SimObject, period: f64) -> bool {
    o.forward_vel > 0.5 && o.grounded && o.held_state == HELD_FREE && o.timer % period == 0.0
}

/// Push the sounds for `id` (just updated) given its `pre` state.
pub fn emit(objects: &mut ObjectManager, id: ObjId, pre: Pre) {
    use ObjectBehaviorKind as K;
    let o = &objects.objects[id.0 as usize];
    let pos = o.pos;
    let key = o.id;
    let died = pre.active && !o.active;
    let touched = pre.interact & INT_STATUS_INTERACTED != 0 && o.interact_status & INT_STATUS_INTERACTED == 0;
    let hit = o.var("flash") > pre.flash && pre.flash <= 0.0;
    let started = pre.forward_vel == 0.0 && o.forward_vel > 0.0;
    let mut out: Vec<ObjectEvent> = Vec::new();
    let mut play = |name: &'static str| out.push(ObjectEvent::Sound { pos, name });
    match o.behavior {
        K::Bobomb => {
            if moving_step(o, 14.0) {
                play("OBJ_BOBOMB_WALK");
            }
        }
        K::Boo | K::BigBoo | K::BooInCastle => {
            if started {
                play("OBJ_BOO_LAUGH_LONG");
            }
            if touched {
                play("OBJ_BOO_LAUGH_SHORT");
            }
            if died {
                play(if o.behavior == K::BigBoo { "OBJ_THWOMP" } else { "OBJ_DYING_ENEMY1" });
            }
        }
        K::Bully => {
            if touched {
                play("OBJ2_BULLY_ATTACKED");
            }
            if moving_step(o, 20.0) {
                play("OBJ_BULLY_WALKING");
            }
        }
        K::BigBully | K::BigChillBully => {
            if touched || hit {
                play("OBJ2_LARGE_BULLY_ATTACKED");
            }
            if moving_step(o, 26.0) {
                play("OBJ_BULLY_WALK");
            }
            if died {
                play("OBJ_BULLY_EXPLODE_2");
            }
        }
        K::Whomp => {
            if hit {
                play("OBJ2_WHOMP_SOUND_SHORT");
            }
            if moving_step(o, 36.0) {
                play("OBJ_POUNDING1");
            }
            if died {
                play("OBJ_KING_WHOMP_DEATH");
            }
        }
        K::WhompKing => {
            if hit {
                play("OBJ2_WHOMP_SOUND_SHORT");
            }
            if moving_step(o, 40.0) {
                play("OBJ_WHOMP");
            }
            if died {
                play("OBJ_KING_WHOMP_DEATH");
            }
        }
        K::ChainChomp => {
            if o.forward_vel > 0.5 && o.timer % 24.0 == 0.0 {
                play("GENERAL_CHAIN_CHOMP1");
            }
            if started {
                play("GENERAL_CHAIN_CHOMP2");
            }
        }
        K::KingBobomb => {
            if o.action != pre.action {
                match o.action {
                    1 => play("OBJ_KING_BOBOMB_JUMP"),
                    0 if pre.action == 1 => {
                        play("OBJ_KING_BOBOMB");
                        play("OBJ2_KING_BOBOMB_DAMAGE");
                    }
                    _ => {}
                }
            }
            if moving_step(o, 24.0) {
                play("OBJ_POUNDING1_HIGHPRIO");
            }
            if died {
                play("OBJ_KING_WHOMP_DEATH");
            }
        }
        K::Klepto => {
            if started || (o.distance_to_mario < 2500.0 && o.timer % 90.0 == 0.0) {
                play("OBJ_KLEPTO1");
            }
            if o.timer % 30.0 == 0.0 && o.distance_to_mario < 1500.0 {
                play("GENERAL_SWISH_WATER");
            }
        }
        K::Pokey => {
            if died {
                play("OBJ_POKEY_DEATH");
            }
        }
        K::MrBlizzard => {
            if started {
                play("OBJ_MR_BLIZZARD_ALERT");
            }
            if died {
                play("OBJ_ENEMY_DEFEAT_SHRINK");
            }
        }
        K::PurpleSwitch => {
            if o.action != pre.action && o.action != 0 {
                play("GENERAL2_PURPLE_SWITCH");
            }
        }
        K::OpenableGrill | K::GrillDoor => {
            if o.action != pre.action && o.action != 0 {
                play("GENERAL_CAGE_OPEN");
            }
        }
        K::OneUp => {
            if started {
                play("GENERAL2_1UP_APPEAR");
            }
        }
        K::SmBowser => {
            let anim = o.var("soundState") as i32;
            let frame = o.var("animFrame");
            if frame != pre.anim_frame {
                let f = frame as i32;
                for (a, f1, f2, name) in BOWSER_ANIM_SOUNDS {
                    if a == anim && ((f1 >= 0 && f == f1) || (f2 >= 0 && f == f2)) {
                        play(name);
                    }
                }
            }
            if o.action != pre.action && o.action == BOWSER_ACT_TELEPORT {
                play("OBJ2_BOWSER_TELEPORT");
            }
            if o.action != pre.action && o.action == BOWSER_ACT_BREATH_FIRE {
                play("OBJ_BOWSER_INHALING");
            }
        }
        K::Bowser => {
            if moving_step(o, 30.0) {
                play("OBJ_BOWSER_WALK");
            }
            if died {
                play("GENERAL2_BOWSER_EXPLODE");
            }
        }
        K::BowserMine => {
            if died {
                play("GENERAL_BOWSER_BOMB_EXPLOSION");
            }
        }
        _ => {}
    }
    // Sounds the decomp re-requests every tick so the engine keeps them going.
    match o.behavior {
        K::Bobomb if o.active && o.var("fuseLit") != 0.0 && o.action != 3 => {
            out.push(ObjectEvent::SoundLoop { pos, name: "AIR_BOBOMB_LIT_FUSE", key });
        }
        K::SmBowser if o.var("soundState") as i32 == BOWSER_ANIM_BREATH && (46..85).contains(&(o.var("animFrame") as i32)) => {
            out.push(ObjectEvent::SoundLoop { pos, name: "AIR_BOWSER_SPIT_FIRE", key });
        }
        _ => {}
    }
    objects.events.extend(out);
}
