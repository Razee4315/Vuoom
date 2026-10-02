// Pick the window to record by pointing at it: the frozen desktop fills the screen, the
// window under the pointer lights up with its name, and a click records it. The list of
// windows stays one button away, for a window that is hidden or on another display.
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { Icon } from "../icons";
import type { WindowInfo } from "../types";
import "../RecordOverlay.css";

/** A window the overlay can point at: `x`/`y` are known, in physical screen pixels. */
export type PickableWindow = WindowInfo & { x: number; y: number };

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export default function WindowPicker(props: {
  backdrop: string | null;
  /** Windows on this display, front to back. */
  windows: PickableWindow[];
  /** The display's top-left corner on the virtual desktop, in physical pixels. */
  origin: { x: number; y: number };
  onPick: (w: PickableWindow) => void;
  /** Leave the overlay for the plain list of windows. */
  onList: () => void;
  onCancel: () => void;
}) {
  let shotEl: HTMLImageElement | undefined;
  const [hover, setHover] = createSignal<PickableWindow | null>(null);
  // Physical pixels per CSS pixel. The backdrop is an exact pixel map of the display
  // stretched over the viewport, so its natural size gives the true scale (see
  // RecordOverlay's toPhysical); without one, the device pixel ratio stands in.
  const [scale, setScale] = createSignal({ sx: window.devicePixelRatio || 1, sy: window.devicePixelRatio || 1 });
  const measure = () => {
    const dpr = window.devicePixelRatio || 1;
    setScale({
      sx: shotEl?.naturalWidth ? shotEl.naturalWidth / window.innerWidth : dpr,
      sy: shotEl?.naturalHeight ? shotEl.naturalHeight / window.innerHeight : dpr,
    });
  };

  /** A window's client area in viewport pixels, clipped to the screen. */
  const boxOf = (w: PickableWindow): Box => {
    const { sx, sy } = scale();
    const x0 = Math.max(0, (w.x - props.origin.x) / sx);
    const y0 = Math.max(0, (w.y - props.origin.y) / sy);
    const x1 = Math.min(window.innerWidth, (w.x - props.origin.x + w.w) / sx);
    const y1 = Math.min(window.innerHeight, (w.y - props.origin.y + w.h) / sy);
    return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
  };
  /** The frontmost window under a point: the list is in front-to-back order. */
  const windowAt = (px: number, py: number): PickableWindow | null =>
    props.windows.find((w) => {
      const b = boxOf(w);
      return px >= b.x && px <= b.x + b.w && py >= b.y && py <= b.y + b.h;
    }) ?? null;

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") props.onCancel();
  };
  onMount(() => {
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", measure);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", measure);
    });
  });

  return (
    <div
      class="sel-root"
      classList={{ full: !hover() }}
      style={{ cursor: hover() ? "pointer" : "default" }}
      onPointerMove={(e) => setHover(windowAt(e.clientX, e.clientY))}
      onPointerLeave={() => setHover(null)}
      onPointerDown={(e) => {
        const w = windowAt(e.clientX, e.clientY);
        if (w) props.onPick(w);
      }}
    >
      <Show when={props.backdrop}>
        <img
          class="sel-shot"
          ref={(el) => (shotEl = el)}
          src={props.backdrop!}
          alt=""
          draggable={false}
          onLoad={measure}
        />
      </Show>
      <Show when={hover()} fallback={
        <div class="sel-fullhint">
          <Icon name="window" size={18} />
          Point at a window and click to record it
        </div>
      }>
        {(w) => {
          const b = () => boxOf(w());
          return (
            <>
              <div
                class="sel-rect"
                style={{ left: `${b().x}px`, top: `${b().y}px`, width: `${b().w}px`, height: `${b().h}px` }}
              />
              <div class="sel-dimtag" style={{ left: `${Math.max(4, b().x)}px`, top: `${Math.max(4, b().y - 30)}px` }}>
                {w().title || "Untitled window"} · {w().w} × {w().h}
              </div>
            </>
          );
        }}
      </Show>

      <div class="hud" onPointerDown={(e) => e.stopPropagation()} onPointerMove={(e) => e.stopPropagation()}>
        <span class="hud-label">Window</span>
        <button
          type="button"
          class="hud-options"
          data-tip="Pick from a list instead, for a window that is covered or on another display"
          onClick={props.onList}
        >
          <Icon name="layers" size={14} />
          <span>Choose from a list</span>
        </button>
        <span class="hud-sep" />
        <div class="hud-actions">
          <button type="button" class="hud-cancel" aria-label="Cancel" data-tip="Cancel" data-kbd="Esc" onClick={props.onCancel}>
            <Icon name="close" size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}
