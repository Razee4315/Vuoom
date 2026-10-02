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
    prev_min: vec2<f32>,   // motion blur: source crop one exposure ago (min, normalized)
    prev_size: vec2<f32>,  // ...and its size
    blur: f32,             // 1 = smear from prev_* to src_*, 0 = a single sample
    bg_image: f32,         // 1 = the backdrop is the picture in bg_tex, 0 = the stops above
    bg_scale: vec2<f32>,   // the picture's visible UV extent, so it covers the frame
    shadow: vec4<f32>,     // the recording's shadow: offset x, y, blur (pixels), strength
    taps: vec2<f32>,       // samples across and down per output pixel (1 = a single sample)
    _pad3: vec2<f32>,
};

// Samples along the camera's path for motion blur.
const BLUR_TAPS: i32 = 12;

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;
@group(0) @binding(3) var bg_tex: texture_2d<f32>;

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
    if u.bg_image > 0.5 {
        // The picture, centered and scaled to cover the frame.
        let st = vec2<f32>(0.5) + (uv - vec2<f32>(0.5)) * u.bg_scale;
        return textureSampleLevel(bg_tex, src_samp, st, 0.0);
    }
    let d = u.bg_dir;
    // Projected span of the [0,1]^2 frame onto d: [pmin, pmax], length |dx| + |dy|.
    let pmin = min(0.0, d.x) + min(0.0, d.y);
    let pmax = max(0.0, d.x) + max(0.0, d.y);
    let denom = max(pmax - pmin, 1e-6);
    let t = clamp((dot(uv, d) - pmin) / denom, 0.0, 1.0);
    return mix(u.bg, u.bg2, t);
}

// The recording at `uv`, averaged over `span`: the stretch of it (in uv) that one output
// pixel covers. Drawn smaller than it was recorded, a pixel covers several source pixels,
// and a single sample would skip most of them (thin strokes of text flicker and break
// up); a taps.x by taps.y grid of bilinear samples spread over the span takes them all in.
// At 1:1 or magnified it is one sample.
fn sample_area(uv: vec2<f32>, span: vec2<f32>) -> vec4<f32> {
    let nx = i32(u.taps.x);
    let ny = i32(u.taps.y);
    if nx <= 1 && ny <= 1 {
        return textureSampleLevel(src_tex, src_samp, uv, 0.0);
    }
    var acc = vec4<f32>(0.0);
    for (var j = 0; j < ny; j = j + 1) {
        let oy = (f32(j) + 0.5) / f32(ny) - 0.5;
        for (var i = 0; i < nx; i = i + 1) {
            let ox = (f32(i) + 0.5) / f32(nx) - 0.5;
            let at = uv + vec2<f32>(ox, oy) * span;
            acc = acc + textureSampleLevel(src_tex, src_samp, at, 0.0);
        }
    }
    return acc / f32(nx * ny);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.uv * u.out_size;

    let center = u.dst_min + u.dst_size * 0.5;
    let half = u.dst_size * 0.5;
    let d = sd_rounded_box(px - center, half, u.corner_px);
    let aa = max(fwidth(d), 0.0001);
    let inside = 1.0 - smoothstep(-aa, aa, d);

    var bg = backdrop(in.uv);
    if u.shadow.w > 0.0 {
        // The recording's shadow on the backdrop: its own outline, moved by the offset
        // and faded across the blur.
        let ds = sd_rounded_box(px - center - u.shadow.xy, half, u.corner_px);
        let shade = u.shadow.w * (1.0 - smoothstep(-u.shadow.z, u.shadow.z, ds));
        bg = vec4<f32>(bg.rgb * (1.0 - shade), bg.a);
    }
    if inside <= 0.0 {
        return bg;
    }

    // Map the pixel within the destination rect into the source crop.
    let local = (px - u.dst_min) / u.dst_size;
    var col: vec4<f32>;
    if u.blur > 0.5 {
        // Motion blur: average what this pixel saw as the camera moved from where it was
        // one exposure ago to where it is now, like a real shutter during a zoom or pan.
        var acc = vec4<f32>(0.0);
        for (var i = 0; i < BLUR_TAPS; i = i + 1) {
            let k = f32(i) / f32(BLUR_TAPS - 1);
            let crop_min = mix(u.prev_min, u.src_min, k);
            let crop_size = mix(u.prev_size, u.src_size, k);
            acc = acc + sample_area(crop_min + local * crop_size, crop_size / u.dst_size);
        }
        col = acc / f32(BLUR_TAPS);
    } else {
        col = sample_area(u.src_min + local * u.src_size, u.src_size / u.dst_size);
    }
    return mix(bg, col, inside);
}
