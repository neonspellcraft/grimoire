// WGSL port of SplatterShaders.swift (Metal).
//
// The paint lives in a persistent float texture (ping-ponged each frame):
// - `dry_fragment` copies the previous canvas across, fading the color very slowly and the wetness (alpha)
//   quickly, so fresh paint glistens and then dries.
// - `stamp_vertex`/`stamp_fragment` lay this frame's new paint on top. Color blends "over", and wetness takes the
//   maximum. Stamp kinds:
//   0 superstring blob: the rim is a closed string vibrating in overtones with a random spectrum
//   1 round droplet (stretched along its flight)
//   2 quantum orbital: |cos mθ|^p lobes, sometimes with a radial node
//   3 Einstein ring: a lensed ring that thickens into two bright arcs, around a central source
//   4 cosmic inflation: a thresholded Gaussian random field
//   5 Kerr black hole: logarithmic spiral arms twisted by frame dragging
// - `downsample` builds the canvas's mipmaps (Metal's generateMipmaps), for the bloom.
// - `present_fragment` draws the canvas with a bloom made from those mipmaps, extra shine on wet paint,
//   a faint canvas grain and the optional band meters.

struct Uniforms {
    view: vec4<f32>,    // width px, height px, aspect, time
    dry: vec4<f32>,     // color decay, wetness decay, glow strength, show meters
    bands: vec4<f32>,   // low, mid, high, unused
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var canvas: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct StampOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) seed: f32,
    @location(3) kind: f32,
    @location(4) params: vec2<f32>,
    @location(5) aa: f32,
};

// smoothstep written out, so reversed edges (a > b) behave the same on every backend, as in Metal.
fn sstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn h1(n: f32) -> f32 {
    return fract(sin(n * 127.1) * 43758.5453);
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

@vertex
fn fullscreen(@builtin(vertex_index) id: u32) -> VertexOut {
    let corner = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@fragment
fn dry_fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let paint = textureLoad(canvas, vec2<i32>(in.position.xy), 0);
    return vec4<f32>(paint.rgb * u.dry.x, paint.a * u.dry.y);
}

@fragment
fn downsample(in: VertexOut) -> @location(0) vec4<f32> {
    // Bilinear at the shared corner of a 2x2 block of the level above = the box average.
    return textureSampleLevel(canvas, smp, in.uv, 0.0);
}

@vertex
fn stamp_vertex(
    @builtin(vertex_index) vid: u32,
    @location(0) shape: vec4<f32>,  // center xy (screen), radius, rotation
    @location(1) color: vec4<f32>,  // rgb, seed
    @location(2) style: vec4<f32>,  // kind, stretch, two shape parameters
) -> StampOut {
    let kind = style.x;
    let stretch = max(style.y, 1.0);
    // Blobs need room for their fingers; droplets are stretched along their direction of travel.
    let extent = select(1.75, 1.1, abs(kind - 1.0) < 0.5);
    let corner = vec2<f32>(select(-1.0, 1.0, (vid & 1u) != 0u), select(-1.0, 1.0, (vid & 2u) != 0u)) * extent;
    let scaled = corner * shape.z * vec2<f32>(stretch, 1.0);
    let c = cos(shape.w);
    let sn = sin(shape.w);
    let point = shape.xy + vec2<f32>(c * scaled.x - sn * scaled.y, sn * scaled.x + c * scaled.y);

    var out: StampOut;
    out.position = vec4<f32>(point.x / u.view.z, point.y, 0.0, 1.0);
    out.local = corner;
    out.color = color.rgb;
    out.seed = color.w;
    out.kind = kind;
    out.params = style.zw;
    out.aa = (2.0 / u.view.y) / max(shape.z, 1e-4) * 1.2;
    return out;
}

@fragment
fn stamp_fragment(in: StampOut) -> @location(0) vec4<f32> {
    let q = in.local;
    let d = length(q);
    let a = atan2(q.y, q.x);
    let s = in.seed;
    let kind = i32(in.kind + 0.5);
    let aa = in.aa;
    var cover: f32;

    if (kind == 0) {
        // Superstring: overtones 2...N of a closed string with random phases; `tilt` sets how fast they fall off.
        let tilt = in.params.x;
        let modes = i32(in.params.y);
        var edge = 1.0;
        for (var n = 2; n < modes; n++) {
            edge += 0.34 / pow(f32(n), tilt) * sin(f32(n) * a + 6.2831853 * h1(s + f32(n) * 1.7));
        }
        let k = 5.0 + floor(h1(s + 3.0) * 6.0);
        edge += 0.5 * pow(saturate(sin(k * a + s * 3.1) * sin(3.0 * a + s * 0.7)), 5.0);
        cover = sstep(edge + aa, edge - aa, d);
    } else if (kind == 2) {
        // Quantum orbital: lobes of |cos m(θ - φ)|^p around a small core, sometimes with a radial node.
        let m = in.params.y;
        let p = in.params.x;
        let lobe = pow(abs(cos(m * (a - s))), p);
        let edge = 0.3 + 0.9 * lobe;
        cover = sstep(edge + aa, edge - aa, d);
        if (h1(s + 9.0) > 0.5) {
            let node = sstep(0.07 + aa, 0.07 - aa, abs(d - 0.5 * edge));
            cover *= 1.0 - node;
        }
    } else if (kind == 3) {
        // Einstein ring: lensing magnifies two opposite arcs; the source sits at the center.
        let thickness = (0.06 + 0.16 * pow(abs(cos(a - s)), 4.0)) * in.params.x;
        let ring = sstep(thickness + aa, thickness - aa, abs(d - 1.0));
        let source = sstep(0.2 + aa, 0.2 - aa, d);
        cover = max(ring, source);
    } else if (kind == 4) {
        // Cosmic inflation: a Gaussian random field, thresholded and confined by distance from the center.
        let p = q * in.params.y + vec2<f32>(s * 13.1, s * 7.7);
        let field = 0.55 * noise(p) + 0.3 * noise(p * 2.1 + 3.0) + 0.15 * noise(p * 4.3 + 7.0);
        let value = field - 0.55 * d * d + in.params.x;
        cover = sstep(0.5 - 0.02, 0.5 + 0.02, value);
    } else if (kind == 5) {
        // Kerr black hole: logarithmic spiral arms, twisted harder near the center by frame dragging.
        let arms = in.params.x;
        let twist = in.params.y;
        let phase = a - twist * log(max(d, 1e-3)) + s;
        let arm = pow(0.5 + 0.5 * cos(arms * phase), 4.0);
        let disc = sstep(1.0 + aa, 1.0 - aa, d);
        let horizon = sstep(0.16 + aa, 0.16 - aa, d);
        cover = max(disc * sstep(0.35, 0.65, arm + 0.25 * (1.0 - d)), horizon);
    } else {
        cover = sstep(1.0 + aa, 1.0 - aa, d);
    }
    if (cover <= 0.0) {
        discard;
    }
    let color = in.color * (0.92 + 0.2 * sstep(1.0, 0.0, d));
    return vec4<f32>(color, cover);
}

@fragment
fn present_fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let base = textureSampleLevel(canvas, smp, in.uv, 0.0);
    // Bloom from progressively blurrier mip levels: neon light bleeding into the black.
    let glow = textureSampleLevel(canvas, smp, in.uv, 2.5).rgb * 0.45
             + textureSampleLevel(canvas, smp, in.uv, 4.5).rgb * 0.35
             + textureSampleLevel(canvas, smp, in.uv, 6.5).rgb * 0.25;
    let wet = base.a;
    // Dried paint sits back so fresh, wet splats pop over it.
    var color = base.rgb * (0.62 + 0.7 * wet) + glow * u.dry.z;
    color = 1.0 - exp(-color * 1.35);
    let grain = (hash21(floor(in.uv * u.view.xy)) - 0.5) * 0.02;
    color = max(color + grain, vec3<f32>(0.0));

    if (u.dry.w > 0.5) {
        let px = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * u.view.xy;
        let unit = max(1.0, u.view.y / 500.0);
        for (var i = 0; i < 3; i++) {
            let x0 = (16.0 + f32(i) * 16.0) * unit;
            let y0 = 16.0 * unit;
            let h = 110.0 * unit;
            if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + h) {
                let fill = (px.y - y0) / h;
                color = mix(color, select(vec3<f32>(0.12), vec3<f32>(1.0, 0.25, 0.6), fill < u.bands[i]), 0.85);
            }
        }
    }
    return vec4<f32>(color, 1.0);
}
