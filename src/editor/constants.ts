// Editor constants and pure helpers shared by the editor store and its components.
import type { Trim } from "../types";

/// Quick-pick annotation colors (white, ink, record red, box yellow, green, text blue).
export const PRESET_COLORS = ["#ffffff", "#0e0e0f", "#e5484d", "#ffd23f", "#30a46c", "#6ea8ff"];

/// Per-corner resize cursors for the four selection handles, in the order they are drawn:
/// top-left, top-right, bottom-left, bottom-right. Opposite corners share a diagonal.
export const CORNER_CURSORS = ["nwse-resize", "nesw-resize", "nesw-resize", "nwse-resize"];

/// Bundled text fonts. `id` is the family name sent to the renderer (empty = default sans);
/// `css` styles the on-canvas preview + the in-typeface picker. Mirrors the @font-face set
/// in styles/base.css and the fonts loaded into glyphon for export.
export const TEXT_FONTS: { id: string; label: string; css: string }[] = [
  { id: "", label: "Default", css: "Inter, sans-serif" },
  { id: "Anton", label: "Anton", css: "Anton, sans-serif" },
  { id: "Bebas Neue", label: "Bebas", css: "'Bebas Neue', sans-serif" },
  { id: "Poppins", label: "Poppins", css: "Poppins, sans-serif" },
  { id: "Permanent Marker", label: "Marker", css: "'Permanent Marker', cursive" },
  { id: "Shrikhand", label: "Shrikhand", css: "Shrikhand, serif" },
];
export const fontCss = (name: string) => TEXT_FONTS.find((f) => f.id === name)?.css ?? "Inter, sans-serif";

/// True when a timeline segment [start,end] never survives to the export because its visible
/// (trimmed) span is entirely swallowed, either it falls fully outside the trim window, or the
/// portion inside the trim window is completely covered by cut regions. Pure; recomputes freely
/// in JSX from the current cuts()/trim() signals. `trim` null means "no trim" (full clip).
/// Cuts count as covering even when they only blanket the visible part of the segment, since the
/// out-of-trim remainder is dropped anyway.
export function isSwallowed(start: number, end: number, cuts: Trim[], trim: Trim | null): boolean {
  if (end <= start) return true; // degenerate span, nothing to render
  const t0 = trim ? trim.start : 0;
  const t1 = trim ? trim.end : Infinity;
  // Clamp the segment to the trim window; if nothing is left, it's outside the export entirely.
  const vs = Math.max(start, t0);
  const ve = Math.min(end, t1);
  if (ve <= vs) return true;
  // Merge overlapping/adjacent cuts, then check whether they fully cover [vs, ve].
  const merged = cuts
    .filter((c) => c.end > c.start)
    .sort((a, b) => a.start - b.start)
    .reduce<Trim[]>((acc, c) => {
      const last = acc[acc.length - 1];
      if (last && c.start <= last.end) last.end = Math.max(last.end, c.end);
      else acc.push({ start: c.start, end: c.end });
      return acc;
    }, []);
  let cursor = vs;
  for (const c of merged) {
    if (c.start > cursor) return false; // uncovered gap before this cut
    if (c.end > cursor) cursor = c.end;
    if (cursor >= ve) return true;
  }
  return cursor >= ve;
}

