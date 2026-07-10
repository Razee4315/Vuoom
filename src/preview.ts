// Preview client: receives composited RGBA frames over the localhost WebSocket and draws
// them to a canvas. Frame format (little-endian trailer):
//   [ RGBA pixels ][ stride u32 | height u32 | width u32 | frame# u32 | t_ns u64 ]
// See docs/05-Compositing-and-Preview.md and crate vuoom-preview::protocol.

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
  private closed = true; // true once disconnect() is called — suppresses reconnects

  /** Bind the canvas frames will be drawn into. */
  attach(canvas: HTMLCanvasElement): void {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
  }

  /** Notified with the frame aspect ratio (width / height) when it first changes —
   *  lets the UI size the preview frame so there is no letterbox to misalign overlays. */
  onAspectChange(cb: (aspect: number) => void): void {
    this.onAspect = cb;
  }

  /** Connect to the engine's preview server on `port` with its per-session auth `token`
   *  (both from the Rust side). The token is required in the URL path or the engine refuses
   *  the connection. The socket auto-reconnects with a short backoff if the engine drops it,
   *  until `disconnect()`. */
  connect(port: number, token: string): void {
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
