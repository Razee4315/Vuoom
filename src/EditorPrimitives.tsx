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

export function ArrowLine(props: {
  from: Vec2;
  to: Vec2;
  color: string;
  width?: number;
  headFrom?: boolean;
  headTo?: boolean;
}): JSX.Element {
  const w = () => props.width ?? 3;
  const headLen = () => Math.max(w() * 4, 10); // mirrors the export head (thickness × 4, min 10)
  const ang = () => Math.atan2(props.to.y - props.from.y, props.to.x - props.from.x);
  const headTo = () => props.headTo ?? true;
  const headFrom = () => props.headFrom ?? false;
  // Triangle for a head whose tip is at `tip`, pointing along angle `a`.
  const tri = (tip: Vec2, a: number) => {
    const p1 = { x: tip.x - headLen() * Math.cos(a - 0.5), y: tip.y - headLen() * Math.sin(a - 0.5) };
    const p2 = { x: tip.x - headLen() * Math.cos(a + 0.5), y: tip.y - headLen() * Math.sin(a + 0.5) };
    return `${tip.x},${tip.y} ${p1.x},${p1.y} ${p2.x},${p2.y}`;
  };
  return (
    <g stroke={props.color} fill={props.color} stroke-width={w()} stroke-linecap="round">
      <line x1={props.from.x} y1={props.from.y} x2={props.to.x} y2={props.to.y} />
      <Show when={headTo()}>
        <polygon points={tri(props.to, ang())} stroke="none" />
      </Show>
      <Show when={headFrom()}>
        <polygon points={tri(props.from, ang() + Math.PI)} stroke="none" />
      </Show>
    </g>
  );
}
