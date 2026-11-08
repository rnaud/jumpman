//! Epona's model against the numbers of the decomp: size, saddle and rider placement.
use super::epona_view::*;
use crate::characters::assets::{self, EponaAsset};
use crate::characters::link_pose::pose_limbs;
use bevy::math::{Mat4, Vec3};

fn assets() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// Skinned bounding box of a pose in model units.
fn bounds(asset: &EponaAsset, clip: &str, frame: f64) -> (Vec3, Vec3, Vec<Mat4>) {
    let bones = pose_limbs(&asset.limbs, &asset.clips[clip], frame, 1.0, false, true, true);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for b in &asset.batches {
        for v in &b.vertices {
            let mut p = Vec3::ZERO;
            for k in 0..v[6] as usize {
                let o = 7 + 5 * k;
                p += bones[v[o] as usize].transform_point3(Vec3::new(v[o + 1] as f32, v[o + 2] as f32, v[o + 3] as f32)) * (v[o + 4] as f32 / 100.0);
            }
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo, hi, bones)
}

#[test]
fn idle_epona_stands_on_the_ground_with_a_horse_sized_body() {
    let asset = assets::load_epona(&assets()).unwrap();
    let (lo, hi, bones) = bounds(&asset, "idle", 0.0);
    eprintln!("epona idle bounds {lo:?} .. {hi:?}; saddle point {:?}", bones[SADDLE_LIMB].transform_point3(RIDER_OFFSET));
    assert!(lo.y > -300.0 && lo.y < 400.0, "hooves at {}", lo.y);
    assert!(hi.y > 8000.0 && hi.y < 12000.0, "head at {}", hi.y);
    assert!((hi.z - lo.z) > 9000.0, "length {}", hi.z - lo.z);
    // The saddle point sits just above the seat, between the shoulders and the croup.
    let seat = bones[SADDLE_LIMB].transform_point3(RIDER_OFFSET);
    assert!(seat.y > 5500.0 && seat.y < 7500.0 && seat.z.abs() < 1500.0, "{seat:?}");
}
