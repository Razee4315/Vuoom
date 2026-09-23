// Tool definitions + the keyboard cheat-sheet. The single source of truth for the "?"
// modal, every chord here is wired in onKey / onGlobalKey / RecordOverlay.
import type { Tool } from "./types";

export const TOOLS: { id: Tool; label: string; key: string; code: string; hint: string }[] = [
  { id: "select", label: "Select", key: "V", code: "KeyV", hint: "Click to select, drag to move or resize. (V)" },
  { id: "text", label: "Text", key: "T", code: "KeyT", hint: "Click to add a text label. (T)" },
  { id: "arrow", label: "Arrow", key: "A", code: "KeyA", hint: "Drag to draw an arrow. (A)" },
  { id: "line", label: "Line", key: "L", code: "KeyL", hint: "Drag to draw a line. (L)" },
  { id: "shape", label: "Shape", key: "S", code: "KeyS", hint: "Drag to draw a box. (S)" },
  { id: "highlight", label: "Highlight", key: "H", code: "KeyH", hint: "Drag to highlight an area. (H)" },
  { id: "mask", label: "Mask", key: "M", code: "KeyM", hint: "Drag to cover an area with an opaque redaction block. (M)" },
];
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
      { keys: ["Home"], label: "Jump to start" },
      { keys: ["End"], label: "Jump to end" },
    ],
  },
  {
    group: "Tools",
    items: [
      { keys: ["V"], label: "Select" },
      { keys: ["T"], label: "Text" },
      { keys: ["A"], label: "Arrow" },
      { keys: ["L"], label: "Line" },
      { keys: ["S"], label: "Shape" },
      { keys: ["H"], label: "Highlight" },
      { keys: ["M"], label: "Mask (redact)" },
    ],
  },
  {
    group: "Insert",
    items: [
      { keys: ["Z"], label: "Zoom at playhead" },
      { keys: ["X"], label: "Speed at playhead" },
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
      { keys: ["Ctrl", "Wheel"], label: "Zoom the timeline" },
      { keys: ["Alt"], label: "Hold while dragging to skip snapping" },
    ],
  },
];
