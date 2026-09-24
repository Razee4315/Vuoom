// Pure geometry + timeline math shared by the editor. No reactive state.
import type { SerVec, SpeedRegion, Trim, Vec2 } from "./types";

export const clamp01 = (n: number) => Math.min(1, Math.max(0, n));
export const v2 = (p: SerVec | Vec2): Vec2 => (Array.isArray(p) ? { x: p[0], y: p[1] } : p);
// Which ends of an arrow carry a head, mirroring vuoom_render's resolution of ArrowStyle.
export const arrowHeads = (style?: string) =>
  style === "Line"
    ? { from: false, to: false }
    : style === "DoubleArrow"
      ? { from: true, to: true }
      : { from: false, to: true };
export const distToSeg = (p: Vec2, a: Vec2, b: Vec2) => {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy || 1e-9;
  let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
  t = Math.min(1, Math.max(0, t));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
};

/** Played duration after trim + speed regions + cuts (mirrors vuoom_project::output_duration).
 * A cut is a region with an infinite factor, it contributes zero output time. */
export function outputDuration(
  duration: number,
  trim: Trim | null,
  regions: SpeedRegion[],
  cuts: Trim[],
): number {
  const t0 = trim?.start ?? 0;
  const t1 = trim?.end ?? duration;
  let out = 0;
  let cursor = t0;
  const sorted = [
    ...regions.filter((r) => r.end > r.start && r.factor > 0),
    ...cuts
      .filter((c) => c.end > c.start)
      .map((c) => ({ start: c.start, end: c.end, factor: Infinity })),
  ].sort((a, b) => a.start - b.start);
  for (const r of sorted) {
    const s = Math.max(r.start, t0);
    const e = Math.min(r.end, t1);
    if (e <= s || s < cursor) continue;
    out += s - cursor + (e - s) / r.factor;
    cursor = e;
  }
  if (cursor < t1) out += t1 - cursor;
  return out;
}

/**
 * What the zoom tool framed, from the drag's start `a` to the pointer `b` (normalized frame
 * coordinates). A zoom's view keeps the frame's shape, so the frame is a square in normalized
 * terms, grown from the start toward the pointer; `amount` is what fits it, 1.2× to 4×. A
 * click (a tiny drag) aims at `a` with the clip's zoom strength `fallback`.
 */
export function zoomFrame(a: Vec2, b: Vec2, fallback: number): { x: number; y: number; side: number; amount: number } {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const drawn = Math.min(1, Math.max(Math.abs(dx), Math.abs(dy)));
  if (drawn < 0.03) {
    const amount = Math.min(4, Math.max(1.2, fallback));
    return { x: a.x, y: a.y, side: 1 / amount, amount };
  }
  const amount = Math.min(4, Math.max(1.2, 1 / drawn));
  const side = 1 / amount;
  return {
    x: a.x + ((Math.sign(dx) || 1) * side) / 2,
    y: a.y + ((Math.sign(dy) || 1) * side) / 2,
    side,
    amount,
  };
}
