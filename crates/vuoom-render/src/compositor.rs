//! Headless wgpu compositor: device bring-up, the composite pipeline (background +
//! zoom/pan-cropped source + rounded-corner SDF, `shaders/composite.wgsl`), and offscreen
//! render + readback. Text (glyphon) and shapes (lyon) layer on top later.
//! See `docs/05-Compositing-and-Preview.md`.
//!
//! `new()` returns `None` when no GPU adapter is available (e.g. a CI runner without a
//! GPU), so tests skip gracefully rather than fail.

use crate::scene::{ResolvedCaption, ResolvedCursor, ResolvedHighlight, Scene};
use crate::shapes::{build_shape_vertices, cursor_scale, pointer_vertices, ShapeVertex};
use glyphon::{
    Attrs, Buffer as TextBuffer, Cache as GlyphCache, Color as GlyphColor, Family, FontSystem,
    Metrics, Resolution, Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer,
    Viewport, Weight,
};
use std::sync::Mutex;
use vuoom_project::{Color, PointerLook};

/// Load the bundled display fonts into the glyphon font DB so text annotations can be
/// rendered by family name (matching the `@font-face` set the web UI previews with).
fn load_bundled_fonts(font_system: &mut FontSystem) {
    let db = font_system.db_mut();
    db.load_font_data(include_bytes!("../../../assets/fonts/Anton-Regular.ttf").to_vec());
    db.load_font_data(include_bytes!("../../../assets/fonts/BebasNeue-Regular.ttf").to_vec());
    db.load_font_data(include_bytes!("../../../assets/fonts/Poppins-Bold.ttf").to_vec());
    db.load_font_data(include_bytes!("../../../assets/fonts/PermanentMarker-Regular.ttf").to_vec());
    db.load_font_data(include_bytes!("../../../assets/fonts/Shrikhand-Regular.ttf").to_vec());
}

struct TextState {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderer: TextRenderer,
}

/// The backdrop fill for the area behind/around the framed recording: a linear 2-stop
/// gradient. A solid fill is the degenerate case (`color2 == color`).
///
/// `dir` is the gradient axis as a unit vector in output UV space (0..1, y down); the
/// compositor projects each pixel onto it and normalizes across the frame, so the two stops
/// land on opposite corners for a diagonal `dir`. Colors are straight RGBA, interpolated in
/// the same (non-linearized) space the target texture is written in, matching the existing
/// solid-fill and source `mix` handling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgFill {
    pub color: [f32; 4],
    pub color2: [f32; 4],
    pub dir: [f32; 2],
    /// Draw the backdrop picture (see [`Compositor::set_backdrop_image`]) instead of the
    /// color stops.
    pub image: bool,
}

impl BgFill {
    /// A flat solid fill (both stops equal, direction irrelevant).
    #[must_use]
    pub fn solid(color: [f32; 4]) -> Self {
        Self {
            color,
            color2: color,
            dir: [0.0, 1.0],
            image: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    out_size: [f32; 2],
    src_min: [f32; 2],
    src_size: [f32; 2],
    dst_min: [f32; 2],
    dst_size: [f32; 2],
    corner_px: f32,
    _pad: f32,
    bg: [f32; 4],
    bg2: [f32; 4],
    bg_dir: [f32; 2],
    _pad2: [f32; 2],
    /// Motion blur: the source crop one exposure ago (equal to `src_*` when still).
    prev_min: [f32; 2],
    prev_size: [f32; 2],
    /// 1.0 = smear from `prev_*` to `src_*`; 0.0 = one sample.
    blur: f32,
    /// 1.0 = the backdrop is the picture; 0.0 = the color stops.
    bg_image: f32,
    /// The picture's visible UV extent, so it covers the frame (centered).
    bg_scale: [f32; 2],
    /// The recording's shadow on the backdrop: offset x, y and blur (pixels), strength.
    shadow: [f32; 4],
    /// Samples across and down per output pixel (see [`area_taps`]).
    taps: [f32; 2],
    _pad3: [f32; 2],
}

/// Spare capacity left on a composited frame's buffer, for a trailer appended in place.
const READBACK_SPARE: usize = 64;

/// Most samples per axis the composite shader takes for one output pixel.
const MAX_TAPS: f64 = 4.0;
/// The most per axis while motion blur is on: the shader is already taking a dozen samples
/// along the camera's path, and the smear hides what a coarser average misses.
const MAX_TAPS_BLURRED: f64 = 2.0;

/// How many samples, across and down, cover the source pixels behind one output pixel when
/// `crop` (a normalized part) of a `src`-pixel-wide axis is drawn `dst` pixels wide. One at
/// 1:1 or magnified; more as the picture is drawn smaller, so no source pixel is skipped.
/// Each sample is bilinear (it blends two source pixels), so `n` of them span a pixel that
/// covers up to about `2n` of them.
fn area_taps(crop: f64, src: u32, dst: f64, max: f64) -> f32 {
    let footprint = crop * f64::from(src) / dst.max(1e-6);
    // A hair over 1:1 (an odd pixel of rounding) is still one sample: two would only blur.
    (footprint - 0.25).ceil().clamp(1.0, max) as f32
}

/// A webcam frame for the scene's bubble.
#[derive(Debug, Clone, Copy)]
pub struct CameraImage<'a> {
    /// BGRA pixels, top row first.
    pub bgra: &'a [u8],
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniforms {
    out_size: [f32; 2],
    rect_min: [f32; 2],
    rect_size: [f32; 2],
    uv_min: [f32; 2],
    uv_size: [f32; 2],
    radius: f32,
    shadow: f32,
}

/// The webcam texture and its bind group, rebuilt when the camera's frame size changes.
struct CameraCache {
    width: u32,
    height: u32,
    tex: wgpu::Texture,
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// The part of a `w`×`h` camera frame that fills a bubble of width over height `aspect`
/// (a centered crop), as texture-space `(min, size)`. Mirrored, it runs right to left.
fn camera_crop(w: u32, h: u32, aspect: f64, mirror: bool) -> ([f32; 2], [f32; 2]) {
    let frame = f64::from(w) / f64::from(h.max(1));
    let (sw, sh) = if frame > aspect {
        (aspect / frame, 1.0)
    } else {
        (1.0, frame / aspect)
    };
    let x = (1.0 - sw) / 2.0;
    let y = (1.0 - sh) / 2.0;
    let (x, uw) = if mirror { (x + sw, -sw) } else { (x, sw) };
    ([x as f32, y as f32], [uw as f32, sh as f32])
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShapeUniforms {
    out_size: [f32; 2],
    _pad: [f32; 2],
}

/// `shaders/sprite.wgsl`'s uniforms: a textured rect faded as one, with an optional shadow.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteUniforms {
    out_size: [f32; 2],
    rect_min: [f32; 2],
    rect_size: [f32; 2],
    shadow_shift: [f32; 2],
    shadow_blur: f32,
    shadow_alpha: f32,
    opacity: f32,
    _pad: f32,
}

/// The longest side the pointer picture is kept at: sharp on a zoomed-in 4K export, and
/// mipmapped below that, since it is mostly drawn far smaller.
const POINTER_MAX: u32 = 512;

/// The user's pointer picture on the GPU, mipmapped, with its sprite uniforms.
struct PointerSprite {
    _tex: wgpu::Texture,
    /// Its size at full resolution (after any shrinking to [`POINTER_MAX`]).
    width: u32,
    height: u32,
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Premultiply BGRA pixels by their alpha, so filtering never bleeds the color of fully
/// transparent pixels (often black) into the edges.
fn premultiply(px: &mut [u8]) {
    for p in px.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
}

/// Half the size of a `w`×`h` premultiplied BGRA picture, averaging each 2×2 block (an odd
/// last row or column is left out, as a GPU does). Never below 1×1.
fn halve(px: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let (w, h) = (w as usize, h as usize);
    let mut out = Vec::with_capacity(nw as usize * nh as usize * 4);
    for y in 0..nh as usize {
        let rows = [(y * 2).min(h - 1), (y * 2 + 1).min(h - 1)];
        for x in 0..nw as usize {
            let cols = [(x * 2).min(w - 1), (x * 2 + 1).min(w - 1)];
            for c in 0..4 {
                let mut sum = 0u32;
                for row in rows {
                    for col in cols {
                        sum += u32::from(px[(row * w + col) * 4 + c]);
                    }
                }
                out.push(((sum + 2) / 4) as u8);
            }
        }
    }
    (out, nw, nh)
}

/// A premultiplied picture and its mip chain, from its full size down to 1×1, the first
/// level shrunk until its longest side fits `max`.
fn mip_chain(mut px: Vec<u8>, mut w: u32, mut h: u32, max: u32) -> Vec<(Vec<u8>, u32, u32)> {
    while w.max(h) > max {
        (px, w, h) = halve(&px, w, h);
    }
    let mut levels = vec![(px, w, h)];
    while w > 1 || h > 1 {
        let (next, nw, nh) = halve(&levels[levels.len() - 1].0, w, h);
        (w, h) = (nw, nh);
        levels.push((next, nw, nh));
    }
    levels
}

/// Where the pointer picture goes for `c`: as tall as the drawn pointer, its own width
/// for that height, its hotspot on the point; with its shadow when the pointer has one.
fn picture_uniforms(c: &ResolvedCursor, pic: (u32, u32), out: (u32, u32)) -> SpriteUniforms {
    let h = cursor_scale(c);
    let w = h * f64::from(pic.0) / f64::from(pic.1.max(1));
    let [hx, hy] = c.hotspot.map(f64::from);
    SpriteUniforms {
        out_size: [out.0 as f32, out.1 as f32],
        rect_min: [(c.x - hx * w) as f32, (c.y - hy * h) as f32],
        rect_size: [w as f32, h as f32],
        shadow_shift: [(0.03 * h) as f32, (0.06 * h) as f32],
        shadow_blur: (0.05 * h).max(1.0) as f32,
        shadow_alpha: if c.shadow { 0.35 } else { 0.0 },
        opacity: c.group_opacity() as f32,
        _pad: 0.0,
    }
}

/// Size-keyed GPU resources reused across same-dimension `composite_scene` calls. Everything
/// here depends only on the input/output dimensions, so during an export (constant dims) it is
/// built once and every frame streams its per-frame data in via `queue.write_*`. Recreated when
/// the dimensions change.
struct CompositeCache {
    src_w: u32,
    src_h: u32,
    out_w: u32,
    out_h: u32,
    src_tex: wgpu::Texture,
    src_view: wgpu::TextureView,
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The backdrop picture `bind_group` was made with.
    backdrop_generation: u64,
    shape_ubuf: wgpu::Buffer,
    shape_bind_group: wgpu::BindGroup,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    /// The pointer drawn on its own, opaque, when it is to be shown see-through: faded
    /// onto `target` as one, so its outline never shows through its body.
    _layer: wgpu::Texture,
    layer_view: wgpu::TextureView,
    layer_ubuf: wgpu::Buffer,
    layer_bind_group: wgpu::BindGroup,
    readback: wgpu::Buffer,
    /// `bytes_per_row` of `readback`, rounded up to `COPY_BYTES_PER_ROW_ALIGNMENT` (256).
    padded_bpr: u32,
}

impl CompositeCache {
    fn matches(&self, src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> bool {
        self.src_w == src_w && self.src_h == src_h && self.out_w == out_w && self.out_h == out_h
    }
}

/// A headless GPU compositor.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    shape_pipeline: wgpu::RenderPipeline,
    shape_bind_group_layout: wgpu::BindGroupLayout,
    camera_pipeline: wgpu::RenderPipeline,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Textured rects faded as one: the pointer picture and the pointer layer.
    sprite_pipeline: wgpu::RenderPipeline,
    sprite_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Trilinear, for the mipmapped pointer picture.
    mip_sampler: wgpu::Sampler,
    /// The user's pointer picture, when one is set.
    pointer: Mutex<Option<PointerSprite>>,
    text: Mutex<TextState>,
    /// Reused size-keyed resources for the hot `composite_scene` path. `None` until the first
    /// composite; rebuilt whenever the frame dimensions change.
    cache: Mutex<Option<CompositeCache>>,
    /// The webcam bubble's texture, sized to the camera frames.
    camera_cache: Mutex<Option<CameraCache>>,
    /// The picture behind the framed recording, for picture backdrops.
    backdrop: Mutex<Backdrop>,
    /// The GPU in use (name, graphics API, type, driver), for diagnostics.
    adapter: String,
}

/// A backdrop placeholder: one opaque black pixel.
const BLANK_BACKDROP: [u8; 4] = [0, 0, 0, 255];

/// The picture behind the framed recording when the backdrop is an image (the black
/// placeholder otherwise). `generation` changes with each new picture, so the composite
/// bind group knows to rebind.
struct Backdrop {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    generation: u64,
}

/// Upload a BGRA picture (top row first) as the backdrop texture.
fn make_backdrop(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgra: &[u8],
    (width, height): (u32, u32),
    generation: u64,
) -> Backdrop {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vuoom-backdrop"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bgra[..width as usize * height as usize * 4],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        size,
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Backdrop {
        _tex: tex,
        view,
        width,
        height,
        generation,
    }
}

/// The part of a `pic` (width, height) picture that covers a `frame` (width, height) output,
/// centered and cropped on its long side, as a UV extent.
fn cover_scale(frame: (u32, u32), pic: (u32, u32)) -> [f32; 2] {
    let aspect = |(w, h): (u32, u32)| f64::from(w) / f64::from(h.max(1));
    let (f, p) = (aspect(frame), aspect(pic));
    if f > p {
        [1.0, (p / f) as f32]
    } else {
        [(f / p) as f32, 1.0]
    }
}

/// "NVIDIA GeForce RTX 3060 (Dx12, DiscreteGpu, driver 560.94)".
fn describe_adapter(info: &wgpu::AdapterInfo) -> String {
    let driver = format!("{} {}", info.driver, info.driver_info);
    let driver = match driver.trim() {
        "" => "unknown",
        d => d,
    };
    format!(
        "{} ({:?}, {:?}, driver {driver})",
        info.name, info.backend, info.device_type
    )
}

impl Compositor {
    /// The GPU this compositor renders on (name, graphics API, type, driver).
    #[must_use]
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// Bring up a headless GPU compositor, or `None` if no adapter is available.
    #[must_use]
    pub fn new() -> Option<Self> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .ok()?;
        let adapter_desc = describe_adapter(&adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .ok()?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vuoom-composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/composite.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vuoom-composite-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vuoom-composite-pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vuoom-composite-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vuoom-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Flat-shape pipeline (highlight boxes + arrows) ──
        let shape_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vuoom-shapes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shapes.wgsl").into()),
        });
        let shape_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vuoom-shapes-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let shape_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vuoom-shapes-pl"),
            bind_group_layouts: &[&shape_bind_group_layout],
            push_constant_ranges: &[],
        });
        let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vuoom-shapes-pipeline"),
            layout: Some(&shape_pl),
            vertex: wgpu::VertexState {
                module: &shape_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ShapeVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shape_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Webcam bubble ──
        let camera_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vuoom-camera"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/camera.wgsl").into()),
        });
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vuoom-camera-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let camera_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vuoom-camera-pl"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });
        let camera_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vuoom-camera-pipeline"),
            layout: Some(&camera_pl),
            vertex: wgpu::VertexState {
                module: &camera_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &camera_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Textured rects: the pointer picture and the pointer layer ──
        let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vuoom-sprite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sprite.wgsl").into()),
        });
        // Same bindings as the webcam bubble's: uniforms, a texture and its sampler.
        let sprite_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vuoom-sprite-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let sprite_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vuoom-sprite-pl"),
            bind_group_layouts: &[&sprite_bind_group_layout],
            push_constant_ranges: &[],
        });
        let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vuoom-sprite-pipeline"),
            layout: Some(&sprite_pl),
            vertex: wgpu::VertexState {
                module: &sprite_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sprite_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let mip_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vuoom-mip-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Text (glyphon) ──
        let glyph_cache = GlyphCache::new(&device);
        let viewport = Viewport::new(&device, &glyph_cache);
        let mut text_atlas = TextAtlas::new(
            &device,
            &queue,
            &glyph_cache,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let mut font_system = FontSystem::new();
        load_bundled_fonts(&mut font_system);
        let text = Mutex::new(TextState {
            font_system,
            swash_cache: SwashCache::new(),
            atlas: text_atlas,
            viewport,
            renderer: text_renderer,
        });

        let backdrop = make_backdrop(&device, &queue, &BLANK_BACKDROP, (1, 1), 0);
        Some(Self {
            adapter: adapter_desc,
            device,
            queue,
            pipeline,
            bind_group_layout,
            shape_pipeline,
            shape_bind_group_layout,
            camera_pipeline,
            camera_bind_group_layout,
            sprite_pipeline,
            sprite_bind_group_layout,
            sampler,
            mip_sampler,
            pointer: Mutex::new(None),
            text,
            cache: Mutex::new(None),
            camera_cache: Mutex::new(None),
            backdrop: Mutex::new(backdrop),
        })
    }

    /// Set the picture drawn behind the framed recording, for frames whose
    /// [`BgFill::image`] is on: `(pixels, width, height)`, BGRA, top row first. `None` (or a
    /// picture of the wrong size) drops it.
    pub fn set_backdrop_image(&self, image: Option<(&[u8], u32, u32)>) {
        let max = self.device.limits().max_texture_dimension_2d;
        let usable = image.filter(|&(px, w, h)| {
            let sized = (1..=max).contains(&w) && (1..=max).contains(&h);
            sized && px.len() >= w as usize * h as usize * 4
        });
        let (px, w, h) = usable.unwrap_or((&BLANK_BACKDROP[..], 1, 1));
        let mut slot = self.backdrop.lock().expect("backdrop poisoned");
        let generation = slot.generation + 1;
        *slot = make_backdrop(&self.device, &self.queue, px, (w, h), generation);
    }

    /// Set the picture drawn as the pointer for scenes whose pointer look is a picture:
    /// `(pixels, width, height)`, BGRA with straight alpha, top row first. `None` (or a
    /// picture of the wrong size) drops it, and such pointers are drawn as Vuoom's own.
    pub fn set_pointer_image(&self, image: Option<(&[u8], u32, u32)>) {
        let usable = image.filter(|&(px, w, h)| {
            let sized = w > 0 && h > 0 && w.max(h) <= 16_384;
            sized && px.len() >= w as usize * h as usize * 4
        });
        let sprite = usable.map(|(px, w, h)| {
            let mut px = px[..w as usize * h as usize * 4].to_vec();
            premultiply(&mut px);
            self.make_pointer(mip_chain(px, w, h, POINTER_MAX))
        });
        *self.pointer.lock().expect("pointer poisoned") = sprite;
    }

    /// Upload a pointer picture's mip chain (largest first) as one mipmapped texture.
    fn make_pointer(&self, levels: Vec<(Vec<u8>, u32, u32)>) -> PointerSprite {
        let (width, height) = (levels[0].1, levels[0].2);
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vuoom-pointer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, (px, w, h)) in levels.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                px,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let ubuf = self.sprite_uniforms("vuoom-pointer-uniforms");
        let bind_group = self.bind_sprite(&ubuf, &view, &self.mip_sampler);
        PointerSprite {
            _tex: tex,
            width,
            height,
            ubuf,
            bind_group,
        }
    }

    /// A uniform buffer for one sprite.
    fn sprite_uniforms(&self, label: &str) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: std::mem::size_of::<SpriteUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// A sprite's bind group: its uniforms, its texture and a sampler.
    fn bind_sprite(
        &self,
        ubuf: &wgpu::Buffer,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vuoom-sprite-bg"),
            layout: &self.sprite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubuf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// A vertex buffer holding `verts`, or `None` when there are none.
    fn vertex_buffer(&self, verts: &[ShapeVertex]) -> Option<wgpu::Buffer> {
        if verts.is_empty() {
            return None;
        }
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-shape-verts"),
            size: std::mem::size_of_val(verts) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&buf, 0, bytemuck::cast_slice(verts));
        Some(buf)
    }

    /// The composite pass's bind group: uniforms, the source frame, the sampler and the
    /// backdrop picture.
    fn bind_composite(
        &self,
        ubuf: &wgpu::Buffer,
        src: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vuoom-composite-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubuf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
            ],
        })
    }

    fn offscreen(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vuoom-offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Read a `COPY_SRC` RGBA texture back into tightly-packed RGBA8 bytes. Only the
    /// `clear_to_rgba` smoke test uses this; the hot path uses `read_back_into`.
    #[cfg(test)]
    fn read_back(&self, texture: &wgpu::Texture, width: u32, height: u32) -> Vec<u8> {
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait);

        let data = slice.get_mapped_range();
        let mut out = Vec::with_capacity((unpadded * height) as usize);
        for row in 0..height {
            let start = (row * padded) as usize;
            out.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buffer.unmap();
        out
    }

    /// Copy a `COPY_SRC` RGBA texture into the caller-owned (cached) readback buffer and return
    /// tightly-packed RGBA8 bytes. Mirrors `read_back` but reuses `buffer` instead of
    /// allocating one per call. `padded` must equal the buffer's `bytes_per_row`.
    fn read_back_into(
        &self,
        texture: &wgpu::Texture,
        buffer: &wgpu::Buffer,
        padded: u32,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let unpadded = width * 4;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait);

        let data = slice.get_mapped_range();
        let len = (unpadded * height) as usize;
        // A little spare room, so the preview can append its frame trailer in place.
        let mut out = Vec::with_capacity(len + READBACK_SPARE);
        if padded == unpadded {
            // No row padding (widths that are a multiple of 64): one straight copy.
            out.extend_from_slice(&data[..len]);
        } else {
            for row in 0..height {
                let start = (row * padded) as usize;
                out.extend_from_slice(&data[start..start + unpadded as usize]);
            }
        }
        drop(data);
        buffer.unmap();
        out
    }

    /// Build the size-keyed resources for `composite_scene` at the given dimensions. Per-frame
    /// data (source pixels, uniforms) is streamed into these afterwards via `queue.write_*`.
    fn build_cache(
        &self,
        src_w: u32,
        src_h: u32,
        out_w: u32,
        out_h: u32,
        backdrop: &Backdrop,
    ) -> CompositeCache {
        let src_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vuoom-source"),
            size: wgpu::Extent3d {
                width: src_w,
                height: src_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let src_view = src_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.bind_composite(&ubuf, &src_view, &backdrop.view);

        let shape_ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-shape-uniforms"),
            size: std::mem::size_of::<ShapeUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shape_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vuoom-shape-bg"),
            layout: &self.shape_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: shape_ubuf.as_entire_binding(),
            }],
        });

        let target = self.offscreen(out_w, out_h);
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let layer = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vuoom-pointer-layer"),
            size: wgpu::Extent3d {
                width: out_w,
                height: out_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let layer_view = layer.create_view(&wgpu::TextureViewDescriptor::default());
        let layer_ubuf = self.sprite_uniforms("vuoom-layer-uniforms");
        let layer_bind_group = self.bind_sprite(&layer_ubuf, &layer_view, &self.sampler);

        let padded_bpr = (out_w * 4).div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-readback"),
            size: u64::from(padded_bpr) * u64::from(out_h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        CompositeCache {
            src_w,
            src_h,
            out_w,
            out_h,
            src_tex,
            src_view,
            ubuf,
            bind_group,
            backdrop_generation: backdrop.generation,
            shape_ubuf,
            shape_bind_group,
            target,
            target_view,
            _layer: layer,
            layer_view,
            layer_ubuf,
            layer_bind_group,
            readback,
            padded_bpr,
        }
    }

    /// Build the webcam bubble's texture and bind group for `width`×`height` frames.
    fn build_camera_cache(&self, width: u32, height: u32) -> CameraCache {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vuoom-camera-frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-camera-uniforms"),
            size: std::mem::size_of::<CameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vuoom-camera-bg"),
            layout: &self.camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubuf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        CameraCache {
            width,
            height,
            tex,
            ubuf,
            bind_group,
        }
    }

    /// Upload a webcam frame and the bubble's placement. Returns whether there is a bubble
    /// to draw (the scene has one and a frame was given).
    fn prepare_camera(
        &self,
        guard: &mut Option<CameraCache>,
        scene: &Scene,
        image: Option<CameraImage<'_>>,
        out: (u32, u32),
    ) -> bool {
        let (Some(bubble), Some(img)) = (scene.camera, image) else {
            return false;
        };
        let expected = img.width as usize * img.height as usize * 4;
        if img.width == 0 || img.height == 0 || img.bgra.len() < expected {
            return false;
        }
        if !guard
            .as_ref()
            .is_some_and(|c| c.width == img.width && c.height == img.height)
        {
            *guard = Some(self.build_camera_cache(img.width, img.height));
        }
        let Some(cache) = guard.as_ref() else {
            return false;
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &cache.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img.bgra[..expected],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(img.height),
            },
            wgpu::Extent3d {
                width: img.width,
                height: img.height,
                depth_or_array_layers: 1,
            },
        );
        let aspect = bubble.w / bubble.h.max(1e-9);
        let (uv_min, uv_size) = camera_crop(img.width, img.height, aspect, bubble.mirror);
        let uniforms = CameraUniforms {
            out_size: [out.0 as f32, out.1 as f32],
            rect_min: [bubble.x as f32, bubble.y as f32],
            rect_size: [bubble.w as f32, bubble.h as f32],
            uv_min,
            uv_size,
            radius: bubble.radius as f32,
            shadow: (bubble.h * 0.06).max(2.0) as f32,
        };
        self.queue
            .write_buffer(&cache.ubuf, 0, bytemuck::bytes_of(&uniforms));
        true
    }

    /// Render an offscreen RGBA texture cleared to `color` and read it back (a smoke test
    /// for the device + render-pass + readback path).
    #[cfg(test)]
    #[must_use]
    pub fn clear_to_rgba(&self, width: u32, height: u32, color: [f64; 4]) -> Vec<u8> {
        let texture = self.offscreen(width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vuoom-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: color[0],
                            g: color[1],
                            b: color[2],
                            a: color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
        self.read_back(&texture, width, height)
    }

    /// Composite a frame and overlay the scene's shape annotations (highlight boxes and
    /// arrows). Text is drawn by the separate glyphon pass. Returns RGBA8 bytes.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn composite_scene(
        &self,
        source_bgra: &[u8],
        src_w: u32,
        src_h: u32,
        out_w: u32,
        out_h: u32,
        scene: &Scene,
        bg: BgFill,
        camera: Option<CameraImage<'_>>,
    ) -> Vec<u8> {
        let layout = &scene.layout;

        // Reuse the size-keyed GPU resources when the dimensions haven't changed (the common
        // case for an export loop); rebuild them only when a dimension differs. The guard is
        // held for the whole call, serializing composites, matching the pre-existing text
        // Mutex, so the single cached source/target/readback are never used concurrently.
        let mut cache_guard = self.cache.lock().expect("composite cache poisoned");
        let backdrop = self.backdrop.lock().expect("backdrop poisoned");
        if !cache_guard
            .as_ref()
            .is_some_and(|c| c.matches(src_w, src_h, out_w, out_h))
        {
            *cache_guard = Some(self.build_cache(src_w, src_h, out_w, out_h, &backdrop));
        }
        let cache = cache_guard.as_mut().expect("cache just populated");
        // A new backdrop picture since the bind group was made: bind it instead.
        if cache.backdrop_generation != backdrop.generation {
            cache.bind_group = self.bind_composite(&cache.ubuf, &cache.src_view, &backdrop.view);
            cache.backdrop_generation = backdrop.generation;
        }
        let cache = &*cache;

        // Stream this frame's source pixels into the cached source texture.
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &cache.src_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            source_bgra,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(src_w * 4),
                rows_per_image: Some(src_h),
            },
            wgpu::Extent3d {
                width: src_w,
                height: src_h,
                depth_or_array_layers: 1,
            },
        );

        let prev = scene.blur_from.unwrap_or(layout.src_rect);
        let max_taps = if scene.blur_from.is_some() {
            MAX_TAPS_BLURRED
        } else {
            MAX_TAPS
        };
        let (src, dst, shadow) = (layout.src_rect, layout.dst_rect, layout.shadow);
        let uniforms = Uniforms {
            out_size: [out_w as f32, out_h as f32],
            src_min: [layout.src_rect.x as f32, layout.src_rect.y as f32],
            src_size: [layout.src_rect.w as f32, layout.src_rect.h as f32],
            dst_min: [layout.dst_rect.x as f32, layout.dst_rect.y as f32],
            dst_size: [layout.dst_rect.w as f32, layout.dst_rect.h as f32],
            corner_px: layout.corner_radius_px as f32,
            _pad: 0.0,
            bg: bg.color,
            bg2: bg.color2,
            bg_dir: bg.dir,
            _pad2: [0.0, 0.0],
            prev_min: [prev.x as f32, prev.y as f32],
            prev_size: [prev.w as f32, prev.h as f32],
            blur: if scene.blur_from.is_some() { 1.0 } else { 0.0 },
            bg_image: if bg.image { 1.0 } else { 0.0 },
            bg_scale: cover_scale((out_w, out_h), (backdrop.width, backdrop.height)),
            shadow: [
                shadow.dx as f32,
                shadow.dy as f32,
                shadow.blur as f32,
                shadow.strength as f32,
            ],
            taps: [
                area_taps(src.w, src_w, dst.w, max_taps),
                area_taps(src.h, src_h, dst.h, max_taps),
            ],
            _pad3: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&cache.ubuf, 0, bytemuck::bytes_of(&uniforms));

        let mut camera_guard = self.camera_cache.lock().expect("camera cache poisoned");
        let bubble = self.prepare_camera(&mut camera_guard, scene, camera, (out_w, out_h));

        // Prepare text labels (glyphon), before the shapes: the caption's plate is sized to
        // its measured text.
        let mut text_guard = self.text.lock().expect("text state poisoned");
        let TextState {
            font_system,
            swash_cache,
            atlas,
            viewport,
            renderer,
        } = &mut *text_guard;
        viewport.update(
            &self.queue,
            Resolution {
                width: out_w,
                height: out_h,
            },
        );
        // Annotation labels + keystroke-overlay labels share one glyph pass.
        let labels: Vec<&crate::scene::ResolvedText> =
            scene.texts.iter().chain(&scene.key_texts).collect();
        let mut text_buffers = Vec::with_capacity(labels.len());
        for label in &labels {
            let metrics = Metrics::new(label.font_px as f32, label.font_px as f32 * 1.25);
            let mut buf = TextBuffer::new(font_system, metrics);
            let family = if label.font.is_empty() {
                Family::SansSerif
            } else {
                Family::Name(label.font.as_str())
            };
            let mut attrs = Attrs::new().family(family);
            if label.bold {
                attrs = attrs.weight(Weight::BOLD);
            }
            if label.italic {
                attrs = attrs.style(Style::Italic);
            }
            buf.set_text(font_system, &label.text, &attrs, Shaping::Advanced);
            text_buffers.push(buf);
        }
        let caption = scene
            .caption
            .as_ref()
            .map(|c| shape_caption(font_system, c));
        let mut verts = build_shape_vertices(scene, caption.as_ref().map(|c| &c.plate));
        let shape_uniforms = ShapeUniforms {
            out_size: [out_w as f32, out_h as f32],
            _pad: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&cache.shape_ubuf, 0, bytemuck::bytes_of(&shape_uniforms));

        // The pointer, over everything it points at: the user's picture, or Vuoom's drawn
        // pointer (which also stands in for a picture that couldn't be loaded). A drawn
        // pointer shown see-through goes to its own layer first and is faded on as one.
        let pointer_guard = self.pointer.lock().expect("pointer poisoned");
        let mut picture = None;
        let mut layer_verts = Vec::new();
        if let Some(mut c) = scene.cursor {
            let wants_picture = c.look == PointerLook::Image;
            match (wants_picture, pointer_guard.as_ref()) {
                (true, Some(pic)) => {
                    let uniforms = picture_uniforms(&c, (pic.width, pic.height), (out_w, out_h));
                    self.queue
                        .write_buffer(&pic.ubuf, 0, bytemuck::bytes_of(&uniforms));
                    picture = Some(pic);
                }
                _ => {
                    if wants_picture {
                        c.look = PointerLook::Classic;
                    }
                    let body = pointer_vertices(&c);
                    let opacity = c.group_opacity();
                    if opacity < 0.999 {
                        let full = [out_w as f32, out_h as f32];
                        let uniforms = SpriteUniforms {
                            out_size: full,
                            rect_size: full,
                            opacity: opacity as f32,
                            ..SpriteUniforms::default()
                        };
                        self.queue.write_buffer(
                            &cache.layer_ubuf,
                            0,
                            bytemuck::bytes_of(&uniforms),
                        );
                        layer_verts = body;
                    } else {
                        verts.extend(body);
                    }
                }
            }
        }
        // Vertices vary in count per frame, so these buffers stay per-frame.
        let shape_vbuf = self.vertex_buffer(&verts);
        let layer_vbuf = self.vertex_buffer(&layer_verts);

        let mut text_areas: Vec<TextArea> = text_buffers
            .iter()
            .zip(&labels)
            .map(|(buf, label)| TextArea {
                buffer: buf,
                left: label.x as f32,
                top: label.y as f32,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: out_w as i32,
                    bottom: out_h as i32,
                },
                default_color: GlyphColor::rgb(
                    (label.color.r * 255.0) as u8,
                    (label.color.g * 255.0) as u8,
                    (label.color.b * 255.0) as u8,
                ),
                custom_glyphs: &[],
            })
            .collect();
        if let Some(c) = &caption {
            text_areas.push(TextArea {
                buffer: &c.buffer,
                left: c.left,
                top: c.top,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: out_w as i32,
                    bottom: out_h as i32,
                },
                default_color: GlyphColor::rgb(255, 255, 255),
                custom_glyphs: &[],
            });
        }
        let _ = renderer.prepare(
            &self.device,
            &self.queue,
            font_system,
            atlas,
            viewport,
            text_areas,
            swash_cache,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        if let Some(vbuf) = &layer_vbuf {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vuoom-pointer-layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cache.layer_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shape_pipeline);
            pass.set_bind_group(0, &cache.shape_bind_group, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..layer_verts.len() as u32, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vuoom-composite-scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cache.target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &cache.bind_group, &[]);
            pass.draw(0..3, 0..1);
            // The webcam bubble over the recording, under annotations and the pointer.
            if let Some(cam) = camera_guard.as_ref().filter(|_| bubble) {
                pass.set_pipeline(&self.camera_pipeline);
                pass.set_bind_group(0, &cam.bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
            if let Some(vbuf) = &shape_vbuf {
                pass.set_pipeline(&self.shape_pipeline);
                pass.set_bind_group(0, &cache.shape_bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..verts.len() as u32, 0..1);
            }
            // The pointer picture, or the see-through pointer's layer.
            let sprite = match picture {
                Some(pic) => Some(&pic.bind_group),
                None => layer_vbuf.as_ref().map(|_| &cache.layer_bind_group),
            };
            if let Some(bind_group) = sprite {
                pass.set_pipeline(&self.sprite_pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
            let _ = renderer.render(atlas, viewport, &mut pass);
        }
        self.queue.submit(Some(encoder.finish()));
        self.read_back_into(
            &cache.target,
            &cache.readback,
            cache.padded_bpr,
            out_w,
            out_h,
        )
    }
}

/// A caption ready to draw: its shaped text, the plate behind it, and where the text goes.
struct CaptionText {
    buffer: TextBuffer,
    plate: ResolvedHighlight,
    left: f32,
    top: f32,
}

/// Wrap a caption to its width, center every line, and measure it to size its plate.
fn shape_caption(fs: &mut FontSystem, c: &ResolvedCaption) -> CaptionText {
    let line_h = c.font_px * ResolvedCaption::LINE;
    let metrics = Metrics::new(c.font_px as f32, line_h as f32);
    let mut buffer = TextBuffer::new(fs, metrics);
    buffer.set_size(fs, Some(c.max_w as f32), None);
    let sans = Attrs::new().family(Family::SansSerif);
    let attrs = sans.weight(Weight::SEMIBOLD);
    buffer.set_text(fs, &c.text, &attrs, Shaping::Advanced);
    for line in &mut buffer.lines {
        line.set_align(Some(glyphon::cosmic_text::Align::Center));
    }
    buffer.shape_until_scroll(fs, false);
    let mut widest = 0.0_f64;
    let mut lines = 0_u32;
    for run in buffer.layout_runs() {
        widest = widest.max(f64::from(run.line_w));
        lines += 1;
    }
    let text_h = f64::from(lines.max(1)) * line_h;
    let pad_x = c.font_px * ResolvedCaption::PAD_X;
    let pad_y = c.font_px * ResolvedCaption::PAD_Y;
    let top = if c.from_top {
        c.edge_y + pad_y
    } else {
        c.edge_y - pad_y - text_h
    };
    let plate_w = widest + 2.0 * pad_x;
    let plate = ResolvedHighlight {
        x: c.center_x - plate_w / 2.0,
        y: top - pad_y,
        w: plate_w,
        h: text_h + 2.0 * pad_y,
        thickness_px: 0.0,
        filled: true,
        ellipse: false,
        color: Color::rgb(0.04, 0.04, 0.05).with_alpha(0.75),
    };
    CaptionText {
        buffer,
        plate,
        left: (c.center_x - c.max_w / 2.0) as f32,
        top: top as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_smaller_picture_takes_more_samples() {
        // 1:1, magnified, and a hair over 1:1: a single sample.
        assert_eq!(area_taps(1.0, 1920, 1920.0, MAX_TAPS), 1.0);
        assert_eq!(area_taps(0.5, 1920, 1920.0, MAX_TAPS), 1.0);
        assert_eq!(area_taps(1.0, 1921, 1920.0, MAX_TAPS), 1.0);
        // Half size: two source pixels per output pixel, two samples.
        assert_eq!(area_taps(1.0, 1920, 960.0, MAX_TAPS), 2.0);
        // 4K down to 1000 px: capped.
        assert_eq!(area_taps(1.0, 3840, 1000.0, MAX_TAPS), 4.0);
        assert_eq!(area_taps(1.0, 3840, 1000.0, MAX_TAPS_BLURRED), 2.0);
    }

    #[test]
    fn a_backdrop_picture_covers_the_frame() {
        let [x, y] = cover_scale((1920, 1080), (1920, 1080));
        assert!((x - 1.0).abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        // A square picture behind a wide frame: all its width, the middle of its height.
        let [x, y] = cover_scale((1600, 900), (1000, 1000));
        assert!((x - 1.0).abs() < 1e-6 && (y - 0.5625).abs() < 1e-6);
        // A wide picture behind a tall frame: the middle of its width.
        let [x, y] = cover_scale((900, 1600), (1600, 900));
        assert!((x - 0.316_406_25).abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn camera_frames_are_center_cropped_and_mirrored() {
        // 16:9 into a circle: the middle square.
        let (min, size) = camera_crop(1600, 900, 1.0, false);
        assert!((size[0] - 0.5625).abs() < 1e-6);
        assert!((size[1] - 1.0).abs() < 1e-6);
        assert!((min[0] - 0.21875).abs() < 1e-6);
        assert!(min[1].abs() < 1e-6);
        // Mirrored: the same span, right to left.
        let (min, size) = camera_crop(1600, 900, 1.0, true);
        assert!((min[0] - 0.78125).abs() < 1e-6);
        assert!((size[0] + 0.5625).abs() < 1e-6);
        // 4:3 into 16:9: a band across the middle.
        let (min, size) = camera_crop(640, 480, 16.0 / 9.0, false);
        assert!((size[0] - 1.0).abs() < 1e-6);
        assert!((size[1] - 0.75).abs() < 1e-6);
        assert!((min[1] - 0.125).abs() < 1e-6);
    }
    use crate::layout::{CompositeLayout, NormRect, PxRect};
    use crate::scene::Scene;

    #[test]
    fn clear_renders_and_reads_back() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter (CI without a GPU), skipping");
            return;
        };
        let px = compositor.clear_to_rgba(2, 2, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(px.len(), 2 * 2 * 4);
        assert!(px[0] > 200 && px[1] < 50 && px[2] < 50 && px[3] > 200);
    }

    #[test]
    fn composite_produces_full_frame() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter (CI without a GPU), skipping");
            return;
        };
        let source = vec![255u8; 4 * 4 * 4]; // 4x4 white BGRA
        let layout = CompositeLayout {
            src_rect: NormRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            dst_rect: PxRect {
                x: 1.0,
                y: 1.0,
                w: 6.0,
                h: 6.0,
            },
            corner_radius_px: 1.0,
            shadow: crate::layout::PxShadow::default(),
        };
        // Minimal scene (no annotations) exercises the same composite pipeline as export.
        let scene = Scene {
            layout,
            texts: Vec::new(),
            arrows: Vec::new(),
            highlights: Vec::new(),
            spot_corners: Vec::new(),
            strokes: Vec::new(),
            ripples: Vec::new(),
            key_chips: Vec::new(),
            key_texts: Vec::new(),
            cursor: None,
            camera: None,
            caption: None,
            blur_from: None,
        };
        let px = compositor.composite_scene(
            &source,
            4,
            4,
            8,
            8,
            &scene,
            BgFill::solid([0.0, 0.0, 0.0, 1.0]),
            None,
        );
        assert_eq!(px.len(), 8 * 8 * 4);
    }

    #[test]
    fn pictures_premultiply_and_halve_evenly() {
        let mut px = vec![200, 100, 50, 128, 9, 9, 9, 0];
        premultiply(&mut px);
        assert_eq!(px, [100, 50, 25, 128, 0, 0, 0, 0]);

        // A 3×2 picture halves to 1×1: the odd column is left out.
        let px: Vec<u8> = [10u8, 20, 30, 40, 50, 60]
            .iter()
            .flat_map(|&v| [v; 4])
            .collect();
        let (small, w, h) = halve(&px, 3, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(small, [30; 4]);

        // A tall picture is shrunk to fit, then mipmapped down to a single pixel.
        let levels = mip_chain(vec![255; 40 * 1200 * 4], 40, 1200, POINTER_MAX);
        let sizes: Vec<(u32, u32)> = levels.iter().map(|l| (l.1, l.2)).collect();
        assert_eq!(sizes[0], (10, 300));
        assert_eq!(*sizes.last().unwrap(), (1, 1));
        let whole = |l: &(Vec<u8>, u32, u32)| l.0.len() == (l.1 * l.2 * 4) as usize;
        assert!(levels.iter().all(whole));
    }

    fn cursor_at_center(look: PointerLook, alpha: f64) -> ResolvedCursor {
        ResolvedCursor {
            x: 32.0,
            y: 32.0,
            size: 40.0,
            press: 0.0,
            opacity: 1.0,
            shape: vuoom_project::PointerShape::Arrow,
            color: None,
            halo: false,
            look,
            hotspot: [0.5, 0.5],
            alpha,
            shadow: true,
        }
    }

    #[test]
    fn a_picture_sits_on_its_hotspot() {
        let c = cursor_at_center(PointerLook::Image, 0.5);
        let u = picture_uniforms(&c, (20, 10), (64, 64));
        // As tall as the pointer, twice as wide, centered on the point.
        assert_eq!(u.rect_size, [80.0, 40.0]);
        assert_eq!(u.rect_min, [-8.0, 12.0]);
        assert!((u.opacity - 0.5).abs() < 1e-6);
        assert!(u.shadow_alpha > 0.0);
        let flat = ResolvedCursor { shadow: false, ..c };
        let flat = picture_uniforms(&flat, (20, 10), (64, 64));
        assert!(flat.shadow_alpha.abs() < f32::EPSILON);
    }

    /// The output pixel at the center of a 64×64 frame of black with `cursor` on it.
    fn center_pixel(compositor: &Compositor, cursor: ResolvedCursor) -> [u8; 4] {
        let source = [0u8, 0, 0, 255].repeat(64 * 64);
        let layout = CompositeLayout {
            src_rect: NormRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            dst_rect: PxRect {
                x: 0.0,
                y: 0.0,
                w: 64.0,
                h: 64.0,
            },
            corner_radius_px: 0.0,
            shadow: crate::layout::PxShadow::default(),
        };
        let scene = Scene {
            layout,
            texts: Vec::new(),
            arrows: Vec::new(),
            highlights: Vec::new(),
            spot_corners: Vec::new(),
            strokes: Vec::new(),
            ripples: Vec::new(),
            key_chips: Vec::new(),
            key_texts: Vec::new(),
            cursor: Some(cursor),
            camera: None,
            caption: None,
            blur_from: None,
        };
        let bg = BgFill::solid([0.0, 0.0, 0.0, 1.0]);
        let px = compositor.composite_scene(&source, 64, 64, 64, 64, &scene, bg, None);
        let i = (32 * 64 + 32) * 4;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn a_see_through_pointer_fades_as_one() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter (CI without a GPU), skipping");
            return;
        };
        // An opaque white dot covers the point.
        let solid = center_pixel(&compositor, cursor_at_center(PointerLook::Dot, 1.0));
        assert!(solid[..3].iter().all(|&v| v > 245), "{solid:?}");
        // Half see-through: half white over black. Its dark outline, under the body, must
        // not show through and darken it.
        let half = center_pixel(&compositor, cursor_at_center(PointerLook::Dot, 0.5));
        let mid = |v: &u8| (118..=138).contains(v);
        assert!(half[..3].iter().all(mid), "{half:?}");
    }

    #[test]
    fn a_picture_pointer_draws_its_picture() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter (CI without a GPU), skipping");
            return;
        };
        // No picture loaded yet: Vuoom's own arrow stands in (its tip is at the center,
        // so the pixel there is the arrow's light body or outline, not red).
        let fallback = center_pixel(&compositor, cursor_at_center(PointerLook::Image, 1.0));
        assert!(fallback[0] < 200 || fallback[1] > 100, "{fallback:?}");
        // An opaque red picture, BGRA.
        let red = [0u8, 0, 255, 255].repeat(8 * 8);
        compositor.set_pointer_image(Some((&red, 8, 8)));
        let px = center_pixel(&compositor, cursor_at_center(PointerLook::Image, 1.0));
        assert!(px[0] > 245 && px[1] < 10 && px[2] < 10, "{px:?}");
        let half = center_pixel(&compositor, cursor_at_center(PointerLook::Image, 0.5));
        assert!((118..=138).contains(&half[0]) && half[1] < 10, "{half:?}");
        // Dropped: back to the drawn pointer.
        compositor.set_pointer_image(None);
        let again = center_pixel(&compositor, cursor_at_center(PointerLook::Image, 1.0));
        assert_eq!(again, fallback);
    }
}
