// The mock preview: paints the mock engine's scene into the canvas the real client would
// stream frames into.
import type { PreviewSource } from "../preview";
import { paintBubble } from "./camera";
import { mockEngine, paintDesktop } from "./engine";

/** Browser-stand-in for PreviewClient. While "recording" it paints at ~12fps; otherwise it
 *  repaints ONLY when the visible state changes (seek, edit, scene bump), coalesced to
 *  animation frames. A paused editor costs zero work per second. */
export class MockPreviewClient implements PreviewSource {
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
