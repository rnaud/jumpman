// One shader for every N64-look attack effect (Star Swarm weapons, Gem Clash hits). It fakes the console's tricks with
// plain vertex + fragment code (no textures, no compute, no storage buffers, so it is the same on Metal, GLES and
// Vulkan):
//   * vertices snap to a coarse screen grid and a few of them jump a cell at 15 fps (the console's wobbling polygons);
//   * "textures" are 16-ish texel procedural lattices, sampled with the 3-point filter, flipped through at 15 fps;
//   * colours are ramped through a handful of hard steps and quantised, with a 4x4 ordered dither on the high tier;
//   * sprites are camera-facing quads (and ribbons for beams), blended additively or with alpha.
// `pattern` picks the look, see `fx/n64.rs` for the numbering.
import bevy_pbr::render::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
};

// rgb: base tint (linear). a: unused.
@group(3) @binding(0) var<uniform> tint: vec4<f32>;
// x: pattern, y: progress through the effect's life (0..1), z: seed, w: intensity (alpha multiplier).
@group(3) @binding(1) var<uniform> p0: vec4<f32>;
// x: quality (1 low, 2 high), y: billboard (0 none, 1 sprite, 2 ribbon), z: vertex snap in pixels, w: 1 / half extent of the mesh.
@group(3) @binding(2) var<uniform> p1: vec4<f32>;
// x: variant, y: spin (radians per second), z: unused, w: 1 additive / 0 alpha blend.
@group(3) @binding(3) var<uniform> p2: vec4<f32>;

const TAU: f32 = 6.2831853;
const FPS: f32 = 15.0;

fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Lattice noise with the N64's 3-point filter: each texel square is two triangles, blended from three corners.
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let a = hash12(i);
    let b = hash12(i + vec2<f32>(1.0, 0.0));
    let c = hash12(i + vec2<f32>(0.0, 1.0));
    let d = hash12(i + vec2<f32>(1.0, 1.0));
    if (f.x + f.y < 1.0) {
        return a + (b - a) * f.x + (c - a) * f.y;
    }
    return d + (c - d) * (1.0 - f.x) + (b - d) * (1.0 - f.y);
}

// Nearest-texel snap: a 16x16 texture stretched over the sprite.
fn texel(p: vec2<f32>, res: f32) -> vec2<f32> {
    return (floor(p * res) + 0.5) / res;
}

fn rot(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

// Heat 0..1 in four hard steps: dim tint, tint, tint towards white, white.
fn ramp(h: f32, base: vec3<f32>) -> vec3<f32> {
    let s = min(floor(clamp(h, 0.0, 1.0) * 4.0), 3.0);
    if (s < 0.5) {
        return base * 0.45;
    }
    if (s < 1.5) {
        return base;
    }
    if (s < 2.5) {
        return mix(base, vec3<f32>(1.0), 0.55);
    }
    return vec3<f32>(1.0);
}

fn bayer4(p: vec2<f32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let x = u32(p.x) & 3u;
    let y = u32(p.y) & 3u;
    return (m[x + y * 4u] + 0.5) / 16.0;
}

// ----------------------------------------------------------------------------------------------------------------
// Vertex stage
// ----------------------------------------------------------------------------------------------------------------

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let v = decompress_vertex(vertex_no_morph, vertex_no_morph.instance_index);
    let m = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);
    var wp: vec3<f32>;
    var lp = v.position * select(p1.w, 1.0, p1.w == 0.0);
    if (p1.y > 1.5) {
        // Ribbon: a quad whose local y runs along the beam (the entity's z axis) and local x across it.
        let centre = m[3].xyz;
        let axis = m[2].xyz;
        let side = normalize(cross(axis, centre - view.world_position));
        wp = centre + axis * v.position.y + side * v.position.x * length(m[0].xyz);
        lp = vec3<f32>(v.position.x, v.position.y * 2.0, 0.0);
    } else if (p1.y > 0.5) {
        // Sprite: faces the camera, sized by the entity's scale.
        let centre = m[3].xyz;
        let right = view.world_from_view[0].xyz;
        let up = view.world_from_view[1].xyz;
        wp = centre + right * v.position.x * length(m[0].xyz) + up * v.position.y * length(m[1].xyz);
        lp = vec3<f32>(v.position.x, v.position.y, 0.0);
    } else {
        wp = mesh_functions::mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    }
    var clip = position_world_to_clip(wp);
    let fr = floor(globals.time * FPS);
    if (p1.z > 0.0 && clip.w > 0.05) {
        // The console's vertex grid: snap to cells of `p1.z` pixels; on the high tier a few vertices jump a cell
        // every flipbook frame.
        let cell = vec2<f32>(p1.z, p1.z) * 2.0 / view.viewport.zw;
        var ndc = clip.xy / clip.w;
        var jump = vec2<f32>(0.0);
        if (p1.x > 1.5) {
            jump = round((vec2<f32>(hash12(v.position.xy * 91.7 + fr), hash12(v.position.yx * 53.3 - fr)) - 0.5) * 1.4);
        }
        ndc = (floor(ndc / cell + 0.5) + jump) * cell;
        clip = vec4<f32>(ndc * clip.w, clip.z, clip.w);
    }
    var facing = 1.0;
@if(VERTEX_NORMALS)
    facing = abs(dot(normalize(mesh_functions::mesh_normal_local_to_world(v.normal, vertex_no_morph.instance_index)), normalize(view.world_position - wp)));
    out.position = clip;
    out.world_position = vec4<f32>(wp, facing);
    out.world_normal = lp;
@if(VERTEX_UVS_A)
    out.uv = v.uv;
    return out;
}

// ----------------------------------------------------------------------------------------------------------------
// Patterns. `p` is the sprite / disc plane in -1..1, `k` the progress, `t` seconds, `fr` the 15 fps frame.
// Every pattern returns (rgb, alpha) before the quantiser.
// ----------------------------------------------------------------------------------------------------------------

// Rings: a hard-edged band whose outer edge is notched; its hot front is the outer rim.
fn pat_ring(p: vec2<f32>, k: f32, seed: f32, variant: i32, base: vec3<f32>, t: f32, fr: f32) -> vec4<f32> {
    let r = length(p);
    let u = clamp((r - 0.78) / 0.22, 0.0, 1.0);
    let seg = atan2(p.y, p.x) / TAU + 0.5;
    var tooth = 1.0;
    if (variant == 0) {
        tooth = 0.8 + 0.2 * hash12(vec2<f32>(floor(seg * 48.0), fr + seed * 13.0));
    } else if (variant == 1) {
        let f = fract(seg * 20.0);
        tooth = 0.3 + 0.7 * (1.0 - abs(f - 0.5) * 2.0) * (0.6 + 0.4 * hash12(vec2<f32>(floor(seg * 20.0), fr)));
    } else if (variant == 2) {
        tooth = 0.25 + 0.75 * hash12(vec2<f32>(floor(seg * 28.0), fr + seed * 9.0));
    } else if (variant == 3) {
        tooth = 0.3 + 0.7 * (1.0 - abs(fract(seg * 16.0) - 0.5) * 2.0);
    } else if (variant == 4) {
        tooth = select(0.0, 1.0, fract(seg * 3.0 + t * 1.5) < 0.62);
    } else if (variant == 5) {
        tooth = select(0.15, 1.0, fract(seg * 12.0 + t * 0.8) < 0.5);
    } else {
        tooth = select(0.4, 1.0, fract(seg * 10.0 - t * 0.35) < 0.6);
    }
    tooth *= 1.0 - 0.35 * k;
    if (u > tooth) {
        return vec4<f32>(0.0);
    }
    var h = u / max(tooth, 0.05);
    if (variant == 4) {
        h = 0.5 + 0.5 * h;
    }
    return vec4<f32>(ramp(h, base), select(0.5, 1.0, h > 0.3));
}

// Ground zones (filled discs): 0 fire, 1 poison cloud, 2 black hole.
fn pat_zone(p: vec2<f32>, k: f32, seed: f32, variant: i32, base: vec3<f32>, t: f32, fr: f32) -> vec4<f32> {
    let r = length(p);
    if (r > 1.0) {
        return vec4<f32>(0.0);
    }
    let q = texel(p, 14.0);
    let rr = length(q);
    if (variant == 0) {
        let n = vnoise(q * 3.4 + vec2<f32>(seed * 7.0, -t * 1.8));
        let n2 = vnoise(q * 7.0 + vec2<f32>(-fr * 0.31, t * 0.9));
        var heat = (n * 0.7 + n2 * 0.3) * 1.35 - rr * 0.9 + 0.38;
        if (r > 0.88) {
            heat = max(heat, 0.55 + 0.35 * hash12(vec2<f32>(floor(atan2(p.y, p.x) * 6.0), fr)));
        }
        let spark = step(0.975, hash12(floor(p * 16.0) + fr));
        heat = max(heat, spark);
        return vec4<f32>(ramp(heat, base), select(0.0, 1.0, heat > 0.2));
    }
    if (variant == 1) {
        let n = vnoise(q * 2.6 + vec2<f32>(t * 0.25, -t * 0.2));
        let g = p * 5.0;
        let cell = floor(g);
        let f = fract(g) - 0.5;
        let phase = fract(t * 0.55 + hash12(cell + seed));
        let centre = (vec2<f32>(hash12(cell + 1.7), hash12(cell + 3.1)) - 0.5) * 0.4;
        let d = length(f - centre);
        let bubble = step(abs(d - 0.3 * phase), 0.07) * step(0.35, hash12(cell + 9.1));
        var col = mix(base * 0.35, base * 0.8, step(0.45, n));
        var a = select(0.0, 0.62, n > 0.3);
        if (bubble > 0.5) {
            col = mix(base, vec3<f32>(1.0), 0.6);
            a = 0.95;
        }
        if (r > 0.9) {
            col = base;
            a = 0.8;
        }
        return vec4<f32>(col, a);
    }
    // Black hole: dark spiral arms, bright rim.
    let s = atan2(p.y, p.x) + r * 7.0 - t * 3.0;
    let arms = step(0.5, fract(s / TAU * 3.0));
    var col = mix(vec3<f32>(0.02, 0.0, 0.06), base, arms * (0.25 + 0.75 * r));
    var a = 0.9 - 0.2 * (1.0 - r);
    if (r > 0.9) {
        col = mix(base, vec3<f32>(1.0), 0.4);
        a = 1.0;
    }
    return vec4<f32>(col, a);
}

// A sword swing: the blade sweeps the pie slice, leaving a crescent trail.
fn pat_slash(p: vec2<f32>, k: f32, seed: f32, base: vec3<f32>, fr: f32) -> vec4<f32> {
    let r = length(p);
    let a = atan2(p.x, p.y);
    let head = mix(-0.6, 0.6, clamp(k * 1.7, 0.0, 1.0));
    let d = head - a;
    if (d < 0.0 || d > 0.9 || r < 0.3) {
        return vec4<f32>(0.0);
    }
    let trail = 1.0 - d / 0.9;
    let band = (r - 0.3) / 0.7;
    if (band < (1.0 - trail) * 0.9) {
        return vec4<f32>(0.0);
    }
    let streak = hash12(vec2<f32>(floor(a * 40.0), seed));
    let h = trail * (0.4 + 0.6 * band) + (streak - 0.5) * 0.4;
    if (h < 0.12) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(ramp(h + 0.3 + select(0.0, 0.4, d < 0.1 && band > 0.7), base), 1.0);
}

// Explosion / fireball / missile / smoke sprites (variant 0..3).
fn pat_burst(p: vec2<f32>, k: f32, seed: f32, variant: i32, base: vec3<f32>, t: f32, fr: f32) -> vec4<f32> {
    let r = length(p);
    let q = texel(p, 12.0);
    let page = floor(k * 8.0);
    let ang = atan2(p.y, p.x) / TAU + 0.5;
    if (variant == 0) {
        let n = vnoise(q * 3.0 + vec2<f32>(seed * 17.0 + page * 3.7, page * 1.3));
        let spike = 0.62 + 0.38 * hash12(vec2<f32>(floor(ang * 14.0), page + seed * 5.0));
        let heat = (1.0 - r / spike) * 1.1 + (n - 0.5) * 0.5 - k * 0.45;
        if (heat <= 0.0) {
            return vec4<f32>(0.0);
        }
        return vec4<f32>(ramp(heat, base), 1.0);
    }
    if (variant == 1) {
        let n = vnoise(q * 3.2 + vec2<f32>(seed * 11.0, -t * 2.4));
        let heat = (1.0 - r * 1.25) + (n - 0.5) * 0.9;
        if (heat <= 0.05 || r > 1.0) {
            return vec4<f32>(0.0);
        }
        return vec4<f32>(ramp(heat * 1.25, base), 1.0);
    }
    if (variant == 2) {
        let n = vnoise(q * 4.0 + vec2<f32>(seed * 11.0, -t * 3.0));
        let heat = (1.0 - r * 1.4) + (n - 0.5) * 0.6;
        if (heat <= 0.0) {
            return vec4<f32>(0.0);
        }
        return vec4<f32>(ramp(heat * 1.4, base), 1.0);
    }
    let n = vnoise(q * 3.0 + vec2<f32>(seed * 17.0, 0.0));
    let edge = 0.95 - k * 0.2;
    if (r + (n - 0.5) * 0.5 > edge) {
        return vec4<f32>(0.0);
    }
    let shade = min(floor((1.0 - r + n * 0.6) * 3.0), 2.0) / 2.0;
    return vec4<f32>(mix(base * 0.4, base, shade), 0.9 * (1.0 - k));
}

// A hard four- (and eight-) pointed star with a glowing core. Spins with `ang`.
fn pat_star(p: vec2<f32>, k: f32, ang: f32, base: vec3<f32>) -> vec4<f32> {
    let rp = rot(p, ang);
    let s = sqrt(abs(rp.x)) + sqrt(abs(rp.y));
    let rd = rot(p, ang + 0.7854);
    let s2 = sqrt(abs(rd.x)) + sqrt(abs(rd.y));
    let size = 1.0 - k * 0.4;
    let r = length(p);
    if (s < 1.0 * size) {
        let heat = 1.0 - s / size;
        return vec4<f32>(ramp(heat * 1.3 + 0.2, base), 1.0);
    }
    if (s2 < 0.75 * size) {
        return vec4<f32>(base, 0.8);
    }
    if (r < 0.62) {
        return vec4<f32>(base * 0.6, 0.35);
    }
    return vec4<f32>(0.0);
}

// Orbiting blades: a spinning cross of tapered spikes in a faint halo.
fn pat_blade(p: vec2<f32>, ang: f32, base: vec3<f32>, fr: f32) -> vec4<f32> {
    let rp = rot(p, ang);
    let ax = abs(rp.x);
    let ay = abs(rp.y);
    let r = length(p);
    if ((ax < 0.2 * (1.0 - ay) && ay < 0.95) || (ay < 0.2 * (1.0 - ax) && ax < 0.95)) {
        return vec4<f32>(ramp(1.0 - r * 0.9, base), 1.0);
    }
    if (r < 0.55) {
        return vec4<f32>(base * 0.7, 0.4);
    }
    return vec4<f32>(0.0);
}

// Dark energy orb: black core, swirling arms, bright rim. Alpha blended.
fn pat_orb(p: vec2<f32>, t: f32, base: vec3<f32>) -> vec4<f32> {
    let r = length(p);
    if (r > 0.92) {
        return vec4<f32>(0.0);
    }
    let s = atan2(p.y, p.x) + r * 5.0 - t * 5.0;
    let arms = step(0.5, fract(s / TAU * 3.0));
    var col = mix(vec3<f32>(0.03, 0.0, 0.08), base * 0.8, arms * r);
    if (r > 0.68) {
        col = mix(base, vec3<f32>(1.0), 0.35);
    }
    return vec4<f32>(col, 0.95);
}

// Boomerang: a pinwheel of three crescents.
fn pat_boom(p: vec2<f32>, t: f32, base: vec3<f32>) -> vec4<f32> {
    let r = length(p);
    if (r > 0.95 || r < 0.22) {
        return vec4<f32>(0.0);
    }
    let a = atan2(p.y, p.x) / TAU + 0.5;
    let arm = fract(a * 3.0 - r * 0.7 + t * 1.2);
    if (arm > 0.45) {
        return vec4<f32>(0.0);
    }
    let edge = step(arm, 0.1) + step(0.36, arm);
    return vec4<f32>(ramp(0.55 + 0.45 * edge + (1.0 - r) * 0.2, base), 1.0);
}

// A comic "pop": an expanding ring with eight rays flying out of it.
fn pat_pop(p: vec2<f32>, k: f32, seed: f32, base: vec3<f32>) -> vec4<f32> {
    let r = length(p);
    let ring = 0.25 + 0.7 * k;
    let seg = atan2(p.y, p.x) / TAU + 0.5;
    let rays = step(0.45, fract(seg * 8.0 + seed)) * step(ring * 0.55, r) * step(r, ring * 0.95 + 0.1);
    let band = step(abs(r - ring), 0.1 * (1.0 - k) + 0.025);
    let core = step(r, 0.3 * (1.0 - k));
    if (core > 0.5) {
        return vec4<f32>(vec3<f32>(1.0), 1.0);
    }
    if (band > 0.5 || rays > 0.5) {
        return vec4<f32>(ramp(1.0 - k * 0.6 - r * 0.2, base), 1.0);
    }
    return vec4<f32>(0.0);
}

// Lightning along a ribbon: a jagged polyline re-rolled every flipbook frame.
fn pat_beam(p: vec2<f32>, k: f32, seed: f32, base: vec3<f32>, fr: f32) -> vec4<f32> {
    let s = (p.y * 0.5 + 0.5) * 9.0;
    let i = floor(s);
    let f = fract(s);
    let taper = 1.0 - p.y * p.y * 0.6;
    let o0 = (hash12(vec2<f32>(i, fr + seed * 31.0)) - 0.5) * 1.1;
    let o1 = (hash12(vec2<f32>(i + 1.0, fr + seed * 31.0)) - 0.5) * 1.1;
    let path = mix(o0, o1, f) * taper;
    let d = abs(p.x - path);
    let w = 1.0 - k * 0.6;
    if (d < 0.07 * w) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }
    if (d < 0.2 * w) {
        return vec4<f32>(base, 0.95);
    }
    if (d < 0.36 * w) {
        return vec4<f32>(base * 0.6, 0.4);
    }
    // A forked twig on the high tier's second noise pass.
    let o2 = (hash12(vec2<f32>(i + 17.0, fr + seed * 7.0)) - 0.5) * 1.6;
    let o3 = (hash12(vec2<f32>(i + 18.0, fr + seed * 7.0)) - 0.5) * 1.6;
    if (abs(p.x - mix(o2, o3, f) * taper) < 0.04 * w && p1.x > 1.5 && fract(seed * 3.0 + i * 0.37) < 0.4) {
        return vec4<f32>(base, 0.8);
    }
    return vec4<f32>(0.0);
}

// A glowing shell round a fighter: flame licks climb its silhouette.
fn pat_aura(l: vec3<f32>, facing: f32, seed: f32, base: vec3<f32>, t: f32) -> vec4<f32> {
    let rim = 1.0 - facing;
    let n = vnoise(texel(vec2<f32>(l.x * 2.0 + l.z, l.y * 2.6 - t * 1.6 + seed * 9.0), 8.0) * 3.0);
    let heat = rim * 0.95 + (n - 0.5) * 0.7;
    if (heat < 0.3) {
        return vec4<f32>(base, 0.1);
    }
    let hot = step(0.85, heat);
    return vec4<f32>(mix(base, vec3<f32>(1.0), 0.35 * hot + 0.1), 0.42 + 0.2 * hot);
}

// A column of light: scrolling bars, fading out at both ends.
fn pat_pillar(l: vec3<f32>, seed: f32, base: vec3<f32>, t: f32) -> vec4<f32> {
    let y = clamp(l.y, -1.0, 1.0);
    let fade = 1.0 - abs(y);
    let bar = step(0.78, fract(y * 3.0 - t * 1.1 + seed));
    let edge = step(0.35, vnoise(vec2<f32>(l.x * 3.0 + l.z * 5.0, floor(y * 6.0 - t * 4.0))));
    let a = fade * (0.35 + 0.3 * bar + 0.15 * edge);
    return vec4<f32>(mix(base, vec3<f32>(1.0), 0.45 * bar), a);
}

// ----------------------------------------------------------------------------------------------------------------
// Fragment stage
// ----------------------------------------------------------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let pat = i32(p0.x + 0.5);
    let k = clamp(p0.y, 0.0, 1.0);
    let seed = p0.z;
    let variant = i32(p2.x + 0.5);
    let hq = p1.x > 1.5;
    let t = globals.time;
    let fr = floor(t * FPS);
    let base = tint.rgb;
    let l = in.world_normal;
    let facing = in.world_position.w;
    var p = l.xy;
    if (p1.y > 0.5 && p1.y < 1.5) {
        p = in.uv * 2.0 - 1.0;
    }
    let ang = seed * TAU + t * p2.y;
    var c = vec4<f32>(0.0);
    switch pat {
        case 0: { c = pat_ring(p, k, seed, variant, base, t, fr); }
        case 1: { c = pat_zone(p, k, seed, variant, base, t, fr); }
        case 2: { c = pat_slash(p, k, seed, base, fr); }
        case 3: { c = pat_burst(p, k, seed, variant, base, t, fr); }
        case 4: { c = pat_star(p, k, ang, base); }
        case 5: { c = pat_blade(p, ang, base, fr); }
        case 6: { c = pat_orb(p, t, base); }
        case 7: { c = pat_boom(p, t, base); }
        case 8: { c = pat_pop(p, k, seed, base); }
        case 9: { c = pat_beam(p, k, seed, base, fr); }
        case 10: { c = pat_aura(l, facing, seed, base, t); }
        case 11: { c = pat_pillar(l, seed, base, t); }
        default: { c = vec4<f32>(base, 0.5); }
    }
    var al = c.a * p0.w;
    // Quantise: a few colour levels, a few alpha levels, ordered dither on the high tier.
    let dith = select(0.0, bayer4(in.position.xy) - 0.5, hq);
    let levels = select(4.0, 6.0, hq);
    let rgb = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let q = floor(sqrt(rgb) * levels + dith + 0.5) / levels;
    let col = q * q;
    let alevels = select(3.0, 4.0, hq);
    al = floor(clamp(al, 0.0, 1.0) * alevels + dith + 0.5) / alevels;
    if (al < 0.05) {
        discard;
    }
    if (p2.w > 0.5) {
        return vec4<f32>(col * al, 0.0);
    }
    return vec4<f32>(col, al);
}
