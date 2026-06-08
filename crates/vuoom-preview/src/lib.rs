//! Live-preview bridge (the hard architectural problem, solved).
//!
//! Pixels never cross JSON IPC. Composited RGBA is read back, packed with trailing
//! metadata `[stride, height, width, frame#, t_ns]`, and pushed as binary frames over a
//! `127.0.0.1` WebSocket ("latest frame wins"). A Web Worker uploads to a WebGPU canvas.
//! Scrubbing is a cheap `seekTo` Tauri command. See `docs/05-Compositing-and-Preview.md`.

// TODO(S2 spike): bind 127.0.0.1:0, serve a WS, stream a moving test pattern at 60fps.
