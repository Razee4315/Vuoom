//! GIF export.
//!
//! v1 exports GIF only. Composited RGBA frames are downscaled, frame-selected to the
//! target fps, and fed to **gifski**, which ships as a separate bundled binary invoked
//! out-of-process (gifski is AGPL; invoking it as a separate process keeps Vuoom's own
//! code Apache-2.0 — NEVER link the gifski crate). Optional `gifsicle` second pass.
//! Live size estimate via sample-and-extrapolate. See `docs/06-Export.md`, `docs/10-Licensing.md`.

// TODO(M4): spawn the gifski sidecar, pipe frames, stream progress via a tauri Channel.
