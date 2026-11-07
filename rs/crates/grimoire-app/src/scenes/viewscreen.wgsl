// WGSL port of ViewscreenShaders.swift (Metal). One render pass, drawn in this order:
// - `nebula_fragment`: deep-space gradient plus drifting noise clouds (the mids).
// - `star_vertex`/`star_fragment` for the far star layers: instanced quads stretched into streaks by speed.
// - `body_fragment`: a planet, moon or asteroid field drifting past (only while one is on screen).
// - the near star layers, so close stars pass in front of the body.
// - `overlay_fragment`: shooting stars, the glass glint and the HUD (bezel, reticle, heading tape, throttle).
//
// Screen space throughout: y spans -1...1 (up), x spans -aspect...aspect.

struct Uniforms {
    view: vec4<f32>,         // width px, height px, aspect, time
    camera: vec4<f32>,       // roll, sway x, sway y, streak length (world units)
    motion: vec4<f32>,       // throttle 0...1, heading degrees, glint, unused
    bands: vec4<f32>,        // low, mid, high, sustained low
    hits: vec4<f32>,         // low, mid, high, show meters
    nebula: vec4<f32>,       // drift x, drift y, hue shift, unused
    body: vec4<f32>,         // center xy (screen, before camera), radius, kind (0 none, 1 planet, 2 moon, 3 asteroids)
    body_look: vec4<f32>,    // has ring, seed, light angle, unused
    body_color_a: vec4<f32>,
    body_color_b: vec4<f32>,
};

struct Palette {
    space: vec4<f32>,
    space_edge: vec4<f32>,
    star: vec4<f32>,
    nebula_a: vec4<f32>,
    nebula_b: vec4<f32>,
    nebula_c: vec4<f32>,
    hud: vec4<f32>,
    atmosphere: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<uniform> pal: Palette;
@group(0) @binding(2) var<uniform> rocks: array<vec4<f32>, 16>;
@group(0) @binding(3) var<uniform> meteors: array<vec4<f32>, 6>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct StarOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,   // along (0 tail ... 1 head), side (-1 ... 1)
    @location(1) color: vec3<f32>,
    @location(2) alpha: f32,
    @location(3) streak: f32,        // 0 = dot, 1 = long streak
};

// smoothstep written out, so reversed edges (a > b) behave the same on every backend, as in Metal.
fn sstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn rotate2(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

fn to_screen(uv: vec2<f32>, aspect: f32) -> vec2<f32> {
    return (uv - 0.5) * vec2<f32>(2.0 * aspect, -2.0);
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
        p = p * 2.03 + vec2<f32>(17.1, 9.2);
        amplitude *= 0.5;
    }
    return value;
}

fn hue_shift(c: vec3<f32>, angle: f32) -> vec3<f32> {
    let k = vec3<f32>(0.57735);
    let ca = cos(angle);
    return c * ca + cross(k, c) * sin(angle) + k * dot(k, c) * (1.0 - ca);
}

// Premultiplied "over".
fn over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
    return top + bottom * (1.0 - top.a);
}

fn segment_distance(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = saturate(dot(pa, ba) / dot(ba, ba));
    return length(pa - ba * h);
}

// Seven-segment digit in a box x 0...1, y 0...2.
fn digit_distance(p: vec2<f32>, digit: i32) -> f32 {
    var m = 0x7F;
    switch digit {
        case 0: { m = 0x3F; }
        case 1: { m = 0x06; }
        case 2: { m = 0x5B; }
        case 3: { m = 0x4F; }
        case 4: { m = 0x66; }
        case 5: { m = 0x6D; }
        case 6: { m = 0x7D; }
        case 7: { m = 0x07; }
        case 8: { m = 0x7F; }
        case 9: { m = 0x6F; }
        default: {}
    }
    var d = 1e5;
    if ((m & 0x01) != 0) { d = min(d, segment_distance(p, vec2<f32>(0.15, 2.0), vec2<f32>(0.85, 2.0))); }
    if ((m & 0x02) != 0) { d = min(d, segment_distance(p, vec2<f32>(1.0, 1.85), vec2<f32>(1.0, 1.15))); }
    if ((m & 0x04) != 0) { d = min(d, segment_distance(p, vec2<f32>(1.0, 0.85), vec2<f32>(1.0, 0.15))); }
    if ((m & 0x08) != 0) { d = min(d, segment_distance(p, vec2<f32>(0.15, 0.0), vec2<f32>(0.85, 0.0))); }
    if ((m & 0x10) != 0) { d = min(d, segment_distance(p, vec2<f32>(0.0, 0.85), vec2<f32>(0.0, 0.15))); }
    if ((m & 0x20) != 0) { d = min(d, segment_distance(p, vec2<f32>(0.0, 1.85), vec2<f32>(0.0, 1.15))); }
    if ((m & 0x40) != 0) { d = min(d, segment_distance(p, vec2<f32>(0.15, 1.0), vec2<f32>(0.85, 1.0))); }
    return d;
}

// Distance (screen units) to a zero-padded number with its bottom-left at `origin`.
fn number_distance(p: vec2<f32>, origin: vec2<f32>, value: i32, count: i32, height: f32) -> f32 {
    let unit = height * 0.5;
    let q = (p - origin) / unit;
    let advance = 1.7;
    if (q.y < -0.5 || q.y > 2.5 || q.x < -0.5 || q.x > advance * f32(count)) {
        return 1e5;
    }
    let index = clamp(i32(floor(q.x / advance)), 0, count - 1);
    var divisor = 1;
    for (var i = 0; i < count - 1 - index; i++) {
        divisor *= 10;
    }
    let digit = (value / divisor) % 10;
    return digit_distance(vec2<f32>(q.x - f32(index) * advance, q.y), digit) * unit;
}

@vertex
fn fullscreen(@builtin(vertex_index) id: u32) -> VertexOut {
    let corner = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

// Nebula

@fragment
fn nebula_fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.z;
    let p = to_screen(in.uv, aspect);
    let view = rotate2(p - u.camera.yz * 0.3, -u.camera.x * 0.5);
    let mid = u.bands.y;

    let q = view * 0.8 + u.nebula.xy;
    let n1 = fbm(q);
    let n2 = fbm(q * 1.7 + vec2<f32>(n1 * 1.5, -n1) + u.nebula.xy * 0.5);
    let cloud = sstep(0.38, 0.85, n1 * 0.55 + n2 * 0.55);

    var color = mix(pal.nebula_a.rgb, pal.nebula_b.rgb, sstep(0.3, 0.75, n2));
    color = mix(color, pal.nebula_c.rgb, sstep(0.45, 0.9, n1));
    color = hue_shift(color, u.nebula.z);
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    color = mix(vec3<f32>(luminance), color, 0.3 + 0.7 * mid);
    let opacity = 0.10 + 0.55 * mid + 0.12 * u.hits.y;

    let radius = length(p / vec2<f32>(aspect, 1.0));
    let space = mix(pal.space.rgb, pal.space_edge.rgb, sstep(0.2, 1.3, radius));
    return vec4<f32>(space + color * cloud * opacity, 1.0);
}

// Stars

@vertex
fn star_vertex(
    @builtin(vertex_index) vid: u32,
    @location(0) position: vec4<f32>, // xyz world, layer speed multiplier
    @location(1) look: vec4<f32>,     // size px, twinkle, tint, brightness
) -> StarOut {
    let aspect = u.view.z;
    let pixel = 2.0 / u.view.y;
    let z = max(position.z, 0.3);
    let head = position.xy / z;
    let tail = position.xy / (z + u.camera.w * position.w);
    let twinkle = look.y;
    let width = look.x * (1.0 + 1.2 * twinkle) * clamp(6.0 / z, 0.8, 2.2) * pixel * 0.5;

    let direction = head - tail;
    let len = length(direction);
    var axis = vec2<f32>(1.0, 0.0);
    if (len > 1e-6) {
        axis = direction / len;
    }
    let normal = vec2<f32>(-axis.y, axis.x);
    let along = f32(vid & 1u);
    let side = select(-1.0, 1.0, (vid & 2u) != 0u);
    var point = mix(tail - axis * width, head + axis * width, along) + normal * side * width;
    point = rotate2(point, u.camera.x) + u.camera.yz;

    var out: StarOut;
    out.position = vec4<f32>(point.x / aspect, point.y, 0.0, 1.0);
    out.local = vec2<f32>(along, side);
    out.color = mix(pal.star.rgb, vec3<f32>(1.0), look.z) * (1.0 + 1.8 * twinkle);
    let streak_pixels = len / pixel;
    out.streak = saturate(streak_pixels / 8.0);
    out.alpha = look.w * mix(1.0, 0.75, out.streak);
    return out;
}

@fragment
fn star_fragment(in: StarOut) -> @location(0) vec4<f32> {
    let across = 1.0 - abs(in.local.y);
    let along_fade = mix(1.0, mix(0.08, 1.0, in.local.x * in.local.x), in.streak);
    let a = sstep(0.0, 0.9, across) * along_fade * in.alpha;
    return vec4<f32>(in.color * a, a);
}

// Passing bodies

@fragment
fn body_fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.z;
    let pixel = 2.0 / u.view.y;
    let p = rotate2(to_screen(in.uv, aspect) - u.camera.yz, -u.camera.x);
    let kind = u.body.w;
    let light = normalize(vec3<f32>(cos(u.body_look.z), sin(u.body_look.z), 0.55));

    if (kind > 2.5) {
        // Asteroid field: lumpy, lit rocks, sorted far to near on the CPU.
        var result = vec4<f32>(0.0);
        for (var i = 0; i < 16; i++) {
            let rock = rocks[i];
            if (rock.z <= 0.0) {
                continue;
            }
            let d = (p - rock.xy) / rock.z;
            if (dot(d, d) > 2.0) {
                continue;
            }
            let angle = atan2(d.y, d.x);
            let lumps = 1.0 + 0.18 * sin(angle * 3.0 + rock.w * 7.0) + 0.1 * sin(angle * 5.0 + rock.w * 13.0);
            let dist = length(d) / lumps;
            if (dist >= 1.0) {
                continue;
            }
            let n = normalize(vec3<f32>(d / lumps, sqrt(max(0.0, 1.0 - dist * dist))));
            let shade = max(0.0, dot(n, light));
            let grain = noise(d * 5.0 + rock.w * 31.0);
            let c = mix(u.body_color_a.rgb, u.body_color_b.rgb, grain) * (0.06 + 0.94 * shade);
            let edge = sstep(1.0, 1.0 - 2.0 * pixel / rock.z, dist);
            result = over(vec4<f32>(c * edge, edge), result);
        }
        return result;
    }

    let d = (p - u.body.xy) / u.body.z;
    let r = length(d);

    // Ring: an ellipse in a tilted plane. The half above the center passes behind the planet.
    var ring_back = vec4<f32>(0.0);
    var ring_front = vec4<f32>(0.0);
    if (u.body_look.x > 0.5) {
        let rd = rotate2(d, 0.35);
        let rr = length(vec2<f32>(rd.x, rd.y / 0.28));
        if (rr > 1.3 && rr < 2.35) {
            let bands = 0.5 + 0.5 * sin(rr * 38.0 + u.body_look.y * 10.0);
            let a = sstep(1.3, 1.42, rr) * sstep(2.35, 2.2, rr) * (0.3 + 0.45 * bands) * 0.85;
            let ring = vec4<f32>(mix(u.body_color_b.rgb, vec3<f32>(0.9), 0.3) * a * 0.75, a);
            if (rd.y > 0.0) {
                ring_back = ring;
            } else {
                ring_front = ring;
            }
        }
    }

    var sphere = vec4<f32>(0.0);
    if (r < 1.0) {
        let z = sqrt(max(0.0, 1.0 - r * r));
        let n = vec3<f32>(d, z);
        let shade = max(0.0, dot(n, light));
        var surface: vec3<f32>;
        if (kind < 1.5) {
            let latitude = d.y + 0.12 * fbm(d * 3.0 + u.body_look.y);
            let b = fbm(vec2<f32>(latitude * 7.0, d.x * 0.7 + u.body_look.y * 3.0));
            surface = mix(u.body_color_a.rgb, u.body_color_b.rgb, sstep(0.3, 0.7, b));
        } else {
            let c = fbm(d * 4.0 + u.body_look.y * 9.0);
            let pits = sstep(0.6, 0.8, noise(d * 10.0 + u.body_look.y * 4.0));
            surface = mix(u.body_color_a.rgb, u.body_color_b.rgb, c) * (1.0 - 0.35 * pits);
        }
        var color = surface * (0.03 + 0.97 * shade);
        let rim = pow(1.0 - z, 3.0) * select(0.15, 0.9, kind < 1.5);
        color += pal.atmosphere.rgb * rim * (0.25 + shade);
        let edge = sstep(1.0, 1.0 - 1.5 * pixel / u.body.z, r);
        sphere = vec4<f32>(color * edge, edge);
    }

    // Thin atmospheric halo just outside a planet's limb.
    var halo = vec4<f32>(0.0);
    if (kind < 1.5 && r >= 1.0) {
        let h = exp(-(r - 1.0) * 22.0) * 0.4;
        halo = vec4<f32>(pal.atmosphere.rgb * h, h * 0.5);
    }

    return over(ring_front, over(sphere, over(halo, ring_back)));
}

// Overlay (meteors, glint, HUD)

fn line_alpha(distance: f32, thickness: f32, pixel: f32) -> f32 {
    return sstep(thickness + pixel, thickness, distance);
}

@fragment
fn overlay_fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.z;
    let pixel = 2.0 / u.view.y;
    let p = to_screen(in.uv, aspect);
    var light = vec3<f32>(0.0);

    // Shooting stars: outside the ship, so they move with the camera.
    for (var i = 0; i < 3; i++) {
        let head = meteors[i * 2];
        let tail = meteors[i * 2 + 1];
        if (head.z <= 0.0) {
            continue;
        }
        let h = rotate2(head.xy, u.camera.x) + u.camera.yz;
        let t = rotate2(tail.xy, u.camera.x) + u.camera.yz;
        let pa = p - t;
        let ba = h - t;
        let k = saturate(dot(pa, ba) / max(dot(ba, ba), 1e-8));
        let d = length(pa - ba * k);
        let core = exp(-(d * d) / (pixel * pixel * 2.5)) * k * k;
        light += mix(pal.star.rgb, vec3<f32>(1.0), 0.5) * core * head.z * 1.6;
    }

    // A faint glint sweeping across the glass.
    let glint = u.motion.z;
    if (glint > 0.001) {
        let band = dot(p, normalize(vec2<f32>(1.0, 0.55))) - mix(-1.8, 1.8, 1.0 - glint);
        light += vec3<f32>(0.7, 0.85, 1.0) * exp(-band * band * 25.0) * glint * 0.06;
    }

    var hud = vec4<f32>(0.0);
    let hc = vec4<f32>(pal.hud.rgb, 1.0);

    // Bezel: a dark frame around the glass, a thin inner line and corner brackets.
    let inset = 0.05;
    let to_edge = vec2<f32>(aspect, 1.0) - abs(p);
    let edge_distance = min(to_edge.x, to_edge.y);
    let frame = sstep(inset + pixel, inset - pixel, edge_distance);
    hud = over(vec4<f32>(pal.space_edge.rgb * 0.5, 1.0) * frame * 0.85, hud);
    var rim = line_alpha(abs(edge_distance - inset), 0.5 * pixel, pixel) * 0.3;
    let corner = abs(p) - (vec2<f32>(aspect, 1.0) - inset);
    if (corner.x > -0.12 && corner.y > -0.12) {
        rim = max(rim, line_alpha(abs(edge_distance - inset), 1.2 * pixel, pixel) * 0.55);
    }
    hud = over(hc * rim, hud);

    // Reticle: a broken ring and four short ticks.
    let r = length(p);
    let angle = atan2(p.y, p.x);
    let ring = line_alpha(abs(r - 0.05), 0.5 * pixel, pixel) * step(0.38, abs(sin(angle * 2.0 + 0.785)));
    var ticks = 0.0;
    if (abs(p.y) < 2.0 * pixel && abs(p.x) > 0.075 && abs(p.x) < 0.12) { ticks = 1.0; }
    if (abs(p.x) < 2.0 * pixel && abs(p.y) > 0.075 && abs(p.y) < 0.12) { ticks = 1.0; }
    let reticle = max(ring, ticks * line_alpha(min(abs(p.x), abs(p.y)), 0.5 * pixel, pixel));
    hud = over(hc * reticle * 0.28, hud);

    // Heading tape along the top, with the heading number above it.
    let heading = u.motion.y;
    let tape_y = 1.0 - inset - 0.09;
    let tape_half = 0.42;
    if (abs(p.x) < tape_half && p.y < tape_y + 0.004 && p.y > tape_y - 0.04) {
        let degrees_per_unit = 70.0;
        let h = heading + p.x * degrees_per_unit;
        let step5 = h / 5.0;
        let offset = abs(fract(step5 + 0.5) - 0.5) * 5.0 / degrees_per_unit;
        let major = i32(round(step5)) % 3 == 0;
        let tick_length = select(0.014, 0.03, major);
        let tick = line_alpha(offset, 0.5 * pixel, pixel) * step(tape_y - tick_length, p.y);
        let fade = sstep(tape_half, tape_half * 0.6, abs(p.x));
        hud = over(hc * tick * fade * 0.45, hud);
    }
    let baseline = line_alpha(abs(p.y - tape_y), 0.4 * pixel, pixel) * sstep(tape_half, tape_half * 0.6, abs(p.x));
    hud = over(hc * baseline * 0.25, hud);
    let caret = line_alpha(abs(p.x), 0.6 * pixel, pixel) * step(tape_y - 0.055, p.y) * step(p.y, tape_y - 0.04);
    hud = over(hc * caret * 0.6, hud);
    let digit_height = 0.036;
    let heading_digits = number_distance(p, vec2<f32>(-1.5 * 1.7 * digit_height * 0.5, tape_y + 0.016),
                                         i32(heading) % 360, 3, digit_height);
    hud = over(hc * line_alpha(heading_digits, digit_height * 0.05, pixel) * 0.6, hud);

    // Throttle ladder on the right, with a percentage readout below it.
    let ladder_x = aspect - inset - 0.06;
    let ladder_low = -0.32;
    let ladder_high = 0.32;
    let throttle = saturate(u.motion.x);
    if (p.y > ladder_low && p.y < ladder_high) {
        let t = (p.y - ladder_low) / (ladder_high - ladder_low);
        let rung = abs(fract(t * 10.0 + 0.5) - 0.5) / 10.0 * (ladder_high - ladder_low);
        let rungs = line_alpha(rung, 0.4 * pixel, pixel) * step(abs(p.x - ladder_x), 0.012);
        hud = over(hc * rungs * 0.25, hud);
        let fill = step(t, throttle) * step(abs(p.x - ladder_x - 0.022), 0.004);
        hud = over(hc * fill * 0.5, hud);
    }
    let throttle_digits = number_distance(p, vec2<f32>(ladder_x - 0.04, ladder_low - 0.07),
                                          i32(round(throttle * 100.0)), 3, 0.03);
    hud = over(hc * line_alpha(throttle_digits, 0.0015, pixel) * 0.5, hud);

    // Optional band meters, bottom left, as in the other scenes.
    if (u.hits.w > 0.5) {
        let px = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * u.view.xy;
        let unit = max(1.0, u.view.y / 500.0);
        for (var i = 0; i < 3; i++) {
            let x0 = (36.0 + f32(i) * 16.0) * unit;
            let y0 = 36.0 * unit;
            let height = 110.0 * unit;
            if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + height) {
                let fill = (px.y - y0) / height;
                var meter = vec3<f32>(0.1);
                if (fill < u.bands[i]) {
                    meter = mix(pal.nebula_a.rgb, pal.nebula_b.rgb, f32(i) * 0.5) * 1.6;
                }
                if (u.hits[i] > 0.6 && fill > 0.94) {
                    meter = vec3<f32>(1.0);
                }
                hud = over(vec4<f32>(meter * 0.85, 0.85), hud);
            }
        }
    }

    return vec4<f32>(light, 0.0) + hud;
}
