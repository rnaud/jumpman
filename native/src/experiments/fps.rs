//! Test hook: `JUMPMAN_FPS=1` prints the average frame time every two seconds.
use bevy::prelude::*;

pub fn log_fps(time: Res<Time>, mut acc: Local<(f32, u32)>) {
    if std::env::var_os("JUMPMAN_FPS").is_none() {
        return;
    }
    acc.0 += time.delta_secs();
    acc.1 += 1;
    if acc.0 >= 2.0 {
        println!("fps {:.1} ({:.1} ms/frame)", acc.1 as f32 / acc.0, 1000.0 * acc.0 / acc.1 as f32);
        *acc = (0.0, 0);
    }
}
