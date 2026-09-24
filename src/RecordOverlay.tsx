import { createSignal, onMount, onCleanup, For, Show } from "solid-js";
import { invoke, listen } from "./bridge";
import { Icon } from "./icons";
import { prefs } from "./prefs";
import { Menu, toast } from "./ui";
import { createPreviewClient } from "./preview";
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
  useCameraPreview,
} from "./components/CameraControls";
import { CURSOR_MODES, cursorSummary, pushCursorMode } from "./cursorMode";
import "./RecordOverlay.css";

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
const PRESETS: Preset[] = [
  { id: "full", label: "Full screen", hint: "Whole display", ratio: "full" },
  { id: "16:9", label: "16:9", hint: "YouTube · Reddit · X", ratio: 16 / 9 },
  { id: "9:16", label: "9:16", hint: "Reels · TikTok · Shorts", ratio: 9 / 16 },
  { id: "1:1", label: "1:1", hint: "Instagram · Facebook", ratio: 1 },
  { id: "4:5", label: "4:5", hint: "Instagram · Facebook", ratio: 4 / 5 },
  { id: "free", label: "Custom", hint: "Any size", ratio: null },
];

type Rect = { x: number; y: number; w: number; h: number };
const fmt = (t: number) => {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
};

/**
 * The full recording flow as a single in-window overlay: pick a region (fullscreen) →
 * 3-2-1 countdown (small bar) → record + Stop. The host window is excluded from the
 * capture, so none of this UI appears in the recording.
 */
const ZOOM_LEVELS = [
  { v: 1.0, label: "Off" },
  { v: 1.5, label: "1.5×" },
  { v: 1.8, label: "1.8×" },
  { v: 2.5, label: "2.5×" },
  { v: 3.0, label: "3×" },
];

export type RecordTarget =
  | { kind: "display"; name: string; label: string }
  | { kind: "window"; hwnd: number; label: string }
  | null;

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
    PRESETS.find((p) => p.id === (props.initialMode === "full" ? "full" : "free")) ?? PRESETS[0],
  );
  const [sel, setSel] = createSignal<Rect | null>(null);
  const [count, setCount] = createSignal(prefs.countdown());
  const [elapsed, setElapsed] = createSignal(0);
  const [paused, setPaused] = createSignal(false);
  // Audio: the mic check feeds the HUD meter while framing; during the take the engine's
  // own capture feeds the live panel's meters.
  const audioDevices = createAudioDevices();
  const framing = () => phase() === "select" || phase() === "countdown";
  useMicCheck(() => framing() && prefs.recordMic());
  const levels = createLevels(() => (framing() && prefs.recordMic()) || phase() === "recording");
  // Webcam: opened while framing so its bubble previews where it will sit in the take; the
  // same camera then records without reopening.
  const cameras = createCameras();
  // It stays open through "preparing" too: closing it there would make the take reopen
  // the device (or race a reopen) instead of reusing it.
  const beforeTake = () => phase() === "select" || phase() === "preparing" || phase() === "countdown";
  const camera = useCameraPreview(() => beforeTake() && prefs.recordCamera());
  // Cursor over the selection surface, reflects what a press-drag would do (draw / move /
  // resize a given edge). Applied inline so it overrides the base crosshair.
  const [cursor, setCursor] = createSignal("crosshair");
  // The active pointer gesture on the selection surface. `null` when idle (hover only).
  let drag:
    | { mode: "new" | "move" | "resize"; hx: number; hy: number; px0: number; py0: number; rect0: Rect }
    | null = null;
  let elapsedTimer: number | undefined;
  let countTimer: number | undefined;
  let startMs = 0;

  const clearCountdown = () => {
    if (countTimer) clearTimeout(countTimer);
    countTimer = undefined;
  };

  // Live "director's monitor": the backend streams a zoom-tracked preview to this canvas.
  const preview = createPreviewClient();
  let canvasEl: HTMLCanvasElement | undefined;
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

  const stopTimer = () => {
    if (elapsedTimer) clearInterval(elapsedTimer);
    elapsedTimer = undefined;
  };

  const cancel = () => {
    clearCountdown();
    stopTimer();
    void invoke("cancel_record_flow").finally(() => props.onCancel());
  };

  const beginCountdown = async () => {
    const p = preset();
    const r = sel();
    const windowMode = props.target?.kind === "window";
    // Any non-full preset without a drawn region (Custom included) must never silently
    // record full screen.
    if (!windowMode && p.ratio !== "full" && (!r || r.w < 8 || r.h < 8)) return;
    stillRegion = null; // recomputed below; never reuse a previous attempt's rect
    setCount(prefs.countdown());
    setPhase("preparing"); // instant acknowledgment: no dead click while the engine sets up
    try {
      if (windowMode) {
        // Window capture records the whole client area; no region call at all.
      } else if (p.ratio === "full" || !r) {
        const { sx, sy } = toPhysical();
        stillRegion = {
          x: 0,
          y: 0,
          w: Math.round(window.innerWidth * sx),
          h: Math.round(window.innerHeight * sy),
        };
        await invoke("set_region", {}); // no fields → full screen
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
        await invoke("set_region", {
          x: stillRegion.x,
          y: stillRegion.y,
          w: stillRegion.w,
          h: stillRegion.h,
        });
      }
      await invoke("enter_stopbar"); // shrink the host window to the bar
      // Show the recorded-region frame as the 3-2-1 begins, so the user sees exactly what's
      // in frame before capture starts. Idempotent + a no-op for full-screen on the Rust side.
      // Best-effort: the command may not exist on older backends, the frame still appears
      // when recording starts. Cancel/Esc runs cancel_record_flow, which clears it.
      try {
        await invoke("show_region_border");
      } catch {
        /* backend without show_region_border, border still shows at record start */
      }
      setPhase("countdown");
      if (count() <= 0) {
        // No countdown: capture starts right away (the preview still hooks up below).
        hookPreview();
        void beginRecording();
        return;
      }
      // Hook the preview socket now: the port exists from engine boot, frames only flow
      // once recording starts, so the very first live frame lands with capture instead of
      // a connect round-trip later. hookPreview also paints the frozen backdrop cropped to
      // the chosen region into the canvas, so the countdown shows exactly what will record
      // and the first live frame replaces the identical geometry instead of popping from
      // black.
      hookPreview();
      runCountdown();
    } catch (e) {
      setPhase("select");
      toast(`Could not start recording: ${String(e)}`, "error");
    }
  };

  const runCountdown = () => {
    const tick = () => {
      countTimer = undefined;
      // Cancelled (Cancel/Esc) during the 3-2-1: stop here, never start the recording.
      if (phase() !== "countdown") return;
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

  // Bind the panel canvas to the live preview stream as early as the countdown. The
  // connect is idempotent, so `beginRecording`'s own call below is a no-op once open.
  // The still is painted after the connect settles: a mock client resets the canvas
  // there, and repainting last keeps the countdown frame intact either way.
  const hookPreview = () => {
    if (!canvasEl) return;
    preview.attach(canvasEl);
    const paint = () => paintStill();
    void invoke<{ port: number; token: string }>("preview_port")
      .then((conn) => {
        preview.connect(conn.port, conn.token);
        paint();
      })
      .catch(paint);
  };

  // The region that will record, in backdrop (natural image) pixels, captured while the
  // overlay is still fullscreen. `null` = no still to show (nothing drawn / window target).
  let stillRegion: { x: number; y: number; w: number; h: number } | null = null;

  // Draw the frozen backdrop cropped to the recorded region into the preview canvas at
  // the stream's own resolution. The still occupies exactly the pixels the live frames
  // will replace, so the hand-over from countdown to recording is invisible instead of
  // the picture jumping in from black once the stream connects.
  const paintStill = () => {
    if (!canvasEl || !shotEl?.naturalWidth || !stillRegion) return;
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;
    const aspect = stillRegion.w / stillRegion.h;
    const w = 480;
    const h = Math.max(1, Math.round(w / aspect));
    canvasEl.width = w;
    canvasEl.height = h;
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
    try {
      await invoke("set_zoom_amount", { amount: props.zoom });
      await invoke("start_recording");
      setPhase("recording");
      // The stream was hooked at countdown; refresh the binding in case the canvas
      // changed, and rely on the idempotent connect to skip an already-open socket.
      if (canvasEl) preview.attach(canvasEl);
      try {
        const conn = await invoke<{ port: number; token: string }>("preview_port");
        preview.connect(conn.port, conn.token);
      } catch {
        /* preview is best-effort; recording proceeds regardless */
      }
      startMs = Date.now();
      elapsedTimer = window.setInterval(() => setElapsed((Date.now() - startMs) / 1000), 200);
    } catch (e) {
      // Capture never started, clean up the backend flow, then surface the real reason
      // (e.g. "No frames were captured…") instead of a bland "cancelled".
      clearCountdown();
      stopTimer();
      void invoke("cancel_record_flow").finally(() => props.onFailed(String(e)));
    }
  };

  let stopping = false; // the Stop button and the global hotkey can race, stop once
  const stop = async () => {
    if (stopping) return;
    stopping = true;
    stopTimer();
    setPhase("finalizing"); // synchronous "Finishing recording..." so the panel never
    // looks like it is still capturing while the take is being written.
    try {
      const summary = await invoke<Summary>("finish_recording");
      props.onFinished(summary);
    } catch (e) {
      // Stop failed: the take is still live, so return to the recording state with the
      // real error surfaced instead of pretending the user cancelled.
      stopping = false;
      setPhase("recording");
      startMs = Date.now() - elapsed() * 1000;
      elapsedTimer = window.setInterval(() => setElapsed((Date.now() - startMs) / 1000), 200);
      toast(`Stop failed, still recording: ${String(e)}`, "error");
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
      elapsedTimer = window.setInterval(() => setElapsed((Date.now() - startMs) / 1000), 200);
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

  // Keep a rect inside the monitor (the viewport maps 1:1 to the recorded display). Preserves
  // size, sliding the rect back in bounds, used when moving and after a resize.
  const clampToView = (r: Rect): Rect => {
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let { x, y } = r;
    if (x + r.w > vw) x = vw - r.w;
    if (y + r.h > vh) y = vh - r.h;
    return { x: Math.max(0, x), y: Math.max(0, y), w: r.w, h: r.h };
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
      const ratio = preset().ratio;
      const x = Math.min(drag.px0, e.clientX);
      const y = Math.min(drag.py0, e.clientY);
      const w = Math.abs(e.clientX - drag.px0);
      const h = typeof ratio === "number" ? w / ratio : Math.abs(e.clientY - drag.py0);
      setSel({ x, y, w, h });
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

  const onKey = (e: KeyboardEvent) => {
    // Esc aborts while picking, preparing, and during the 3-2-1 countdown.
    if (
      e.key === "Escape" &&
      (phase() === "select" || phase() === "countdown" || phase() === "preparing")
    ) {
      cancel();
      return;
    }
    if (phase() !== "select") return;
    if (e.key === "Enter") {
      const p = preset();
      const r = sel();
      if (props.target?.kind === "window") {
        void beginCountdown();
      } else if (p.ratio === "full" || (r && r.w >= 8 && r.h >= 8)) {
        void beginCountdown();
      }
    }
  };
  onMount(() => {
    window.addEventListener("keydown", onKey);
    // Global Ctrl+Shift+X (watched by the backend while recording) stops the recording
    // even when this panel doesn't have focus.
    const unlistenStop = listen("stop-hotkey", () => {
      if (phase() === "recording") void stop();
    });
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      void unlistenStop.then((un) => un());
      clearCountdown();
      stopTimer();
      preview.disconnect();
    });
  });

  const pickPreset = (p: Preset) => {
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
    if (preset().ratio === "full") return "Full screen";
    if (!r) return "No area yet";
    const { sx, sy } = toPhysical();
    return `${Math.round(r.w * sx)} × ${Math.round(r.h * sy)} px`;
  };

  const windowMode = () => props.target?.kind === "window";
  const canStart = () => windowMode() || preset().ratio === "full" || !!sel();
  /** The bubble's spot: the bottom right of the recorded area, sized like a new take's. */
  const bubbleSpot = () => {
    const whole = windowMode() || preset().ratio === "full";
    const r = whole ? { x: 0, y: 0, w: window.innerWidth, h: window.innerHeight } : sel();
    if (!r) return null;
    const size = Math.max(44, r.h * BUBBLE_SIZE);
    const m = r.h * BUBBLE_MARGIN;
    return { left: r.x + r.w - m - size, top: r.y + r.h - m - size, size };
  };

  return (
    <Show
      when={phase() === "select"}
      fallback={
        <div class="rec-panel-root">
          <div class="rec-panel" classList={{ live: phase() === "recording", paused: paused() }}>
            <div class="rec-drag" data-tauri-drag-region>
              <span class="rec-state" data-tauri-drag-region>
                <Show
                  when={phase() === "recording"}
                  fallback={
                    <span data-tauri-drag-region>
                      {phase() === "finalizing" ? "Saving take…" : phase() === "preparing" ? "Preparing…" : "Get ready"}
                    </span>
                  }
                >
                  <span class="live-dot" />
                  <span data-tauri-drag-region>{paused() ? "Paused" : "Recording"}</span>
                </Show>
              </span>
              <span class="rec-grip" data-tauri-drag-region>
                <Icon name="more" size={14} />
              </span>
            </div>
            <div class="rec-screen">
              <canvas ref={(el) => (canvasEl = el)} class="rec-canvas" />
              <Show when={phase() === "countdown"}>
                <div class="rec-countdown">
                  <div class="rec-ring" style={{ "--total": String(Math.max(1, prefs.countdown())) }}>
                    <span class="rec-num">{count() > 0 ? count() : "Go"}</span>
                  </div>
                  <span class="rec-sub">
                    {preset().ratio === "full" ? "Vuoom hides from the recording" : "Recording starts…"}
                  </span>
                </div>
              </Show>
              <Show when={phase() === "recording" && props.zoom > 1}>
                <span class="rec-previewtag">Zoom {props.zoom.toFixed(1)}×</span>
              </Show>
              <Show when={phase() === "recording" && (prefs.recordMic() || prefs.recordSystem())}>
                <div class="rec-audio">
                  <Show when={prefs.recordMic()}>
                    <span class="rec-audio-row" data-tip="Microphone level">
                      <Icon name="mic" size={11} />
                      <LevelMeter level={paused() ? 0 : levels.mic()} />
                    </span>
                  </Show>
                  <Show when={prefs.recordSystem()}>
                    <span class="rec-audio-row" data-tip="System sound level">
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
                      <button type="button" class="rec-cancel" disabled={phase() === "preparing"} onClick={cancel}>
                        Cancel
                      </button>
                    }
                  >
                    <span class="rec-hint">Writing the take and opening the editor…</span>
                  </Show>
                }
              >
                <button
                  type="button"
                  class="rec-stop"
                  aria-label="Stop recording"
                  data-tip="Stop recording"
                  data-kbd="Ctrl+Shift+X"
                  onClick={() => void stop()}
                >
                  <span />
                </button>
                <div class="rec-timebox">
                  <span class="rec-time">{fmt(elapsed())}</span>
                  <span class="rec-hint">
                    <kbd>Ctrl</kbd>
                    <kbd>Shift</kbd>
                    <kbd>Z</kbd> zoom
                  </span>
                </div>
                <button
                  type="button"
                  class="rec-pause"
                  aria-label={paused() ? "Resume recording" : "Pause recording"}
                  data-tip={paused() ? "Resume" : "Pause. The gap is cut from the take"}
                  onClick={() => void togglePause()}
                >
                  <Icon name={paused() ? "play" : "pause"} size={14} />
                </button>
              </Show>
            </div>
          </div>
        </div>
      }
    >
      <div
        class="sel-root"
        classList={{ full: preset().ratio === "full" || windowMode() }}
        style={preset().ratio === "full" ? undefined : { cursor: cursor() }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={() => {
          drag = null;
        }}
      >
        <Show when={props.backdrop}>
          <img class="sel-shot" ref={(el) => (shotEl = el)} src={props.backdrop!} alt="" draggable={false} />
        </Show>

        {/* Dim everything; the selection rect punches a bright hole via a huge box-shadow.
            The 8 handles + dims tag ride on top for adjustment (hit-tested in JS, so they
            stay pointer-events:none and never block a drag). */}
        <Show when={preset().ratio !== "full" && !windowMode() && sel()}>
          {(r) => (
            <>
              <div
                class="sel-rect"
                style={{ left: `${r().x}px`, top: `${r().y}px`, width: `${r().w}px`, height: `${r().h}px` }}
              />
              <div class="sel-dimtag" style={{ left: `${Math.max(4, r().x)}px`, top: `${Math.max(4, r().y - 30)}px` }}>
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
          )}
        </Show>
        <Show when={framing() && prefs.recordCamera() && bubbleSpot()}>
          {(b) => (
            <CameraBubble
              url={camera.url()}
              starting={camera.starting()}
              style={{ left: `${b().left}px`, top: `${b().top}px`, width: `${b().size}px`, height: `${b().size}px` }}
            />
          )}
        </Show>
        <Show when={preset().ratio === "full" || windowMode()}>
          <div class="sel-fullhint">
            <Icon name={windowMode() ? "window" : "fullscreen"} size={18} />
            {windowMode()
              ? `Recording window: ${props.target?.kind === "window" ? props.target.label : ""}`
              : "Recording the whole display"}
          </div>
        </Show>
        <Show when={!windowMode() && preset().ratio !== "full" && !sel()}>
          <div class="sel-drawhint">
            <Icon name="region" size={16} />
            Drag to frame the area you want to record
          </div>
        </Show>

        <div class="hud" onPointerDown={(e) => e.stopPropagation()}>
          <Show when={!windowMode()}>
            <div class="hud-group">
              <span class="hud-label">Frame</span>
              <div class="hud-seg">
                <For each={PRESETS}>
                  {(p) => (
                    <button
                      type="button"
                      class="hud-chip"
                      classList={{ on: preset().id === p.id }}
                      aria-pressed={preset().id === p.id}
                      data-tip={p.hint}
                      onClick={() => pickPreset(p)}
                    >
                      {p.id === "full" ? "Full" : p.id === "free" ? "Free" : p.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
            <span class="hud-sep" />
          </Show>
          <div class="hud-group">
            <span class="hud-label">Zoom</span>
            <div class="hud-seg">
              <For each={ZOOM_LEVELS}>
                {(z) => (
                  <button
                    type="button"
                    class="hud-chip"
                    classList={{ on: Math.abs(props.zoom - z.v) < 0.001 }}
                    aria-pressed={Math.abs(props.zoom - z.v) < 0.001}
                    data-tip={z.v === 1 ? "No zoom" : `Ctrl+Shift+Z zooms to ${z.label}`}
                    onClick={() => props.onZoomChange(z.v)}
                  >
                    {z.label}
                  </button>
                )}
              </For>
            </div>
          </div>
          <span class="hud-sep" />
          <Menu
            class="hud-menu"
            items={() => audioMenuItems(audioDevices())}
            trigger={(m) => (
              <button
                type="button"
                class="hud-options hud-audio"
                classList={{ on: m.open, off: !prefs.recordMic() && !prefs.recordSystem() }}
                ref={m.ref}
                data-tip={prefs.recordMic() ? micName(audioDevices()) : "Record your voice or the computer's sound"}
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
                class="hud-options hud-audio hud-camera"
                classList={{ on: m.open, off: !prefs.recordCamera() }}
                ref={m.ref}
                aria-label={prefs.recordCamera() ? `Camera: ${cameraName(cameras())}` : "Camera off"}
                data-tip={prefs.recordCamera() ? cameraName(cameras()) : "Add your webcam as a bubble"}
                onClick={m.toggle}
              >
                <Icon name={prefs.recordCamera() ? "camera" : "cameraOff"} size={14} />
                <Icon name="chevronUp" size={12} />
              </button>
            )}
          />
          <Menu
            class="hud-menu"
            items={() => [
              { heading: "Frame rate" },
              ...[24, 30, 60].map((f) => ({
                label: `${f} fps`,
                checked: prefs.captureFps() === f,
                onSelect: () => {
                  prefs.captureFps.set(f);
                  void invoke("set_capture_fps", { fps: f }).catch(() => undefined);
                },
              })),
              { heading: "Mouse pointer" },
              ...CURSOR_MODES.map((c) => ({
                label: c.menu,
                checked: prefs.cursorMode() === c.value,
                onSelect: () => {
                  prefs.cursorMode.set(c.value);
                  pushCursorMode(c.value);
                },
              })),
              { heading: "Countdown" },
              ...[0, 3, 5, 10].map((c) => ({
                label: c === 0 ? "Start immediately" : `${c} seconds`,
                checked: prefs.countdown() === c,
                onSelect: () => prefs.countdown.set(c),
              })),
            ]}
            trigger={(m) => (
              <button
                type="button"
                class="hud-options"
                classList={{ on: m.open }}
                ref={m.ref}
                data-tip="Frame rate, cursor and countdown"
                onClick={m.toggle}
              >
                <Icon name="sliders" size={14} />
                <span>
                  {prefs.captureFps()} fps · {cursorSummary()} ·{" "}
                  {prefs.countdown() === 0 ? "no timer" : `${prefs.countdown()}s`}
                </span>
                <Icon name="chevronUp" size={12} />
              </button>
            )}
          />
          <span class="hud-sep" />
          <div class="hud-actions">
            <span class="hud-dims">{windowMode() ? "Window" : dims()}</span>
            <button type="button" class="hud-cancel" disabled={phase() === "preparing"} onClick={cancel} data-tip="Cancel" data-kbd="Esc">
              <Icon name="close" size={14} />
            </button>
            <button
              type="button"
              class="hud-record"
              disabled={phase() === "preparing" || !canStart()}
              data-tip={canStart() ? "Start recording" : "Drag on the screen to frame an area first"}
              data-kbd="Enter"
              onClick={() => void beginCountdown()}
            >
              <span class="hud-record-dot" />
              {phase() === "preparing" ? "Preparing…" : "Record"}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
