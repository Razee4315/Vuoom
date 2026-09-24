// Synthetic webcam for the browser mock: a person in front of a softly lit wall, with a
// slow sway so the live bubble visibly updates. The engine uses a real camera.
import type { CameraOverlay } from "../types";

export const MOCK_CAMERAS = [
  { id: "mock-cam-1", name: "Integrated Webcam" },
  { id: "mock-cam-2", name: "HD Pro Webcam C920" },
];

export const defaultOverlay = (): CameraOverlay => ({
  visible: true,
  corner: "bottom-right",
  size: 0.28,
  shape: "circle",
  mirror: true,
  offset: 0,
});

/** Paint one camera frame covering `w`×`h` at time `t` seconds. */
export function drawMockCamera(ctx: CanvasRenderingContext2D, w: number, h: number, t: number): void {
  const wall = ctx.createLinearGradient(0, 0, w, h);
  wall.addColorStop(0, "#4a5a6a");
  wall.addColorStop(1, "#232b35");
  ctx.fillStyle = wall;
  ctx.fillRect(0, 0, w, h);
  // A lamp's glow and a shelf behind.
  const glow = ctx.createRadialGradient(w * 0.82, h * 0.2, 0, w * 0.82, h * 0.2, w * 0.45);
  glow.addColorStop(0, "rgba(255, 214, 160, 0.35)");
  glow.addColorStop(1, "rgba(255, 214, 160, 0)");
  ctx.fillStyle = glow;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = "rgba(20, 24, 30, 0.55)";
  ctx.fillRect(w * 0.04, h * 0.3, w * 0.22, h * 0.035);
  for (const [x, bh, c] of [
    [0.06, 0.1, "#b5654a"],
    [0.1, 0.13, "#d9b25f"],
    [0.14, 0.09, "#5f8fa8"],
    [0.18, 0.12, "#8a6fb0"],
  ] as const) {
    ctx.fillStyle = c;
    ctx.fillRect(w * x, h * (0.3 - bh), w * 0.03, h * bh);
  }
  // The person, swaying a little.
  const cx = w / 2 + Math.sin(t * 1.3) * w * 0.02;
  const nod = Math.sin(t * 2.1) * h * 0.006;
  ctx.fillStyle = "#2f6f73";
  ctx.beginPath();
  ctx.ellipse(cx, h * 1.05, w * 0.34, h * 0.4, 0, Math.PI, 0);
  ctx.fill();
  ctx.fillStyle = "#d9a98a";
  ctx.fillRect(cx - w * 0.045, h * 0.5 + nod, w * 0.09, h * 0.17);
  ctx.beginPath();
  ctx.ellipse(cx, h * 0.42 + nod, w * 0.12, h * 0.18, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#35261f";
  ctx.beginPath();
  ctx.ellipse(cx, h * 0.32 + nod, w * 0.13, h * 0.1, 0, Math.PI, 0);
  ctx.fill();
}

let canvas: HTMLCanvasElement | null = null;

/** The mock camera's current frame as JPEG bytes, like `camera_preview_frame`. */
export async function mockCameraJpeg(): Promise<ArrayBuffer> {
  canvas ??= document.createElement("canvas");
  canvas.width = 320;
  canvas.height = 240;
  const ctx = canvas.getContext("2d");
  if (!ctx) return new ArrayBuffer(0);
  drawMockCamera(ctx, 320, 240, performance.now() / 1000);
  const blob = await new Promise<Blob | null>((done) => canvas?.toBlob(done, "image/jpeg", 0.8));
  return blob ? blob.arrayBuffer() : new ArrayBuffer(0);
}

/** Draw the bubble over a painted mock frame, the way the engine composites it. */
export function paintBubble(ctx: CanvasRenderingContext2D, w: number, h: number, o: CameraOverlay, t: number) {
  if (!o.visible) return;
  const bh = Math.max(0.12, Math.min(0.6, o.size)) * h;
  const bw = o.shape === "wide" ? (bh * 16) / 9 : bh;
  const m = 0.035 * h;
  const left = o.corner.endsWith("left");
  const top = o.corner.startsWith("top");
  const x = left ? m : w - m - bw;
  const y = top ? m : h - m - bh;
  const r = o.shape === "circle" ? bh / 2 : o.shape === "square" ? bh * 0.18 : bh * 0.1;
  const outline = () => {
    ctx.beginPath();
    ctx.roundRect(x, y, bw, bh, r);
  };
  ctx.save();
  ctx.shadowColor = "rgba(0, 0, 0, 0.45)";
  ctx.shadowBlur = bh * 0.1;
  ctx.shadowOffsetY = bh * 0.03;
  ctx.fillStyle = "#16171b";
  outline();
  ctx.fill();
  ctx.restore();
  ctx.save();
  outline();
  ctx.clip();
  ctx.translate(x, y);
  if (o.mirror) {
    ctx.translate(bw, 0);
    ctx.scale(-1, 1);
  }
  // Center-crop a 4:3 frame into the bubble.
  const fw = Math.max(bw, (bh * 4) / 3);
  const fh = fw * 0.75;
  ctx.translate((bw - fw) / 2, (bh - fh) / 2);
  drawMockCamera(ctx, fw, fh, t);
  ctx.restore();
  ctx.save();
  ctx.lineWidth = Math.max(1, bh * 0.012);
  ctx.strokeStyle = "rgba(255, 255, 255, 0.85)";
  outline();
  ctx.stroke();
  ctx.restore();
}
