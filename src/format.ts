// Color + time + size formatting helpers. Pure, no reactive state.
import { clamp01 } from "./geometry";
import type { Color } from "./types";

export const cssColor = (c: Color) =>
  `rgba(${Math.round(c.r * 255)},${Math.round(c.g * 255)},${Math.round(c.b * 255)},${c.a})`;
const h2 = (n: number) => Math.round(clamp01(n) * 255).toString(16).padStart(2, "0");
export const rgbHex = (c: Color) => `#${h2(c.r)}${h2(c.g)}${h2(c.b)}`;
export const hexRgb = (hex: string) => {
  const n = parseInt(hex.slice(1), 16);
  return { r: ((n >> 16) & 255) / 255, g: ((n >> 8) & 255) / 255, b: (n & 255) / 255 };
};
export const fmt = (t: number) => {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
};
// Playhead readout with tenths, so annotations can be aligned precisely.
export const fmtT = (t: number) => `${fmt(t)}.${Math.floor((t % 1) * 10)}`;

export const fmtBytes = (b: number) => {
  if (b <= 0) return "—";
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
};
