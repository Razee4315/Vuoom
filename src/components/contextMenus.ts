// Right-click menus for the stage and the timeline. Each builder selects what was
// clicked (so the menu always acts on what you see highlighted) and returns its items.
import type { Editor } from "../editor/createEditor";
import { TOOLS } from "../shortcuts";
import type { Kind } from "../types";
import { type MenuItem, openContextMenu } from "../ui";

function annotationItems(ed: Editor): MenuItem[] {
  return [
    { label: "Duplicate", icon: "duplicate", kbd: "Ctrl+D", onSelect: () => void ed.duplicateSelected() },
    { label: "Copy", icon: "copy", kbd: "Ctrl+C", onSelect: () => void ed.copySelected() },
    { separator: true },
    { label: "Bring forward", icon: "bringForward", kbd: "Ctrl+]", onSelect: () => void ed.reorderSelected("forward") },
    { label: "Send backward", icon: "sendBackward", kbd: "Ctrl+[", onSelect: () => void ed.reorderSelected("backward") },
    { label: "Bring to front", onSelect: () => void ed.reorderSelected("front") },
    { label: "Send to back", onSelect: () => void ed.reorderSelected("back") },
    { separator: true },
    { label: "Show from playhead", icon: "timer", onSelect: () => {
      const r = ed.selectedRange();
      if (r) ed.editRange(ed.playhead(), Math.min(ed.duration(), ed.playhead() + (r.end - r.start)));
    } },
    { separator: true },
    { label: "Delete", icon: "trash", kbd: "Del", danger: true, onSelect: () => void ed.deleteSelection() },
  ];
}

function insertItems(ed: Editor): MenuItem[] {
  return [
    { label: "Zoom here", icon: "zoomIn", kbd: "Z", onSelect: () => void ed.addZoomAt() },
    { label: "Speed up here", icon: "speed", kbd: "X", onSelect: () => void ed.addSpeedAtPlayhead() },
    { label: "Cut here", icon: "cut", kbd: "C", onSelect: () => void ed.addCutAtPlayhead() },
  ];
}

/** Right-click on the stage overlay. */
export function stageMenu(ed: Editor, e: MouseEvent): void {
  if (!ed.hasClip()) return;
  const hit = ed.hitTest(ed.norm(e as PointerEvent));
  if (hit) {
    if (!ed.isSelected(hit.kind, hit.id)) {
      ed.setSelZoom(null);
      ed.setSelSpeed(null);
      ed.setSelCut(null);
      ed.clearExtra();
      ed.setSelected(hit);
    }
    openContextMenu(e, [{ heading: ed.selCount() > 1 ? `${ed.selCount()} selected` : ed.inspTitle() }, ...annotationItems(ed)]);
    return;
  }
  openContextMenu(e, [
    {
      label: "Paste at playhead",
      icon: "copy",
      kbd: "Ctrl+V",
      disabled: ed.clipboard().length === 0,
      onSelect: () => void ed.pasteClipboard(),
    },
    { separator: true },
    { heading: "Draw" },
    ...TOOLS.filter((t) => t.id !== "select").map(
      (t): MenuItem => ({ label: t.label, kbd: t.key, onSelect: () => ed.setTool(t.id) }),
    ),
    { separator: true },
    { heading: "At the playhead" },
    ...insertItems(ed),
  ]);
}

/** Right-click on a timeline zoom block. */
export function zoomMenu(ed: Editor, e: MouseEvent, index: number): void {
  ed.setSelected(null);
  ed.setSelSpeed(null);
  ed.setSelCut(null);
  ed.setSelZoom(index);
  const z = ed.zooms()[index];
  if (!z) return;
  openContextMenu(e, [
    { heading: `Zoom ${z.amount.toFixed(1)}×` },
    ...[1.5, 1.8, 2, 2.5, 3].map(
      (a): MenuItem => ({
        label: `${a}×`,
        checked: Math.abs(z.amount - a) < 0.05,
        onSelect: () => void ed.applyZoomEdit(index, z.start, z.end, a),
      }),
    ),
    { separator: true },
    { label: "Follow cursor", checked: typeof z.mode !== "object", onSelect: () => void ed.applyZoomFocus(null) },
    {
      label: "Fixed point",
      checked: typeof z.mode === "object",
      onSelect: () => void ed.applyZoomFocus(ed.selZoomFocus() ?? { x: 0.5, y: 0.5 }),
    },
    { separator: true },
    { label: "Jump to start", onSelect: () => ed.scrub(z.start) },
    { label: "Delete zoom", icon: "trash", kbd: "Del", danger: true, onSelect: () => void ed.deleteSelectedZoom() },
  ]);
}

/** Right-click on a timeline speed band. */
export function speedMenu(ed: Editor, e: MouseEvent, index: number): void {
  ed.setSelected(null);
  ed.setSelZoom(null);
  ed.setSelCut(null);
  ed.setSelSpeed(index);
  const r = ed.speed()[index];
  if (!r) return;
  openContextMenu(e, [
    { heading: `Speed ${r.factor}×` },
    ...[1.5, 2, 3, 4, 6, 8].map(
      (f): MenuItem => ({
        label: `${f}×`,
        checked: r.factor === f,
        onSelect: () => void ed.applySpeedEdit(index, r.start, r.end, f),
      }),
    ),
    { separator: true },
    { label: "Delete speed-up", icon: "trash", kbd: "Del", danger: true, onSelect: () => void ed.deleteSelectedSpeed() },
  ]);
}

/** Right-click on a timeline cut band. */
export function cutMenu(ed: Editor, e: MouseEvent, index: number): void {
  ed.setSelected(null);
  ed.setSelZoom(null);
  ed.setSelSpeed(null);
  ed.setSelCut(index);
  openContextMenu(e, [
    { heading: "Cut" },
    { label: "Restore this section", icon: "reset", kbd: "Del", onSelect: () => void ed.deleteSelectedCut() },
  ]);
}

/** Right-click on an annotation's timeline bar. */
export function annBarMenu(ed: Editor, e: MouseEvent, kind: Kind, id: number): void {
  if (!ed.isSelected(kind, id)) {
    ed.setSelZoom(null);
    ed.setSelSpeed(null);
    ed.setSelCut(null);
    ed.clearExtra();
    ed.setSelected({ kind, id });
  }
  openContextMenu(e, [{ heading: ed.inspTitle() }, ...annotationItems(ed)]);
}

/** Right-click on empty timeline space: seek there, then offer inserts. */
export function timelineMenu(ed: Editor, e: MouseEvent): void {
  if (!ed.hasClip()) return;
  ed.scrub(ed.timeFromClientX(e.clientX));
  openContextMenu(e, [
    { heading: "At this moment" },
    ...insertItems(ed),
    { separator: true },
    { label: "Set trim start here", onSelect: () => void setTrim(ed, "start") },
    { label: "Set trim end here", onSelect: () => void setTrim(ed, "end") },
    { label: "Clear trim", disabled: !ed.trim(), onSelect: () => void clearTrim(ed) },
  ]);
}

async function setTrim(ed: Editor, which: "start" | "end") {
  const t = ed.playhead();
  const start = which === "start" ? Math.min(t, ed.tEnd() - 0.2) : ed.tStart();
  const end = which === "end" ? Math.max(t, ed.tStart() + 0.2) : ed.tEnd();
  await ed.commitTrim(start, end);
}

async function clearTrim(ed: Editor) {
  await ed.commitTrim(0, ed.duration());
}
