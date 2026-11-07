// The CRT look for the main window: phosphor bloom (a separable blur at half resolution), a slight RGB
// split, scanlines, flicker, grain and a vignette. The bloom swells with the bass.

struct Uniforms {
    resolution: vec2<f32>,   // target size in pixels
    texel: vec2<f32>,        // 1 / size of the texture being sampled
    direction: vec2<f32>,    // blur passes: (1, 0) or (0, 1)
    time: f32,
    glow: f32,               // 0...1 bass envelope
    scale: f32,              // pixels per logical point
    _pad: vec3<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;
@group(0) @binding(3) var bloom: texture_2d<f32>;

@vertex
fn fullscreen(@builtin(vertex_index) id: u32) -> VertexOut {
    let corner = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@fragment
fn blur(in: VertexOut) -> @location(0) vec4<f32> {
    let weights = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    let step = u.direction * u.texel * 2.0;
    var c = textureSampleLevel(image, smp, in.uv, 0.0).rgb * weights[0];
    for (var i = 1; i < 5; i++) {
        let offset = step * f32(i);
        c += textureSampleLevel(image, smp, in.uv + offset, 0.0).rgb * weights[i];
        c += textureSampleLevel(image, smp, in.uv - offset, 0.0).rgb * weights[i];
    }
    return vec4<f32>(c, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn composite(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let split = vec2<f32>(0.55 * u.scale * u.texel.x, 0.0);
    var c = vec3<f32>(
        textureSampleLevel(image, smp, uv + split, 0.0).r,
        textureSampleLevel(image, smp, uv, 0.0).g,
        textureSampleLevel(image, smp, uv - split, 0.0).b);

    // Phosphor halo around everything lit, brighter on the beat.
    c += textureSampleLevel(bloom, smp, uv, 0.0).rgb * (0.55 + 0.75 * u.glow);

    // Scanlines: one dark band every 1.5 points.
    let y = in.position.y;
    let band = 0.5 + 0.5 * cos(6.2831853 * y / (1.5 * u.scale));
    c *= 1.0 - 0.14 * band;

    // Flicker and grain, kept faint enough to stay readable.
    c *= 1.0 + 0.01 * sin(u.time * 53.0) + 0.006 * sin(u.time * 7.0);
    c += (hash(floor(in.position.xy) + fract(u.time) * 91.0) - 0.5) * 0.014;

    let d = length((uv - 0.5) * vec2<f32>(u.resolution.x / u.resolution.y, 1.0));
    c *= mix(0.68, 1.0, 1.0 - smoothstep(0.35, 1.05, d));
    return vec4<f32>(max(c, vec3<f32>(0.0)), 1.0);
}
