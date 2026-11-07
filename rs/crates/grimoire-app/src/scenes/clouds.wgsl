// Night Clouds: lying on a hillside at night, watching clouds drift past the moon. A deep indigo sky with the
// Milky Way, coloured stars and a faint aurora; moonlit clouds in three layers whose edges catch silver light and
// whose thin veils throw a rainbow corona around the moon; a distant ridge with pines along the bottom. Everything
// stays dim; the music only nudges it (see clouds.rs). Colours are display values (the surface isn't sRGB).

struct Uniforms {
    view: vec4<f32>,    // width px, height px, aspect, time
    drift: vec4<f32>,   // middle layer offset xy, near layer offset xy
    drift2: vec4<f32>,  // high (cirrus) layer offset xy, aurora phase, unused
    mood: vec4<f32>,    // swell (slow bass), shimmer (mids), twinkle (highs), moon glow (kicks)
    meteor: vec4<f32>,  // shooting star: start xy (uv), heading (radians), progress (0...1; outside = none)
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
        p = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * p + vec2<f32>(11.7, 3.1);
        amplitude *= 0.5;
    }
    return value;
}

fn fbm3(start: vec2<f32>) -> f32 {
    var p = start;
    var value = 0.0;
    var amplitude = 0.5;
    for (var i = 0; i < 3; i++) {
        value += amplitude * noise(p);
        p = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * p + vec2<f32>(5.3, 9.1);
        amplitude *= 0.5;
    }
    return value / 0.875;
}

// Cloud density at `p`: domain-warped fbm, so the shapes billow instead of looking like plain noise.
fn clouds(p: vec2<f32>, coverage: f32) -> f32 {
    let warp = vec2<f32>(fbm3(p * 0.55 + vec2<f32>(3.0, 7.0)), fbm3(p * 0.55 + vec2<f32>(8.3, 1.7)));
    var n = fbm(p + warp * 1.1);
    // Erode the edges with finer noise, so they break up into wisps instead of blurring out.
    n -= 0.12 * (fbm3(p * 4.0 + warp * 2.0) - 0.5);
    return sstep(1.0 - coverage, 1.0 - coverage + 0.16, n);
}

// One layer of stars: at most one per grid cell, soft round points 1-3 px across, blue-white to warm.
fn star_layer(p: vec2<f32>, scale: f32, threshold: f32, px: f32, time: f32, twinkle: f32, gain: f32) -> vec3<f32> {
    let g = p * scale;
    let cell = floor(g);
    let h = hash21(cell);
    if (h < threshold) {
        return vec3<f32>(0.0);
    }
    let spot = vec2<f32>(hash21(cell + 3.1), hash21(cell + 7.7)) * 0.7 + 0.15;
    let d = length(fract(g) - spot) / scale;
    let size = px * (0.55 + 1.3 * pow(hash21(cell + 5.3), 5.0));
    let core = exp(-d * d / (size * size));
    let warmth = hash21(cell + 9.1);
    let tint = mix(vec3<f32>(0.72, 0.80, 1.0), vec3<f32>(1.0, 0.86, 0.70), warmth * warmth);
    let phase = hash21(cell + 1.3) * 6.2831853;
    let flicker = 1.0 - (0.25 + 0.45 * twinkle) * (0.5 + 0.5 * sin(time * (1.3 + 3.0 * hash21(cell + 2.2)) + phase));
    let brightness = pow((h - threshold) / (1.0 - threshold), 2.0);
    return tint * core * (0.25 + 0.75 * brightness) * flicker * gain;
}

// Distance from `p` to the segment a-b.
fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return vec2<f32>(length(p - (a + ab * t)), t);
}

@fragment
fn sky(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.z;
    let time = u.view.w;
    let px = 1.0 / u.view.y; // one pixel, in sky units (the sky is 1 tall)
    let uv = in.uv;
    let p = vec2<f32>(uv.x * aspect, uv.y);
    let alt = 1.0 - uv.y; // 0 at the bottom of the window, 1 at the top
    let swell = u.mood.x;
    let shimmer = u.mood.y;
    let twinkle = u.mood.z;
    let glow = u.mood.w;

    // The sky: deep indigo overhead, easing to a dusky violet near the horizon, with a faint teal airglow band.
    var color = mix(vec3<f32>(0.056, 0.046, 0.100), vec3<f32>(0.026, 0.032, 0.082), sstep(0.0, 0.45, alt));
    color = mix(color, vec3<f32>(0.010, 0.014, 0.046), sstep(0.4, 1.05, alt));
    color += vec3<f32>(0.0, 0.018, 0.016) * exp(-pow((alt - 0.16) / 0.08, 2.0));

    // The Milky Way: a soft diagonal band with clumps and dark dust lanes; warmer at its core, violet at its edges.
    let mw_dir = normalize(vec2<f32>(1.0, -0.62));
    let mw_rel = p - vec2<f32>(0.1 * aspect, 0.95);
    let across = dot(mw_rel, vec2<f32>(-mw_dir.y, mw_dir.x));
    let along = dot(mw_rel, mw_dir);
    let band = exp(-across * across / 0.018);
    let core = exp(-across * across / 0.004);
    let clumps = fbm(vec2<f32>(along * 4.0, across * 6.0) + vec2<f32>(2.0, 5.0));
    let lanes = sstep(0.45, 0.68, fbm(vec2<f32>(along * 5.0, across * 11.0) + vec2<f32>(9.0, 1.0)));
    let mw_color = mix(vec3<f32>(0.040, 0.034, 0.072), vec3<f32>(0.072, 0.060, 0.064), core);
    color += 1.5 * mw_color * band * (0.25 + 0.95 * clumps * clumps) * (1.0 - 0.8 * lanes * sstep(0.2, 0.8, band));

    // Stars, denser along the Milky Way.
    var stars = star_layer(p, 95.0, 0.90 - 0.08 * band, px, time, twinkle, 0.34);
    stars += star_layer(p + vec2<f32>(17.0, 3.0), 42.0, 0.94 - 0.04 * band, px, time, twinkle, 0.5);
    stars += star_layer(p + vec2<f32>(5.0, 11.0), 13.0, 0.88, px * 1.3, time, twinkle, 0.85);
    color += stars * sstep(0.08, 0.22, alt);

    // A faint aurora on the far side from the moon: rayed curtains, teal at the hem fading to violet above.
    let aurora_phase = u.drift2.z;
    let hem = 0.58 + 0.09 * sin(p.x * 2.1 + aurora_phase * 0.3) + 0.04 * sin(p.x * 5.3 - aurora_phase * 0.45)
        + 0.06 * (fbm3(vec2<f32>(p.x * 1.1, aurora_phase * 0.1)) - 0.5);
    let above = hem - uv.y;
    // Fine vertical rays that shimmer sideways, over a smooth glow; the curtain is taller in some places.
    let ray_x = p.x * 70.0 + 9.0 * fbm3(vec2<f32>(p.x * 2.5, aurora_phase * 0.15));
    let rays = 0.55 + 0.45 * noise(vec2<f32>(ray_x, aurora_phase * 0.5 + uv.y * 2.0));
    let reach = 0.09 + 0.16 * fbm3(vec2<f32>(p.x * 1.7 + 4.0, aurora_phase * 0.08));
    let curtain = sstep(-0.02, 0.025, above) * exp(-max(above, 0.0) / reach * 2.2) * rays;
    let aurora_mask = (1.0 - sstep(0.3, 0.6, uv.x)) * sstep(0.0, 0.12, uv.x);
    let aurora_color = mix(vec3<f32>(0.05, 0.20, 0.15), vec3<f32>(0.11, 0.05, 0.19), sstep(0.0, 0.22, above));
    color += aurora_color * curtain * aurora_mask * (0.3 + 0.6 * shimmer);

    // A shooting star now and then (CPU picks when; usually on a drop in the music).
    let meteor_t = u.meteor.w;
    if (meteor_t > 0.0 && meteor_t < 1.0) {
        let heading = vec2<f32>(cos(u.meteor.z), sin(u.meteor.z));
        let start = vec2<f32>(u.meteor.x * aspect, u.meteor.y);
        let head = start + heading * meteor_t * 0.55;
        let tail = head - heading * 0.16 * sstep(0.0, 0.25, meteor_t);
        let hit = segment(p, tail, head);
        let streak = exp(-hit.x * hit.x / (px * px * 1.6)) * hit.y * hit.y * sin(3.14159 * meteor_t);
        color += vec3<f32>(0.75, 0.85, 1.0) * streak * 0.8;
    }

    // The moon: limb-darkened, faint maria, crisp antialiased edge, a wide soft halo that breathes with the kicks.
    let moon = vec2<f32>(0.76 * aspect, 0.27);
    let radius = 0.052;
    let to_moon = p - moon;
    let r = length(to_moon);
    let inside = 1.0 - sstep(radius - px, radius + px, r);
    let z = sqrt(max(1.0 - (r * r) / (radius * radius), 0.0));
    let maria = sstep(0.42, 0.68, fbm(to_moon / radius * 1.8 + vec2<f32>(4.0, 9.0)));
    let moon_face = vec3<f32>(0.91, 0.88, 0.79) * (0.62 + 0.38 * pow(z, 0.5)) * (1.0 - 0.28 * maria);
    color = mix(color, moon_face * 0.82, inside);
    let halo = exp(-r * 6.0) * (0.055 + 0.04 * glow + 0.02 * swell) + exp(-r * 26.0) * (0.09 + 0.05 * glow);
    color += vec3<f32>(0.50, 0.56, 0.74) * halo * (1.0 - inside);

    // Three cloud layers, back to front: high wispy cirrus, then billowing cloud lit from the moon, then a darker
    // near layer. Bass swells how much of the sky they cover. Each lit layer samples its density once more, a
    // little toward the moon: thinning toward the moon means this side faces it, so it catches the light.
    let coverage = 0.45 + 0.12 * swell;
    let moonward = normalize(moon - p + vec2<f32>(1e-4, 0.0));
    let near_moon = exp(-r * 3.2);
    var veil = 0.0; // how much thin cloud lies over this pixel, for the corona

    let cirrus = sstep(0.52, 0.85, fbm(vec2<f32>(p.x * 1.2, p.y * 5.5) + u.drift2.xy)) * sstep(0.25, 0.6, alt);
    let cirrus_color = mix(vec3<f32>(0.07, 0.075, 0.12), vec3<f32>(0.34, 0.37, 0.46), near_moon);
    color = mix(color, cirrus_color, cirrus * 0.35);
    veil += cirrus * 0.35;

    // `scale` sets how big the billows look; the layers drift at different speeds (parallax).
    for (var layer = 0; layer < 2; layer++) {
        let near = layer == 1;
        let scale = select(2.6, 1.6, near);
        let offset = select(u.drift.xy, u.drift.zw + vec2<f32>(40.0, 0.0), near);
        let q = p * scale + offset;
        let cover = select(coverage - 0.04, coverage - 0.02, near) - 0.10 * sstep(0.5, 1.0, alt);
        let density = clouds(q, cover);
        if (density > 0.002) {
            let toward = clouds(q + moonward * 0.07 * scale, cover);
            let lit = clamp(0.45 + (density - toward) * 5.0, 0.0, 1.0);
            let thick = density * density;
            // Undersides: deep indigo with a hint of violet. Moon-facing sides: cool silver, brightest near the moon.
            let shadow = select(vec3<f32>(0.034, 0.027, 0.074), vec3<f32>(0.019, 0.015, 0.046), near);
            let reach = exp(-r * 1.8);
            let moonlit = mix(vec3<f32>(0.15, 0.17, 0.29), vec3<f32>(0.45, 0.42, 0.41), near_moon) * (0.35 + 0.9 * reach) * (0.85 + 0.3 * shimmer);
            var cloud = mix(shadow, moonlit, lit * lit * (1.0 - 0.5 * thick));
            // Bright rims on the edges that face the moon.
            cloud += vec3<f32>(0.20, 0.21, 0.27) * density * (1.0 - density) * 4.0 * lit * reach;
            // A silver lining where thin edges pass in front of the moon's glow.
            let edge = density * (1.0 - density) * 4.0;
            cloud += vec3<f32>(0.42, 0.46, 0.58) * edge * exp(-r * 9.0) * (0.6 + 0.4 * glow);
            // Low clouds pick up a little of the horizon's violet and teal.
            cloud += vec3<f32>(0.012, 0.010, 0.022) * sstep(0.5, 0.0, alt);
            let opacity = density * select(0.82, 0.94, near);
            color = mix(color, cloud, opacity);
            veil += opacity * (1.0 - density);
        }
    }

    // The corona: where a thin veil of cloud covers the moon, light diffracts into faint coloured rings
    // (bluish-white aureole, then a brownish-red ring, then faint green and violet).
    let ring = (r - radius) / (radius * 2.2);
    if (ring > -0.2 && ring < 2.2) {
        let hues = 0.5 + 0.5 * cos(6.2831853 * (ring * 1.15 + vec3<f32>(0.0, 0.33, 0.67)));
        let aureole = exp(-max(ring, 0.0) * 2.2);
        let thin = clamp(veil * 2.0, 0.0, 1.0) * (1.0 - clamp(veil - 0.6, 0.0, 1.0));
        color += mix(vec3<f32>(0.75, 0.80, 0.95), hues, 0.55) * aureole * thin * 0.10 * (1.0 - inside * 0.5);
    }

    // A distant ridge with a few pines along the bottom, its crest faintly rimmed by the horizon's glow.
    let ridge = 0.075 + 0.035 * fbm3(vec2<f32>(p.x * 1.6, 3.0)) + 0.008 * noise(vec2<f32>(p.x * 14.0, 1.0));
    let slot = floor(p.x * 90.0);
    let local = fract(p.x * 90.0);
    let pine = select(0.0, (0.012 + 0.022 * hash21(vec2<f32>(slot, 4.0))) * (1.0 - abs(local - 0.5) * 2.0), hash21(vec2<f32>(slot, 2.0)) > 0.72);
    let land = ridge + pine * sstep(0.3, 0.7, noise(vec2<f32>(p.x * 2.2, 8.0)));
    let ground = 1.0 - sstep(land - px, land + px, alt);
    let rim = exp(-abs(alt - land) / (px * 2.0)) * 0.6;
    color = mix(color, vec3<f32>(0.006, 0.007, 0.016), ground);
    color += vec3<f32>(0.020, 0.024, 0.045) * rim * (1.0 - ground * 0.5);

    // A gentle vignette, then a whisper of dither so the dark gradients never band.
    let edge_distance = length((uv - 0.5) * vec2<f32>(aspect, 1.0));
    color *= mix(0.70, 1.0, 1.0 - sstep(0.35, 1.15, edge_distance));
    color += (hash21(in.position.xy + fract(time * 7.0) * 61.0) - 0.5) / 255.0;

    // Optional band meters, bottom left, as in the other scenes (kept dim to suit the scene).
    if (u.meters.w > 0.5) {
        let screen = vec2<f32>(uv.x, 1.0 - uv.y) * u.view.xy;
        let unit = max(1.0, u.view.y / 500.0);
        for (var i = 0; i < 3; i++) {
            let x0 = (16.0 + f32(i) * 16.0) * unit;
            let y0 = 16.0 * unit;
            let h = 110.0 * unit;
            if (screen.x >= x0 && screen.x < x0 + 11.0 * unit && screen.y >= y0 && screen.y < y0 + h) {
                let fill = (screen.y - y0) / h;
                color = mix(color, select(vec3<f32>(0.04), vec3<f32>(0.35, 0.42, 0.60), fill < u.meters[i]), 0.85);
            }
        }
    }
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(0.85)), 1.0);
}
