//! Animation clips of a glb, fetched with one load of the file.
//!
//! `AssetServer::load("model.glb#AnimationN")` runs the whole glTF loader once per label while the
//! file is not loaded yet (bevy_asset 0.20 does not share concurrent sub-asset loads, bevy issue
//! 10549). Mario's glb has 209 clips, so asking for each by label read and parsed the 4.8 MB file
//! 209 times on the IO pool at the start of every course: about 12 GB of allocations, a 4 GB RSS
//! peak and a stalled first second.
//!
//! [`clip_handles`] hands out reserved handles straight away, loads the root [`Gltf`] once, and
//! [`fill_clips`] moves the loaded clips under the reserved ids when the file arrives.
use bevy::asset::{AssetHandleProvider, LoadState, StrongHandle};
use bevy::gltf::Gltf;
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use super::AssetRoot;

struct ClipSet {
    /// Held until the clips are moved out; dropping it frees the rest of the root asset.
    root: Option<Handle<Gltf>>,
    clips: Vec<Weak<StrongHandle>>,
}

#[derive(Default)]
struct Inner {
    provider: Option<AssetHandleProvider>,
    sets: HashMap<String, ClipSet>,
}

/// Per-app registry of clip sets, carried by [`AssetRoot`] so every model spawner can reach it.
#[derive(Clone, Default)]
pub struct ClipBank(Arc<Mutex<Inner>>);

impl ClipBank {
    /// Number of sets still waiting for their glb (tests).
    pub fn pending(&self) -> usize {
        self.0.lock().unwrap().sets.values().filter(|s| s.root.is_some()).count()
    }
}

/// Handles of the first `count` clips of `model` (index == glTF animation index). The same live
/// handles are returned to every caller; the glb is read once however many clips it has.
pub fn clip_handles(assets: &AssetServer, root: &AssetRoot, model: &str, count: usize) -> Vec<Handle<AnimationClip>> {
    let mut bank = root.1 .0.lock().unwrap();
    if let Some(set) = bank.sets.get(model) {
        let live: Vec<Handle<AnimationClip>> = set.clips.iter().filter_map(|w| w.upgrade().map(Handle::Strong)).collect();
        if live.len() == set.clips.len() && live.len() >= count {
            return live.into_iter().take(count).collect();
        }
    }
    let Some(provider) = bank.provider.clone() else {
        // No clip store registered (bare test apps): the plain per-label load.
        return (0..count).map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(model.to_owned()))).collect();
    };
    let handles: Vec<Handle<AnimationClip>> = (0..count).map(|_| provider.reserve_handle().typed()).collect();
    let clips = handles
        .iter()
        .map(|h| match h {
            Handle::Strong(arc) => Arc::downgrade(arc),
            Handle::Uuid(..) => Weak::new(),
        })
        .collect();
    let gltf = assets.load_builder().override_unapproved().load(model.to_owned());
    bank.sets.insert(model.to_owned(), ClipSet { root: Some(gltf), clips });
    handles
}

fn register_store(root: Res<AssetRoot>, clips: Res<Assets<AnimationClip>>) {
    root.1 .0.lock().unwrap().provider.get_or_insert_with(|| clips.get_handle_provider());
}

/// Move the clips of every glb that finished loading under the handles given out for it.
fn fill_clips(root: Res<AssetRoot>, assets: Res<AssetServer>, gltfs: Res<Assets<Gltf>>, mut clips: ResMut<Assets<AnimationClip>>) {
    let mut bank = root.1 .0.lock().unwrap();
    if bank.sets.values().all(|s| s.root.is_none()) {
        return;
    }
    bank.sets.retain(|model, set| {
        let Some(handle) = &set.root else {
            // Filled: keep the entry while any spawner still holds its clips.
            return set.clips.iter().any(|w| w.strong_count() > 0);
        };
        if let Some(gltf) = gltfs.get(handle) {
            for (weak, source) in set.clips.iter().zip(&gltf.animations) {
                let Some(arc) = weak.upgrade() else { continue };
                if let Some(clip) = clips.remove(source.id()) {
                    let _ = clips.insert(Handle::<AnimationClip>::Strong(arc).id(), clip);
                }
            }
            if gltf.animations.len() < set.clips.len() {
                warn!("{model}: {} clips wanted, the glb has {}", set.clips.len(), gltf.animations.len());
            }
            set.root = None;
        } else if matches!(assets.load_state(handle), LoadState::Failed(_)) {
            warn!("{model}: clips not loaded");
            return false;
        }
        true
    });
}

pub struct ClipsPlugin;

impl Plugin for ClipsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, register_store.run_if(resource_exists::<AssetRoot>).run_if(resource_exists::<Assets<AnimationClip>>));
        app.add_systems(PreUpdate, fill_clips.run_if(resource_exists::<AssetRoot>).run_if(resource_exists::<Assets<AnimationClip>>).run_if(resource_exists::<Assets<Gltf>>));
    }
}
