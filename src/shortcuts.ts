// Tool definitions + the keyboard cheat-sheet. The single source of truth for the "?"
// modal, every chord here is wired in onKey / onGlobalKey / RecordOverlay.
import { RECORD_KEYS, stopKeys, zoomKeys } from "./hotkeys";
import { t } from "./i18n";
import type { Tool } from "./types";

export const TOOLS = (): { id: Tool; label: string; key: string; code: string; hint: string }[] => [
  { id: "select", label: t("Select"), key: "V", code: "KeyV", hint: t("Click to select, drag to move or resize. (V)") },
  {
    id: "zoom",
    label: t("Zoom"),
    key: "Z",
    code: "KeyZ",
    hint: t("Drag a box over what to zoom into, or click a spot. It starts at the playhead. (Z)"),
  },
  { id: "text", label: t("Text"), key: "T", code: "KeyT", hint: t("Click where the label should go. (T)") },
  { id: "arrow", label: t("Arrow"), key: "A", code: "KeyA", hint: t("Drag from the label toward what it points at. (A)") },
  { id: "step", label: t("Step"), key: "N", code: "KeyN", hint: t("Click to place the next numbered step. (N)") },
  { id: "pen", label: t("Pen"), key: "P", code: "KeyP", hint: t("Drag to draw. Draw as many lines as you like. (P)") },
  { id: "shape", label: t("Shape"), key: "S", code: "KeyS", hint: t("Drag to frame something with a box. (S)") },
  { id: "highlight", label: t("Highlight"), key: "H", code: "KeyH", hint: t("Drag over an area to highlight it. (H)") },
  {
    id: "spotlight",
    label: t("Spotlight"),
    key: "L",
    code: "KeyL",
    hint: t("Drag over what matters. Everything around it goes dark. (L)"),
  },
  {
    id: "mask",
    label: t("Hide"),
    key: "M",
    code: "KeyM",
    hint: t("Drag over private details, like an email or a password, to cover them. (M)"),
  },
];
/** Crop isn't a drawing tool but a mode, entered from the rail or with R. */
export const CROP_KEY = { key: "R", code: "KeyR" };
// e.code → tool, for single-key tool switching (only while a clip is loaded).
export const TOOL_KEYS: Record<string, Tool> = Object.fromEntries(TOOLS().map((tool) => [tool.code, tool.id]));

// The one source of truth for the "?" cheat-sheet. Kept next to the handlers above so it
// can't drift, every chord here is wired in onKey / onGlobalKey / RecordOverlay.
export const SHORTCUTS = (): { group: string; items: { keys: string[]; label: string }[] }[] => [
  {
    group: t("Recording"),
    items: [
      { keys: RECORD_KEYS.split("+"), label: t("Start recording") },
      // The two that can be rebound (above): read when the list is drawn.
      {
        get keys() {
          return zoomKeys().split("+");
        },
        label: t("Zoom in or out while recording"),
      },
      {
        get keys() {
          return stopKeys().split("+");
        },
        label: t("Stop recording"),
      },
      { keys: ["Esc"], label: t("Cancel region / countdown") },
    ],
  },
  {
    group: t("Playback"),
    items: [
      { keys: ["Space"], label: t("Play / pause") },
      { keys: ["←", "→"], label: t("Scrub (Shift = 1s)") },
      { keys: [",", "."], label: t("Step one frame (Shift = 10)") },
      { keys: ["Home"], label: t("Jump to start") },
      { keys: ["End"], label: t("Jump to end") },
      { keys: ["I"], label: t("Trim start at playhead (Shift clears)") },
      { keys: ["O"], label: t("Trim end at playhead (Shift clears)") },
    ],
  },
  {
    group: t("Tools"),
    items: [
      { keys: ["V"], label: t("Select") },
      { keys: ["Z"], label: t("Zoom (drag over what to zoom into)") },
      { keys: ["R"], label: t("Crop") },
      { keys: ["T"], label: t("Text") },
      { keys: ["A"], label: t("Arrow") },
      { keys: ["N"], label: t("Numbered step") },
      { keys: ["P"], label: t("Pen (draw freehand)") },
      { keys: ["S"], label: t("Shape") },
      { keys: ["H"], label: t("Highlight") },
      { keys: ["L"], label: t("Spotlight (dim everything else)") },
      { keys: ["M"], label: t("Hide (cover private details)") },
    ],
  },
  {
    group: t("Insert"),
    items: [
      { keys: ["X"], label: t("Speed up at playhead") },
      { keys: ["C"], label: t("Cut at playhead") },
    ],
  },
  {
    group: t("Editing"),
    items: [
      { keys: ["Ctrl", "Z"], label: t("Undo") },
      { keys: ["Ctrl", "Y"], label: t("Redo") },
      { keys: ["Ctrl", "D"], label: t("Duplicate selection") },
      { keys: ["Ctrl", "C"], label: t("Copy selection") },
      { keys: ["Ctrl", "V"], label: t("Paste at playhead") },
      { keys: ["Ctrl", "]"], label: t("Bring forward (Shift = front)") },
      { keys: ["Ctrl", "["], label: t("Send backward (Shift = back)") },
      { keys: ["Del"], label: t("Delete selection") },
      { keys: ["Enter"], label: t("Type in the selected text (also double-click)") },
      { keys: ["Shift", "Enter"], label: t("New line while typing a text") },
      { keys: ["←", "→", "↑", "↓"], label: t("Nudge selection (Shift = further)") },
      { keys: ["Esc"], label: t("Clear selection") },
    ],
  },
  {
    group: t("Project"),
    items: [
      { keys: ["Ctrl", "O"], label: t("Open project") },
      { keys: ["Ctrl", "S"], label: t("Save project") },
      { keys: ["Ctrl", "Shift", "S"], label: t("Save project as") },
      { keys: ["Ctrl", "E"], label: t("Export") },
      { keys: ["?"], label: t("Keyboard shortcuts") },
    ],
  },
  {
    group: t("Workspace"),
    items: [
      { keys: ["Ctrl", "K"], label: t("Search every action") },
      { keys: ["Ctrl", ","], label: t("Settings") },
      { keys: ["Ctrl", "1"], label: t("Show / hide tools") },
      { keys: ["Ctrl", "2"], label: t("Show / hide inspector") },
      { keys: ["Ctrl", "3"], label: t("Show / hide timeline") },
      { keys: ["Ctrl", "0"], label: t("Reset layout") },
      { keys: ["G"], label: t("Composition guides") },
      { keys: ["Ctrl", "Wheel"], label: t("Zoom the timeline") },
      { keys: ["Alt"], label: t("Hold while dragging to skip snapping") },
    ],
  },
];
