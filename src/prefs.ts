// User preferences and workspace layout, persisted per machine in localStorage.
// Every setting is a signal, so any screen reading it re-renders when it changes, and
// every write is saved immediately. Storage failures (private mode, quota) are ignored:
// the app then simply starts from defaults next time.
import { createSignal, type Accessor } from "solid-js";
import type { CursorMode } from "./types";

const PREFIX = "vuoom-pref:";

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw === null) return fallback;
    const v = JSON.parse(raw) as T;
    if (fallback === null) return v;
    return typeof v === typeof fallback ? v : fallback;
  } catch {
    return fallback;
  }
}

export interface Persisted<T> {
  (): T;
  set: (v: T) => void;
  reset: () => void;
}

function persisted<T>(key: string, fallback: T): Persisted<T> {
  const [get, set] = createSignal<T>(read(key, fallback));
  const acc = get as Accessor<T> & Partial<Persisted<T>>;
  acc.set = (v: T) => {
    set(() => v);
    try {
      localStorage.setItem(PREFIX + key, JSON.stringify(v));
    } catch {
      /* storage unavailable */
    }
  };
  acc.reset = () => acc.set!(fallback);
  return acc as Persisted<T>;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** The pointer mode implied by the older show/hide preference, else the smooth default. */
function legacyCursorMode(): CursorMode {
  const old = read<boolean | null>("capture-cursor", null);
  return old === null ? "smooth" : old ? "show" : "hide";
}

/** Recording, editing and interface preferences (Settings dialog). */
export const prefs = {
  /** Capture frame-rate cap for new recordings. */
  captureFps: persisted<number>("capture-fps", 60),
  /**
   * The pointer in new takes: re-drawn and smoothed (the real one hidden), captured as is,
   * or left out. Installs that chose show/hide before this option existed keep that.
   */
  cursorMode: persisted<CursorMode>("cursor-mode", legacyCursorMode()),
  /** Record the microphone (narration) with new takes. */
  recordMic: persisted<boolean>("record-mic", false),
  /** Microphone endpoint id; null = the system default input. */
  micDevice: persisted<string | null>("mic-device", null),
  /** Record everything the computer plays (app sounds, a video being demoed). */
  recordSystem: persisted<boolean>("record-system", false),
  /** Record the webcam as a bubble over the take. */
  recordCamera: persisted<boolean>("record-camera", false),
  /** The chosen camera's id; `null` is the first camera. */
  cameraDevice: persisted<string | null>("camera-device", null),
  /** Countdown before capture starts, in seconds (0 = start immediately). */
  countdown: persisted<number>("countdown", 3),
  /** Zoom strength applied by Ctrl+Shift+Z while recording (1 = zoom off). */
  recordZoom: persisted<number>("record-zoom", 1.8),
  /** What every new take starts with (each can be changed per clip in the editor). */
  newClicks: persisted<boolean>("new-clicks", true),
  newKeys: persisted<boolean>("new-keys", true),
  newFrame: persisted<"none" | "subtle" | "studio">("new-frame", "subtle"),
  newDenoise: persisted<boolean>("new-denoise", true),
  /** Magnetic snapping of timeline edges (Alt still bypasses it per drag). */
  snapping: persisted<boolean>("snapping", true),
  /** Hover hints, coachmarks and lane hints for newcomers. */
  hints: persisted<boolean>("hints", true),
  /** Tone down interface animation. */
  reduceMotion: persisted<boolean>("reduce-motion", false),
  /** Default export format when the export dialog opens. */
  exportFormat: persisted<"gif" | "mp4">("export-format", "gif"),
  /** Loop playback by default (GIFs loop). */
  loop: persisted<boolean>("loop", false),
  /** Preview playback speed (does not affect the export). */
  previewRate: persisted<number>("preview-rate", 1),
  /** Last export settings per format, restored the next time the dialog opens. */
  exportGif: persisted<ExportSettings | null>("export-gif", null),
  exportMp4: persisted<ExportSettings | null>("export-mp4", null),
};

export interface ExportSettings {
  preset: "readme" | "hq" | "custom";
  fps: number;
  width: number;
  quality: number;
}

/** Workspace layout: which panels are open and how large they are. */
export const layout = {
  /** Tool rail shows labels under icons (wide) or icons only (compact). */
  railCompact: persisted<boolean>("rail-compact", false),
  railOpen: persisted<boolean>("rail-open", true),
  /** The recording panel's size: the bigger live preview, or compact. */
  panelLarge: persisted<boolean>("panel-large", false),
  inspectorOpen: persisted<boolean>("inspector-open", true),
  inspectorW: persisted<number>("inspector-w", 312),
  timelineOpen: persisted<boolean>("timeline-open", true),
  timelineH: persisted<number>("timeline-h", 300),
  /** All annotation bars on one lane instead of one lane each. */
  compactNotes: persisted<boolean>("compact-notes", false),
  /** Rule-of-thirds + center guides over the stage (G). */
  guides: persisted<boolean>("guides", false),
  /** Frame thumbnails along the top of the timeline. */
  filmstrip: persisted<boolean>("filmstrip", true),
};

export const INSPECTOR_MIN = 260;
export const INSPECTOR_MAX = 520;
export const TIMELINE_MIN = 132;
export const TIMELINE_MAX = 560;

export const setInspectorW = (w: number) =>
  layout.inspectorW.set(Math.round(clamp(w, INSPECTOR_MIN, INSPECTOR_MAX)));
export const setTimelineH = (h: number) =>
  layout.timelineH.set(Math.round(clamp(h, TIMELINE_MIN, TIMELINE_MAX)));

/** Restore every panel to its default size and visibility. */
export function resetLayout(): void {
  for (const p of Object.values(layout)) p.reset();
}

// Collapsible inspector sections remember whether they are open, keyed by section id.
const [sections, setSections] = createSignal<Record<string, boolean>>(read("sections", {}));
export const sectionOpen = (id: string, fallback = true): boolean => sections()[id] ?? fallback;
export function toggleSection(id: string, fallback = true): void {
  const next = { ...sections(), [id]: !sectionOpen(id, fallback) };
  setSections(next);
  try {
    localStorage.setItem(`${PREFIX}sections`, JSON.stringify(next));
  } catch {
    /* storage unavailable */
  }
}
