// StandardMaterial plus one world-space clip plane (THREE.Plane semantics: keep dot(n, p) + w >= 0).
import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
};

struct ClipPlane {
    plane: vec4<f32>,
}

@group(3) @binding(100) var<uniform> clip: ClipPlane;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    if (dot(in.world_position.xyz, clip.plane.xyz) + clip.plane.w < 0.0) {
        discard;
    }
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
