//! Glue between feature modules' cue events and [`SfxCue`]: progression cues (`ProgressCue`),
//! lift / signpost / Goomba events from the object layer. Ports the `this.sfx.*` calls that
//! `Game.ts` makes next to those features.
use super::{MusicEvent, MusicTrack, PeachVoice, SfxCue, SignCue};
use crate::game::goomba::GoombaEvent;
use crate::game::platform::LiftSound;
use crate::game::runner::GameSim;
use crate::game::signpost::SignSound;
use crate::objects::dialog::SignSoundMsg;
use crate::objects::goombas::GoombaFxMsg;
use crate::objects::platforms::LiftSoundMsg;
use crate::progress::ProgressCue;
use bevy::prelude::*;

pub struct CuesPlugin;

impl Plugin for CuesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, forward_cues.before(super::apply_cues).after(crate::SimSet));
    }
}

/// Map a `ProgressCue` name to the sound(s) it stands for.
pub fn progress_cue(name: &str, sim: &GameSim) -> Vec<SfxCue> {
    let m = &sim.mario;
    match name {
        "teleport" => vec![SfxCue::Teleport],
        "menu_move" => vec![SfxCue::MoveMenu],
        "select_mission" => vec![SfxCue::SelectMission],
        "course_coin" => vec![SfxCue::CourseCoin { count: 0, record: false }],
        "star_spawn" => vec![SfxCue::SpawnedStar { pos: m.pos, appearing: true }],
        "cap_switch" => vec![SfxCue::CapSwitch],
        "death_warp" => vec![SfxCue::DeathWarp],
        "star" => vec![SfxCue::Star],
        "star_sound" => vec![SfxCue::StarSound],
        "sign_open" => vec![SfxCue::SignMessage(SignCue::Open)],
        "sign_page" => vec![SfxCue::SignMessage(SignCue::Page)],
        "sign_close" => vec![SfxCue::SignMessage(SignCue::Close)],
        "clear" => vec![SfxCue::Music(MusicEvent::Clear)],
        "exit_star" => vec![SfxCue::Music(MusicEvent::ExitStar)],
        "exit_key" => vec![SfxCue::Music(MusicEvent::ExitKey)],
        "star_select_music" => {
            let mut track: MusicTrack = super::course_music(&sim.course.request.id, sim.course.request.area);
            track.name = "star select".into();
            track.sequence = 0x0d;
            track.preset = 0;
            vec![SfxCue::StartMusic(track)]
        }
        "peach_mario" => vec![SfxCue::Peach(PeachVoice::Mario)],
        _ => vec![],
    }
}

fn forward_cues(
    sim: Res<GameSim>,
    mut progress: EventReader<ProgressCue>,
    mut lifts: EventReader<LiftSoundMsg>,
    mut signs: EventReader<SignSoundMsg>,
    mut goombas: EventReader<GoombaFxMsg>,
    mut out: EventWriter<SfxCue>,
) {
    for c in progress.read() {
        for cue in progress_cue(c.0, &sim) {
            out.write(cue);
        }
    }
    for l in lifts.read() {
        let cue = match l.sound {
            LiftSound::Moving => "moving",
            LiftSound::Stop => "stop",
            LiftSound::Lower => "lower",
            LiftSound::Upper => "upper",
        };
        out.write(SfxCue::Lift { pos: l.pos, cue });
    }
    for s in signs.read() {
        out.write(SfxCue::SignMessage(match s.0 {
            SignSound::Read => SignCue::Read,
            SignSound::Open => SignCue::Open,
            SignSound::Page => SignCue::Page,
            SignSound::Close => SignCue::Close,
        }));
    }
    for g in goombas.read() {
        match g.0 {
            GoombaEvent::Stomp => {
                out.write(SfxCue::StompGoomba);
            }
            GoombaEvent::Hurt => {
                out.write(SfxCue::Bonk);
            }
            _ => {}
        }
    }
}
