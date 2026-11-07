// Black Hole, second pass: see blackhole.wgsl.

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

const TAU: f32 = 6.2831853;

fn sstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn hash21(q: vec2<f32>) -> f32 {
    var p = fract(q * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fn palette(t: f32) -> vec3<f32> {
    let c = 0.5 + 0.5 * cos(TAU * (t + vec3<f32>(0.0, 0.33, 0.67)));
    return mix(c, vec3<f32>(0.6), 0.15);
}

// Presentation: scales the traced frame up to the window with a little zoom blur (the sense of falling), colour
// fringing toward the edges, filmic tone mapping and band meters.

struct Present {
    window: vec4<f32>,  // width, height, zoom blur, colour fringe
    meters: vec4<f32>,  // low, mid, high, show meters
};

@group(0) @binding(0) var<uniform> pu: Present;
@group(0) @binding(1) var frame: texture_2d<f32>;
@group(0) @binding(2) var frame_sampler: sampler;

fn tonemap(x: vec3<f32>) -> vec3<f32> {
    // ACES fit (Narkowicz).
    let a = x * (2.51 * x + 0.03);
    let b = x * (2.43 * x + 0.59) + 0.14;
    return clamp(a / b, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn present(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let from_center = uv - 0.5;
    var color = vec3<f32>(0.0);
    let taps = 6;
    for (var i = 0; i < taps; i++) {
        let zoom = 1.0 - pu.window.z * f32(i) / f32(taps);
        let fringe = pu.window.w * dot(from_center, from_center);
        let base = 0.5 + from_center * zoom;
        color += vec3<f32>(
            textureSampleLevel(frame, frame_sampler, 0.5 + (base - 0.5) * (1.0 + fringe), 0.0).r,
            textureSampleLevel(frame, frame_sampler, base, 0.0).g,
            textureSampleLevel(frame, frame_sampler, 0.5 + (base - 0.5) * (1.0 - fringe), 0.0).b,
        );
    }
    color = tonemap(color / f32(taps) * 1.1);
    color *= mix(0.65, 1.0, 1.0 - sstep(0.35, 0.85, length(from_center * vec2<f32>(pu.window.x / pu.window.y, 1.0))));
    color += (hash21(in.position.xy) - 0.5) / 255.0;

    if (pu.meters.w > 0.5) {
        let px = vec2<f32>(uv.x, 1.0 - uv.y) * pu.window.xy;
        let unit = max(1.0, pu.window.y / 500.0);
        for (var i = 0; i < 3; i++) {
            let x0 = (16.0 + f32(i) * 16.0) * unit;
            let y0 = 16.0 * unit;
            let h = 110.0 * unit;
            if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + h) {
                let fill = (px.y - y0) / h;
                color = mix(color, select(vec3<f32>(0.06), palette(f32(i) * 0.33 + 0.1), fill < pu.meters[i]), 0.85);
            }
        }
    }
    return vec4<f32>(color, 1.0);
}
