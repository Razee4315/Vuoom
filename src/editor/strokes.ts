// Pen strokes on the canvas. A stroke's geometry in the editor is its bounding box
// [x, y, w, h] (normalized), so it moves, resizes, snaps and nudges like a box; its points
// are mapped from the stored box into the new one when that changes.
import type { Color, SerVec, Vec2 } from "../types";

/** The two looks of the Pen tool: a fine opaque pen and a wide see-through marker. */
export const PEN_LOOKS = {
  pen: { color: { r: 0.95, g: 0.25, b: 0.25, a: 1 }, width: 0.006 },
  marker: { color: { r: 1, g: 0.86, b: 0.18, a: 0.45 }, width: 0.024 },
} satisfies Record<string, { color: Color; width: number }>;

/** A marker is any stroke you can see through (the pen draws opaque). */
export const isMarker = (s: { color: Color }) => (s.color.a ?? 1) < 0.8;

/** The bounding box [x, y, w, h] of a stroke's points. */
export function strokeBox(points: SerVec[]): number[] {
  let [x0, y0, x1, y1] = [1, 1, 0, 0];
  for (const [x, y] of points) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  if (x1 < x0) return [0, 0, 0, 0];
  return [x0, y0, x1 - x0, y1 - y0];
}

/** The stroke's points moved and scaled from their own box into `g` ([x, y, w, h]). */
export function mapStroke(points: SerVec[], g: number[]): SerVec[] {
  const [ox, oy, ow, oh] = strokeBox(points);
  const sx = ow > 1e-6 ? g[2] / ow : 1;
  const sy = oh > 1e-6 ? g[3] / oh : 1;
  return points.map(([x, y]) => [g[0] + (x - ox) * sx, g[1] + (y - oy) * sy]);
}

// Distance from p to the segment ab, with x and y scaled by `ax`/`ay` (so distances are in
// pixels, not in the stretched normalized space).
function segDist(p: Vec2, a: SerVec, b: SerVec, ax: number, ay: number): number {
  const [px, py] = [p.x * ax, p.y * ay];
  const [x1, y1, x2, y2] = [a[0] * ax, a[1] * ay, b[0] * ax, b[1] * ay];
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len2 = dx * dx + dy * dy;
  const t = len2 > 0 ? Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / len2)) : 0;
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}

/** Distance in pixels from `p` to the stroke's path, on a stage `w`×`h` pixels. */
export function strokeDist(p: Vec2, points: SerVec[], w: number, h: number): number {
  if (points.length === 1) return segDist(p, points[0], points[0], w, h);
  let best = Number.POSITIVE_INFINITY;
  for (let i = 1; i < points.length; i++) best = Math.min(best, segDist(p, points[i - 1], points[i], w, h));
  return best;
}

/** Drop points that bend the path by less than `tol` pixels (Ramer-Douglas-Peucker). */
export function simplify(points: Vec2[], tol: number, w: number, h: number): Vec2[] {
  if (points.length < 3) return points.slice();
  const keep = new Uint8Array(points.length);
  keep[0] = 1;
  keep[points.length - 1] = 1;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length) {
    const [i, j] = stack.pop()!;
    const a: SerVec = [points[i].x, points[i].y];
    const b: SerVec = [points[j].x, points[j].y];
    let far = -1;
    let dmax = tol;
    for (let k = i + 1; k < j; k++) {
      const d = segDist(points[k], a, b, w, h);
      if (d > dmax) {
        dmax = d;
        far = k;
      }
    }
    if (far >= 0) {
      keep[far] = 1;
      stack.push([i, far], [far, j]);
    }
  }
  return points.filter((_, k) => keep[k]);
}

/** Even out hand tremor: each inner point moves toward the average of its neighbours. */
export function smooth(points: Vec2[], passes = 2): Vec2[] {
  let pts = points;
  for (let n = 0; n < passes && pts.length > 2; n++) {
    const next = [pts[0]];
    for (let i = 1; i < pts.length - 1; i++) {
      next.push({
        x: (pts[i - 1].x + pts[i].x * 2 + pts[i + 1].x) / 4,
        y: (pts[i - 1].y + pts[i].y * 2 + pts[i + 1].y) / 4,
      });
    }
    next.push(pts[pts.length - 1]);
    pts = next;
  }
  return pts;
}

/**
 * An SVG path through the points (in pixels): straight segments, exactly as the export draws
 * them. The points are already smoothed and dense where the line bends.
 */
export function strokePath(pts: Vec2[]): string {
  if (pts.length === 0) return "";
  const f = (n: number) => n.toFixed(1);
  if (pts.length === 1) return `M${f(pts[0].x)} ${f(pts[0].y)}h0.01`;
  return pts.map((p, i) => `${i ? "L" : "M"}${f(p.x)} ${f(p.y)}`).join("");
}
