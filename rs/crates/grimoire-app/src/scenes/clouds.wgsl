// Night Clouds: a dark night sky with slow, layered clouds drifting past a soft half-hidden moon, and faint stars
// in the gaps. Everything stays dim; the music only nudges it (see clouds.rs).

struct Uniforms {
    view: vec4<f32>,    // width px, height px, aspect, time
    drift: vec4<f32>,   // far layer offset xy, near layer offset xy
    mood: vec4<f32>,    // swell (slow bass), shimmer (mids), twinkle (highs), moon glow (kicks)
    meters: vec4<f32>,  // low, mid, high, show meters
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn fullscreen(@builtin(vertex_index) id: u32) -> VertexOut {
    let corner = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

fn sstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn hash21(q: vec2<f32>) -> f32 {
    var p = fract(q * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), w.x),
               mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), w.x), w.y);
}

fn fbm(start: vec2<f32>) -> f32 {
    var p = start;
    var value = 0.0;
    var amplitude = 0.5;
    for (var i = 0; i < 5; i++) {
        value += amplitude * noise(p);
        p = p * 2.02 + vec2<f32>(11.7, 3.1);
        amplitude *= 0.5;
    }
    return value;
}

// Cloud density at `p`: domain-warped fbm, so the shapes billow instead of looking like plain noise.
fn clouds(p: vec2<f32>, coverage: f32) -> f32 {
    let warp = vec2<f32>(fbm(p * 0.6 + vec2<f32>(3.0, 7.0)), fbm(p * 0.6 + vec2<f32>(8.3, 1.7)));
    let n = fbm(p + warp * 0.9);
    return sstep(1.0 - coverage, 1.0 - coverage + 0.32, n);
}

@fragment
fn sky(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.z;
    let time = u.view.w;
    let p = vec2<f32>(in.uv.x * aspect, in.uv.y);
    let swell = u.mood.x;
    let shimmer = u.mood.y;
    let twinkle = u.mood.z;
    let glow = u.mood.w;

    // Deep navy, a little lighter toward the horizon.
    var color = mix(vec3<f32>(0.010, 0.016, 0.034), vec3<f32>(0.003, 0.005, 0.013), sstep(0.0, 1.0, 1.0 - in.uv.y));

    // Stars: sparse, faint, twinkling with the highs.
    let cell = floor(p * 140.0);
    let star = hash21(cell);
    if (star > 0.9965) {
        let center = (cell + 0.5 + (vec2<f32>(hash21(cell + 3.1), hash21(cell + 7.7)) - 0.5) * 0.6) / 140.0;
        let d = length(p - center) * 140.0;
        let phase = hash21(cell + 1.3) * 6.2831853;
        let flicker = 0.55 + 0.45 * sin(time * (0.7 + 2.5 * twinkle) + phase) * (0.3 + 0.7 * twinkle);
        color += vec3<f32>(0.55, 0.62, 0.80) * sstep(1.2, 0.0, d) * flicker * 0.22;
    }

    // The moon: a pale disc with a soft halo, upper right.
    let moon = vec2<f32>(0.78 * aspect, 0.30);
    let r = length(p - moon);
    let halo = exp(-r * 5.0) * (0.10 + 0.06 * glow);
    // Faint maria, so it reads as a moon rather than a flat disc.
    let maria = 0.66 + 0.34 * sstep(0.35, 0.7, fbm((p - moon) * 30.0 + vec2<f32>(4.0, 9.0)));
    let disc = sstep(0.062, 0.058, r) * 0.30 * maria;
    color += vec3<f32>(0.62, 0.70, 0.86) * (halo + disc);

    // Two cloud layers: far and slow, near and a little faster. Bass swells the coverage.
    let coverage = 0.53 + 0.12 * swell;
    let far = clouds(p * 2.2 + u.drift.xy, coverage - 0.05);
    let near = clouds(p * 1.4 + u.drift.zw + vec2<f32>(40.0, 0.0), coverage);

    // Cloud bodies are dark blue-grey; edges facing the moon catch silver light (stronger with the mids).
    let toward_moon = exp(-r * 2.4);
    let rim_far = far * (1.0 - far) * 4.0;
    let rim_near = near * (1.0 - near) * 4.0;
    let body = vec3<f32>(0.036, 0.043, 0.066);
    let silver = vec3<f32>(0.40, 0.46, 0.60) * toward_moon * (0.35 + 0.45 * shimmer + 0.3 * glow);
    color = mix(color, body * 1.2 + silver * rim_far * 0.5, far * 0.75);
    color = mix(color, body + silver * rim_near, near * 0.92);

    // A faint mist low on the horizon, and a gentle vignette.
    color += vec3<f32>(0.010, 0.013, 0.022) * sstep(0.55, 1.0, in.uv.y);
    let edge = length((in.uv - 0.5) * vec2<f32>(aspect, 1.0));
    color *= mix(0.72, 1.0, 1.0 - sstep(0.4, 1.1, edge));

    // Optional band meters, bottom left, as in the other scenes (kept dim to suit the scene).
    if (u.meters.w > 0.5) {
        let px = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * u.view.xy;
        let unit = max(1.0, u.view.y / 500.0);
        for (var i = 0; i < 3; i++) {
            let x0 = (16.0 + f32(i) * 16.0) * unit;
            let y0 = 16.0 * unit;
            let h = 110.0 * unit;
            if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + h) {
                let fill = (px.y - y0) / h;
                color = mix(color, select(vec3<f32>(0.04), vec3<f32>(0.35, 0.42, 0.60), fill < u.meters[i]), 0.85);
            }
        }
    }
    return vec4<f32>(min(color, vec3<f32>(0.6)), 1.0);
}
