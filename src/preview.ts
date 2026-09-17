// Preview client: receives composited RGBA frames over the localhost WebSocket and draws
// them to a canvas. Frame format (little-endian trailer):
//   [ RGBA pixels ][ stride u32 | height u32 | width u32 | frame# u32 | t_ns u64 ]
// See docs/05-Compositing-and-Preview.md and crate vuoom-preview::protocol.

import { isMock } from "./bridge";
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
      if (ev.data instanceof ArrayBuffer) this.draw(ev.data);
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
    if (this.reconnectTimer !== undefined) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
    this.closeSocket();
  }

  private draw(buf: ArrayBuffer): void {
    const frame = parseFrame(buf);
    if (!frame || !this.canvas || !this.ctx) return;
    const { width, height, stride, pixels } = frame;

    // Un-pad rows (stride may exceed width*4) into a tightly packed RGBA buffer.
    const rowBytes = width * 4;
    const packed = new Uint8ClampedArray(rowBytes * height);
    for (let y = 0; y < height; y++) {
      const src = y * stride;
      packed.set(pixels.subarray(src, src + rowBytes), y * rowBytes);
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

  attach(canvas: HTMLCanvasElement): void {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
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
  }
}

/** Preview client factory: the real WebSocket client in the app, the mock in a browser. */
export function createPreviewClient(): PreviewClient | MockPreviewClient {
  return isMock ? new MockPreviewClient() : new PreviewClient();
}
