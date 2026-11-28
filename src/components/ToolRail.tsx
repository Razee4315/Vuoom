// The tool rail, grouped by what the user is doing: select; aim the camera (zoom, crop);
// point things out or hide them (text, arrow, shape, highlight, hide). A tool arms for one
// use and hands back to Select; double click keeps it armed (also a switch in the tool's
// Inspector card). Compact mode drops the labels for a slimmer rail.
import { createSignal, For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon, type IconName } from "../icons";
import { layout } from "../prefs";
import { CROP_KEY, TOOLS } from "../shortcuts";
import type { Tool } from "../types";
import { createRovingTabindex } from "../ui";

/** A rail button: a drawing tool, or Crop (a mode, not a tool). */
type RailItem = Tool | "crop";
type RailFocusId = RailItem | "collapse";

const GLYPH: Record<RailItem, IconName> = {
  select: "cursor",
  zoom: "zoomIn",
  crop: "crop",
  text: "text",
  arrow: "arrow",
  step: "step",
  pen: "pen",
  shape: "shape",
  highlight: "highlight",
  spotlight: "spotlight",
  mask: "eyeOff",
};

const GROUPS: RailItem[][] = [["select"], ["zoom", "crop"], ["text", "arrow", "step", "pen", "shape", "highlight", "spotlight", "mask"]];

export default function ToolRail() {
  const ed = useEditor();
  let railEl: HTMLDivElement | undefined;
  const [rovingId, setRovingId] = createSignal<RailFocusId | null>(null);
  const onRovingKey = createRovingTabindex(() => railEl, ".rail-tool", { orientation: "vertical" });

  const meta = (id: RailItem) => (id === "crop" ? null : TOOLS().find((tool) => tool.id === id));
  const label = (id: RailItem) => meta(id)?.label ?? t("Crop");
  const on = (id: RailItem) => (id === "crop" ? !!ed.cropEdit() : ed.tool() === id && !ed.cropEdit());
  const locked = (id: RailItem) => id !== "crop" && id !== "select" && ed.toolLock() && ed.tool() === id;
  const tabTarget = (): RailFocusId => {
    const r = rovingId();
    if (r && (r === "collapse" || ed.hasClip())) return r;
    if (!ed.hasClip()) return "collapse";
    return ed.cropEdit() ? "crop" : ed.tool();
  };
  const tip = (id: RailItem) => {
    if (id === "crop") return t("Crop the recording");
    if (id === "select") return t("Select, move and resize");
    return t("{hint} Double click to keep it armed.", { hint: meta(id)?.hint.replace(/\s*[(（].[)）]$/, "") ?? "" });
  };
  const pick = (id: RailItem) => {
    if (id === "crop") {
      if (!ed.cropEdit()) void ed.beginCropEdit();
      return;
    }
    ed.pickTool(id);
  };
  const toggleLock = (id: RailItem) => {
    if (id === "select" || id === "crop") return;
    if (locked(id)) ed.setToolLock(false);
    else ed.lockTool(id);
  };
  const onToolbarKeyDown = (e: KeyboardEvent) => {
    onRovingKey(e);
    if (e.defaultPrevented) e.stopPropagation();
  };
  return (
    <div
      ref={railEl}
      class="rail"
      classList={{ compact: layout.railCompact() }}
      role="toolbar"
      aria-orientation="vertical"
      aria-label={t("Tools")}
      onKeyDown={onToolbarKeyDown}
      onFocusOut={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setRovingId(null);
      }}
    >
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
                  tabindex={tabTarget() === id ? 0 : -1}
                  aria-pressed={on(id)}
                  aria-label={locked(id) ? t("{label} (locked)", { label: label(id) }) : label(id)}
                  data-tip={tip(id)}
                  data-kbd={meta(id)?.key ?? CROP_KEY.key}
                  disabled={!ed.hasClip()}
                  onFocus={() => setRovingId(id)}
                  onClick={() => pick(id)}
                  onDblClick={() => id !== "select" && id !== "crop" && ed.lockTool(id)}
                  onKeyDown={(e) => {
                    if (id === "select" || id === "crop") return;
                    if (
                      (e.key === "Enter" && e.shiftKey) ||
                      (e.code === "KeyL" && !e.ctrlKey && !e.metaKey && !e.altKey)
                    ) {
                      e.preventDefault();
                      e.stopPropagation();
                      toggleLock(id);
                    }
                  }}
                >
                  <Icon name={GLYPH[id]} size={16} />
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
        tabindex={tabTarget() === "collapse" ? 0 : -1}
        aria-label={layout.railCompact() ? t("Show labels") : t("Hide labels")}
        data-tip={layout.railCompact() ? t("Show labels") : t("Compact rail")}
        onFocus={() => setRovingId("collapse")}
        onClick={() => layout.railCompact.set(!layout.railCompact())}
      >
        <Icon name={layout.railCompact() ? "chevronRight" : "chevronLeft"} size={14} />
      </button>
    </div>
  );
}
