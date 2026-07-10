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
}) {
  const [phase, setPhase] = createSignal<"select" | "countdown" | "recording">("select");
  const [preset, setPreset] = createSignal<Preset>(PRESETS[1]);
  const [sel, setSel] = createSignal<Rect | null>(null);
  const [count, setCount] = createSignal(3);
  const [elapsed, setElapsed] = createSignal(0);
  const [paused, setPaused] = createSignal(false);
  let start: { x: number; y: number } | null = null;
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
        const port = await invoke<number>("preview_port");
        preview.connect(port);
      } catch {
        /* preview is best-effort; recording proceeds regardless */
      }
      startMs = Date.now();
      elapsedTimer = window.setInterval(() => setElapsed((Date.now() - startMs) / 1000), 200);
    } catch {
      cancel();
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
    } catch {
      props.onCancel();
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

  // ── region drawing (select phase) ──────────────────────────────────────────────
  const onDown = (e: PointerEvent) => {
    if (phase() !== "select" || preset().ratio === "full") return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    start = { x: e.clientX, y: e.clientY };
    setSel({ x: e.clientX, y: e.clientY, w: 0, h: 0 });
  };
  const onMove = (e: PointerEvent) => {
    if (!start) return;
    const ratio = preset().ratio;
    const x = Math.min(start.x, e.clientX);
    const y = Math.min(start.y, e.clientY);
    const w = Math.abs(e.clientX - start.x);
    const h = typeof ratio === "number" ? w / ratio : Math.abs(e.clientY - start.y);
    setSel({ x, y, w, h });
  };
  const onUp = () => {
    start = null;
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
    start = null;
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

        {/* Dim everything; the selection rect punches a bright hole via a huge box-shadow. */}
        <Show when={preset().ratio !== "full" && sel()}>
          {(r) => (
            <div
              class="sel-rect"
              style={{ left: `${r().x}px`, top: `${r().y}px`, width: `${r().w}px`, height: `${r().h}px` }}
            />
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
