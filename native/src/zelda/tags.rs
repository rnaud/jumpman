//! Ocarina tags (`jumpman_sim::oot_tags`): a song played at a royal crest or a Triforce emblem sets its switch flag.
use crate::game::runner::GameSim;
use crate::link::prop_world::PropWorld;
use crate::link::WorldFx;
use crate::render::AssetRoot;
use bevy::prelude::*;
use jumpman_sim::link::ocarina::Song;
use jumpman_sim::oot_tags::{self, Tags};

#[derive(Resource, Default)]
pub struct OcarinaTags {
    pub tags: Tags,
    key: String,
}

/// `ocarinaSong` of the actor for a song Link knows.
pub fn song_index(s: Song) -> Option<u8> {
    Some(match s {
        Song::Sarias => oot_tags::SARIA,
        Song::Eponas => oot_tags::EPONA,
        Song::Lullaby => oot_tags::LULLABY,
        Song::Suns => oot_tags::SUN,
        Song::Time => oot_tags::TIME,
        Song::Storms => oot_tags::STORMS,
        _ => return None,
    })
}

pub fn step_tags(mut tags: ResMut<OcarinaTags>, mut fx: ResMut<WorldFx>, sim: Res<GameSim>, root: Res<AssetRoot>, props: Option<ResMut<PropWorld>>) {
    let req = sim.course.request.clone();
    let key = format!("{}:{}", req.id, req.area);
    let mut props = props;
    if tags.key != key {
        let list: Vec<_> = crate::zelda_enemies::load_placements(&root.0, &req.id, req.area)
            .map(|p| p.mechanisms)
            .unwrap_or_default()
            .iter()
            .filter(|m| m.actor == "ACTOR_EN_OKARINA_TAG")
            .map(|m| (m.pos, m.param & 0xffff, m.rot[2] as i32))
            .collect();
        let flag = |f: i32| props.as_ref().is_some_and(|p| p.props.flag(f));
        tags.tags = Tags::new(&list, &flag);
        tags.key = key;
        fx.songs.clear();
        return;
    }
    for song in std::mem::take(&mut fx.songs) {
        let Some(idx) = song_index(song) else { continue };
        let flag = |f: i32| props.as_ref().is_some_and(|p| p.props.flag(f));
        for f in tags.tags.on_song(idx, sim.mario.pos, &flag) {
            if let Some(p) = props.as_mut() {
                p.props.set_switch(f, true);
            }
            crate::audio::oot_sfx::play("SY_CORRECT_CHIME");
        }
    }
}
