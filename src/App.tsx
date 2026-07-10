import { createSignal, createEffect, onMount, onCleanup, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { save, open, ask } from "@tauri-apps/plugin-dialog";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import RecordOverlay from "./RecordOverlay";
import WindowControls from "./WindowControls";
import ThemeMenu from "./ThemeMenu";
import { applyTheme, initialTheme } from "./themes";
import { PreviewClient } from "./preview";
import { LogoWordmark } from "./Logo";
import ScrubField from "./ScrubField";
import { ExportDialog } from "./ExportDialog";
import {
  ArrowLine,
  Handles,
  InspectorPanel,
  InspRow,
  InspSection,
  LockIcon,
  ToolIcon,
} from "./EditorPrimitives";
import { dialogA11y } from "./dialog";
import { arrowHeads, clamp01, distToSeg, outputDuration, v2 } from "./geometry";
import { cssColor, fmt, fmtT, hexRgb, rgbHex } from "./format";
import { SHORTCUTS, TOOL_KEYS, TOOLS } from "./shortcuts";
import type {
  AnnotationSet,
  ClipState,
  Color,
  Drag,
  Kind,
  RecordingSummary,
  Selection,
  SpeedRegion,
  TextAnn,
  TimeRange,
  Tool,
  Trim,
  Vec2,
  ZoomSeg,
} from "./types";
import "./App.css";

// TODO(decompose): App() is still ~3.3k lines. A future store-based pass should lift the
// editor's reactive state (signals for clip/anns/zooms/trim/speed/cuts/selection/drag/
// playback + their derived getters and mutators) into an editor store, then split the JSX
// that reads it into Topbar / Toolrail / CanvasStage / Timeline / Inspector / RecordController
// / Onboarding components wired to that store. This pass only moved self-contained,
// closure-free pieces (types, pure helpers, dialog a11y, tool/shortcut config, and the
// stateless presentational components + ExportDialog) — see src/types.ts, geometry.ts,
// format.ts, shortcuts.ts, dialog.ts, EditorPrimitives.tsx, ExportDialog.tsx.

/// Quick-pick annotation colors (white, ink, record red, box yellow, green, text blue).
const PRESET_COLORS = ["#ffffff", "#0e0e0f", "#e5484d", "#ffd23f", "#30a46c", "#6ea8ff"];

/// Bundled text fonts. `id` is the family name sent to the renderer (empty = default sans);
/// `css` styles the on-canvas preview + the in-typeface picker. Mirrors the @font-face set
/// in App.css and the fonts loaded into glyphon for export.
const TEXT_FONTS: { id: string; label: string; css: string }[] = [
  { id: "", label: "Default", css: "Inter, sans-serif" },
  { id: "Anton", label: "Anton", css: "Anton, sans-serif" },
  { id: "Bebas Neue", label: "Bebas", css: "'Bebas Neue', sans-serif" },
  { id: "Poppins", label: "Poppins", css: "Poppins, sans-serif" },
  { id: "Permanent Marker", label: "Marker", css: "'Permanent Marker', cursive" },
  { id: "Shrikhand", label: "Shrikhand", css: "Shrikhand, serif" },
];
const fontCss = (name: string) => TEXT_FONTS.find((f) => f.id === name)?.css ?? "Inter, sans-serif";

function App() {
  const [tool, setTool] = createSignal<Tool>("select");
  // When locked, a drawing tool stays active after creating an element (draw several in a
  // row); when unlocked (default) we fall back to Select so the new element is editable.
  const [toolLock, setToolLock] = createSignal(false);
  const [status, setStatus] = createSignal("Ready");
  const [projectName, setProjectName] = createSignal("Untitled");
  const [editingText, setEditingText] = createSignal<number | null>(null);
  const [theme, setTheme] = createSignal(initialTheme());
  const [hasClip, setHasClip] = createSignal(false);
  // True once the loaded clip has unsaved edits (annotations, zooms, trim, cuts, speed,
  // frame, click/key overlays). Drives the "discard edits?" guard before a new recording
  // replaces the clip. Set wherever an edit lands; cleared on load / save / export.
  const [dirty, setDirty] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [playhead, setPlayhead] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);
  const [looping, setLooping] = createSignal(false);

  const [anns, setAnns] = createSignal<AnnotationSet>({ texts: [], arrows: [], highlights: [] });
  const [zooms, setZooms] = createSignal<ZoomSeg[]>([]);
  const [trim, setTrimState] = createSignal<Trim | null>(null);
  const [speed, setSpeed] = createSignal<SpeedRegion[]>([]);
  const [cuts, setCuts] = createSignal<Trim[]>([]);
  const [selZoom, setSelZoom] = createSignal<number | null>(null);
  const [selSpeed, setSelSpeed] = createSignal<number | null>(null);
  const [selCut, setSelCut] = createSignal<number | null>(null);
  const [skimFactor, setSkimFactor] = createSignal(3);
  const [showClicks, setShowClicks] = createSignal(false);
  const [showKeys, setShowKeys] = createSignal(false);
  const [framePreset, setFramePreset] = createSignal("none");
  const [recoverable, setRecoverable] = createSignal<number | null>(null);
  const [selected, setSelected] = createSignal<Selection | null>(null);
  const [drag, setDrag] = createSignal<Drag>(null);
  const [stage, setStage] = createSignal({ w: 1, h: 1 });
  const [frameAspect, setFrameAspect] = createSignal(16 / 9);
  // Pixel width of the timeline track surface — drives the adaptive ruler ticks
  // (kept in sync via a ResizeObserver in onMount).
  const [tlWidth, setTlWidth] = createSignal(800);
  // While an annotation is dragged on the canvas, the normalized x/y of an active
  // center/edge snap guide (or null). Drawn as crosshair lines on the overlay.
  const [snapX, setSnapX] = createSignal<number | null>(null);
  const [snapY, setSnapY] = createSignal<number | null>(null);
  const [showExport, setShowExport] = createSignal(false);
  const [recordPhase, setRecordPhase] = createSignal<"idle" | "active">("idle");
  const [backdrop, setBackdrop] = createSignal<string | null>(null);
  const [zoomAmount, setZoomAmount] = createSignal(1.8);
  // Auto-update: a pending update (if any) and whether we're mid-download.
  const [update, setUpdate] = createSignal<Update | null>(null);
  const [updating, setUpdating] = createSignal(false);
  // First-run onboarding: a one-time welcome card, then a coachmark pointing at Record.
  const [showWelcome, setShowWelcome] = createSignal(false);
  // Keyboard cheat-sheet modal (opened with "?").
  const [showShortcuts, setShowShortcuts] = createSignal(false);
  const [coachRecord, setCoachRecord] = createSignal(false);
  const [coachPos, setCoachPos] = createSignal({ x: 0, y: 0 });
  let recordBtnEl: HTMLButtonElement | undefined;

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
    // While a modal owns the screen, skip the editor accelerators (undo/save/export/…) so
    // they can't mutate the clip behind it — but still swallow browser chords further down.
    const modalOpen = showExport() || showWelcome() || showShortcuts();
    // Undo / redo (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y) — inputs keep their native undo.
    if (!modalOpen && e.ctrlKey && !e.altKey && !inField && e.code === "KeyZ") {
      e.preventDefault();
      void (e.shiftKey ? doRedo() : doUndo());
      return;
    }
    if (!modalOpen && e.ctrlKey && !e.shiftKey && !e.altKey && !inField && e.code === "KeyY") {
      e.preventDefault();
      void doRedo();
      return;
    }
    if (!modalOpen && e.ctrlKey && !e.shiftKey && !e.altKey) {
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
      // Ctrl+D would otherwise be swallowed below as a browser-bookmark chord.
      if (e.code === "KeyD" && !inField) {
        e.preventDefault();
        if (hasClip() && selected()) void duplicateSelected();
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
    if (tlEl) {
      const tro = new ResizeObserver(() => {
        if (tlEl) setTlWidth(tlEl.clientWidth || 800);
      });
      tro.observe(tlEl);
      setTlWidth(tlEl.clientWidth || 800);
      onCleanup(() => tro.disconnect());
    }
    await connectEngine();
    void checkForUpdate();
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
        maybeShowWelcome();
        // A previous session's frames are still on disk (crash or accidental close)?
        invoke<number | null>("check_recovery")
          .then((d) => setRecoverable(d ?? null))
          .catch(() => setRecoverable(null));
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

  // ── auto-update (signed GitHub releases) ───────────────────────────────────────
  // Check once on launch; surfaces an "Update" pill in the top bar if one is ready.
  const checkForUpdate = async () => {
    try {
      const u = await check();
      if (u) setUpdate(u);
    } catch {
      /* updater not configured (dev) or offline — silently ignore */
    }
  };
  const runUpdate = async () => {
    const u = update();
    if (!u || updating()) return;
    setUpdating(true);
    setStatus(`Downloading update v${u.version}…`);
    try {
      let total = 0;
      let got = 0;
      await u.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? 0;
        } else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          setStatus(
            total > 0
              ? `Downloading update… ${Math.round((got / total) * 100)}%`
              : "Downloading update…",
          );
        } else if (ev.event === "Finished") {
          setStatus("Update downloaded — restarting…");
        }
      });
      await relaunch();
    } catch (e) {
      setUpdating(false);
      setStatus(`Update failed: ${String(e)}`);
    }
  };

  // ── first-run onboarding ───────────────────────────────────────────────────────
  const maybeShowWelcome = () => {
    try {
      if (!localStorage.getItem("vuoom-seen-welcome")) setShowWelcome(true);
    } catch {
      /* storage unavailable — skip onboarding */
    }
  };
  // Dismiss the welcome card; `hint` pops a coachmark pointing at Record for skippers.
  const dismissWelcome = (hint: boolean) => {
    try {
      localStorage.setItem("vuoom-seen-welcome", "1");
    } catch {
      /* ignore */
    }
    setShowWelcome(false);
    if (hint && recordBtnEl) {
      const r = recordBtnEl.getBoundingClientRect();
      setCoachPos({ x: r.left, y: r.bottom });
      setCoachRecord(true);
    }
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
      // Every annotation edit re-syncs through here; refresh() is never called on a
      // pristine load without loadFinishedClip() clearing the flag straight after.
      setDirty(true);
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
      setCuts(cs.cuts);
      setShowClicks(cs.show_clicks);
      setShowKeys(cs.show_keys);
      setFramePreset(cs.frame_preset);
      // Covers trim edits and undo/redo, which re-sync clip state through here.
      setDirty(true);
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
      // Cut sections are removed from the output — playback jumps over them.
      const cut = cuts().find((c) => t >= c.start && t < c.end);
      if (cut) t = cut.end;
      if (t >= tEnd()) {
        // GIFs loop — with Loop on, the preview does too.
        if (looping()) {
          t = tStart();
        } else {
          t = tEnd();
          setPlaying(false);
        }
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

  // Move the selected annotation by a normalized delta (arrow-key nudging).
  const nudgeSelected = async (dx: number, dy: number) => {
    const s = selected();
    if (!s) return;
    const g = geomOf(s.kind, s.id).slice();
    if (s.kind === "arrow") {
      g[0] = clamp01(g[0] + dx);
      g[1] = clamp01(g[1] + dy);
      g[2] = clamp01(g[2] + dx);
      g[3] = clamp01(g[3] + dy);
    } else {
      g[0] = clamp01(g[0] + dx);
      g[1] = clamp01(g[1] + dy);
    }
    await applyGeom(s.kind, s.id, g);
    await refresh();
    await pushSeek(playhead());
  };

  const onKey = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest("input, textarea")) return;
    // "?" toggles the keyboard cheat-sheet (Shift+/) — available any time except behind another modal.
    if (e.key === "?" && !showExport() && !showWelcome()) {
      e.preventDefault();
      setShowShortcuts((v) => !v);
      return;
    }
    // A modal owns the screen — its own handler deals with Esc/Tab; don't drive the editor behind it.
    if (showExport() || showWelcome() || showShortcuts()) return;
    if (e.ctrlKey && e.shiftKey && e.code === "KeyR" && recordPhase() === "idle") {
      e.preventDefault();
      void startRecord();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selZoom() !== null) {
      e.preventDefault();
      void deleteSelectedZoom();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selSpeed() !== null) {
      e.preventDefault();
      void deleteSelectedSpeed();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selCut() !== null) {
      e.preventDefault();
      void deleteSelectedCut();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selected()) {
      e.preventDefault();
      void deleteSelected();
    } else if (
      e.key === "Escape" &&
      (selected() || selZoom() !== null || selSpeed() !== null || selCut() !== null)
    ) {
      setSelected(null);
      setSelZoom(null);
      setSelSpeed(null);
      setSelCut(null);
      setSelCut(null);
    } else if (e.code === "Space" && hasClip()) {
      e.preventDefault();
      togglePlay();
    } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && hasClip() && !e.ctrlKey) {
      e.preventDefault();
      const dir = e.key === "ArrowRight" ? 1 : -1;
      if (selected() && !playing()) {
        void nudgeSelected(dir * (e.shiftKey ? 0.02 : 0.005), 0);
      } else {
        const step = e.shiftKey ? 1 : 0.05;
        scrub(Math.min(Math.max(playhead() + dir * step, tStart()), tEnd()));
      }
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && hasClip() && !e.ctrlKey && selected() && !playing()) {
      e.preventDefault();
      const dir = e.key === "ArrowDown" ? 1 : -1;
      void nudgeSelected(0, dir * (e.shiftKey ? 0.02 : 0.005));
    } else if (e.key === "Home" && hasClip()) {
      e.preventDefault();
      scrub(tStart());
    } else if (e.key === "End" && hasClip()) {
      e.preventDefault();
      scrub(tEnd());
    } else if (
      hasClip() &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      !e.shiftKey &&
      editingText() === null &&
      (e.code === "KeyZ" || e.code === "KeyX" || e.code === "KeyC")
    ) {
      // Insert a segment at the playhead — Z/X/C mirror the Insert group (Zoom/Speed/Cut).
      e.preventDefault();
      if (e.code === "KeyZ") void addZoomAtPlayhead();
      else if (e.code === "KeyX") void addSpeedAtPlayhead();
      else void addCutAtPlayhead();
    } else if (
      hasClip() &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      !e.shiftKey &&
      TOOL_KEYS[e.code] &&
      editingText() === null
    ) {
      // Single-key tool switching (V/T/A/L/S/H) — matches the badges on the tool rail.
      e.preventDefault();
      setTool(TOOL_KEYS[e.code]);
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
  const inWindow = (r: TimeRange) => playhead() >= r.start && playhead() < r.end;
  const inView = (r: TimeRange, sel: boolean) => inWindow(r) || (sel && !playing());
  // Selected but outside its window → drawn ghosted, so it's obvious the element is NOT
  // visible at this moment (it's only on screen to stay editable).
  const isGhost = (r: TimeRange, sel: boolean) => sel && !playing() && !inWindow(r);

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
      setDirty(true); // live property / geometry / text edits flow through here
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
  // Approximate width of a text label in normalized-X space (glyph width is in height-
  // fraction units; convert to width fraction). Shared by hit-testing and resize handles.
  const textWNorm = (t: TextAnn) =>
    Math.max(t.text.length * t.font_size * 0.6 * (stage().h / Math.max(stage().w, 1)), 0.05);
  // The live font size for a text label (the scale-text drag override, else the stored size).
  const liveFont = (id: number, fallback: number) => {
    const d = drag();
    return d && d.mode === "scale-text" && d.id === id ? d.cur : fallback;
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
    } else if (s.kind === "text") {
      const t = anns().texts.find((x) => x.id === s.id);
      if (t) {
        const pos = v2(t.pos);
        const w = textWNorm(t);
        const h = t.font_size;
        if (near(pos.x, pos.y)) return "nw";
        if (near(pos.x + w, pos.y)) return "ne";
        if (near(pos.x, pos.y + h)) return "sw";
        if (near(pos.x + w, pos.y + h)) return "se";
      }
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
      const wApprox = textWNorm(t);
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

  // Snap a moved annotation's geometry to the canvas edges/center (0, 0.5, 1) within a
  // pixel-constant threshold, shifting the whole element and flashing crosshair guides.
  const CANVAS_SNAPS = [0, 0.5, 1];
  const snapMoveGeom = (kind: Kind, g: number[]): number[] => {
    const tx = 8 / Math.max(stage().w, 1);
    const ty = 8 / Math.max(stage().h, 1);
    let xs: number[];
    let ys: number[];
    if (kind === "box") {
      xs = [g[0], g[0] + g[2] / 2, g[0] + g[2]];
      ys = [g[1], g[1] + g[3] / 2, g[1] + g[3]];
    } else if (kind === "arrow") {
      xs = [g[0], g[2], (g[0] + g[2]) / 2];
      ys = [g[1], g[3], (g[1] + g[3]) / 2];
    } else {
      xs = [g[0]];
      ys = [g[1]];
    }
    let offX = 0;
    let gx: number | null = null;
    let bestX = tx;
    for (const x of xs)
      for (const s of CANVAS_SNAPS) {
        const dd = Math.abs(x - s);
        if (dd < bestX) {
          bestX = dd;
          offX = s - x;
          gx = s;
        }
      }
    let offY = 0;
    let gy: number | null = null;
    let bestY = ty;
    for (const y of ys)
      for (const s of CANVAS_SNAPS) {
        const dd = Math.abs(y - s);
        if (dd < bestY) {
          bestY = dd;
          offY = s - y;
          gy = s;
        }
      }
    const ng = g.slice();
    ng[0] += offX;
    ng[1] += offY;
    if (kind === "arrow") {
      ng[2] += offX;
      ng[3] += offY;
    }
    setSnapX(gx);
    setSnapY(gy);
    return ng;
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
      setSelCut(null);
      setSelected({ kind: "text", id });
      setEditingText(id);
      if (!toolLock()) setTool("select");
      return;
    }
    if (t === "arrow") {
      setDrag({ mode: "create-arrow", start: p, cur: p });
      return;
    }
    if (t === "line") {
      setDrag({ mode: "create-line", start: p, cur: p });
      return;
    }
    if (t === "shape") {
      setDrag({ mode: "create-box", start: p, cur: p });
      return;
    }
    if (t === "highlight") {
      setDrag({ mode: "create-highlight", start: p, cur: p });
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

    // select tool: handle → resize, body → move, empty → keep selection (closes on ✕/Esc)
    const h = handleAt(p);
    if (h && selected()) {
      const s = selected()!;
      if (s.kind === "text") {
        // Corner-resize a text label = scale its font, anchored to the opposite corner.
        const tx = anns().texts.find((x) => x.id === s.id)!;
        const pos = v2(tx.pos);
        const w = textWNorm(tx);
        const ht = tx.font_size;
        const opp: Record<string, Vec2> = {
          nw: { x: pos.x + w, y: pos.y + ht },
          ne: { x: pos.x, y: pos.y + ht },
          sw: { x: pos.x + w, y: pos.y },
          se: { x: pos.x, y: pos.y },
        };
        const anchor = opp[h];
        const startDist = Math.hypot(p.x - anchor.x, p.y - anchor.y) || 1e-4;
        setDrag({ mode: "scale-text", id: s.id, anchor, startFont: tx.font_size, startDist, cur: tx.font_size });
        return;
      }
      const g = geomOf(s.kind, s.id);
      setDrag({ mode: "resize", kind: s.kind, id: s.id, handle: h, orig: g, geom: g.slice() });
      return;
    }
    const hit = hitTest(p);
    if (hit) {
      setSelZoom(null);
      setSelSpeed(null);
      setSelCut(null);
      setSelected(hit);
      const g = geomOf(hit.kind, hit.id);
      setDrag({ mode: "move", kind: hit.kind, id: hit.id, grab: p, orig: g, geom: g.slice() });
    }
    // Clicking empty space keeps the current selection (and its inspector) open —
    // it only closes on the inspector's ✕ or Esc, so the layout doesn't jump around.
  };

  const onPointerMove = (e: PointerEvent) => {
    const d = drag();
    if (!d) return;
    const p = norm(e);
    if (
      d.mode === "create-arrow" ||
      d.mode === "create-line" ||
      d.mode === "create-box" ||
      d.mode === "create-ellipse" ||
      d.mode === "create-highlight"
    ) {
      setDrag({ ...d, cur: p });
      return;
    }
    if (d.mode === "scale-text") {
      // Font scales with the cursor's distance from the anchored opposite corner.
      const dist = Math.hypot(p.x - d.anchor.x, p.y - d.anchor.y);
      const f = Math.min(0.2, Math.max(0.02, (d.startFont * dist) / d.startDist));
      setDrag({ ...d, cur: f });
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
      g = snapMoveGeom(d.kind, g);
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
    setSnapX(null);
    setSnapY(null);
    const p = norm(e);
    if (d.mode === "create-arrow" || d.mode === "create-line") {
      const isLine = d.mode === "create-line";
      setDrag(null);
      if (Math.hypot(p.x - d.start.x, p.y - d.start.y) > 0.01) {
        const id = await invoke<number>("add_arrow", {
          fx: d.start.x,
          fy: d.start.y,
          tx: p.x,
          ty: p.y,
          t: playhead(),
        });
        if (isLine) await invoke("set_arrow_style", { id, style: "line" });
        await refresh();
        await pushSeek(playhead());
        setSelZoom(null);
        setSelSpeed(null);
        setSelected({ kind: "arrow", id });
        if (!toolLock()) setTool("select");
      }
    } else if (
      d.mode === "create-box" ||
      d.mode === "create-ellipse" ||
      d.mode === "create-highlight"
    ) {
      const cmd =
        d.mode === "create-box"
          ? "add_box"
          : d.mode === "create-ellipse"
            ? "add_ellipse"
            : "add_highlighter";
      setDrag(null);
      const x = Math.min(d.start.x, p.x);
      const y = Math.min(d.start.y, p.y);
      const w = Math.abs(p.x - d.start.x);
      const h = Math.abs(p.y - d.start.y);
      if (w > 0.01 && h > 0.01) {
        const id = await invoke<number>(cmd, { x, y, w, h, t: playhead() });
        await refresh();
        await pushSeek(playhead());
        setSelZoom(null);
        setSelSpeed(null);
        setSelected({ kind: "box", id });
        if (!toolLock()) setTool("select");
      }
    } else if (d.mode === "scale-text") {
      const f = d.cur;
      setDrag(null);
      await invoke("update_text", { id: d.id, fontSize: f });
      await refresh();
      await pushSeek(playhead());
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
  const selectedBox = () => {
    const s = selected();
    return s?.kind === "box" ? anns().highlights.find((b) => b.id === s.id) : undefined;
  };
  const selectedArrow = () => {
    const s = selected();
    return s?.kind === "arrow" ? anns().arrows.find((a) => a.id === s.id) : undefined;
  };
  // The inspector "Content" field is seeded from the model only while it is NOT focused, so
  // the async edit→refresh round-trip can't reset the caret to the end mid-typing.
  let contentInput: HTMLInputElement | undefined;
  createEffect(() => {
    const t = selectedText();
    const el = contentInput;
    if (el && t && document.activeElement !== el) el.value = t.text;
  });
  // Scrub-driven inspector edits (thickness / opacity / colour / font size / text) fire on
  // every pointer-move or keystroke, so they run through pushEdit — the same edit throttle
  // the inline text editor uses — to bound the invoke→refresh→seek round-trips. pushEdit
  // appends the seek and always lets the trailing value land, so the drag-end value sticks.
  const editStyle = (patch: { thickness?: number; filled?: boolean }) =>
    pushEdit(async () => {
      const s = selected();
      if (!s) return;
      await invoke("set_annotation_style", { id: s.id, ...patch });
      await refresh();
    });
  const setShape = async (ellipse: boolean) => {
    const s = selected();
    if (s?.kind !== "box") return;
    await invoke("set_highlight_shape", { id: s.id, ellipse });
    await refresh();
    await pushSeek(playhead());
  };
  const setArrowStyle = async (style: "arrow" | "line" | "double") => {
    const s = selected();
    if (s?.kind !== "arrow") return;
    await invoke("set_arrow_style", { id: s.id, style });
    await refresh();
    await pushSeek(playhead());
  };
  const setOpacity = (a: number) =>
    pushEdit(async () => {
      const s = selected();
      if (!s) return;
      await invoke("set_annotation_opacity", { id: s.id, a });
      await refresh();
    });
  const inspTitle = () => {
    const s = selected()!;
    if (s.kind === "box") {
      const b = selectedBox();
      if (b?.shape === "Ellipse") return "Ellipse";
      if (b?.filled && (b.color.a ?? 1) < 0.6) return "Highlight";
      return "Box";
    }
    if (s.kind === "arrow") return selectedArrow()?.style === "Line" ? "Line" : "Arrow";
    return s.kind[0].toUpperCase() + s.kind.slice(1);
  };
  const selectedColor = (): Color | undefined => {
    const s = selected();
    if (!s) return undefined;
    if (s.kind === "text") return anns().texts.find((t) => t.id === s.id)?.color;
    if (s.kind === "arrow") return anns().arrows.find((a) => a.id === s.id)?.color;
    return anns().highlights.find((b) => b.id === s.id)?.color;
  };
  const setColor = (hex: string) =>
    pushEdit(async () => {
      const s = selected();
      if (!s) return;
      const c = hexRgb(hex);
      await invoke("set_annotation_color", { id: s.id, r: c.r, g: c.g, b: c.b });
      await refresh();
    });
  const editText = (text: string) =>
    pushEdit(async () => {
      const s = selected();
      if (s?.kind !== "text") return;
      await invoke("update_text", { id: s.id, text });
      await refresh();
    });
  const editFontSize = (size: number) =>
    pushEdit(async () => {
      const s = selected();
      if (s?.kind !== "text") return;
      await invoke("update_text", { id: s.id, fontSize: size });
      await refresh();
    });
  const editTextStyle = async (patch: {
    bold?: boolean;
    italic?: boolean;
    background?: boolean;
    font?: string;
  }) => {
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
  // ── undo / redo ────────────────────────────────────────────────────────────────
  const refreshAll = async () => {
    // An undo can change anything — clear selections that may now dangle, resync all.
    setSelected(null);
    setSelZoom(null);
    setSelSpeed(null);
    setSelCut(null);
    setEditingText(null);
    await refresh();
    await refreshClip();
    await pushSeek(playhead());
  };
  const doUndo = async () => {
    if (!hasClip()) return;
    try {
      if (!(await invoke<boolean>("undo"))) {
        setStatus("Nothing to undo");
        return;
      }
      await refreshAll();
      setStatus("Undone");
    } catch (e) {
      setStatus(`Undo failed: ${String(e)}`);
    }
  };
  const doRedo = async () => {
    if (!hasClip()) return;
    try {
      if (!(await invoke<boolean>("redo"))) {
        setStatus("Nothing to redo");
        return;
      }
      await refreshAll();
      setStatus("Redone");
    } catch (e) {
      setStatus(`Redo failed: ${String(e)}`);
    }
  };

  const duplicateSelected = async () => {
    const s = selected();
    if (!s) return;
    try {
      const id = await invoke<number>("duplicate_annotation", { id: s.id });
      await refresh();
      await pushSeek(playhead());
      setSelected({ kind: s.kind, id });
      setStatus("Duplicated — drag the copy into place");
    } catch (e) {
      setStatus(`Duplicate failed: ${String(e)}`);
    }
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
    // A new recording replaces the loaded clip, so warn before throwing away unsaved edits.
    // Soft copy: the previous session's recovery dir survives one more recording.
    if (hasClip() && dirty()) {
      const ok = await ask(
        "Start new recording? Unsaved edits to the current clip will be discarded.",
        { title: "Discard edits?", kind: "warning", okLabel: "Discard & record", cancelLabel: "Cancel" },
      );
      if (!ok) return;
    }
    setCoachRecord(false);
    try {
      setStatus("Choose the area to record…");
      setBackdrop(null);
      setRecordPhase("active"); // overlay shows immediately (dark + presets)
      // enter_overlay hides the editor, grabs the desktop as the selector backdrop, then
      // brings the window back fullscreen + excluded from capture. It returns the frozen
      // desktop as a data-URL (empty string if the grab failed → dark canvas fallback).
      const shot = await invoke<string>("enter_overlay");
      setBackdrop(shot || null);
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
    setSelCut(null);
    setStatus(
      summary.warning ??
        `Recorded ${summary.duration.toFixed(1)}s · ${summary.zooms} zooms`,
    );
    await refresh();
    await refreshClip();
    scrub(trim()?.start ?? 0);
    // A freshly loaded clip (new recording / recover / open project) starts clean —
    // reset after the syncs above, which optimistically flag dirty.
    setDirty(false);
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
      setDirty(true);
      const idx = list.findIndex((z) => playhead() >= z.start - 1e-6 && playhead() <= z.end + 1e-6);
      setSelected(null);
      setSelSpeed(null);
      setSelCut(null);
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
      setDirty(true);
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
      setDirty(true);
      setSelZoom(null);
      await pushSeek(playhead());
    } catch (e) {
      setStatus(`Zoom delete failed: ${String(e)}`);
    }
  };

  // ── zoom focus (follow the cursor, or hold a fixed draggable point) ──────────────
  const selZoomFocus = (): Vec2 | null => {
    const z = selectedZoom();
    if (!z || typeof z.mode !== "object") return null;
    return v2(z.mode.Manual.pos);
  };
  const applyZoomFocus = async (focus: Vec2 | null) => {
    const i = selZoom();
    if (i === null) return;
    try {
      const args = focus ? { index: i, x: focus.x, y: focus.y } : { index: i };
      setZooms(await invoke<ZoomSeg[]>("set_zoom_focus", args));
      setDirty(true);
      await pushSeek(playhead());
      setStatus(focus ? "Zoom aimed at the crosshair" : "Zoom follows the cursor");
    } catch (e) {
      setStatus(`Zoom focus failed: ${String(e)}`);
    }
  };
  // Crosshair dragging on the canvas.
  const [focusDrag, setFocusDrag] = createSignal<Vec2 | null>(null);
  const onFocusDown = (e: PointerEvent) => {
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    setFocusDrag(norm(e));
  };
  const onFocusMove = (e: PointerEvent) => {
    if (focusDrag()) setFocusDrag(norm(e));
  };
  const onFocusUp = async () => {
    const f = focusDrag();
    if (!f) return;
    setFocusDrag(null);
    await applyZoomFocus(f);
  };

  // ── speed-up dead time ─────────────────────────────────────────────────────────
  const toggleSkim = async () => {
    if (!hasClip()) return;
    try {
      if (speed().length > 0) {
        await invoke("clear_speed");
        setSpeed([]);
        setDirty(true);
        setSelSpeed(null);
        setStatus("Idle stretches back to normal speed");
      } else {
        const f = skimFactor();
        const regions = await invoke<SpeedRegion[]>("auto_speed", { factor: f });
        setSpeed(regions);
        setDirty(true);
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
      setDirty(true);
      const idx = list.findIndex((r) => Math.abs(r.start - start) < 0.01);
      setSelected(null);
      setSelZoom(null);
      setSelCut(null);
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
      setDirty(true);
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
      setDirty(true);
      setSelSpeed(null);
    } catch (e) {
      setStatus(`Speed delete failed: ${String(e)}`);
    }
  };

  // ── cuts (sections removed from the output) ────────────────────────────────────
  const selectedCut = () => {
    const i = selCut();
    return i === null ? undefined : cuts()[i];
  };
  const addCutAtPlayhead = async () => {
    if (!hasClip()) return;
    try {
      const start = Math.min(playhead(), Math.max(0, duration() - 0.2));
      const end = Math.min(start + 1, duration());
      const list = await invoke<Trim[]>("add_cut", { start, end });
      setCuts(list);
      setDirty(true);
      const idx = list.findIndex((c) => Math.abs(c.start - start) < 0.01);
      setSelected(null);
      setSelZoom(null);
      setSelSpeed(null);
      setSelCut(idx >= 0 ? idx : null);
      setStatus("Section cut — drag the band to choose exactly what's removed");
    } catch (e) {
      setStatus(`Could not cut: ${String(e)}`);
    }
  };
  const applyCutEdit = async (index: number, start: number, end: number) => {
    try {
      const list = await invoke<Trim[]>("update_cut", { index, start, end });
      setCuts(list);
      setDirty(true);
      // Re-find the edited cut (the list re-sorts by start).
      const idx = list.findIndex((c) => Math.abs(c.start - Math.min(start, end)) < 0.25);
      if (idx >= 0) setSelCut(idx);
    } catch (e) {
      setStatus(`Cut edit failed: ${String(e)}`);
    }
  };
  const deleteSelectedCut = async () => {
    const i = selCut();
    if (i === null) return;
    try {
      setCuts(await invoke<Trim[]>("delete_cut", { index: i }));
      setDirty(true);
      setSelCut(null);
      setStatus("Section restored");
    } catch (e) {
      setStatus(`Restore failed: ${String(e)}`);
    }
  };

  // ── frame preset (padding + rounded corners + shadow around the recording) ──────
  const applyFramePreset = async (preset: string) => {
    if (!hasClip()) return;
    try {
      await invoke("set_frame_preset", { preset });
      setFramePreset(preset);
      setDirty(true);
      await pushSeek(playhead());
      setStatus(
        preset === "none" ? "Frame removed — edge-to-edge export" : `Frame: ${preset}`,
      );
    } catch (e) {
      setStatus(`Frame failed: ${String(e)}`);
    }
  };

  // ── click ripples ──────────────────────────────────────────────────────────────
  const toggleClicks = async () => {
    if (!hasClip()) return;
    try {
      const on = !showClicks();
      await invoke("set_show_clicks", { on });
      setShowClicks(on);
      setDirty(true);
      await pushSeek(playhead());
      setStatus(on ? "Mouse clicks will ripple in the GIF" : "Click ripples off");
    } catch (e) {
      setStatus(`Click ripples failed: ${String(e)}`);
    }
  };

  // ── keystroke overlay ──────────────────────────────────────────────────────────
  const toggleKeys = async () => {
    if (!hasClip()) return;
    try {
      const on = !showKeys();
      await invoke("set_show_keys", { on });
      setShowKeys(on);
      setDirty(true);
      await pushSeek(playhead());
      setStatus(
        on
          ? "Shortcuts you pressed will show as chips (plain typing never does)"
          : "Keystroke overlay off",
      );
    } catch (e) {
      setStatus(`Keystroke overlay failed: ${String(e)}`);
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
  // `force` is set by the explicit edge handles ("l"/"r") and the body ("move"); a forced
  // l/r counts as moved immediately so a small edge drag resizes instead of scrubbing.
  const onZoomDown = (idx: number, z: ZoomSeg, force: "l" | "r" | "move") => (e: PointerEvent) => {
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setZoomDrag({
      idx,
      mode: force,
      grabT: tlTime(e),
      orig: { ...z },
      cur: { start: z.start, end: z.end },
      moved: force !== "move",
    });
  };
  // Clamp a dragged segment against its same-type neighbours so the timeline never shows
  // an overlap. `prevEnd` / `nextStart` are the facing edges of the adjacent segments
  // (folded together with the [0, duration] bounds by the callers).
  const clampSegDrag = (
    mode: "move" | "l" | "r",
    orig: { start: number; end: number },
    dt: number,
    minLen: number,
    prevEnd: number,
    nextStart: number,
  ) => {
    let { start, end } = orig;
    if (mode === "move") {
      const len = end - start;
      start = Math.min(Math.max(prevEnd, start + dt), nextStart - len);
      end = start + len;
    } else if (mode === "l") {
      start = Math.min(Math.max(prevEnd, start + dt), end - minLen);
    } else {
      end = Math.max(Math.min(nextStart, end + dt), start + minLen);
    }
    return { start, end };
  };
  const onZoomMove = (e: PointerEvent) => {
    const d = zoomDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    const arr = zooms();
    const prevEnd = Math.max(0, arr[d.idx - 1]?.end ?? 0);
    const nextStart = Math.min(duration(), arr[d.idx + 1]?.start ?? duration());
    const { start, end } = clampSegDrag(d.mode, d.orig, dt, 0.2, prevEnd, nextStart);
    setZoomDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onZoomUp = async () => {
    const d = zoomDrag();
    if (!d) return;
    setZoomDrag(null);
    setSelected(null);
    setSelSpeed(null);
    setSelCut(null);
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
  const onSpeedDown = (idx: number, r: SpeedRegion, force: "l" | "r" | "move") => (e: PointerEvent) => {
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setSpeedDrag({
      idx,
      mode: force,
      grabT: tlTime(e),
      orig: { ...r },
      cur: { start: r.start, end: r.end },
      moved: force !== "move",
    });
  };
  const onSpeedMove = (e: PointerEvent) => {
    const d = speedDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    const arr = speed();
    const prevEnd = Math.max(0, arr[d.idx - 1]?.end ?? 0);
    const nextStart = Math.min(duration(), arr[d.idx + 1]?.start ?? duration());
    const { start, end } = clampSegDrag(d.mode, d.orig, dt, 0.2, prevEnd, nextStart);
    setSpeedDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onSpeedUp = async () => {
    const d = speedDrag();
    if (!d) return;
    setSpeedDrag(null);
    setSelected(null);
    setSelZoom(null);
    setSelCut(null);
    if (d.moved) {
      await applySpeedEdit(d.idx, d.cur.start, d.cur.end, speed()[d.idx]?.factor ?? skimFactor());
    } else {
      // A plain click: select the region and jump to it.
      setSelSpeed(d.idx);
      scrub(d.orig.start);
    }
  };

  // Cut-band dragging: grab the chip to move the cut, its edges (8px) to resize.
  const [cutDrag, setCutDrag] = createSignal<{
    idx: number;
    mode: "move" | "l" | "r";
    grabT: number;
    orig: Trim;
    cur: { start: number; end: number };
    moved: boolean;
  } | null>(null);
  const cutGeom = (idx: number, c: Trim) => {
    const d = cutDrag();
    return d && d.idx === idx ? d.cur : { start: c.start, end: c.end };
  };
  const onCutDown = (idx: number, c: Trim, force: "l" | "r" | "move") => (e: PointerEvent) => {
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setCutDrag({
      idx,
      mode: force,
      grabT: tlTime(e),
      orig: { ...c },
      cur: { start: c.start, end: c.end },
      moved: force !== "move",
    });
  };
  const onCutMove = (e: PointerEvent) => {
    const d = cutDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    const arr = cuts();
    const prevEnd = Math.max(0, arr[d.idx - 1]?.end ?? 0);
    const nextStart = Math.min(duration(), arr[d.idx + 1]?.start ?? duration());
    const { start, end } = clampSegDrag(d.mode, d.orig, dt, 0.1, prevEnd, nextStart);
    setCutDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onCutUp = async () => {
    const d = cutDrag();
    if (!d) return;
    setCutDrag(null);
    setSelected(null);
    setSelZoom(null);
    setSelSpeed(null);
    if (d.moved) {
      await applyCutEdit(d.idx, d.cur.start, d.cur.end);
    } else {
      // A plain click: select the cut and jump to it.
      setSelCut(d.idx);
      scrub(d.orig.start);
    }
  };

  const pct = (t: number) => (duration() > 0 ? (t / duration()) * 100 : 0);
  // Adaptive ruler: pick a "nice" major interval targeting ~90px between labels, then a
  // minor subdivision that keeps minor ticks ≥11px apart. The midpoint minor is drawn
  // taller. Mirrors the spacing logic in pro editors instead of a fixed tick count.
  const NICE_STEPS = [0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1200, 1800, 3600];
  const pxPerSec = () => (duration() > 0 ? tlWidth() / duration() : 0);
  const tickStep = () => {
    const target = pxPerSec() > 0 ? 90 / pxPerSec() : duration();
    return NICE_STEPS.find((s) => s >= target) ?? NICE_STEPS[NICE_STEPS.length - 1];
  };
  const minorStep = (major: number) => {
    for (const div of [5, 4, 2]) {
      const s = major / div;
      if (s * pxPerSec() >= 11) return s;
    }
    return major;
  };
  const tickMarks = () => {
    const d = duration();
    if (d <= 0) return [];
    const major = tickStep();
    const minor = minorStep(major);
    const out: { t: number; major: boolean; mid: boolean }[] = [];
    const count = Math.floor(d / minor + 1e-9);
    for (let i = 0; i <= count; i++) {
      const t = i * minor;
      const ratio = t / major;
      const isMajor = Math.abs(ratio - Math.round(ratio)) < 1e-6;
      const frac = ((t % major) + major) % major;
      const isMid = !isMajor && Math.abs(frac - major / 2) < minor / 8;
      out.push({ t, major: isMajor, mid: isMid });
    }
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
    (b: { kind: Kind; id: number; start: number; end: number }, force: "l" | "r" | "move") =>
    (e: PointerEvent) => {
      e.stopPropagation();
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      setAnnDrag({
        kind: b.kind,
        id: b.id,
        mode: force,
        grabT: tlTime(e),
        orig: { start: b.start, end: b.end },
        cur: { start: b.start, end: b.end },
        moved: force !== "move",
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
    setSelCut(null);
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
      bars.push({
        kind: "arrow",
        id: ar.id,
        start: ar.range.start,
        end: ar.range.end,
        label: ar.style === "Line" ? "Line" : "Arrow",
      });
    for (const b of a.highlights)
      bars.push({
        kind: "box",
        id: b.id,
        start: b.range.start,
        end: b.range.end,
        label: b.shape === "Ellipse" ? "Ellipse" : "Box",
      });
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
  const somethingSelected = () =>
    !!selected() || selZoom() !== null || selSpeed() !== null || selCut() !== null;
  // A drawing tool is armed (not Select). We reserve the inspector column for it so the
  // canvas doesn't reflow the moment you place an element (which would jump the editor box).
  const drawingToolActive = () => hasClip() && tool() !== "select";
  const inspectorOpen = () => somethingSelected() || drawingToolActive();

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
      setDirty(false);
      setStatus(`Saved ${dir}`);
    } catch (e) {
      setStatus(`Save failed: ${String(e)}`);
    }
  };

  const onRecover = async () => {
    setStatus("Recovering your last session…");
    try {
      const summary = await invoke<RecordingSummary>("recover_session");
      setRecoverable(null);
      await loadFinishedClip(summary);
      setStatus(`Recovered ${summary.duration.toFixed(1)}s — don't forget to export`);
    } catch (e) {
      setRecoverable(null);
      setStatus(`Recovery failed: ${String(e)}`);
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
      <header class="topbar" data-tauri-drag-region="">
        <LogoWordmark />
        <button
          class="btn record"
          ref={(el) => (recordBtnEl = el)}
          title="Record your screen (Ctrl+Shift+R) — captures the monitor Vuoom is on"
          onClick={() => void startRecord()}
        >
          <span class="dot" /> Record
        </button>
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

        {/* Flexible draggable gap — keeps the window movable and pins actions right. */}
        <div class="topbar-drag" data-tauri-drag-region="" />

        <button class="btn ghost" disabled={!hasClip()} title="Undo (Ctrl+Z)" onClick={() => void doUndo()}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
            <path d="M8.5 5L4 9.5 8.5 14M4 9.5h10a6 6 0 0 1 0 12h-3" />
          </svg>
        </button>
        <button class="btn ghost" disabled={!hasClip()} title="Redo (Ctrl+Y)" onClick={() => void doRedo()}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
            <path d="M15.5 5L20 9.5 15.5 14M20 9.5H10a6 6 0 0 0 0 12h3" />
          </svg>
        </button>
        <span class="toolbar-sep" />
        <button class="btn ghost" title="Open a saved project (Ctrl+O)" onClick={() => void onOpenProject()}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 8V6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v2M3 8h17.2a1 1 0 0 1 .97 1.24l-2 8a1 1 0 0 1-.97.76H4a1 1 0 0 1-1-1z" />
          </svg>
        </button>
        <button class="btn ghost" disabled={!hasClip()} title="Save project — video + edits (Ctrl+S)" onClick={() => void onSaveProject()}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 3h11l5 5v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 3v5h8V3M7 21v-7h10v7" />
          </svg>
        </button>
        <span class="toolbar-sep" />
        <select
          class="tbtn-sel"
          title="Frame the recording on a padded backdrop with rounded corners + shadow"
          disabled={!hasClip()}
          value={framePreset()}
          onChange={(e) => void applyFramePreset(e.currentTarget.value)}
        >
          <option value="none">No frame</option>
          <option value="subtle">Subtle frame</option>
          <option value="studio">Studio frame</option>
        </select>
        <button class="btn export" disabled={!hasClip()} title="Export a GIF or MP4 (Ctrl+E)" onClick={() => setShowExport(true)}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3v12m0 0l-4.5-4.5M12 15l4.5-4.5M4 21h16" />
          </svg>
          Export
        </button>
        <Show when={update()}>
          <button
            class="btn update-pill"
            disabled={updating()}
            title={`Update to v${update()!.version} and restart`}
            onClick={() => void runUpdate()}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 3v10m0 0l-4-4m4 4l4-4M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" />
            </svg>
            {updating() ? "Updating…" : `Update ${update()!.version}`}
          </button>
        </Show>
        <span class="toolbar-sep" />
        <button
          class="btn ghost"
          title="Keyboard shortcuts (?)"
          aria-label="Keyboard shortcuts"
          onClick={() => setShowShortcuts(true)}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2" y="6" width="20" height="12" rx="2" />
            <path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M6 14h.01M18 14h.01M9.5 14h5" />
          </svg>
        </button>
        <ThemeMenu current={theme()} onSelect={setTheme} />
        <WindowControls />
      </header>

      <div
        class="workspace"
        style={{
          // The tool rail only matters once there's a clip to annotate, so it (and its
          // 76px column) drops out of the empty editor — keeping the focus on Record.
          "grid-template-columns": hasClip()
            ? inspectorOpen()
              ? `76px 1fr ${inspectorW()}px`
              : "76px 1fr"
            : "1fr",
        }}
      >
        <Show when={hasClip()}>
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
                  <kbd class="tool-key">{t.key}</kbd>
                </button>
              )}
            </For>
            <div class="toolrail-spacer" />
            <button
              classList={{ tool: true, "tool-lock": true, active: toolLock() }}
              title={
                toolLock()
                  ? "Tool lock on — the drawing tool stays active so you can add several in a row"
                  : "Tool lock off — switches back to Select after you add an element"
              }
              onClick={() => setToolLock(!toolLock())}
            >
              <LockIcon locked={toolLock()} />
              <span>Lock</span>
            </button>
          </nav>
        </Show>

        <main class="canvas">
          <Show when={hasClip()}>
            <div class="tool-hint">{TOOLS.find((t) => t.id === tool())?.hint}</div>
          </Show>
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
                <p class="sub">
                  Record your screen — Vuoom zooms in where you click, then exports a crisp
                  GIF or MP4.
                </p>
                <button class="btn record cta" onClick={() => void startRecord()}>
                  <span class="dot" /> Start recording
                </button>
                <span class="placeholder-hint">
                  <kbd>Ctrl+Shift+R</kbd> record · <kbd>Ctrl+Shift+Z</kbd> zoom ·{" "}
                  <kbd>Ctrl+Shift+X</kbd> stop
                </span>
                <Show when={recoverable() !== null}>
                  <div class="recents">
                    <span class="recents-label">Pick up where you left off</span>
                    <button
                      class="recent-card"
                      title="Recover your last recording and its edits"
                      onClick={() => void onRecover()}
                    >
                      <div class="recent-thumb">
                        <svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                          <rect x="3" y="5" width="18" height="14" rx="2" />
                          <path d="M3 9h18M7 5v14M17 5v14M3 14h4M17 14h4" />
                        </svg>
                      </div>
                      <div class="recent-meta">
                        <strong>Last session</strong>
                        <small>{recoverable()!.toFixed(1)}s · recover</small>
                      </div>
                    </button>
                  </div>
                </Show>
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
                            <g opacity={isGhost(b.range, sel()) ? 0.35 : 1}>
                              <Show
                                when={b.shape === "Ellipse"}
                                fallback={
                                  <rect
                                    x={a().x}
                                    y={a().y}
                                    width={s().x}
                                    height={s().y}
                                    fill={b.filled ? cssColor(b.color) : "none"}
                                    stroke={cssColor(b.color)}
                                    stroke-width={Math.max(b.thickness * stage().h, 1.5)}
                                  />
                                }
                              >
                                <ellipse
                                  cx={a().x + s().x / 2}
                                  cy={a().y + s().y / 2}
                                  rx={s().x / 2}
                                  ry={s().y / 2}
                                  fill={b.filled ? cssColor(b.color) : "none"}
                                  stroke={cssColor(b.color)}
                                  stroke-width={Math.max(b.thickness * stage().h, 1.5)}
                                />
                              </Show>
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
                            </g>
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
                            <g opacity={isGhost(ar.range, sel()) ? 0.35 : 1}>
                              <ArrowLine
                                from={f()}
                                to={tp()}
                                color={cssColor(ar.color)}
                                width={Math.max(ar.thickness * stage().h, 1.5)}
                                headFrom={arrowHeads(ar.style).from}
                                headTo={arrowHeads(ar.style).to}
                              />
                              <Show when={sel()}>
                                <Handles pts={[f(), tp()]} />
                              </Show>
                            </g>
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
                      <Show when={inView(tx.range, sel()) && editingText() !== tx.id}>
                        {(() => {
                          const g = () => liveGeom("text", tx.id);
                          const p = () => px({ x: g()[0], y: g()[1] });
                          const fs = () => liveFont(tx.id, tx.font_size) * stage().h;
                          const wbox = () => Math.max(40, tx.text.length * fs() * 0.6);
                          return (
                            <g opacity={isGhost(tx.range, sel()) ? 0.35 : 1}>
                              <Show when={tx.background}>
                                <rect
                                  class="text-plate"
                                  x={p().x - fs() * 0.3}
                                  y={p().y - fs() * 0.16}
                                  width={wbox() + fs() * 0.6}
                                  height={fs() * 1.25 + fs() * 0.32}
                                  rx={fs() * 0.12}
                                />
                              </Show>
                              <text
                                x={p().x}
                                y={p().y + fs()}
                                font-size={String(fs())}
                                fill={cssColor(tx.color)}
                                style={{
                                  "font-family": fontCss(tx.font),
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
                                  width={wbox() + 8}
                                  height={fs() + 8}
                                />
                                <Handles
                                  pts={[
                                    { x: p().x, y: p().y },
                                    { x: p().x + wbox(), y: p().y },
                                    { x: p().x, y: p().y + fs() },
                                    { x: p().x + wbox(), y: p().y + fs() },
                                  ]}
                                />
                              </Show>
                            </g>
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
                <Show when={drag()?.mode === "create-line"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    return <ArrowLine from={px(d.start)} to={px(d.cur)} color="#e5484d" headTo={false} />;
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
                <Show when={drag()?.mode === "create-highlight"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    const a = px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
                    const w = Math.abs(d.cur.x - d.start.x) * stage().w;
                    const h = Math.abs(d.cur.y - d.start.y) * stage().h;
                    return (
                      <rect x={a.x} y={a.y} width={w} height={h} fill="rgba(255,214,63,0.3)" stroke="#ffd23f" stroke-width={1.5} />
                    );
                  })()}
                </Show>
                <Show when={drag()?.mode === "create-ellipse"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    const a = px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
                    const w = Math.abs(d.cur.x - d.start.x) * stage().w;
                    const h = Math.abs(d.cur.y - d.start.y) * stage().h;
                    return (
                      <ellipse cx={a.x + w / 2} cy={a.y + h / 2} rx={w / 2} ry={h / 2} fill="none" stroke="#ffd23f" stroke-width={2} />
                    );
                  })()}
                </Show>

                {/* Zoom focus crosshair — drag to aim the selected zoom segment. */}
                <Show when={selZoomFocus()}>
                  {(() => {
                    const f = () => focusDrag() ?? selZoomFocus()!;
                    const p = () => px(f());
                    return (
                      <g
                        class="focus-reticle"
                        onPointerDown={onFocusDown}
                        onPointerMove={onFocusMove}
                        onPointerUp={() => void onFocusUp()}
                      >
                        <circle class="ring" cx={p().x} cy={p().y} r={16} />
                        <circle class="dot" cx={p().x} cy={p().y} r={3} />
                        <line x1={p().x - 26} y1={p().y} x2={p().x - 10} y2={p().y} />
                        <line x1={p().x + 10} y1={p().y} x2={p().x + 26} y2={p().y} />
                        <line x1={p().x} y1={p().y - 26} x2={p().x} y2={p().y - 10} />
                        <line x1={p().x} y1={p().y + 10} x2={p().x} y2={p().y + 26} />
                      </g>
                    );
                  })()}
                </Show>

                {/* Canvas alignment guides — flash when a dragged element snaps. */}
                <Show when={snapX() !== null}>
                  <line
                    class="canvas-snap"
                    x1={snapX()! * stage().w}
                    y1={0}
                    x2={snapX()! * stage().w}
                    y2={stage().h}
                  />
                </Show>
                <Show when={snapY() !== null}>
                  <line
                    class="canvas-snap"
                    x1={0}
                    y1={snapY()! * stage().h}
                    x2={stage().w}
                    y2={snapY()! * stage().h}
                  />
                </Show>
              </svg>

              <Show when={editingTextAnn()}>
                {(() => {
                  const id = editingText()!;
                  // Reactive accessors so the editor box tracks the label as the canvas
                  // resizes (e.g. when the inspector opens). The value stays uncontrolled
                  // (seeded once) so typing never resets the caret.
                  const live = () => anns().texts.find((t) => t.id === id);
                  const initial = live()?.text ?? "";
                  const p = () => {
                    const t = live();
                    return t ? px({ x: v2(t.pos).x, y: v2(t.pos).y }) : { x: 0, y: 0 };
                  };
                  const fs = () => (live()?.font_size ?? 0.05) * stage().h;
                  return (
                    <input
                      class="text-edit"
                      style={{
                        left: `${p().x}px`,
                        top: `${p().y}px`,
                        "font-size": `${fs()}px`,
                        "font-family": fontCss(live()?.font ?? ""),
                        "font-weight": live()?.bold ? "700" : "400",
                        "font-style": live()?.italic ? "italic" : "normal",
                      }}
                      value={initial}
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
          <InspectorPanel
            title={inspTitle()}
            onClose={() => setSelected(null)}
            onResizeDown={onInspDown}
            onResizeMove={onInspMove}
            onResizeUp={onInspUp}
          >
            <Show when={selectedText()}>
              <InspSection title="Text">
                <InspRow label="Content" stack>
                  <input
                    class="insp-text-input"
                    type="text"
                    spellcheck={false}
                    ref={(el) => (contentInput = el)}
                    onInput={(e) => void editText(e.currentTarget.value)}
                  />
                </InspRow>
                <InspRow label="Style">
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
                    <button
                      classList={{ stylebtn: true, label: true, on: selectedText()!.background }}
                      title="Background plate — keeps the caption legible over busy footage"
                      onClick={() => void editTextStyle({ background: !selectedText()!.background })}
                    >
                      BG
                    </button>
                  </div>
                </InspRow>
                <InspRow label="Font" stack>
                  <div class="font-grid">
                    <For each={TEXT_FONTS}>
                      {(f) => (
                        <button
                          classList={{ fontbtn: true, on: (selectedText()!.font || "") === f.id }}
                          style={{ "font-family": f.css }}
                          title={f.label}
                          onClick={() => void editTextStyle({ font: f.id })}
                        >
                          {f.label}
                        </button>
                      )}
                    </For>
                  </div>
                </InspRow>
                <InspRow label="Size">
                  <ScrubField
                    value={selectedText()!.font_size}
                    min={0.02}
                    max={0.2}
                    step={0.005}
                    displayScale={100}
                    suffix="%"
                    title="Font size as a percent of the video height — drag to scrub, click to type"
                    onInput={(v) => void editFontSize(v)}
                    onCommit={(v) => void editFontSize(v)}
                  />
                </InspRow>
              </InspSection>
            </Show>

            <Show when={selectedBox()}>
              <InspSection title="Shape">
                <InspRow label="Shape">
                  <div class="style-row">
                    <button
                      classList={{ stylebtn: true, label: true, on: selectedBox()!.shape !== "Ellipse" }}
                      onClick={() => void setShape(false)}
                    >
                      Rectangle
                    </button>
                    <button
                      classList={{ stylebtn: true, label: true, on: selectedBox()!.shape === "Ellipse" }}
                      onClick={() => void setShape(true)}
                    >
                      Ellipse
                    </button>
                  </div>
                </InspRow>
                <InspRow label="Fill">
                  <div class="style-row">
                    <button
                      classList={{ stylebtn: true, label: true, on: !selectedBox()!.filled }}
                      onClick={() => void editStyle({ filled: false })}
                    >
                      Outline
                    </button>
                    <button
                      classList={{ stylebtn: true, label: true, on: selectedBox()!.filled }}
                      onClick={() => void editStyle({ filled: true })}
                    >
                      Filled
                    </button>
                  </div>
                </InspRow>
                <InspRow label="Opacity">
                  <ScrubField
                    value={selectedBox()!.color.a ?? 1}
                    min={0.1}
                    max={1}
                    step={0.05}
                    displayScale={100}
                    suffix="%"
                    title="Opacity — drag to scrub, click to type"
                    onInput={(v) => void setOpacity(v)}
                    onCommit={(v) => void setOpacity(v)}
                  />
                </InspRow>
                <Show when={!selectedBox()!.filled}>
                  <InspRow label="Thickness">
                    <ScrubField
                      value={selectedBox()!.thickness}
                      min={0.002}
                      max={0.02}
                      step={0.001}
                      displayScale={100}
                      suffix="%"
                      title="Outline thickness as a percent of height"
                      onInput={(v) => void editStyle({ thickness: v })}
                      onCommit={(v) => void editStyle({ thickness: v })}
                    />
                  </InspRow>
                </Show>
              </InspSection>
            </Show>
            <Show when={selectedArrow()}>
              <InspSection title="Style">
                <InspRow label="Ends">
                  <div class="style-row">
                    <button
                      classList={{ stylebtn: true, label: true, on: (selectedArrow()!.style ?? "Arrow") === "Arrow" }}
                      onClick={() => void setArrowStyle("arrow")}
                    >
                      Arrow
                    </button>
                    <button
                      classList={{ stylebtn: true, label: true, on: selectedArrow()!.style === "Line" }}
                      onClick={() => void setArrowStyle("line")}
                    >
                      Line
                    </button>
                    <button
                      classList={{ stylebtn: true, label: true, on: selectedArrow()!.style === "DoubleArrow" }}
                      onClick={() => void setArrowStyle("double")}
                    >
                      Double
                    </button>
                  </div>
                </InspRow>
                <InspRow label="Thickness">
                  <ScrubField
                    value={selectedArrow()!.thickness}
                    min={0.002}
                    max={0.02}
                    step={0.001}
                    displayScale={100}
                    suffix="%"
                    title="Stroke thickness as a percent of height"
                    onInput={(v) => void editStyle({ thickness: v })}
                    onCommit={(v) => void editStyle({ thickness: v })}
                  />
                </InspRow>
              </InspSection>
            </Show>

            <Show when={selectedColor()}>
              <InspSection title="Color">
                <InspRow label="Color" stack>
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
                </InspRow>
              </InspSection>
            </Show>

            <Show when={selectedRange()}>
              <InspSection title="Timing">
                <InspRow label="Appears">
                  <ScrubField
                    value={Number(selectedRange()!.start.toFixed(1))}
                    min={0}
                    max={duration()}
                    step={0.1}
                    suffix="s"
                    title="When this appears — drag to scrub, click to type"
                    onCommit={(v) => void editRange(v, selectedRange()!.end)}
                  />
                </InspRow>
                <InspRow label="Disappears">
                  <ScrubField
                    value={Number(selectedRange()!.end.toFixed(1))}
                    min={0}
                    max={duration()}
                    step={0.1}
                    suffix="s"
                    title="When this disappears — drag to scrub, click to type"
                    onCommit={(v) => void editRange(selectedRange()!.start, v)}
                  />
                </InspRow>
              </InspSection>
            </Show>

            <Show when={selectedRange() && isGhost(selectedRange()!, true)}>
              <p class="muted small ghost-note">
                Hidden at the playhead (shown dimmed for editing) — it appears from{" "}
                {fmt(selectedRange()!.start)} to {fmt(selectedRange()!.end)}.
              </p>
            </Show>
            <p class="muted small">Drag to move · drag a handle to resize · Delete to remove.</p>
            <button class="btn block" title="Duplicate (Ctrl+D)" onClick={() => void duplicateSelected()}>
              Duplicate
            </button>
            <button class="btn danger" onClick={() => void deleteSelected()}>
              Delete element
            </button>
          </InspectorPanel>
        </Show>

        <Show when={selZoom() !== null && selectedZoom()}>
          <InspectorPanel
            title="Zoom"
            onClose={() => setSelZoom(null)}
            onResizeDown={onInspDown}
            onResizeMove={onInspMove}
            onResizeUp={onInspUp}
          >
            <InspSection title="Zoom">
              <InspRow label="Strength">
                <ScrubField
                  value={selectedZoom()!.amount}
                  min={1.2}
                  max={4}
                  step={0.1}
                  suffix="×"
                  title="Zoom strength — drag to scrub, click to type"
                  onCommit={(v) => {
                    const z = selectedZoom()!;
                    void applyZoomEdit(selZoom()!, z.start, z.end, v);
                  }}
                />
              </InspRow>
              <InspRow label="Focus">
                <div class="style-row">
                  <button
                    classList={{ stylebtn: true, label: true, on: !selZoomFocus() }}
                    title="The camera follows your recorded cursor"
                    onClick={() => void applyZoomFocus(null)}
                  >
                    Follow cursor
                  </button>
                  <button
                    classList={{ stylebtn: true, label: true, on: !!selZoomFocus() }}
                    title="Hold one spot — drag the crosshair on the video to aim"
                    onClick={() => {
                      if (!selZoomFocus()) void applyZoomFocus({ x: 0.5, y: 0.5 });
                    }}
                  >
                    Fixed point
                  </button>
                </div>
              </InspRow>
            </InspSection>
            <Show when={selZoomFocus()}>
              <p class="muted small">Drag the crosshair on the video to aim this zoom.</p>
            </Show>
            <p class="muted small">
              {fmt(selectedZoom()!.start)} – {fmt(selectedZoom()!.end)} · drag the block on the
              timeline to retime, drag its edges to resize.
            </p>
            <button class="btn danger" onClick={() => void deleteSelectedZoom()}>
              Delete zoom
            </button>
          </InspectorPanel>
        </Show>

        <Show when={selSpeed() !== null && selectedSpeed()}>
          <InspectorPanel
            title="Speed"
            onClose={() => setSelSpeed(null)}
            onResizeDown={onInspDown}
            onResizeMove={onInspMove}
            onResizeUp={onInspUp}
          >
            <InspSection title="Speed">
              <InspRow label="Rate">
                <ScrubField
                  value={selectedSpeed()!.factor}
                  min={1.25}
                  max={8}
                  step={0.25}
                  suffix="×"
                  title="Playback rate — drag to scrub, click to type"
                  onCommit={(v) => {
                    const r = selectedSpeed()!;
                    void applySpeedEdit(selSpeed()!, r.start, r.end, v);
                  }}
                />
              </InspRow>
            </InspSection>
            <p class="muted small">
              {fmt(selectedSpeed()!.start)} – {fmt(selectedSpeed()!.end)} · drag the band on the
              timeline to retime, drag its edges to resize.
            </p>
            <button class="btn danger" onClick={() => void deleteSelectedSpeed()}>
              Delete speed region
            </button>
          </InspectorPanel>
        </Show>

        <Show when={selCut() !== null && selectedCut()}>
          <InspectorPanel
            title="Cut"
            onClose={() => setSelCut(null)}
            onResizeDown={onInspDown}
            onResizeMove={onInspMove}
            onResizeUp={onInspUp}
          >
            <p class="muted small">
              {fmt(selectedCut()!.start)} – {fmt(selectedCut()!.end)} is removed from the GIF —
              playback and export skip straight over it. Drag the band on the timeline to retime,
              drag its edges to resize.
            </p>
            <button class="btn danger" onClick={() => void deleteSelectedCut()}>
              Restore this section
            </button>
          </InspectorPanel>
        </Show>

        {/* Tool-context panel: holds the inspector column while a drawing tool is armed
            (nothing selected yet) so placing an element never reflows the canvas. */}
        <Show when={drawingToolActive() && !somethingSelected()}>
          <aside class="properties">
            <div class="inspector-head">
              <h2>{TOOLS.find((t) => t.id === tool())?.label}</h2>
            </div>
            <p class="muted small">{TOOLS.find((t) => t.id === tool())?.hint}</p>
            <p class="muted small">
              Press <kbd>V</kbd> for Select, or pick another tool on the left. Its options appear
              here once you place an element.
            </p>
          </aside>
        </Show>
      </div>

      <footer class="timeline">
        <div class="transport">
          <Show when={hasClip()}>
          {/* Playback transport */}
          <div class="tgroup">
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
            <button
              class="tbtn"
              classList={{ on: looping() }}
              title="Loop playback — preview the clip the way the GIF loops"
              disabled={!hasClip()}
              onClick={() => setLooping(!looping())}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M17 2l4 4-4 4" />
                <path d="M3 11v-1a4 4 0 0 1 4-4h14" />
                <path d="M7 22l-4-4 4-4" />
                <path d="M21 13v1a4 4 0 0 1-4 4H3" />
              </svg>
            </button>
            <span class="time">
              {fmtT(playhead())} <span class="time-sep">/</span> {fmt(duration())}
              <Show when={trim() || speed().length > 0 || cuts().length > 0}>
                <span class="time-out" title="Final GIF duration after trim + speed-up + cuts">
                  → {outputDuration(duration(), trim(), speed(), cuts()).toFixed(1)}s
                </span>
              </Show>
            </span>
          </div>

          {/* Insert a segment at the playhead — each becomes a draggable band on the timeline */}
          <div class="tgroup labeled">
            <span class="tgroup-label">Insert</span>
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
              title={`Mark 2s at the playhead to play at ${skimFactor()}× — drag the band to retime`}
              disabled={!hasClip()}
              onClick={() => void addSpeedAtPlayhead()}
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                <path d="M5 5l7 7-7 7M13 5l7 7-7 7" />
              </svg>
              <span>Speed</span>
            </button>
            <button
              class="tbtn wide"
              title="Remove 1s at the playhead from the GIF — drag the band to choose the exact section"
              disabled={!hasClip()}
              onClick={() => void addCutAtPlayhead()}
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="6" cy="6" r="2.6" />
                <circle cx="6" cy="18" r="2.6" />
                <path d="M8.1 7.8L20 19M8.1 16.2L20 5" />
              </svg>
              <span>Cut</span>
            </button>
          </div>

          {/* Whole-clip enhancements you toggle on or off */}
          <div class="tgroup labeled">
            <span class="tgroup-label">Enhance</span>
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
              classList={{ on: showClicks() }}
              title="Draw an expanding ripple at every recorded mouse click (shows in the GIF)"
              disabled={!hasClip()}
              onClick={() => void toggleClicks()}
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <circle cx="12" cy="12" r="2.4" fill="currentColor" stroke="none" />
                <circle cx="12" cy="12" r="6.5" />
                <path d="M12 1.8v2.4M12 19.8v2.4M1.8 12h2.4M19.8 12h2.4" />
              </svg>
              <span>Clicks</span>
            </button>
            <button
              class="tbtn wide"
              classList={{ on: showKeys() }}
              title="Show pressed shortcuts (Ctrl+C…) as chips in the GIF — plain typing is never shown"
              disabled={!hasClip()}
              onClick={() => void toggleKeys()}
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <rect x="2.5" y="6" width="19" height="12" rx="2" />
                <path d="M6.5 10h0M10.3 10h0M14.1 10h0M17.7 10h0M7.5 14h9" />
              </svg>
              <span>Keys</span>
            </button>
          </div>
          </Show>

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
              <For each={tickMarks()}>
                {(m) => (
                  <span
                    class="tl-tick"
                    classList={{ major: m.major, mid: m.mid }}
                    style={{ left: `${pct(m.t)}%` }}
                  >
                    <i />
                    <Show when={m.major}>{fmt(m.t)}</Show>
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
                        width: `${Math.max(pct(g().end) - pct(g().start), 1.6)}%`,
                      }}
                      title={`Zoom ${z.amount.toFixed(1)}× · drag to move, drag an edge to resize`}
                      onPointerDown={onZoomDown(i(), z, "move")}
                      onPointerMove={onZoomMove}
                      onPointerUp={() => void onZoomUp()}
                    >
                      <div class="tl-handle l" onPointerDown={onZoomDown(i(), z, "l")} onPointerMove={onZoomMove} onPointerUp={() => void onZoomUp()} />
                      {z.amount.toFixed(1)}×
                      <div class="tl-handle r" onPointerDown={onZoomDown(i(), z, "r")} onPointerMove={onZoomMove} onPointerUp={() => void onZoomUp()} />
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
                          width: `${Math.max(pct(g().end) - pct(g().start), 2.4)}%`,
                        }}
                        title={`${b.label} · drag to move, drag an edge to set how long it shows`}
                        onPointerDown={onAnnDown(b, "move")}
                        onPointerMove={onAnnMove}
                        onPointerUp={() => void onAnnUp()}
                      >
                        <span class="tl-handle l" onPointerDown={onAnnDown(b, "l")} onPointerMove={onAnnMove} onPointerUp={() => void onAnnUp()} />
                        {b.label}
                        <span class="tl-handle r" onPointerDown={onAnnDown(b, "r")} onPointerMove={onAnnMove} onPointerUp={() => void onAnnUp()} />
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
                      width: `${Math.max(pct(g().end) - pct(g().start), 1.2)}%`,
                    }}
                  >
                    <div class="tl-handle l" onPointerDown={onSpeedDown(i(), r, "l")} onPointerMove={onSpeedMove} onPointerUp={() => void onSpeedUp()} />
                    <button
                      class="tl-speedchip"
                      title={`Plays at ${r.factor}× · drag the chip to move, drag an edge to resize`}
                      onPointerDown={onSpeedDown(i(), r, "move")}
                      onPointerMove={onSpeedMove}
                      onPointerUp={() => void onSpeedUp()}
                    >
                      {r.factor}×
                    </button>
                    <div class="tl-handle r" onPointerDown={onSpeedDown(i(), r, "r")} onPointerMove={onSpeedMove} onPointerUp={() => void onSpeedUp()} />
                  </div>
                );
              }}
            </For>

            {/* Cut bands — sections removed from the output. Click the chip to select. */}
            <For each={cuts()}>
              {(c, i) => {
                const g = () => cutGeom(i(), c);
                return (
                  <div
                    classList={{ "tl-cutband": true, selected: selCut() === i() }}
                    style={{
                      left: `${pct(g().start)}%`,
                      width: `${Math.max(pct(g().end) - pct(g().start), 1.2)}%`,
                    }}
                  >
                    <div class="tl-handle l" onPointerDown={onCutDown(i(), c, "l")} onPointerMove={onCutMove} onPointerUp={() => void onCutUp()} />
                    <button
                      class="tl-cutchip"
                      title="Removed from the GIF · drag the chip to move, drag an edge to resize, Delete to restore"
                      onPointerDown={onCutDown(i(), c, "move")}
                      onPointerMove={onCutMove}
                      onPointerUp={() => void onCutUp()}
                    >
                      ✂
                    </button>
                    <div class="tl-handle r" onPointerDown={onCutDown(i(), c, "r")} onPointerMove={onCutMove} onPointerUp={() => void onCutUp()} />
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
          cuts={cuts()}
          onClose={() => setShowExport(false)}
          onStatus={setStatus}
          onExported={() => setDirty(false)}
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

      {/* Keyboard cheat-sheet — data-driven from SHORTCUTS so it can't drift. */}
      <Show when={showShortcuts()}>
        <div class="modal-backdrop">
          <div
            class="modal shortcuts-modal"
            ref={(el) => dialogA11y(el, "Keyboard shortcuts", () => setShowShortcuts(false))}
          >
            <h2>Keyboard shortcuts</h2>
            <div class="shortcuts-grid">
              <For each={SHORTCUTS}>
                {(g) => (
                  <section class="shortcut-group">
                    <h3 class="shortcut-group-title">{g.group}</h3>
                    <For each={g.items}>
                      {(s) => (
                        <div class="shortcut-row">
                          <span class="shortcut-label">{s.label}</span>
                          <span class="shortcut-keys">
                            <For each={s.keys}>{(k) => <kbd>{k}</kbd>}</For>
                          </span>
                        </div>
                      )}
                    </For>
                  </section>
                )}
              </For>
            </div>
            <div class="modal-actions">
              <button class="btn ghost" onClick={() => setShowShortcuts(false)}>
                Done
              </button>
            </div>
          </div>
        </div>
      </Show>

      {/* First-run welcome card. */}
      <Show when={showWelcome()}>
        <div class="modal-backdrop welcome-backdrop">
          <div class="welcome-card" ref={(el) => dialogA11y(el, "Welcome to Vuoom", () => dismissWelcome(true))}>
            <LogoWordmark />
            <h2 class="welcome-title">Record. Auto-zoom. Ship.</h2>
            <p class="welcome-sub">
              Record your screen, auto-zoom where you click, and export a crisp GIF or MP4 for
              your README, Slack, or socials.
            </p>
            <div class="welcome-steps">
              <div class="welcome-step">
                <span class="welcome-num">1</span>
                <div>
                  <strong>Record</strong>
                  <small>Pick an area and hit record. Your clicks drive the zoom.</small>
                </div>
              </div>
              <div class="welcome-step">
                <span class="welcome-num">2</span>
                <div>
                  <strong>Polish</strong>
                  <small>Trim, speed up idle time, and add text, arrows, and highlights.</small>
                </div>
              </div>
              <div class="welcome-step">
                <span class="welcome-num">3</span>
                <div>
                  <strong>Export</strong>
                  <small>One click to a GIF or MP4 you can paste anywhere.</small>
                </div>
              </div>
            </div>
            <div class="welcome-actions">
              <button class="btn welcome-skip" onClick={() => dismissWelcome(true)}>
                Skip
              </button>
              <button
                class="btn record welcome-cta"
                onClick={() => {
                  dismissWelcome(false);
                  void startRecord();
                }}
              >
                <span class="dot" /> Start recording
              </button>
            </div>
          </div>
        </div>
      </Show>

      {/* Coachmark pointing at the Record button for users who skipped the welcome. */}
      <Show when={coachRecord()}>
        <div class="coachmark" style={{ left: `${coachPos().x}px`, top: `${coachPos().y + 10}px` }}>
          <span class="coach-arrow" />
          <p>
            Click to start, or press <kbd>Ctrl+Shift+R</kbd> any time.
          </p>
          <button class="btn ghost coach-dismiss" onClick={() => setCoachRecord(false)}>
            Got it
          </button>
        </div>
      </Show>
    </div>
  );
}

export default App;
