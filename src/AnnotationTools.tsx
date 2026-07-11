// The post-recording annotation tool rail: redesigned, grouped tool buttons with crisp
// hand-drawn icons and a discoverable "keep tool active" (lock) affordance. Purely
// presentational — every bit of state (which tool is active, whether lock is on) and every
// action (pick a tool, lock a tool, toggle the global lock) comes in via props so App.tsx
// keeps ownership of the editor's reactive signals.
import { For, Show, type JSX } from "solid-js";
import { LockIcon } from "./EditorPrimitives";
import { TOOLS } from "./shortcuts";
import type { Tool } from "./types";

// Consistent icon system: 20px, 1.75px stroke, currentColor so each glyph inherits the
// button's themed text color and reads correctly across every mono theme. Kept minimal and
// evenly weighted (Figma/Linear grammar) — one visual idea per tool, no busy compound marks.
function ToolGlyph(props: { tool: Tool }): JSX.Element {
  const common = {
    width: 20,
    height: 20,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "1.75",
    "stroke-linecap": "round" as const,
    "stroke-linejoin": "round" as const,
  };
  switch (props.tool) {
    case "select":
      // Classic pointer/cursor arrow — the universal "select & move" mark.
      return (
        <svg {...common}>
          <path d="M5 3l6 15.5 2.3-6.2 6.2-2.3z" />
        </svg>
      );
    case "text":
      // Serifed capital T — reads as "type".
      return (
        <svg {...common}>
          <path d="M5 7V5h14v2M12 5v14M9 19h6" />
        </svg>
      );
    case "shape":
      // A single rounded rectangle (the tool draws a box; ellipse is a toggle in the inspector).
      return (
        <svg {...common}>
          <rect x="4" y="6" width="16" height="12" rx="2.5" />
        </svg>
      );
    case "arrow":
      // Diagonal shaft with an arrowhead bracket at the tip.
      return (
        <svg {...common}>
          <path d="M6 18L18 6M10.5 6H18v7.5" />
        </svg>
      );
    case "line":
      // Plain diagonal stroke, no head.
      return (
        <svg {...common}>
          <path d="M6 18L18 6" />
        </svg>
      );
    case "highlight":
      // A marker/highlighter nib over an ink underline.
      return (
        <svg {...common}>
          <path d="M9 20l-4.4.9.9-4.4L15.4 5.6a1.6 1.6 0 0 1 2.3 0l.7.7a1.6 1.6 0 0 1 0 2.3z" />
          <path d="M4 22h10" />
        </svg>
      );
  }
}

// Tools grouped by intent, with a hairline separator between groups: Select · Shapes ·
// Connectors · Text. The order here is display-only; ids still map back to TOOLS metadata.
const GROUPS: Tool[][] = [
  ["select"],
  ["shape", "highlight"],
  ["arrow", "line"],
  ["text"],
];

const metaOf = (id: Tool) => TOOLS.find((t) => t.id === id)!;

export function ToolRail(props: {
  tool: Tool;
  /** Global "keep the drawing tool active" state (one-shot when off — the default). */
  locked: boolean;
  /** Single-click: switch to this tool. */
  onPick: (t: Tool) => void;
  /** Double-click a drawing tool: switch to it AND turn lock on (draw several in a row). */
  onLock: (t: Tool) => void;
  /** The bottom lock affordance — toggles the global lock. */
  onToggleLock: () => void;
}): JSX.Element {
  const isActive = (id: Tool) => props.tool === id;
  // A locked drawing tool shows a lock badge; Select is a mode and is never "locked".
  const isLocked = (id: Tool) => props.locked && isActive(id) && id !== "select";
  return (
    <nav class="toolrail" aria-label="Annotation tools">
      <For each={GROUPS}>
        {(group, gi) => (
          <>
            <Show when={gi() > 0}>
              <div class="toolrail-sep" aria-hidden="true" />
            </Show>
            <For each={group}>
              {(id) => {
                const m = metaOf(id);
                return (
                  <button
                    type="button"
                    classList={{ tool: true, active: isActive(id), locked: isLocked(id) }}
                    aria-pressed={isActive(id)}
                    title={`${m.label} (${m.key})`}
                    onClick={() => props.onPick(id)}
                    onDblClick={() => id !== "select" && props.onLock(id)}
                  >
                    <ToolGlyph tool={id} />
                    <span class="tool-label">{m.label}</span>
                    <Show when={isLocked(id)}>
                      <svg
                        class="tool-lockbadge"
                        width="11"
                        height="11"
                        viewBox="0 0 24 24"
                        fill="currentColor"
                        aria-hidden="true"
                      >
                        <rect x="5" y="11" width="14" height="9" rx="2" />
                        <path d="M8 11V7a4 4 0 0 1 8 0v4" fill="none" stroke="currentColor" stroke-width="2.5" />
                      </svg>
                    </Show>
                  </button>
                );
              }}
            </For>
          </>
        )}
      </For>
      <div class="toolrail-spacer" />
      <button
        type="button"
        classList={{ tool: true, "tool-lock": true, active: props.locked }}
        aria-pressed={props.locked}
        title={
          props.locked
            ? "Tool stays active. Click to turn off."
            : "Tool returns to Select after each shape. Click to keep it active, or double click a tool."
        }
        onClick={() => props.onToggleLock()}
      >
        <LockIcon locked={props.locked} />
        <span class="tool-label">Lock</span>
      </button>
    </nav>
  );
}
