import { createEffect, createSignal, onMount, onCleanup, For, Show } from "solid-js";
import { invoke, listen } from "./bridge";
import { dialogA11y } from "./dialog";
import { friendlyError } from "./format";
import { t } from "./i18n";
import { Icon } from "./icons";
import { stopKeys, zoomKeys } from "./hotkeys";
import { layout, prefs } from "./prefs";
import { Menu, type MenuItem, Rich, toast } from "./ui";
import { createPreviewClient } from "./preview";
import type { RecordTarget } from "./editor/recordFlow";
import {
  audioMenuItems,
  audioSummary,
  createAudioDevices,
  createLevels,
  LevelMeter,
  micName,
  useMicCheck,
} from "./components/AudioControls";
import {
  BUBBLE_MARGIN,
  BUBBLE_SIZE,
  CameraBubble,
  cameraMenuItems,
  cameraName,
  createCameras,
  pushCameraChoice,
  useCameraPreview,
  waitForCameraChoice,
} from "./components/CameraControls";
import { CURSOR_MODES, cursorSummary, pushCursorMode } from "./cursorMode";
import { COUNTDOWN_CHOICES, FPS_CHOICES, ZOOM_CHOICES } from "./recordOptions";
import { pushTakeDefaults } from "./takeDefaults";
import "./RecordOverlay.css";
import { keyCaps } from "./platform";

/** Mirrors src-tauri session::RecordingSummary. */
interface Summary {
  duration: number;
  frames: number;
  zooms: number;
  /** Set when the take was truncated (e.g. the disk filled mid-recording). */
  warning?: string | null;
}

type Preset = { id: string; label: string; hint: string; ratio: number | null | "full" };

// ratio = width / height. null = free draw. "full" = whole screen, no draw.
const PRESETS = (): Preset[] => [
  { id: "full", label: t("Full screen"), hint: t("Whole display"), ratio: "full" },
  { id: "16:9", label: "16:9", hint: "YouTube · Reddit · X", ratio: 16 / 9 },
  { id: "9:16", label: "9:16", hint: "Reels · TikTok · Shorts", ratio: 9 / 16 },
  { id: "1:1", label: "1:1", hint: "Instagram · Facebook", ratio: 1 },
  { id: "4:5", label: "4:5", hint: "Instagram · Facebook", ratio: 4 / 5 },
  { id: "free", label: t("Custom"), hint: t("Any size"), ratio: null },
];

type Rect = { x: number; y: number; w: number; h: number };
const fmt = (t: number) => {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
};

/** From this many pixels (a 3K screen) a take at 60 fps gets the offer of a lighter rate. */
const HEAVY_PIXELS = 6_000_000;
const RING_RADIUS = 36;
const RING_CIRC = 2 * Math.PI * RING_RADIUS;
type PanelMode = "mini" | "compact" | "large";

/**
 * The full recording flow as a single in-window overlay: pick a region (fullscreen) →
 * 3-2-1 countdown (small bar) → record + Stop. The host window is excluded from the
 * capture, so none of this UI appears in the recording. The editor mounts it for the
 * record flow's `framing` step (editor/recordFlow.ts); its own phases are below.
 *
 *   select ─Record─▶ preparing ─▶ countdown ─0─▶ recording ─Stop─▶ finalizing ─▶ onFinished
 *   preparing ─engine setup error─▶ select
 *   countdown ─capture failed to start─▶ onFailed
 *   finalizing ─save error─▶ onFailed (final: the engine already ended the take)
 *   Cancel / Esc in select, preparing or countdown, until capture starts ─▶ onCancel
 *
 * Alongside the phase, `control` (below) tracks whether control has been handed back, so an
 * engine call still in flight never carries a cancelled flow on.
 */
export default function RecordOverlay(props: {
  backdrop: string | null;
  zoom: number;
  /** What the take records: a display (region-selectable) or a whole app window. */
  target: RecordTarget;
  /** Which framing the selector opens on: the whole display or a drawn region. */
  initialMode?: "full" | "region";
  onZoomChange: (v: number) => void;
  onFinished: (s: Summary) => void;
  onCancel: () => void;
  /** A real failure (capture never started, finish errored), surfaces the backend reason. */
  onFailed: (message: string) => void;
}) {
  const [phase, setPhase] = createSignal<
    "select" | "preparing" | "countdown" | "recording" | "finalizing"
  >("select");
  // Custom is the default: the chip highlights "Custom" and Start stays disabled until the
  // user drags a region, so what's highlighted always matches what will actually record.
  const [preset, setPreset] = createSignal<Preset>(
    PRESETS().find((p) => p.id === (props.initialMode === "full" ? "full" : "free")) ?? PRESETS()[0],
  );
  const [sel, setSel] = createSignal<Rect | null>(null);
  // The area the last region take recorded is offered again: most people record the same
  // part of the screen take after take, and redrawing it by eye never lands on quite the
  // same pixels. It only fits the display (and picker size) it was drawn on, and it never
  // overrides anything the user has already done in this picker.
  let touched = false;
  const offerLastRegion = () => {
    const last = prefs.lastRegion();
    const target = props.target;
    if (touched || !last || phase() !== "select" || sel()) return;
    if (props.initialMode === "full" || target?.kind !== "display" || target.name !== last.display) return;
    if (Math.abs(window.innerWidth - last.vw) > 1 || Math.abs(window.innerHeight - last.vh) > 1) return;
    const fits = last.w >= 8 && last.h >= 8 && last.x >= 0 && last.y >= 0;
    if (!fits || last.x + last.w > last.vw + 1 || last.y + last.h > last.vh + 1) return;
    const chip = PRESETS().find((p) => p.id === last.preset && p.ratio !== "full");
    if (!chip) return;
    setPreset(chip);
    setSel({ x: last.x, y: last.y, w: last.w, h: last.h });
    setCursor("move");
  };
  const [count, setCount] = createSignal(prefs.countdown());
  const [elapsed, setElapsed] = createSignal(0);
  const [paused, setPaused] = createSignal(false);
  const [targetMeta, setTargetMeta] = createSignal("");
  // Audio: the mic check feeds the HUD meter while framing (and through "preparing", so the
  // check stays warm); during the take the engine's own capture feeds the panel meters.
  const audioDevices = createAudioDevices();
  const beforeTake = () => phase() === "select" || phase() === "preparing" || phase() === "countdown";
  useMicCheck(() => beforeTake() && prefs.recordMic());
  const levels = createLevels(() => (beforeTake() && prefs.recordMic()) || phase() === "recording");
  // Webcam: opened while framing so its bubble previews where it will sit in the take; the
  // same camera then records without reopening.
  const cameras = createCameras();
  const camera = useCameraPreview(() => beforeTake() && prefs.recordCamera());
  // Cursor over the selection surface, reflects what a press-drag would do (draw / move /
  // resize a given edge). Applied inline so it overrides the base crosshair.
  const [cursor, setCursor] = createSignal("crosshair");
  // The active pointer gesture on the selection surface. `null` when idle (hover only).
  let drag:
    | { mode: "new" | "move" | "resize"; hx: number; hy: number; px0: number; py0: number; rect0: Rect }
    | null = null;
  // Apart from what it shows (`phase`), where the overlay stands with the engine:
  //   "open"      before capture: Cancel / Esc hand control back to the editor
  //   "committed" capture is starting or running: too late to cancel, Stop ends the take
  //   "closed"    control was handed back. An engine call still in flight must not carry
  //               the flow on after this: shrink the window to the bar, raise the region
  //               border, count down, or start a capture nobody can see.
  //   open ─Cancel/Esc─▶ closed     open ─capture starts─▶ committed ─start/stop failed─▶ closed
  // Every await in the start chain checks it before going on. A signal, so Cancel can
  // show it no longer applies while capture is starting.
  const [control, setControl] = createSignal<"open" | "committed" | "closed">("open");
  let elapsedTimer: number | undefined;
  let countTimer: number | undefined;
  let startMs = 0;

  const clearCountdown = () => {
    if (countTimer) clearTimeout(countTimer);
    countTimer = undefined;
  };

  // Live "director's monitor": the backend streams a zoom-tracked preview to this canvas.
  const preview = createPreviewClient();
  const [canvas, setCanvas] = createSignal<HTMLCanvasElement | undefined>();
  let shotEl: HTMLImageElement | undefined;

  // Physical px per CSS px. The backdrop screenshot is an exact pixel map of the recorded
  // monitor stretched across the viewport, so its natural size over the viewport is the
  // true scale even when the window doesn't line up with the display exactly (e.g. a
  // work-area-sized "fullscreen"). Without a backdrop, fall back to devicePixelRatio.
  const toPhysical = () => {
    const dpr = window.devicePixelRatio || 1;
    return {
      sx: shotEl?.naturalWidth ? shotEl.naturalWidth / window.innerWidth : dpr,
      sy: shotEl?.naturalHeight ? shotEl.naturalHeight / window.innerHeight : dpr,
    };
  };

  const panelMode = (): PanelMode =>
    !prefs.livePreview() ? "mini" : layout.panelLarge() ? "large" : "compact";

  const stopTimer = () => {
    if (elapsedTimer) clearTimeout(elapsedTimer);
    elapsedTimer = undefined;
  };

  const startElapsedTimer = () => {
    stopTimer();
    const tick = () => {
      elapsedTimer = undefined;
      if (phase() !== "recording" || paused() || control() === "closed" || document.hidden) return;
      setElapsed((Date.now() - startMs) / 1000);
      elapsedTimer = window.setTimeout(tick, 200);
    };
    if (!document.hidden) elapsedTimer = window.setTimeout(tick, 200);
  };

  /** Hand control back to the editor: the backend flow is undone, then `then` reports why. */
  const close = (then: () => void) => {
    setControl("closed");
    clearCountdown();
    stopTimer();
    void invoke("cancel_record_flow").finally(then);
  };

  const cancel = () => {
    if (control() !== "open") return;
    close(() => props.onCancel());
  };

  const beginCountdown = async () => {
    if (control() !== "open") return; // cancelled, waiting for the editor to take over
    const p = preset();
    const r = sel();
    const windowMode = props.target?.kind === "window";
    // Any non-full preset without a drawn region (Custom included) must never silently
    // record full screen.
    if (!windowMode && p.ratio !== "full" && (!r || r.w < 8 || r.h < 8)) return;
    stillRegion = null; // recomputed below; never reuse a previous attempt's rect
    setCount(prefs.countdown());
    setPhase("preparing"); // instant acknowledgment on the HUD button; panel waits until resized
    try {
      await waitForCameraChoice();
      if (control() !== "open") return;
      if (windowMode) {
        // Window capture records the whole client area; the backdrop (when captured) is
        // already cropped to that window's client rect.
        if (shotEl?.naturalWidth && shotEl.naturalHeight) {
          stillRegion = { x: 0, y: 0, w: shotEl.naturalWidth, h: shotEl.naturalHeight };
        }
        const label = props.target?.kind === "window" ? props.target.label : "";
        setTargetMeta(label ? `${label} · ${prefs.captureFps()} fps` : `${prefs.captureFps()} fps`);
      } else if (p.ratio === "full" || !r) {
        const { sx, sy } = toPhysical();
        stillRegion = {
          x: 0,
          y: 0,
          w: Math.round(window.innerWidth * sx),
          h: Math.round(window.innerHeight * sy),
        };
        setTargetMeta(`${stillRegion.w} × ${stillRegion.h} · ${prefs.captureFps()} fps`);
        await invoke("set_region", {}); // no fields → full screen
        if (control() !== "open") return;
      } else {
        // Resolved to backdrop pixels NOW, while the overlay is still fullscreen and
        // `toPhysical` maps the viewport 1:1 onto the shot — after `enter_stopbar` shrinks
        // the window to the panel this scale is gone.
        const { sx, sy } = toPhysical();
        stillRegion = {
          x: Math.round(r.x * sx),
          y: Math.round(r.y * sy),
          w: Math.round(r.w * sx),
          h: Math.round(r.h * sy),
        };
        setTargetMeta(`${stillRegion.w} × ${stillRegion.h} · ${prefs.captureFps()} fps`);
        if (props.target?.kind === "display") {
          prefs.lastRegion.set({
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            vw: window.innerWidth,
            vh: window.innerHeight,
            preset: p.id,
            display: props.target.name,
          });
        }
        await invoke("set_region", {
          x: stillRegion.x,
          y: stillRegion.y,
          w: stillRegion.w,
          h: stillRegion.h,
        });
        if (control() !== "open") return;
      }
      // Shrink and park the host window BEFORE leaving the selector phase so the floating
      // panel never expands fullscreen over the desktop.
      await invoke("enter_stopbar", { mode: panelMode(), large: layout.panelLarge() });
      // Cancelled while the engine set up: the cancel's restore ran after the shrink (the
      // commands run in order), so stop here and leave the editor window as it now is.
      if (control() !== "open") return;
      // Show the recorded-region frame as the 3-2-1 begins, so the user sees exactly what's
      // in frame before capture starts. Idempotent + a no-op for full-screen on the Rust side.
      // Best-effort: the command may not exist on older backends, the frame still appears
      // when recording starts. Cancel/Esc runs cancel_record_flow, which clears it.
      try {
        await invoke("show_region_border");
      } catch {
        /* backend without show_region_border, border still shows at record start */
      }
      if (control() !== "open") return;
      setPhase("countdown");
      if (count() <= 0) {
        void beginRecording();
        return;
      }
      runCountdown();
    } catch (e) {
      if (control() !== "open") return; // cancelled meanwhile: the editor already has control
      setPhase("select");
      toast(t("Could not start recording: {why}", { why: friendlyError(e) }), "error");
    }
  };

  const runCountdown = () => {
    const tick = () => {
      countTimer = undefined;
      // Cancelled (Cancel/Esc) during the 3-2-1: stop here, never start the recording.
      if (phase() !== "countdown" || control() !== "open") return;
      const c = count() - 1;
      if (c <= 0) {
        setCount(0);
        void beginRecording();
      } else {
        setCount(c);
        countTimer = window.setTimeout(tick, 1000);
      }
    };
    countTimer = window.setTimeout(tick, 1000);
  };

  const startNow = () => {
    if (phase() !== "countdown" || control() !== "open") return;
    clearCountdown();
    setCount(0);
    void beginRecording();
  };

  // Bind the panel canvas to the live preview stream whenever monitor mode is active.
  // Toggling Mini Bar disconnects the WebSocket and cancels rAF work; toggling Live
  // Preview back on attaches the newly mounted canvas immediately. During countdown the
  // still is painted after `connect()` settles (the mock client resets on connect).
  createEffect(() => {
    const p = phase();
    const el = canvas();
    const active = (p === "countdown" || p === "recording") && prefs.livePreview();
    if (!active || !el) {
      preview.disconnect();
      return;
    }
    preview.attach(el);
    const paint = () => {
      if (phase() === "countdown") paintStill();
    };
    let cancelled = false;
    void invoke<{ port: number; token: string }>("preview_port")
      .then((conn) => {
        if (cancelled) return;
        preview.connect(conn.port, conn.token);
        paint();
      })
      .catch(() => {
        if (!cancelled) paint();
      });
    onCleanup(() => {
      cancelled = true;
    });
  });

  // The region that will record, in backdrop (natural image) pixels, captured while the
  // overlay is still fullscreen. `null` = no still to show.
  let stillRegion: { x: number; y: number; w: number; h: number } | null = null;

  // Draw the frozen backdrop cropped to the recorded region into the preview canvas at
  // the stream's own resolution (matching `live_preview::PREVIEW_WIDTH`).
  const paintStill = () => {
    const el = canvas();
    if (!el || !shotEl?.naturalWidth || !stillRegion) return;
    const ctx = el.getContext("2d");
    if (!ctx) return;
    const aspect = stillRegion.w / stillRegion.h;
    const w = 640;
    const h = Math.max(1, Math.round(w / aspect));
    el.width = w;
    el.height = h;
    ctx.drawImage(
      shotEl,
      stillRegion.x,
      stillRegion.y,
      stillRegion.w,
      stillRegion.h,
      0,
      0,
      w,
      h,
    );
  };

  const beginRecording = async () => {
    if (control() !== "open") return;
    setControl("committed");
    try {
      await waitForCameraChoice();
      await invoke("set_zoom_amount", { amount: props.zoom });
      await invoke("start_recording");
      setPhase("recording");
      startMs = Date.now();
      startElapsedTimer();
    } catch (e) {
      // Capture never started, clean up the backend flow, then surface the real reason
      // (e.g. "No frames were captured…") instead of a bland "cancelled".
      close(() => props.onFailed(String(e)));
    }
  };

  const stop = async () => {
    // The Stop button and the global hotkey can race: the first one moves the phase on.
    if (phase() !== "recording") return;
    stopTimer();
    setPhase("finalizing"); // synchronous "Finishing recording..." so the panel never
    // looks like it is still capturing while the take is being written.
    try {
      const summary = await invoke<Summary>("finish_recording");
      props.onFinished(summary);
    } catch (e) {
      // A failed stop is final: the engine takes the take out before anything can fail, and
      // finish_recording restores the editor window either way. So hand back with the real
      // reason (e.g. "No frames were captured…"); there is no live take to go back to.
      setControl("closed");
      props.onFailed(String(e));
    }
  };

  // Pause/resume: capture keeps running, the paused span becomes a cut at stop time.
  // The elapsed counter freezes so the readout matches what will actually be kept.
  const togglePause = async () => {
    const next = !paused();
    try {
      await invoke("set_record_paused", { paused: next });
    } catch {
      return; // not recording (race with stop), leave the UI as is
    }
    setPaused(next);
    if (next) {
      stopTimer();
    } else {
      startMs = Date.now() - elapsed() * 1000;
      startElapsedTimer();
    }
  };

  // ── region select + adjust (select phase) ───────────────────────────────────────
  // The 8 resize handles, addressed as (hx, hy) in {-1, 0, 1}: -1 = left/top edge,
  // 1 = right/bottom edge, 0 = centre of that axis. (0, 0) is the interior (move), not a handle.
  const HANDLES: { hx: -1 | 0 | 1; hy: -1 | 0 | 1 }[] = [
    { hx: -1, hy: -1 }, { hx: 0, hy: -1 }, { hx: 1, hy: -1 },
    { hx: -1, hy: 0 }, { hx: 1, hy: 0 },
    { hx: -1, hy: 1 }, { hx: 0, hy: 1 }, { hx: 1, hy: 1 },
  ];
  const HANDLE_HIT = 12; // px half-extent for grabbing a handle

  type Hit =
    | { mode: "resize"; hx: -1 | 0 | 1; hy: -1 | 0 | 1 }
    | { mode: "move" }
    | { mode: "new" };

  // What a press at (px, py) would start: grab a handle, move the region, or draw a fresh one.
  const hitTest = (px: number, py: number): Hit => {
    const r = sel();
    if (!r || preset().ratio === "full") return { mode: "new" };
    const xs: [-1 | 0 | 1, number][] = [
      [-1, r.x],
      [0, r.x + r.w / 2],
      [1, r.x + r.w],
    ];
    const ys: [-1 | 0 | 1, number][] = [
      [-1, r.y],
      [0, r.y + r.h / 2],
      [1, r.y + r.h],
    ];
    for (const [hy, ay] of ys) {
      for (const [hx, ax] of xs) {
        if (hx === 0 && hy === 0) continue; // interior, not a handle
        if (Math.abs(px - ax) <= HANDLE_HIT && Math.abs(py - ay) <= HANDLE_HIT) {
          return { mode: "resize", hx, hy };
        }
      }
    }
    if (px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h) return { mode: "move" };
    return { mode: "new" };
  };

  const cursorFor = (h: Hit): string => {
    if (h.mode === "move") return "move";
    if (h.mode === "new") return "crosshair";
    if (h.hx !== 0 && h.hy !== 0) return h.hx === h.hy ? "nwse-resize" : "nesw-resize";
    return h.hx !== 0 ? "ew-resize" : "ns-resize";
  };

  // Keep a rect inside the monitor (the viewport maps 1:1 to the recorded display). Clamps
  // dimensions to viewport bounds and slides the rect back in bounds.
  const clampToView = (r: Rect): Rect => {
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const w = Math.min(r.w, vw);
    const h = Math.min(r.h, vh);
    let { x, y } = r;
    if (x + w > vw) x = vw - w;
    if (y + h > vh) y = vh - h;
    return { x: Math.max(0, x), y: Math.max(0, y), w, h };
  };

  // Minimum region in CSS px (>= 64 physical px, above the Rust MIN_PX=8 sliver guard).
  const minCss = () => {
    const { sx, sy } = toPhysical();
    return { w: 64 / (sx || 1), h: 64 / (sy || 1) };
  };

  const resizeRect = (px: number, py: number) => {
    const { hx, hy, rect0 } = drag!;
    const ratio = preset().ratio;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const cx = Math.max(0, Math.min(px, vw));
    const cy = Math.max(0, Math.min(py, vh));
    // The anchor is the fixed point opposite the grabbed handle (an edge or the centre).
    const ax = hx === 1 ? rect0.x : hx === -1 ? rect0.x + rect0.w : rect0.x + rect0.w / 2;
    const ay = hy === 1 ? rect0.y : hy === -1 ? rect0.y + rect0.h : rect0.y + rect0.h / 2;
    const { w: minW, h: minH } = minCss();

    if (typeof ratio === "number") {
      // Fixed aspect: derive size on the driving axis, keep ratio, grow from the anchor.
      let w: number;
      if (hx !== 0 && hy !== 0) w = Math.max(Math.abs(cx - ax), Math.abs(cy - ay) * ratio);
      else if (hx !== 0) w = Math.abs(cx - ax);
      else w = Math.abs(cy - ay) * ratio;
      const mW = Math.max(minW, minH * ratio);
      if (w < mW) w = mW;
      const h = w / ratio;
      const x = hx === 1 ? ax : hx === -1 ? ax - w : ax - w / 2;
      const y = hy === 1 ? ay : hy === -1 ? ay - h : ay - h / 2;
      setSel(clampToView({ x, y, w, h }));
      return;
    }

    // Free draw: each active edge follows the pointer; clamp at min without flipping.
    let { x, y, w, h } = rect0;
    if (hx === 1) {
      x = ax;
      w = cx - ax;
    } else if (hx === -1) {
      x = cx;
      w = ax - cx;
    }
    if (hy === 1) {
      y = ay;
      h = cy - ay;
    } else if (hy === -1) {
      y = cy;
      h = ay - cy;
    }
    if (w < minW) {
      w = minW;
      if (hx === -1) x = ax - minW;
    }
    if (h < minH) {
      h = minH;
      if (hy === -1) y = ay - minH;
    }
    setSel(clampToView({ x, y, w, h }));
  };

  const onDown = (e: PointerEvent) => {
    if (phase() !== "select" || preset().ratio === "full" || props.target?.kind === "window") return;
    touched = true;
    try { (e.currentTarget as Element).setPointerCapture(e.pointerId); } catch { /* synthetic pointer */ }
    const hit = hitTest(e.clientX, e.clientY);
    if (hit.mode === "new") {
      drag = { mode: "new", hx: 0, hy: 0, px0: e.clientX, py0: e.clientY, rect0: { x: e.clientX, y: e.clientY, w: 0, h: 0 } };
      setSel({ x: e.clientX, y: e.clientY, w: 0, h: 0 });
    } else {
      drag = {
        mode: hit.mode,
        hx: hit.mode === "resize" ? hit.hx : 0,
        hy: hit.mode === "resize" ? hit.hy : 0,
        px0: e.clientX,
        py0: e.clientY,
        rect0: { ...sel()! },
      };
    }
    setCursor(cursorFor(hit));
  };

  const onMove = (e: PointerEvent) => {
    if (!drag) {
      // Idle hover: show what a press here would do.
      if (phase() === "select" && preset().ratio !== "full") setCursor(cursorFor(hitTest(e.clientX, e.clientY)));
      return;
    }
    if (drag.mode === "new") {
      // Held to the display, and a fixed shape is fitted to the room there is: a region
      // hanging off the edge is clamped by the capture, and would lose its shape.
      const ratio = preset().ratio;
      const px = Math.min(Math.max(e.clientX, 0), window.innerWidth);
      const py = Math.min(Math.max(e.clientY, 0), window.innerHeight);
      const up = py < drag.py0;
      let w = Math.abs(px - drag.px0);
      let h = Math.abs(py - drag.py0);
      if (typeof ratio === "number") {
        h = Math.min(w / ratio, up ? drag.py0 : window.innerHeight - drag.py0);
        w = h * ratio;
      }
      setSel({ x: px < drag.px0 ? drag.px0 - w : drag.px0, y: up ? drag.py0 - h : drag.py0, w, h });
    } else if (drag.mode === "move") {
      const r0 = drag.rect0;
      setSel(clampToView({ x: r0.x + (e.clientX - drag.px0), y: r0.y + (e.clientY - drag.py0), w: r0.w, h: r0.h }));
    } else {
      resizeRect(e.clientX, e.clientY);
    }
  };

  const onUp = () => {
    // A near-zero "new" drag (a stray click outside the region) clears the selection rather
    // than leaving a sliver; a real drag/resize/move keeps its result for further adjustment.
    if (drag?.mode === "new") {
      const r = sel();
      if (r && (r.w < 4 || r.h < 4)) setSel(null);
    }
    drag = null;
  };

  const [vp, setVp] = createSignal({ w: window.innerWidth, h: window.innerHeight });
  const [liveStatus, setLiveStatus] = createSignal("");
  let selRootEl: HTMLDivElement | undefined;
  let tagEl: HTMLDivElement | undefined;
  const [tagSize, setTagSize] = createSignal({ w: 120, h: 26 });

  createEffect(() => {
    const ph = phase();
    if (ph === "countdown") {
      const c = count();
      setLiveStatus(c > 0 ? t("Recording in {n}", { n: c }) : t("Recording started"));
    } else if (ph === "finalizing") {
      setLiveStatus(t("Saving take…"));
    } else {
      setLiveStatus("");
    }
  });

  createEffect(() => {
    void sel();
    queueMicrotask(() => {
      if (!tagEl) return;
      const r = tagEl.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) setTagSize({ w: r.width, h: r.height });
    });
  });

  const clampedTag = (r: Rect) => {
    const pad = 8;
    const { w: vw, h: vh } = vp();
    const tw = Math.min(tagSize().w, Math.max(80, vw - pad * 2));
    const th = tagSize().h;
    const x = Math.max(pad, Math.min(vw - tw - pad, r.x));
    const preferTop = r.y - th - 6;
    const y = preferTop >= pad ? preferTop : Math.max(pad, Math.min(vh - th - pad, r.y + 8));
    return { x, y };
  };

  /** Flip the selector HUD to the top edge whenever a low selection would collide with it. */
  const hudAtTop = () => {
    const r = sel();
    if (!r || preset().ratio === "full" || windowMode()) return false;
    return r.y + r.h > vp().h - 76;
  };

  /** Commit physical pixel W or H entered in the HUD numeric inputs through the same bounds/ratio path. */
  const commitPhysDim = (axis: "w" | "h", raw: string, inputEl?: HTMLInputElement) => {
    const r = sel();
    if (!r) return;
    const { sx, sy } = toPhysical();
    const scaleX = Math.max(sx, 0.1);
    const scaleY = Math.max(sy, 0.1);
    const val = Number(raw);
    if (!Number.isFinite(val) || val <= 0) {
      if (inputEl) inputEl.value = String(Math.round(axis === "w" ? r.w * sx : r.h * sy));
      return;
    }
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const { w: minW, h: minH } = minCss();
    const ratio = preset().ratio;
    let w = r.w;
    let h = r.h;
    if (typeof ratio === "number") {
      const loW = Math.min(vw, Math.max(minW, minH * ratio));
      const hiW = Math.max(loW, Math.min(vw, vh * ratio));
      const desiredW = axis === "w" ? val / scaleX : (val / scaleY) * ratio;
      w = Math.max(loW, Math.min(hiW, desiredW));
      h = w / ratio;
    } else if (axis === "w") {
      w = Math.max(minW, Math.min(vw, val / scaleX));
    } else {
      h = Math.max(minH, Math.min(vh, val / scaleY));
    }
    const next = clampToView({ x: r.x, y: r.y, w, h });
    setSel(next);
    if (inputEl) {
      inputEl.value = String(Math.round(axis === "w" ? next.w * sx : next.h * sy));
    }
  };

  const onKey = (e: KeyboardEvent) => {
    const tag = (e.target as HTMLElement | null)?.tagName;
    const inInput = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
    // Esc aborts while picking, preparing, and during the 3-2-1 countdown.
    if (
      e.key === "Escape" &&
      (phase() === "select" || phase() === "countdown" || phase() === "preparing")
    ) {
      cancel();
      return;
    }
    if (inInput) return;
    if (phase() === "countdown" && e.key === "Enter" && control() === "open") {
      startNow();
      return;
    }
    if (phase() !== "select") return;
    // Arrow keys move the active region by 1 physical pixel; Shift+Arrow resizes the trailing edge.
    if (
      (e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "ArrowUp" || e.key === "ArrowDown") &&
      (e.target === selRootEl || e.target === document.body)
    ) {
      const r = sel();
      if (r && preset().ratio !== "full" && !windowMode()) {
        e.preventDefault();
        touched = true;
        const { sx, sy } = toPhysical();
        const stepX = 1 / Math.max(sx, 0.1);
        const stepY = 1 / Math.max(sy, 0.1);
        const dx = e.key === "ArrowLeft" ? -stepX : e.key === "ArrowRight" ? stepX : 0;
        const dy = e.key === "ArrowUp" ? -stepY : e.key === "ArrowDown" ? stepY : 0;
        if (!e.shiftKey) {
          setSel(clampToView({ ...r, x: r.x + dx, y: r.y + dy }));
        } else {
          const vw = window.innerWidth;
          const vh = window.innerHeight;
          const { w: minW, h: minH } = minCss();
          const ratio = preset().ratio;
          let w = Math.max(minW, Math.min(vw, r.w + dx));
          let h = Math.max(minH, Math.min(vh, r.h + dy));
          if (typeof ratio === "number") {
            const loW = Math.min(vw, Math.max(minW, minH * ratio));
            const hiW = Math.max(loW, Math.min(vw, vh * ratio));
            const desiredW = dx !== 0 ? w : h * ratio;
            w = Math.max(loW, Math.min(hiW, desiredW));
            h = w / ratio;
          }
          setSel(clampToView({ x: r.x, y: r.y, w, h }));
        }
        return;
      }
    }
    if (e.key === "Enter" && tag !== "BUTTON") {
      const p = preset();
      const r = sel();
      if (props.target?.kind === "window") {
        void beginCountdown();
      } else if (p.ratio === "full" || (r && r.w >= 8 && r.h >= 8)) {
        void beginCountdown();
      }
    }
  };
  // The picker opens inside the editor's window and the engine then grows it over the
  // display, so the last region is offered once it has: when the backdrop arrives, and on
  // any resize in case no backdrop could be grabbed.
  createEffect(() => {
    if (props.backdrop) queueMicrotask(offerLastRegion);
  });
  onMount(() => {
    const onResize = () => {
      setVp({ w: window.innerWidth, h: window.innerHeight });
      offerLastRegion();
    };
    window.addEventListener("resize", onResize);
    onCleanup(() => window.removeEventListener("resize", onResize));
    window.addEventListener("keydown", onKey);
    const onVisibility = () => {
      if (document.hidden) {
        stopTimer();
      } else if (phase() === "recording" && !paused()) {
        setElapsed((Date.now() - startMs) / 1000);
        startElapsedTimer();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    // The global stop hotkey (watched by the backend while recording) stops the recording
    // even when this panel doesn't have focus.
    const unlistenStop = listen("stop-hotkey", () => {
      if (phase() === "recording") void stop();
    });
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("visibilitychange", onVisibility);
      void unlistenStop.then((un) => un());
      clearCountdown();
      stopTimer();
      preview.disconnect();
    });
  });

  const pickPreset = (p: Preset) => {
    touched = true;
    setPreset(p);
    drag = null;
    if (p.ratio === "full" || p.ratio === null || props.target?.kind === "window") {
      // Full screen and window targets need no rectangle; Custom starts empty (Start
      // stays disabled until the user draws), so the highlighted chip ALWAYS matches
      // what will actually record.
      setSel(null);
      setCursor(p.ratio === "full" || props.target?.kind === "window" ? "default" : "crosshair");
      return;
    }
    // Seed a centered 2/3-width rectangle in the chosen aspect so the highlight on the
    // chip matches a real, visible region from the first frame.
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const w = Math.round(vw * 0.66);
    const h = Math.round(w / p.ratio);
    setSel(clampToView({ x: Math.round((vw - w) / 2), y: Math.round(Math.max(0, (vh - h) / 2 - 40)), w, h }));
    setCursor("move");
  };
  const dims = () => {
    const r = sel();
    if (preset().ratio === "full") return t("Full screen");
    if (!r) return t("No area yet");
    const { sx, sy } = toPhysical();
    return `${Math.round(r.w * sx)} × ${Math.round(r.h * sy)} px`;
  };

  // A big area at the top frame rate is hard work for a mid-range PC. Better to offer the
  // way out here than to report lost frames after the take.
  const heavyTake = () => {
    const r = preset().ratio === "full" ? null : sel();
    const { sx, sy } = toPhysical();
    const w = r ? r.w * sx : window.innerWidth * sx;
    const h = r ? r.h * sy : window.innerHeight * sy;
    return w * h >= HEAVY_PIXELS && prefs.captureFps() >= 60 && (navigator.hardwareConcurrency || 8) < 12;
  };
  const windowMode = () => props.target?.kind === "window";
  const canStart = () => windowMode() || preset().ratio === "full" || !!sel();
  /** Show or hide the live picture. Applies at once, also mid-take: off, the engine stops
   *  copying preview frames and collapses the floating panel into the slim Mini Bar. */
  const setLivePreview = (on: boolean) => {
    prefs.livePreview.set(on);
    void invoke("set_live_preview", { on }).catch(() => undefined);
    if (phase() !== "select" && phase() !== "preparing") {
      void invoke("set_panel_size", { mode: panelMode(), large: layout.panelLarge() }).catch(() => undefined);
    }
  };
  /** Switch the live monitor between its compact size and a bigger preview. */
  const togglePanelSize = () => {
    const next = !layout.panelLarge();
    layout.panelLarge.set(next);
    if (phase() !== "select" && phase() !== "preparing") {
      void invoke("set_panel_size", { mode: next ? "large" : "compact", large: next }).catch(() => undefined);
    }
  };
  /** The bubble's spot: the bottom right of the recorded area, sized like a new take's. */
  const bubbleSpot = () => {
    const whole = windowMode() || preset().ratio === "full";
    const r = whole ? { x: 0, y: 0, w: window.innerWidth, h: window.innerHeight } : sel();
    if (!r) return null;
    const size = Math.max(44, r.h * BUBBLE_SIZE);
    const m = r.h * BUBBLE_MARGIN;
    return { left: r.x + r.w - m - size, top: r.y + r.h - m - size, size };
  };

  const ringOffset = () => RING_CIRC * (1 - Math.max(0, count()) / Math.max(1, prefs.countdown()));

  const optionsMenuItems = (): MenuItem[] => [
    { heading: t("Zoom") },
    {
      label: t("Auto zoom where I click"),
      checked: prefs.recordAutoZoom(),
      onSelect: () => {
        prefs.recordAutoZoom.set(!prefs.recordAutoZoom());
        pushTakeDefaults();
      },
    },
    ...ZOOM_CHOICES().map((z) => ({
      label: z.value === 1 ? t("No zoom") : t("Zoom {label}", { label: z.label }),
      checked: Math.abs(props.zoom - z.value) < 0.001,
      onSelect: () => props.onZoomChange(z.value),
    })),
    { heading: t("Frame rate") },
    ...FPS_CHOICES().map((f) => ({
      label: f.label,
      checked: prefs.captureFps() === f.value,
      onSelect: () => {
        prefs.captureFps.set(f.value);
        void invoke("set_capture_fps", { fps: f.value }).catch(() => undefined);
      },
    })),
    { heading: t("Mouse pointer") },
    ...CURSOR_MODES().map((c) => ({
      label: c.menu,
      checked: prefs.cursorMode() === c.value,
      onSelect: () => {
        prefs.cursorMode.set(c.value);
        pushCursorMode(c.value);
      },
    })),
    { heading: t("Recording panel") },
    {
      label: t("Show live preview"),
      checked: prefs.livePreview(),
      onSelect: () => setLivePreview(!prefs.livePreview()),
    },
    { heading: t("Countdown") },
    ...COUNTDOWN_CHOICES().map((c) => ({
      label: c.value === 0 ? t("Start immediately") : t("{n} seconds", { n: c.value }),
      checked: prefs.countdown() === c.value,
      onSelect: () => prefs.countdown.set(c.value),
    })),
  ];

  return (
    <Show
      when={phase() === "select" || phase() === "preparing"}
      fallback={
        <div class="rec-panel-root">
          <span class="sr-only" role="status" aria-live="polite">
            {liveStatus()}
          </span>
          <div
            class="rec-panel"
            classList={{
              live: phase() === "recording",
              paused: paused(),
              large: prefs.livePreview() && layout.panelLarge(),
            }}
          >
            <Show
              when={prefs.livePreview()}
              fallback={
                <div class="rec-mini" data-tauri-drag-region>
                  <span class="rec-grip" data-tauri-drag-region aria-hidden="true">
                    <Icon name="grip" size={14} />
                  </span>
                  <Show
                    when={phase() === "recording"}
                    fallback={
                      <Show
                        when={phase() === "finalizing"}
                        fallback={
                          <>
                            <span class="rec-mini-badge" data-tauri-drag-region>
                              {count() > 0 ? count() : t("Go")}
                            </span>
                            <span class="rec-mini-label" data-tauri-drag-region>
                              {t("Get ready")}
                            </span>
                            <span class="rec-mini-drag" data-tauri-drag-region />
                            <button
                              type="button"
                              class="rec-icon-btn"
                              aria-label={t("Show live preview")}
                              onClick={() => setLivePreview(true)}
                            >
                              <Icon name="eye" size={14} />
                            </button>
                            <button
                              type="button"
                              class="rec-start-now mini"
                              disabled={control() !== "open"}
                              onClick={startNow}
                            >
                              {t("Start")}
                            </button>
                            <button
                              type="button"
                              class="rec-cancel mini"
                              disabled={control() !== "open"}
                              onClick={cancel}
                            >
                              {t("Cancel")}
                            </button>
                          </>
                        }
                      >
                        <span class="spinner rec-mini-spin" aria-hidden="true" />
                        <span class="rec-mini-label" data-tauri-drag-region>
                          {t("Saving take…")}
                        </span>
                        <span class="rec-mini-drag" data-tauri-drag-region />
                      </Show>
                    }
                  >
                    <span class="live-dot" data-tauri-drag-region />
                    <span class="rec-mini-time" classList={{ paused: paused() }} aria-live="off" data-tauri-drag-region>
                      {fmt(elapsed())}
                    </span>
                    <Show when={prefs.recordMic() || prefs.recordSystem()}>
                      <div class="rec-mini-meters" data-tauri-drag-region>
                        <Show when={prefs.recordMic()}>
                          <LevelMeter class="rec-mini-meter" level={paused() ? 0 : levels.mic()} />
                        </Show>
                        <Show when={prefs.recordSystem()}>
                          <LevelMeter class="rec-mini-meter" level={paused() ? 0 : levels.system()} />
                        </Show>
                      </div>
                    </Show>
                    <span class="rec-mini-drag" data-tauri-drag-region />
                    <button
                      type="button"
                      class="rec-icon-btn"
                      aria-label={t("Show live preview")}
                      onClick={() => setLivePreview(true)}
                    >
                      <Icon name="eye" size={14} />
                    </button>
                    <button
                      type="button"
                      class="rec-pause mini"
                      classList={{ on: paused() }}
                      aria-label={paused() ? t("Resume recording") : t("Pause recording")}
                      onClick={() => void togglePause()}
                    >
                      <Icon name={paused() ? "play" : "pause"} size={13} />
                      {paused() ? t("Resume") : t("Pause")}
                    </button>
                    <button
                      type="button"
                      class="rec-stop mini"
                      aria-label={t("Stop recording")}
                      onClick={() => void stop()}
                    >
                      <span class="rec-stop-square" />
                      {t("Stop")}
                    </button>
                  </Show>
                </div>
              }
            >
              <div class="rec-drag" data-tauri-drag-region>
                <div class="rec-drag-start" data-tauri-drag-region>
                  <span class="rec-grip" data-tauri-drag-region aria-hidden="true">
                    <Icon name="grip" size={14} />
                  </span>
                  <span class="rec-state" data-tauri-drag-region>
                    <span class="rec-state-label" data-tauri-drag-region>
                      <Show when={phase() === "recording"} fallback={<span class="live-dot ready" />}>
                        <span class="live-dot" />
                      </Show>
                      <span data-tauri-drag-region>
                        {phase() === "recording"
                          ? paused()
                            ? t("Paused")
                            : t("Recording")
                          : phase() === "finalizing"
                            ? t("Saving take…")
                            : t("Get ready")}
                      </span>
                    </span>
                  </span>
                  <Show when={targetMeta()}>
                    {(meta) => (
                      <span class="rec-meta" data-tauri-drag-region>
                        {meta()}
                      </span>
                    )}
                  </Show>
                </div>
                <div class="rec-drag-end">
                  <button
                    type="button"
                    class="rec-icon-btn"
                    aria-label={t("Hide live preview")}
                    data-tip={t("Collapse to mini bar (hides live preview)")}
                    onClick={() => setLivePreview(false)}
                  >
                    <Icon name="eyeOff" size={14} />
                  </button>
                  <button
                    type="button"
                    class="rec-icon-btn"
                    aria-label={layout.panelLarge() ? t("Make the panel smaller") : t("Make the preview bigger")}
                    data-tip={
                      layout.panelLarge()
                        ? t("Smaller panel (or double-click the preview)")
                        : t("Bigger preview (or double-click the preview)")
                    }
                    onClick={togglePanelSize}
                  >
                    <Icon name={layout.panelLarge() ? "shrink" : "grow"} size={14} />
                  </button>
                </div>
              </div>
              <div class="rec-screen" onDblClick={togglePanelSize}>
                <canvas
                  ref={(el) => {
                    setCanvas(el);
                    onCleanup(() => {
                      if (canvas() === el) setCanvas(undefined);
                    });
                  }}
                  class="rec-canvas"
                />
                <Show when={phase() === "countdown"}>
                  <div class="rec-countdown">
                    <div class="rec-ring-wrap">
                      <svg class="rec-ring-svg" viewBox="0 0 84 84" aria-hidden="true">
                        <circle class="rec-ring-track" cx="42" cy="42" r={RING_RADIUS} />
                        <circle
                          class="rec-ring-arc"
                          cx="42"
                          cy="42"
                          r={RING_RADIUS}
                          stroke-dasharray={String(RING_CIRC)}
                          stroke-dashoffset={String(ringOffset())}
                        />
                      </svg>
                      <span class="rec-num">{count() > 0 ? count() : t("Go")}</span>
                    </div>
                    <span class="rec-sub">
                      {preset().ratio === "full" ? t("Vuoom hides from the recording") : t("Recording starts…")}
                    </span>
                  </div>
                </Show>
                <Show when={phase() === "recording" && paused()}>
                  <div class="rec-paused-veil">
                    <span class="rec-paused-badge">{t("Paused")}</span>
                    <span class="rec-sub">{t("This gap is cut from the take")}</span>
                    <button type="button" class="rec-resume-pill" onClick={() => void togglePause()}>
                      <Icon name="play" size={12} />
                      {t("Resume")}
                    </button>
                  </div>
                </Show>
                <Show when={phase() === "recording" && !paused() && props.zoom > 1}>
                  <span class="rec-previewtag">
                    <Rich
                      text={t("{keys} zooms {amount}×", { amount: props.zoom.toFixed(1) })}
                      parts={{ keys: () => <For each={keyCaps(zoomKeys())}>{(k) => <kbd>{k}</kbd>}</For> }}
                    />
                  </span>
                </Show>
                <Show when={phase() === "recording" && (prefs.recordMic() || prefs.recordSystem())}>
                  <div class="rec-audio">
                    <Show when={prefs.recordMic()}>
                      <span class="rec-audio-row" data-tip={t("Microphone level")}>
                        <Icon name="mic" size={11} />
                        <LevelMeter level={paused() ? 0 : levels.mic()} />
                      </span>
                    </Show>
                    <Show when={prefs.recordSystem()}>
                      <span class="rec-audio-row" data-tip={t("System sound level")}>
                        <Icon name="volume" size={11} />
                        <LevelMeter level={paused() ? 0 : levels.system()} />
                      </span>
                    </Show>
                  </div>
                </Show>
              </div>
              <div class="rec-controls">
                <Show
                  when={phase() === "recording"}
                  fallback={
                    <Show
                      when={phase() === "finalizing"}
                      fallback={
                        <>
                          <div class="rec-timebox">
                            <span class="rec-time" aria-live="off">
                              {fmt(0)}
                            </span>
                            <span class="rec-hint">
                              <Rich
                                text={t("{keys} cancels")}
                                parts={{ keys: () => <kbd>Esc</kbd> }}
                              />
                            </span>
                          </div>
                          <div class="rec-btn-group">
                            <button
                              type="button"
                              class="rec-cancel"
                              disabled={control() !== "open"}
                              onClick={cancel}
                            >
                              {t("Cancel")}
                            </button>
                            <button
                              type="button"
                              class="rec-start-now"
                              disabled={control() !== "open"}
                              onClick={startNow}
                            >
                              {t("Start now")}
                            </button>
                          </div>
                        </>
                      }
                    >
                      <span class="rec-hint finalizing">{t("Writing the take and opening the editor…")}</span>
                    </Show>
                  }
                >
                  <div class="rec-timebox">
                    <span class="rec-time" classList={{ paused: paused() }} aria-live="off">
                      {fmt(elapsed())}
                    </span>
                    <span class="rec-hint">
                      <Rich
                        text={t("{keys} stops")}
                        parts={{ keys: () => <For each={keyCaps(stopKeys())}>{(k) => <kbd>{k}</kbd>}</For> }}
                      />
                    </span>
                  </div>
                  <div class="rec-btn-group">
                    <button
                      type="button"
                      class="rec-pause"
                      classList={{ on: paused() }}
                      aria-label={paused() ? t("Resume recording") : t("Pause recording")}
                      data-tip={paused() ? t("Carry on recording") : t("Pause. The gap is cut from the take")}
                      onClick={() => void togglePause()}
                    >
                      <Icon name={paused() ? "play" : "pause"} size={14} />
                      {paused() ? t("Resume") : t("Pause")}
                    </button>
                    <button
                      type="button"
                      class="rec-stop"
                      aria-label={t("Stop recording")}
                      data-tip={t("Stop and open the editor")}
                      data-kbd={stopKeys()}
                      onClick={() => void stop()}
                    >
                      <span class="rec-stop-square" />
                      {t("Stop")}
                    </button>
                  </div>
                </Show>
              </div>
            </Show>
          </div>
        </div>
      }
    >
      <div
        class="sel-root"
        classList={{ full: preset().ratio === "full" || windowMode() }}
        ref={(el) => {
          selRootEl = el;
          dialogA11y(el, t("Select recording area"), cancel, { initialFocus: () => el });
        }}
        style={preset().ratio === "full" ? undefined : { cursor: cursor() }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={() => {
          drag = null;
        }}
      >
        <span class="sr-only" role="status" aria-live="polite">
          {liveStatus()}
        </span>
        <Show when={props.backdrop}>
          <img class="sel-shot" ref={(el) => (shotEl = el)} src={props.backdrop!} alt="" draggable={false} />
        </Show>

        {/* Four bounded dim rectangles around the selection (zero 100vmax spread shadow). */}
        <Show when={preset().ratio !== "full" && !windowMode() && sel()}>
          {(r) => {
            const tp = () => clampedTag(r());
            return (
              <>
                <div class="sel-dim" style={{ left: "0", top: "0", right: "0", height: `${r().y}px` }} />
                <div class="sel-dim" style={{ left: "0", top: `${r().y + r().h}px`, right: "0", bottom: "0" }} />
                <div class="sel-dim" style={{ left: "0", top: `${r().y}px`, width: `${r().x}px`, height: `${r().h}px` }} />
                <div
                  class="sel-dim"
                  style={{ left: `${r().x + r().w}px`, top: `${r().y}px`, right: "0", height: `${r().h}px` }}
                />
                <div
                  class="sel-rect"
                  style={{ left: `${r().x}px`, top: `${r().y}px`, width: `${r().w}px`, height: `${r().h}px` }}
                />
                <div ref={tagEl} class="sel-dimtag" style={{ left: `${tp().x}px`, top: `${tp().y}px` }}>
                  {dims()}
                </div>
                <For each={HANDLES}>
                  {(hnd) => (
                    <div
                      class="sel-handle"
                      style={{
                        left: `${r().x + ((hnd.hx + 1) / 2) * r().w}px`,
                        top: `${r().y + ((hnd.hy + 1) / 2) * r().h}px`,
                      }}
                    />
                  )}
                </For>
              </>
            );
          }}
        </Show>
        <Show when={beforeTake() && prefs.recordCamera() && bubbleSpot()}>
          {(b) => (
            <Show
              when={camera.url() || camera.starting()}
              fallback={
                <div class="sel-cam-error" role="alert" onPointerDown={(e) => e.stopPropagation()}>
                  <Icon name="cameraOff" size={14} />
                  <span>{t("Camera unavailable")}</span>
                  <button
                    type="button"
                    class="hud-heavy"
                    onClick={() => {
                      prefs.recordCamera.set(false);
                      void pushCameraChoice();
                    }}
                  >
                    {t("Continue without camera")}
                  </button>
                </div>
              }
            >
              <CameraBubble
                url={camera.url()}
                starting={camera.starting()}
                style={{ left: `${b().left}px`, top: `${b().top}px`, width: `${b().size}px`, height: `${b().size}px` }}
              />
            </Show>
          )}
        </Show>
        <Show when={preset().ratio === "full" || windowMode()}>
          <div class="sel-fullhint">
            <Icon name={windowMode() ? "window" : "fullscreen"} size={18} />
            {windowMode()
              ? t("Recording window: {label}", { label: props.target?.kind === "window" ? props.target.label : "" })
              : t("Recording the whole display")}
          </div>
        </Show>
        <Show when={!windowMode() && preset().ratio !== "full" && !sel()}>
          <div class="sel-drawhint">
            <Icon name="region" size={16} />
            {t("Drag to frame the area you want to record")}
          </div>
        </Show>

        <div
          class="hud"
          classList={{ "hud-top": hudAtTop(), "hud-compact": vp().w < 1040 }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <Show when={!windowMode()}>
            <div class="hud-group">
              <span class="hud-label">{t("Area")}</span>
              <div class="hud-seg">
                <For each={PRESETS()}>
                  {(p) => (
                    <button
                      type="button"
                      class="hud-chip"
                      classList={{ on: preset().id === p.id }}
                      aria-pressed={preset().id === p.id}
                      data-tip={p.hint}
                      onClick={() => pickPreset(p)}
                    >
                      {p.id === "full" ? t("Full") : p.id === "free" ? t("Free") : p.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
            <span class="hud-sep" />
          </Show>
          <Show
            when={vp().w >= 1040}
            fallback={
              <Menu
                class="hud-menu"
                items={() => [
                  { heading: t("Audio") },
                  ...audioMenuItems(audioDevices()),
                  { separator: true },
                  { heading: t("Camera") },
                  ...cameraMenuItems(cameras()),
                  { separator: true },
                  ...optionsMenuItems(),
                ]}
                trigger={(m) => (
                  <button
                    type="button"
                    class="hud-options"
                    classList={{ on: m.open }}
                    ref={m.ref}
                    aria-label={t("Recording setup")}
                    onClick={m.toggle}
                  >
                    <Icon name="sliders" size={14} />
                    <span>{t("Setup")}</span>
                    <Icon name="chevronUp" size={12} />
                  </button>
                )}
              />
            }
          >
            <Menu
              class="hud-menu"
              items={() => audioMenuItems(audioDevices())}
              trigger={(m) => (
                <button
                  type="button"
                  class="hud-options hud-audio"
                  classList={{ on: m.open, off: !prefs.recordMic() && !prefs.recordSystem() }}
                  ref={m.ref}
                  data-tip={prefs.recordMic() ? micName(audioDevices()) : t("Record your voice or the computer's sound")}
                  onClick={m.toggle}
                >
                  <Icon name={prefs.recordMic() ? "mic" : prefs.recordSystem() ? "volume" : "micOff"} size={14} />
                  <span>{audioSummary()}</span>
                  <Show when={prefs.recordMic()}>
                    <LevelMeter level={levels.mic()} class="hud-meter" />
                  </Show>
                  <Icon name="chevronUp" size={12} />
                </button>
              )}
            />
            <Menu
              class="hud-menu"
              items={() => cameraMenuItems(cameras())}
              trigger={(m) => (
                <button
                  type="button"
                  class="hud-options hud-camera"
                  classList={{ on: m.open, off: !prefs.recordCamera() }}
                  ref={m.ref}
                  aria-label={prefs.recordCamera() ? t("Camera: {name}", { name: cameraName(cameras()) }) : t("Camera off")}
                  data-tip={prefs.recordCamera() ? cameraName(cameras()) : t("Add your webcam as a bubble")}
                  onClick={m.toggle}
                >
                  <Icon name={prefs.recordCamera() ? "camera" : "cameraOff"} size={14} />
                  <span>{prefs.recordCamera() ? t("Camera") : t("No camera")}</span>
                  <Icon name="chevronUp" size={12} />
                </button>
              )}
            />
            <Menu
              class="hud-menu"
              items={optionsMenuItems}
              trigger={(m) => (
                <button
                  type="button"
                  class="hud-options"
                  classList={{ on: m.open }}
                  ref={m.ref}
                  data-tip={[
                    props.zoom > 1 ? t("{keys} zooms {amount}×", { keys: zoomKeys(), amount: props.zoom }) : t("No zoom"),
                    `${prefs.captureFps()} fps`,
                    cursorSummary(),
                    prefs.countdown() === 0 ? t("no countdown") : t("{countdown}s countdown", { countdown: prefs.countdown() }),
                  ].join(" · ")}
                  onClick={m.toggle}
                >
                  <Icon name="sliders" size={14} />
                  <span>{t("Options")}</span>
                  <Icon name="chevronUp" size={12} />
                </button>
              )}
            />
          </Show>
          <span class="hud-sep" />
          <div class="hud-actions">
            <Show when={heavyTake()}>
              <button
                type="button"
                class="hud-heavy"
                data-tip={t("Recording this much at 60 fps can lose frames on this PC")}
                onClick={() => {
                  prefs.captureFps.set(30);
                  void invoke("set_capture_fps", { fps: 30 }).catch(() => undefined);
                }}
              >
                {t("Use 30 fps")}
              </button>
            </Show>
            <Show
              when={!windowMode() && preset().ratio !== "full" && sel()}
              fallback={<span class="hud-dims">{windowMode() ? t("Window") : dims()}</span>}
            >
              {(r) => {
                const phys = () => {
                  const { sx, sy } = toPhysical();
                  return { w: Math.round(r().w * sx), h: Math.round(r().h * sy) };
                };
                return (
                  <div class="hud-dims-inputs">
                    <input
                      type="number"
                      class="hud-dim-input"
                      aria-label={t("Region width in pixels")}
                      value={phys().w}
                      onBlur={(e) => commitPhysDim("w", e.currentTarget.value, e.currentTarget)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          commitPhysDim("w", e.currentTarget.value, e.currentTarget);
                          e.currentTarget.blur();
                        }
                      }}
                    />
                    <span class="hud-dim-x" aria-hidden="true">×</span>
                    <input
                      type="number"
                      class="hud-dim-input"
                      aria-label={t("Region height in pixels")}
                      value={phys().h}
                      onBlur={(e) => commitPhysDim("h", e.currentTarget.value, e.currentTarget)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          commitPhysDim("h", e.currentTarget.value, e.currentTarget);
                          e.currentTarget.blur();
                        }
                      }}
                    />
                  </div>
                );
              }}
            </Show>
            <button
              type="button"
              class="hud-cancel"
              aria-label={t("Cancel")}
              disabled={phase() === "preparing" || control() !== "open"}
              onClick={cancel}
              data-tip={t("Cancel")}
              data-kbd="Esc"
            >
              <Icon name="close" size={14} />
            </button>
            <button
              type="button"
              class="hud-record"
              disabled={phase() === "preparing" || !canStart()}
              data-tip={canStart() ? t("Start recording") : t("Drag on the screen to frame an area first")}
              data-kbd="Enter"
              onClick={() => void beginCountdown()}
            >
              <span class="hud-record-dot" />
              {phase() === "preparing" ? t("Preparing…") : t("Record")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
