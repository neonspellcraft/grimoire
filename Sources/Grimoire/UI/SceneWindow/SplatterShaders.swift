/// Metal source for `SplatterScene`, compiled at runtime (SwiftPM doesn't build .metal files).
///
/// The paint lives in a persistent float texture (ping-ponged each frame):
/// - `dryFragment` copies the previous canvas across, fading the color very slowly and the wetness (alpha)
///   quickly, so fresh paint glistens and then dries.
/// - `stampVertex`/`stampFragment` lay this frame's new paint on top. Color blends "over", and wetness takes the
///   maximum. Stamp kinds:
///   0 superstring blob: the rim is a closed string vibrating in overtones with a random spectrum
///   1 round droplet (stretched along its flight)
///   2 quantum orbital: |cos mθ|^p lobes, sometimes with a radial node
///   3 Einstein ring: a lensed ring that thickens into two bright arcs, around a central source
///   4 cosmic inflation: a thresholded Gaussian random field
///   5 Kerr black hole: logarithmic spiral arms twisted by frame dragging
/// - `presentFragment` draws the canvas with a bloom made from the texture's mipmaps, extra shine on wet paint,
///   a faint canvas grain and the optional band meters.
enum SplatterShaders {
    static let source = """
    #include <metal_stdlib>
    using namespace metal;

    struct Uniforms {
        float4 view;    // width px, height px, aspect, time
        float4 dry;     // color decay, wetness decay, glow strength, show meters
        float4 bands;   // low, mid, high, unused
    };

    struct Stamp {
        float4 shape;   // center xy (screen), radius, rotation
        float4 color;   // rgb, seed
        float4 style;   // kind (see above), stretch, two shape parameters
    };

    struct VertexOut {
        float4 position [[position]];
        float2 uv;
    };

    struct StampOut {
        float4 position [[position]];
        float2 local;
        float3 color;
        float seed;
        float kind;
        float2 params;
        float aa;
    };

    static float h1(float n) {
        return fract(sin(n * 127.1) * 43758.5453);
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

    vertex VertexOut fullscreen(uint id [[vertex_id]]) {
        float2 corner = float2((id << 1) & 2, id & 2);
        VertexOut out;
        out.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
        out.uv = float2(corner.x, 1.0 - corner.y);
        return out;
    }

    fragment float4 dryFragment(VertexOut in [[stage_in]],
                                constant Uniforms& u [[buffer(0)]],
                                texture2d<float, access::read> previous [[texture(0)]]) {
        float4 paint = previous.read(uint2(in.position.xy));
        return float4(paint.rgb * u.dry.x, paint.a * u.dry.y);
    }

    vertex StampOut stampVertex(uint vid [[vertex_id]], uint iid [[instance_id]],
                                const device Stamp* stamps [[buffer(0)]],
                                constant Uniforms& u [[buffer(1)]]) {
        Stamp s = stamps[iid];
        float kind = s.style.x;
        float stretch = max(s.style.y, 1.0);
        // Blobs need room for their fingers; droplets are stretched along their direction of travel.
        float extent = abs(kind - 1.0) < 0.5 ? 1.1 : 1.75;
        float2 corner = float2((vid & 1) ? 1.0 : -1.0, (vid & 2) ? 1.0 : -1.0) * extent;
        float2 scaled = corner * s.shape.z * float2(stretch, 1.0);
        float c = cos(s.shape.w), sn = sin(s.shape.w);
        float2 point = s.shape.xy + float2(c * scaled.x - sn * scaled.y, sn * scaled.x + c * scaled.y);

        StampOut out;
        out.position = float4(point.x / u.view.z, point.y, 0.0, 1.0);
        out.local = corner;
        out.color = s.color.rgb;
        out.seed = s.color.w;
        out.kind = kind;
        out.params = s.style.zw;
        out.aa = (2.0 / u.view.y) / max(s.shape.z, 1e-4) * 1.2;
        return out;
    }

    fragment float4 stampFragment(StampOut in [[stage_in]]) {
        float2 q = in.local;
        float d = length(q);
        float a = atan2(q.y, q.x);
        float s = in.seed;
        int kind = int(in.kind + 0.5);
        float aa = in.aa;
        float cover;

        if (kind == 0) {
            // Superstring: overtones 2...N of a closed string with random phases; `tilt` sets how fast they fall off.
            float tilt = in.params.x;
            int modes = int(in.params.y);
            float edge = 1.0;
            for (int n = 2; n < modes; n++) {
                edge += 0.34 / pow(float(n), tilt) * sin(float(n) * a + 6.2831853 * h1(s + float(n) * 1.7));
            }
            float k = 5.0 + floor(h1(s + 3.0) * 6.0);
            edge += 0.5 * pow(saturate(sin(k * a + s * 3.1) * sin(3.0 * a + s * 0.7)), 5.0);
            cover = smoothstep(edge + aa, edge - aa, d);
        } else if (kind == 2) {
            // Quantum orbital: lobes of |cos m(θ - φ)|^p around a small core, sometimes with a radial node.
            float m = in.params.y, p = in.params.x;
            float lobe = pow(abs(cos(m * (a - s))), p);
            float edge = 0.3 + 0.9 * lobe;
            cover = smoothstep(edge + aa, edge - aa, d);
            if (h1(s + 9.0) > 0.5) {
                float node = smoothstep(0.07 + aa, 0.07 - aa, abs(d - 0.5 * edge));
                cover *= 1.0 - node;
            }
        } else if (kind == 3) {
            // Einstein ring: lensing magnifies two opposite arcs; the source sits at the center.
            float thickness = (0.06 + 0.16 * pow(abs(cos(a - s)), 4.0)) * in.params.x;
            float ring = smoothstep(thickness + aa, thickness - aa, abs(d - 1.0));
            float source = smoothstep(0.2 + aa, 0.2 - aa, d);
            cover = max(ring, source);
        } else if (kind == 4) {
            // Cosmic inflation: a Gaussian random field, thresholded and confined by distance from the center.
            float2 p = q * in.params.y + float2(s * 13.1, s * 7.7);
            float field = 0.55 * noise(p) + 0.3 * noise(p * 2.1 + 3.0) + 0.15 * noise(p * 4.3 + 7.0);
            float value = field - 0.55 * d * d + in.params.x;
            cover = smoothstep(0.5 - 0.02, 0.5 + 0.02, value);
        } else if (kind == 5) {
            // Kerr black hole: logarithmic spiral arms, twisted harder near the center by frame dragging.
            float arms = in.params.x, twist = in.params.y;
            float phase = a - twist * log(max(d, 1e-3)) + s;
            float arm = pow(0.5 + 0.5 * cos(arms * phase), 4.0);
            float disc = smoothstep(1.0 + aa, 1.0 - aa, d);
            float horizon = smoothstep(0.16 + aa, 0.16 - aa, d);
            cover = max(disc * smoothstep(0.35, 0.65, arm + 0.25 * (1.0 - d)), horizon);
        } else {
            cover = smoothstep(1.0 + aa, 1.0 - aa, d);
        }
        if (cover <= 0.0) discard_fragment();
        float3 color = in.color * (0.92 + 0.2 * smoothstep(1.0, 0.0, d));
        return float4(color, cover);
    }

    fragment float4 presentFragment(VertexOut in [[stage_in]],
                                    constant Uniforms& u [[buffer(0)]],
                                    texture2d<float> paint [[texture(0)]]) {
        constexpr sampler smp(address::clamp_to_edge, filter::linear, mip_filter::linear);
        float4 base = paint.sample(smp, in.uv, level(0.0));
        // Bloom from progressively blurrier mip levels: neon light bleeding into the black.
        float3 glow = paint.sample(smp, in.uv, level(2.5)).rgb * 0.45
                    + paint.sample(smp, in.uv, level(4.5)).rgb * 0.35
                    + paint.sample(smp, in.uv, level(6.5)).rgb * 0.25;
        float wet = base.a;
        // Dried paint sits back so fresh, wet splats pop over it.
        float3 color = base.rgb * (0.62 + 0.7 * wet) + glow * u.dry.z;
        color = 1.0 - exp(-color * 1.35);
        float grain = (hash21(floor(in.uv * u.view.xy)) - 0.5) * 0.02;
        color = max(color + grain, 0.0);

        if (u.dry.w > 0.5) {
            float2 px = float2(in.uv.x, 1.0 - in.uv.y) * u.view.xy;
            float unit = max(1.0, u.view.y / 500.0);
            for (int i = 0; i < 3; i++) {
                float x0 = (16.0 + float(i) * 16.0) * unit;
                float y0 = 16.0 * unit;
                float h = 110.0 * unit;
                if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + h) {
                    float fill = (px.y - y0) / h;
                    color = mix(color, fill < u.bands[i] ? float3(1.0, 0.25, 0.6) : float3(0.12), 0.85);
                }
            }
        }
        return float4(color, 1.0);
    }
    """
}
