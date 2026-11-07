// Draws the cell grid as instanced quads: solid rectangles (cell backgrounds, box lines, blocks) and glyphs
// from the single-channel atlas. Output is premultiplied alpha.

struct Uniforms {
    size: vec2<f32>,       // target size in pixels
    atlas_size: vec2<f32>,
};

struct Instance {
    @location(0) rect: vec4<f32>,  // x, y, w, h in pixels, origin top-left
    @location(1) uv: vec4<f32>,    // atlas x0, y0, x1, y1 in pixels
    @location(2) color: vec4<f32>,
    @location(3) kind: vec4<f32>,  // x: 0 solid, 1 glyph
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) kind: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var smp: sampler;

@vertex
fn vs(@builtin(vertex_index) id: u32, inst: Instance) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0));
    let corner = corners[id];
    let pixel = inst.rect.xy + corner * inst.rect.zw;
    var out: VertexOut;
    out.position = vec4<f32>(pixel.x / u.size.x * 2.0 - 1.0, 1.0 - pixel.y / u.size.y * 2.0, 0.0, 1.0);
    out.uv = mix(inst.uv.xy, inst.uv.zw, corner) / u.atlas_size;
    out.color = inst.color;
    out.kind = inst.kind.x;
    return out;
}

@fragment
fn fs(in: VertexOut) -> @location(0) vec4<f32> {
    var alpha = in.color.a;
    if (in.kind > 0.5) {
        alpha *= textureSampleLevel(atlas, smp, in.uv, 0.0).r;
    }
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
