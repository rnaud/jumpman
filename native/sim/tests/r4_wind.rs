//! Snowman's Land strong wind: particles push Mario into ACT_GETTING_BLOWN, away from the Snowman.
mod r2a_common;
use jumpman_sim::mario_r2a::ACT_GETTING_BLOWN;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn the_snowmans_wind_blows_mario_backwards() {
    let mut m = runtime(ground(0.0, 9000.0), vec![spec(ObjectBehaviorKind::SlSnowmanWind, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 0.0, 700.0);
    m.invinc_timer = 0.0;
    let id = find(&m, ObjectBehaviorKind::SlSnowmanWind)[0];
    // Skip the introduction dialog: straight to the blowing sub-action.
    m.objects[id].action_state = 2.0;
    let mut blown = false;
    let mut max_z = 700.0f64;
    for _ in 0..120 {
        tick(&mut m);
        blown |= m.action == ACT_GETTING_BLOWN;
        max_z = max_z.max(m.pos[2]);
    }
    assert!(blown, "the wind caught Mario");
    assert!(max_z > 800.0, "pushed away from the Snowman: {max_z}");
}
