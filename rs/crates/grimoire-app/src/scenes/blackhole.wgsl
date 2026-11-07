// Black Hole: an endless fall into a black hole, traced with real light bending.
//
// Each pixel's light ray is followed backwards through Schwarzschild spacetime (units where the event horizon has
// radius 1): the ray bends by a' = -3/2 h² x / r⁵, which reproduces the black hole's shadow, the photon ring and
// the Einstein ring of the sky behind it. The accretion disk orbits at Keplerian speed (inner rings faster than
// outer, so its patterns wind up), and its light is Doppler beamed and gravitationally redshifted: the side
// swinging towards us is brighter and shifted along the palette, the inner edge dimmed by gravity.
//
// The ride never ends: the camera falls from far out to just above the horizon over one "cycle". Rays that fall
// into the hole aren't black; they come out in the next cycle's universe, seen from its starting point. As the
// shadow grows to fill the view, that next universe fills it, and at the end of the cycle the frame is exactly
// the next cycle's first frame. Then it starts again.

struct Uniforms {
    view: vec4<f32>,   // render width px, height px, aspect, time
    ride: vec4<f32>,   // cycle, phase (0...1), hue drift, disk clock
    mood: vec4<f32>,   // low, mid, high, kick pulse (1 on a kick, fading)
    look: vec4<f32>,   // gaze sway x, y, unused, unused
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

const TAU: f32 = 6.2831853;

fn sstep(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn h1(n: f32) -> f32 {
    return fract(sin(n * 127.1 + 311.7) * 43758.5453);
}

fn hash21(q: vec2<f32>) -> f32 {
    var p = fract(q * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fn hash31(q: vec3<f32>) -> f32 {
    var p = fract(q * vec3<f32>(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yzx + 33.33);
    return fract((p.x + p.y) * p.z);
}

fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), w.x),
               mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), w.x), w.y);
}

fn fbm2(start: vec2<f32>) -> f32 {
    var p = start;
    var value = 0.0;
    var amplitude = 0.5;
    for (var i = 0; i < 4; i++) {
        value += amplitude * noise2(p);
        p = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * p + vec2<f32>(7.1, 2.3);
        amplitude *= 0.5;
    }
    return value / 0.9375;
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash31(i), hash31(i + vec3<f32>(1.0, 0.0, 0.0)), w.x),
                mix(hash31(i + vec3<f32>(0.0, 1.0, 0.0)), hash31(i + vec3<f32>(1.0, 1.0, 0.0)), w.x), w.y);
    let b = mix(mix(hash31(i + vec3<f32>(0.0, 0.0, 1.0)), hash31(i + vec3<f32>(1.0, 0.0, 1.0)), w.x),
                mix(hash31(i + vec3<f32>(0.0, 1.0, 1.0)), hash31(i + vec3<f32>(1.0, 1.0, 1.0)), w.x), w.y);
    return mix(a, b, w.z);
}

fn fbm3(start: vec3<f32>) -> f32 {
    var p = start;
    var value = 0.0;
    var amplitude = 0.5;
    for (var i = 0; i < 4; i++) {
        value += amplitude * noise3(p);
        p = p * 2.03 + vec3<f32>(5.1, 1.7, 9.3);
        amplitude *= 0.5;
    }
    return value / 0.9375;
}

// A psychedelic rainbow, softened a little so it stays rich rather than garish.
fn palette(t: f32) -> vec3<f32> {
    let c = 0.5 + 0.5 * cos(TAU * (t + vec3<f32>(0.0, 0.33, 0.67)));
    return mix(c, vec3<f32>(0.6), 0.15);
}

struct Camera {
    pos: vec3<f32>,
    fwd: vec3<f32>,
    right: vec3<f32>,
    up: vec3<f32>,
    cycle: f32,
    hue: f32,
    outer: f32,
};

// Where the camera is `phase` of the way through `cycle`'s fall: spiralling in from 26 horizon radii to just
// above the horizon, slightly above the disk (each cycle at its own angle), slowly rolling.
fn camera(cycle: f32, phase: f32) -> Camera {
    let elevation = 0.08 + 0.30 * h1(cycle + 1.0);
    let azimuth = cycle * 2.4 + phase * 4.2;
    let distance = exp(mix(log(26.0), log(1.05), phase));
    let pos = distance * vec3<f32>(cos(elevation) * cos(azimuth), sin(elevation), cos(elevation) * sin(azimuth));
    let ahead = normalize(-pos);
    let side = normalize(cross(ahead, vec3<f32>(0.0, 1.0, 0.0)));
    let lift = cross(side, ahead);
    let fwd = normalize(ahead + side * u.look.x + lift * u.look.y);
    let right0 = normalize(cross(fwd, vec3<f32>(0.0, 1.0, 0.0)));
    let up0 = cross(right0, fwd);
    let roll = phase * 1.6 + cycle * 0.7;
    var cam: Camera;
    cam.pos = pos;
    cam.fwd = fwd;
    cam.right = cos(roll) * right0 + sin(roll) * up0;
    cam.up = -sin(roll) * right0 + cos(roll) * up0;
    cam.cycle = cycle;
    cam.hue = h1(cycle + 7.0);
    cam.outer = 10.0 + 6.0 * h1(cycle + 3.0);
    return cam;
}

// The sky behind the hole: a nebula in shifting colours, and stars.
fn background(d: vec3<f32>, cam: Camera) -> vec3<f32> {
    let hue = cam.hue + u.ride.z;
    let n = fbm3(d * 2.2 + vec3<f32>(cam.cycle * 3.7));
    let wisps = fbm3(d * 5.5 + vec3<f32>(11.0, cam.cycle, 2.0));
    var color = palette(hue + 0.6 * n + 0.4) * pow(n, 3.0) * 0.9;
    color += palette(hue + 0.3 + wisps * 0.7) * pow(wisps, 5.0) * 0.8;
    let g = d * 70.0;
    let cell = floor(g);
    let h = hash31(cell + vec3<f32>(cam.cycle));
    if (h > 0.975) {
        let spot = cell + 0.5 + (vec3<f32>(hash31(cell + 1.3), hash31(cell + 4.1), hash31(cell + 8.7)) - 0.5) * 0.6;
        let dist = length(g - spot);
        let twinkle = 0.7 + 0.3 * sin(u.view.w * 3.0 + h * 50.0) * u.mood.z;
        color += mix(vec3<f32>(0.8, 0.85, 1.0), palette(h * 9.0 + hue), 0.35) * exp(-dist * dist * 14.0) * (h - 0.975) * 60.0 * twinkle;
    }
    return color;
}

// Light from the accretion disk where a ray crosses it at `hit` (radius `rr`), travelling along `ray`.
fn disk(hit: vec3<f32>, rr: f32, ray: vec3<f32>, cam: Camera) -> vec4<f32> {
    let low = u.mood.x;
    let kick = u.mood.w;
    // Keplerian rotation: angular speed ∝ r^-3/2, so the texture shears into spirals as it turns.
    let spin = u.ride.w * pow(rr, -1.5) * 4.0;
    let c = cos(spin);
    let s = sin(spin);
    let flat = vec2<f32>(c * hit.x - s * hit.z, s * hit.x + c * hit.z);
    let tex = fbm2(flat * 0.75 + vec2<f32>(cam.cycle * 5.3, 0.0));
    let streak = fbm2(vec2<f32>(rr * 2.2, 0.0) + flat * 0.25);
    let rings = 0.55 + 0.45 * sin(rr * 4.0 + tex * 4.0);
    let density = sstep(2.5, 3.1, rr) * (1.0 - sstep(cam.outer * 0.55, cam.outer, rr)) * (0.3 + 0.9 * tex * rings);

    // Doppler beaming and gravitational redshift. Orbital speed seen by a static observer: √(M / (r - 2M)).
    let tangent = normalize(vec3<f32>(-hit.z, 0.0, hit.x));
    let v = min(sqrt(0.5 / max(rr - 1.0, 0.2)), 0.75);
    let gamma = 1.0 / sqrt(1.0 - v * v);
    let g = sqrt(max(1.0 - 1.0 / rr, 0.0)) / (gamma * (1.0 - v * dot(tangent, -ray)));
    let heat = pow(3.0 / rr, 0.9);
    // A ripple of light runs outward through the disk on each kick.
    let ripple_r = 2.8 + (1.0 - kick) * 12.0;
    let ripple = kick * exp(-pow(rr - ripple_r, 2.0) * 1.2) * 1.6;
    let bright = heat * pow(g, 3.0) * (0.75 + 0.9 * low + ripple) * (0.6 + 0.8 * streak);
    let hue = cam.hue + u.ride.z + 0.4 * log(rr) + 0.25 * tex + 0.3 * (g - 1.0);
    let color = mix(palette(hue), vec3<f32>(1.0, 0.95, 0.88), clamp(bright * 0.3 - 0.15, 0.0, 0.75)) * bright * 1.5;
    return vec4<f32>(color, clamp(density, 0.0, 0.95));
}

// Follows one ray backwards from the camera. Returns its colour and how much of it fell through the horizon.
fn trace(cam: Camera, dir: vec3<f32>, max_steps: i32) -> vec4<f32> {
    var pos = cam.pos;
    var vel = dir;
    let l = cross(pos, vel);
    let h2 = dot(l, l);
    var color = vec3<f32>(0.0);
    var alpha = 0.0;
    var ring = 0.0;
    var fate = 0; // 0 still going, 1 fell in, 2 escaped
    for (var i = 0; i < max_steps; i++) {
        let r = length(pos);
        let dt = clamp(0.07 * r, 0.035, 2.0);
        vel += -1.5 * h2 * pos / pow(r, 5.0) * dt;
        let previous = pos;
        pos += vel * dt;
        let rn = length(pos);
        // Light that skims the photon sphere (r = 1.5) piles up into the thin bright photon ring.
        ring += exp(-pow((rn - 1.5) * 4.5, 2.0)) * dt;
        if (previous.y * pos.y < 0.0) {
            let t = previous.y / (previous.y - pos.y);
            let hit = mix(previous, pos, t);
            let rr = length(hit.xz);
            if (rr > 2.4 && rr < cam.outer) {
                let d = disk(hit, rr, normalize(vel), cam);
                color += (1.0 - alpha) * d.rgb * d.a;
                alpha += (1.0 - alpha) * d.a;
            }
        }
        if (rn < 1.0) {
            fate = 1;
            break;
        }
        if (rn > 40.0 && dot(pos, vel) > 0.0) {
            fate = 2;
            break;
        }
        if (alpha > 0.97) {
            break;
        }
    }
    if (fate == 2) {
        color += (1.0 - alpha) * background(normalize(vel), cam);
    }
    let fell = select(0.0, 1.0, fate == 1 || (fate == 0 && alpha < 0.97));
    // Only light that got away shows the photon ring: rays that skimmed the photon sphere and escaped.
    let hue = cam.hue + u.ride.z + 0.15;
    color += mix(palette(hue), vec3<f32>(1.0), 0.4) * ring * (0.5 + 0.6 * u.mood.x) * (1.0 - alpha) * (1.0 - fell);
    return vec4<f32>(color, fell * (1.0 - alpha));
}

@fragment
fn trace_frame(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = u.view.z;
    let cycle = u.ride.x;
    let phase = u.ride.y;
    let screen = vec2<f32>((in.uv.x - 0.5) * aspect, 0.5 - in.uv.y) * 2.0;
    let fov = 0.78; // tan of half the vertical field of view

    let cam = camera(cycle, phase);
    let dir = normalize(cam.fwd + (screen.x * cam.right + screen.y * cam.up) * fov);
    let outer = trace(cam, dir, 150);

    // What lies through the horizon: the next universe. Needed where rays fell in (once the hole is big enough
    // to see into), and everywhere during the last stretch, when the whole frame crossfades into it.
    let through = sstep(0.5, 0.88, phase);
    let handover = sstep(0.93, 1.0, phase);
    var color = outer.rgb;
    if ((outer.a > 0.0 && through > 0.0) || handover > 0.0) {
        let next = camera(cycle + 1.0, 0.0);
        let next_dir = normalize(next.fwd + (screen.x * next.right + screen.y * next.up) * fov);
        let inner = trace(next, next_dir, 110);
        color += inner.rgb * outer.a * through;
        color = mix(color, inner.rgb, handover);
    }
    return vec4<f32>(color, 1.0);
}
