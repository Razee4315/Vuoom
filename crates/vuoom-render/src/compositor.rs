//! Headless wgpu compositor: device bring-up, the composite pipeline (background +
//! zoom/pan-cropped source + rounded-corner SDF, `shaders/composite.wgsl`), and offscreen
//! render + readback. Text (glyphon) and shapes (lyon) layer on top later.
//! See `docs/05-Compositing-and-Preview.md`.
//!
//! `new()` returns `None` when no GPU adapter is available (e.g. a CI runner without a
//! GPU), so tests skip gracefully rather than fail.

use crate::scene::Scene;
use crate::shapes::{build_shape_vertices, ShapeVertex};
use glyphon::{
    Attrs, Buffer as TextBuffer, Cache as GlyphCache, Color as GlyphColor, Family, FontSystem,
    Metrics, Resolution, Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer,
    Viewport, Weight,
};
use std::sync::Mutex;

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
/// the same (non-linearized) space the target texture is written in — matching the existing
/// solid-fill and source `mix` handling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgFill {
    pub color: [f32; 4],
    pub color2: [f32; 4],
    pub dir: [f32; 2],
}

impl BgFill {
    /// A flat solid fill (both stops equal, direction irrelevant).
    #[must_use]
    pub fn solid(color: [f32; 4]) -> Self {
        Self {
            color,
            color2: color,
            dir: [0.0, 1.0],
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
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShapeUniforms {
    out_size: [f32; 2],
    _pad: [f32; 2],
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
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    shape_ubuf: wgpu::Buffer,
    shape_bind_group: wgpu::BindGroup,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
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
    sampler: wgpu::Sampler,
    text: Mutex<TextState>,
    /// Reused size-keyed resources for the hot `composite_scene` path. `None` until the first
    /// composite; rebuilt whenever the frame dimensions change.
    cache: Mutex<Option<CompositeCache>>,
}

impl Compositor {
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

        Some(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            shape_pipeline,
            shape_bind_group_layout,
            sampler,
            text,
            cache: Mutex::new(None),
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
        let mut out = Vec::with_capacity((unpadded * height) as usize);
        for row in 0..height {
            let start = (row * padded) as usize;
            out.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buffer.unmap();
        out
    }

    /// Build the size-keyed resources for `composite_scene` at the given dimensions. Per-frame
    /// data (source pixels, uniforms) is streamed into these afterwards via `queue.write_*`.
    fn build_cache(&self, src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> CompositeCache {
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
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vuoom-composite-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubuf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

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
            ubuf,
            bind_group,
            shape_ubuf,
            shape_bind_group,
            target,
            target_view,
            readback,
            padded_bpr,
        }
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
    ) -> Vec<u8> {
        let layout = &scene.layout;

        // Reuse the size-keyed GPU resources when the dimensions haven't changed (the common
        // case for an export loop); rebuild them only when a dimension differs. The guard is
        // held for the whole call, serializing composites — matching the pre-existing text
        // Mutex — so the single cached source/target/readback are never used concurrently.
        let mut cache_guard = self.cache.lock().expect("composite cache poisoned");
        if !cache_guard
            .as_ref()
            .is_some_and(|c| c.matches(src_w, src_h, out_w, out_h))
        {
            *cache_guard = Some(self.build_cache(src_w, src_h, out_w, out_h));
        }
        let cache = cache_guard.as_ref().expect("cache just populated");

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
        };
        self.queue
            .write_buffer(&cache.ubuf, 0, bytemuck::bytes_of(&uniforms));

        let verts = build_shape_vertices(scene);
        let shape_uniforms = ShapeUniforms {
            out_size: [out_w as f32, out_h as f32],
            _pad: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&cache.shape_ubuf, 0, bytemuck::bytes_of(&shape_uniforms));
        // Shape vertices vary in count per frame, so this buffer stays per-frame.
        let shape_vbuf = if verts.is_empty() {
            None
        } else {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vuoom-shape-verts"),
                size: (verts.len() * std::mem::size_of::<ShapeVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&buf, 0, bytemuck::cast_slice(&verts));
            Some(buf)
        };

        // Prepare text labels (glyphon).
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
        let text_areas: Vec<TextArea> = text_buffers
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
            if let Some(vbuf) = &shape_vbuf {
                pass.set_pipeline(&self.shape_pipeline);
                pass.set_bind_group(0, &cache.shape_bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..verts.len() as u32, 0..1);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{CompositeLayout, NormRect, PxRect};
    use crate::scene::Scene;

    #[test]
    fn clear_renders_and_reads_back() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter (CI without a GPU) — skipping");
            return;
        };
        let px = compositor.clear_to_rgba(2, 2, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(px.len(), 2 * 2 * 4);
        assert!(px[0] > 200 && px[1] < 50 && px[2] < 50 && px[3] > 200);
    }

    #[test]
    fn composite_produces_full_frame() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter (CI without a GPU) — skipping");
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
        };
        // Minimal scene (no annotations) exercises the same composite pipeline as export.
        let scene = Scene {
            layout,
            texts: Vec::new(),
            arrows: Vec::new(),
            highlights: Vec::new(),
            ripples: Vec::new(),
            key_chips: Vec::new(),
            key_texts: Vec::new(),
        };
        let px = compositor.composite_scene(
            &source,
            4,
            4,
            8,
            8,
            &scene,
            BgFill::solid([0.0, 0.0, 0.0, 1.0]),
        );
        assert_eq!(px.len(), 8 * 8 * 4);
    }
}
