// Shows the Doom engine's 640×400 frame at Doom's original 4:3 shape, letterboxed in the window.

struct Uniforms {
    view: vec4<f32>, // window width, window height, unused, unused
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var frame: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

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

@fragment
fn present(in: VertexOut) -> @location(0) vec4<f32> {
    // Fit a 4:3 picture inside the window, centered.
    let window = u.view.xy;
    let picture = select(vec2<f32>(window.y * 4.0 / 3.0, window.y), vec2<f32>(window.x, window.x * 3.0 / 4.0),
                         window.x / window.y < 4.0 / 3.0);
    let offset = (window - picture) * 0.5;
    let uv = (in.uv * window - offset) / picture;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(textureSampleLevel(frame, smp, uv, 0.0).rgb, 1.0);
}
