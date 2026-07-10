// Vuoom composite shader: draws the styled background, then the zoom/pan-cropped source
// inside a rounded-corner frame (SDF, anti-aliased). Text/shapes/shadow layer on top in
// later passes. See docs/05-Compositing-and-Preview.md.

struct Uniforms {
    out_size: vec2<f32>,   // output pixels
    src_min: vec2<f32>,    // source crop rect min (normalized 0..1)
    src_size: vec2<f32>,   // source crop rect size (normalized)
    dst_min: vec2<f32>,    // destination rect min (pixels)
    dst_size: vec2<f32>,   // destination rect size (pixels)
    corner_px: f32,        // rounded-corner radius (pixels)
    _pad: f32,
    bg: vec4<f32>,         // backdrop stop 0 (straight RGBA)
    bg2: vec4<f32>,        // backdrop stop 1 (straight RGBA); == bg for a solid fill
    bg_dir: vec2<f32>,     // gradient axis (unit vector, output UV space, y down)
    _pad2: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vid: u32) -> VsOut {
    // Oversized fullscreen triangle.
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let xy = corners[vid];
    var out: VsOut;
    out.pos = vec4<f32>(xy, 0.0, 1.0);
    // uv with (0,0) at top-left of the output.
    out.uv = vec2<f32>(xy.x * 0.5 + 0.5, -xy.y * 0.5 + 0.5);
    return out;
}

// Signed distance to a rounded box (Inigo Quilez).
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// Linear 2-stop backdrop at this pixel. Projects the output UV onto the gradient axis and
// normalizes by the axis's extent across the unit frame, so the stops land on opposite
// corners for a diagonal direction. Solid fills pass bg2 == bg, so the mix is a no-op.
fn backdrop(uv: vec2<f32>) -> vec4<f32> {
    let d = u.bg_dir;
    // Projected span of the [0,1]^2 frame onto d: [pmin, pmax], length |dx| + |dy|.
    let pmin = min(0.0, d.x) + min(0.0, d.y);
    let pmax = max(0.0, d.x) + max(0.0, d.y);
    let denom = max(pmax - pmin, 1e-6);
    let t = clamp((dot(uv, d) - pmin) / denom, 0.0, 1.0);
    return mix(u.bg, u.bg2, t);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.uv * u.out_size;

    let center = u.dst_min + u.dst_size * 0.5;
    let half = u.dst_size * 0.5;
    let d = sd_rounded_box(px - center, half, u.corner_px);
    let aa = max(fwidth(d), 0.0001);
    let inside = 1.0 - smoothstep(-aa, aa, d);

    let bg = backdrop(in.uv);
    if inside <= 0.0 {
        return bg;
    }

    // Map the pixel within the destination rect into the source crop.
    let local = (px - u.dst_min) / u.dst_size;
    let src_uv = u.src_min + local * u.src_size;
    let col = textureSample(src_tex, src_samp, src_uv);
    return mix(bg, col, inside);
}
