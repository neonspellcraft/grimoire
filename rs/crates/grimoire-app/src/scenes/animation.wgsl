// Draws an animation frame covering the whole window: scaled to fill, edges trimmed, never letterboxed.

struct Uniforms {
    view: vec4<f32>, // window width, window height, frame width, frame height
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
    let window_aspect = u.view.x / u.view.y;
    let frame_aspect = u.view.z / u.view.w;
    // The part of the frame that's visible, centered: full height in a narrower window, full width in a wider one.
    var visible = vec2<f32>(1.0, 1.0);
    if (window_aspect < frame_aspect) {
        visible.x = window_aspect / frame_aspect;
    } else {
        visible.y = frame_aspect / window_aspect;
    }
    let uv = (in.uv - 0.5) * visible + 0.5;
    return vec4<f32>(textureSampleLevel(frame, smp, uv, 0.0).rgb, 1.0);
}
