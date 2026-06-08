# 05 — GPU Compositing & the Live-Preview Bridge

The wgpu compositor that turns raw frames + camera keyframes + styling into the final look, and
the solution to the hardest architectural problem in the app: getting a smooth 60fps preview into
the Tauri webview without choking on IPC.

---

## The preview-bridge problem (decide this first)

Raw frames are huge: 1080p BGRA ≈ 8 MB; 60fps ≈ 500 MB/s. **You cannot send raw frames over
Tauri's JSON command IPC** — it serializes them and chokes. Three options were evaluated:

| Option | What | Verdict |
|---|---|---|
| **A** Native child/overlay wgpu window layered into the Tauri window | Highest theoretical perf (no readback) | **High-risk on Windows.** wgpu + WebView2 fight over the surface → flicker; overlay z-order/click/DPI/resize sync is fragile. Only if profiling demands it. |
| **B** Stream frames to the webview (custom protocol **or WebSocket**) | Render offscreen, send pixels to a `<canvas>` | **✅ RECOMMENDED — WebSocket variant.** Proven by Cap at sustained 60fps. |
| **C** Shared GPU texture into WebView2 | Zero-copy into the webview's GPU | **Not achievable.** WebView2 exposes no API to import an external D3D shared texture or to do offscreen/shared-memory rendering. |

### The chosen design (Cap's, validated by their source)

```
 timeline scrub (webview) ──invoke seekTo(frame)──▶ Rust
                                                     │
 wgpu compositor (DX12): render ONE frame to an offscreen texture
                                                     │
 copy_texture_to_buffer (rows padded to 256 bytes) → map_async → RGBA bytes
                                                     │
 pack: [ rgba pixels ][ stride u32 | height u32 | width u32 | frame# u32 | t_ns u64 ]  (LE)
                                                     │
 send as a BINARY message over  ws://127.0.0.1:<random-port>   ("latest frame wins")
                                                     │
 frame-worker.ts (Web Worker): parse trailing metadata → WebGPU writeTexture → draw <canvas>
                                  (Canvas2D putImageData fallback) — rAF loop, OffscreenCanvas
```

**Why this wins:** pixels never touch JSON IPC; a scrub is just a tiny `seekTo` command. Cap
measures ~430 MB/s for full-res preview over loopback, WebSocket send ~0.5–1.3 ms, WebGPU upload
~2.1 ms, **end-to-end ≈ 10–15 ms, sustained 60fps with zero renderer drops.**

### Critical implementation details (learned from Cap)

- **Send RAW RGBA, not compressed**, for local preview. Cap originally did RGBA→NV12 on the CPU
  (~15–25 ms/frame) which capped preview at 40–50fps; switching to raw RGBA (≈2.7× bandwidth,
  trivial over loopback) unlocked stable 60fps. Don't repeat their mistake.
- **Carry `stride` in the metadata** — `copy_texture_to_buffer` pads rows to 256 bytes; the
  worker must un-pad.
- **Bind to `127.0.0.1` only**, random port returned to the frontend; consider a per-session
  token in the WS path. (Tauri's own localhost guidance warns about exposure.)
- **Do all upload/draw in a Web Worker on an OffscreenCanvas** — never block the main thread.
- **Pre-create renderer layers and preload assets** (cursor textures, backgrounds) at init, not
  lazily mid-playback, to avoid frame-time spikes. Cap pools readback buffers — scrub p95 dropped
  231 ms → 47 ms.
- **4K preview:** raw RGBA at 4K60 is heavy; drop to half-res preview (Cap's "half" ≈ 183 MB/s)
  or switch that case to H.264 + WebCodecs `VideoDecoder` in the webview. (Only worth it when
  transport bandwidth — not local loopback — is the constraint.)

**Fallback (Option B2):** a Tauri **async custom URI scheme protocol**
(`register_asynchronous_uri_scheme_protocol`) serving `Vec<u8>` works for a pull-per-frame model
(`fetch('vuoom-frame://.../123')` after a scrub). On Windows/WebView2 it delivers one complete
response per request (no chunked streaming), so model preview as many small requests. Slightly
higher per-frame overhead than a persistent WebSocket; keep as the backup.

---

## The wgpu compositor

### Backend & device

- **Default to the DX12 backend on Windows** (`Backends::DX12`) — DXGI swapchains are more stable
  and faster than Vulkan WSI on Windows; wgpu is moving toward DX12-as-default there. Cap
  explicitly prefers DX12. Vulkan is the fallback.
- **One device, one compositor, two sinks.** Build a single `wgpu::Device`/`Queue` + `Compositor`
  used by **both** preview and export. Only the sink differs (WebSocket vs encoder). This
  guarantees the preview is pixel-identical to the export.
- The compositor renders to an **offscreen texture** (`RENDER_ATTACHMENT | COPY_SRC`); you never
  present to a wgpu surface (the webview owns the visible surface).

### Render graph (runs identically for preview & export)

```
offscreen target = Texture(out_w, out_h, RENDER_ATTACHMENT | COPY_SRC)

Pass 0  Background   → styled bg into padded canvas (solid / gradient / image / blurred image)
Pass 1  Source       → upload captured BGRA → (GPU) BGRA→RGBA into a source texture
Pass 2  Composite    → sample source with the camera zoom/pan transform (uniform);
                       apply rounded-corner SDF clip + drop-shadow SDF (drawn behind)
Pass 3  Motion blur  → optional velocity-based post pass (uses prev-frame transform)
Pass 4  Overlays     → custom cursor, click ripple, captions/annotations
→ offscreen RGBA texture

preview sink:  copy_texture_to_buffer → map → pack → WebSocket
export sink:   (GPU) RGBA→NV12 → Media Foundation / ffmpeg encoder  (runs unthrottled)
```

The camera transform comes from `vuoom-zoom`'s per-frame `(center, zoom)` (see
[`04`](./04-Input-and-AutoZoom.md)). Interpolate keyframes at the requested time, feed as a uniform.

### Uniforms (Cap-style `ProjectUniforms`)

Output dims, source crop/bounds, camera `center`+`zoom`, cursor pos/size, background descriptor,
corner radius, shadow params, padding, aspect-ratio reframe, motion-blur descriptor. One uniform
buffer updated per frame.

---

## SDF shader: rounded corners + drop shadow

Use a **signed distance field** rounded box — resolution-independent corners, anti-aliased edges,
and a free soft shadow.

```wgsl
// Inigo Quilez rounded-box SDF (per-corner radii in r)
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r_in: vec4<f32>) -> f32 {
    var r = r_in;
    r = select(r.zw, r.xy, p.x > 0.0).xyxy; // pick x-side radii
    let rx = select(r.y, r.x, p.y > 0.0);
    let q = abs(p) - b + rx;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - rx;
}
```

- **Corners + AA:** `alpha = 1.0 - smoothstep(-aa, aa, d)` where `aa ≈ fwidth(d)` (~1px). Multiply
  the composited frame's alpha by this to clip to a rounded rect.
- **Drop shadow:** evaluate the SDF at a shifted/inflated box (offset by shadow direction,
  inflated by blur), `shadowAlpha = strength * (1.0 - smoothstep(0.0, blurRadius, max(d,0.0)))`;
  draw the shadow pass **behind** the rounded frame, onto the styled background.

Cap implements corners+shadow inside `composite-video-frame.wgsl`; color converters live in
`nv12_to_rgba.wgsl` / `rgba_to_nv12.wgsl`.

---

## Readback correctness checklist

- [ ] `copy_texture_to_buffer` `bytes_per_row` rounded up to a multiple of 256; ship real stride.
- [ ] `map_async` + poll; **pool** readback buffers (don't allocate per frame).
- [ ] Worker un-pads stride, uploads to a WebGPU texture sized to `width`×`height`.
- [ ] Playback driven by `requestAnimationFrame`; queue depth ≥1, "latest frame wins" to avoid lag.

## Sources

- Cap preview pipeline: `crates/editor/PLAYBACK-FINDINGS.md`, `apps/desktop/src-tauri/src/frame_ws.rs`,
  `apps/desktop/src/utils/frame-worker.ts`, `apps/desktop/src/routes/editor/Player.tsx`,
  `crates/rendering/src/lib.rs` — all at <https://github.com/CapSoftware/Cap> ·
  breakdown: <https://memo.d.foundation/breakdown/cap>
- Tauri wgpu-overlay (Option A) risk: <https://github.com/tauri-apps/tauri/discussions/11944> ·
  flicker: <https://github.com/tauri-apps/tauri/issues/9220>
- WebView2 no shared-texture/offscreen (Option C): <https://github.com/MicrosoftEdge/WebView2Feedback/issues/547> ·
  <https://github.com/MicrosoftEdge/WebView2Feedback/issues/526>
- Tauri custom protocol: <https://github.com/tauri-apps/tauri/discussions/5690> ·
  localhost plugin warning: <https://v2.tauri.app/plugin/localhost/>
- wgpu DX12-on-Windows: <https://github.com/gfx-rs/wgpu/issues/2719> ·
  <https://docs.rs/wgpu/latest/wgpu/struct.Backends.html>
- SDF: <https://iquilezles.org/articles/distfunctions2d/> · <https://mini.gmshaders.com/p/sdf>
