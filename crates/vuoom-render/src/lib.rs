//! The Vuoom compositor.
//!
//! Today: the pure, GPU-free [`layout`] math (per-frame source crop + destination rect +
//! corner radius) that the wgpu pipeline will consume. Next: a single `wgpu::Device` on
//! the DX12 backend rendering background → source(zoom/pan) → rounded-corner SDF + shadow
//! → cursor → text (glyphon) + annotations (lyon) into an offscreen RGBA texture, shared
//! by the preview and GIF-export sinks. See `docs/05-Compositing-and-Preview.md`.

mod compositor;
mod layout;
mod scene;

pub use compositor::Compositor;

pub use layout::{
    camera_src_rect, compute_layout, content_rect, CompositeLayout, NormRect, PxRect,
};
pub use scene::{build_scene, ResolvedArrow, ResolvedHighlight, ResolvedText, Scene};
