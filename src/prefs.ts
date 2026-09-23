// User preferences and workspace layout, persisted per machine in localStorage.
// Every setting is a signal, so any screen reading it re-renders when it changes, and
// every write is saved immediately. Storage failures (private mode, quota) are ignored:
// the app then simply starts from defaults next time.
import { createSignal, type Accessor } from "solid-js";

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

/** Recording, editing and interface preferences (Settings dialog). */
export const prefs = {
  /** Capture frame-rate cap for new recordings. */
  captureFps: persisted<number>("capture-fps", 60),
  /** Countdown before capture starts, in seconds (0 = start immediately). */
  countdown: persisted<number>("countdown", 3),
  /** Zoom strength applied by Ctrl+Shift+Z while recording (1 = zoom off). */
  recordZoom: persisted<number>("record-zoom", 1.8),
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
  inspectorOpen: persisted<boolean>("inspector-open", true),
  inspectorW: persisted<number>("inspector-w", 312),
  timelineOpen: persisted<boolean>("timeline-open", true),
  timelineH: persisted<number>("timeline-h", 268),
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
