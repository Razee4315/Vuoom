// Critically-damped spring camera ported from crates/vuoom-zoom/src/camera.rs, plus
// preset angular frequencies (Snappy=20, Smooth=14, Slow=9) for the interactive studio stage.

const LN_2 = Math.LN2;

export interface Spring {
  x: number;
  v: number;
}

export type SpringPreset = 'smooth' | 'snappy' | 'slow';

export const SPRING_OMEGA: Record<SpringPreset, { cam: number; zoom: number; pointer: number }> = {
  snappy: { cam: 20, zoom: 20, pointer: 26 },
  smooth: { cam: 14, zoom: 14, pointer: 18 },
  slow: { cam: 9, zoom: 9, pointer: 13 },
};

/** Exact analytic critically-damped spring toward `goal` with angular frequency `omega` (s^-1). */
export function springUpdateOmega(s: Spring, goal: number, omega: number, dt: number): void {
  const y = Math.max(omega, 0.1);
  const j0 = s.x - goal;
  const j1 = s.v + j0 * y;
  const eydt = Math.exp(-y * dt);
  s.x = eydt * (j0 + j1 * dt) + goal;
  s.v = eydt * (s.v - j1 * y * dt);
}

/** Critically-damped spring toward `goal`. `hl` is the half-life in seconds. */
export function springUpdate(s: Spring, goal: number, hl: number, dt: number): void {
  const h = Math.max(hl, 1e-4);
  springUpdateOmega(s, goal, (2 * LN_2) / h, dt);
}

/** Clamp the camera center so the zoomed viewport never reveals area outside the frame. */
export function clampCamera(cx: number, cy: number, zoom: number): [number, number] {
  const half = 0.5 / Math.max(zoom, 1);
  return [Math.min(Math.max(cx, half), 1 - half), Math.min(Math.max(cy, half), 1 - half)];
}

/** The app's config.rs defaults. */
export const HL = { zoom: 0.3, pan: 0.22, pointer: 0.12 } as const;

/** Transform for a camera element with transform-origin 0 0 over a W×H frame. */
export function camTransform(cx: number, cy: number, z: number, w: number, h: number): string {
  const [x, y] = clampCamera(cx, cy, z);
  const tx = (0.5 - x * z) * w;
  const ty = (0.5 - y * z) * h;
  return `translate3d(${tx.toFixed(2)}px, ${ty.toFixed(2)}px, 0) scale(${z.toFixed(4)})`;
}
