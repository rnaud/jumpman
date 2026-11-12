// Camera-centred sky dome: the course gradient plus the `SkyDome` (src/game/experiments/sky.ts)
// time-scrolling fbm cloud layer on a virtual plane above, fading toward the horizon.
import bevy_pbr::render::{forward_io::VertexOutput, mesh_view_bindings::view};

@group(3) @binding(0) var<uniform> top_color: vec4<f32>;
@group(3) @binding(1) var<uniform> mid_color: vec4<f32>;
@group(3) @binding(2) var<uniform> bottom_color: vec4<f32>;
// x: cloud amount (0 = none), y: time in seconds, z: 1 for the HDR (ray-traced) tier.
@group(3) @binding(3) var<uniform> params: vec4<f32>;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p_in: vec2<f32>) -> f32 {
    var p = p_in;
    var v = 0.0;
    var a = 0.5;
    for (var i = 0; i < 5; i++) {
        v += a * noise(p);
        p *= 2.03;
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    let h = dir.y;
    var color = mix(bottom_color.rgb, mid_color.rgb, smoothstep(-0.05, 0.22, h));
    color = mix(color, top_color.rgb, smoothstep(0.2, 0.75, h));
    if (params.x > 0.0 && h > 0.02) {
        let t = params.y;
        let uv = dir.xz / (h + 0.18) * 1.6 + vec2<f32>(t * 0.004, t * 0.0015);
        let c = smoothstep(0.52, 0.78, fbm(uv)) * params.x * smoothstep(0.02, 0.25, h);
        color = mix(color, vec3<f32>(1.0), c);
    }
    // The ray-traced tier renders in HDR and tone maps the whole frame, sky included; the other tiers draw this dome
    // straight into an LDR target. Pre-compensate the luminance-Reinhard curve (out = c / (1 + L)) so the sky keeps its
    // exact colours in every tier.
    if (params.z > 0.5) {
        let l = clamp(dot(color, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 0.98);
        color = color / (1.0 - l);
    }
    return vec4<f32>(color, 1.0);
}
