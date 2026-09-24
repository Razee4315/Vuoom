// Preview client: receives composited RGBA frames over the localhost WebSocket and draws
// them to a canvas. Frame format (little-endian trailer):
//   [ RGBA pixels ][ stride u32 | height u32 | width u32 | frame# u32 | t_ns u64 ]
// See docs/05-Compositing-and-Preview.md and crate vuoom-preview::protocol.

import { isMock } from "./bridge";
import { paintBubble } from "./mock/camera";
import { mockEngine, paintDesktop } from "./mock/engine";

const META_LEN = 24;

interface PreviewFrame {
  width: number;
  height: number;
  stride: number;
  pixels: Uint8Array;
}

function parseFrame(buf: ArrayBuffer): PreviewFrame | null {
  if (buf.byteLength < META_LEN) return null;
  const view = new DataView(buf);
  const n = buf.byteLength;
  const stride = view.getUint32(n - 24, true);
  const height = view.getUint32(n - 20, true);
  const width = view.getUint32(n - 16, true);
  // frame# (n-12) and t_ns (n-8) are available for playback timing later.
  const pixels = new Uint8Array(buf, 0, n - META_LEN);
  return { width, height, stride, pixels };
}

/** Streams composited preview frames from the Rust engine into a `<canvas>`. */
export class PreviewClient {
  private ws: WebSocket | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private aspect = 0;
  private onAspect: ((aspect: number) => void) | null = null;
  private port = 0;
  private token = "";
  private reconnectTimer: number | undefined;
  private closed = true; // true once disconnect() is called, suppresses reconnects

  /** Bind the canvas frames will be drawn into. */
  attach(canvas: HTMLCanvasElement): void {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
  }

  /** Notified with the frame aspect ratio (width / height) when it first changes,
   *  lets the UI size the preview frame so there is no letterbox to misalign overlays. */
  onAspectChange(cb: (aspect: number) => void): void {
    this.onAspect = cb;
  }

  /** Connect to the engine's preview server on `port` with its per-session auth `token`
   *  (both from the Rust side). The token is required in the URL path or the engine refuses
   *  the connection. The socket auto-reconnects with a short backoff if the engine drops it,
   *  until `disconnect()`. Idempotent: connecting again with the same port/token while the
   *  socket is open (or being open already) is a no-op, so callers can hook the stream
   *  early and refresh the call later without churning the connection. */
  connect(port: number, token: string): void {
    if (this.ws && this.port === port && this.token === token && !this.closed) return;
    this.disconnect();
    this.port = port;
    this.token = token;
    this.closed = false;
    this.open();
  }

  private open(): void {
    this.closeSocket();
    const ws = new WebSocket(`ws://127.0.0.1:${this.port}/ws/${this.token}`);
    ws.binaryType = "arraybuffer";
    ws.onmessage = (ev) => {
      if (ev.data instanceof ArrayBuffer) this.queue(ev.data);
    };
    ws.onclose = () => this.scheduleReconnect();
    ws.onerror = () => {
      try {
        ws.close();
      } catch {
        /* already closing */
      }
    };
    this.ws = ws;
  }

  private scheduleReconnect(): void {
    if (this.closed || this.reconnectTimer !== undefined || !this.port) return;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = undefined;
      if (!this.closed) this.open();
    }, 1000);
  }

  private closeSocket(): void {
    const ws = this.ws;
    if (ws) {
      ws.onclose = null;
      ws.onerror = null;
      ws.onmessage = null;
      try {
        ws.close();
      } catch {
        /* already closing */
      }
      this.ws = null;
    }
  }

  /** Close the connection and stop reconnecting. */
  disconnect(): void {
    this.closed = true;
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.pending = null;
    if (this.reconnectTimer !== undefined) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
    this.closeSocket();
  }

  // Frames can arrive faster than the screen refreshes (live preview, fast scrubbing): keep
  // only the newest and draw it on the next animation frame, so none is drawn for nothing.
  private pending: ArrayBuffer | null = null;
  private raf = 0;
  private queue(buf: ArrayBuffer): void {
    this.pending = buf;
    if (this.raf) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      const next = this.pending;
      this.pending = null;
      if (next) this.draw(next);
    });
  }

  private draw(buf: ArrayBuffer): void {
    const frame = parseFrame(buf);
    if (!frame || !this.canvas || !this.ctx) return;
    const { width, height, stride, pixels } = frame;

    const rowBytes = width * 4;
    let packed: Uint8ClampedArray;
    if (stride === rowBytes) {
      // Tightly packed (the usual case): draw straight from the received bytes.
      packed = new Uint8ClampedArray(pixels.buffer, pixels.byteOffset, rowBytes * height);
    } else {
      // Un-pad rows (stride exceeds width*4) into a tightly packed RGBA buffer.
      packed = new Uint8ClampedArray(rowBytes * height);
      for (let y = 0; y < height; y++) {
        const src = y * stride;
        packed.set(pixels.subarray(src, src + rowBytes), y * rowBytes);
      }
    }

    if (this.canvas.width !== width) this.canvas.width = width;
    if (this.canvas.height !== height) this.canvas.height = height;
    this.ctx.putImageData(new ImageData(packed, width, height), 0, 0);

    const aspect = width / height;
    if (Math.abs(aspect - this.aspect) > 1e-3) {
      this.aspect = aspect;
      this.onAspect?.(aspect);
    }
  }
}

/** Browser-stand-in for PreviewClient. While "recording" it paints at ~12fps; otherwise it
 *  repaints ONLY when the visible state changes (seek, edit, scene bump), coalesced to
 *  animation frames. A paused editor costs zero work per second. */
export class MockPreviewClient {
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private aspect = 0;
  private onAspect: ((aspect: number) => void) | null = null;
  private timer: number | undefined;
  private raf = 0;
  private dirty = true;
  private lastT = -1;
  private lastVersion = -1;
  private unlistenDirty: (() => void) | null = null;

  constructor() {
    // A paint skipped while the page was hidden is retried once it is visible again.
    document.addEventListener("visibilitychange", () => {
      if (!document.hidden && this.dirty) this.schedulePaint();
    });
  }

  attach(canvas: HTMLCanvasElement): void {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    // A canvas mounted after connect (the editor appears once a clip loads) gets sized and
    // painted right away instead of waiting for the next state change.
    if (this.unlistenDirty) {
      canvas.width = 960;
      canvas.height = 540;
      this.dirty = true;
      this.schedulePaint();
    }
  }
  onAspectChange(cb: (aspect: number) => void): void {
    this.onAspect = cb;
  }
  connect(_port: number, _token: string): void {
    // Idempotent like the real client: a second connect (countdown → recording) keeps the
    // running loop, the canvas size and the painted still intact.
    if (this.unlistenDirty) return;
    this.disconnect();
    if (this.canvas) {
      this.canvas.width = 960;
      this.canvas.height = 540;
    }
    if (Math.abs(16 / 9 - this.aspect) > 1e-3) {
      this.aspect = 16 / 9;
      this.onAspect?.(16 / 9);
    }
    this.dirty = true;
    this.unlistenDirty = mockEngine.onDirty(() => {
      this.dirty = true;
      // Live recording keeps a continuous ~12fps loop; idle mode paints on change only.
      if (mockEngine.live && this.timer === undefined) {
        this.timer = window.setInterval(() => this.draw(), 80);
      } else if (!mockEngine.live && this.timer !== undefined) {
        clearInterval(this.timer);
        this.timer = undefined;
      }
      this.schedulePaint();
    });
    if (mockEngine.live) {
      this.timer = window.setInterval(() => this.draw(), 80);
    }
    this.schedulePaint();
  }
  disconnect(): void {
    if (this.timer !== undefined) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
    if (this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
    this.unlistenDirty?.();
    this.unlistenDirty = null;
  }
  private schedulePaint(): void {
    if (this.raf || !this.canvas) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.draw();
    });
  }
  private draw(): void {
    if (!this.canvas || !this.ctx) return;
    // Skip work entirely when the canvas is hidden (e.g. behind the empty state) or the
    // page is backgrounded, so the mock costs nothing while idle.
    if (document.hidden || this.canvas.offsetParent === null) return;
    const t = mockEngine.live ? mockEngine.liveElapsed() : mockEngine.playhead;
    const v = mockEngine.sceneVersion;
    if (!mockEngine.live && !this.dirty && t === this.lastT && v === this.lastVersion) return;
    this.lastT = t;
    this.lastVersion = v;
    this.dirty = false;
    paintDesktop(this.ctx, this.canvas.width, this.canvas.height, t, { live: mockEngine.live });
    // The webcam bubble is composited over the take, not the live monitor (like the engine).
    if (!mockEngine.live && mockEngine.camera) {
      paintBubble(this.ctx, this.canvas.width, this.canvas.height, mockEngine.camera, t);
    }
    if (!mockEngine.live) paintCaption(this.ctx, this.canvas.width, this.canvas.height, t);
  }
}

/** The caption on screen, as the engine draws it: centered lines over a dark plate. */
function paintCaption(ctx: CanvasRenderingContext2D, w: number, h: number, t: number): void {
  const style = mockEngine.captionStyle;
  const cue = style.visible ? mockEngine.captionAt(t) : null;
  const text = cue?.text.trim();
  if (!text) return;
  const font = style.size * h;
  const maxW = w * 0.86;
  ctx.save();
  ctx.font = `600 ${font}px "Segoe UI", system-ui, sans-serif`;
  // Greedy word wrap to the caption width.
  const lines: string[] = [];
  for (const word of text.split(/\s+/)) {
    const last = lines[lines.length - 1];
    if (last !== undefined && ctx.measureText(`${last} ${word}`).width <= maxW) {
      lines[lines.length - 1] = `${last} ${word}`;
    } else {
      lines.push(word);
    }
  }
  const lineH = font * 1.25;
  const padX = font * 0.5;
  const padY = font * 0.25;
  const textH = lines.length * lineH;
  const widest = Math.max(...lines.map((l) => ctx.measureText(l).width));
  const margin = font * 0.8;
  const top = style.position === "top" ? margin + padY : h - margin - padY - textH;
  ctx.fillStyle = "rgba(10, 10, 13, 0.75)";
  ctx.fillRect(w / 2 - widest / 2 - padX, top - padY, widest + 2 * padX, textH + 2 * padY);
  ctx.fillStyle = "#fff";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  lines.forEach((l, i) => {
    ctx.fillText(l, w / 2, top + lineH * (i + 0.5));
  });
  ctx.restore();
}

/** Preview client factory: the real WebSocket client in the app, the mock in a browser. */
export function createPreviewClient(): PreviewClient | MockPreviewClient {
  return isMock ? new MockPreviewClient() : new PreviewClient();
}
