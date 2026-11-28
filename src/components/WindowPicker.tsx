// Pick the window to record by pointing at it: the frozen desktop fills the screen, the
// window under the pointer or keyboard selection lights up with its name, and click/Enter
// records it. The list of windows stays one button away for hidden or other-display windows.
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { dialogA11y } from "../dialog";
import { t } from "../i18n";
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
  let surfaceEl: HTMLDivElement | undefined;
  let tagEl: HTMLDivElement | undefined;
  const [activeHwnd, setActiveHwnd] = createSignal<number | null>(null);
  const [tagSize, setTagSize] = createSignal({ w: 220, h: 28 });
  const [vw, setVw] = createSignal(window.innerWidth);
  const [vh, setVh] = createSignal(window.innerHeight);

  // Physical pixels per CSS pixel. The backdrop is an exact pixel map of the display
  // stretched over the viewport, so its natural size gives the true scale (see
  // RecordOverlay's toPhysical); without one, the device pixel ratio stands in.
  const [scale, setScale] = createSignal({ sx: window.devicePixelRatio || 1, sy: window.devicePixelRatio || 1 });
  const measure = () => {
    const dpr = window.devicePixelRatio || 1;
    const w = window.innerWidth;
    const h = window.innerHeight;
    setVw(w);
    setVh(h);
    setScale({
      sx: shotEl?.naturalWidth ? shotEl.naturalWidth / w : dpr,
      sy: shotEl?.naturalHeight ? shotEl.naturalHeight / h : dpr,
    });
  };

  /** A window's client area in viewport pixels, clipped to the screen. */
  const boxOf = (w: PickableWindow): Box => {
    const { sx, sy } = scale();
    const x0 = Math.max(0, (w.x - props.origin.x) / sx);
    const y0 = Math.max(0, (w.y - props.origin.y) / sy);
    const x1 = Math.min(vw(), (w.x - props.origin.x + w.w) / sx);
    const y1 = Math.min(vh(), (w.y - props.origin.y + w.h) / sy);
    return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
  };

  const activeWindow = (): PickableWindow | null => {
    const id = activeHwnd();
    if (id === null) return null;
    return props.windows.find((w) => w.hwnd === id) ?? null;
  };

  /** The frontmost window under a point: the list is in front-to-back order. */
  const windowAt = (px: number, py: number): PickableWindow | null =>
    props.windows.find((w) => {
      const b = boxOf(w);
      return px >= b.x && px <= b.x + b.w && py >= b.y && py <= b.y + b.h;
    }) ?? null;

  createEffect(() => {
    void activeWindow();
    queueMicrotask(() => {
      if (!tagEl) return;
      const r = tagEl.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) setTagSize({ w: r.width, h: r.height });
    });
  });

  const clampedTag = (b: Box) => {
    const pad = 8;
    const tw = Math.min(tagSize().w, Math.max(80, vw() - pad * 2));
    const th = tagSize().h;
    const x = Math.max(pad, Math.min(vw() - tw - pad, b.x));
    const preferTop = b.y - th - 6;
    const y = preferTop >= pad ? preferTop : Math.max(pad, Math.min(vh() - th - pad, b.y + 8));
    return { x, y };
  };

  const cycle = (dir: 1 | -1) => {
    const list = props.windows;
    if (list.length === 0) return;
    const curIdx = list.findIndex((w) => w.hwnd === activeHwnd());
    const nextIdx = curIdx < 0 ? (dir > 0 ? 0 : list.length - 1) : (curIdx + dir + list.length) % list.length;
    setActiveHwnd(list[nextIdx].hwnd);
  };

  const onSurfaceKey = (e: KeyboardEvent) => {
    if (e.target !== surfaceEl) return;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      cycle(1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      cycle(-1);
    } else if (e.key === "Enter" || e.key === " ") {
      const w = activeWindow();
      if (w) {
        e.preventDefault();
        props.onPick(w);
      }
    }
  };

  onMount(() => {
    window.addEventListener("resize", measure);
    onCleanup(() => window.removeEventListener("resize", measure));
  });

  return (
    <div
      class="sel-root"
      classList={{ full: !activeWindow() }}
      ref={(el) => {
        surfaceEl = el;
        dialogA11y(el, t("Choose a window to record"), props.onCancel, {
          initialFocus: () => el,
        });
      }}
      style={{ cursor: activeWindow() ? "pointer" : "default" }}
      onKeyDown={onSurfaceKey}
      onPointerMove={(e) => {
        const hit = windowAt(e.clientX, e.clientY);
        setActiveHwnd(hit ? hit.hwnd : null);
      }}
      onPointerLeave={() => setActiveHwnd(null)}
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
      <Show
        when={activeWindow()}
        fallback={
          <div class="sel-fullhint">
            <Icon name="window" size={18} />
            {props.windows.length === 0
              ? t("No visible windows on this display. Choose from a list or cancel.")
              : t("Point at a window and click, or use arrow keys and Enter")}
          </div>
        }
      >
        {(w) => {
          const b = () => boxOf(w());
          const tp = () => clampedTag(b());
          return (
            <>
              <div class="sel-dim" style={{ left: "0", top: "0", right: "0", height: `${b().y}px` }} />
              <div class="sel-dim" style={{ left: "0", top: `${b().y + b().h}px`, right: "0", bottom: "0" }} />
              <div class="sel-dim" style={{ left: "0", top: `${b().y}px`, width: `${b().x}px`, height: `${b().h}px` }} />
              <div
                class="sel-dim"
                style={{ left: `${b().x + b().w}px`, top: `${b().y}px`, right: "0", height: `${b().h}px` }}
              />
              <div
                class="sel-rect"
                style={{ left: `${b().x}px`, top: `${b().y}px`, width: `${b().w}px`, height: `${b().h}px` }}
              />
              <div ref={tagEl} class="sel-dimtag" style={{ left: `${tp().x}px`, top: `${tp().y}px` }}>
                {w().title || t("Untitled window")} · {w().w} × {w().h}
              </div>
            </>
          );
        }}
      </Show>

      <div
        class="hud"
        onPointerDown={(e) => e.stopPropagation()}
        onPointerMove={(e) => e.stopPropagation()}
      >
        <span class="hud-label">{t("Window")}</span>
        <button
          type="button"
          class="hud-options"
          data-tip={t("Pick from a list instead, for a window that is covered or on another display")}
          onClick={props.onList}
        >
          <Icon name="layers" size={14} />
          <span>{t("Choose from a list")}</span>
        </button>
        <span class="hud-sep" />
        <div class="hud-actions">
          <button
            type="button"
            class="hud-cancel"
            aria-label={t("Cancel")}
            data-tip={t("Cancel")}
            data-kbd="Esc"
            onClick={props.onCancel}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}
