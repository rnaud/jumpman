// Distance fog for the ray-traced tier. Solari shades the deferred G-buffer itself and knows no fog, so the haze that
// the other tiers get from `DistanceFog` is laid over the frame afterwards: a screen-filling blended triangle that reads
// the depth prepass, rebuilds each pixel's distance from the eye and blends the fog colour with the same linear ramp.
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::prepass::utils::prepass_depth;

// rgb: fog colour (linear). a: unused.
@group(3) @binding(0) var<uniform> fog_color: vec4<f32>;
// x: ramp start, y: ramp end (world units), z: distance beyond which nothing is fogged (the sky dome), w: strength.
@group(3) @binding(1) var<uniform> fog_params: vec4<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that covers the whole target.
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 1.0, 1.0);
}

@fragment
fn fragment(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let depth = prepass_depth(frag, 0u);
    // Reverse-Z: 0 is the far plane, i.e. nothing was drawn there.
    if depth <= 0.0 || fog_params.w <= 0.0 {
        return vec4<f32>(0.0);
    }
    let uv = (frag.xy - view.viewport.xy) / view.viewport.zw;
    let ndc = vec3<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth);
    let world = view.world_from_clip * vec4<f32>(ndc, 1.0);
    let dist = distance(world.xyz / world.w, view.world_position);
    if dist > fog_params.z {
        return vec4<f32>(0.0);
    }
    let amount = clamp((dist - fog_params.x) / max(fog_params.y - fog_params.x, 0.0001), 0.0, 1.0) * fog_params.w;
    return vec4<f32>(fog_color.rgb, amount);
}
