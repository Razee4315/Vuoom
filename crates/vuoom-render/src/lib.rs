//! The wgpu compositor — the visual heart.
//!
//! One `wgpu::Device` on the DX12 backend, one compositor, two sinks (preview + export).
//! Render graph: background → source(zoom/pan) → rounded-corner SDF + shadow → motion blur
//! → cursor → text (glyphon) + arrows/highlights (lyon). Renders to an offscreen RGBA
//! texture. See `docs/05-Compositing-and-Preview.md`.

// TODO(S2 spike): build the device (DX12), render a test pattern to an offscreen texture,
// hand the RGBA readback to vuoom-preview.
