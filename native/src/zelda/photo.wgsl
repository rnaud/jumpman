// Fixed-camera Kokiri rooms: the room mesh writes the depth buffer and its colour is the original
// photograph, sampled in screen space (the camera never moves, so the plate lines up exactly).
#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

@group(2) @binding(0) var photo: texture_2d<f32>;
@group(2) @binding(1) var photo_sampler: sampler;
// x = camera aspect / (4 / 3): the photograph is 4:3 and spans the full height.
@group(2) @binding(2) var<uniform> aspect: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let s = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let uv = vec2<f32>(0.5 + (s.x - 0.5) * aspect.x, s.y);
    return vec4<f32>(textureSampleLevel(photo, photo_sampler, uv, 0.0).rgb, 1.0);
}
