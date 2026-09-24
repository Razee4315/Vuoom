// The tool rail, grouped by what the user is doing: select; aim the camera (zoom, crop);
// point things out or hide them (text, arrow, shape, highlight, hide). A tool arms for one
// use and hands back to Select; double click keeps it armed (also a switch in the tool's
// Inspector card). Compact mode drops the labels for a slimmer rail.
import { For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon, type IconName } from "../icons";
import { layout } from "../prefs";
import { CROP_KEY, TOOLS } from "../shortcuts";
import type { Tool } from "../types";

/** A rail button: a drawing tool, or Crop (a mode, not a tool). */
type RailItem = Tool | "crop";

const GLYPH: Record<RailItem, IconName> = {
  select: "cursor",
  zoom: "zoomIn",
  crop: "crop",
  text: "text",
  arrow: "arrow",
  pen: "pen",
  shape: "shape",
  highlight: "highlight",
  spotlight: "spotlight",
  mask: "eyeOff",
};

const GROUPS: RailItem[][] = [["select"], ["zoom", "crop"], ["text", "arrow", "pen", "shape", "highlight", "spotlight", "mask"]];

const CROP = { label: "Crop", key: CROP_KEY.key, tip: "Crop the recording" };

export default function ToolRail() {
  const ed = useEditor();
  const meta = (id: RailItem) => (id === "crop" ? null : TOOLS.find((t) => t.id === id));
  const label = (id: RailItem) => meta(id)?.label ?? CROP.label;
  const on = (id: RailItem) => (id === "crop" ? !!ed.cropEdit() : ed.tool() === id && !ed.cropEdit());
  const locked = (id: RailItem) => id !== "crop" && id !== "select" && ed.toolLock() && ed.tool() === id;
  const tip = (id: RailItem) => {
    if (id === "crop") return CROP.tip;
    if (id === "select") return "Select, move and resize";
    return `${meta(id)?.hint.replace(/ \(.\)$/, "") ?? ""} Double click to keep it armed.`;
  };
  const pick = (id: RailItem) => {
    if (id === "crop") {
      if (!ed.cropEdit()) void ed.beginCropEdit();
      return;
    }
    ed.pickTool(id);
  };
  return (
    <nav class="rail" classList={{ compact: layout.railCompact() }} aria-label="Tools">
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
                  classList={{ on: on(id), locked: locked(id) }}
                  aria-pressed={on(id)}
                  aria-label={label(id)}
                  data-tip={tip(id)}
                  data-kbd={meta(id)?.key ?? CROP.key}
                  disabled={!ed.hasClip()}
                  onClick={() => pick(id)}
                  onDblClick={() => id !== "select" && id !== "crop" && ed.lockTool(id)}
                >
                  <Icon name={GLYPH[id]} size={18} />
                  <span class="rail-label">{label(id)}</span>
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
