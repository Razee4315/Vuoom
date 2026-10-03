// A textured rect over the frame, faded as one: the user's pointer picture (with an
// optional soft shadow cast from its own shape), or the pointer layer (the drawn pointer
// rendered opaque into its own texture, so a see-through pointer never shows its outline
// through its body). Texture colors are premultiplied, and so is the output.

struct Sprite {
    out_size: vec2<f32>,     // output pixels
    rect_min: vec2<f32>,     // the picture's rect (pixels)
    rect_size: vec2<f32>,
    shadow_shift: vec2<f32>, // the shadow's offset (pixels)
    shadow_blur: f32,        // the shadow's softness (pixels)
    shadow_alpha: f32,       // the shadow's strength; 0 = none
    opacity: f32,            // applied to the picture and its shadow together
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: Sprite;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) px: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vid: u32) -> VsOut {
    // The rect, grown to hold the shadow, as two triangles.
    let grow = vec2<f32>(u.shadow_blur * 2.0);
    let lo = u.rect_min + min(u.shadow_shift, vec2<f32>(0.0)) - grow;
    let hi = u.rect_min + u.rect_size + max(u.shadow_shift, vec2<f32>(0.0)) + grow;
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

// The texture at `uv`, transparent outside the rect.
fn texel(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSample(tex, samp, uv);
    let inside = step(vec2<f32>(0.0), uv) * step(uv, vec2<f32>(1.0));
    return c * inside.x * inside.y;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let size = max(u.rect_size, vec2<f32>(1.0));
    let uv = (in.px - u.rect_min) / size;
    let c = texel(uv);

    // The shadow: the picture's coverage, shifted and blurred over five taps.
    let s_uv = (in.px - u.shadow_shift - u.rect_min) / size;
    let b = u.shadow_blur / size;
    let cover = texel(s_uv).a * 0.4
        + texel(s_uv + vec2<f32>(b.x, b.y)).a * 0.15
        + texel(s_uv + vec2<f32>(-b.x, b.y)).a * 0.15
        + texel(s_uv + vec2<f32>(b.x, -b.y)).a * 0.15
        + texel(s_uv + vec2<f32>(-b.x, -b.y)).a * 0.15;
    let shade = u.shadow_alpha * cover;

    // The picture over its shadow (black, so it adds only coverage).
    let alpha = c.a + shade * (1.0 - c.a);
    return vec4<f32>(c.rgb, alpha) * u.opacity;
}
