//! Mumbo's transformations on screen: while `BanjoExt::form` is not Banjo the form's skinned model (exported to
//! `public/models/bk/enemies/form_*.json` with the same Rare rig format as the enemies, states 1 idle, 2 walk, 3 jump, 4 ow, 5 die,
//! 6 special) replaces Banjo's own model, which `characters` hides for the duration.
use super::*;
use jumpman_sim::banjo_ext::Form;

#[derive(Resource, Default)]
pub struct FormView {
    form: Option<Form>,
    view: Option<View>,
}

/// Which animation state a form shows for what Mario is doing.
pub fn form_state(action: u32, forward_vel: f64, attacking: bool) -> i32 {
    use jumpman_sim::state::ACT_FLAG_AIR;
    if attacking {
        6
    } else if action & ACT_FLAG_AIR != 0 {
        3
    } else if forward_vel.abs() > 1.0 {
        2
    } else {
        1
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_form_view(
    mut commands: Commands,
    sim: Res<GameSim>,
    root: Res<AssetRoot>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut assets: ResMut<EnemyAssets>,
    mut state: ResMut<FormView>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
) {
    let form = if sim.as_banjo { sim.mario.banjo.form } else { Form::Banjo };
    if state.form != Some(form) {
        if let Some(v) = state.view.take() {
            commands.entity(v.root).despawn();
        }
        state.form = Some(form);
        if form != Form::Banjo {
            let rig = assets.get(&root.0, form.model());
            state.view = Some(spawn_view(&mut commands, &mut meshes, &mut materials, &mut images, Kind::Termite, rig));
        }
        return; // the entities exist next frame
    }
    let Some(view) = state.view.as_mut() else { return };
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    if let Ok(mut tf) = transforms.get_mut(view.root) {
        tf.translation = to_bevy(pos);
        tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(yaw) as f32);
        tf.scale = Vec3::splat(S as f32 * SCALE);
    }
    if let Ok(mut v) = visibility.get_mut(view.root) {
        *v = Visibility::Inherited;
    }
    let Some(rig) = view.rig.clone() else { return };
    let m = &sim.mario;
    let attacking = form == Form::Croc && matches!(m.action, jumpman_sim::state::ACT_PUNCHING | jumpman_sim::state::ACT_MOVE_PUNCHING);
    let st = form_state(m.action, m.forward_vel, attacking);
    let Some((clip, secs)) = rig.asset.state_clip(st) else { return };
    let dur = if st == 2 { (0.5 - (m.forward_vel / 30.0).min(1.0) * 0.2).max(0.2) } else { secs };
    let dt = time.delta_secs_f64().min(0.1);
    if view.clip != clip {
        view.from = view.pose.clone();
        view.blend = 0.0;
        view.progress = 0.0;
        view.clip = clip.to_string();
    } else {
        view.progress = (view.progress + dt / dur.max(0.05)) % 1.0;
    }
    view.blend = (view.blend + dt / BLEND).min(1.0);
    let mut target = Pose::new(rig.asset.bones.len());
    rig.evaluate(clip, view.progress, &mut target);
    let b = view.blend as f32;
    for i in 0..view.pose.rot.len() {
        view.pose.rot[i] = view.from.rot[i].slerp(target.rot[i], b);
        view.pose.scale[i] = view.from.scale[i].lerp(target.scale[i], b);
        view.pose.trans[i] = view.from.trans[i].lerp(target.trans[i], b);
    }
    let mats = rig.matrices(&view.pose);
    for ((_, handle), batch) in view.batches.iter().zip(&rig.asset.batches) {
        let Some(mut mesh) = meshes.get_mut(handle) else { continue };
        let positions: Vec<[f32; 3]> = batch
            .vertices
            .iter()
            .map(|v| mats[(v[9].max(0.0) as usize).min(mats.len() - 1)].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array())
            .collect();
        let normals = crate::characters::flat_normals(&positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
}
