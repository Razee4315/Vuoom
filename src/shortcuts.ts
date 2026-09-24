// Tool definitions + the keyboard cheat-sheet. The single source of truth for the "?"
// modal, every chord here is wired in onKey / onGlobalKey / RecordOverlay.
import type { Tool } from "./types";

export const TOOLS: { id: Tool; label: string; key: string; code: string; hint: string }[] = [
  { id: "select", label: "Select", key: "V", code: "KeyV", hint: "Click to select, drag to move or resize. (V)" },
  {
    id: "zoom",
    label: "Zoom",
    key: "Z",
    code: "KeyZ",
    hint: "Drag a box over what to zoom into, or click a spot. It starts at the playhead. (Z)",
  },
  { id: "text", label: "Text", key: "T", code: "KeyT", hint: "Click where the label should go. (T)" },
  { id: "arrow", label: "Arrow", key: "A", code: "KeyA", hint: "Drag from the label toward what it points at. (A)" },
  { id: "pen", label: "Pen", key: "P", code: "KeyP", hint: "Drag to draw. Draw as many lines as you like. (P)" },
  { id: "shape", label: "Shape", key: "S", code: "KeyS", hint: "Drag to frame something with a box. (S)" },
  { id: "highlight", label: "Highlight", key: "H", code: "KeyH", hint: "Drag over an area to highlight it. (H)" },
  {
    id: "spotlight",
    label: "Spotlight",
    key: "L",
    code: "KeyL",
    hint: "Drag over what matters. Everything around it goes dark. (L)",
  },
  {
    id: "mask",
    label: "Hide",
    key: "M",
    code: "KeyM",
    hint: "Drag over private details, like an email or a password, to cover them. (M)",
  },
];
/** Crop isn't a drawing tool but a mode, entered from the rail or with R. */
export const CROP_KEY = { key: "R", code: "KeyR" };
// e.code → tool, for single-key tool switching (only while a clip is loaded).
export const TOOL_KEYS: Record<string, Tool> = Object.fromEntries(TOOLS.map((t) => [t.code, t.id]));

// The one source of truth for the "?" cheat-sheet. Kept next to the handlers above so it
// can't drift, every chord here is wired in onKey / onGlobalKey / RecordOverlay.
export const SHORTCUTS: { group: string; items: { keys: string[]; label: string }[] }[] = [
  {
    group: "Recording",
    items: [
      { keys: ["Ctrl", "Shift", "R"], label: "Start recording" },
      { keys: ["Ctrl", "Shift", "X"], label: "Stop recording" },
      { keys: ["Esc"], label: "Cancel region / countdown" },
    ],
  },
  {
    group: "Playback",
    items: [
      { keys: ["Space"], label: "Play / pause" },
      { keys: ["←", "→"], label: "Scrub (Shift = 1s)" },
      { keys: [",", "."], label: "Step one frame (Shift = 10)" },
      { keys: ["Home"], label: "Jump to start" },
      { keys: ["End"], label: "Jump to end" },
      { keys: ["I"], label: "Trim start at playhead (Shift clears)" },
      { keys: ["O"], label: "Trim end at playhead (Shift clears)" },
    ],
  },
  {
    group: "Tools",
    items: [
      { keys: ["V"], label: "Select" },
      { keys: ["Z"], label: "Zoom (drag over what to zoom into)" },
      { keys: ["R"], label: "Crop" },
      { keys: ["T"], label: "Text" },
      { keys: ["A"], label: "Arrow" },
      { keys: ["P"], label: "Pen (draw freehand)" },
      { keys: ["S"], label: "Shape" },
      { keys: ["H"], label: "Highlight" },
      { keys: ["L"], label: "Spotlight (dim everything else)" },
      { keys: ["M"], label: "Hide (cover private details)" },
    ],
  },
  {
    group: "Insert",
    items: [
      { keys: ["X"], label: "Speed up at playhead" },
      { keys: ["C"], label: "Cut at playhead" },
    ],
  },
  {
    group: "Editing",
    items: [
      { keys: ["Ctrl", "Z"], label: "Undo" },
      { keys: ["Ctrl", "Y"], label: "Redo" },
      { keys: ["Ctrl", "D"], label: "Duplicate selection" },
      { keys: ["Ctrl", "C"], label: "Copy selection" },
      { keys: ["Ctrl", "V"], label: "Paste at playhead" },
      { keys: ["Ctrl", "]"], label: "Bring forward (Shift = front)" },
      { keys: ["Ctrl", "["], label: "Send backward (Shift = back)" },
      { keys: ["Del"], label: "Delete selection" },
      { keys: ["Enter"], label: "Type in the selected text (also double-click)" },
      { keys: ["Shift", "Enter"], label: "New line while typing a text" },
      { keys: ["←", "→", "↑", "↓"], label: "Nudge selection (Shift = further)" },
      { keys: ["Esc"], label: "Clear selection" },
    ],
  },
  {
    group: "Project",
    items: [
      { keys: ["Ctrl", "O"], label: "Open project" },
      { keys: ["Ctrl", "S"], label: "Save project" },
      { keys: ["Ctrl", "E"], label: "Export" },
      { keys: ["?"], label: "Keyboard shortcuts" },
    ],
  },
  {
    group: "Workspace",
    items: [
      { keys: ["Ctrl", "K"], label: "Search every action" },
      { keys: ["Ctrl", ","], label: "Settings" },
      { keys: ["Ctrl", "1"], label: "Show / hide tools" },
      { keys: ["Ctrl", "2"], label: "Show / hide inspector" },
      { keys: ["Ctrl", "3"], label: "Show / hide timeline" },
      { keys: ["Ctrl", "0"], label: "Reset layout" },
      { keys: ["G"], label: "Composition guides" },
      { keys: ["Ctrl", "Wheel"], label: "Zoom the timeline" },
      { keys: ["Alt"], label: "Hold while dragging to skip snapping" },
    ],
  },
];
