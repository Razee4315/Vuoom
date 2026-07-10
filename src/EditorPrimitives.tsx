// Stateless presentational primitives shared across the editor: toolbar/lock icons,
// the inspector chrome (panel + sections + rows), and the canvas overlay SVG (handles +
// arrow/line). None of these read editor state — everything comes in via props.
import { For, Show, type JSX } from "solid-js";
import type { Tool, Vec2 } from "./types";

export function ToolIcon(props: { tool: Tool }): JSX.Element {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "1.8", "stroke-linecap": "round" as const, "stroke-linejoin": "round" as const };
  switch (props.tool) {
    case "select":
      return <svg {...common}><path d="M5 3l6 16 2-6 6-2z" /></svg>;
    case "text":
      return <svg {...common}><path d="M5 6V4h14v2M12 4v16M9 20h6" /></svg>;
    case "arrow":
      return <svg {...common}><path d="M5 19L19 5M11 5h8v8" /></svg>;
    case "line":
      return <svg {...common}><path d="M5 19L19 5" /></svg>;
    case "shape":
      return <svg {...common}><rect x="3.5" y="6.5" width="11" height="11" rx="1.5" /><ellipse cx="16.5" cy="14" rx="4.5" ry="4" /></svg>;
    case "highlight":
      return <svg {...common}><path d="M4 20h6M14.5 4.5l5 5L10 19l-5 1 1-5z" /></svg>;
  }
}

/** Shared chrome for every right-hand inspector: resizer + titled header + a Done button. */
export function InspectorPanel(props: {
  title: string;
  onClose: () => void;
  onResizeDown: (e: PointerEvent) => void;
  onResizeMove: (e: PointerEvent) => void;
  onResizeUp: () => void;
  children: JSX.Element;
}): JSX.Element {
  return (
    <aside class="properties">
      <div
        class="panel-resizer"
        title="Drag to resize"
        onPointerDown={props.onResizeDown}
        onPointerMove={props.onResizeMove}
        onPointerUp={props.onResizeUp}
      />
      <div class="inspector-head">
        <h2>{props.title}</h2>
        <button class="icon-btn" title="Done" onClick={props.onClose}>
          ✕
        </button>
      </div>
      {props.children}
    </aside>
  );
}

/** A titled inspector group — a tiny uppercase header over its rows. */
export function InspSection(props: { title: string; children: JSX.Element }): JSX.Element {
  return (
    <div class="insp-section">
      <div class="insp-section-title">{props.title}</div>
      {props.children}
    </div>
  );
}

/** One inspector row: a label on the left, the control right-aligned (or stacked). */
export function InspRow(props: { label: string; stack?: boolean; children: JSX.Element }): JSX.Element {
  return (
    <div class="insp-row" classList={{ stack: props.stack }}>
      <span class="insp-row-label">{props.label}</span>
      <div class="insp-row-value">{props.children}</div>
    </div>
  );
}

export function LockIcon(props: { locked: boolean }): JSX.Element {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="5" y="11" width="14" height="9" rx="2" />
      <Show when={props.locked} fallback={<path d="M8 11V7a4 4 0 0 1 7.5-2" />}>
        <path d="M8 11V7a4 4 0 0 1 8 0v4" />
      </Show>
    </svg>
  );
}

export function Handles(props: { pts: Vec2[] }): JSX.Element {
  return (
    <For each={props.pts}>
      {(p) => <circle class="handle" cx={p.x} cy={p.y} r={6} />}
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
