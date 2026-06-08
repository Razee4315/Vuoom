//! Headless wgpu device + offscreen render & readback — the GPU compositor core.
//!
//! Step one: bring up a headless `Device`/`Queue`, render to an offscreen RGBA texture,
//! and read it back (handling the 256-byte row alignment). The composite pipeline (source
//! sampling, rounded-corner SDF in `shaders/composite.wgsl`, then glyphon text + lyon
//! shapes) layers on top. See `docs/05-Compositing-and-Preview.md`.
//!
//! `new()` returns `None` when no adapter is available (e.g. a CI runner without a GPU),
//! so tests skip gracefully rather than fail.

/// A headless GPU context for offscreen compositing.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Compositor {
    /// Create a headless GPU context, or `None` if no adapter is available.
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
        Some(Self { device, queue })
    }

    /// Render an offscreen RGBA texture cleared to `color` (`0.0..=1.0` per channel) and
    /// read it back as tightly-packed RGBA8 bytes (`width * height * 4`).
    #[must_use]
    pub fn clear_to_rgba(&self, width: u32, height: u32, color: [f64; 4]) -> Vec<u8> {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vuoom-offscreen"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Readback buffer; rows padded to 256 bytes (wgpu requirement).
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
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
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
            size,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_renders_and_reads_back() {
        let Some(compositor) = Compositor::new() else {
            eprintln!("no GPU adapter available (CI without a GPU) — skipping");
            return;
        };
        let px = compositor.clear_to_rgba(2, 2, [1.0, 0.0, 0.0, 1.0]); // red
        assert_eq!(px.len(), 2 * 2 * 4);
        // First pixel should read back as opaque red.
        assert!(px[0] > 200 && px[1] < 50 && px[2] < 50 && px[3] > 200);
    }
}
