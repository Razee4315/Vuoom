// The annotation tool rail. Single click arms a tool for one shape; double click (or the
// lock at the bottom) keeps it armed to draw several in a row. Compact mode drops the
// labels for a slimmer rail.
import { For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon, type IconName } from "../icons";
import { layout } from "../prefs";
import { TOOLS } from "../shortcuts";
import type { Tool } from "../types";

const GLYPH: Record<Tool, IconName> = {
  select: "cursor",
  text: "text",
  shape: "shape",
  arrow: "arrow",
  line: "line",
  highlight: "highlight",
  mask: "mask",
};

// Grouped by intent: select, shapes, connectors, text.
const GROUPS: Tool[][] = [["select"], ["shape", "highlight", "mask"], ["arrow", "line"], ["text"]];
const meta = (id: Tool) => TOOLS.find((t) => t.id === id)!;

export default function ToolRail() {
  const ed = useEditor();
  const locked = (id: Tool) => ed.toolLock() && ed.tool() === id && id !== "select";
  return (
    <nav class="rail" classList={{ compact: layout.railCompact() }} aria-label="Annotation tools">
      <For each={GROUPS}>
        {(group, gi) => (
          <>
            <Show when={gi() > 0}>
              <div class="rail-sep" aria-hidden="true" />
            </Show>
            <For each={group}>
              {(id) => (
                <button
                  type="button"
                  class="rail-tool"
                  classList={{ on: ed.tool() === id, locked: locked(id) }}
                  aria-pressed={ed.tool() === id}
                  aria-label={meta(id).label}
                  data-tip={`${meta(id).label}${id === "select" ? "" : ". Double click to keep it armed"}`}
                  data-kbd={meta(id).key}
                  onClick={() => ed.pickTool(id)}
                  onDblClick={() => id !== "select" && ed.lockTool(id)}
                >
                  <Icon name={GLYPH[id]} size={18} />
                  <span class="rail-label">{meta(id).label}</span>
                  <Show when={locked(id)}>
                    <span class="rail-lockdot" aria-hidden="true" />
                  </Show>
                </button>
              )}
            </For>
          </>
        )}
      </For>
      <div class="rail-spacer" />
      <button
        type="button"
        class="rail-tool rail-lock"
        classList={{ on: ed.toolLock() }}
        aria-pressed={ed.toolLock()}
        aria-label="Keep tool armed"
        data-tip={ed.toolLock() ? "Tool stays armed. Click to release" : "Keep the tool armed after each shape"}
        onClick={() => ed.setToolLock(!ed.toolLock())}
      >
        <Icon name={ed.toolLock() ? "lock" : "unlock"} size={16} />
        <span class="rail-label">Lock</span>
      </button>
      <button
        type="button"
        class="rail-tool rail-collapse"
        aria-label={layout.railCompact() ? "Show labels" : "Hide labels"}
        data-tip={layout.railCompact() ? "Show labels" : "Compact rail"}
        onClick={() => layout.railCompact.set(!layout.railCompact())}
      >
        <Icon name={layout.railCompact() ? "chevronRight" : "chevronLeft"} size={14} />
      </button>
    </nav>
  );
}
