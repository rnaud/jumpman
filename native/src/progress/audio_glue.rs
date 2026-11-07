//! Sounds that `Game.ts` triggers from its progression glue: forwards [`ProgressCue`] names (and the
//! star-select music switch) to the audio layer as [`SfxCue`].
use super::ProgressCue;
use crate::audio::{course_music, MusicEvent, MusicTrack, SfxCue, SignCue};
use crate::game::runner::GameSim;
use bevy::prelude::*;

pub struct ProgressAudioPlugin;

impl Plugin for ProgressAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, forward_cues);
    }
}

/// Maps a progression cue name to the sound it stands for.
pub fn map_cue(name: &str, base: &MusicTrack) -> Option<SfxCue> {
    Some(match name {
        "star" => SfxCue::Star,
        "star_sound" => SfxCue::StarSound,
        "teleport" => SfxCue::Teleport,
        "cap_switch" => SfxCue::CapSwitch,
        "death_warp" => SfxCue::DeathWarp,
        "menu_move" => SfxCue::MoveMenu,
        "select_mission" => SfxCue::SelectMission,
        "sign_open" => SfxCue::SignMessage(SignCue::Open),
        "sign_page" => SfxCue::SignMessage(SignCue::Page),
        "sign_close" => SfxCue::SignMessage(SignCue::Close),
        "star_select_music" => SfxCue::StartMusic(MusicTrack { name: "star select".into(), sequence: 0x0d, preset: 0, ..base.clone() }),
        "stop_music" => SfxCue::StopMusic,
        "clear" => SfxCue::Music(MusicEvent::Clear),
        "exit_star" => SfxCue::Music(MusicEvent::ExitStar),
        "exit_key" => SfxCue::Music(MusicEvent::ExitKey),
        "star_spawn" => SfxCue::Music(MusicEvent::Spawn),
        _ => return None,
    })
}

fn forward_cues(mut cues: EventReader<ProgressCue>, mut out: EventWriter<SfxCue>, sim: Res<GameSim>) {
    for cue in cues.read() {
        let base = course_music(&sim.course.request.id, sim.course.request.area);
        match map_cue(cue.0, &base) {
            Some(c) => {
                out.write(c);
            }
            None => debug!("unmapped progress cue {}", cue.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flow_cue_has_a_sound() {
        let base = course_music("bob", 1);
        for name in ["star", "star_sound", "teleport", "cap_switch", "death_warp", "menu_move", "select_mission", "sign_open", "sign_page", "sign_close", "star_select_music", "stop_music", "clear", "exit_star", "exit_key", "star_spawn"] {
            assert!(map_cue(name, &base).is_some(), "{name}");
        }
    }
}
