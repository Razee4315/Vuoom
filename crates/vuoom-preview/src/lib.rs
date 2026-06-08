//! Live-preview bridge (the hard architectural problem, solved).
//!
//! Pixels never cross JSON IPC. Composited RGBA is read back, packed with a trailing
//! `[stride, height, width, frame#, t_ns]` trailer ([`protocol`]), and pushed as binary
//! frames over a `127.0.0.1` WebSocket ("latest frame wins", [`PreviewServer`]). A Web
//! Worker uploads to a WebGPU canvas. Scrubbing is a cheap `seekTo` Tauri command.
//! See `docs/05-Compositing-and-Preview.md`.

mod protocol;
mod server;

pub use protocol::{pack_frame, parse_meta, payload, FrameMeta, META_LEN};
pub use server::{FrameSink, PreviewServer};
