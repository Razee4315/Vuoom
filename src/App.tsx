import { createSignal, createEffect, onMount, onCleanup, For, Show, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save, open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import RecordOverlay from "./RecordOverlay";
import WindowControls from "./WindowControls";
import ThemeMenu from "./ThemeMenu";
import { applyTheme, initialTheme } from "./themes";
import { PreviewClient } from "./preview";
import { LogoWordmark } from "./Logo";
import "./App.css";

type Tool = "select" | "text" | "arrow" | "box";
type Vec2 = { x: number; y: number };

/** Mirrors src-tauri session::RecordingSummary. */
interface RecordingSummary {
  duration: number;
  frames: number;
  zooms: number;
}

interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}
interface TimeRange {
  start: number;
  end: number;
  fade_in: number;
  fade_out: number;
}
// glam DVec2 serializes as a [x, y] array.
type SerVec = [number, number];
interface TextAnn {
  id: number;
  text: string;
  pos: SerVec;
  font_size: number;
  color: Color;
  bold: boolean;
  italic: boolean;
  range: TimeRange;
}
interface ArrowAnn {
  id: number;
  from: SerVec;
  to: SerVec;
  color: Color;
  thickness: number;
  range: TimeRange;
}
interface BoxAnn {
  id: number;
  rect: { x: number; y: number; w: number; h: number };
  color: Color;
  thickness: number;
  filled: boolean;
  range: TimeRange;
}
interface AnnotationSet {
  texts: TextAnn[];
  arrows: ArrowAnn[];
  highlights: BoxAnn[];
}

/** Mirrors vuoom_zoom::ZoomKeyframe (mode is unused by the timeline). */
interface ZoomSeg {
  start: number;
  end: number;
  amount: number;
}
interface SpeedRegion {
  start: number;
  end: number;
  factor: number;
}
interface Trim {
  start: number;
  end: number;
}
/** Mirrors src-tauri session::ClipState. */
interface ClipState {
  duration: number;
  trim: Trim | null;
  speed_regions: SpeedRegion[];
  zooms: ZoomSeg[];
}

/** Played duration after trim + speed regions (mirrors vuoom_project::output_duration). */
function outputDuration(duration: number, trim: Trim | null, regions: SpeedRegion[]): number {
  const t0 = trim?.start ?? 0;
  const t1 = trim?.end ?? duration;
  let out = 0;
  let cursor = t0;
  const sorted = [...regions]
    .filter((r) => r.end > r.start && r.factor > 0)
    .sort((a, b) => a.start - b.start);
  for (const r of sorted) {
    const s = Math.max(r.start, t0);
    const e = Math.min(r.end, t1);
    if (e <= s || s < cursor) continue;
    out += s - cursor + (e - s) / r.factor;
    cursor = e;
  }
  if (cursor < t1) out += t1 - cursor;
  return out;
}

type Kind = "text" | "arrow" | "box";
interface Selection {
  kind: Kind;
  id: number;
}

/// Quick-pick annotation colors (white, ink, record red, box yellow, green, text blue).
const PRESET_COLORS = ["#ffffff", "#0e0e0f", "#e5484d", "#ffd23f", "#30a46c", "#6ea8ff"];

const TOOLS: { id: Tool; label: string; hint: string }[] = [
  { id: "select", label: "Select", hint: "Click an element to select, drag to move, drag a handle to resize." },
  { id: "text", label: "Text", hint: "Click on the video to drop a text label." },
  { id: "arrow", label: "Arrow", hint: "Drag on the video to draw an arrow." },
  { id: "box", label: "Box", hint: "Drag on the video to draw a highlight box." },
];

// ── small helpers ──────────────────────────────────────────────────────────────
const clamp01 = (n: number) => Math.min(1, Math.max(0, n));
const v2 = (p: SerVec | Vec2): Vec2 => (Array.isArray(p) ? { x: p[0], y: p[1] } : p);
const cssColor = (c: Color) =>
  `rgba(${Math.round(c.r * 255)},${Math.round(c.g * 255)},${Math.round(c.b * 255)},${c.a})`;
const h2 = (n: number) => Math.round(clamp01(n) * 255).toString(16).padStart(2, "0");
const rgbHex = (c: Color) => `#${h2(c.r)}${h2(c.g)}${h2(c.b)}`;
const hexRgb = (hex: string) => {
  const n = parseInt(hex.slice(1), 16);
  return { r: ((n >> 16) & 255) / 255, g: ((n >> 8) & 255) / 255, b: (n & 255) / 255 };
};
const fmt = (t: number) => {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
};
// Playhead readout with tenths, so annotations can be aligned precisely.
const fmtT = (t: number) => `${fmt(t)}.${Math.floor((t % 1) * 10)}`;
const distToSeg = (p: Vec2, a: Vec2, b: Vec2) => {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy || 1e-9;
  let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
  t = Math.min(1, Math.max(0, t));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
};

// Drag state for the interactive overlay.
type Drag =
  | { mode: "create-arrow"; start: Vec2; cur: Vec2 }
  | { mode: "create-box"; start: Vec2; cur: Vec2 }
  | { mode: "move"; kind: Kind; id: number; grab: Vec2; orig: number[]; geom: number[] }
  | { mode: "resize"; kind: Kind; id: number; handle: string; orig: number[]; geom: number[] }
  | null;

function App() {
  const [tool, setTool] = createSignal<Tool>("select");
  const [status, setStatus] = createSignal("Ready");
  const [projectName, setProjectName] = createSignal("Untitled");
  const [editingText, setEditingText] = createSignal<number | null>(null);
  const [theme, setTheme] = createSignal(initialTheme());
  const [hasClip, setHasClip] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [playhead, setPlayhead] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);

  const [anns, setAnns] = createSignal<AnnotationSet>({ texts: [], arrows: [], highlights: [] });
  const [zooms, setZooms] = createSignal<ZoomSeg[]>([]);
  const [trim, setTrimState] = createSignal<Trim | null>(null);
  const [speed, setSpeed] = createSignal<SpeedRegion[]>([]);
  const [selZoom, setSelZoom] = createSignal<number | null>(null);
  const [selSpeed, setSelSpeed] = createSignal<number | null>(null);
  const [skimFactor, setSkimFactor] = createSignal(3);
  const [selected, setSelected] = createSignal<Selection | null>(null);
  const [drag, setDrag] = createSignal<Drag>(null);
  const [stage, setStage] = createSignal({ w: 1, h: 1 });
  const [frameAspect, setFrameAspect] = createSignal(16 / 9);
  const [showExport, setShowExport] = createSignal(false);
  const [recordPhase, setRecordPhase] = createSignal<"idle" | "active">("idle");
  const [backdrop, setBackdrop] = createSignal<string | null>(null);
  const [zoomAmount, setZoomAmount] = createSignal(1.8);

  const preview = new PreviewClient();
  let canvasEl: HTMLCanvasElement | undefined;
  let stageEl: HTMLDivElement | undefined;

  const onContextMenu = (e: MouseEvent) => {
    const el = e.target as HTMLElement;
    if (!el.closest("input, textarea, [contenteditable=true]")) e.preventDefault();
  };

  // Desktop-app hardening: this is a native app, not a website. Swallow the WebView2
  // browser chrome accelerators (Ctrl+J downloads, Ctrl+F find, Ctrl+R reload, F5, zoom
  // keys, Alt-nav, devtools chords) and map the useful ones to app actions instead.
  const BROWSER_CODES = new Set([
    "KeyJ", "KeyF", "KeyG", "KeyH", "KeyL", "KeyK", "KeyT", "KeyN",
    "KeyP", "KeyU", "KeyD", "KeyW", "Equal", "Minus", "Digit0",
  ]);
  const onGlobalKey = (e: KeyboardEvent) => {
    const inField = (e.target as HTMLElement).closest("input, textarea");
    if (e.ctrlKey && !e.shiftKey && !e.altKey) {
      if (e.code === "KeyS") {
        e.preventDefault();
        if (hasClip()) void onSaveProject();
        return;
      }
      if (e.code === "KeyO") {
        e.preventDefault();
        void onOpenProject();
        return;
      }
      if (e.code === "KeyE") {
        e.preventDefault();
        if (hasClip()) setShowExport(true);
        return;
      }
      if (e.code === "KeyA" && !inField) {
        e.preventDefault();
        return;
      }
    }
    const browserChord =
      (e.ctrlKey && !e.shiftKey && !e.altKey && BROWSER_CODES.has(e.code)) ||
      (e.ctrlKey && !e.shiftKey && e.code === "KeyR") ||
      (e.ctrlKey && e.shiftKey && (e.code === "KeyI" || e.code === "KeyJ" || e.code === "KeyC") && !inField) ||
      (e.altKey && (e.code === "ArrowLeft" || e.code === "ArrowRight") && !inField) ||
      e.code === "F3" || e.code === "F5" || e.code === "F7" || e.code === "F11";
    if (browserChord) {
      e.preventDefault();
      e.stopPropagation();
    }
  };
  const onWheelGuard = (e: WheelEvent) => {
    // Ctrl+wheel is browser page-zoom — meaningless in a desktop editor.
    if (e.ctrlKey) e.preventDefault();
  };

  onMount(async () => {
    applyTheme(theme());
    document.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("keydown", onGlobalKey, true);
    window.addEventListener("wheel", onWheelGuard, { passive: false });
    window.addEventListener("keydown", onKey);
    if (canvasEl) preview.attach(canvasEl);
    preview.onAspectChange((a) => setFrameAspect(a));
    if (stageEl) {
      const ro = new ResizeObserver(() => {
        if (stageEl) setStage({ w: stageEl.clientWidth, h: stageEl.clientHeight });
      });
      ro.observe(stageEl);
      onCleanup(() => ro.disconnect());
    }
    await connectEngine();
  });

  // The engine (GPU compositor + preview server) boots on a background thread; retry
  // until it's up, keeping the launch splash visible so startup never looks dead.
  const hideSplash = () => {
    const el = document.getElementById("splash");
    if (el) {
      el.classList.add("hide");
      setTimeout(() => el.remove(), 300);
    }
  };
  const connectEngine = async () => {
    for (let tries = 0; tries < 200; tries++) {
      try {
        const port = await invoke<number>("preview_port");
        preview.connect(port);
        setStatus("Ready — press Record to capture your screen");
        hideSplash();
        return;
      } catch (e) {
        const msg = String(e);
        if (!msg.includes("engine-starting")) {
          setStatus(`Engine error: ${msg}`);
          hideSplash();
          return;
        }
        await new Promise((r) => setTimeout(r, 150));
      }
    }
    setStatus("The engine did not start — try restarting Vuoom.");
    hideSplash();
  };
  onCleanup(() => {
    document.removeEventListener("contextmenu", onContextMenu);
    window.removeEventListener("keydown", onGlobalKey, true);
    window.removeEventListener("wheel", onWheelGuard);
    window.removeEventListener("keydown", onKey);
    preview.disconnect();
  });

  // ── seek throttling (shared by scrubbing, playback, live edits) ────────────────
  let seekBusy = false;
  let seekPending: number | null = null;
  const pushSeek = async (t: number) => {
    if (seekBusy) {
      seekPending = t;
      return;
    }
    seekBusy = true;
    try {
      await invoke("seek", { t });
    } catch {
      /* no clip yet */
    }
    seekBusy = false;
    if (seekPending !== null) {
      const n = seekPending;
      seekPending = null;
      void pushSeek(n);
    }
  };
  const scrub = (t: number) => {
    setPlayhead(t);
    void pushSeek(t);
  };

  const refresh = async () => {
    try {
      setAnns(await invoke<AnnotationSet>("list_annotations"));
    } catch {
      /* no recording */
    }
  };
  /// Re-sync trim / speed / zooms from the backend's clip state.
  const refreshClip = async () => {
    try {
      const cs = await invoke<ClipState>("clip_state");
      setZooms(cs.zooms);
      setTrimState(cs.trim);
      setSpeed(cs.speed_regions);
    } catch {
      /* no recording */
    }
  };

  // ── playback transport (honors trim bounds + speed regions) ─────────────────────
  const tStart = () => trim()?.start ?? 0;
  const tEnd = () => trim()?.end ?? duration();
  const factorAt = (t: number) =>
    speed().find((r) => t >= r.start && t < r.end)?.factor ?? 1;

  let raf = 0;
  let lastTs = 0;
  const tick = (ts: number) => {
    if (!playing()) return;
    if (lastTs) {
      let t = playhead() + ((ts - lastTs) / 1000) * factorAt(playhead());
      if (t >= tEnd()) {
        t = tEnd();
        setPlaying(false);
      }
      setPlayhead(t);
      void pushSeek(t);
    }
    lastTs = ts;
    if (playing()) raf = requestAnimationFrame(tick);
  };
  const togglePlay = () => {
    if (!hasClip()) return;
    if (playing()) {
      setPlaying(false);
      cancelAnimationFrame(raf);
    } else {
      if (playhead() >= tEnd() - 1e-3 || playhead() < tStart()) scrub(tStart());
      setPlaying(true);
      lastTs = 0;
      raf = requestAnimationFrame(tick);
    }
  };
  const restart = () => {
    setPlaying(false);
    cancelAnimationFrame(raf);
    scrub(tStart());
  };

  const onKey = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest("input, textarea")) return;
    if (e.ctrlKey && e.shiftKey && e.code === "KeyR" && recordPhase() === "idle") {
      e.preventDefault();
      void startRecord();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selZoom() !== null) {
      e.preventDefault();
      void deleteSelectedZoom();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selSpeed() !== null) {
      e.preventDefault();
      void deleteSelectedSpeed();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selected()) {
      e.preventDefault();
      void deleteSelected();
    } else if (e.key === "Escape" && (selected() || selZoom() !== null || selSpeed() !== null)) {
      setSelected(null);
      setSelZoom(null);
      setSelSpeed(null);
    } else if (e.code === "Space" && hasClip()) {
      e.preventDefault();
      togglePlay();
    }
  };

  // ── coordinate mapping ──────────────────────────────────────────────────────────
  const norm = (e: PointerEvent): Vec2 => {
    const r = stageEl!.getBoundingClientRect();
    return { x: clamp01((e.clientX - r.left) / r.width), y: clamp01((e.clientY - r.top) / r.height) };
  };
  const px = (n: Vec2) => ({ x: n.x * stage().w, y: n.y * stage().h });

  // Visible at the current playhead. A selected element also shows while PAUSED (so it
  // stays editable when scrubbed past its window) — but never during playback, which
  // must match the exported GIF exactly.
  const inView = (r: TimeRange, sel: boolean) =>
    (playhead() >= r.start && playhead() < r.end) || (sel && !playing());

  // ── live edit (update + re-composite), throttled ─────────────────────────────────
  let editBusy = false;
  let editPending: (() => Promise<void>) | null = null;
  const pushEdit = async (op: () => Promise<void>) => {
    if (editBusy) {
      editPending = op;
      return;
    }
    editBusy = true;
    try {
      await op();
      await invoke("seek", { t: playhead() });
    } catch {
      /* ignore */
    }
    editBusy = false;
    if (editPending) {
      const n = editPending;
      editPending = null;
      void pushEdit(n);
    }
  };

  // Geometry of an annotation as a flat number[] (for the drag override + live updates).
  const geomOf = (kind: Kind, id: number): number[] => {
    if (kind === "box") {
      const b = anns().highlights.find((a) => a.id === id)!;
      return [b.rect.x, b.rect.y, b.rect.w, b.rect.h];
    }
    if (kind === "arrow") {
      const a = anns().arrows.find((x) => x.id === id)!;
      return [a.from[0], a.from[1], a.to[0], a.to[1]];
    }
    const t = anns().texts.find((x) => x.id === id)!;
    return [t.pos[0], t.pos[1]];
  };
  // The geometry the overlay should draw for an item (drag override if it is being dragged).
  const liveGeom = (kind: Kind, id: number): number[] => {
    const d = drag();
    if (d && (d.mode === "move" || d.mode === "resize") && d.kind === kind && d.id === id) return d.geom;
    return geomOf(kind, id);
  };
  const applyGeom = async (kind: Kind, id: number, g: number[]) => {
    if (kind === "box") await invoke("update_box", { id, x: g[0], y: g[1], w: g[2], h: g[3] });
    else if (kind === "arrow")
      await invoke("update_arrow", { id, fx: g[0], fy: g[1], tx: g[2], ty: g[3] });
    else await invoke("update_text", { id, x: g[0], y: g[1] });
  };
  // ── hit testing (normalized) ─────────────────────────────────────────────────────
  const TOL = () => 11 / Math.max(stage().w, stage().h); // ~11px grab radius in normalized space
  const handleAt = (p: Vec2): string | null => {
    const s = selected();
    if (!s) return null;
    const g = liveGeom(s.kind, s.id);
    const near = (hx: number, hy: number) => Math.hypot(p.x - hx, p.y - hy) <= TOL() * 1.4;
    if (s.kind === "box") {
      const [x, y, w, h] = g;
      if (near(x, y)) return "nw";
      if (near(x + w, y)) return "ne";
      if (near(x, y + h)) return "sw";
      if (near(x + w, y + h)) return "se";
    } else if (s.kind === "arrow") {
      if (near(g[0], g[1])) return "from";
      if (near(g[2], g[3])) return "to";
    }
    return null;
  };
  const hitTest = (p: Vec2): Selection | null => {
    for (const b of anns().highlights) {
      if (!inView(b.range, false)) continue;
      const [x, y, w, h] = [b.rect.x, b.rect.y, b.rect.w, b.rect.h];
      if (p.x >= x - TOL() && p.x <= x + w + TOL() && p.y >= y - TOL() && p.y <= y + h + TOL())
        return { kind: "box", id: b.id };
    }
    for (const a of anns().arrows) {
      if (!inView(a.range, false)) continue;
      if (distToSeg(p, v2(a.from), v2(a.to)) <= TOL() * 1.5) return { kind: "arrow", id: a.id };
    }
    for (const t of anns().texts) {
      if (!inView(t.range, false)) continue;
      const pos = v2(t.pos);
      // font_size is a fraction of stage HEIGHT; convert the glyph width into
      // width-normalized space or wide text on a wide stage can't be clicked.
      const wApprox = Math.max(
        t.text.length * t.font_size * 0.6 * (stage().h / Math.max(stage().w, 1)),
        0.05,
      );
      // The glyphs sit between pos.y (top) and pos.y + font_size (baseline); pad by TOL.
      if (
        p.x >= pos.x - TOL() &&
        p.x <= pos.x + wApprox + TOL() &&
        p.y >= pos.y - TOL() &&
        p.y <= pos.y + t.font_size + TOL()
      )
        return { kind: "text", id: t.id };
    }
    return null;
  };

  // ── pointer interaction on the overlay ───────────────────────────────────────────
  const onPointerDown = async (e: PointerEvent) => {
    if (!hasClip()) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    const p = norm(e);
    const t = tool();

    if (t === "text") {
      const id = await invoke<number>("add_text", { text: "Text", x: p.x, y: p.y, t: playhead() });
      await refresh();
      await pushSeek(playhead());
      setSelZoom(null);
      setSelSpeed(null);
      setSelected({ kind: "text", id });
      setEditingText(id);
      setTool("select");
      return;
    }
    if (t === "arrow") {
      setDrag({ mode: "create-arrow", start: p, cur: p });
      return;
    }
    if (t === "box") {
      setDrag({ mode: "create-box", start: p, cur: p });
      return;
    }

    // Second click of a double-click on a text label → inline edit. Detected here via
    // e.detail because pointer capture can swallow the synthesized dblclick event.
    if (e.detail >= 2) {
      const hit = hitTest(p);
      if (hit?.kind === "text") {
        setDrag(null);
        setSelected(hit);
        setEditingText(hit.id);
        return;
      }
    }

    // select tool: handle → resize, body → move, empty → deselect
    const h = handleAt(p);
    if (h && selected()) {
      const s = selected()!;
      const g = geomOf(s.kind, s.id);
      setDrag({ mode: "resize", kind: s.kind, id: s.id, handle: h, orig: g, geom: g.slice() });
      return;
    }
    const hit = hitTest(p);
    if (hit) {
      setSelZoom(null);
      setSelSpeed(null);
      setSelected(hit);
      const g = geomOf(hit.kind, hit.id);
      setDrag({ mode: "move", kind: hit.kind, id: hit.id, grab: p, orig: g, geom: g.slice() });
    }
    // Clicking empty space keeps the current selection so the inspector stays open;
    // deselect deliberately with Esc or the inspector's ✕.
  };

  const onPointerMove = (e: PointerEvent) => {
    const d = drag();
    if (!d) return;
    const p = norm(e);
    if (d.mode === "create-arrow" || d.mode === "create-box") {
      setDrag({ ...d, cur: p });
      return;
    }
    if (d.mode === "move") {
      const og = d.orig;
      const dx = p.x - d.grab.x;
      const dy = p.y - d.grab.y;
      let g: number[];
      if (d.kind === "box") g = [clamp01(og[0] + dx), clamp01(og[1] + dy), og[2], og[3]];
      else if (d.kind === "arrow")
        g = [clamp01(og[0] + dx), clamp01(og[1] + dy), clamp01(og[2] + dx), clamp01(og[3] + dy)];
      else g = [clamp01(og[0] + dx), clamp01(og[1] + dy)];
      setDrag({ ...d, geom: g });
    } else if (d.mode === "resize") {
      const og = d.orig;
      let g = og.slice();
      if (d.kind === "box") {
        let [x, y, w, h] = og;
        let x2 = x + w;
        let y2 = y + h;
        if (d.handle.includes("w")) x = p.x;
        if (d.handle.includes("e")) x2 = p.x;
        if (d.handle.includes("n")) y = p.y;
        if (d.handle.includes("s")) y2 = p.y;
        g = [Math.min(x, x2), Math.min(y, y2), Math.abs(x2 - x), Math.abs(y2 - y)];
      } else if (d.kind === "arrow") {
        g = d.handle === "from" ? [p.x, p.y, og[2], og[3]] : [og[0], og[1], p.x, p.y];
      }
      setDrag({ ...d, geom: g });
    }
  };

  const onPointerUp = async (e: PointerEvent) => {
    const d = drag();
    if (!d) return;
    const p = norm(e);
    if (d.mode === "create-arrow") {
      setDrag(null);
      if (Math.hypot(p.x - d.start.x, p.y - d.start.y) > 0.01) {
        const id = await invoke<number>("add_arrow", {
          fx: d.start.x,
          fy: d.start.y,
          tx: p.x,
          ty: p.y,
          t: playhead(),
        });
        await refresh();
        await pushSeek(playhead());
        setSelZoom(null);
        setSelSpeed(null);
        setSelected({ kind: "arrow", id });
        setTool("select");
      }
    } else if (d.mode === "create-box") {
      setDrag(null);
      const x = Math.min(d.start.x, p.x);
      const y = Math.min(d.start.y, p.y);
      const w = Math.abs(p.x - d.start.x);
      const h = Math.abs(p.y - d.start.y);
      if (w > 0.01 && h > 0.01) {
        const id = await invoke<number>("add_box", { x, y, w, h, t: playhead() });
        await refresh();
        await pushSeek(playhead());
        setSelZoom(null);
        setSelSpeed(null);
        setSelected({ kind: "box", id });
        setTool("select");
      }
    } else {
      // Commit the moved/resized geometry and refresh the source of truth BEFORE clearing
      // the drag, so the overlay never flashes back to the pre-drag position for a frame.
      await applyGeom(d.kind, d.id, d.geom);
      await refresh();
      setDrag(null);
    }
  };

  // ── selected-element editing ─────────────────────────────────────────────────────
  const selectedText = () => {
    const s = selected();
    return s?.kind === "text" ? anns().texts.find((t) => t.id === s.id) : undefined;
  };
  const selectedColor = (): Color | undefined => {
    const s = selected();
    if (!s) return undefined;
    if (s.kind === "text") return anns().texts.find((t) => t.id === s.id)?.color;
    if (s.kind === "arrow") return anns().arrows.find((a) => a.id === s.id)?.color;
    return anns().highlights.find((b) => b.id === s.id)?.color;
  };
  const setColor = async (hex: string) => {
    const s = selected();
    if (!s) return;
    const c = hexRgb(hex);
    await invoke("set_annotation_color", { id: s.id, r: c.r, g: c.g, b: c.b });
    await refresh();
    await pushSeek(playhead());
  };
  const editText = async (text: string) => {
    const s = selected();
    if (s?.kind !== "text") return;
    await invoke("update_text", { id: s.id, text });
    await refresh();
    await pushSeek(playhead());
  };
  const editFontSize = async (size: number) => {
    const s = selected();
    if (s?.kind !== "text") return;
    await invoke("update_text", { id: s.id, fontSize: size });
    await refresh();
    await pushSeek(playhead());
  };
  const editTextStyle = async (patch: { bold?: boolean; italic?: boolean }) => {
    const s = selected();
    if (s?.kind !== "text") return;
    await invoke("update_text", { id: s.id, ...patch });
    await refresh();
    await pushSeek(playhead());
  };
  const selectedRange = (): TimeRange | undefined => {
    const s = selected();
    if (!s) return undefined;
    if (s.kind === "text") return anns().texts.find((t) => t.id === s.id)?.range;
    if (s.kind === "arrow") return anns().arrows.find((a) => a.id === s.id)?.range;
    return anns().highlights.find((b) => b.id === s.id)?.range;
  };
  const editRange = async (start: number, end: number) => {
    const s = selected();
    if (!s || Number.isNaN(start) || Number.isNaN(end)) return;
    await invoke("update_annotation_range", { id: s.id, start, end });
    await refresh();
    await pushSeek(playhead());
  };
  const deleteSelected = async () => {
    const s = selected();
    if (!s) return;
    await invoke("delete_annotation", { id: s.id });
    setSelected(null);
    await refresh();
    await pushSeek(playhead());
  };

  // ── inline text editing ──────────────────────────────────────────────────────────
  const editingTextAnn = () => {
    const id = editingText();
    return id === null ? undefined : anns().texts.find((t) => t.id === id);
  };
  const editTextLive = (text: string) => {
    const id = editingText();
    if (id === null) return;
    void pushEdit(() => invoke("update_text", { id, text }));
  };
  const finishTextEdit = async () => {
    const id = editingText();
    setEditingText(null);
    if (id === null) return;
    await refresh(); // sync the live-typed value before deciding
    const ann = anns().texts.find((t) => t.id === id);
    if (ann && ann.text.trim() === "") {
      await invoke("delete_annotation", { id });
      setSelected(null);
      await refresh();
    }
    await pushSeek(playhead());
  };

  // ── recording / export ───────────────────────────────────────────────────────────
  // The record flow (region selector → countdown → stop bar) runs as an overlay INSIDE
  // this window — the window is excluded from the capture and grown/shrunk by the backend,
  // so the overlay never lands in the recording and we avoid fragile extra webviews.
  const startRecord = async () => {
    try {
      setStatus("Choose the area to record…");
      await invoke("enter_overlay"); // hide editor from capture + go fullscreen
      setBackdrop(null);
      setRecordPhase("active"); // overlay shows immediately (dark + presets)
      // Freeze the desktop behind us as a backdrop to draw on (non-blocking).
      invoke<string>("screenshot")
        .then(setBackdrop)
        .catch(() => setBackdrop(null));
    } catch (e) {
      setRecordPhase("idle");
      setStatus(`Error: ${String(e)}`);
    }
  };

  const onRecordFinished = async (summary: RecordingSummary) => {
    setRecordPhase("idle");
    setBackdrop(null);
    await loadFinishedClip(summary);
  };
  const onRecordCancel = () => {
    setRecordPhase("idle");
    setBackdrop(null);
    setStatus("Recording cancelled");
  };

  const loadFinishedClip = async (summary: RecordingSummary) => {
    setHasClip(true);
    setDuration(summary.duration);
    setSelected(null);
    setSelZoom(null);
    setSelSpeed(null);
    setStatus(`Recorded ${summary.duration.toFixed(1)}s · ${summary.zooms} zooms`);
    await refresh();
    await refreshClip();
    scrub(trim()?.start ?? 0);
  };

  // ── zoom segment editing ───────────────────────────────────────────────────────
  const selectedZoom = () => {
    const i = selZoom();
    return i === null ? undefined : zooms()[i];
  };
  const addZoomAtPlayhead = async () => {
    if (!hasClip()) return;
    try {
      const list = await invoke<ZoomSeg[]>("add_zoom", { t: playhead() });
      setZooms(list);
      const idx = list.findIndex((z) => playhead() >= z.start - 1e-6 && playhead() <= z.end + 1e-6);
      setSelected(null);
      setSelSpeed(null);
      setSelZoom(idx >= 0 ? idx : null);
      await pushSeek(playhead());
      setStatus("Zoom added — drag its edges on the timeline to retime");
    } catch (e) {
      setStatus(`Could not add zoom: ${String(e)}`);
    }
  };
  const applyZoomEdit = async (index: number, start: number, end: number, amount: number) => {
    try {
      const list = await invoke<ZoomSeg[]>("update_zoom", { index, start, end, amount });
      setZooms(list);
      // Re-find the edited segment (the list re-sorts by start).
      const idx = list.findIndex((z) => Math.abs(z.start - Math.min(start, end)) < 0.25);
      if (idx >= 0) setSelZoom(idx);
      await pushSeek(playhead());
    } catch (e) {
      setStatus(`Zoom edit failed: ${String(e)}`);
    }
  };
  const deleteSelectedZoom = async () => {
    const i = selZoom();
    if (i === null) return;
    try {
      setZooms(await invoke<ZoomSeg[]>("delete_zoom", { index: i }));
      setSelZoom(null);
      await pushSeek(playhead());
    } catch (e) {
      setStatus(`Zoom delete failed: ${String(e)}`);
    }
  };

  // ── speed-up dead time ─────────────────────────────────────────────────────────
  const toggleSkim = async () => {
    if (!hasClip()) return;
    try {
      if (speed().length > 0) {
        await invoke("clear_speed");
        setSpeed([]);
        setSelSpeed(null);
        setStatus("Idle stretches back to normal speed");
      } else {
        const f = skimFactor();
        const regions = await invoke<SpeedRegion[]>("auto_speed", { factor: f });
        setSpeed(regions);
        setStatus(
          regions.length > 0
            ? `${regions.length} idle ${regions.length === 1 ? "stretch" : "stretches"} will play at ${f}×`
            : "No idle stretches longer than ~2.5s found",
        );
      }
    } catch (e) {
      setStatus(`Speed-up failed: ${String(e)}`);
    }
  };

  // ── manual speed regions ───────────────────────────────────────────────────────
  const selectedSpeed = () => {
    const i = selSpeed();
    return i === null ? undefined : speed()[i];
  };
  const addSpeedAtPlayhead = async () => {
    if (!hasClip()) return;
    try {
      const start = Math.min(playhead(), Math.max(0, duration() - 0.5));
      const end = Math.min(start + 2, duration());
      const list = await invoke<SpeedRegion[]>("add_speed", {
        start,
        end,
        factor: skimFactor(),
      });
      setSpeed(list);
      const idx = list.findIndex((r) => Math.abs(r.start - start) < 0.01);
      setSelected(null);
      setSelZoom(null);
      setSelSpeed(idx >= 0 ? idx : null);
      setStatus("Speed region added — drag it on the timeline to retime");
    } catch (e) {
      setStatus(`Could not add speed region: ${String(e)}`);
    }
  };
  const applySpeedEdit = async (index: number, start: number, end: number, factor: number) => {
    try {
      const list = await invoke<SpeedRegion[]>("update_speed", { index, start, end, factor });
      setSpeed(list);
      // Re-find the edited region (the list re-sorts by start).
      const idx = list.findIndex((r) => Math.abs(r.start - Math.min(start, end)) < 0.25);
      if (idx >= 0) setSelSpeed(idx);
    } catch (e) {
      setStatus(`Speed edit failed: ${String(e)}`);
    }
  };
  const deleteSelectedSpeed = async () => {
    const i = selSpeed();
    if (i === null) return;
    try {
      setSpeed(await invoke<SpeedRegion[]>("delete_speed", { index: i }));
      setSelSpeed(null);
    } catch (e) {
      setStatus(`Speed delete failed: ${String(e)}`);
    }
  };

  // ── timeline (ruler + tracks + drag-to-scrub) ─────────────────────────────────────
  let tlEl: HTMLDivElement | undefined;
  let tlDrag = false;
  const tlTime = (e: PointerEvent) => {
    const r = tlEl!.getBoundingClientRect();
    return clamp01((e.clientX - r.left) / r.width) * duration();
  };
  const tlSeekFromEvent = (e: PointerEvent) => {
    if (!tlEl || !hasClip() || duration() <= 0) return;
    scrub(tlTime(e));
  };

  // Trim handle dragging (local preview while dragging, committed on release).
  let trimDrag: "start" | "end" | null = null;
  const onTrimDown = (which: "start" | "end") => (e: PointerEvent) => {
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    trimDrag = which;
  };
  const onTrimMove = (e: PointerEvent) => {
    if (!trimDrag || !tlEl) return;
    const t = tlTime(e);
    const cur = trim() ?? { start: 0, end: duration() };
    const next =
      trimDrag === "start"
        ? { start: Math.min(t, cur.end - 0.2), end: cur.end }
        : { start: cur.start, end: Math.max(t, cur.start + 0.2) };
    next.start = Math.max(0, next.start);
    next.end = Math.min(duration(), next.end);
    setTrimState(next);
  };
  const onTrimUp = async () => {
    if (!trimDrag) return;
    trimDrag = null;
    const t = trim();
    if (!t) return;
    try {
      await invoke("set_trim", { start: t.start, end: t.end });
      await refreshClip(); // backend may normalize a full-range trim to null
      if (playhead() < tStart() || playhead() > tEnd()) scrub(tStart());
    } catch (e) {
      setStatus(`Trim failed: ${String(e)}`);
    }
  };

  // Zoom block dragging: grab the middle to move, the edges (8px) to resize.
  const [zoomDrag, setZoomDrag] = createSignal<{
    idx: number;
    mode: "move" | "l" | "r";
    grabT: number;
    orig: ZoomSeg;
    cur: { start: number; end: number };
    moved: boolean;
  } | null>(null);
  const zoomGeom = (idx: number, z: ZoomSeg) => {
    const d = zoomDrag();
    return d && d.idx === idx ? d.cur : { start: z.start, end: z.end };
  };
  const onZoomDown = (idx: number, z: ZoomSeg) => (e: PointerEvent) => {
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const r = el.getBoundingClientRect();
    const mode = e.clientX - r.left < 8 ? "l" : r.right - e.clientX < 8 ? "r" : "move";
    setZoomDrag({ idx, mode, grabT: tlTime(e), orig: { ...z }, cur: { start: z.start, end: z.end }, moved: false });
  };
  const onZoomMove = (e: PointerEvent) => {
    const d = zoomDrag();
    if (!d) return;
    const t = tlTime(e);
    const dt = t - d.grabT;
    let { start, end } = d.orig;
    if (d.mode === "move") {
      const len = end - start;
      start = Math.min(Math.max(0, start + dt), duration() - len);
      end = start + len;
    } else if (d.mode === "l") {
      start = Math.min(Math.max(0, start + dt), end - 0.2);
    } else {
      end = Math.max(Math.min(duration(), end + dt), start + 0.2);
    }
    setZoomDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onZoomUp = async () => {
    const d = zoomDrag();
    if (!d) return;
    setZoomDrag(null);
    setSelected(null);
    setSelSpeed(null);
    if (d.moved) {
      await applyZoomEdit(d.idx, d.cur.start, d.cur.end, zooms()[d.idx]?.amount ?? 1.8);
    } else {
      // A plain click: select the block and jump to it.
      setSelZoom(d.idx);
      scrub(d.orig.start);
    }
  };
  // Speed-band dragging: grab the chip to move the region, its edges (8px) to resize.
  const [speedDrag, setSpeedDrag] = createSignal<{
    idx: number;
    mode: "move" | "l" | "r";
    grabT: number;
    orig: SpeedRegion;
    cur: { start: number; end: number };
    moved: boolean;
  } | null>(null);
  const speedGeom = (idx: number, r: SpeedRegion) => {
    const d = speedDrag();
    return d && d.idx === idx ? d.cur : { start: r.start, end: r.end };
  };
  const onSpeedDown = (idx: number, r: SpeedRegion) => (e: PointerEvent) => {
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = el.getBoundingClientRect();
    const mode = e.clientX - rect.left < 8 ? "l" : rect.right - e.clientX < 8 ? "r" : "move";
    setSpeedDrag({
      idx,
      mode,
      grabT: tlTime(e),
      orig: { ...r },
      cur: { start: r.start, end: r.end },
      moved: false,
    });
  };
  const onSpeedMove = (e: PointerEvent) => {
    const d = speedDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    let { start, end } = d.orig;
    if (d.mode === "move") {
      const len = end - start;
      start = Math.min(Math.max(0, start + dt), duration() - len);
      end = start + len;
    } else if (d.mode === "l") {
      start = Math.min(Math.max(0, start + dt), end - 0.2);
    } else {
      end = Math.max(Math.min(duration(), end + dt), start + 0.2);
    }
    setSpeedDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onSpeedUp = async () => {
    const d = speedDrag();
    if (!d) return;
    setSpeedDrag(null);
    setSelected(null);
    setSelZoom(null);
    if (d.moved) {
      await applySpeedEdit(d.idx, d.cur.start, d.cur.end, speed()[d.idx]?.factor ?? skimFactor());
    } else {
      // A plain click: select the region and jump to it.
      setSelSpeed(d.idx);
      scrub(d.orig.start);
    }
  };

  const pct = (t: number) => (duration() > 0 ? (t / duration()) * 100 : 0);
  const tickStep = () => {
    for (const s of [0.25, 0.5, 1, 2, 5, 10, 15, 30, 60]) {
      if (duration() / s <= 12) return s;
    }
    return 120;
  };
  const ticks = () => {
    const s = tickStep();
    const out: number[] = [];
    for (let t = 0; t <= duration() + 1e-9; t += s) out.push(t);
    return out;
  };
  // Annotation bar dragging: grab the middle to move it in time, the edges to resize
  // how long it stays on screen.
  const [annDrag, setAnnDrag] = createSignal<{
    kind: Kind;
    id: number;
    mode: "move" | "l" | "r";
    grabT: number;
    orig: { start: number; end: number };
    cur: { start: number; end: number };
    moved: boolean;
  } | null>(null);
  const annGeom = (b: { kind: Kind; id: number; start: number; end: number }) => {
    const d = annDrag();
    return d && d.kind === b.kind && d.id === b.id ? d.cur : { start: b.start, end: b.end };
  };
  const onAnnDown =
    (b: { kind: Kind; id: number; start: number; end: number }) => (e: PointerEvent) => {
      e.stopPropagation();
      const el = e.currentTarget as HTMLElement;
      el.setPointerCapture(e.pointerId);
      const r = el.getBoundingClientRect();
      const mode = e.clientX - r.left < 8 ? "l" : r.right - e.clientX < 8 ? "r" : "move";
      setAnnDrag({
        kind: b.kind,
        id: b.id,
        mode,
        grabT: tlTime(e),
        orig: { start: b.start, end: b.end },
        cur: { start: b.start, end: b.end },
        moved: false,
      });
    };
  const onAnnMove = (e: PointerEvent) => {
    const d = annDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    let { start, end } = d.orig;
    if (d.mode === "move") {
      const len = end - start;
      start = Math.min(Math.max(0, start + dt), duration() - len);
      end = start + len;
    } else if (d.mode === "l") {
      start = Math.min(Math.max(0, start + dt), end - 0.2);
    } else {
      end = Math.max(Math.min(duration(), end + dt), start + 0.2);
    }
    setAnnDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onAnnUp = async () => {
    const d = annDrag();
    if (!d) return;
    setAnnDrag(null);
    setSelZoom(null);
    setSelSpeed(null);
    setSelected({ kind: d.kind, id: d.id });
    if (d.moved) {
      try {
        await invoke("update_annotation_range", { id: d.id, start: d.cur.start, end: d.cur.end });
        await refresh();
        await pushSeek(playhead());
      } catch (e) {
        setStatus(`Retime failed: ${String(e)}`);
      }
    } else {
      scrub(d.orig.start);
    }
  };

  // All annotations as flat timeline bars, sorted by start time.
  const annBars = () => {
    const a = anns();
    const bars: { kind: Kind; id: number; start: number; end: number; label: string }[] = [];
    for (const t of a.texts)
      bars.push({ kind: "text", id: t.id, start: t.range.start, end: t.range.end, label: t.text || "Text" });
    for (const ar of a.arrows)
      bars.push({ kind: "arrow", id: ar.id, start: ar.range.start, end: ar.range.end, label: "Arrow" });
    for (const b of a.highlights)
      bars.push({ kind: "box", id: b.id, start: b.range.start, end: b.range.end, label: "Box" });
    return bars.sort((x, y) => x.start - y.start);
  };

  // ── resizable inspector ────────────────────────────────────────────────────────
  const [inspectorW, setInspectorW] = createSignal(
    Number(localStorage.getItem("vuoom-inspector-w")) || 296,
  );
  let inspectorDrag = false;
  const onInspDown = (e: PointerEvent) => {
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    inspectorDrag = true;
  };
  const onInspMove = (e: PointerEvent) => {
    if (!inspectorDrag) return;
    setInspectorW(Math.min(440, Math.max(240, window.innerWidth - e.clientX)));
  };
  const onInspUp = () => {
    if (!inspectorDrag) return;
    inspectorDrag = false;
    try {
      localStorage.setItem("vuoom-inspector-w", String(inspectorW()));
    } catch {
      /* storage unavailable */
    }
  };
  const inspectorOpen = () => !!selected() || selZoom() !== null || selSpeed() !== null;

  const safeName = () =>
    projectName().replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "vuoom";

  const onSaveProject = async () => {
    const dir = await save({
      defaultPath: `${safeName()}.vuoom`,
      filters: [{ name: "Vuoom project", extensions: ["vuoom"] }],
    });
    if (!dir) return;
    setStatus("Saving project…");
    try {
      await invoke("save_project_bundle", { dir });
      setStatus(`Saved ${dir}`);
    } catch (e) {
      setStatus(`Save failed: ${String(e)}`);
    }
  };

  const onOpenProject = async () => {
    const dir = await open({ directory: true, title: "Open a .vuoom project folder" });
    if (!dir || Array.isArray(dir)) return;
    setStatus("Opening project…");
    try {
      const summary = await invoke<RecordingSummary>("open_project_bundle", { dir });
      const base = dir.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "Untitled";
      setProjectName(base.replace(/\.vuoom$/i, "") || "Untitled");
      await loadFinishedClip(summary);
      setStatus("Project opened");
    } catch (e) {
      setStatus(`Open failed: ${String(e)}`);
    }
  };

  return (
    <div class="editor">
      <header class="titlebar" data-tauri-drag-region="">
        <LogoWordmark />
        <div class="titlebar-right">
          <ThemeMenu current={theme()} onSelect={setTheme} />
          <WindowControls />
        </div>
      </header>

      <div class="toolbar">
        <div class="toolbar-group">
          <button class="btn record" title="Record your screen (Ctrl+Shift+R)" onClick={() => void startRecord()}>
            <span class="dot" /> Record
          </button>
        </div>

        <div class="project-title">
          <input
            class="project-name"
            value={projectName()}
            spellcheck={false}
            aria-label="Project name"
            title="Rename project"
            onInput={(e) => setProjectName(e.currentTarget.value)}
            onFocus={(e) => e.currentTarget.select()}
            onBlur={(e) => {
              if (!e.currentTarget.value.trim()) setProjectName("Untitled");
            }}
          />
        </div>

        <div class="toolbar-group">
          <button class="btn ghost" title="Open a saved project (Ctrl+O)" onClick={() => void onOpenProject()}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 8V6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v2M3 8h17.2a1 1 0 0 1 .97 1.24l-2 8a1 1 0 0 1-.97.76H4a1 1 0 0 1-1-1z" />
            </svg>
            Open
          </button>
          <button class="btn ghost" disabled={!hasClip()} title="Save project — video + edits (Ctrl+S)" onClick={() => void onSaveProject()}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 3h11l5 5v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 3v5h8V3M7 21v-7h10v7" />
            </svg>
            Save
          </button>
          <span class="toolbar-sep" />
          <button class="btn export" disabled={!hasClip()} title="Export an optimized GIF (Ctrl+E)" onClick={() => setShowExport(true)}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 3v12m0 0l-4.5-4.5M12 15l4.5-4.5M4 21h16" />
            </svg>
            Export GIF
          </button>
        </div>
      </div>

      <div
        class="workspace"
        style={{
          "grid-template-columns": inspectorOpen() ? `76px 1fr ${inspectorW()}px` : "76px 1fr",
        }}
      >
        <nav class="toolrail">
          <For each={TOOLS}>
            {(t) => (
              <button
                classList={{ tool: true, active: tool() === t.id }}
                onClick={() => setTool(t.id)}
                title={t.hint}
              >
                <ToolIcon tool={t.id} />
                <span>{t.label}</span>
              </button>
            )}
          </For>
        </nav>

        <main class="canvas">
          <div class="tool-hint">{TOOLS.find((t) => t.id === tool())?.hint}</div>
          <div
            class="canvas-frame"
            ref={(el) => (stageEl = el)}
            style={{ "aspect-ratio": String(frameAspect()) }}
          >
            <canvas
              ref={(el) => (canvasEl = el)}
              class="preview-canvas"
              classList={{ hidden: !hasClip() }}
            />
            <Show when={!hasClip()}>
              <div class="canvas-placeholder">
                <p class="big">Ready when you are</p>
                <button class="btn record cta" onClick={() => void startRecord()}>
                  <span class="dot" /> Start recording
                </button>
                <span class="placeholder-hint">
                  <kbd>Ctrl+Shift+R</kbd> record · <kbd>Ctrl+Shift+Z</kbd> zoom ·{" "}
                  <kbd>Ctrl+Shift+X</kbd> stop
                </span>
              </div>
            </Show>

            <Show when={hasClip()}>
              <svg
                class="overlay"
                classList={{ "tool-draw": tool() !== "select" }}
                onPointerDown={(e) => void onPointerDown(e)}
                onPointerMove={onPointerMove}
                onPointerUp={(e) => void onPointerUp(e)}
                onDblClick={(e) => {
                  const hit = hitTest(norm(e as unknown as PointerEvent));
                  if (hit?.kind === "text") {
                    setSelected(hit);
                    setEditingText(hit.id);
                  }
                }}
              >
                {/* boxes */}
                <For each={anns().highlights}>
                  {(b) => {
                    const sel = () => selected()?.kind === "box" && selected()?.id === b.id;
                    return (
                      <Show when={inView(b.range, sel())}>
                        {(() => {
                          const g = () => liveGeom("box", b.id);
                          const a = () => px({ x: g()[0], y: g()[1] });
                          const s = () => px({ x: g()[2], y: g()[3] });
                          return (
                            <>
                              <rect
                                x={a().x}
                                y={a().y}
                                width={s().x}
                                height={s().y}
                                fill={b.filled ? cssColor(b.color) : "none"}
                                stroke={cssColor(b.color)}
                                stroke-width={2}
                              />
                              <Show when={sel()}>
                                <Handles
                                  pts={[
                                    { x: a().x, y: a().y },
                                    { x: a().x + s().x, y: a().y },
                                    { x: a().x, y: a().y + s().y },
                                    { x: a().x + s().x, y: a().y + s().y },
                                  ]}
                                />
                              </Show>
                            </>
                          );
                        })()}
                      </Show>
                    );
                  }}
                </For>

                {/* arrows */}
                <For each={anns().arrows}>
                  {(ar) => {
                    const sel = () => selected()?.kind === "arrow" && selected()?.id === ar.id;
                    return (
                      <Show when={inView(ar.range, sel())}>
                        {(() => {
                          const g = () => liveGeom("arrow", ar.id);
                          const f = () => px({ x: g()[0], y: g()[1] });
                          const tp = () => px({ x: g()[2], y: g()[3] });
                          return (
                            <>
                              <ArrowLine from={f()} to={tp()} color={cssColor(ar.color)} />
                              <Show when={sel()}>
                                <Handles pts={[f(), tp()]} />
                              </Show>
                            </>
                          );
                        })()}
                      </Show>
                    );
                  }}
                </For>

                {/* text */}
                <For each={anns().texts}>
                  {(tx) => {
                    const sel = () => selected()?.kind === "text" && selected()?.id === tx.id;
                    return (
                      <Show when={inView(tx.range, sel())}>
                        {(() => {
                          const g = () => liveGeom("text", tx.id);
                          const p = () => px({ x: g()[0], y: g()[1] });
                          const fs = () => tx.font_size * stage().h;
                          return (
                            <>
                              <text
                                x={p().x}
                                y={p().y + fs()}
                                font-size={String(fs())}
                                fill={cssColor(tx.color)}
                                style={{
                                  "font-family": "Inter, sans-serif",
                                  "font-weight": tx.bold ? "700" : "400",
                                  "font-style": tx.italic ? "italic" : "normal",
                                }}
                              >
                                {tx.text}
                              </text>
                              <Show when={sel()}>
                                <rect
                                  class="sel-outline"
                                  x={p().x - 4}
                                  y={p().y - 4}
                                  width={Math.max(40, tx.text.length * fs() * 0.6) + 8}
                                  height={fs() + 8}
                                />
                              </Show>
                            </>
                          );
                        })()}
                      </Show>
                    );
                  }}
                </For>

                {/* live creation draft */}
                <Show when={drag()?.mode === "create-arrow"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    return <ArrowLine from={px(d.start)} to={px(d.cur)} color="#e5484d" />;
                  })()}
                </Show>
                <Show when={drag()?.mode === "create-box"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    const a = px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
                    const w = Math.abs(d.cur.x - d.start.x) * stage().w;
                    const h = Math.abs(d.cur.y - d.start.y) * stage().h;
                    return (
                      <rect x={a.x} y={a.y} width={w} height={h} fill="none" stroke="#ffd23f" stroke-width={2} />
                    );
                  })()}
                </Show>
              </svg>

              <Show when={editingTextAnn()}>
                {(() => {
                  const ta = editingTextAnn()!;
                  const p = px({ x: v2(ta.pos).x, y: v2(ta.pos).y });
                  const fs = ta.font_size * stage().h;
                  return (
                    <input
                      class="text-edit"
                      style={{
                        left: `${p.x}px`,
                        top: `${p.y}px`,
                        "font-size": `${fs}px`,
                        "font-weight": ta.bold ? "700" : "400",
                        "font-style": ta.italic ? "italic" : "normal",
                      }}
                      value={ta.text}
                      spellcheck={false}
                      ref={(el) => queueMicrotask(() => { el.focus(); el.select(); })}
                      onInput={(e) => editTextLive(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === "Escape") {
                          e.preventDefault();
                          e.currentTarget.blur();
                        }
                      }}
                      onBlur={() => void finishTextEdit()}
                    />
                  );
                })()}
              </Show>
            </Show>
          </div>
        </main>

        <Show when={selected()}>
          <aside class="properties">
            <div
              class="panel-resizer"
              title="Drag to resize"
              onPointerDown={onInspDown}
              onPointerMove={onInspMove}
              onPointerUp={onInspUp}
            />
            <div class="inspector-head">
              <h2>{selected()!.kind[0].toUpperCase() + selected()!.kind.slice(1)}</h2>
              <button class="icon-btn" title="Done" onClick={() => setSelected(null)}>
                ✕
              </button>
            </div>

            <Show when={selectedText()}>
              <label class="field">
                <span>Text</span>
                <input
                  type="text"
                  value={selectedText()!.text}
                  onInput={(e) => void editText(e.currentTarget.value)}
                />
              </label>
              <div class="field">
                <span>Style</span>
                <div class="style-row">
                  <button
                    classList={{ stylebtn: true, on: selectedText()!.bold }}
                    title="Bold"
                    onClick={() => void editTextStyle({ bold: !selectedText()!.bold })}
                  >
                    B
                  </button>
                  <button
                    classList={{ stylebtn: true, italic: true, on: selectedText()!.italic }}
                    title="Italic"
                    onClick={() => void editTextStyle({ italic: !selectedText()!.italic })}
                  >
                    I
                  </button>
                </div>
              </div>
              <label class="field">
                <span>Size · {Math.round(selectedText()!.font_size * 100)}% of height</span>
                <input
                  type="range"
                  min="0.02"
                  max="0.2"
                  step="0.005"
                  value={selectedText()!.font_size}
                  onInput={(e) => void editFontSize(Number(e.currentTarget.value))}
                />
              </label>
            </Show>

            <Show when={selectedColor()}>
              <div class="field">
                <span>Color</span>
                <div class="swatch-row">
                  <For each={PRESET_COLORS}>
                    {(c) => (
                      <button
                        classList={{ swatchbtn: true, active: rgbHex(selectedColor()!) === c }}
                        style={{ background: c }}
                        title={c}
                        onClick={() => void setColor(c)}
                      />
                    )}
                  </For>
                </div>
                <input
                  type="color"
                  value={rgbHex(selectedColor()!)}
                  onInput={(e) => void setColor(e.currentTarget.value)}
                />
              </div>
            </Show>

            <Show when={selectedRange()}>
              <div class="field">
                <span>Timing (seconds)</span>
                <div class="time-row">
                  <input
                    type="number"
                    min="0"
                    max={duration()}
                    step="0.1"
                    title="Appears at"
                    value={Number(selectedRange()!.start.toFixed(1))}
                    onChange={(e) =>
                      void editRange(Number(e.currentTarget.value), selectedRange()!.end)
                    }
                  />
                  <span class="time-dash">–</span>
                  <input
                    type="number"
                    min="0"
                    max={duration()}
                    step="0.1"
                    title="Disappears at"
                    value={Number(selectedRange()!.end.toFixed(1))}
                    onChange={(e) =>
                      void editRange(selectedRange()!.start, Number(e.currentTarget.value))
                    }
                  />
                </div>
              </div>
            </Show>

            <p class="muted small">Drag to move · drag a handle to resize · Delete to remove.</p>
            <button class="btn danger" onClick={() => void deleteSelected()}>
              Delete element
            </button>
          </aside>
        </Show>

        <Show when={selZoom() !== null && selectedZoom()}>
          <aside class="properties">
            <div
              class="panel-resizer"
              title="Drag to resize"
              onPointerDown={onInspDown}
              onPointerMove={onInspMove}
              onPointerUp={onInspUp}
            />
            <div class="inspector-head">
              <h2>Zoom</h2>
              <button class="icon-btn" title="Done" onClick={() => setSelZoom(null)}>
                ✕
              </button>
            </div>

            <label class="field">
              <span>Strength · {selectedZoom()!.amount.toFixed(1)}×</span>
              <input
                type="range"
                min="1.2"
                max="4"
                step="0.1"
                value={selectedZoom()!.amount}
                onChange={(e) => {
                  const z = selectedZoom()!;
                  void applyZoomEdit(selZoom()!, z.start, z.end, Number(e.currentTarget.value));
                }}
              />
            </label>
            <p class="muted small">
              {fmt(selectedZoom()!.start)} – {fmt(selectedZoom()!.end)} · drag the block on the
              timeline to retime, drag its edges to resize.
            </p>
            <button class="btn danger" onClick={() => void deleteSelectedZoom()}>
              Delete zoom
            </button>
          </aside>
        </Show>

        <Show when={selSpeed() !== null && selectedSpeed()}>
          <aside class="properties">
            <div
              class="panel-resizer"
              title="Drag to resize"
              onPointerDown={onInspDown}
              onPointerMove={onInspMove}
              onPointerUp={onInspUp}
            />
            <div class="inspector-head">
              <h2>Speed</h2>
              <button class="icon-btn" title="Done" onClick={() => setSelSpeed(null)}>
                ✕
              </button>
            </div>

            <label class="field">
              <span>Speed · {selectedSpeed()!.factor}×</span>
              <input
                type="range"
                min="1.25"
                max="8"
                step="0.25"
                value={selectedSpeed()!.factor}
                onChange={(e) => {
                  const r = selectedSpeed()!;
                  void applySpeedEdit(selSpeed()!, r.start, r.end, Number(e.currentTarget.value));
                }}
              />
            </label>
            <p class="muted small">
              {fmt(selectedSpeed()!.start)} – {fmt(selectedSpeed()!.end)} · drag the band on the
              timeline to retime, drag its edges to resize.
            </p>
            <button class="btn danger" onClick={() => void deleteSelectedSpeed()}>
              Delete speed region
            </button>
          </aside>
        </Show>
      </div>

      <footer class="timeline">
        <div class="transport">
          <button class="tbtn" title="Back to start" disabled={!hasClip()} onClick={restart}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 5h2.5v14H6zM20 5.8v12.4a.8.8 0 0 1-1.25.66L9.6 12.66a.8.8 0 0 1 0-1.32l9.15-6.2A.8.8 0 0 1 20 5.8z" />
            </svg>
          </button>
          <button class="tbtn play" title="Play / Pause (Space)" disabled={!hasClip()} onClick={togglePlay}>
            <Show
              when={playing()}
              fallback={
                <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5.6v12.8a.9.9 0 0 0 1.38.76l10.1-6.4a.9.9 0 0 0 0-1.52l-10.1-6.4A.9.9 0 0 0 8 5.6z" />
                </svg>
              }
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
                <rect x="6" y="5" width="4.4" height="14" rx="1" />
                <rect x="13.6" y="5" width="4.4" height="14" rx="1" />
              </svg>
            </Show>
          </button>
          <span class="time">
            {fmtT(playhead())} <span class="time-sep">/</span> {fmt(duration())}
            <Show when={trim() || speed().length > 0}>
              <span class="time-out" title="Final GIF duration after trim + speed-up">
                → {outputDuration(duration(), trim(), speed()).toFixed(1)}s
              </span>
            </Show>
          </span>
          <button
            class="tbtn wide"
            title="Add a zoom segment at the playhead"
            disabled={!hasClip()}
            onClick={() => void addZoomAtPlayhead()}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
              <circle cx="10.5" cy="10.5" r="6.5" />
              <path d="M15.5 15.5L21 21M10.5 7.5v6M7.5 10.5h6" />
            </svg>
            <span>Zoom</span>
          </button>
          <button
            class="tbtn wide"
            classList={{ on: speed().length > 0 }}
            title={`Play idle stretches at ${skimFactor()}× (auto-detected from your activity)`}
            disabled={!hasClip()}
            onClick={() => void toggleSkim()}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 6.5v11a.8.8 0 0 0 1.25.66L12 13v4.5a.8.8 0 0 0 1.25.66l8.3-5.5a.8.8 0 0 0 0-1.32l-8.3-5.5A.8.8 0 0 0 12 6.5V11L4.25 5.84A.8.8 0 0 0 3 6.5z" />
            </svg>
            <span>Skim idle</span>
          </button>
          <select
            class="tbtn-sel"
            title="Speed-up factor for Skim idle and new speed regions"
            disabled={!hasClip()}
            value={String(skimFactor())}
            onChange={(e) => setSkimFactor(Number(e.currentTarget.value))}
          >
            <For each={[2, 3, 4, 6, 8]}>{(f) => <option value={String(f)}>{f}×</option>}</For>
          </select>
          <button
            class="tbtn wide"
            title={`Mark 2s at the playhead to play at ${skimFactor()}× — drag the band to retime`}
            disabled={!hasClip()}
            onClick={() => void addSpeedAtPlayhead()}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
              <path d="M5 5l7 7-7 7M13 5l7 7-7 7" />
            </svg>
            <span>Speed</span>
          </button>
          <span class="statusline">{status()}</span>
        </div>

        <div
          class="tl"
          classList={{ empty: !hasClip() }}
          ref={(el) => (tlEl = el)}
          onPointerDown={(e) => {
            if (!hasClip()) return;
            (e.currentTarget as Element).setPointerCapture(e.pointerId);
            tlDrag = true;
            tlSeekFromEvent(e);
          }}
          onPointerMove={(e) => {
            if (tlDrag) tlSeekFromEvent(e);
          }}
          onPointerUp={() => (tlDrag = false)}
        >
          <Show
            when={hasClip()}
            fallback={<div class="tl-empty">Your recording's timeline appears here</div>}
          >
            <div class="tl-ruler">
              <For each={ticks()}>
                {(t) => (
                  <span class="tl-tick" style={{ left: `${pct(t)}%` }}>
                    <i />
                    {fmt(t)}
                  </span>
                )}
              </For>
            </div>
            <div class="tl-track">
              <span class="tl-tracklabel">Zoom</span>
              <For each={zooms()}>
                {(z, i) => {
                  const g = () => zoomGeom(i(), z);
                  return (
                    <div
                      classList={{ "tl-seg": true, selected: selZoom() === i() }}
                      style={{
                        left: `${pct(g().start)}%`,
                        width: `${Math.max(pct(g().end) - pct(g().start), 0.6)}%`,
                      }}
                      title={`Zoom ${z.amount.toFixed(1)}× · drag to move, drag an edge to resize`}
                      onPointerDown={onZoomDown(i(), z)}
                      onPointerMove={onZoomMove}
                      onPointerUp={() => void onZoomUp()}
                    >
                      {z.amount.toFixed(1)}×
                    </div>
                  );
                }}
              </For>
            </div>
            {/* One lane per annotation — its own layer, individually visible and draggable. */}
            <div class="tl-lanes">
              <span class="tl-tracklabel">Notes</span>
              <Show when={annBars().length === 0}>
                <div class="tl-lane" />
              </Show>
              <For each={annBars()}>
                {(b) => {
                  const g = () => annGeom(b);
                  return (
                    <div class="tl-lane">
                      <button
                        classList={{
                          "tl-ann": true,
                          [`tl-${b.kind}`]: true,
                          selected: selected()?.kind === b.kind && selected()?.id === b.id,
                        }}
                        style={{
                          left: `${pct(g().start)}%`,
                          width: `${Math.max(pct(g().end) - pct(g().start), 1)}%`,
                        }}
                        title={`${b.label} · drag to move, drag an edge to set how long it shows`}
                        onPointerDown={onAnnDown(b)}
                        onPointerMove={onAnnMove}
                        onPointerUp={() => void onAnnUp()}
                      >
                        {b.label}
                      </button>
                    </div>
                  );
                }}
              </For>
            </div>
            {/* Speed-up bands — click the chip to select, drag to move, drag an edge to resize. */}
            <For each={speed()}>
              {(r, i) => {
                const g = () => speedGeom(i(), r);
                return (
                  <div
                    classList={{ "tl-speedband": true, selected: selSpeed() === i() }}
                    style={{
                      left: `${pct(g().start)}%`,
                      width: `${Math.max(pct(g().end) - pct(g().start), 0.6)}%`,
                    }}
                  >
                    <button
                      class="tl-speedchip"
                      title={`Plays at ${r.factor}× · drag to move, drag an edge to resize`}
                      onPointerDown={onSpeedDown(i(), r)}
                      onPointerMove={onSpeedMove}
                      onPointerUp={() => void onSpeedUp()}
                    >
                      {r.factor}×
                    </button>
                  </div>
                );
              }}
            </For>

            {/* Trim: dimmed cut-off areas + draggable in/out handles */}
            <div class="tl-shade" style={{ left: "0", width: `${pct(tStart())}%` }} />
            <div class="tl-shade" style={{ left: `${pct(tEnd())}%`, right: "0" }} />
            <div
              class="tl-trim"
              style={{ left: `${pct(tStart())}%` }}
              title="Trim start — drag"
              onPointerDown={onTrimDown("start")}
              onPointerMove={onTrimMove}
              onPointerUp={() => void onTrimUp()}
            />
            <div
              class="tl-trim end"
              style={{ left: `${pct(tEnd())}%` }}
              title="Trim end — drag"
              onPointerDown={onTrimDown("end")}
              onPointerMove={onTrimMove}
              onPointerUp={() => void onTrimUp()}
            />

            <div class="tl-playhead" style={{ left: `${pct(playhead())}%` }}>
              <i />
            </div>
          </Show>
        </div>
      </footer>

      <Show when={showExport()}>
        <ExportDialog
          name={projectName()}
          duration={duration()}
          trim={trim()}
          speed={speed()}
          onClose={() => setShowExport(false)}
          onStatus={setStatus}
        />
      </Show>

      <Show when={recordPhase() === "active"}>
        <RecordOverlay
          backdrop={backdrop()}
          zoom={zoomAmount()}
          onZoomChange={setZoomAmount}
          onFinished={(s) => void onRecordFinished(s)}
          onCancel={onRecordCancel}
        />
      </Show>
    </div>
  );
}

// ── sub-components ────────────────────────────────────────────────────────────────

function ToolIcon(props: { tool: Tool }): JSX.Element {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "1.8", "stroke-linecap": "round" as const, "stroke-linejoin": "round" as const };
  switch (props.tool) {
    case "select":
      return <svg {...common}><path d="M5 3l6 16 2-6 6-2z" /></svg>;
    case "text":
      return <svg {...common}><path d="M5 6V4h14v2M12 4v16M9 20h6" /></svg>;
    case "arrow":
      return <svg {...common}><path d="M5 19L19 5M11 5h8v8" /></svg>;
    case "box":
      return <svg {...common}><rect x="4" y="6" width="16" height="12" rx="1" /></svg>;
  }
}

function Handles(props: { pts: Vec2[] }): JSX.Element {
  return (
    <For each={props.pts}>
      {(p) => <circle class="handle" cx={p.x} cy={p.y} r={6} />}
    </For>
  );
}

function ArrowLine(props: { from: Vec2; to: Vec2; color: string }): JSX.Element {
  const ang = () => Math.atan2(props.to.y - props.from.y, props.to.x - props.from.x);
  const head = (off: number) => ({
    x: props.to.x - 14 * Math.cos(ang() - off),
    y: props.to.y - 14 * Math.sin(ang() - off),
  });
  return (
    <g stroke={props.color} fill={props.color} stroke-width={3} stroke-linecap="round">
      <line x1={props.from.x} y1={props.from.y} x2={props.to.x} y2={props.to.y} />
      <polygon
        points={`${props.to.x},${props.to.y} ${head(0.5).x},${head(0.5).y} ${head(-0.5).x},${head(-0.5).y}`}
        stroke="none"
      />
    </g>
  );
}

const fmtBytes = (b: number) => {
  if (b <= 0) return "—";
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
};

function ExportDialog(props: {
  name: string;
  duration: number;
  trim: Trim | null;
  speed: SpeedRegion[];
  onClose: () => void;
  onStatus: (s: string) => void;
}): JSX.Element {
  const [preset, setPreset] = createSignal<"readme" | "hq" | "custom">("readme");
  const [fps, setFps] = createSignal(15);
  const [width, setWidth] = createSignal(1000);
  const [quality, setQuality] = createSignal(80);
  const [phase, setPhase] = createSignal<"configure" | "exporting" | "done">("configure");
  const [progress, setProgress] = createSignal(0);
  const [estimate, setEstimate] = createSignal<number | null>(null);
  const [outPath, setOutPath] = createSignal("");
  const [copied, setCopied] = createSignal("");

  const outDur = () => outputDuration(props.duration, props.trim, props.speed);

  // Live size estimate (sample-and-extrapolate), debounced as sliders move.
  let estimateTimer: number | undefined;
  let estimateGen = 0;
  createEffect(() => {
    const args = { fps: fps(), width: width(), quality: quality() };
    setEstimate(null);
    clearTimeout(estimateTimer);
    const gen = ++estimateGen;
    estimateTimer = window.setTimeout(() => {
      invoke<number>("estimate_gif", args)
        .then((b) => {
          if (gen === estimateGen) setEstimate(b);
        })
        .catch(() => {
          if (gen === estimateGen) setEstimate(0);
        });
    }, 350);
  });
  onCleanup(() => clearTimeout(estimateTimer));

  const applyPreset = (p: "readme" | "hq" | "custom") => {
    setPreset(p);
    if (p === "readme") {
      setFps(15);
      setWidth(1000);
      setQuality(80);
    } else if (p === "hq") {
      setFps(20);
      setWidth(1280);
      setQuality(95);
    }
  };

  const doExport = async () => {
    const safe = props.name.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "vuoom";
    const path = await save({
      defaultPath: `${safe}.gif`,
      filters: [{ name: "GIF", extensions: ["gif"] }],
    });
    if (!path) return;
    setPhase("exporting");
    setProgress(0);
    props.onStatus("Exporting GIF…");
    const unlisten = await listen<{ done: number; total: number }>("export-progress", (ev) => {
      setProgress(ev.payload.total > 0 ? ev.payload.done / ev.payload.total : 0);
    });
    try {
      await invoke("export_gif", { path, fps: fps(), width: width(), quality: quality() });
      setOutPath(path);
      setPhase("done");
      props.onStatus(`Exported ${path}`);
    } catch (e) {
      setPhase("configure");
      props.onStatus(`Export failed: ${String(e)}`);
    } finally {
      unlisten();
    }
  };

  const copyGif = async () => {
    try {
      await invoke("copy_gif_to_clipboard", { path: outPath() });
      setCopied("Copied! Paste it into Slack, Discord, or a GitHub comment.");
    } catch (e) {
      setCopied(`Copy failed: ${String(e)}`);
    }
  };
  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(outPath());
      setCopied("Path copied.");
    } catch {
      setCopied("Could not copy the path.");
    }
  };
  const reveal = () => void revealItemInDir(outPath()).catch(() => undefined);

  return (
    <div class="modal-backdrop" onClick={() => phase() !== "exporting" && props.onClose()}>
      <div class="modal" onClick={(e) => e.stopPropagation()}>
        <Show when={phase() === "configure"}>
          <h2>Export GIF</h2>
          <div class="preset-row">
            <button classList={{ chip: true, active: preset() === "readme" }} onClick={() => applyPreset("readme")}>
              README<small>small · 15fps · 1000px</small>
            </button>
            <button classList={{ chip: true, active: preset() === "hq" }} onClick={() => applyPreset("hq")}>
              High quality<small>crisp · 20fps · 1280px</small>
            </button>
            <button classList={{ chip: true, active: preset() === "custom" }} onClick={() => applyPreset("custom")}>
              Custom<small>tune it yourself</small>
            </button>
          </div>

          <label class="field">
            <span>Frame rate · {fps()} fps</span>
            <input type="range" min="8" max="30" step="1" value={fps()} onInput={(e) => { setFps(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>
          <label class="field">
            <span>Max width · {width()} px</span>
            <input type="range" min="400" max="1920" step="20" value={width()} onInput={(e) => { setWidth(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>
          <label class="field">
            <span>Quality · {quality()}</span>
            <input type="range" min="40" max="100" step="1" value={quality()} onInput={(e) => { setQuality(Number(e.currentTarget.value)); setPreset("custom"); }} />
          </label>

          <div class="export-meta">
            <span>{outDur().toFixed(1)}s of GIF</span>
            <span class="export-size">
              {estimate() === null ? "estimating size…" : `≈ ${fmtBytes(estimate()!)}`}
            </span>
          </div>

          <div class="modal-actions">
            <button class="btn" onClick={props.onClose}>
              Cancel
            </button>
            <button class="btn export" onClick={() => void doExport()}>
              Choose location & export
            </button>
          </div>
        </Show>

        <Show when={phase() === "exporting"}>
          <h2>Exporting…</h2>
          <div class="progress">
            <div class="progress-fill" style={{ width: `${Math.round(progress() * 100)}%` }} />
          </div>
          <p class="muted small">
            Compositing {Math.round(progress() * 100)}% — annotations, zoom and speed-up are
            baked into the final GIF.
          </p>
        </Show>

        <Show when={phase() === "done"}>
          <h2>GIF exported</h2>
          <p class="export-path" title={outPath()}>
            {outPath()}
          </p>
          <div class="done-actions">
            <button class="btn export" onClick={() => void copyGif()}>
              Copy GIF
            </button>
            <button class="btn" onClick={() => void copyPath()}>
              Copy path
            </button>
            <button class="btn" onClick={reveal}>
              Show in folder
            </button>
          </div>
          <p class="muted small">{copied() || "Paste the copied GIF anywhere that accepts files."}</p>
          <div class="modal-actions">
            <button class="btn" onClick={props.onClose}>
              Done
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}

export default App;
