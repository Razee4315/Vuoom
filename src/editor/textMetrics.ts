// Text label sizes measured the way they render, not guessed from the character count, so the
// selection outline, the handles and clicks land on the actual words. Labels can hold several
// lines (Shift+Enter in the editor); the engine lays them out with the same line height.
import { fontCss } from "./constants";

/** Line height as a multiple of the font size (the engine uses the same). */
export const LINE_HEIGHT = 1.25;

/** What sizing a label needs. */
export interface TextLook {
  text: string;
  font: string;
  bold: boolean;
  italic: boolean;
}

const REF_PX = 100;
let ctx: CanvasRenderingContext2D | null = null;
const widths = new Map<string, number>();

// Web fonts load after the first paint: widths measured with a fallback font are dropped then.
if (typeof document !== "undefined") {
  document.fonts?.addEventListener?.("loadingdone", () => widths.clear());
}

/** The CSS font shorthand for a label at `px`. */
export function cssFont(t: Omit<TextLook, "text">, px: number): string {
  return `${t.italic ? "italic " : ""}${t.bold ? 700 : 400} ${px}px ${fontCss(t.font)}`;
}

/** Width of `line` at a 1 px font (multiply by the font size). */
function unitWidth(line: string, t: Omit<TextLook, "text">): number {
  const key = `${t.font}|${t.bold}|${t.italic}|${line}`;
  const known = widths.get(key);
  if (known !== undefined) return known;
  if (!ctx) ctx = document.createElement("canvas").getContext("2d");
  let w = line.length * 0.6;
  if (ctx) {
    ctx.font = cssFont(t, REF_PX);
    w = ctx.measureText(line).width / REF_PX;
  }
  if (widths.size > 4000) widths.clear();
  widths.set(key, w);
  return w;
}

/** The label's lines. */
export const textLines = (text: string): string[] => text.split("\n");

/** A label's box in pixels at a font of `fontPx`: its widest line and all its lines. */
export function textBox(t: TextLook, fontPx: number): { w: number; h: number; lines: string[] } {
  const lines = textLines(t.text);
  const unit = Math.max(0, ...lines.map((l) => unitWidth(l, t)));
  return {
    w: Math.max(unit * fontPx, fontPx * 0.6),
    h: lines.length * LINE_HEIGHT * fontPx,
    lines,
  };
}
