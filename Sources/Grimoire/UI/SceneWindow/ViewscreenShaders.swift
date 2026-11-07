/// Metal source for `ViewscreenScene`, compiled at runtime (SwiftPM doesn't build .metal files).
///
/// One render pass, drawn in this order:
/// - `nebulaFragment`: deep-space gradient plus drifting noise clouds (the mids).
/// - `starVertex`/`starFragment` for the far star layers: instanced quads stretched into streaks by speed.
/// - `bodyFragment`: a planet, moon or asteroid field drifting past (only while one is on screen).
/// - the near star layers, so close stars pass in front of the body.
/// - `overlayFragment`: shooting stars, the glass glint and the HUD (bezel, reticle, heading tape, throttle).
///
/// Screen space throughout: y spans -1...1 (up), x spans -aspect...aspect.
enum ViewscreenShaders {
    static let source = """
    #include <metal_stdlib>
    using namespace metal;

    struct Uniforms {
        float4 view;       // width px, height px, aspect, time
        float4 camera;     // roll, sway x, sway y, streak length (world units)
        float4 motion;     // throttle 0...1, heading degrees, glint, unused
        float4 bands;      // low, mid, high, sustained low
        float4 hits;       // low, mid, high, show meters
        float4 nebula;     // drift x, drift y, hue shift, unused
        float4 body;       // center xy (screen, before camera), radius, kind (0 none, 1 planet, 2 moon, 3 asteroids)
        float4 bodyLook;   // has ring, seed, light angle, unused
        float4 bodyColorA;
        float4 bodyColorB;
    };

    struct Palette {
        float4 space;
        float4 spaceEdge;
        float4 star;
        float4 nebulaA;
        float4 nebulaB;
        float4 nebulaC;
        float4 hud;
        float4 atmosphere;
    };

    struct Star {
        float4 position;   // xyz world, layer speed multiplier
        float4 look;       // size px, twinkle, tint, brightness
    };

    struct VertexOut {
        float4 position [[position]];
        float2 uv;
    };

    struct StarOut {
        float4 position [[position]];
        float2 local;      // along (0 tail ... 1 head), side (-1 ... 1)
        float3 color;
        float alpha;
        float streak;      // 0 = dot, 1 = long streak
    };

    static float2 rotate2(float2 p, float a) {
        float c = cos(a), s = sin(a);
        return float2(c * p.x - s * p.y, s * p.x + c * p.y);
    }

    static float2 toScreen(float2 uv, float aspect) {
        return (uv - 0.5) * float2(2.0 * aspect, -2.0);
    }

    static float hash21(float2 p) {
        p = fract(p * float2(123.34, 456.21));
        p += dot(p, p + 45.32);
        return fract(p.x * p.y);
    }

    static float noise(float2 p) {
        float2 i = floor(p), f = fract(p);
        float2 u = f * f * (3.0 - 2.0 * f);
        return mix(mix(hash21(i), hash21(i + float2(1, 0)), u.x),
                   mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), u.x), u.y);
    }

    static float fbm(float2 p) {
        float value = 0.0, amplitude = 0.5;
        for (int i = 0; i < 5; i++) {
            value += amplitude * noise(p);
            p = p * 2.03 + float2(17.1, 9.2);
            amplitude *= 0.5;
        }
        return value;
    }

    static float3 hueShift(float3 c, float angle) {
        const float3 k = float3(0.57735);
        float ca = cos(angle);
        return c * ca + cross(k, c) * sin(angle) + k * dot(k, c) * (1.0 - ca);
    }

    // Premultiplied "over".
    static float4 over(float4 top, float4 bottom) {
        return top + bottom * (1.0 - top.a);
    }

    static float segmentDistance(float2 p, float2 a, float2 b) {
        float2 pa = p - a, ba = b - a;
        float h = saturate(dot(pa, ba) / dot(ba, ba));
        return length(pa - ba * h);
    }

    // Seven-segment digit in a box x 0...1, y 0...2.
    static float digitDistance(float2 p, int digit) {
        int m = 0x7F;
        switch (digit) {
            case 0: m = 0x3F; break;
            case 1: m = 0x06; break;
            case 2: m = 0x5B; break;
            case 3: m = 0x4F; break;
            case 4: m = 0x66; break;
            case 5: m = 0x6D; break;
            case 6: m = 0x7D; break;
            case 7: m = 0x07; break;
            case 8: m = 0x7F; break;
            case 9: m = 0x6F; break;
        }
        float d = 1e5;
        if (m & 0x01) d = min(d, segmentDistance(p, float2(0.15, 2.0), float2(0.85, 2.0)));
        if (m & 0x02) d = min(d, segmentDistance(p, float2(1.0, 1.85), float2(1.0, 1.15)));
        if (m & 0x04) d = min(d, segmentDistance(p, float2(1.0, 0.85), float2(1.0, 0.15)));
        if (m & 0x08) d = min(d, segmentDistance(p, float2(0.15, 0.0), float2(0.85, 0.0)));
        if (m & 0x10) d = min(d, segmentDistance(p, float2(0.0, 0.85), float2(0.0, 0.15)));
        if (m & 0x20) d = min(d, segmentDistance(p, float2(0.0, 1.85), float2(0.0, 1.15)));
        if (m & 0x40) d = min(d, segmentDistance(p, float2(0.15, 1.0), float2(0.85, 1.0)));
        return d;
    }

    // Distance (screen units) to a zero-padded number with its bottom-left at `origin`.
    static float numberDistance(float2 p, float2 origin, int value, int count, float height) {
        float unit = height * 0.5;
        float2 q = (p - origin) / unit;
        const float advance = 1.7;
        if (q.y < -0.5 || q.y > 2.5 || q.x < -0.5 || q.x > advance * float(count)) return 1e5;
        int index = clamp(int(floor(q.x / advance)), 0, count - 1);
        int divisor = 1;
        for (int i = 0; i < count - 1 - index; i++) divisor *= 10;
        int digit = (value / divisor) % 10;
        return digitDistance(float2(q.x - float(index) * advance, q.y), digit) * unit;
    }

    vertex VertexOut fullscreen(uint id [[vertex_id]]) {
        float2 corner = float2((id << 1) & 2, id & 2);
        VertexOut out;
        out.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
        out.uv = float2(corner.x, 1.0 - corner.y);
        return out;
    }

    // MARK: Nebula

    fragment float4 nebulaFragment(VertexOut in [[stage_in]],
                                   constant Uniforms& u [[buffer(0)]],
                                   constant Palette& pal [[buffer(1)]]) {
        float aspect = u.view.z;
        float2 p = toScreen(in.uv, aspect);
        float2 view = rotate2(p - u.camera.yz * 0.3, -u.camera.x * 0.5);
        float mid = u.bands.y;

        float2 q = view * 0.8 + u.nebula.xy;
        float n1 = fbm(q);
        float n2 = fbm(q * 1.7 + float2(n1 * 1.5, -n1) + u.nebula.xy * 0.5);
        float cloud = smoothstep(0.38, 0.85, n1 * 0.55 + n2 * 0.55);

        float3 color = mix(pal.nebulaA.rgb, pal.nebulaB.rgb, smoothstep(0.3, 0.75, n2));
        color = mix(color, pal.nebulaC.rgb, smoothstep(0.45, 0.9, n1));
        color = hueShift(color, u.nebula.z);
        float luminance = dot(color, float3(0.299, 0.587, 0.114));
        color = mix(float3(luminance), color, 0.3 + 0.7 * mid);
        float opacity = 0.10 + 0.55 * mid + 0.12 * u.hits.y;

        float radius = length(p / float2(aspect, 1.0));
        float3 space = mix(pal.space.rgb, pal.spaceEdge.rgb, smoothstep(0.2, 1.3, radius));
        return float4(space + color * cloud * opacity, 1.0);
    }

    // MARK: Stars

    vertex StarOut starVertex(uint vid [[vertex_id]], uint iid [[instance_id]],
                              const device Star* stars [[buffer(0)]],
                              constant Uniforms& u [[buffer(1)]],
                              constant Palette& pal [[buffer(2)]]) {
        Star s = stars[iid];
        float aspect = u.view.z;
        float pixel = 2.0 / u.view.y;
        float z = max(s.position.z, 0.3);
        float2 head = s.position.xy / z;
        float2 tail = s.position.xy / (z + u.camera.w * s.position.w);
        float twinkle = s.look.y;
        float width = s.look.x * (1.0 + 1.2 * twinkle) * clamp(6.0 / z, 0.8, 2.2) * pixel * 0.5;

        float2 direction = head - tail;
        float len = length(direction);
        float2 axis = len > 1e-6 ? direction / len : float2(1.0, 0.0);
        float2 normal = float2(-axis.y, axis.x);
        float along = float(vid & 1);
        float side = (vid & 2) ? 1.0 : -1.0;
        float2 point = mix(tail - axis * width, head + axis * width, along) + normal * side * width;
        point = rotate2(point, u.camera.x) + u.camera.yz;

        StarOut out;
        out.position = float4(point.x / aspect, point.y, 0.0, 1.0);
        out.local = float2(along, side);
        out.color = mix(pal.star.rgb, float3(1.0), s.look.z) * (1.0 + 1.8 * twinkle);
        float streakPixels = len / pixel;
        out.streak = saturate(streakPixels / 8.0);
        out.alpha = s.look.w * mix(1.0, 0.75, out.streak);
        return out;
    }

    fragment float4 starFragment(StarOut in [[stage_in]]) {
        float across = 1.0 - abs(in.local.y);
        float alongFade = mix(1.0, mix(0.08, 1.0, in.local.x * in.local.x), in.streak);
        float a = smoothstep(0.0, 0.9, across) * alongFade * in.alpha;
        return float4(in.color * a, a);
    }

    // MARK: Passing bodies

    fragment float4 bodyFragment(VertexOut in [[stage_in]],
                                 constant Uniforms& u [[buffer(0)]],
                                 constant Palette& pal [[buffer(1)]],
                                 constant float4* rocks [[buffer(2)]]) {
        float aspect = u.view.z;
        float pixel = 2.0 / u.view.y;
        float2 p = rotate2(toScreen(in.uv, aspect) - u.camera.yz, -u.camera.x);
        float kind = u.body.w;
        float3 light = normalize(float3(cos(u.bodyLook.z), sin(u.bodyLook.z), 0.55));

        if (kind > 2.5) {
            // Asteroid field: lumpy, lit rocks, sorted far to near on the CPU.
            float4 result = float4(0.0);
            for (int i = 0; i < 16; i++) {
                float4 rock = rocks[i];
                if (rock.z <= 0.0) continue;
                float2 d = (p - rock.xy) / rock.z;
                if (dot(d, d) > 2.0) continue;
                float angle = atan2(d.y, d.x);
                float lumps = 1.0 + 0.18 * sin(angle * 3.0 + rock.w * 7.0) + 0.1 * sin(angle * 5.0 + rock.w * 13.0);
                float dist = length(d) / lumps;
                if (dist >= 1.0) continue;
                float3 n = normalize(float3(d / lumps, sqrt(max(0.0, 1.0 - dist * dist))));
                float shade = max(0.0, dot(n, light));
                float grain = noise(d * 5.0 + rock.w * 31.0);
                float3 c = mix(u.bodyColorA.rgb, u.bodyColorB.rgb, grain) * (0.06 + 0.94 * shade);
                float edge = smoothstep(1.0, 1.0 - 2.0 * pixel / rock.z, dist);
                result = over(float4(c * edge, edge), result);
            }
            return result;
        }

        float2 d = (p - u.body.xy) / u.body.z;
        float r = length(d);

        // Ring: an ellipse in a tilted plane. The half above the center passes behind the planet.
        float4 ringBack = float4(0.0), ringFront = float4(0.0);
        if (u.bodyLook.x > 0.5) {
            float2 rd = rotate2(d, 0.35);
            float rr = length(float2(rd.x, rd.y / 0.28));
            if (rr > 1.3 && rr < 2.35) {
                float bands = 0.5 + 0.5 * sin(rr * 38.0 + u.bodyLook.y * 10.0);
                float a = smoothstep(1.3, 1.42, rr) * smoothstep(2.35, 2.2, rr) * (0.3 + 0.45 * bands) * 0.85;
                float4 ring = float4(mix(u.bodyColorB.rgb, float3(0.9), 0.3) * a * 0.75, a);
                if (rd.y > 0.0) ringBack = ring; else ringFront = ring;
            }
        }

        float4 sphere = float4(0.0);
        if (r < 1.0) {
            float z = sqrt(max(0.0, 1.0 - r * r));
            float3 n = float3(d, z);
            float shade = max(0.0, dot(n, light));
            float3 surface;
            if (kind < 1.5) {
                float latitude = d.y + 0.12 * fbm(d * 3.0 + u.bodyLook.y);
                float b = fbm(float2(latitude * 7.0, d.x * 0.7 + u.bodyLook.y * 3.0));
                surface = mix(u.bodyColorA.rgb, u.bodyColorB.rgb, smoothstep(0.3, 0.7, b));
            } else {
                float c = fbm(d * 4.0 + u.bodyLook.y * 9.0);
                float pits = smoothstep(0.6, 0.8, noise(d * 10.0 + u.bodyLook.y * 4.0));
                surface = mix(u.bodyColorA.rgb, u.bodyColorB.rgb, c) * (1.0 - 0.35 * pits);
            }
            float3 color = surface * (0.03 + 0.97 * shade);
            float rim = pow(1.0 - z, 3.0) * (kind < 1.5 ? 0.9 : 0.15);
            color += pal.atmosphere.rgb * rim * (0.25 + shade);
            float edge = smoothstep(1.0, 1.0 - 1.5 * pixel / u.body.z, r);
            sphere = float4(color * edge, edge);
        }

        // Thin atmospheric halo just outside a planet's limb.
        float4 halo = float4(0.0);
        if (kind < 1.5 && r >= 1.0) {
            float h = exp(-(r - 1.0) * 22.0) * 0.4;
            halo = float4(pal.atmosphere.rgb * h, h * 0.5);
        }

        return over(ringFront, over(sphere, over(halo, ringBack)));
    }

    // MARK: Overlay (meteors, glint, HUD)

    static float lineAlpha(float distance, float thickness, float pixel) {
        return smoothstep(thickness + pixel, thickness, distance);
    }

    fragment float4 overlayFragment(VertexOut in [[stage_in]],
                                    constant Uniforms& u [[buffer(0)]],
                                    constant Palette& pal [[buffer(1)]],
                                    constant float4* meteors [[buffer(2)]]) {
        float aspect = u.view.z;
        float pixel = 2.0 / u.view.y;
        float2 p = toScreen(in.uv, aspect);
        float3 light = float3(0.0);

        // Shooting stars: outside the ship, so they move with the camera.
        for (int i = 0; i < 3; i++) {
            float4 head = meteors[i * 2];
            float4 tail = meteors[i * 2 + 1];
            if (head.z <= 0.0) continue;
            float2 h = rotate2(head.xy, u.camera.x) + u.camera.yz;
            float2 t = rotate2(tail.xy, u.camera.x) + u.camera.yz;
            float2 pa = p - t, ba = h - t;
            float k = saturate(dot(pa, ba) / max(dot(ba, ba), 1e-8));
            float d = length(pa - ba * k);
            float core = exp(-(d * d) / (pixel * pixel * 2.5)) * k * k;
            light += mix(pal.star.rgb, float3(1.0), 0.5) * core * head.z * 1.6;
        }

        // A faint glint sweeping across the glass.
        float glint = u.motion.z;
        if (glint > 0.001) {
            float band = dot(p, normalize(float2(1.0, 0.55))) - mix(-1.8, 1.8, 1.0 - glint);
            light += float3(0.7, 0.85, 1.0) * exp(-band * band * 25.0) * glint * 0.06;
        }

        float4 hud = float4(0.0);
        float3 hc = pal.hud.rgb;

        // Bezel: a dark frame around the glass, a thin inner line and corner brackets.
        float inset = 0.05;
        float2 toEdge = float2(aspect, 1.0) - abs(p);
        float edgeDistance = min(toEdge.x, toEdge.y);
        float frame = smoothstep(inset + pixel, inset - pixel, edgeDistance);
        hud = over(float4(pal.spaceEdge.rgb * 0.5, 1.0) * frame * 0.85, hud);
        float rim = lineAlpha(abs(edgeDistance - inset), 0.5 * pixel, pixel) * 0.3;
        float2 corner = abs(p) - (float2(aspect, 1.0) - inset);
        bool nearCorner = corner.x > -0.12 && corner.y > -0.12;
        if (nearCorner) rim = max(rim, lineAlpha(abs(edgeDistance - inset), 1.2 * pixel, pixel) * 0.55);
        hud = over(float4(hc, 1.0) * rim, hud);

        // Reticle: a broken ring and four short ticks.
        float r = length(p);
        float angle = atan2(p.y, p.x);
        float ring = lineAlpha(abs(r - 0.05), 0.5 * pixel, pixel) * step(0.38, abs(sin(angle * 2.0 + 0.785)));
        float ticks = 0.0;
        if (abs(p.y) < 2.0 * pixel && abs(p.x) > 0.075 && abs(p.x) < 0.12) ticks = 1.0;
        if (abs(p.x) < 2.0 * pixel && abs(p.y) > 0.075 && abs(p.y) < 0.12) ticks = 1.0;
        float reticle = max(ring, ticks * lineAlpha(min(abs(p.x), abs(p.y)), 0.5 * pixel, pixel));
        hud = over(float4(hc, 1.0) * reticle * 0.28, hud);

        // Heading tape along the top, with the heading number above it.
        float heading = u.motion.y;
        float tapeY = 1.0 - inset - 0.09;
        float tapeHalf = 0.42;
        if (abs(p.x) < tapeHalf && p.y < tapeY + 0.004 && p.y > tapeY - 0.04) {
            const float degreesPerUnit = 70.0;
            float h = heading + p.x * degreesPerUnit;
            float step5 = h / 5.0;
            float offset = abs(fract(step5 + 0.5) - 0.5) * 5.0 / degreesPerUnit;
            bool major = int(round(step5)) % 3 == 0;
            float tickLength = major ? 0.03 : 0.014;
            float tick = lineAlpha(offset, 0.5 * pixel, pixel) * step(tapeY - tickLength, p.y);
            float fade = smoothstep(tapeHalf, tapeHalf * 0.6, abs(p.x));
            hud = over(float4(hc, 1.0) * tick * fade * 0.45, hud);
        }
        float baseline = lineAlpha(abs(p.y - tapeY), 0.4 * pixel, pixel) * smoothstep(tapeHalf, tapeHalf * 0.6, abs(p.x));
        hud = over(float4(hc, 1.0) * baseline * 0.25, hud);
        float caret = lineAlpha(abs(p.x), 0.6 * pixel, pixel) * step(tapeY - 0.055, p.y) * step(p.y, tapeY - 0.04);
        hud = over(float4(hc, 1.0) * caret * 0.6, hud);
        float digitHeight = 0.036;
        float headingDigits = numberDistance(p, float2(-1.5 * 1.7 * digitHeight * 0.5, tapeY + 0.016),
                                             int(heading) % 360, 3, digitHeight);
        hud = over(float4(hc, 1.0) * lineAlpha(headingDigits, digitHeight * 0.05, pixel) * 0.6, hud);

        // Throttle ladder on the right, with a percentage readout below it.
        float ladderX = aspect - inset - 0.06;
        float ladderLow = -0.32, ladderHigh = 0.32;
        float throttle = saturate(u.motion.x);
        if (p.y > ladderLow && p.y < ladderHigh) {
            float t = (p.y - ladderLow) / (ladderHigh - ladderLow);
            float rung = abs(fract(t * 10.0 + 0.5) - 0.5) / 10.0 * (ladderHigh - ladderLow);
            float rungs = lineAlpha(rung, 0.4 * pixel, pixel) * step(abs(p.x - ladderX), 0.012);
            hud = over(float4(hc, 1.0) * rungs * 0.25, hud);
            float fill = step(t, throttle) * step(abs(p.x - ladderX - 0.022), 0.004);
            hud = over(float4(hc, 1.0) * fill * 0.5, hud);
        }
        float throttleDigits = numberDistance(p, float2(ladderX - 0.04, ladderLow - 0.07),
                                              int(round(throttle * 100.0)), 3, 0.03);
        hud = over(float4(hc, 1.0) * lineAlpha(throttleDigits, 0.0015, pixel) * 0.5, hud);

        // Optional band meters, bottom left, as in the other scenes.
        if (u.hits.w > 0.5) {
            float2 px = float2(in.uv.x, 1.0 - in.uv.y) * u.view.xy;
            float unit = max(1.0, u.view.y / 500.0);
            for (int i = 0; i < 3; i++) {
                float x0 = (36.0 + float(i) * 16.0) * unit;
                float y0 = 36.0 * unit;
                float height = 110.0 * unit;
                if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + height) {
                    float fill = (px.y - y0) / height;
                    float3 meter = fill < u.bands[i] ? mix(pal.nebulaA.rgb, pal.nebulaB.rgb, float(i) * 0.5) * 1.6 : float3(0.1);
                    if (u.hits[i] > 0.6 && fill > 0.94) meter = float3(1.0);
                    hud = over(float4(meter * 0.85, 0.85), hud);
                }
            }
        }

        return float4(light, 0.0) + hud;
    }
    """
}
