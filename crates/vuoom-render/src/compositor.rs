//! Headless wgpu compositor: device bring-up, the composite pipeline (background +
//! zoom/pan-cropped source + rounded-corner SDF, `shaders/composite.wgsl`), and offscreen
//! render + readback. Text (glyphon) and shapes (lyon) layer on top later.
//! See `docs/05-Compositing-and-Preview.md`.
//!
//! `new()` returns `None` when no GPU adapter is available (e.g. a CI runner without a
//! GPU), so tests skip gracefully rather than fail.

use crate::layout::CompositeLayout;

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
}

/// A headless GPU compositor.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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

        Some(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            sampler,
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

    /// Read a `COPY_SRC` RGBA texture back into tightly-packed RGBA8 bytes.
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

    /// Render an offscreen RGBA texture cleared to `color` and read it back (a smoke test
    /// for the device + render-pass + readback path).
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

    /// Composite a BGRA source frame into an `out_w`×`out_h` RGBA frame: styled background
    /// + the zoom/pan-cropped source inside the rounded-corner frame from `layout`.
    /// Returns tightly-packed RGBA8 bytes.
    #[must_use]
    pub fn composite(
        &self,
        source_bgra: &[u8],
        src_w: u32,
        src_h: u32,
        out_w: u32,
        out_h: u32,
        layout: &CompositeLayout,
        bg: [f32; 4],
    ) -> Vec<u8> {
        // Upload the source as a BGRA texture.
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
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &src_tex,
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
        let src_view = src_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let uniforms = Uniforms {
            out_size: [out_w as f32, out_h as f32],
            src_min: [layout.src_rect.x as f32, layout.src_rect.y as f32],
            src_size: [layout.src_rect.w as f32, layout.src_rect.h as f32],
            dst_min: [layout.dst_rect.x as f32, layout.dst_rect.y as f32],
            dst_size: [layout.dst_rect.w as f32, layout.dst_rect.h as f32],
            corner_px: layout.corner_radius_px as f32,
            _pad: 0.0,
            bg,
        };
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vuoom-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&ubuf, 0, bytemuck::bytes_of(&uniforms));

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

        let target = self.offscreen(out_w, out_h);
        let tview = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vuoom-composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &tview,
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
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        self.read_back(&target, out_w, out_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{CompositeLayout, NormRect, PxRect};

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
        let px = compositor.composite(&source, 4, 4, 8, 8, &layout, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(px.len(), 8 * 8 * 4);
    }
}
