// Flat colored 2D shapes (highlight boxes + arrows), drawn over the composited frame.
// Vertices are in output pixels; the vertex shader maps them to clip space.

struct ShapeUniforms {
    out_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: ShapeUniforms;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(in: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = in.pos / u.out_size * 2.0 - vec2<f32>(1.0, 1.0);
    out.clip = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
