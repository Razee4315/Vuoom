// Shared UI primitives: the component grammar every Vuoom screen is built from.
// Buttons, segmented controls, switches, sliders, collapsible sections, menus, keyboard
// chips, a global tooltip layer, and the toast stack for action feedback.

import {
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  type JSX,
} from "solid-js";
import { Portal } from "solid-js/web";
import { Icon, type IconName } from "./icons";
import { sectionOpen, toggleSection } from "./prefs";

// ── buttons ────────────────────────────────────────────────────────────────────

/** Icon-only button. `tip` becomes the accessible name and the hover tooltip, `kbd` the
 *  shortcut shown beside it. */
export function IconButton(props: {
  icon: IconName;
  tip: string;
  kbd?: string;
  onClick?: (e: MouseEvent) => void;
  disabled?: boolean;
  active?: boolean;
  size?: number;
  class?: string;
  ref?: (el: HTMLButtonElement) => void;
}): JSX.Element {
  return (
    <button
      type="button"
      ref={props.ref}
      class={`ibtn ${props.class ?? ""}`}
      classList={{ on: !!props.active }}
      aria-label={props.tip}
      aria-pressed={props.active === undefined ? undefined : props.active}
      data-tip={props.tip}
      data-kbd={props.kbd}
      disabled={props.disabled}
      onClick={(e) => props.onClick?.(e)}
    >
      <Icon name={props.icon} size={props.size ?? 16} />
    </button>
  );
}

// ── segmented control ─────────────────────────────────────────────────────────

/** A group of exclusive options rendered as one pill switcher with a sliding thumb. */
export function Seg<T extends string | number>(props: {
  options: { value: T; label: JSX.Element; tip?: string }[];
  value: T;
  onChange: (v: T) => void;
  class?: string;
  disabled?: boolean;
  full?: boolean;
  label?: string;
}): JSX.Element {
  const idx = () => Math.max(0, props.options.findIndex((o) => o.value === props.value));
  return (
    <div
      class={`seg ${props.class ?? ""}`}
      classList={{ disabled: !!props.disabled, full: !!props.full }}
      style={{ "--n": props.options.length, "--i": idx() }}
    >
      <span class="seg-thumb" aria-hidden="true" />
      <For each={props.options}>
        {(o) => (
          <button
            type="button"
            aria-pressed={props.value === o.value}
            data-tip={o.tip}
            disabled={props.disabled}
            classList={{ "seg-btn": true, on: props.value === o.value }}
            onClick={() => props.onChange(o.value)}
          >
            {o.label}
          </button>
        )}
      </For>
    </div>
  );
}

// ── switch ─────────────────────────────────────────────────────────────────────

export function Switch(props: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  disabled?: boolean;
}): JSX.Element {
  return (
    <button
      type="button"
      role="switch"
      class="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      disabled={props.disabled}
      onClick={() => props.onChange(!props.checked)}
    >
      <span class="switch-knob" />
    </button>
  );
}

// ── slider ─────────────────────────────────────────────────────────────────────

/** A range slider with a filled track and an inline value readout. */
export function Slider(props: {
  value: number;
  min: number;
  max: number;
  step: number;
  onInput: (v: number) => void;
  onCommit?: (v: number) => void;
  format?: (v: number) => string;
  label?: string;
  disabled?: boolean;
}): JSX.Element {
  const pct = () => ((props.value - props.min) / (props.max - props.min || 1)) * 100;
  return (
    <div class="slider" classList={{ disabled: !!props.disabled }}>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        disabled={props.disabled}
        aria-label={props.label}
        style={{ "--pct": `${pct()}%` }}
        onInput={(e) => props.onInput(Number(e.currentTarget.value))}
        onChange={(e) => props.onCommit?.(Number(e.currentTarget.value))}
      />
      <Show when={props.format}>
        <output class="slider-val">{props.format!(props.value)}</output>
      </Show>
    </div>
  );
}

// ── inspector layout ───────────────────────────────────────────────────────────

/** What the inspector's settings search is looking for (lowercase, trimmed). */
const [sectionQuery, setSectionQueryRaw] = createSignal("");
export { sectionQuery };
export const setSectionQuery = (q: string) => setSectionQueryRaw(q.trim().toLowerCase());

/**
 * A collapsible, titled group. Its open state persists per `id`. While the settings search
 * has a query, a section shows only if its title, `keywords` or any of its words match,
 * and then it opens by itself.
 */
export function Section(props: {
  id: string;
  title: string;
  icon?: IconName;
  aside?: JSX.Element;
  defaultOpen?: boolean;
  keywords?: string;
  children: JSX.Element;
}): JSX.Element {
  let body: HTMLDivElement | undefined;
  const matches = () => {
    const q = sectionQuery();
    if (!q) return true;
    const hay = `${props.title} ${props.keywords ?? ""} ${body?.textContent ?? ""}`.toLowerCase();
    return q.split(/\s+/).every((w) => hay.includes(w));
  };
  const open = () => (sectionQuery() ? matches() : sectionOpen(props.id, props.defaultOpen ?? true));
  return (
    <section class="sect" classList={{ open: open(), "sect-miss": !matches() }} data-sect={props.id}>
      <header class="sect-head">
        <button
          type="button"
          class="sect-toggle"
          aria-expanded={open()}
          onClick={() => toggleSection(props.id, props.defaultOpen ?? true)}
        >
          <Icon name="chevronRight" size={12} class="sect-chev" />
          <Show when={props.icon}>
            <Icon name={props.icon!} size={14} class="sect-icon" />
          </Show>
          <span>{props.title}</span>
        </button>
        <Show when={props.aside}>
          <div class="sect-aside">{props.aside}</div>
        </Show>
      </header>
      <div class="sect-body-wrap">
        <div class="sect-body" ref={body}>
          {props.children}
        </div>
      </div>
    </section>
  );
}

/** One labelled property row: label on the left, control on the right (or stacked). */
export function Field(props: {
  label: string;
  hint?: string;
  stack?: boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div class="field" classList={{ stack: !!props.stack }}>
      <span class="field-label" data-tip={props.hint}>
        {props.label}
      </span>
      <div class="field-control">{props.children}</div>
    </div>
  );
}

/** Keyboard chip(s). A string like "Ctrl+Shift+R" renders one chip per key. */
export function Kbd(props: { keys: string }): JSX.Element {
  return (
    <span class="kbds">
      <For each={props.keys.split("+")}>{(k) => <kbd>{k}</kbd>}</For>
    </span>
  );
}

// ── menus ──────────────────────────────────────────────────────────────────────

export type MenuItem =
  | {
      label: string;
      icon?: IconName;
      kbd?: string;
      onSelect: () => void;
      disabled?: boolean;
      checked?: boolean;
      danger?: boolean;
    }
  | { separator: true }
  | { heading: string };

/** The popup half of a menu: positioned list, keyboard navigation, dismissal. Shared by
 *  dropdown menus and right-click context menus. */
function MenuPopup(props: {
  items: MenuItem[];
  x: number;
  y: number;
  align?: "start" | "end";
  class?: string;
  onClose: (refocus: boolean) => void;
  keepOpenOn?: () => HTMLElement | undefined;
  /** Top edge of the trigger: when the menu would overflow the bottom, it opens above it. */
  flipAbove?: number;
}): JSX.Element {
  let menuEl: HTMLDivElement | undefined;
  // Keep the popup on screen: flip left/up when it would overflow the viewport.
  const [shift, setShift] = createSignal({ x: 0, y: 0 });
  onMount(() => {
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (menuEl?.contains(t) || props.keepOpenOn?.()?.contains(t)) return;
      props.onClose(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose(true);
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const items = [...(menuEl?.querySelectorAll<HTMLButtonElement>(".menu-item:not(:disabled)") ?? [])];
        if (!items.length) return;
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        const next = e.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
        items[next].focus();
      }
    };
    const close = () => props.onClose(false);
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    queueMicrotask(() => {
      if (!menuEl) return;
      // Layout sizes, not the bounding box: the pop-in animation transforms the popup.
      const w = menuEl.offsetWidth;
      const h = menuEl.offsetHeight;
      const left = props.align === "end" ? props.x - w : props.x;
      setShift({
        x: left + w > window.innerWidth - 8 ? window.innerWidth - 8 - (left + w) : 0,
        y:
          props.y + h > window.innerHeight - 8
            ? props.flipAbove !== undefined
              ? props.flipAbove - 6 - h - props.y
              : -(h + 12)
            : 0,
      });
      menuEl.querySelector<HTMLButtonElement>(".menu-item:not(:disabled)")?.focus();
    });
    onCleanup(() => {
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    });
  });
  return (
    <Portal>
      <div
        ref={menuEl}
        class={`menu ${props.class ?? ""}`}
        classList={{ end: props.align === "end" }}
        role="menu"
        style={{ left: `${props.x + shift().x}px`, top: `${props.y + shift().y}px` }}
        onContextMenu={(e) => e.preventDefault()}
      >
        <For each={props.items}>
          {(it) => {
            if ("separator" in it) return <div class="menu-sep" />;
            if ("heading" in it) return <div class="menu-heading">{it.heading}</div>;
            return (
              <button
                type="button"
                role="menuitem"
                class="menu-item"
                classList={{ danger: !!it.danger }}
                disabled={it.disabled}
                onClick={() => {
                  props.onClose(false);
                  it.onSelect();
                }}
              >
                <span class="menu-icon">
                  <Show
                    when={it.checked !== undefined}
                    fallback={
                      <Show when={it.icon}>
                        <Icon name={it.icon!} size={15} />
                      </Show>
                    }
                  >
                    <Show when={it.checked}>
                      <Icon name="check" size={14} />
                    </Show>
                  </Show>
                </span>
                <span class="menu-label">{it.label}</span>
                <Show when={it.kbd}>
                  <span class="menu-kbd">{it.kbd}</span>
                </Show>
              </button>
            );
          }}
        </For>
      </div>
    </Portal>
  );
}

/** A dropdown menu anchored to its trigger. Closes on selection, outside click, or Esc;
 *  arrow keys move between items. */
export function Menu(props: {
  trigger: (p: { open: boolean; toggle: () => void; ref: (el: HTMLElement) => void }) => JSX.Element;
  items: () => MenuItem[];
  align?: "start" | "end";
  class?: string;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [pos, setPos] = createSignal({ x: 0, y: 0, top: 0 });
  let anchor: HTMLElement | undefined;
  const toggle = () => {
    if (anchor) {
      const r = anchor.getBoundingClientRect();
      setPos({ x: props.align === "end" ? r.right : r.left, y: r.bottom + 6, top: r.top });
    }
    setOpen(!open());
  };
  return (
    <>
      {props.trigger({ open: open(), toggle, ref: (el) => (anchor = el) })}
      <Show when={open()}>
        <MenuPopup
          items={props.items()}
          x={pos().x}
          y={pos().y}
          align={props.align}
          class={props.class}
          flipAbove={pos().top}
          keepOpenOn={() => anchor}
          onClose={(refocus) => {
            setOpen(false);
            if (refocus) anchor?.focus();
          }}
        />
      </Show>
    </>
  );
}

// ── context menus ──────────────────────────────────────────────────────────────
// One right-click menu for the whole app. Components call `openContextMenu(e, items)`
// from their `onContextMenu`; <ContextMenuHost/> renders it at the pointer.

const [ctxMenu, setCtxMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);

export function openContextMenu(e: MouseEvent, items: MenuItem[]): void {
  e.preventDefault();
  e.stopPropagation();
  setCtxMenu(null);
  queueMicrotask(() => setCtxMenu({ x: e.clientX, y: e.clientY, items }));
}

export function ContextMenuHost(): JSX.Element {
  return (
    <Show when={ctxMenu()} keyed>
      {(m) => <MenuPopup items={m.items} x={m.x} y={m.y} onClose={() => setCtxMenu(null)} />}
    </Show>
  );
}

// ── tooltips ───────────────────────────────────────────────────────────────────
// One tooltip layer for the whole app: any element with `data-tip` (and optionally
// `data-kbd`) gets a styled tooltip after a short hover delay. No per-button wiring, no
// native `title` lag, and the shortcut is always shown next to the action.

export function TooltipLayer(): JSX.Element {
  const [tip, setTip] = createSignal<{ text: string; kbd?: string; x: number; y: number; below: boolean } | null>(null);
  let timer: number | undefined;
  let current: HTMLElement | null = null;
  const hide = () => {
    clearTimeout(timer);
    current = null;
    setTip(null);
  };
  onMount(() => {
    const over = (e: PointerEvent) => {
      const el = (e.target as HTMLElement).closest<HTMLElement>("[data-tip]");
      if (el === current) return;
      clearTimeout(timer);
      current = el;
      setTip(null);
      if (!el?.dataset.tip) return;
      timer = window.setTimeout(() => {
        if (current !== el || !el.isConnected) return;
        const r = el.getBoundingClientRect();
        const below = r.top < 64;
        setTip({
          text: el.dataset.tip!,
          kbd: el.dataset.kbd,
          x: Math.min(Math.max(r.left + r.width / 2, 90), window.innerWidth - 90),
          y: below ? r.bottom + 8 : r.top - 8,
          below,
        });
      }, 420);
    };
    document.addEventListener("pointerover", over);
    document.addEventListener("pointerdown", hide, true);
    window.addEventListener("blur", hide);
    window.addEventListener("keydown", hide, true);
    onCleanup(() => {
      document.removeEventListener("pointerover", over);
      document.removeEventListener("pointerdown", hide, true);
      window.removeEventListener("blur", hide);
      window.removeEventListener("keydown", hide, true);
    });
  });
  return (
    <Show when={tip()}>
      {(t) => (
        <div
          class="tooltip"
          classList={{ below: t().below }}
          role="tooltip"
          style={{ left: `${t().x}px`, top: `${t().y}px` }}
        >
          <span>{t().text}</span>
          <Show when={t().kbd}>
            <span class="tooltip-kbd">{t().kbd}</span>
          </Show>
        </div>
      )}
    </Show>
  );
}

// ── toasts ─────────────────────────────────────────────────────────────────────
export type ToastKind = "info" | "success" | "error";
export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
  action?: { label: string; run: () => void };
}

const [toasts, setToasts] = createSignal<Toast[]>([]);
let nextToastId = 1;
const toastTimers = new Map<number, number>();

function scheduleDismiss(id: number, ttl: number) {
  clearTimeout(toastTimers.get(id));
  toastTimers.set(
    id,
    window.setTimeout(() => dismissToast(id), ttl),
  );
}

/** Push a toast. Auto-dismisses after `ttl` ms (errors stay a little longer); hovering a
 *  toast pauses the timer, and every toast has a real, focusable dismiss button. An
 *  optional action (e.g. "Undo", "Show in folder") renders as a button in the toast. */
export function toast(
  text: string,
  kind: ToastKind = "info",
  ttl = 3800,
  action?: { label: string; run: () => void },
): void {
  const id = nextToastId++;
  setToasts((prev) => [...prev.slice(-3), { id, kind, text, action }]);
  scheduleDismiss(id, kind === "error" ? ttl + 2200 : ttl);
}

export function dismissToast(id: number): void {
  clearTimeout(toastTimers.get(id));
  toastTimers.delete(id);
  setToasts((prev) => prev.filter((t) => t.id !== id));
}

const TOAST_ICON: Record<ToastKind, IconName> = { success: "check", error: "warning", info: "info" };

/** Renders the toast stack. Mount once, near the end of the app tree. */
export function ToastHost(): JSX.Element {
  return (
    <div class="toasts" role="status" aria-live="polite">
      <For each={toasts()}>
        {(t) => (
          <div
            class="toast"
            classList={{ [t.kind]: true }}
            onPointerEnter={() => clearTimeout(toastTimers.get(t.id))}
            onPointerLeave={() => scheduleDismiss(t.id, 2000)}
          >
            <span class="toast-icon">
              <Icon name={TOAST_ICON[t.kind]} size={14} stroke={2.2} />
            </span>
            <span class="toast-text">{t.text}</span>
            <Show when={t.action}>
              <button
                type="button"
                class="toast-action"
                onClick={() => {
                  t.action!.run();
                  dismissToast(t.id);
                }}
              >
                {t.action!.label}
              </button>
            </Show>
            <button
              type="button"
              class="toast-x"
              aria-label="Dismiss notification"
              onClick={() => dismissToast(t.id)}
            >
              <Icon name="close" size={10} stroke={2.5} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
