// WGSL port of ThreeBodyShaders.swift (Metal). Two passes:
// - `evolve` renders into a half-resolution float texture. It warps the previous frame (zoom, rotation and a
//   smear along the gravity field), fades it, and adds the bodies' glow and equipotential rings, so motion
//   leaves psychedelic trails.
// - `present` draws that texture to the window with a split-color edge, tone mapping, a vignette and the
//   optional band meters.

struct Uniforms {
    view: vec4<f32>,             // resolution.x, resolution.y, flow phase, aspect
    bodies: array<vec4<f32>, 3>, // position.xy (y spans -1...1), mass, band level
    bands: vec4<f32>,            // low, mid, high, loudest
    hits: vec4<f32>,             // low, mid, high, hue phase
    feedback: vec4<f32>,         // zoom, rotation, decay, show meters
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

@vertex
fn fullscreen(@builtin(vertex_index) id: u32) -> VertexOut {
    let corner = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

fn palette(t: f32) -> vec3<f32> {
    return 0.5 + 0.5 * cos(6.2831853 * (t + vec3<f32>(0.0, 0.33, 0.67)));
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Metal's fmin/fmax drop NaNs; WGSL may assume there are none, so test the bits instead.
fn finite_or_zero(v: vec3<f32>) -> vec3<f32> {
    let bits = bitcast<vec3<u32>>(v) & vec3<u32>(0x7fffffffu);
    return select(v, vec3<f32>(0.0), bits >= vec3<u32>(0x7f800000u));
}

// smoothstep(1.3, 0.3, x) in Metal: reversed edges, written out so it doesn't depend on the backend.
fn falloff(x: f32) -> f32 {
    return 1.0 - smoothstep(0.3, 1.3, x);
}

@fragment
fn evolve(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.w;
    let flow = u.view.z;
    let low = u.bands.x;
    let mid = u.bands.y;
    let high = u.bands.z;

    // y spans -1...1 and points up, matching the physics.
    let p = (in.uv - 0.5) * vec2<f32>(aspect, -1.0) * 2.0;

    // Gravitational potential, pull and per-body glow.
    var phi = 0.0;
    var pull = vec2<f32>(0.0);
    var glow = vec3<f32>(0.0);
    for (var i = 0; i < 3; i++) {
        let d = u.bodies[i].xy - p;
        let r2 = dot(d, d) + 0.003;
        let mass = u.bodies[i].z;
        phi += mass / sqrt(r2);
        pull += mass * d / (r2 * sqrt(r2));
        let size = 0.0025 + 0.008 * u.bodies[i].w;
        glow += palette(u.hits.w + f32(i) / 3.0) * (size / r2);
    }

    // Feedback: rotate and zoom the last frame, then smear it along the field like gravitational lensing.
    let angle = u.feedback.y;
    let rotation = mat2x2<f32>(vec2<f32>(cos(angle), sin(angle)), vec2<f32>(-sin(angle), cos(angle)));
    var q = rotation * p * u.feedback.x;
    let pull_length = length(pull);
    q -= (pull / max(pull_length, 1e-4)) * min(pull_length, 30.0) * (0.0003 + 0.0015 * mid);
    let source_uv = vec2<f32>(q.x / aspect, -q.y) * 0.5 + 0.5;
    // Metal sampled with clamp_to_zero; outside the texture there is nothing to carry over.
    let inside = all(source_uv >= vec2<f32>(0.0)) && all(source_uv <= vec2<f32>(1.0));
    let previous = select(vec3<f32>(0.0), textureSampleLevel(image, smp, source_uv, 0.0).rgb, inside);
    let old = clamp(finite_or_zero(previous), vec3<f32>(0.0), vec3<f32>(64.0)) * u.feedback.z;

    // Equipotential rings flowing outward. Bass makes them denser and brighter; highs sharpen them.
    let density = 0.6 + 1.6 * low;
    let wave = phi * density - flow;
    // Clamp the base: rounding can push it a hair below zero, and pow() of a negative is NaN.
    let ring = pow(saturate(0.5 + 0.5 * cos(wave * 6.2831853)), 8.0 - 5.0 * high);
    var color = palette(phi * 0.06 + u.hits.w) * ring;

    // Highs: glitter along the rings.
    let sparkle = step(1.0 - 0.05 * high, hash(floor(in.uv * u.view.xy) + floor(flow * 7.0)));
    color += sparkle * ring * high * 1.5;

    let strength = 0.06 + 0.2 * low + 0.35 * u.hits.x;
    let fresh = color * strength + glow * (0.35 + 0.6 * u.bands.w + 0.6 * u.hits.y);
    return vec4<f32>(old + fresh, 1.0);
}

@fragment
fn present(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // Split-color edge that widens with the highs.
    let split = (uv - 0.5) * (0.002 + 0.014 * u.bands.z + 0.01 * u.hits.z);
    var c = vec3<f32>(
        textureSampleLevel(image, smp, uv + split, 0.0).r,
        textureSampleLevel(image, smp, uv, 0.0).g,
        textureSampleLevel(image, smp, uv - split, 0.0).b);
    c = 1.0 - exp(-c * 1.3);

    let vignette = falloff(length((uv - 0.5) * vec2<f32>(u.view.w, 1.0)));
    c *= mix(0.5, 1.0, vignette);

    if (u.feedback.w > 0.5) {
        let px = vec2<f32>(uv.x, 1.0 - uv.y) * u.view.xy;
        let unit = max(1.0, u.view.y / 500.0);
        for (var i = 0; i < 3; i++) {
            let x0 = (16.0 + f32(i) * 16.0) * unit;
            let y0 = 16.0 * unit;
            let h = 110.0 * unit;
            if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + h) {
                let fill = (px.y - y0) / h;
                var meter = select(vec3<f32>(0.1), palette(f32(i) / 3.0 + 0.05), fill < u.bands[i]);
                if (u.hits[i] > 0.6 && fill > 0.94) {
                    meter = vec3<f32>(1.0);
                }
                c = mix(c, meter, 0.85);
            }
        }
    }
    return vec4<f32>(c, 1.0);
}
