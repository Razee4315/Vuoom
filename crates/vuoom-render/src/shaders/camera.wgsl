// Vuoom webcam bubble: the camera frame, center-cropped to the bubble (and mirrored by a
// right-to-left crop), clipped to a rounded box (a circle at half the height) with a soft
// shadow and a thin light rim. Output is premultiplied. See docs/17-Camera.md.

struct Cam {
    out_size: vec2<f32>,   // output pixels
    rect_min: vec2<f32>,   // bubble rect (pixels)
    rect_size: vec2<f32>,
    uv_min: vec2<f32>,     // camera crop (texture space; negative width = mirrored)
    uv_size: vec2<f32>,
    radius: f32,           // corner radius (pixels)
    shadow: f32,           // shadow softness (pixels)
};

@group(0) @binding(0) var<uniform> u: Cam;
@group(0) @binding(1) var cam_tex: texture_2d<f32>;
@group(0) @binding(2) var cam_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) px: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vid: u32) -> VsOut {
    // The bubble's rect, grown to hold its shadow, as two triangles.
    let grow = u.shadow * 2.0;
    let lo = u.rect_min - vec2<f32>(grow, grow);
    let hi = u.rect_min + u.rect_size + vec2<f32>(grow, grow * 1.5);
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let px = mix(lo, hi, corners[vid]);
    var out: VsOut;
    out.pos = vec4<f32>(px.x / u.out_size.x * 2.0 - 1.0, 1.0 - px.y / u.out_size.y * 2.0, 0.0, 1.0);
    out.px = px;
    return out;
}

// Signed distance to a rounded box (Inigo Quilez).
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let half = u.rect_size * 0.5;
    let center = u.rect_min + half;
    let r = min(u.radius, min(half.x, half.y));
    let d = sd_rounded_box(in.px - center, half, r);
    let aa = max(fwidth(d), 0.0001);
    let inside = 1.0 - smoothstep(-aa, aa, d);

    // The shadow: the same shape a little lower, blurred.
    let drop = vec2<f32>(0.0, u.shadow * 0.4);
    let ds = sd_rounded_box(in.px - center - drop, half, r);
    let shade = 0.4 * (1.0 - smoothstep(-u.shadow, u.shadow, ds));

    let local = clamp((in.px - u.rect_min) / u.rect_size, vec2<f32>(0.0), vec2<f32>(1.0));
    let uv = u.uv_min + local * u.uv_size;
    let col = textureSampleLevel(cam_tex, cam_samp, uv, 0.0).rgb;

    // A thin light rim lifts the bubble off dark backdrops.
    let rim_w = max(1.0, u.rect_size.y * 0.012);
    let rim = smoothstep(-rim_w - aa, -rim_w + aa, d);
    let rgb = mix(col, vec3<f32>(1.0), rim * 0.85);

    // Premultiplied: the bubble over its shadow (black, so it adds only coverage).
    let alpha = inside + shade * (1.0 - inside);
    return vec4<f32>(rgb * inside, alpha);
}
