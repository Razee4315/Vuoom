//! Windows screen capture → GPU textures.
//!
//! Primary: `windows-capture` (Windows Graphics Capture), cursor excluded, `Bgra8`,
//! 60fps cap, frames kept as D3D11 textures and bridged to wgpu (DX12) via an NT
//! shared handle + keyed mutex. Fallback: DXGI Desktop Duplication. QPC timestamps
//! align frames to the input event log. See `docs/03-Capture.md`.

// TODO(S1 spike): start a WGC session, expose `(texture, qpc_timestamp)` frames.
