import { createSignal, onMount, onCleanup, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { PreviewClient } from "./preview";
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
  { v: 2.0, label: "2×" },
  { v: 2.5, label: "2.5×" },
  { v: 3.0, label: "3×" },
];

export default function RecordOverlay(props: {
  backdrop: string | null;
  zoom: number;
  onZoomChange: (v: number) => void;
  onFinished: (s: Summary) => void;
  onCancel: () => void;
  /** A real failure (capture never started, finish errored) — surfaces the backend reason. */
  onFailed: (message: string) => void;
}) {
  const [phase, setPhase] = createSignal<"select" | "countdown" | "recording">("select");
  const [preset, setPreset] = createSignal<Preset>(PRESETS[1]);
  const [sel, setSel] = createSignal<Rect | null>(null);
  const [count, setCount] = createSignal(3);
  const [elapsed, setElapsed] = createSignal(0);
  const [paused, setPaused] = createSignal(false);
  // Cursor over the selection surface — reflects what a press-drag would do (draw / move /
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
  const preview = new PreviewClient();
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
    if (p.ratio === "full" || !r || r.w < 8 || r.h < 8) {
      await invoke("set_region", {}); // no fields → full screen
    } else {
      const { sx, sy } = toPhysical();
      await invoke("set_region", {
        x: Math.round(r.x * sx),
        y: Math.round(r.y * sy),
        w: Math.round(r.w * sx),
        h: Math.round(r.h * sy),
      });
    }
    await invoke("enter_stopbar"); // shrink the host window to the bar
    // Show the recorded-region frame as the 3-2-1 begins, so the user sees exactly what's
    // in frame before capture starts. Idempotent + a no-op for full-screen on the Rust side.
    // Best-effort: the command may not exist on older backends — the frame still appears when
    // recording starts. Cancel/Esc runs cancel_record_flow → drop_border, which clears it.
    try {
      await invoke("show_region_border");
    } catch {
      /* backend without show_region_border — border still shows at record start */
    }
    setPhase("countdown");
    runCountdown();
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

  const beginRecording = async () => {
    try {
      await invoke("set_zoom_amount", { amount: props.zoom });
      await invoke("start_recording");
      setPhase("recording");
      // Hook the live preview stream to the panel canvas.
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
      // Capture never started — clean up the backend flow, then surface the real reason
      // (e.g. "No frames were captured…") instead of a bland "cancelled".
      clearCountdown();
      stopTimer();
      void invoke("cancel_record_flow").finally(() => props.onFailed(String(e)));
    }
  };

  let stopping = false; // the Stop button and the global hotkey can race — stop once
  const stop = async () => {
    if (stopping) return;
    stopping = true;
    stopTimer();
    try {
      const summary = await invoke<Summary>("finish_recording");
      props.onFinished(summary);
    } catch (e) {
      // Stop failed — the take may still be recoverable, so leave the flow intact and
      // surface the real error rather than pretending the user cancelled.
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
      return; // not recording (race with stop) — leave the UI as is
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
  // size, sliding the rect back in bounds — used when moving and after a resize.
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
    if (phase() !== "select" || preset().ratio === "full") return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
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
    // Esc aborts both while picking a region and during the 3-2-1 countdown.
    if (e.key === "Escape" && (phase() === "select" || phase() === "countdown")) {
      cancel();
      return;
    }
    if (phase() !== "select") return;
    if (e.key === "Enter") void beginCountdown();
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
    setSel(null);
    drag = null;
    setCursor(p.ratio === "full" ? "default" : "crosshair");
  };
  const dims = () => {
    const r = sel();
    if (preset().ratio === "full" || !r) return "Full screen";
    const { sx, sy } = toPhysical();
    return `${Math.round(r.w * sx)} × ${Math.round(r.h * sy)} px`;
  };

  return (
    <Show
      when={phase() === "select"}
      fallback={
        <div class="rec-panel-root">
          <div class="rec-panel">
            <div class="rec-drag" data-tauri-drag-region>
              <span class="rec-grip" data-tauri-drag-region>
                ⠿
              </span>
              <span data-tauri-drag-region>Live preview · drag to move</span>
            </div>
            <div class="rec-screen">
              <canvas ref={(el) => (canvasEl = el)} class="rec-canvas" />
              <Show when={phase() === "countdown"}>
                <div class="rec-countdown">
                  <div class="rec-ring">
                    <Show when={count() > 0} fallback={<span class="rec-num">Go</span>}>
                      <span class="rec-num">{count()}</span>
                    </Show>
                  </div>
                  <span class="rec-sub">
                    {preset().ratio === "full"
                      ? "Vuoom minimizes while recording · Ctrl+Shift+X stops"
                      : "Recording starts…"}
                  </span>
                </div>
              </Show>
              <Show when={phase() === "recording"}>
                <Show
                  when={paused()}
                  fallback={
                    <span class="rec-live">
                      <span class="rec-dot" /> LIVE
                    </span>
                  }
                >
                  <span class="rec-live paused">⏸ PAUSED</span>
                </Show>
                <span class="rec-previewtag">Zoom preview</span>
              </Show>
            </div>
            <div class="rec-controls">
              <Show
                when={phase() === "recording"}
                fallback={
                  <button class="rec-cancel" onClick={cancel}>
                    Cancel
                  </button>
                }
              >
                <span class="rec-time">{fmt(elapsed())}</span>
                <span class="rec-hint">
                  <kbd>Ctrl+Shift+Z</kbd> zoom · <kbd>Ctrl+Shift+X</kbd> stop
                </span>
                <button
                  class="rec-pause"
                  title={paused() ? "Resume recording" : "Pause — the gap is cut from the GIF"}
                  onClick={() => void togglePause()}
                >
                  {paused() ? "Resume" : "Pause"}
                </button>
                <button class="rec-stop" onClick={() => void stop()}>
                  Stop
                </button>
              </Show>
            </div>
          </div>
        </div>
      }
    >
      <div
        class="sel-root"
        classList={{ full: preset().ratio === "full" }}
        style={preset().ratio === "full" ? undefined : { cursor: cursor() }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
      >
        <Show when={props.backdrop}>
          <img
            class="sel-shot"
            ref={(el) => (shotEl = el)}
            src={props.backdrop!}
            alt=""
            draggable={false}
          />
        </Show>

        {/* Dim everything; the selection rect punches a bright hole via a huge box-shadow.
            The 8 handles + dims tag ride on top for adjustment (hit-tested in JS, so they
            stay pointer-events:none and never block a drag). */}
        <Show when={preset().ratio !== "full" && sel()}>
          {(r) => (
            <>
              <div
                class="sel-rect"
                style={{ left: `${r().x}px`, top: `${r().y}px`, width: `${r().w}px`, height: `${r().h}px` }}
              />
              <div
                class="sel-dimtag"
                style={{ left: `${Math.max(4, r().x)}px`, top: `${Math.max(4, r().y - 26)}px` }}
              >
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
        <Show when={preset().ratio === "full"}>
          <div class="sel-fullhint">Recording the whole display</div>
        </Show>

        <div class="sel-bar" onPointerDown={(e) => e.stopPropagation()}>
          <div class="sel-presets">
            <For each={PRESETS}>
              {(p) => (
                <button
                  classList={{ "sel-chip": true, active: preset().id === p.id }}
                  title={p.hint}
                  onClick={() => pickPreset(p)}
                >
                  <strong>{p.label}</strong>
                  <small>{p.hint}</small>
                </button>
              )}
            </For>
          </div>
          <div class="sel-zoomrow">
            <span class="sel-zoomlabel">Zoom level</span>
            <div class="sel-zooms">
              <For each={ZOOM_LEVELS}>
                {(z) => (
                  <button
                    classList={{ "sel-zoom": true, active: Math.abs(props.zoom - z.v) < 0.001 }}
                    title={z.v === 1 ? "No zoom" : `Zoom to ${z.label} on Ctrl+Shift+Z`}
                    onClick={() => props.onZoomChange(z.v)}
                  >
                    {z.label}
                  </button>
                )}
              </For>
            </div>
          </div>
          <div class="sel-actions">
            <span class="sel-dims">{dims()}</span>
            <button class="sel-btn ghost" onClick={cancel}>
              Cancel
            </button>
            <button class="sel-btn primary" onClick={() => void beginCountdown()}>
              Start →
            </button>
          </div>
        </div>

        <Show when={preset().ratio !== "full" && !sel()}>
          <div class="sel-drawhint">Drag to mark the area · Esc to cancel</div>
        </Show>
      </div>
    </Show>
  );
}
