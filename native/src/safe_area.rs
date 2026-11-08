//! Safe-area insets (notch / Dynamic Island / home indicator) in logical px. Only iOS reports
//! non-zero values; they come from `jm_safe_insets` in `ios/Sources/main.m`.
use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeArea {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    /// Writes `[left, right, bottom, top]` in points.
    fn jm_safe_insets(out: *mut f32);
}

/// Holding a `NonSend` resource pins the polling system to the main thread, which UIKit requires.
pub struct MainThreadPoll {
    last: f32,
}

/// Marker resource whose `NonSend` use pins a system to the main thread.
pub struct MainThread;

pub struct SafeAreaPlugin;

impl Plugin for SafeAreaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SafeArea>().insert_non_send(MainThreadPoll { last: -1.0 }).insert_non_send(MainThread).add_systems(PreUpdate, poll);
    }
}

#[cfg_attr(not(target_os = "ios"), allow(unused_variables, unused_mut))]
fn poll(mut main: NonSendMut<MainThreadPoll>, time: Res<Time<Real>>, mut area: ResMut<SafeArea>) {
    let now = time.elapsed_secs();
    if main.last >= 0.0 && now - main.last < 0.25 {
        return;
    }
    main.last = now;
    #[cfg(target_os = "ios")]
    {
        let mut v = [0f32; 4];
        // SAFETY: `jm_safe_insets` writes exactly four floats; called on the main thread (NonSend).
        unsafe { jm_safe_insets(v.as_mut_ptr()) };
        let next = SafeArea { left: v[0], right: v[1], bottom: v[2], top: v[3] };
        if *area != next {
            *area = next;
        }
    }
    // Desktop test hook: `JUMPMAN_SAFE_AREA=left,right,bottom,top` (logical px), e.g. `59,59,21,0` for a landscape iPhone.
    #[cfg(not(target_os = "ios"))]
    if let Some(v) = std::env::var("JUMPMAN_SAFE_AREA").ok().map(|v| v.split(',').filter_map(|p| p.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 4) {
        let next = SafeArea { left: v[0], right: v[1], bottom: v[2], top: v[3] };
        if *area != next {
            *area = next;
        }
    }
}
