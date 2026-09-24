// The editor store: every signal, derived getter and action behind the Vuoom editor.
// Created once by <App/> and handed to the UI through EditorContext (see context.ts), so
// the components under src/components render and dispatch without owning any state.
//
// This is the logic that used to live inline in App.tsx, moved verbatim where possible;
// DOM refs are now registered through the `refs` setters at the bottom.
import { createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { invoke, isMock, save, open, ask, check, relaunch, type Update } from "../bridge";
import { applyTheme, initialTheme } from "../themes";
import { createPreviewClient } from "../preview";
import { createAudio } from "./audio";
import { pushAudioChoice } from "../components/AudioControls";
import { pushCursorMode } from "../cursorMode";
import { toast } from "../ui";
import { createSyncSlot, createPointerFrame } from "../sync";
import { clamp01, distToSeg, v2 } from "../geometry";
import { fmtBytes, friendlyError, hexRgb } from "../format";
import { TOOL_KEYS } from "../shortcuts";
import { layout, prefs } from "../prefs";
import type {
  AnnotationSet,
  ArrowAnn,
  BoxAnn,
  ClipState,
  CropRect,
  CursorStyle,
  FrameInfo,
  DisplayInfo,
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
  ZoomStyle,
} from "../types";

export function createEditor() {
  const [tool, setTool] = createSignal<Tool>("select");
  // When locked, a drawing tool stays active after creating an element (draw several in a
  // row); when unlocked (default) we fall back to Select so the new element is editable.
  const [toolLock, setToolLock] = createSignal(false);
  // Tool-rail gestures. Single-click just arms a tool (one-shot by default); double-clicking a
  // drawing tool also turns lock on, the discoverable "draw several" gesture. Lock is a sticky
  // user preference, a single click never silently clears it.
  const pickTool = (t: Tool) => setTool(t);
  const lockTool = (t: Tool) => {
    setTool(t);
    setToolLock(true);
  };
  const [status, setStatus] = createSignal("Ready");
  const [projectName, setProjectName] = createSignal("Untitled");
  const [editingText, setEditingText] = createSignal<number | null>(null);
  const [theme, setTheme] = createSignal(initialTheme());
  const [hasClip, setHasClip] = createSignal(false);
  // True once the loaded clip has unsaved edits (annotations, zooms, trim, cuts, speed,
  // frame, click/key overlays). Drives the "discard edits?" guard before a new recording
  // replaces the clip. Set wherever an edit lands; cleared on load / save / export.
  const [dirty, setDirtyRaw] = createSignal(false);
  // Bumped on every edit; the autosave loop compares it with the last persisted version.
  let editVersion = 0;
  let persistedVersion = 0;
  const setDirty = (v: boolean) => {
    if (v) editVersion++;
    setDirtyRaw(v);
  };
  const [duration, setDuration] = createSignal(0);
  const [playhead, setPlayhead] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);
  const [looping, setLooping] = createSignal(prefs.loop());

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
  const [crop, setCrop] = createSignal<CropRect | null>(null);
  const [framePreset, setFramePreset] = createSignal("none");
  const [bgPreset, setBgPreset] = createSignal("");
  // Exact frame values (padding, corners, shadow, backdrop colors) for the fine controls.
  const [frameInfo, setFrameInfo] = createSignal<FrameInfo | null>(null);
  const [recoverable, setRecoverable] = createSignal<number | null>(null);
  const [selected, setSelected] = createSignal<Selection | null>(null);
  // Multi-selection foundation (annotations only). `selected` stays the PRIMARY / last-clicked
  // item so all single-selection code + the inspector keep working; `selExtra` holds the extra
  // members as "kind:id" keys. The full selection is primary + extras.
  const [selExtra, setSelExtra] = createSignal<Set<string>>(new Set());
  const selKey = (k: Kind, id: number) => `${k}:${id}`;
  const clearExtra = () => setSelExtra((prev) => (prev.size ? new Set<string>() : prev));
  const isSelected = (k: Kind, id: number) => {
    const s = selected();
    return (!!s && s.kind === k && s.id === id) || selExtra().has(selKey(k, id));
  };
  const selectionAll = (): Selection[] => {
    const out: Selection[] = [];
    const s = selected();
    if (s) out.push(s);
    for (const key of selExtra()) {
      const [k, idStr] = key.split(":");
      out.push({ kind: k as Kind, id: Number(idStr) });
    }
    return out;
  };
  const selCount = () => selectionAll().length;
  // Whenever the primary clears, drop the extras too, this single effect covers every
  // setSelected(null) site (Escape, inspector ✕, undo/redo resync, new recording, delete…).
  createEffect(() => {
    if (selected() === null) clearExtra();
  });
  // Shift/Ctrl-click toggle: add X (promoting it to primary, demoting the old primary to an
  // extra) or remove X (promoting an extra when X was the primary). Annotations-only.
  const toggleSelect = (kind: Kind, id: number) => {
    setSelZoom(null);
    setSelSpeed(null);
    setSelCut(null);
    const s = selected();
    const key = selKey(kind, id);
    if (s && s.kind === kind && s.id === id) {
      const extras = new Set(selExtra());
      const first = extras.values().next().value as string | undefined;
      if (first) {
        extras.delete(first);
        const [k, idStr] = first.split(":");
        setSelExtra(extras);
        setSelected({ kind: k as Kind, id: Number(idStr) });
      } else {
        setSelected(null);
      }
      return;
    }
    const extras = new Set(selExtra());
    if (extras.has(key)) {
      extras.delete(key);
      setSelExtra(extras);
      return;
    }
    if (s) extras.add(selKey(s.kind, s.id));
    setSelExtra(extras);
    setSelected({ kind, id });
  };
  const [drag, setDrag] = createSignal<Drag>(null);
  // Temp ids (negative) for optimistic creations map to their in-flight engine promise;
  // anything that must act on a REAL engine id (empty-text delete, duplicate, copy)
  // awaits this first.
  const pendingTemp = new Map<number, Promise<number>>();
  const ensureRealId = async (id: number): Promise<number> => {
    if (id >= 0) return id;
    const p = pendingTemp.get(id);
    return p ? await p.catch(() => id) : id;
  };
  const [stage, setStage] = createSignal({ w: 1, h: 1 });
  const [frameAspect, setFrameAspect] = createSignal(16 / 9);
  // Pixel width of the timeline *viewport* (the outer .tl box), drives the fit-to-width
  // scale and the adaptive ruler ticks (kept in sync via a ResizeObserver in onMount).
  const [tlWidth, setTlWidth] = createSignal(800);
  // Timeline horizontal scale. null = fit-to-width (the default: the track exactly fills
  // the viewport, every position resolves in % of duration, identical to the old layout).
  // A number is a fixed px-per-second scale; the inner track grows wider than the viewport
  // and the wrapper scrolls. Ctrl+wheel / the +/−/Fit cluster drive it.
  const [tlScale, setTlScale] = createSignal<number | null>(null);
  // While an annotation is dragged on the canvas, the normalized x/y of an active
  // center/edge snap guide (or null). Drawn as crosshair lines on the overlay.
  const [snapX, setSnapX] = createSignal<number | null>(null);
  const [snapY, setSnapY] = createSignal<number | null>(null);
  const [showExport, setShowExport] = createSignal(false);
  const [recordPhase, setRecordPhase] = createSignal<"idle" | "active">("idle");
  const [backdrop, setBackdrop] = createSignal<string | null>(null);
  // Zoom strength used by Ctrl+Shift+Z while recording, and for new zoom blocks.
  const zoomAmount = prefs.recordZoom;
  const setZoomAmount = (v: number) => prefs.recordZoom.set(v);
  // Auto-update: a pending update (if any) and whether we're mid-download.
  const [update, setUpdate] = createSignal<Update | null>(null);
  const [updating, setUpdating] = createSignal(false);
  // The GPU compositor failed at boot: preview and export are dead (recording to disk
  // still works). Drives a persistent, dismissible warning strip under the top bar.
  const [gpuLost, setGpuLost] = createSignal(false);
  // First-run onboarding: a one-time welcome card, then a coachmark pointing at Record.
  const [showWelcome, setShowWelcome] = createSignal(false);
  // Keyboard cheat-sheet modal (opened with "?").
  // Settings dialog: null = closed, else the open tab. "?" opens it on Shortcuts.
  type SettingsTab = "general" | "recording" | "editing" | "storage" | "shortcuts" | "about";
  const [settingsTab, setSettingsTab] = createSignal<SettingsTab | null>(null);
  const showShortcuts = () => settingsTab() !== null;
  const setShowShortcuts = (v: boolean | ((prev: boolean) => boolean)) => {
    const next = typeof v === "function" ? v(showShortcuts()) : v;
    setSettingsTab(next ? "shortcuts" : null);
  };
  // Command palette (Ctrl+K).
  const [showPalette, setShowPalette] = createSignal(false);
  // Any dialog that owns the keyboard: editor accelerators stand down behind it.
  const isModal = () =>
    showExport() || showWelcome() || settingsTab() !== null || showPalette() || showSource();
  // Recovery-store disk usage, shown in the shortcuts panel. `null` until first probed.
  const [recoveryBytes, setRecoveryBytes] = createSignal<number | null>(null);
  const [clearingStorage, setClearingStorage] = createSignal(false);
  const [coachRecord, setCoachRecord] = createSignal(false);
  const [coachPos, setCoachPos] = createSignal({ x: 0, y: 0 });
  let recordBtnEl: HTMLButtonElement | undefined;

  // ── recent projects (local) ─────────────────────────────────────────────────
  // A lightweight "pick up where you left off" grid for the empty state. Entries are
  // {dir, name, ts} recorded on every successful save/open, kept newest-first.
  interface Recent {
    dir: string;
    name: string;
    ts: number;
    /** Small JPEG snapshot of the project frame, captured at save time. */
    thumb?: string;
  }
  const RECENTS_KEY = "vuoom-recents";
  const [recents, setRecents] = createSignal<Recent[]>([]);
  const loadRecents = () => {
    try {
      const raw = localStorage.getItem(RECENTS_KEY);
      const list = raw ? (JSON.parse(raw) as Recent[]) : [];
      setRecents(Array.isArray(list) ? list.filter((r) => r?.dir).slice(0, 6) : []);
    } catch {
      setRecents([]);
    }
  };
  // Small preview of the current clip for the recents grid; best-effort (the canvas may
  // be hidden or tainted in odd states, in which case the grid falls back to the icon).
  const captureThumb = (): string | undefined => {
    try {
      if (!canvasEl || canvasEl.classList.contains("hidden")) return undefined;
      const off = document.createElement("canvas");
      off.width = 168;
      off.height = 94;
      const ctx = off.getContext("2d");
      if (!ctx) return undefined;
      ctx.drawImage(canvasEl, 0, 0, off.width, off.height);
      return off.toDataURL("image/jpeg", 0.6);
    } catch {
      return undefined;
    }
  };
  const rememberRecent = (dir: string, thumb?: string) => {
    const name = dir.replace(/[/]+$/, "").split(/[/]/).pop() ?? dir;
    const prev = recents().find((r) => r.dir === dir);
    const next = [
      { dir, name, ts: Date.now(), thumb: thumb ?? prev?.thumb },
      ...recents().filter((r) => r.dir !== dir),
    ].slice(0, 6);
    setRecents(next);
    try {
      localStorage.setItem(RECENTS_KEY, JSON.stringify(next));
    } catch {
      /* storage unavailable */
    }
  };
  const removeRecent = (dir: string) => {
    const next = recents().filter((r) => r.dir !== dir);
    setRecents(next);
    try {
      localStorage.setItem(RECENTS_KEY, JSON.stringify(next));
    } catch {
      /* ignore */
    }
  };
  const [recentSearch, setRecentSearch] = createSignal("");

  const openRecent = async (dir: string) => {
    setStatus("Opening project…");
    try {
      const summary = await invoke<RecordingSummary>("open_project_bundle", { dir });
      setProjectName(
        dir.replace(/[\\/]+$/, "").split(/[\\/]/).pop()?.replace(/\.vuoom$/i, "") || "Untitled",
      );
      await loadFinishedClip(summary);
      rememberRecent(dir);
      setStatus("Project opened");
      toast("Project opened", "success");
    } catch (e) {
      setStatus(`Open failed: ${String(e)}`);
      toast(`Could not open project: ${friendlyError(e)}`, "error");
      // The folder is likely gone (moved or deleted): offer to drop it from the grid.
      const remove = await ask(
        "This project folder could not be opened. It may have been moved or deleted. Remove it from the recents list?",
        { title: "Project unavailable", kind: "warning", okLabel: "Remove", cancelLabel: "Keep" },
      );
      if (remove) removeRecent(dir);
    }
  };
  const fmtAgo = (ts: number) => {
    const mins = Math.round((Date.now() - ts) / 60000);
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins} min ago`;
    const hrs = Math.round(mins / 60);
    if (hrs < 24) return `${hrs} hr ago`;
    return `${Math.round(hrs / 24)} days ago`;
  };

  // ── timeline hover affordances ───────────────────────────────────────────────
  // A ghost playhead tracks the pointer across the whole track, and the zoom row shows a
  // ghost block with a + under the cursor; a plain click (not a drag-scrub) adds the zoom.
  // Click detection lives on the .tl pointer pair because pointer capture retargets
  // pointerup to the capturing element, so a child's click handler would never fire.
  // ALL pointermove work runs once per animation frame (createPointerFrame) and reads a
  // cached track rect: hovering used to hit-test + sort on every single pointer event.
  const [hoverT, setHoverT] = createSignal<number | null>(null);
  const [ghostT, setGhostT] = createSignal<number | null>(null);
  let tlDownX = -1;
  let tlDownY = -1;
  let tlDownInZoomLane = false;
  const onTlHoverMove = (e: PointerEvent) => {
    if (tlDrag || zoomDrag() || speedDrag() || cutDrag() || !hasClip()) {
      setHoverT(null);
      setGhostT(null);
      return;
    }
    const t = tlTime(e);
    setHoverT(t);
    setGhostT((e.target as Element).closest(".tl-zoomlane") ? t : null);
  };
  const onTlHoverLeave = () => {
    setHoverT(null);
    setGhostT(null);
  };
  const onTlPointerMove = (e: PointerEvent) => {
    onTlHoverMove(e);
    if (tlDrag) tlSeekFromEvent(e);
  };
  // One frame-runner per gesture owner: events within a frame coalesce to the latest.
  const frameTl = createPointerFrame();
  const frameZoom = createPointerFrame();
  const frameSpeed = createPointerFrame();
  const frameCut = createPointerFrame();
  const frameAnn = createPointerFrame();
  const frameTrim = createPointerFrame();
  const frameCanvas = createPointerFrame();

  const preview = createPreviewClient();
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
    // they can't mutate the clip behind it, but still swallow browser chords further down.
    const modalOpen = isModal();
    // Undo / redo (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y), inputs keep their native undo.
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
      // Ctrl+C / Ctrl+V, copy the annotation selection, paste at the playhead. Both no-op
      // silently when there's nothing to act on: Ctrl+C with no annotation selected (a
      // zoom/speed/cut selection leaves selected() null) and Ctrl+V with an empty clipboard
      // both fall through WITHOUT preventDefault, so they never steal native copy/paste in a
      // text field (guarded by !inField) or hijack the event from a non-annotation selection.
      if (e.code === "KeyC" && !inField && hasClip() && selected()) {
        if (copySelected()) {
          e.preventDefault();
          return;
        }
      }
      if (e.code === "KeyV" && !inField && hasClip() && clipboard().length > 0) {
        e.preventDefault();
        void pasteClipboard();
        return;
      }
    }
    // Z-order for the primary selected annotation: Ctrl+] / Ctrl+[ nudge one step,
    // Ctrl+Shift+] / Ctrl+Shift+[ jump to front / back. Guarded like the other editor
    // accelerators (no modal, not in a field, an annotation selected).
    if (!modalOpen && e.ctrlKey && !e.altKey && !inField && hasClip() && selected()) {
      if (e.code === "BracketRight") {
        e.preventDefault();
        void reorderSelected(e.shiftKey ? "front" : "forward");
        return;
      }
      if (e.code === "BracketLeft") {
        e.preventDefault();
        void reorderSelected(e.shiftKey ? "back" : "backward");
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
    // Ctrl+wheel is browser page-zoom, meaningless in a desktop editor.
    if (e.ctrlKey) e.preventDefault();
  };

  onMount(async () => {
    applyTheme(theme());
    document.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("keydown", onGlobalKey, true);
    window.addEventListener("wheel", onWheelGuard, { passive: false });
    window.addEventListener("keydown", onKey);
    preview.onAspectChange((a) => setFrameAspect(a));
    await connectEngine();
    loadRecents();
    void checkForUpdate();
    // Browser-mock helpers for screenshots/tests: ?demo=1 loads the sample take,
    // &export=1 opens the export card, &record=1 jumps into the region selector.
    const mockParams = new URLSearchParams(window.location.search);
    if (isMock && mockParams.has("demo")) {
      try {
        localStorage.setItem("vuoom-seen-welcome", "1");
      } catch {
        /* ignore */
      }
      void invoke("set_pref", { key: "seen_welcome", value: "1" }).catch(() => undefined);
      setShowWelcome(false);
      setCoachRecord(false);
      try {
        const summary = await invoke<RecordingSummary>("recover_session");
        setRecoverable(null);
        await loadFinishedClip(summary);
      } catch (e) {
        console.warn("[mock] demo load failed", e);
      }
      if (mockParams.has("export")) setShowExport(true);
      if (mockParams.has("record")) void startRecord();
    }
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
        const conn = await invoke<{ port: number; token: string }>("preview_port");
        preview.connect(conn.port, conn.token);
        setStatus("Ready. Press Record to start.");
        hideSplash();
        void maybeShowWelcome();
        // One-shot health probe: a failed GPU compositor still "boots", but preview and
        // export are dead, warn up front instead of every operation failing cryptically.
        invoke<{ gpu: boolean }>("engine_health")
          .then((h) => setGpuLost(!h.gpu))
          .catch(() => setGpuLost(false));
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
    setStatus("The engine did not start. Try restarting Vuoom.");
    hideSplash();
  };

  // ── auto-update (signed GitHub releases) ───────────────────────────────────────
  // Check once on launch; surfaces an "Update" pill in the top bar if one is ready.
  const checkForUpdate = async () => {
    try {
      const u = await check();
      if (u) setUpdate(u);
    } catch {
      /* updater not configured (dev) or offline, silently ignore */
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
          setStatus("Update downloaded. Restarting…");
        }
      });
      await relaunch();
    } catch (e) {
      setUpdating(false);
      setStatus(`Update failed: ${String(e)}`);
    }
  };

  // ── first-run onboarding ───────────────────────────────────────────────────────
  // The disk-backed pref is the durable source of truth (localStorage doesn't survive an app
  // restart on every machine); localStorage is only a same-session fast-path cache. The card
  // shows only when NEITHER store has recorded a dismissal, so once dismissed it never returns.
  const maybeShowWelcome = async () => {
    // Screenshot/test mode: the mock can suppress the first-run card entirely.
    if (isMock && new URLSearchParams(window.location.search).has("nowelcome")) {
      try {
        localStorage.setItem("vuoom-seen-welcome", "1");
      } catch {
        /* ignore */
      }
      return;
    }
    let seen = false;
    try {
      seen = !!localStorage.getItem("vuoom-seen-welcome");
    } catch {
      /* storage unavailable */
    }
    try {
      const pref = await invoke<string | null>("get_pref", { key: "seen_welcome" });
      seen = seen || !!pref;
    } catch {
      /* pref store unavailable, fall back to the cache result */
    }
    if (!seen) setShowWelcome(true);
  };
  // Dismiss the welcome card; `hint` pops a coachmark pointing at Record for skippers. Persist
  // to BOTH the disk pref (durable) and localStorage (fast cache) so it stays dismissed.
  const dismissWelcome = (hint: boolean) => {
    try {
      localStorage.setItem("vuoom-seen-welcome", "1");
    } catch {
      /* ignore */
    }
    void invoke("set_pref", { key: "seen_welcome", value: "1" }).catch(() => {
      /* pref store unavailable, localStorage cache still covers this session */
    });
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

  // Reconcile a fresh engine snapshot into the model: an item whose snapshot is UNCHANGED
  // keeps its object reference (Solid <For> keeps that DOM row), a CHANGED item takes the
  // fresh object (so its row re-renders), new ids append, missing ids drop. Result is
  // ordered like the engine's list.
  const reconcileAnns = (next: AnnotationSet) => {
    const cur = anns();
    const merge = <T extends { id: number }>(curList: T[], nextList: T[]): T[] =>
      nextList.map((nu) => {
        const old = curList.find((x) => x.id === nu.id);
        return old && JSON.stringify(old) === JSON.stringify(nu) ? old : nu;
      });
    setAnns({
      texts: merge(cur.texts, next.texts),
      arrows: merge(cur.arrows, next.arrows),
      highlights: merge(cur.highlights, next.highlights),
    });
  };
  const refresh = async () => {
    try {
      reconcileAnns(await invoke<AnnotationSet>("list_annotations"));
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
      setCrop(cs.crop);
      setFramePreset(cs.frame_preset);
      setBgPreset(cs.background_preset);
      setFrameInfo(cs.frame ?? null);
      audio.adopt(cs.audio ?? []);
      setCursorStyle(cs.cursor ?? null);
      setPointerCaptured(cs.pointer_captured ?? true);
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

  // Recorded audio follows the playhead (see editor/audio.ts); any stop silences it.
  const audio = createAudio({ onEdit: () => setDirty(true) });
  createEffect(() => {
    if (!playing()) audio.stop();
  });

  let raf = 0;
  let lastTs = 0;
  const tick = (ts: number) => {
    if (!playing()) return;
    if (lastTs) {
      let t = playhead() + ((ts - lastTs) / 1000) * factorAt(playhead()) * prefs.previewRate();
      // Cut sections are removed from the output, playback jumps over them.
      const cut = cuts().find((c) => t >= c.start && t < c.end);
      if (cut) t = cut.end;
      if (t >= tEnd()) {
        // GIFs loop, with Loop on, the preview does too.
        if (looping()) {
          t = tStart();
        } else {
          t = tEnd();
          setPlaying(false);
        }
      }
      setPlayhead(t);
      void pushSeek(t);
      audio.sync(playing(), t, factorAt(t), prefs.previewRate());
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
      audio.prime();
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
    patchAnn(s.kind, s.id, (a) => {
      if (s.kind === "arrow") {
        (a as ArrowAnn).from = [g[0], g[1]];
        (a as ArrowAnn).to = [g[2], g[3]];
      } else if (s.kind === "box") {
        (a as BoxAnn).rect = { x: g[0], y: g[1], w: g[2], h: g[3] };
      } else {
        (a as TextAnn).pos = [g[0], g[1]];
      }
    });
    try {
      await applyGeom(s.kind, s.id, g);
      await refresh();
      await pushSeek(playhead());
    } catch (e) {
      await refresh();
      toast(`Move failed: ${friendlyError(e)}`, "error");
    }
  };

  const onKey = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest("input, textarea")) return;
    // "?" toggles the keyboard cheat-sheet (Shift+/), available any time except behind another modal.
    if (e.key === "?" && !showExport() && !showWelcome() && !showPalette()) {
      e.preventDefault();
      setShowShortcuts((v) => !v);
      return;
    }
    // A modal owns the screen, its own handler deals with Esc/Tab; don't drive the editor behind it.
    if (isModal()) return;
    // The crop editor owns the keyboard: Enter applies, Esc cancels.
    if (cropEdit()) {
      if (e.key === "Enter" || e.key === "Escape") {
        e.preventDefault();
        void finishCropEdit(e.key === "Enter");
      }
      return;
    }
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
      void deleteSelection();
    } else if (
      e.key === "Escape" &&
      (selected() ||
        selZoom() !== null ||
        selSpeed() !== null ||
        selCut() !== null ||
        tool() !== "select")
    ) {
      // One key clears everything: disarm the current drawing tool (back to Select) AND
      // drop any selection. Whatever state the user is in, Escape returns them to neutral.
      if (tool() !== "select") setTool("select");
      setSelected(null);
      setSelZoom(null);
      setSelSpeed(null);
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
    } else if ((e.code === "Comma" || e.code === "Period") && hasClip() && !e.ctrlKey && !e.altKey) {
      // Frame step (1/30 s; Shift = 10 frames), pausing playback like any precise seek.
      e.preventDefault();
      if (playing()) togglePlay();
      const step = (e.shiftKey ? 10 : 1) / 30;
      scrub(Math.min(Math.max(playhead() + (e.code === "Period" ? step : -step), tStart()), tEnd()));
    } else if (e.key === "Home" && hasClip()) {
      e.preventDefault();
      scrub(tStart());
    } else if (e.key === "End" && hasClip()) {
      e.preventDefault();
      scrub(tEnd());
    } else if (hasClip() && !e.ctrlKey && !e.altKey && !e.metaKey && e.code === "KeyG" && editingText() === null) {
      // G toggles the composition guides over the stage.
      e.preventDefault();
      layout.guides.set(!layout.guides());
    } else if (
      hasClip() &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      editingText() === null &&
      (e.code === "KeyI" || e.code === "KeyO")
    ) {
      // I / O set the trim in / out point at the playhead (Shift clears that side).
      e.preventDefault();
      if (e.code === "KeyI") void commitTrim(e.shiftKey ? 0 : playhead(), tEnd());
      else void commitTrim(tStart(), e.shiftKey ? duration() : playhead());
    } else if (
      hasClip() &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      !e.shiftKey &&
      editingText() === null &&
      (e.code === "KeyZ" || e.code === "KeyX" || e.code === "KeyC")
    ) {
      // Insert a segment at the playhead, Z/X/C mirror the Insert group (Zoom/Speed/Cut).
      e.preventDefault();
      if (e.code === "KeyZ") void addZoomAt();
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
      // Single-key tool switching (V/T/A/L/S/H), matches the badges on the tool rail.
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
  // stays editable when scrubbed past its window), but never during playback, which
  // must match the exported GIF exactly.
  const inWindow = (r: TimeRange) => playhead() >= r.start && playhead() < r.end;
  const inView = (r: TimeRange, sel: boolean) => inWindow(r) || (sel && !playing());
  // Selected but outside its window → drawn ghosted, so it's obvious the element is NOT
  // visible at this moment (it's only on screen to stay editable).
  const isGhost = (r: TimeRange, sel: boolean) => sel && !playing() && !inWindow(r);

  // ── live edit queue (optimistic + keyed coalescing) ───────────────────────────
  // pushEdit(key, patch, command):
  //   1. patch() runs SYNCHRONOUSLY against the local model, so the UI reacts this frame.
  //   2. command() is queued under `key`. A newer push for the same key replaces the
  //      queued command (latest value wins); pushes for OTHER keys are never dropped.
  //   3. The loop drains everything queued, then re-composites ONCE via the shared seek
  //      scheduler, so scrubbing stays smooth under a storm of slider/scrub input.
  //   4. A failed command re-syncs from the engine (which still holds the old value) and
  //      surfaces a toast; later commands for other keys proceed untouched.
  const editQueues = new Map<string, () => Promise<void>>();
  let editsBusy = false;
  const pushEdit = (key: string, patch: () => void, command: () => Promise<void>) => {
    patch();
    setDirty(true); // live property / geometry / text edits flow through here
    editQueues.set(key, command);
    if (editsBusy) return;
    editsBusy = true;
    void (async () => {
      while (editQueues.size > 0) {
        const entries = [...editQueues.values()];
        editQueues.clear();
        for (const cmd of entries) {
          try {
            await cmd();
          } catch (e) {
            await refresh(); // engine truth undoes the optimistic patch
            toast(`Edit failed: ${friendlyError(e)}`, "error");
          }
        }
        await pushSeek(playhead());
      }
      editsBusy = false;
    })();
  };

  // Optimistic local model patch. The patched annotation is REPLACED with a new object
  // (clone + mutate): Solid's <For> diffs by reference and plain object properties are not
  // reactive, so an in-place mutation would never re-render the canvas label. Untouched
  // annotations keep their references, so their timeline rows stay put.
  const patchAnn = (kind: Kind, id: number, mut: (a: TextAnn | ArrowAnn | BoxAnn) => void) => {
    const cur = anns();
    const swap = <T extends { id: number }>(list: T[]): T[] =>
      list.map((x) => {
        if (x.id !== id) return x;
        const clone = structuredClone(x);
        mut(clone as unknown as TextAnn | ArrowAnn | BoxAnn);
        return clone;
      });
    setAnns(
      kind === "text"
        ? { texts: swap(cur.texts), arrows: cur.arrows, highlights: cur.highlights }
        : kind === "arrow"
          ? { texts: cur.texts, arrows: swap(cur.arrows), highlights: cur.highlights }
          : { texts: cur.texts, arrows: cur.arrows, highlights: swap(cur.highlights) },
    );
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
    if (d && d.mode === "move" && d.group) {
      const m = d.group.find((x) => x.kind === kind && x.id === id);
      if (m) return m.geom;
    }
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
    try { (e.currentTarget as Element).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
    const p = norm(e);
    const t = tool();

    if (t === "text") {
      // Optimistic creation: the label appears, is selected, and its inline editor opens
      // THIS frame under a negative temp id. The engine call runs in the background and
      // the temp id is remapped to the real one when it resolves. Any action that needs
      // the real id (empty-text delete, duplicate, copy) awaits `ensureRealId` first.
      const tempId = -Date.now() - Math.floor(Math.random() * 1e6);
      const t0 = playhead();
      const cur = anns();
      const temp: TextAnn = {
        id: tempId,
        text: "Text",
        pos: [p.x, p.y],
        font_size: 0.05,
        color: { r: 255, g: 255, b: 255, a: 1 },
        bold: false,
        italic: false,
        background: false,
        font: "",
        range: {
          start: Math.max(0, t0 - 0.2),
          end: Math.min(duration(), t0 + 2.8),
          fade_in: 0.15,
          fade_out: 0.25,
        },
      };
      setAnns({ ...cur, texts: [...cur.texts, temp] });
      setDirty(true);
      setSelZoom(null);
      setSelSpeed(null);
      setSelCut(null);
      clearExtra();
      setSelected({ kind: "text", id: tempId });
      // The inline editor must NOT open during pointerdown: the compatibility mousedown
      // that fires right after this listener would pull focus (its default action) and
      // blur the freshly mounted input, closing it instantly. This is the same focus race
      // documented for double-click inline editing; mounting on pointerup sidesteps it.
      // `resolvedId` tracks the temp-to-real remap so a fast engine response (which
      // re-points the selection BEFORE the user releases the button) still opens.
      let resolvedId = tempId;
      window.addEventListener(
        "pointerup",
        () => {
          const sid = selected()?.id;
          if (sid === tempId || sid === resolvedId) setEditingText(sid);
        },
        { once: true },
      );
      if (!toolLock()) setTool("select");
      const creation = (async () => {
        const realId = await invoke<number>("add_text", { text: "Text", x: p.x, y: p.y, t: t0 });
        resolvedId = realId;
        const cs = anns();
        const t = cs.texts.find((x) => x.id === tempId);
        if (t) t.id = realId;
        setAnns({ texts: [...cs.texts], arrows: [...cs.arrows], highlights: [...cs.highlights] });
        if (selected()?.id === tempId) setSelected({ kind: "text", id: realId });
        if (editingText() === tempId) setEditingText(realId);
        return realId;
      })();
      pendingTemp.set(tempId, creation);
      void creation
        .then(async (realId) => {
          pendingTemp.delete(tempId);
          await refresh();
          await pushSeek(playhead());
          if (selected()?.id === realId) setEditingText(realId);
        })
        .catch(async (e) => {
          pendingTemp.delete(tempId);
          const cs = anns();
          setAnns({ ...cs, texts: cs.texts.filter((x) => x.id !== tempId) });
          if (selected()?.id === tempId) setSelected(null);
          if (editingText() === tempId) setEditingText(null);
          toast(`Could not add text: ${friendlyError(e)}`, "error");
        });
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
    if (t === "mask") {
      setDrag({ mode: "create-mask", start: p, cur: p });
      return;
    }

    // Second click of a double-click on a text label → inline edit. Detected here via
    // e.detail because pointer capture can swallow the synthesized dblclick event.
    if (e.detail >= 2) {
      const hit = hitTest(p);
      if (hit?.kind === "text") {
        setDrag(null);
        // Release the pointer capture grabbed at the top of this handler so the pending
        // pointerup resolves normally on the window.
        try {
          (e.currentTarget as Element).releasePointerCapture(e.pointerId);
        } catch {
          /* capture already released */
        }
        setSelected(hit);
        // Open the inline editor on the double-click's RELEASE, not here on its press.
        //
        // Verified root cause of the "editor opens then instantly closes / now nothing shows"
        // regression: the second click dispatches `pointerdown` → (microtask checkpoint) →
        // `mousedown` → `pointerup` → `mouseup` → `click` → `dblclick`. If we `setEditingText`
        // here, Solid mounts the <input> and the ref focuses it in the microtask that runs
        // immediately after THIS pointerdown listener, i.e. BEFORE the compatibility
        // `mousedown`. `mousedown`'s (uncancelled) default action then runs the HTML focusing
        // steps on the non-focusable overlay, pulling focus off the freshly-focused input; its
        // onBlur fires, `finishTextEdit()` commits + unmounts it, and the editor vanishes in the
        // same frame. Releasing pointer capture alone never helped because the focus theft is the
        // `mousedown` default action, not the capture. (The Text-TOOL create path is immune only
        // because its `setEditingText` runs after an awaited `invoke`, i.e. after every mouse
        // event of that click has already fired.)
        //
        // The focus-changing default action is bound to `mousedown` (press). Deferring the mount
        // to the gesture's `pointerup` (release) means we mount + focus AFTER `mousedown` has
        // passed; the trailing `mouseup`/`click`/`dblclick` carry no focus default, so the input
        // keeps focus until the user blurs / presses Enter / Esc. This is event-driven (no timer).
        const id = hit.id;
        window.addEventListener("pointerup", () => setEditingText(id), { once: true });
        return;
      }
    }

    // select tool: handle → resize, body → move, empty → deselect (see the fall-through below)
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
      // Shift/Ctrl-click toggles the item in/out of the multi-selection (no drag).
      if (e.shiftKey || e.ctrlKey || e.metaKey) {
        toggleSelect(hit.kind, hit.id);
        return;
      }
      // Plain click on a NEW / lone item replaces the selection; plain click on a member of an
      // existing multi-selection keeps the whole group so the drag below moves all of it.
      if (!isSelected(hit.kind, hit.id) || selCount() <= 1) {
        setSelZoom(null);
        setSelSpeed(null);
        setSelCut(null);
        clearExtra();
        setSelected(hit);
      }
      const group = selectionAll()
        .filter((s) => !(s.kind === hit.kind && s.id === hit.id))
        .map((s) => {
          const gg = geomOf(s.kind, s.id);
          return { kind: s.kind, id: s.id, orig: gg, geom: gg.slice() };
        });
      const g = geomOf(hit.kind, hit.id);
      setDrag({ mode: "move", kind: hit.kind, id: hit.id, grab: p, orig: g, geom: g.slice(), group });
      return;
    }
    // Clicking empty canvas deselects, the trivial "get me out of this" gesture users expect
    // (Figma/Excalidraw). The inspector column stays reserved while a clip is loaded (it falls
    // back to a hint), so clearing the selection never reflows the canvas.
    setSelected(null);
    setSelZoom(null);
    setSelSpeed(null);
    setSelCut(null);
    clearExtra();
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
      d.mode === "create-highlight" ||
      d.mode === "create-mask"
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
      // Translate the rest of the multi-selection by the SAME net delta the primary took
      // (post-snap), so the group moves rigidly and snapping keys off the primary alone.
      let group = d.group;
      if (group?.length) {
        const ndx = g[0] - og[0];
        const ndy = g[1] - og[1];
        group = group.map((m) => {
          const mo = m.orig;
          let mg: number[];
          if (m.kind === "box") mg = [clamp01(mo[0] + ndx), clamp01(mo[1] + ndy), mo[2], mo[3]];
          else if (m.kind === "arrow")
            mg = [clamp01(mo[0] + ndx), clamp01(mo[1] + ndy), clamp01(mo[2] + ndx), clamp01(mo[3] + ndy)];
          else mg = [clamp01(mo[0] + ndx), clamp01(mo[1] + ndy)];
          return { ...m, geom: mg };
        });
      }
      setDrag({ ...d, geom: g, group });
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
        clearExtra();
        setSelected({ kind: "arrow", id });
        if (!toolLock()) setTool("select");
      }
    } else if (
      d.mode === "create-box" ||
      d.mode === "create-ellipse" ||
      d.mode === "create-highlight" ||
      d.mode === "create-mask"
    ) {
      const cmd =
        d.mode === "create-box"
          ? "add_box"
          : d.mode === "create-ellipse"
            ? "add_ellipse"
            : d.mode === "create-mask"
              ? "add_mask"
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
        clearExtra();
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
      // Commit every other group member (per-item backend commands → per-item geo: undo tags).
      if (d.mode === "move" && d.group) {
        for (const m of d.group) await applyGeom(m.kind, m.id, m.geom);
      }
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
  // every pointer-move or keystroke, so they run through pushEdit, the same edit throttle
  // the inline text editor uses, to bound the invoke→refresh→seek round-trips. pushEdit
  // appends the seek and always lets the trailing value land, so the drag-end value sticks.
  const editStyle = (patch: { thickness?: number; filled?: boolean }) => {
    const s = selected();
    if (!s) return;
    const { id, kind } = s;
    pushEdit(
      `sty:${id}`,
      () =>
        patchAnn(kind, id, (a) => {
          if (patch.thickness !== undefined) (a as ArrowAnn).thickness = patch.thickness;
          if (patch.filled !== undefined && kind === "box") (a as BoxAnn).filled = patch.filled;
        }),
      async () => {
        await invoke("set_annotation_style", { id, ...patch });
        await refresh();
      },
    );
  };
  const setShape = (ellipse: boolean) => {
    const s = selected();
    if (s?.kind !== "box") return;
    const { id } = s;
    patchAnn("box", id, (a) => (a as BoxAnn).shape = ellipse ? "Ellipse" : "Rect");
    void (async () => {
      try {
        await invoke("set_highlight_shape", { id, ellipse });
        await refresh();
        await pushSeek(playhead());
      } catch (e) {
        await refresh();
        toast(`Shape failed: ${friendlyError(e)}`, "error");
      }
    })();
  };
  const setArrowStyle = (style: "arrow" | "line" | "double") => {
    const s = selected();
    if (s?.kind !== "arrow") return;
    const { id } = s;
    patchAnn("arrow", id, (a) => {
      (a as ArrowAnn).style = style === "arrow" ? "Arrow" : style === "line" ? "Line" : "DoubleArrow";
    });
    void (async () => {
      try {
        await invoke("set_arrow_style", { id, style });
        await refresh();
        await pushSeek(playhead());
      } catch (e) {
        await refresh();
        toast(`Style failed: ${friendlyError(e)}`, "error");
      }
    })();
  };
  const setOpacity = (a: number) => {
    const s = selected();
    if (!s) return;
    const { id, kind } = s;
    pushEdit(
      `opa:${id}`,
      () =>
        patchAnn(kind, id, (ann) => {
          const c = (ann as TextAnn).color;
          (ann as TextAnn).color = { ...c, a };
        }),
      async () => {
        await invoke("set_annotation_opacity", { id, a });
        await refresh();
      },
    );
  };
  const isMask = () => selectedBox()?.shape === "Mask";
  const inspTitle = () => {
    const s = selected()!;
    if (s.kind === "box") {
      const b = selectedBox();
      if (b?.shape === "Mask") return "Mask";
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
  const setColor = (hex: string) => {
    const s = selected();
    if (!s) return;
    const { id, kind } = s;
    const c = hexRgb(hex);
    pushEdit(
      `col:${id}`,
      () =>
        patchAnn(kind, id, (ann) => {
          (ann as TextAnn).color = { ...(ann as TextAnn).color, ...c };
        }),
      async () => {
        await invoke("set_annotation_color", { id, r: c.r, g: c.g, b: c.b });
        await refresh();
      },
    );
  };
  const editText = (text: string) => {
    const s = selected();
    if (s?.kind !== "text") return;
    const { id } = s;
    pushEdit(
      `text:${id}`,
      () => patchAnn("text", id, (a) => (a as TextAnn).text = text),
      async () => {
        await invoke("update_text", { id, text });
        await refresh();
      },
    );
  };
  const editFontSize = (size: number) => {
    const s = selected();
    if (s?.kind !== "text") return;
    const { id } = s;
    pushEdit(
      `fs:${id}`,
      () => patchAnn("text", id, (a) => (a as TextAnn).font_size = size),
      async () => {
        await invoke("update_text", { id, fontSize: size });
        await refresh();
      },
    );
  };
  const editTextStyle = (patch: {
    bold?: boolean;
    italic?: boolean;
    background?: boolean;
    font?: string;
  }) => {
    const s = selected();
    if (s?.kind !== "text") return;
    const { id } = s;
    const field = Object.keys(patch)[0] ?? "style";
    pushEdit(
      `tstyle:${id}:${field}`,
      () => patchAnn("text", id, (a) => Object.assign(a, patch)),
      async () => {
        await invoke("update_text", { id, ...patch });
        await refresh();
        await pushSeek(playhead());
      },
    );
  };
  const selectedRange = (): TimeRange | undefined => {
    const s = selected();
    if (!s) return undefined;
    if (s.kind === "text") return anns().texts.find((t) => t.id === s.id)?.range;
    if (s.kind === "arrow") return anns().arrows.find((a) => a.id === s.id)?.range;
    return anns().highlights.find((b) => b.id === s.id)?.range;
  };
  const editRange = (start: number, end: number) => {
    const s = selected();
    if (!s || Number.isNaN(start) || Number.isNaN(end)) return;
    const { id, kind } = s;
    pushEdit(
      `range:${id}`,
      () =>
        patchAnn(kind, id, (a) => {
          const r = (a as TextAnn).range;
          (a as TextAnn).range = { ...r, start, end };
        }),
      async () => {
        await invoke("update_annotation_range", { id, start, end });
        await refresh();
        await pushSeek(playhead());
      },
    );
  };
  const editFades = (fadeIn: number, fadeOut: number) => {
    const s = selected();
    if (!s || Number.isNaN(fadeIn) || Number.isNaN(fadeOut)) return;
    const { id, kind } = s;
    pushEdit(
      `fades:${id}`,
      () =>
        patchAnn(kind, id, (a) => {
          const r = (a as TextAnn).range;
          (a as TextAnn).range = { ...r, fade_in: fadeIn, fade_out: fadeOut };
        }),
      async () => {
        await invoke("set_annotation_fades", { id, fadeIn, fadeOut });
        await refresh();
        await pushSeek(playhead());
      },
    );
  };
  // Delete the whole selection (primary + extras). A lone delete keeps today's behaviour
  // (empty, non-coalescing undo tag). A group delete passes ONE shared non-empty tag for the
  // run so the backend's snapshot() coalesces every removal into a single undo step.
  let delGesture = 0;
  const deleteSelection = async () => {
    const all = selectionAll();
    if (all.length === 0) return;
    // Temp (optimistic) ids must become real before the engine can delete them.
    const resolved: Selection[] = [];
    for (const it of all) resolved.push({ kind: it.kind, id: await ensureRealId(it.id) });
    if (resolved.length === 1) {
      await invoke("delete_annotation", { id: resolved[0].id });
    } else {
      const tag = `multidel:${++delGesture}`;
      for (const it of resolved) await invoke("delete_annotation", { id: it.id, tag });
    }
    setSelected(null);
    await refresh();
    await pushSeek(playhead());
  };
  // ── undo / redo ────────────────────────────────────────────────────────────────
  const refreshAll = async () => {
    // An undo can change anything, clear selections that may now dangle, resync all.
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
    const realId = await ensureRealId(s.id);
    if (realId !== s.id) setSelected({ kind: s.kind, id: realId });
    try {
      const id = await invoke<number>("duplicate_annotation", { id: s.id });
      await refresh();
      await pushSeek(playhead());
      clearExtra();
      setSelected({ kind: s.kind, id });
      setStatus("Duplicated. Drag the copy into place.");
    } catch (e) {
      setStatus(`Duplicate failed: ${String(e)}`);
    }
  };

  // Change the stacking order of the primary selected annotation within its own type. Stacking
  // is per-type (highlights below arrows below texts), so this only reorders relative to items
  // of the same kind. dir: "forward" | "backward" | "front" | "back".
  const reorderSelected = async (dir: "forward" | "backward" | "front" | "back") => {
    const s = selected();
    if (!s) return;
    try {
      await invoke("reorder_annotation", { id: s.id, dir });
      await refresh();
      await pushSeek(playhead());
      setStatus(
        {
          forward: "Brought forward",
          backward: "Sent backward",
          front: "Brought to front",
          back: "Sent to back",
        }[dir],
      );
    } catch (e) {
      setStatus(`Reorder failed: ${String(e)}`);
    }
  };

  // ── copy / paste ─────────────────────────────────────────────────────────────────
  // A frontend-only clipboard of deep-copied annotation snapshots (geometry + style + their
  // absolute time windows). It never touches the OS clipboard, and, being self-contained,
  // survives the originals being moved or deleted, and can be pasted repeatedly.
  type ClipItem =
    | ({ kind: "text" } & TextAnn)
    | ({ kind: "arrow" } & ArrowAnn)
    | ({ kind: "box" } & BoxAnn);
  const [clipboard, setClipboard] = createSignal<ClipItem[]>([]);
  // Snapshot the current annotation selection into the clipboard. Returns whether anything was
  // captured so the caller only swallows Ctrl+C when there was a selection to copy.
  const copySelected = (): boolean => {
    const all = selectionAll();
    if (all.length === 0 || all.some((x) => x.id < 0)) return false;
    const items: ClipItem[] = [];
    for (const sel of all) {
      if (sel.kind === "text") {
        const a = anns().texts.find((t) => t.id === sel.id);
        if (a) items.push({ kind: "text", ...structuredClone(a) });
      } else if (sel.kind === "arrow") {
        const a = anns().arrows.find((x) => x.id === sel.id);
        if (a) items.push({ kind: "arrow", ...structuredClone(a) });
      } else {
        const a = anns().highlights.find((b) => b.id === sel.id);
        if (a) items.push({ kind: "box", ...structuredClone(a) });
      }
    }
    if (items.length === 0) return false;
    setClipboard(items);
    setStatus(`Copied ${items.length} annotation${items.length > 1 ? "s" : ""}`);
    return true;
  };
  // Paste the clipboard at the playhead. The backend re-anchors the set so its earliest item
  // starts at the playhead (relative offsets + durations preserved), assigns fresh ids in one
  // undo step, and hands back the new (kind,id) refs so we can select them (primary = first).
  const pasteClipboard = async () => {
    const items = clipboard();
    if (items.length === 0) return;
    try {
      const refs = await invoke<{ kind: Kind; id: number }[]>("paste_annotations", {
        items,
        at: playhead(),
      });
      await refresh();
      await pushSeek(playhead());
      setSelZoom(null);
      setSelSpeed(null);
      setSelCut(null);
      clearExtra();
      if (refs.length > 0) {
        setSelected({ kind: refs[0].kind, id: refs[0].id });
        const extras = new Set<string>();
        for (let i = 1; i < refs.length; i++) extras.add(selKey(refs[i].kind, refs[i].id));
        setSelExtra(extras);
      }
      setStatus(`Pasted ${refs.length} annotation${refs.length > 1 ? "s" : ""}`);
    } catch (e) {
      setStatus(`Paste failed: ${String(e)}`);
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
    pushEdit(
      `text:${id}`,
      () => patchAnn("text", id, (a) => (a as TextAnn).text = text),
      async () => {
        await invoke("update_text", { id, text });
      },
    );
  };
  const finishTextEdit = async () => {
    const picked = editingText();
    setEditingText(null);
    if (picked === null) return;
    const id = await ensureRealId(picked);
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
  // this window, the window is excluded from the capture and grown/shrunk by the backend,
  // so the overlay never lands in the recording and we avoid fragile extra webviews.
  // ── record source: display picker + window capture ──────────────────────────────
  // A single display records directly (the old behavior); with several attached, a
  // chooser offers displays AND app windows before the region overlay appears.
  type RecordTarget =
    | { kind: "display"; name: string; label: string }
    | { kind: "window"; hwnd: number; label: string };
  const [showSource, setShowSource] = createSignal(false);
  const [sources, setSources] = createSignal<{
    displays: DisplayInfo[];
    windows: { hwnd: number; title: string; w: number; h: number }[];
  }>({ displays: [], windows: [] });

  // How the next take is framed: the whole display, a region the user draws, or one app
  // window. Home's source cards and the File menu pick it; Ctrl+Shift+R reuses the last.
  type RecordMode = "full" | "region" | "window";
  const [recordMode, setRecordMode] = createSignal<RecordMode>("region");
  const [sourceTab, setSourceTab] = createSignal<"display" | "window">("display");

  const startRecord = async (mode: RecordMode = recordMode()) => {
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
    setRecordMode(mode);
    let displays: DisplayInfo[] = [];
    try {
      displays = await invoke<DisplayInfo[]>("list_displays");
    } catch (e) {
      // No display enumeration (older backend): fall straight through to the editor's monitor.
      setStatus(`Falling back to the editor's display: ${String(e)}`);
    }
    let windows: { hwnd: number; title: string; w: number; h: number }[] = [];
    if (mode === "window" || displays.length > 1) {
      try {
        windows = await invoke<{ hwnd: number; title: string; w: number; h: number }[]>("list_windows");
      } catch {
        /* window capture unavailable: displays only */
      }
    }
    if (mode !== "window" && displays.length <= 1) {
      await beginRecordWith({ kind: "display", name: displays[0]?.name ?? "", label: "Display 1" });
      return;
    }
    setSources({ displays, windows });
    setSourceTab(mode === "window" && windows.length > 0 ? "window" : "display");
    setShowSource(true);
  };

  const beginRecordWith = async (target: RecordTarget) => {
    setShowSource(false);
    setCoachRecord(false);
    setRecordTarget(target);
    // The capture rate is a preference; older engines without the command just keep theirs.
    void invoke("set_capture_fps", { fps: prefs.captureFps() }).catch(() => undefined);
    pushCursorMode();
    pushAudioChoice();
    try {
      setStatus("Choose the area to record…");
      setBackdrop(null);
      setRecordPhase("active"); // overlay shows immediately (dark + presets)
      // enter_overlay hides the editor, grabs the target as the selector backdrop, then
      // brings the window back fullscreen + excluded from capture. It returns the frozen
      // frame as a data-URL (empty string if the grab failed → dark canvas fallback).
      const shot =
        target.kind === "display"
          ? await invoke<string>("enter_overlay", { monitorName: target.name || null })
          : await invoke<string>("enter_overlay", { windowHwnd: target.hwnd });
      setBackdrop(shot || null);
    } catch (e) {
      setRecordPhase("idle");
      setStatus(`Error: ${String(e)}`);
      toast(`Could not start: ${friendlyError(e)}`, "error");
    }
  };

  const [recordTarget, setRecordTarget] = createSignal<RecordTarget | null>(null);

  const onRecordFinished = async (summary: RecordingSummary) => {
    setRecordPhase("idle");
    setBackdrop(null);
    setRecordTarget(null);
    await loadFinishedClip(summary);
    toast(
      `Recording loaded: ${summary.duration.toFixed(1)}s, ${summary.zooms} zoom${summary.zooms === 1 ? "" : "s"}`,
      "success",
    );
  };
  const onRecordCancel = () => {
    setRecordPhase("idle");
    setBackdrop(null);
    setRecordTarget(null);
    setStatus("Recording cancelled");
  };
  const onRecordFailed = (message: string) => {
    setRecordPhase("idle");
    setBackdrop(null);
    setStatus(`Recording failed: ${message}`);
  };

  // ── timeline filmstrip ────────────────────────────────────────────────────────────
  // Thumbnails of the raw recording, fetched once per loaded clip (they never change with
  // edits). A newer clip load supersedes an in-flight fetch.
  const FILM_COUNT = 32;
  const [thumbs, setThumbs] = createSignal<string[]>([]);
  let thumbGen = 0;
  const loadThumbs = async () => {
    const gen = ++thumbGen;
    setThumbs([]);
    try {
      const list = await invoke<string[]>("thumbnails", { count: FILM_COUNT, width: 160 });
      if (gen === thumbGen) setThumbs(list);
    } catch {
      /* older engine or no clip: the filmstrip simply stays empty */
    }
  };

  const loadFinishedClip = async (summary: RecordingSummary) => {
    // A new clip's tracks load fresh (refreshClip below adopts and decodes them).
    audio.unload();
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
    // A freshly loaded clip (new recording / recover / open project) starts clean,
    // reset after the syncs above, which optimistically flag dirty.
    setDirty(false);
    void loadThumbs();
    persistedVersion = editVersion; // nothing new for autosave to write
  };

  // ── zoom segment editing ───────────────────────────────────────────────────────
  const selectedZoom = () => {
    const i = selZoom();
    return i === null ? undefined : zooms()[i];
  };
  const addZoomAt = async (t: number = playhead()) => {
    if (!hasClip()) return;
    // Optimistic: the block appears and is selected THIS frame; the engine call follows
    // and its result replaces the local insert. On failure the insert is rolled back.
    const start = Math.max(0, Math.min(t, duration() - 0.5));
    const end = Math.min(duration(), start + 1.6);
    const inserted: ZoomSeg = { start, end, amount: zoomAmount(), mode: "Auto", style: "Smooth" };
    const local = [...zooms(), inserted].sort((a, b) => a.start - b.start);
    const localIdx = local.indexOf(inserted);
    setZooms(local);
    setDirty(true);
    setSelected(null);
    setSelSpeed(null);
    setSelCut(null);
    setSelZoom(localIdx >= 0 ? localIdx : null);
    setStatus("Zoom added. Drag its edges to retime.");
    try {
      if (!localStorage.getItem("vuoom-hint-zoom-undo")) {
        localStorage.setItem("vuoom-hint-zoom-undo", "1");
        toast("Zoom added. Ctrl+Z undoes it.", "info");
      }
    } catch {
      /* hint is best-effort */
    }
    try {
      const list = await invoke<ZoomSeg[]>("add_zoom", { t });
      setZooms(list);
      const idx = list.findIndex((z) => t >= z.start - 1e-6 && t <= z.end + 1e-6);
      setSelZoom(idx >= 0 ? idx : null);
      await pushSeek(playhead());
    } catch (e) {
      setZooms(zooms().filter((z) => z !== inserted));
      setSelZoom(null);
      toast(`Could not add zoom: ${friendlyError(e)}`, "error");
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
      await refreshClip(); // the drag already moved the local block; restore engine truth
      toast(`Zoom edit failed: ${friendlyError(e)}`, "error");
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
  // ── zoom easing/feel preset ──────────────────────────────────────────────────────
  const applyZoomStyle = async (style: ZoomStyle) => {
    const i = selZoom();
    if (i === null) return;
    try {
      setZooms(await invoke<ZoomSeg[]>("set_zoom_style", { index: i, style }));
      setDirty(true);
      await pushSeek(playhead());
    } catch (e) {
      setStatus(`Zoom feel failed: ${String(e)}`);
    }
  };
  // Crosshair dragging on the canvas.
  const [focusDrag, setFocusDrag] = createSignal<Vec2 | null>(null);
  const onFocusDown = (e: PointerEvent) => {
    e.stopPropagation();
    try { (e.currentTarget as Element).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
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
  const skimSync = createSyncSlot<{ clear: boolean; factor: number; prev: SpeedRegion[] }>();
  const toggleSkim = () => {
    if (!hasClip()) return;
    const clearing = speed().length > 0;
    const prev = speed();
    if (clearing) {
      setSpeed([]);
      setSelSpeed(null);
    }
    setDirty(true);
    skimSync.push({ clear: clearing, factor: skimFactor(), prev }, async (val, superseded) => {
      try {
        if (val.clear) {
          await invoke("clear_speed");
          if (!superseded()) setStatus("Idle stretches back to normal speed");
        } else {
          const regions = await invoke<SpeedRegion[]>("auto_speed", { factor: val.factor });
          setSpeed(regions);
          if (!superseded()) {
            setStatus(
              regions.length > 0
                ? `${regions.length} idle ${regions.length === 1 ? "stretch" : "stretches"} will play at ${val.factor}×`
                : "No idle stretches longer than ~2.5s found",
            );
          }
        }
      } catch (e) {
        if (!superseded()) {
          setSpeed(val.prev);
          toast(`Skim idle failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };

  // ── crop ───────────────────────────────────────────────────────────────────────
  // Presets anchor on the full frame; custom rects come from the Appearance popover.
  const applyCrop = async (c: CropRect | null) => {
    if (!hasClip()) return;
    try {
      const cs = c
        ? await invoke<ClipState>("set_crop", { x: c.x, y: c.y, w: c.w, h: c.h })
        : await invoke<ClipState>("set_crop");
      setCrop(cs.crop);
      setDirty(true);
      await pushSeek(playhead());
      setStatus(c ? "Crop applied. Annotations kept their on-screen placement." : "Crop reset to full frame.");
    } catch (e) {
      setStatus(`Crop failed: ${String(e)}`);
      toast(`Crop failed: ${friendlyError(e)}`, "error");
    }
  };

  // ── visual crop editing ─────────────────────────────────────────────────────────
  // While editing, the stage shows the whole uncropped, unframed recording and the crop
  // editor draws the draft rect on top; Apply writes it (or clears it when it covers the
  // whole frame), Cancel restores the previous crop. The frame preset comes back either way.
  const FULL_CROP: CropRect = { x: 0, y: 0, w: 1, h: 1 };
  const [cropEdit, setCropEdit] = createSignal<{ prevCrop: CropRect | null; prevFrame: string } | null>(null);
  const [cropDraft, setCropDraft] = createSignal<CropRect>(FULL_CROP);
  const beginCropEdit = async () => {
    if (!hasClip() || cropEdit()) return;
    const prevCrop = crop();
    const prevFrame = framePreset();
    setPlaying(false);
    setTool("select");
    setSelected(null);
    setSelZoom(null);
    setSelSpeed(null);
    setSelCut(null);
    setCropDraft(prevCrop ?? FULL_CROP);
    setCropEdit({ prevCrop, prevFrame });
    if (prevFrame !== "none") applyFramePreset("none");
    if (prevCrop) await applyCrop(null);
  };
  const finishCropEdit = async (apply: boolean) => {
    const st = cropEdit();
    if (!st) return;
    setCropEdit(null);
    const r = cropDraft();
    const whole = r.x < 0.002 && r.y < 0.002 && r.w > 0.996 && r.h > 0.996;
    const target = apply ? (whole ? null : r) : st.prevCrop;
    if (target || crop()) await applyCrop(target);
    if (st.prevFrame !== "none") applyFramePreset(st.prevFrame);
  };

  // Centered crop presets: shrink the LONGER side to match the target ratio.
  const centeredCrop = (ratio: number): CropRect => {
    const srcAspect = frameAspect();
    let w = 1.0;
    let h = 1.0;
    if (srcAspect > ratio) {
      w = ratio / srcAspect;
    } else {
      h = srcAspect / ratio;
    }
    return { x: (1 - w) / 2, y: (1 - h) / 2, w, h };
  };

  // ── zoom re-planning ────────────────────────────────────────────────────────────
  const [zoomStrength, setZoomStrength] = createSignal(1.8);
  const planZoomAuto = async () => {
    if (!hasClip()) return;
    try {
      const list = await invoke<ZoomSeg[]>("plan_zoom_auto", { amount: zoomStrength() });
      setZooms(list);
      setDirty(true);
      setSelZoom(null);
      setStatus(
        list.length > 0
          ? `Auto-planned ${list.length} zoom${list.length === 1 ? "" : "s"} at ${zoomStrength()}×. Ctrl+Z restores your manual zooms.`
          : "No click activity found to plan zooms from",
      );
      toast(`Auto-planned ${list.length} zoom${list.length === 1 ? "" : "s"}`, "success");
      await pushSeek(playhead());
    } catch (e) {
      setStatus(`Auto zoom failed: ${String(e)}`);
      toast(`Auto zoom failed: ${friendlyError(e)}`, "error");
    }
  };

  // ── click ripples ──────────────────────────────────────────────────────────────
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
      setStatus("Speed region added. Drag it to retime.");
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
      await refreshClip(); // the drag already moved the local band; restore engine truth
      toast(`Speed edit failed: ${friendlyError(e)}`, "error");
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
      setSelCut(idx >= 0 ? idx : null);
      setStatus("Section cut. Drag the band to adjust.");
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
      await refreshClip(); // the drag already moved the local band; restore engine truth
      toast(`Cut edit failed: ${friendlyError(e)}`, "error");
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
  // Latest-wins optimistic sync: the UI flips immediately, the newest desired value is
  // what persists, and an older failure rolls back only when no newer click superseded it.
  const frameSync = createSyncSlot<{ preset: string; prevBg: string }>();
  const applyFramePreset = (preset: string) => {
    if (!hasClip()) return;
    const prev = framePreset();
    const prevBg = bgPreset();
    setFramePreset(preset);
    // The backend seeds a graphite backdrop the first time a frame is enabled on the
    // still-default black one; mirror that so the swatch picker reflects it immediately.
    if (preset !== "none" && !bgPreset()) setBgPreset("graphite");
    setDirty(true);
    frameSync.push({ preset, prevBg }, async (val, superseded) => {
      try {
        await invoke("set_frame_preset", { preset: val.preset });
        await refreshFrame();
        await pushSeek(playhead());
        setStatus(
          val.preset === "none" ? "Frame removed. Edge to edge export." : `Frame: ${val.preset}`,
        );
      } catch (e) {
        if (!superseded()) {
          setFramePreset(prev);
          setBgPreset(val.prevBg);
          toast(`Frame failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };

  // ── background backdrop (gradient/solid behind a framed recording) ───────────────
  // Swatch CSS mirrors the Rust presets in vuoom-project/frame.rs (135 = top-left light).
  const BG_SWATCHES: { name: string; label: string; css: string }[] = [
    { name: "graphite", label: "Graphite", css: "linear-gradient(135deg,#29292b,#0a0a0d)" },
    { name: "slate", label: "Slate", css: "linear-gradient(135deg,#333d4d,#12171f)" },
    { name: "teal", label: "Teal", css: "linear-gradient(135deg,#0f3335,#051417)" },
    { name: "dusk", label: "Dusk", css: "linear-gradient(135deg,#2b3043,#0f0f1a)" },
    { name: "paper", label: "Paper", css: "linear-gradient(135deg,#f5f2eb,#d9d4c7)" },
    { name: "midnight", label: "Midnight", css: "linear-gradient(135deg,#0f121a,#030305)" },
    { name: "solid", label: "Solid", css: "#17171a" },
  ];
  const bgSync = createSyncSlot<string>();
  const applyBackground = (name: string) => {
    if (!hasClip()) return;
    const prev = bgPreset();
    setBgPreset(name);
    setDirty(true);
    bgSync.push(name, async (val, superseded) => {
      try {
        await invoke("set_background_preset", { name: val });
        await refreshFrame();
        await pushSeek(playhead());
        setStatus(`Backdrop: ${val}`);
      } catch (e) {
        if (!superseded()) {
          setBgPreset(prev);
          toast(`Backdrop failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };

  // ── fine frame + backdrop controls ─────────────────────────────────────────────
  const refreshFrame = async () => {
    try {
      const cs = await invoke<ClipState>("clip_state");
      setFrameInfo(cs.frame ?? null);
      setFramePreset(cs.frame_preset);
      setBgPreset(cs.background_preset);
    } catch {
      /* no clip */
    }
  };
  // Slider drags: the value lands locally this frame, the engine gets the latest value.
  const frameStyleSync = createSyncSlot<{ padding: number; radius: number; shadow: number }>();
  const applyFrameStyle = (patch: { padding?: number; radius?: number; shadow?: number }) => {
    const cur = frameInfo();
    if (!hasClip() || !cur) return;
    const next = {
      padding: patch.padding ?? cur.padding,
      radius: patch.radius ?? cur.corner_radius,
      shadow: patch.shadow ?? cur.shadow,
    };
    setFrameInfo({ ...cur, padding: next.padding, corner_radius: next.radius, shadow: next.shadow });
    setDirty(true);
    frameStyleSync.push(next, async (val, superseded) => {
      try {
        await invoke("set_frame_style", { padding: val.padding, cornerRadius: val.radius, shadow: val.shadow });
        if (!superseded()) await refreshFrame();
        await pushSeek(playhead());
      } catch (e) {
        if (!superseded()) {
          await refreshFrame();
          toast(`Frame change failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };
  const bgCustomSync = createSyncSlot<{ from: number[]; to: number[] | null; angle: number }>();
  /** Custom backdrop: `to` null = solid color. Colors are "#rrggbb". */
  const applyBackgroundCustom = (from: string, to: string | null, angle: number) => {
    const cur = frameInfo();
    if (!hasClip() || !cur) return;
    const rgb = (hex: string) => {
      const c = hexRgb(hex);
      return [c.r, c.g, c.b] as [number, number, number];
    };
    const f = rgb(from);
    const t = to ? rgb(to) : null;
    setFrameInfo({ ...cur, bg_kind: t ? "gradient" : "solid", bg_from: f, bg_to: t ?? f, bg_angle: angle });
    setBgPreset("");
    setDirty(true);
    bgCustomSync.push({ from: f, to: t, angle }, async (val, superseded) => {
      try {
        await invoke("set_background_custom", val.to ? { from: val.from, to: val.to, angle: val.angle } : { from: val.from, angle: val.angle });
        await pushSeek(playhead());
      } catch (e) {
        if (!superseded()) {
          await refreshFrame();
          toast(`Backdrop failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };

  const clicksSync = createSyncSlot<boolean>();
  const toggleClicks = () => {
    if (!hasClip()) return;
    const on = !showClicks();
    setShowClicks(on);
    setDirty(true);
    clicksSync.push(on, async (val, superseded) => {
      try {
        await invoke("set_show_clicks", { on: val });
        await pushSeek(playhead());
        setStatus(val ? "Mouse clicks will ripple in the GIF" : "Click ripples off");
      } catch (e) {
        if (!superseded()) {
          setShowClicks(!val);
          toast(`Click ripples failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };

  // ── re-drawn pointer ────────────────────────────────────────────────────────────
  const DEFAULT_CURSOR: CursorStyle = { size: 1.5, smoothing: 0.05, hide_idle: false };
  const [cursorStyle, setCursorStyle] = createSignal<CursorStyle | null>(null);
  const [pointerCaptured, setPointerCaptured] = createSignal(true);
  // Remembered while the pointer is off, so switching it back on restores the last look.
  let lastCursor: CursorStyle = DEFAULT_CURSOR;
  const cursorSync = createSyncSlot<{ style: CursorStyle | null }>();
  const applyCursor = (style: CursorStyle | null) => {
    if (!hasClip()) return;
    if (style) lastCursor = style;
    setCursorStyle(style);
    setDirty(true);
    cursorSync.push({ style }, async (val, superseded) => {
      try {
        await invoke("set_cursor_style", { style: val.style });
        await pushSeek(playhead());
      } catch (e) {
        if (!superseded()) toast(`Pointer change failed: ${friendlyError(e)}`, "error");
      }
    });
  };
  const toggleCursor = () => applyCursor(cursorStyle() ? null : lastCursor);
  const setCursorSize = (size: number) => applyCursor({ ...(cursorStyle() ?? lastCursor), size });
  const setCursorSmoothing = (smoothing: number) => applyCursor({ ...(cursorStyle() ?? lastCursor), smoothing });
  const setCursorHideIdle = (hide_idle: boolean) => applyCursor({ ...(cursorStyle() ?? lastCursor), hide_idle });

  // ── keystroke overlay ──────────────────────────────────────────────────────────
  const keysSync = createSyncSlot<boolean>();
  const toggleKeys = () => {
    if (!hasClip()) return;
    const on = !showKeys();
    setShowKeys(on);
    setDirty(true);
    keysSync.push(on, async (val, superseded) => {
      try {
        await invoke("set_show_keys", { on: val });
        await pushSeek(playhead());
        setStatus(
          val
            ? "Shortcuts you pressed will show as chips (plain typing never does)"
            : "Keystroke overlay off",
        );
      } catch (e) {
        if (!superseded()) {
          setShowKeys(!val);
          toast(`Keystroke overlay failed: ${friendlyError(e)}`, "error");
        }
      }
    });
  };

  // ── timeline (ruler + tracks + drag-to-scrub) ─────────────────────────────────────
  let tlEl: HTMLDivElement | undefined; // outer viewport box (.tl)
  let tlScrollEl: HTMLDivElement | undefined; // horizontal-scroll wrapper (.tl-scroll)
  let tlTrackEl: HTMLDivElement | undefined; // inner track (.tl-track-inner), the scaled surface
  let tlDrag = false;
  // Single source of truth for clientX → time. It measures the *inner track*, whose
  // getBoundingClientRect already reflects scrollLeft (its left edge slides negative as the
  // wrapper scrolls) and whose width is the scaled track width, so this one formula works
  // in both fit mode and zoomed-and-scrolled mode with no scroll math of its own.
  // The track's client rect, cached per gesture and invalidated on scroll/resize/scale.
  // getBoundingClientRect on every pointermove is the single hottest timeline cost.
  let tlRect: DOMRect | null = null;
  const refreshTlRect = () => {
    tlRect = (tlTrackEl ?? tlEl)?.getBoundingClientRect() ?? null;
  };
  const invalidateTlRect = () => {
    tlRect = null;
  };
  const timeFromClientX = (clientX: number) => {
    if (!tlRect) refreshTlRect();
    const r = tlRect;
    if (!r || r.width === 0) return 0;
    return clamp01((clientX - r.left) / r.width) * duration();
  };
  const tlTime = (e: PointerEvent) => timeFromClientX(e.clientX);
  const tlSeekFromEvent = (e: PointerEvent) => {
    if (!tlEl || !hasClip() || duration() <= 0) return;
    scrub(tlTime(e));
  };

  // Trim handle dragging (local preview while dragging, committed on release).
  let trimDrag: "start" | "end" | null = null;
  const onTrimDown = (which: "start" | "end") => (e: PointerEvent) => {
    e.stopPropagation();
    try { (e.currentTarget as Element).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
    trimDrag = which;
    beginSnapGesture(which === "start" ? "tS" : "tE");
    refreshTlRect();
  };
  const onTrimMove = (e: PointerEvent) => {
    if (!trimDrag || !tlEl) return;
    const cur = trim() ?? { start: 0, end: duration() };
    let t = tlTime(e);
    const snap = snapProbes([t], trimDrag === "start" ? "tS" : "tE", e.altKey);
    if (snap) {
      t += snap.off;
      setSnapLine(snap.line);
    } else {
      setSnapLine(null);
    }
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
    setSnapLine(null);
    endSnapGesture();
    const t = trim();
    if (!t) return;
    try {
      await invoke("set_trim", { start: t.start, end: t.end });
      await refreshClip(); // backend may normalize a full-range trim to null
      if (playhead() < tStart() || playhead() > tEnd()) scrub(tStart());
    } catch (e) {
      await refreshClip(); // the handle already moved locally; restore engine truth
      toast(`Trim failed: ${friendlyError(e)}`, "error");
    }
  };

  // Set the trim window directly (context menu, I / O keys). A full-range window clears it.
  const commitTrim = async (start: number, end: number) => {
    if (!hasClip()) return;
    const s0 = Math.max(0, Math.min(start, end - 0.2));
    const e0 = Math.min(duration(), Math.max(end, s0 + 0.2));
    setTrimState({ start: s0, end: e0 });
    setDirty(true);
    try {
      await invoke("set_trim", { start: s0, end: e0 });
      await refreshClip();
      if (playhead() < tStart() || playhead() > tEnd()) scrub(tStart());
    } catch (e) {
      await refreshClip();
      toast(`Trim failed: ${friendlyError(e)}`, "error");
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
    try { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
    beginSnapGesture(`z${idx}`);
    refreshTlRect();
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

  // ── magnetic snapping ─────────────────────────────────────────────────────────────
  // Ported from palmier's SnapEngine: an 8px catch radius, the playhead gets a 1.5× radius
  // and wins ties, and a snap is "sticky", once engaged it takes 1.5× the radius to break
  // away. Alt bypasses. `snapHold` is the per-drag sticky target (seconds); `snapLine`
  // drives the guide that flashes across the timeline while a snap is engaged.
  const SNAP_PX = 8;
  const SNAP_STICKY = 1.5;
  const SNAP_PLAYHEAD = 1.5;
  let snapHold: number | null = null;
  const [snapLine, setSnapLine] = createSignal<number | null>(null);

  // Grid step: the smallest of these that renders ≥8px wide at the *effective* scale, so we
  // snap to whole seconds on a long clip and finer marks as the timeline is zoomed in.
  const snapGrid = () => {
    const pps = pxPerSec();
    if (pps <= 0) return 1;
    return [0.25, 0.5, 1, 5].find((s) => s * pps >= SNAP_PX) ?? 5;
  };

  // Fixed snap targets: clip bounds, the trim in/out, and every segment edge on every track
  // (cross-track alignment is free and useful). `excludeTag` drops the segment being dragged
  // so an edge never snaps to its own original position. Playhead and grid are added by
  // snapProbes so they can carry their own catch radius.
  const snapTargets = (excludeTag?: string): number[] => {
    const out: number[] = [0, duration()];
    const tr = trim();
    if (tr) {
      if (excludeTag !== "tS") out.push(tr.start);
      if (excludeTag !== "tE") out.push(tr.end);
    }
    const seg = (tag: string, s: number, e: number) => {
      if (tag !== excludeTag) out.push(s, e);
    };
    zooms().forEach((z, i) => {
      seg(`z${i}`, z.start, z.end);
    });
    speed().forEach((r, i) => {
      seg(`s${i}`, r.start, r.end);
    });
    cuts().forEach((c, i) => {
      seg(`c${i}`, c.start, c.end);
    });
    annBars().forEach((b) => {
      seg(`a${b.kind}${b.id}`, b.start, b.end);
    });
    return out;
  };

  // Snap the given probe time(s) to the nearest target. Returns the time offset to apply to
  // every probe (so a two-edge segment move shifts as one unit) plus the guide position, or
  // null when nothing is in reach. Playhead considered first + strict-less keeps its tie
  // priority; while a snap is held it takes 1.5× the radius for any probe to break away.
  // Fixed targets for the ACTIVE gesture, snapshotted once at pointer-down (snapTargets
  // sorts every edge on every track; rebuilding that on every pointermove is waste).
  // The playhead stays dynamic inside snapProbes.
  let activeSnapBase: number[] | null = null;
  const beginSnapGesture = (excludeTag: string) => {
    activeSnapBase = snapTargets(excludeTag);
    snapHold = null;
  };
  const endSnapGesture = () => {
    activeSnapBase = null;
    snapHold = null;
  };
  const snapProbes = (
    probes: number[],
    _excludeTag: string | undefined,
    alt: boolean,
  ): { off: number; line: number } | null => {
    const pps = pxPerSec();
    if (alt || !prefs.snapping() || pps <= 0) {
      snapHold = null;
      return null;
    }
    const base = SNAP_PX / pps; // 8px, in seconds
    if (snapHold !== null) {
      const hold = base * SNAP_STICKY;
      let bp: number | null = null;
      let bd = Number.POSITIVE_INFINITY;
      for (const p of probes) {
        const d = Math.abs(p - snapHold);
        if (d <= hold && d < bd) {
          bd = d;
          bp = p;
        }
      }
      if (bp !== null) return { off: snapHold - bp, line: snapHold };
      snapHold = null;
    }
    const targets = activeSnapBase ?? snapTargets(_excludeTag);
    const ph = playhead();
    const grid = snapGrid();
    // Collect every in-reach (probe, target) pair, then take the closest. Playhead is pushed
    // first (and compared with strict-less below) so it wins ties, its priority.
    const cands: { off: number; line: number; dist: number }[] = [];
    const consider = (p: number, target: number, thr: number) => {
      const d = Math.abs(p - target);
      if (d <= thr) cands.push({ off: target - p, line: target, dist: d });
    };
    for (const p of probes) {
      consider(p, ph, base * SNAP_PLAYHEAD); // playhead: wider radius, considered first
      for (const t of targets) consider(p, t, base);
      const g = Math.round(p / grid) * grid; // nearest grid line
      if (g >= 0 && g <= duration()) consider(p, g, base);
    }
    let best: { off: number; line: number; dist: number } | null = null;
    for (const c of cands) if (!best || c.dist < best.dist) best = c;
    if (!best) return null;
    snapHold = best.line;
    return { off: best.off, line: best.line };
  };

  // Snap a clamped segment drag: re-clamp with the snap offset folded into dt, and keep the
  // guide only if the intended edge actually reached the target (a neighbour may block it).
  const snapSegDrag = (
    mode: "move" | "l" | "r",
    orig: { start: number; end: number },
    dt: number,
    minLen: number,
    prevEnd: number,
    nextStart: number,
    excludeTag: string,
    alt: boolean,
  ) => {
    const c = clampSegDrag(mode, orig, dt, minLen, prevEnd, nextStart);
    const probes = mode === "move" ? [c.start, c.end] : mode === "l" ? [c.start] : [c.end];
    const snap = snapProbes(probes, excludeTag, alt);
    if (!snap) {
      setSnapLine(null);
      return c;
    }
    const c2 = clampSegDrag(mode, orig, dt + snap.off, minLen, prevEnd, nextStart);
    const landed =
      mode === "move"
        ? Math.abs(c2.start - snap.line) < 1e-4 || Math.abs(c2.end - snap.line) < 1e-4
        : mode === "l"
          ? Math.abs(c2.start - snap.line) < 1e-4
          : Math.abs(c2.end - snap.line) < 1e-4;
    if (landed) {
      setSnapLine(snap.line);
      return c2;
    }
    snapHold = null;
    setSnapLine(null);
    return c;
  };

  const onZoomMove = (e: PointerEvent) => {
    const d = zoomDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    const arr = zooms();
    const prevEnd = Math.max(0, arr[d.idx - 1]?.end ?? 0);
    const nextStart = Math.min(duration(), arr[d.idx + 1]?.start ?? duration());
    const { start, end } = snapSegDrag(d.mode, d.orig, dt, 0.2, prevEnd, nextStart, `z${d.idx}`, e.altKey);
    setZoomDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onZoomUp = async () => {
    const d = zoomDrag();
    if (!d) return;
    setSnapLine(null);
    endSnapGesture();
    setSelected(null);
    setSelSpeed(null);
    setSelCut(null);
    if (d.moved) {
      // Commit the dragged geometry into the LOCAL model before clearing the drag
      // override, so the block never flashes back to its pre-drag position during the
      // engine round-trip. A rejected commit re-syncs from the engine (applyZoomEdit).
      setZooms(zooms().map((z, i) => (i === d.idx ? { ...z, start: d.cur.start, end: d.cur.end } : z)));
      setDirty(true);
      setZoomDrag(null);
      await applyZoomEdit(d.idx, d.cur.start, d.cur.end, zooms()[d.idx]?.amount ?? 1.8);
    } else {
      setZoomDrag(null);
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
    try { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
    beginSnapGesture(`s${idx}`);
    refreshTlRect();
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
    const { start, end } = snapSegDrag(d.mode, d.orig, dt, 0.2, prevEnd, nextStart, `s${d.idx}`, e.altKey);
    setSpeedDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onSpeedUp = async () => {
    const d = speedDrag();
    if (!d) return;
    setSnapLine(null);
    endSnapGesture();
    setSelected(null);
    setSelZoom(null);
    setSelCut(null);
    if (d.moved) {
      // Local-first commit (see onZoomUp): the band holds its dragged span until the
      // engine acknowledges, and refreshClip() reverts it if the commit is rejected.
      setSpeed(speed().map((r, i) => (i === d.idx ? { ...r, start: d.cur.start, end: d.cur.end } : r)));
      setDirty(true);
      setSpeedDrag(null);
      await applySpeedEdit(d.idx, d.cur.start, d.cur.end, speed()[d.idx]?.factor ?? skimFactor());
    } else {
      setSpeedDrag(null);
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
    try { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
    beginSnapGesture(`c${idx}`);
    refreshTlRect();
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
    const { start, end } = snapSegDrag(d.mode, d.orig, dt, 0.1, prevEnd, nextStart, `c${d.idx}`, e.altKey);
    setCutDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onCutUp = async () => {
    const d = cutDrag();
    if (!d) return;
    setSnapLine(null);
    endSnapGesture();
    setSelected(null);
    setSelZoom(null);
    setSelSpeed(null);
    if (d.moved) {
      // Local-first commit (see onZoomUp).
      setCuts(cuts().map((c, i) => (i === d.idx ? { ...c, start: d.cur.start, end: d.cur.end } : c)));
      setDirty(true);
      setCutDrag(null);
      await applyCutEdit(d.idx, d.cur.start, d.cur.end);
    } else {
      setCutDrag(null);
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
  // Fit scale = the px-per-second at which the track exactly fills the viewport.
  const fitPps = () => (duration() > 0 ? tlWidth() / duration() : 0);
  // Effective px-per-second: the current zoom scale, or the fit scale in fit mode. EVERYTHING
  // downstream (ruler ticks, and, crucially, the snap catch radius which is SNAP_PX/pxPerSec)
  // reads this, so snap tolerances and grid stay constant in *screen pixels* at any zoom.
  const pxPerSec = () => tlScale() ?? fitPps();
  createEffect(() => {
    // Timeline zoom changes the track's rendered geometry: the cached rect goes stale.
    void tlScale();
    void duration();
    invalidateTlRect();
  });
  const TL_MAX_PPS = 200; // ~200 px/s ceiling; fitPps() is the floor
  // CSS width for the inner track: 100% in fit mode (exact old layout), else duration*scale
  // (never below the viewport, so a barely-zoomed track can't leave a gap).
  const trackWidth = () => {
    const s = tlScale();
    if (s == null || duration() <= 0) return "100%";
    return `${Math.max(duration() * s, tlWidth())}px`;
  };
  // Apply a new scale while keeping `tAnchor` (seconds) pinned under viewport pixel `keepX`.
  // Any target at or below fit snaps back to fit mode (null) so fit renders byte-identical.
  const applyScaleAnchored = (next: number, tAnchor: number, keepX: number) => {
    if (next <= fitPps() + 0.001 || duration() <= 0) {
      setTlScale(null);
      return;
    }
    const n = Math.min(TL_MAX_PPS, next);
    setTlScale(n);
    // Width updates reactively; set scrollLeft after the DOM reflows.
    requestAnimationFrame(() => {
      if (!tlScrollEl) return;
      const w = duration() * n;
      const max = Math.max(0, w - tlScrollEl.clientWidth);
      tlScrollEl.scrollLeft = Math.max(0, Math.min(tAnchor * n - keepX, max));
    });
  };
  // +/− buttons: zoom around the centre of the current view.
  const zoomTimeline = (dir: 1 | -1) => {
    if (duration() <= 0) return;
    const cur = tlScale() ?? fitPps();
    if (cur <= 0) return;
    const w = tlScrollEl?.clientWidth ?? tlWidth();
    const centerX = (tlScrollEl?.scrollLeft ?? 0) + w / 2;
    applyScaleAnchored(cur * (dir > 0 ? 1.6 : 1 / 1.6), centerX / cur, w / 2);
  };
  // Ctrl+wheel zooms around the cursor; plain wheel scrolls horizontally when zoomed.
  const onTlWheel = (e: WheelEvent) => {
    if (!hasClip() || duration() <= 0) return;
    if (e.ctrlKey) {
      e.preventDefault();
      const cur = tlScale() ?? fitPps();
      if (cur <= 0) return;
      const keepX = e.clientX - (tlScrollEl ?? tlEl)!.getBoundingClientRect().left;
      applyScaleAnchored(cur * Math.exp(-e.deltaY * 0.0015), timeFromClientX(e.clientX), keepX);
    } else if (tlScale() != null && tlScrollEl) {
      const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      if (delta !== 0) {
        e.preventDefault();
        tlScrollEl.scrollLeft += delta;
      }
    }
  };
  // Re-fit if the viewport grew past the current scale (window resize / smaller clip): the
  // fit floor rose above the zoom, so drop back to fit mode rather than show a sub-fit track.
  createEffect(() => {
    const s = tlScale();
    if (s != null && s < fitPps()) setTlScale(null);
  });
  // While zoomed, keep the playhead on screen: when it leaves the visible span, jump the
  // wrapper so the head sits ~40% from the left. Only fires on playhead change, so manual
  // horizontal scrolling of a paused clip is never fought.
  createEffect(() => {
    const s = tlScale();
    const ph = playhead();
    if (s == null || !tlScrollEl) return;
    const x = ph * s;
    const view = tlScrollEl.scrollLeft;
    const w = tlScrollEl.clientWidth;
    if (x < view + 24 || x > view + w - 24) {
      const max = Math.max(0, duration() * s - w);
      tlScrollEl.scrollLeft = Math.max(0, Math.min(x - w * 0.4, max));
    }
  });
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
    // A Shift/Ctrl-click on the bar toggles multi-selection instead of selecting/retiming.
    additive: boolean;
  } | null>(null);
  const annGeom = (b: { kind: Kind; id: number; start: number; end: number }) => {
    const d = annDrag();
    return d && d.kind === b.kind && d.id === b.id ? d.cur : { start: b.start, end: b.end };
  };
  const onAnnDown =
    (b: { kind: Kind; id: number; start: number; end: number }, force: "l" | "r" | "move") =>
    (e: PointerEvent) => {
      e.stopPropagation();
      try { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
      beginSnapGesture(`a${b.kind}${b.id}`);
      refreshTlRect();
      setAnnDrag({
        kind: b.kind,
        id: b.id,
        mode: force,
        grabT: tlTime(e),
        orig: { start: b.start, end: b.end },
        cur: { start: b.start, end: b.end },
        moved: force !== "move",
        additive: force === "move" && (e.shiftKey || e.ctrlKey || e.metaKey),
      });
    };
  const onAnnMove = (e: PointerEvent) => {
    const d = annDrag();
    if (!d) return;
    const dt = tlTime(e) - d.grabT;
    const { start, end } = snapSegDrag(d.mode, d.orig, dt, 0.2, 0, duration(), `a${d.kind}${d.id}`, e.altKey);
    setAnnDrag({ ...d, cur: { start, end }, moved: d.moved || Math.abs(dt) > 0.02 });
  };
  const onAnnUp = async () => {
    const d = annDrag();
    if (!d) return;
    setAnnDrag(null);
    setSnapLine(null);
    endSnapGesture();
    // A Shift/Ctrl-click (no drag) toggles the bar in/out of the multi-selection.
    if (d.additive && !d.moved) {
      toggleSelect(d.kind, d.id);
      scrub(d.orig.start);
      return;
    }
    setSelZoom(null);
    setSelSpeed(null);
    setSelCut(null);
    clearExtra();
    setSelected({ kind: d.kind, id: d.id });
    if (d.moved) {
      // Local-first commit: the lane bar holds its dragged span until the engine answers.
      patchAnn(d.kind, d.id, (a) => {
        const r = (a as TextAnn).range;
        (a as TextAnn).range = { ...r, start: d.cur.start, end: d.cur.end };
      });
      setDirty(true);
      try {
        await invoke("update_annotation_range", { id: d.id, start: d.cur.start, end: d.cur.end });
        await refresh();
        await pushSeek(playhead());
      } catch (e) {
        await refresh(); // rejected: engine truth restores the old span
        toast(`Retime failed: ${friendlyError(e)}`, "error");
      }
    } else {
      scrub(d.orig.start);
    }
  };

  // All annotations as flat timeline bars, sorted by start time. Memoized on the anns()
  // reference AND reusing bar objects per kind:id, so Solid's <For> keeps DOM rows across
  // refreshes (new objects every call would recreate every lane on every edit).
  let barsCacheSrc: AnnotationSet | null = null;
  let barsCache: { kind: Kind; id: number; start: number; end: number; label: string }[] = [];
  const annBars = () => {

    const a = anns();
    if (a !== barsCacheSrc) {
      const prev = new Map(barsCache.map((b) => [`${b.kind}:${b.id}`, b]));
      const next: typeof barsCache = [];
      for (const t of a.texts)
        next.push({ kind: "text", id: t.id, start: t.range.start, end: t.range.end, label: t.text || "Text" });
      for (const ar of a.arrows)
        next.push({
          kind: "arrow",
          id: ar.id,
          start: ar.range.start,
          end: ar.range.end,
          label: ar.style === "Line" ? "Line" : "Arrow",
        });
      for (const b of a.highlights)
        next.push({
          kind: "box",
          id: b.id,
          start: b.range.start,
          end: b.range.end,
          label: b.shape === "Ellipse" ? "Ellipse" : "Box",
        });
      // Reuse the previous object for an unchanged id+span so row identity survives.
      for (let i = 0; i < next.length; i++) {
        const bar = next[i];
        const old = prev.get(`${bar.kind}:${bar.id}`);
        if (old && old.start === bar.start && old.end === bar.end && old.label === bar.label) {
          next[i] = old;
        }
      }
      barsCache = next.sort((x, y) => x.start - y.start);
      barsCacheSrc = a;
    }
    return barsCache;
  };

  // ── resizable inspector ────────────────────────────────────────────────────────
  const [inspectorW, setInspectorW] = createSignal(
    Number(localStorage.getItem("vuoom-inspector-w")) || 296,
  );
  let inspectorDrag = false;
  const onInspDown = (e: PointerEvent) => {
    e.stopPropagation();
    try { (e.currentTarget as Element).setPointerCapture(e.pointerId); } catch { /* synthetic/inactive pointer: drag still tracks via bubbling */ }
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
  // A drawing tool is armed (not Select). Used to swap the inspector into a tool-context card.
  const drawingToolActive = () => hasClip() && tool() !== "select";

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
      rememberRecent(dir, captureThumb());
      setStatus(`Saved ${dir}`);
      toast("Project saved", "success");
    } catch (e) {
      setStatus(`Save failed: ${String(e)}`);
      toast(`Save failed: ${friendlyError(e)}`, "error");
    }
  };

  const onRecover = async () => {
    setStatus("Recovering your last session…");
    try {
      const summary = await invoke<RecordingSummary>("recover_session");
      setRecoverable(null);
      await loadFinishedClip(summary);
      setStatus(`Recovered ${summary.duration.toFixed(1)}s. Don't forget to export.`);
      toast("Last session recovered", "success");
    } catch (e) {
      setRecoverable(null);
      setStatus(`Recovery failed: ${String(e)}`);
      toast(`Recovery failed: ${friendlyError(e)}`, "error");
    }
  };

  // Probe the recovery store's disk usage for the shortcuts-panel storage line.
  const refreshStorage = async () => {
    try {
      const s = await invoke<{ bytes: number; sessions: number }>("recovery_storage");
      setRecoveryBytes(s.bytes);
    } catch {
      setRecoveryBytes(null);
    }
  };
  // Load the size lazily whenever the shortcuts panel opens, so it's current without polling.
  createEffect(() => {
    if (showShortcuts()) void refreshStorage();
  });

  // Delete recovery data from previous sessions. Destructive, confirm first, and the
  // currently-loaded clip's store is always kept by the backend.
  const clearStorage = async () => {
    const ok = await ask(
      "Delete saved recovery data? Unsaved takes from previous sessions can no longer be recovered.",
      { title: "Clear recovery storage?", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    setClearingStorage(true);
    try {
      const freed = await invoke<number>("clear_recovery_storage");
      await refreshStorage();
      setStatus(`Cleared ${fmtBytes(freed)} of recovery data`);
    } catch (e) {
      setStatus(`Couldn't clear storage: ${String(e)}`);
    } finally {
      setClearingStorage(false);
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
      rememberRecent(dir);
      setStatus("Project opened");
      toast("Project opened", "success");
    } catch (e) {
      setStatus(`Open failed: ${String(e)}`);
      toast(`Could not open project: ${friendlyError(e)}`, "error");
    }
  };


  // ── timeline surface gestures ───────────────────────────────────────────────────
  // Press anywhere on the timeline scrubs; a plain click (not a drag) on the empty zoom
  // lane adds a zoom at that moment.
  const onTlDown = (e: PointerEvent) => {
    if (!hasClip()) return;
    try {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
    } catch {
      /* synthetic/inactive pointer: drag still tracks via bubbling */
    }
    tlDrag = true;
    refreshTlRect();
    tlDownX = e.clientX;
    tlDownY = e.clientY;
    const target = e.target as Element;
    tlDownInZoomLane = !!target.closest(".tl-zoomlane") && !target.closest(".tl-seg, .tl-handle");
    setHoverT(null);
    setGhostT(null);
    tlSeekFromEvent(e);
  };
  const onTlUp = (e: PointerEvent) => {
    if (
      tlDrag &&
      tlDownInZoomLane &&
      Math.abs(e.clientX - tlDownX) <= 5 &&
      Math.abs(e.clientY - tlDownY) <= 5
    ) {
      void addZoomAt(timeFromClientX(e.clientX));
    }
    tlDownInZoomLane = false;
    tlDrag = false;
  };
  const onTlCancel = () => {
    tlDrag = false;
    tlDownInZoomLane = false;
    endSnapGesture();
    setSnapLine(null);
    setHoverT(null);
    setGhostT(null);
  };

  // ── autosave for crash recovery ────────────────────────────────────────────────
  // Every few seconds, if anything changed, the engine writes the edited project next to
  // the take's frames. Recovery after a crash then restores the edits, not just the raw
  // recording. Cheap (a JSON write) and skipped while recording.
  const AUTOSAVE_MS = 8000;
  const autosave = window.setInterval(() => {
    if (!hasClip() || recordPhase() !== "idle" || editVersion === persistedVersion) return;
    const v = editVersion;
    invoke("persist_edits")
      .then(() => {
        persistedVersion = v;
      })
      .catch(() => undefined);
  }, AUTOSAVE_MS);
  onCleanup(() => clearInterval(autosave));

  // ── DOM refs ─────────────────────────────────────────────────────────────────────
  // Components register the elements they own; size-tracking elements get a
  // ResizeObserver that is swapped out when the element is re-created (e.g. a panel
  // collapsing and re-opening).
  const observers = new Map<string, ResizeObserver>();
  const observe = (key: string, el: Element, cb: () => void) => {
    observers.get(key)?.disconnect();
    const ro = new ResizeObserver(cb);
    ro.observe(el);
    observers.set(key, ro);
    cb();
  };
  onCleanup(() => {
    for (const ro of observers.values()) ro.disconnect();
    observers.clear();
  });
  const refs = {
    canvas: (el: HTMLCanvasElement) => {
      canvasEl = el;
      preview.attach(el);
      // Repaint into the fresh canvas (the engine only pushes frames on seek/edit).
      if (hasClip()) queueMicrotask(() => void pushSeek(playhead()));
    },
    stage: (el: HTMLDivElement) => {
      stageEl = el;
      observe("stage", el, () => setStage({ w: el.clientWidth || 1, h: el.clientHeight || 1 }));
    },
    timeline: (el: HTMLDivElement) => {
      tlEl = el;
      el.addEventListener("wheel", onTlWheel, { passive: false });
      observe("timeline", el, () => {
        setTlWidth(el.clientWidth || 800);
        invalidateTlRect();
      });
    },
    tlScroll: (el: HTMLDivElement) => {
      tlScrollEl = el;
    },
    tlTrack: (el: HTMLDivElement) => {
      tlTrackEl = el;
    },
    recordBtn: (el: HTMLButtonElement) => {
      recordBtnEl = el;
    },
    contentInput: (el: HTMLInputElement) => {
      contentInput = el;
      // The field mounts after the selection changed, so seed it now.
      el.value = selectedText()?.text ?? "";
    },
  };

  return {
    thumbs,
    frameInfo,
    applyFrameStyle,
    applyBackgroundCustom,
    editFades,
    cropEdit,
    cropDraft,
    setCropDraft,
    beginCropEdit,
    finishCropEdit,
    commitTrim,
    settingsTab,
    setSettingsTab,
    showPalette,
    setShowPalette,
    isModal,
    recordMode,
    setRecordMode,
    sourceTab,
    setSourceTab,
    refs,
    onTlDown,
    onTlUp,
    onTlCancel,
    tool,
    setTool,
    toolLock,
    setToolLock,
    pickTool,
    lockTool,
    status,
    setStatus,
    projectName,
    setProjectName,
    editingText,
    setEditingText,
    theme,
    setTheme,
    hasClip,
    setHasClip,
    dirty,
    setDirty,
    duration,
    setDuration,
    playhead,
    setPlayhead,
    playing,
    setPlaying,
    audio,
    cursorStyle,
    pointerCaptured,
    toggleCursor,
    setCursorSize,
    setCursorSmoothing,
    setCursorHideIdle,
    looping,
    setLooping,
    anns,
    setAnns,
    zooms,
    setZooms,
    trim,
    setTrimState,
    speed,
    setSpeed,
    cuts,
    setCuts,
    selZoom,
    setSelZoom,
    selSpeed,
    setSelSpeed,
    selCut,
    setSelCut,
    skimFactor,
    setSkimFactor,
    showClicks,
    setShowClicks,
    showKeys,
    setShowKeys,
    crop,
    setCrop,
    framePreset,
    setFramePreset,
    bgPreset,
    setBgPreset,
    recoverable,
    setRecoverable,
    selected,
    setSelected,
    selExtra,
    setSelExtra,
    selKey,
    clearExtra,
    isSelected,
    selectionAll,
    selCount,
    toggleSelect,
    drag,
    setDrag,
    ensureRealId,
    stage,
    setStage,
    frameAspect,
    setFrameAspect,
    tlWidth,
    setTlWidth,
    tlScale,
    setTlScale,
    snapX,
    setSnapX,
    snapY,
    setSnapY,
    showExport,
    setShowExport,
    recordPhase,
    setRecordPhase,
    backdrop,
    setBackdrop,
    zoomAmount,
    setZoomAmount,
    update,
    setUpdate,
    updating,
    setUpdating,
    gpuLost,
    setGpuLost,
    showWelcome,
    setShowWelcome,
    showShortcuts,
    setShowShortcuts,
    recoveryBytes,
    setRecoveryBytes,
    clearingStorage,
    setClearingStorage,
    coachRecord,
    setCoachRecord,
    coachPos,
    setCoachPos,
    RECENTS_KEY,
    recents,
    setRecents,
    loadRecents,
    captureThumb,
    rememberRecent,
    removeRecent,
    recentSearch,
    setRecentSearch,
    openRecent,
    fmtAgo,
    hoverT,
    setHoverT,
    ghostT,
    setGhostT,
    onTlHoverMove,
    onTlHoverLeave,
    onTlPointerMove,
    frameTl,
    frameZoom,
    frameSpeed,
    frameCut,
    frameAnn,
    frameTrim,
    frameCanvas,
    preview,
    onContextMenu,
    BROWSER_CODES,
    onGlobalKey,
    onWheelGuard,
    hideSplash,
    connectEngine,
    checkForUpdate,
    runUpdate,
    maybeShowWelcome,
    dismissWelcome,
    pushSeek,
    scrub,
    reconcileAnns,
    refresh,
    refreshClip,
    tStart,
    tEnd,
    factorAt,
    tick,
    togglePlay,
    restart,
    nudgeSelected,
    onKey,
    norm,
    px,
    inWindow,
    inView,
    isGhost,
    pushEdit,
    patchAnn,
    geomOf,
    liveGeom,
    applyGeom,
    textWNorm,
    liveFont,
    TOL,
    handleAt,
    hitTest,
    CANVAS_SNAPS,
    snapMoveGeom,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    selectedText,
    selectedBox,
    selectedArrow,
    editStyle,
    setShape,
    setArrowStyle,
    setOpacity,
    isMask,
    inspTitle,
    selectedColor,
    setColor,
    editText,
    editFontSize,
    editTextStyle,
    selectedRange,
    editRange,
    deleteSelection,
    refreshAll,
    doUndo,
    doRedo,
    duplicateSelected,
    reorderSelected,
    clipboard,
    setClipboard,
    copySelected,
    pasteClipboard,
    editingTextAnn,
    editTextLive,
    finishTextEdit,
    showSource,
    setShowSource,
    sources,
    setSources,
    startRecord,
    beginRecordWith,
    recordTarget,
    setRecordTarget,
    onRecordFinished,
    onRecordCancel,
    onRecordFailed,
    loadFinishedClip,
    selectedZoom,
    addZoomAt,
    applyZoomEdit,
    deleteSelectedZoom,
    selZoomFocus,
    applyZoomFocus,
    applyZoomStyle,
    focusDrag,
    setFocusDrag,
    onFocusDown,
    onFocusMove,
    onFocusUp,
    skimSync,
    toggleSkim,
    applyCrop,
    centeredCrop,
    zoomStrength,
    setZoomStrength,
    planZoomAuto,
    selectedSpeed,
    addSpeedAtPlayhead,
    applySpeedEdit,
    deleteSelectedSpeed,
    selectedCut,
    addCutAtPlayhead,
    applyCutEdit,
    deleteSelectedCut,
    frameSync,
    applyFramePreset,
    BG_SWATCHES,
    bgSync,
    applyBackground,
    clicksSync,
    toggleClicks,
    keysSync,
    toggleKeys,
    refreshTlRect,
    invalidateTlRect,
    timeFromClientX,
    tlTime,
    tlSeekFromEvent,
    onTrimDown,
    onTrimMove,
    onTrimUp,
    zoomDrag,
    setZoomDrag,
    zoomGeom,
    onZoomDown,
    clampSegDrag,
    SNAP_PX,
    SNAP_STICKY,
    SNAP_PLAYHEAD,
    snapLine,
    setSnapLine,
    snapGrid,
    snapTargets,
    beginSnapGesture,
    endSnapGesture,
    snapProbes,
    snapSegDrag,
    onZoomMove,
    onZoomUp,
    speedDrag,
    setSpeedDrag,
    speedGeom,
    onSpeedDown,
    onSpeedMove,
    onSpeedUp,
    cutDrag,
    setCutDrag,
    cutGeom,
    onCutDown,
    onCutMove,
    onCutUp,
    pct,
    NICE_STEPS,
    fitPps,
    pxPerSec,
    TL_MAX_PPS,
    trackWidth,
    applyScaleAnchored,
    zoomTimeline,
    onTlWheel,
    tickStep,
    minorStep,
    tickMarks,
    annDrag,
    setAnnDrag,
    annGeom,
    onAnnDown,
    onAnnMove,
    onAnnUp,
    annBars,
    inspectorW,
    setInspectorW,
    onInspDown,
    onInspMove,
    onInspUp,
    somethingSelected,
    drawingToolActive,
    safeName,
    onSaveProject,
    onRecover,
    refreshStorage,
    clearStorage,
    onOpenProject,
  };
}

export type Editor = ReturnType<typeof createEditor>;
