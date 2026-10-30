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

/// The last segment of a Windows or POSIX path: the file or folder name.
export const baseName = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;

export const fmtBytes = (b: number) => {
  if (b <= 0) return "-";
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} MB`;
  return `${(b / (1024 * 1024 * 1024)).toFixed(1)} GB`;
};

/// User-facing copy for a failed GPU compositor init, shared by the boot warning banner
/// and the error mapper below so both surfaces tell the same story.
export const GPU_FAILED_MSG =
  "Graphics initialization failed, preview and export won't work (recording still saves " +
  "to disk). Update your GPU drivers and restart Vuoom.";

/// Terse engine errors and what to tell the user instead. Everything else the engine says
/// is already written for people, and passes through unchanged.
const FRIENDLY: [string, string][] = [
  ["no GPU compositor", GPU_FAILED_MSG],
  ["no recording", "Record something or open a project first."],
  ["no frames", "This clip has no frames to work with."],
  ["no such", "That item is gone. It may have been undone or deleted."],
  ["already recording", "A recording is already running."],
  ["not recording", "Nothing is being recorded right now."],
  ["There is not enough space", "The disk is full. Free up some space and try again."],
  ["os error 112", "The disk is full. Free up some space and try again."],
  ["Access is denied", "Windows wouldn't allow writing there. Try a different folder."],
  ["os error 5)", "Windows wouldn't allow writing there. Try a different folder."],
  ["os error 32", "Another program has that file open. Close it and try again."],
];

/// Map raw engine error strings to friendly, actionable copy before they hit a status line.
export const friendlyError = (e: unknown): string => {
  const msg = String(e);
  return FRIENDLY.find(([key]) => msg.includes(key))?.[1] ?? msg;
};

/// How long something has left, in words: "About 2 min left", or "" until there's a fair
/// idea (the first moments of an export say little about the rest).
export const timeLeft = (done: number, startedMs: number, nowMs: number): string => {
  if (done < 0.03 || done >= 1) return "";
  const left = ((nowMs - startedMs) / 1000) * ((1 - done) / done);
  if (left < 5) return "Almost done";
  if (left < 60) return `About ${Math.ceil(left / 5) * 5} s left`;
  return `About ${Math.ceil(left / 60)} min left`;
};
