// Spellrun present pass (port of SpellrunScene.swift's Metal shader): each game pixel covers an exact
// scale×scale block of screen pixels, plus a whisper of scanline.

struct Uniforms {
    info: vec4<f32>, // scale, canvas width, canvas height, unused
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var image: texture_2d<f32>;

@vertex
fn fullscreen(@builtin(vertex_index) id: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn present(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let scale = u.info.x;
    let texel = min(vec2<u32>(position.xy / scale), vec2<u32>(u32(u.info.y - 1.0), u32(u.info.z - 1.0)));
    var color = textureLoad(image, texel, 0).rgb;
    if (scale >= 3.0 && (position.y % scale) >= scale - max(1.0, floor(scale / 4.0))) {
        color *= 0.9;
    }
    return vec4<f32>(color, 1.0);
}
