// Stateless canvas-overlay primitives: selection handles and arrows/lines. None of these
// read editor state, everything comes in via props.
import { For, Show, type JSX } from "solid-js";
import type { Vec2 } from "./types";

export function Handles(props: { pts: Vec2[]; cursors?: string[] }): JSX.Element {
  // Figma-style square handles: an 8px panel-filled square with a 1.5px accent border, sitting
  // on a slightly larger dark "ring" square so the handle stays crisp on any footage/background.
  // Both squares are centered on the point; styling lives in `.handle-ring` / `.handle` (tokens).
  const SIZE = 8;
  const RING = SIZE + 3; // the dark backing halo, ~1.5px proud on every side
  return (
    <For each={props.pts}>
      {(p, i) => (
        <g style={{ cursor: props.cursors?.[i()] ?? "pointer" }}>
          <rect
            class="handle-ring"
            x={p.x - RING / 2}
            y={p.y - RING / 2}
            width={RING}
            height={RING}
            rx={2}
          />
          <rect
            class="handle"
            x={p.x - SIZE / 2}
            y={p.y - SIZE / 2}
            width={SIZE}
            height={SIZE}
            rx={1.5}
          />
        </g>
      )}
    </For>
  );
}

/** The control point that bows the line `from`-`to` by `curve` (a fraction of its length). */
export function arrowCtrl(from: Vec2, to: Vec2, curve: number): Vec2 {
  // A quadratic Bézier's middle is half way to its control point, hence the 2.
  return {
    x: (from.x + to.x) / 2 - (to.y - from.y) * curve * 2,
    y: (from.y + to.y) / 2 + (to.x - from.x) * curve * 2,
  };
}

export function ArrowLine(props: {
  from: Vec2;
  to: Vec2;
  color: string;
  width?: number;
  headFrom?: boolean;
  headTo?: boolean;
  /** How far the middle bows off the straight line (see `ArrowAnn.curve`). */
  curve?: number;
}): JSX.Element {
  const w = () => props.width ?? 3;
  const ctrl = () => arrowCtrl(props.from, props.to, props.curve ?? 0);
  const len = () => Math.hypot(props.to.x - props.from.x, props.to.y - props.from.y);
  // Mirrors the export head: thickness × 4, at least 10, at most half the arrow.
  const headLen = () => Math.min(Math.max(w() * 4, 10), len() * 0.5);
  const headTo = () => props.headTo ?? true;
  const headFrom = () => props.headFrom ?? false;
  // A head whose tip is at `tip`, coming from `base` (a point just behind it on the line).
  const tri = (tip: Vec2, base: Vec2) => {
    const a = Math.atan2(tip.y - base.y, tip.x - base.x);
    const bx = tip.x - headLen() * Math.cos(a);
    const by = tip.y - headLen() * Math.sin(a);
    const [px, py] = [-Math.sin(a) * headLen() * 0.55, Math.cos(a) * headLen() * 0.55];
    return `${tip.x},${tip.y} ${bx + px},${by + py} ${bx - px},${by - py}`;
  };
  // Stop the line short of a head, so its round end doesn't show past the head's point.
  const trim = (end: Vec2, toward: Vec2) => {
    const d = Math.hypot(toward.x - end.x, toward.y - end.y) || 1;
    const k = headLen() * 0.9;
    return { x: end.x + ((toward.x - end.x) / d) * k, y: end.y + ((toward.y - end.y) / d) * k };
  };
  const start = () => (headFrom() ? trim(props.from, ctrl()) : props.from);
  const end = () => (headTo() ? trim(props.to, ctrl()) : props.to);
  return (
    <g stroke={props.color} fill={props.color} stroke-width={w()} stroke-linecap="round">
      <path
        d={`M${start().x} ${start().y}Q${ctrl().x} ${ctrl().y} ${end().x} ${end().y}`}
        fill="none"
      />
      <Show when={headTo()}>
        <polygon points={tri(props.to, ctrl())} stroke="none" />
      </Show>
      <Show when={headFrom()}>
        <polygon points={tri(props.from, ctrl())} stroke="none" />
      </Show>
    </g>
  );
}
