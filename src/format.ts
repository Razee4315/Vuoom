// Color + time + size formatting helpers, and the error mapper.
import { clamp01 } from "./geometry";
import { t } from "./i18n";
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
export const gpuFailedMsg = () =>
  t("Graphics initialization failed, preview and export won't work (recording still saves to disk). Update your GPU drivers and restart Vuoom.");

const diskFull = () => t("The disk is full. Free up some space and try again.");
const accessDenied = () => t("Windows wouldn't allow writing there. Try a different folder.");

/// Terse engine errors and what to tell the user instead. Matched on the engine's English.
const FRIENDLY: [string, () => string][] = [
  ["no GPU compositor", gpuFailedMsg],
  ["no recording", () => t("Record something or open a project first.")],
  ["it sent no frames", () => t("The camera was not recorded (it sent no frames).")],
  ["no frames", () => t("This clip has no frames to work with.")],
  ["no such", () => t("That item is gone. It may have been undone or deleted.")],
  ["already recording", () => t("A recording is already running.")],
  ["not recording", () => t("Nothing is being recorded right now.")],
  ["There is not enough space", diskFull],
  ["os error 112", diskFull],
  ["Access is denied", accessDenied],
  ["os error 5)", accessDenied],
  ["os error 32", () => t("Another program has that file open. Close it and try again.")],
];

/// Engine messages with values in them: each pattern pulls the values out of the English and
/// hands them to a whole translated sentence.
const ENGINE_PATTERNS: [RegExp, (m: RegExpExecArray) => string][] = [
  [
    /^Not enough disk space to record: only (.+) GB free, need at least (.+) GB\. A busy screen can write up to ~(\d+) MB\/s at this size, free up space and try again\.$/,
    (m) => t("Not enough disk space to record: only {free} GB free, need at least {need} GB. A busy screen can write up to ~{rate} MB/s at this size, free up space and try again.", { free: m[1], need: m[2], rate: m[3] }),
  ],
  [
    /^Not enough disk space to save this project: it needs about (.+) and only (.+) is free on that drive\. Free up some space and try again\.$/,
    (m) => t("Not enough disk space to save this project: it needs about {need} and only {free} is free on that drive. Free up some space and try again.", { need: m[1], free: m[2] }),
  ],
  [
    /^Not enough disk space to open this project: it needs about (.+) and only (.+) is free on that drive\. Free up some space and try again\.$/,
    (m) => t("Not enough disk space to open this project: it needs about {need} and only {free} is free on that drive. Free up some space and try again.", { need: m[1], free: m[2] }),
  ],
  [
    /^Heads up: your disk was low on space when recording started, about (\d+) min fits \((.+) GB free\)\. Vuoom stops automatically before the disk fills\.$/,
    (m) => t("Heads up: your disk was low on space when recording started, about {minutes} min fits ({free} GB free). Vuoom stops automatically before the disk fills.", { minutes: m[1], free: m[2] }),
  ],
  [
    /^Recording was cut short to protect your disk, low on space, ~(\d+) MB left\. Kept the (\d+) frames captured before that\.$/,
    (m) => t("Recording was cut short to protect your disk, low on space, ~{left} MB left. Kept the {frames} frames captured before that.", { left: m[1], frames: m[2] }),
  ],
  [
    /^Recording was cut short to protect your disk, (.+)\. Kept the (\d+) frames captured before that\.$/,
    (m) => t("Recording was cut short to protect your disk, {why}. Kept the {frames} frames captured before that.", { why: m[1], frames: m[2] }),
  ],
  [
    /^Capture couldn't keep up, dropped (\d+) frames \((.+)%\)\. Try 30 fps, a smaller region or a less busy disk\.$/,
    (m) => t("Capture couldn't keep up, dropped {dropped} frames ({percent}%). Try 30 fps, a smaller region or a less busy disk.", { dropped: m[1], percent: m[2] }),
  ],
  [/^Audio was not recorded: (.+)\.$/, (m) => t("Audio was not recorded: {why}.", { why: friendlyError(m[1]) })],
  [/^The camera was not recorded \((.+)\)\.$/, (m) => t("The camera was not recorded ({why}).", { why: friendlyError(m[1]) })],
  [/^The camera stopped early \((.+)\)\.$/, (m) => t("The camera stopped early ({why}).", { why: friendlyError(m[1]) })],
  [/^(.+) is unavailable \((.+)\)$/, (m) => t("{camera} is unavailable ({why})", { camera: m[1], why: m[2] })],
  [/^(.+) has no usable format \((.+)\)$/, (m) => t("{camera} has no usable format ({why})", { camera: m[1], why: m[2] })],
  [/^(.+) reported no frame size$/, (m) => t("{camera} reported no frame size", { camera: m[1] })],
  [/^camera: (.+)$/, (m) => t("Camera: {why}", { why: friendlyError(m[1]) })],
  [/^audio: (.+)$/, (m) => t("Audio: {why}", { why: friendlyError(m[1]) })],
  [/^captions: (.+)$/, (m) => t("Captions: {why}", { why: friendlyError(m[1]) })],
  [/^couldn't load the speech model: (.+)$/, (m) => t("Couldn't load the speech model: {why}", { why: m[1] })],
  [/^couldn't save the speech model: (.+)$/, (m) => t("Couldn't save the speech model: {why}", { why: m[1] })],
  [/^couldn't start the download: (.+)$/, (m) => t("Couldn't start the download: {why}", { why: m[1] })],
  [/^speech recognition failed: (.+)$/, (m) => t("Speech recognition failed: {why}", { why: m[1] })],
  [/^couldn't save the captions: (.+)$/, (m) => t("Couldn't save the captions: {why}", { why: m[1] })],
  [
    /^Only (\d+) of (\d+) frames could be read, the end is missing\.$/,
    (m) => t("Only {readable} of {total} frames could be read, the end is missing.", { readable: m[1], total: m[2] }),
  ],
  [/^(\d+) unreadable frames were skipped\.$/, (m) => t("{skipped} unreadable frames were skipped.", { skipped: m[1] })],
  [
    /^The new take is (\d+) x (\d+) and this clip is (\d+) x (\d+), so it wasn't added\. File > Recover last session opens it on its own\.$/,
    (m) => t("The new take is {takeW} x {takeH} and this clip is {clipW} x {clipH}, so it wasn't added. File > Recover last session opens it on its own.", { takeW: m[1], takeH: m[2], clipW: m[3], clipH: m[4] }),
  ],
  [
    /^The take couldn't be added \((.+)\)\. File > Recover last session opens it on its own\.$/,
    (m) => t("The take couldn't be added ({why}). File > Recover last session opens it on its own.", { why: friendlyError(m[1]) }),
  ],
  [/^This project folder no longer exists: (.+)$/, (m) => t("This project folder no longer exists: {path}", { path: m[1] })],
  [/^Couldn't read this project's file: (.+)$/, (m) => t("Couldn't read this project's file: {why}", { why: m[1] })],
  [/^This project's file is damaged \((.+)\)\.$/, (m) => t("This project's file is damaged ({why}).", { why: m[1] })],
  [
    /^This project wasn't saved completely\. The earlier save is intact in "(.+)", open that folder instead\.$/,
    (m) => t('This project wasn\'t saved completely. The earlier save is intact in "{path}", open that folder instead.', { path: m[1] }),
  ],
  [/^couldn't replace the earlier save \((.+)\): (.+)$/, (m) => t("Couldn't replace the earlier save ({name}): {why}", { name: m[1], why: m[2] })],
  [/^couldn't move the new save into place \((.+)\): (.+)$/, (m) => t("Couldn't move the new save into place ({name}): {why}", { name: m[1], why: m[2] })],
  [/^no recoverable session: (.+)$/, (m) => t("No session to recover: {why}", { why: m[1] })],
  [/^Can't open the picture: (.+)$/, (m) => t("Can't open the picture: {why}", { why: m[1] })],
  [
    /^This computer's video encoder can't make an MP4 of (\d+) x (\d+) at (\d+) fps\. Try a smaller width or a lower frame rate\. \((.+)\)$/,
    (m) => t("This computer's video encoder can't make an MP4 of {w} x {h} at {fps} fps. Try a smaller width or a lower frame rate. ({why})", { w: m[1], h: m[2], fps: m[3], why: m[4] }),
  ],
  [/^could not write (.+): (.+)$/, (m) => t("Could not write {path}: {why}", { path: m[1], why: m[2] })],
  [/^could not create (.+): (.+)$/, (m) => t("Could not create {path}: {why}", { path: m[1], why: m[2] })],
  [/^could not open (.+): (.+)$/, (m) => t("Could not open {path}: {why}", { path: m[1], why: m[2] })],
  [/^capture window is gone: (.+)$/, (m) => t("The window to record is gone: {why}", { why: m[1] })],
  [/^engine failed to start: (.+)$/, (m) => t("The engine failed to start: {why}", { why: m[1] })],
  [/^"(.+)" can't be a hotkey: use a letter, a digit or F1 to F12$/, (m) => t('"{key}" can\'t be a hotkey: use a letter, a digit or F1 to F12', { key: m[1] })],
];

/// Marks a string as a dictionary key without looking it up: the key check reads these.
const tk = (text: string) => text;

/// Whole engine messages, translated as they stand.
const ENGINE_SENTENCES = new Set([
  tk("capture region is empty"),
  tk("capture window is too small"),
  tk("recording already stopped"),
  tk("No frames were captured, screen capture failed to start."),
  tk("Capture ended unexpectedly (monitor disconnected?). Kept the frames recorded up to that point."),
  tk("There was no clip to add this take to."),
  tk("The added take's camera isn't shown: a clip has one camera track."),
  tk("the takes were recorded in different formats"),
  tk("the MP4 encoder stopped unexpectedly"),
  tk("a stroke needs at least one point"),
  tk("this take has no camera"),
  tk("the camera did not start"),
  tk("the camera stopped sending frames"),
  tk("camera unavailable"),
  tk("Vuoom didn't hear any speech in this take."),
  tk("This take has no recorded sound to make captions from."),
  tk("Captions need a newer processor (most PCs from 2015 on)."),
  tk("Captions need a newer processor (with AVX2, most PCs from 2015 on)"),
  tk("Couldn't download the speech model. Check your internet connection and try again."),
  tk("There are no captions in the part of the video that plays."),
  tk("trim range too short"),
  tk("no room for a zoom segment here"),
  tk("This project's frames are missing, so it can't be opened."),
  tk("This project has no readable frames, so it can't be opened."),
  tk("This project wasn't saved completely: its project file is missing."),
  tk("This folder isn't a Vuoom project (it has no project file)."),
  tk("pick a folder to save the project in"),
  tk("no recoverable session"),
  tk("no free file name left in that folder"),
  tk("Can't clear storage while recording."),
  tk("That file isn't a picture Vuoom can read."),
  tk("That picture is too narrow to use."),
  tk("That picture is too big for a pointer."),
]);

/// Map raw engine error strings to friendly, actionable copy in the interface language,
/// before they hit a status line. Anything unrecognized passes through unchanged.
export function friendlyError(e: unknown): string {
  const msg = String(e);
  const friendly = FRIENDLY.find(([key]) => msg.includes(key));
  if (friendly) return friendly[1]();
  if (ENGINE_SENTENCES.has(msg)) return t(msg);
  for (const [pattern, say] of ENGINE_PATTERNS) {
    const m = pattern.exec(msg);
    if (m) return say(m);
  }
  return msg;
}

/// How long something has left, in words: "About 2 min left", or "" until there's a fair
/// idea (the first moments of an export say little about the rest).
export const timeLeft = (done: number, startedMs: number, nowMs: number): string => {
  if (done < 0.03 || done >= 1) return "";
  const left = ((nowMs - startedMs) / 1000) * ((1 - done) / done);
  if (left < 5) return t("Almost done");
  if (left < 60) return t("About {seconds} s left", { seconds: Math.ceil(left / 5) * 5 });
  return t("About {minutes} min left", { minutes: Math.ceil(left / 60) });
};
