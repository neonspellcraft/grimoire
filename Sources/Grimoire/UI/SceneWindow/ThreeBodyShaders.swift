/// Metal source for `ThreeBodyScene`, compiled at runtime (SwiftPM doesn't build .metal files).
///
/// Two passes:
/// - `evolve` renders into a half-resolution float texture. It warps the previous frame (zoom, rotation and a
///   smear along the gravity field), fades it, and adds the bodies' glow and equipotential rings, so motion
///   leaves psychedelic trails.
/// - `present` draws that texture to the window with a split-color edge, tone mapping, a vignette and the
///   optional band meters.
enum ThreeBodyShaders {
    static let source = """
    #include <metal_stdlib>
    using namespace metal;

    struct Uniforms {
        float4 view;      // resolution.x, resolution.y, flow phase, aspect
        float4 bodies[3]; // position.xy (y spans -1...1), mass, band level
        float4 bands;     // low, mid, high, loudest
        float4 hits;      // low, mid, high, hue phase
        float4 feedback;  // zoom, rotation, decay, show meters
    };

    struct VertexOut {
        float4 position [[position]];
        float2 uv;
    };

    vertex VertexOut fullscreen(uint id [[vertex_id]]) {
        float2 corner = float2((id << 1) & 2, id & 2);
        VertexOut out;
        out.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
        out.uv = float2(corner.x, 1.0 - corner.y);
        return out;
    }

    static float3 palette(float t) {
        return 0.5 + 0.5 * cos(6.2831853 * (t + float3(0.0, 0.33, 0.67)));
    }

    static float hash(float2 p) {
        return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
    }

    fragment float4 evolve(VertexOut in [[stage_in]],
                           constant Uniforms& u [[buffer(0)]],
                           texture2d<float> previous [[texture(0)]]) {
        constexpr sampler smp(address::clamp_to_zero, filter::linear);
        float aspect = u.view.w;
        float flow = u.view.z;
        float low = u.bands.x, mid = u.bands.y, high = u.bands.z;

        // y spans -1...1 and points up, matching the physics.
        float2 p = (in.uv - 0.5) * float2(aspect, -1.0) * 2.0;

        // Gravitational potential, pull and per-body glow.
        float phi = 0.0;
        float2 pull = float2(0.0);
        float3 glow = float3(0.0);
        for (int i = 0; i < 3; i++) {
            float2 d = u.bodies[i].xy - p;
            float r2 = dot(d, d) + 0.003;
            float mass = u.bodies[i].z;
            phi += mass / sqrt(r2);
            pull += mass * d / (r2 * sqrt(r2));
            float size = 0.0025 + 0.008 * u.bodies[i].w;
            glow += palette(u.hits.w + float(i) / 3.0) * (size / r2);
        }

        // Feedback: rotate and zoom the last frame, then smear it along the field like gravitational lensing.
        float angle = u.feedback.y;
        float2x2 rotation = float2x2(float2(cos(angle), sin(angle)), float2(-sin(angle), cos(angle)));
        float2 q = rotation * p * u.feedback.x;
        float pullLength = length(pull);
        q -= (pull / max(pullLength, 1e-4)) * min(pullLength, 30.0) * (0.0003 + 0.0015 * mid);
        float2 sourceUV = float2(q.x / aspect, -q.y) * 0.5 + 0.5;
        // fmax/fmin drop NaNs, so one bad pixel can't poison the feedback loop forever.
        float3 old = fmin(fmax(previous.sample(smp, sourceUV).rgb, 0.0), 64.0) * u.feedback.z;

        // Equipotential rings flowing outward. Bass makes them denser and brighter; highs sharpen them.
        float density = 0.6 + 1.6 * low;
        float wave = phi * density - flow;
        // Clamp the base: rounding can push it a hair below zero, and pow() of a negative is NaN.
        float ring = pow(saturate(0.5 + 0.5 * cos(wave * 6.2831853)), 8.0 - 5.0 * high);
        float3 color = palette(phi * 0.06 + u.hits.w) * ring;

        // Highs: glitter along the rings.
        float sparkle = step(1.0 - 0.05 * high, hash(floor(in.uv * u.view.xy) + floor(flow * 7.0)));
        color += sparkle * ring * high * 1.5;

        float strength = 0.06 + 0.2 * low + 0.35 * u.hits.x;
        float3 fresh = color * strength + glow * (0.35 + 0.6 * u.bands.w + 0.6 * u.hits.y);
        return float4(old + fresh, 1.0);
    }

    fragment float4 present(VertexOut in [[stage_in]],
                            constant Uniforms& u [[buffer(0)]],
                            texture2d<float> image [[texture(0)]]) {
        constexpr sampler smp(address::clamp_to_edge, filter::linear);
        float2 uv = in.uv;

        // Split-color edge that widens with the highs.
        float2 split = (uv - 0.5) * (0.002 + 0.014 * u.bands.z + 0.01 * u.hits.z);
        float3 c = float3(image.sample(smp, uv + split).r,
                          image.sample(smp, uv).g,
                          image.sample(smp, uv - split).b);
        c = 1.0 - exp(-c * 1.3);

        float vignette = smoothstep(1.3, 0.3, length((uv - 0.5) * float2(u.view.w, 1.0)));
        c *= mix(0.5, 1.0, vignette);

        if (u.feedback.w > 0.5) {
            float2 px = float2(uv.x, 1.0 - uv.y) * u.view.xy;
            float unit = max(1.0, u.view.y / 500.0);
            for (int i = 0; i < 3; i++) {
                float x0 = (16.0 + float(i) * 16.0) * unit;
                float y0 = 16.0 * unit;
                float h = 110.0 * unit;
                if (px.x >= x0 && px.x < x0 + 11.0 * unit && px.y >= y0 && px.y < y0 + h) {
                    float fill = (px.y - y0) / h;
                    float3 meter = fill < u.bands[i] ? palette(float(i) / 3.0 + 0.05) : float3(0.1);
                    if (u.hits[i] > 0.6 && fill > 0.94) meter = float3(1.0);
                    c = mix(c, meter, 0.85);
                }
            }
        }
        return float4(c, 1.0);
    }
    """
}
