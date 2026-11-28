// Shared UI primitives: the consolidated component grammar every Vuoom screen builds from.
// Actions, segmented controls, switches, sliders, sections, fields, roving focus helpers,
// modals, floating islands, banners, empty states, skeletons, choice/action cards, menus, tooltips, toasts.

import { createEffect, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { dialogA11y } from "./dialog";
import { t } from "./i18n";
import { Icon, type IconName } from "./icons";
import { keyCaps, keyText } from "./platform";
import { sectionOpen, toggleSection } from "./prefs";

// ── focus & presence helpers ───────────────────────────────────────────────────

/** Keyboard arrow/Home/End navigation across focusable children inside a composite control. */
export function createRovingTabindex(
  container: () => HTMLElement | undefined,
  selector: string,
  opts?: { orientation?: "horizontal" | "vertical" | "both"; loop?: boolean },
): (e: KeyboardEvent) => void {
  const orientation = opts?.orientation ?? "horizontal";
  const loop = opts?.loop ?? true;
  return (e: KeyboardEvent) => {
    const forward =
      (orientation !== "vertical" && e.key === "ArrowRight") ||
      (orientation !== "horizontal" && e.key === "ArrowDown");
    const backward =
      (orientation !== "vertical" && e.key === "ArrowLeft") ||
      (orientation !== "horizontal" && e.key === "ArrowUp");
    const toStart = e.key === "Home";
    const toEnd = e.key === "End";
    if (!forward && !backward && !toStart && !toEnd) return;
    const root = container();
    if (!root) return;
    const items = Array.from(root.querySelectorAll<HTMLElement>(selector)).filter(
      (n) => !n.hasAttribute("disabled") && n.getAttribute("aria-disabled") !== "true" && n.offsetParent !== null,
    );
    if (items.length === 0) return;
    e.preventDefault();
    const cur = items.indexOf(document.activeElement as HTMLElement);
    let nextIdx = 0;
    if (toStart) nextIdx = 0;
    else if (toEnd) nextIdx = items.length - 1;
    else if (forward) nextIdx = cur < 0 ? 0 : loop ? (cur + 1) % items.length : Math.min(items.length - 1, cur + 1);
    else nextIdx = cur < 0 ? items.length - 1 : loop ? (cur - 1 + items.length) % items.length : Math.max(0, cur - 1);
    items[nextIdx].focus();
  };
}

/** Interruptible mount/exit helper: keeps exiting surfaces mounted for `exitMs` unless reduced motion is active. */
export function createPresence(
  when: () => boolean,
  exitMs = 140,
): { mounted: () => boolean; exiting: () => boolean } {
  const [mounted, setMounted] = createSignal(when());
  const [exiting, setExiting] = createSignal(false);
  let timer: number | undefined;
  createEffect(() => {
    clearTimeout(timer);
    if (when()) {
      setExiting(false);
      setMounted(true);
      return;
    }
    if (!mounted()) return;
    const reduced =
      document.documentElement.dataset.motion === "reduced" ||
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const wait = reduced ? 0 : exitMs;
    if (wait <= 0) {
      setExiting(false);
      setMounted(false);
      return;
    }
    setExiting(true);
    timer = window.setTimeout(() => {
      setExiting(false);
      setMounted(false);
    }, wait);
  });
  onCleanup(() => clearTimeout(timer));
  return { mounted, exiting };
}

// ── buttons & pickers ──────────────────────────────────────────────────────────

/** Icon-only button. `tip` becomes the accessible name and hover tooltip, `kbd` the shortcut badge. */
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

/** Shared trigger chrome for dropdown pickers (Audio, Camera, Workspace). */
export function PickerTrigger(props: {
  icon: IconName;
  label: JSX.Element;
  open?: boolean;
  off?: boolean;
  tip?: string;
  onClick?: (e: MouseEvent) => void;
  ref?: (el: HTMLButtonElement) => void;
  class?: string;
  children?: JSX.Element;
}): JSX.Element {
  return (
    <button
      type="button"
      ref={props.ref}
      class={`audio-picker picker-trigger ${props.class ?? ""}`}
      classList={{ on: !!props.open, off: !!props.off }}
      aria-expanded={props.open}
      data-tip={props.tip}
      onClick={(e) => props.onClick?.(e)}
    >
      <Icon name={props.icon} size={14} />
      <span>{props.label}</span>
      {props.children}
      <Icon name="chevronDown" size={12} />
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
    // biome-ignore lint/a11y/useSemanticElements: a fieldset can't lay out as this control.
    <div
      class={`seg ${props.class ?? ""}`}
      classList={{ disabled: !!props.disabled, full: !!props.full }}
      style={{ "--n": props.options.length, "--i": idx() }}
      role="group"
      aria-label={props.label}
      onKeyDown={(e) => {
        if (props.disabled || props.options.length === 0) return;
        const n = props.options.length;
        let nextIdx: number | null = null;
        const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
        if (step !== undefined) nextIdx = (idx() + step + n) % n;
        else if (e.key === "Home") nextIdx = 0;
        else if (e.key === "End") nextIdx = n - 1;
        if (nextIdx === null) return;
        e.preventDefault();
        const container = e.currentTarget;
        props.onChange(props.options[nextIdx].value);
        queueMicrotask(() => container.querySelector<HTMLElement>(".seg-btn.on")?.focus());
      }}
    >
      <span class="seg-thumb" aria-hidden="true" />
      <For each={props.options}>
        {(o, i) => (
          <button
            type="button"
            aria-pressed={props.value === o.value}
            tabindex={idx() === i() ? 0 : -1}
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

// ── switch & slider ───────────────────────────────────────────────────────────

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

// ── inspector & form layout ───────────────────────────────────────────────────

/** What the inspector's settings search is looking for (lowercase, trimmed). */
const [sectionQuery, setSectionQueryRaw] = createSignal("");
export { sectionQuery };
export const setSectionQuery = (q: string) => setSectionQueryRaw(q.trim().toLowerCase());

/** A collapsible, titled group whose open state persists per `id`. */
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

/** One labelled property row: associated label (when `forId` provided), optional description/error, and control. */
export function Field(props: {
  label: string;
  forId?: string;
  hint?: string;
  description?: string;
  error?: string;
  stack?: boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div class="field" classList={{ stack: !!props.stack }}>
      <Show
        when={props.forId}
        fallback={
          <span class="field-label" data-tip={props.hint}>
            {props.label}
          </span>
        }
      >
        <label class="field-label" for={props.forId} data-tip={props.hint}>
          {props.label}
        </label>
      </Show>
      <div class="field-control">{props.children}</div>
      <Show when={props.description}>
        <p class="field-desc">{props.description}</p>
      </Show>
      <Show when={props.error}>
        <p class="field-error" role="alert">
          {props.error}
        </p>
      </Show>
    </div>
  );
}

// ── containers, cards, feedback ───────────────────────────────────────────────

/** Accessible modal shell sharing .modal-backdrop, .modal, and hardened dialogA11y(). */
export function Modal(props: {
  label: string;
  onClose: () => void;
  class?: string;
  backdropClass?: string;
  initialFocus?: () => HTMLElement | null;
  closeOnBackdrop?: boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div
      class={`modal-backdrop ${props.backdropClass ?? ""}`}
      onClick={(e) => {
        if (props.closeOnBackdrop !== false && e.target === e.currentTarget) props.onClose();
      }}
    >
      <div
        class={`modal ${props.class ?? ""}`}
        ref={(el) => dialogA11y(el, props.label, props.onClose, { initialFocus: props.initialFocus })}
        onClick={(e) => e.stopPropagation()}
      >
        {props.children}
      </div>
    </div>
  );
}

export function ModalHeader(props: {
  title: string;
  subtitle?: string;
  onClose?: () => void;
  closeLabel?: string;
  aside?: JSX.Element;
}): JSX.Element {
  return (
    <header class="modal-head">
      <div>
        <h2>{props.title}</h2>
        <Show when={props.subtitle}>
          <p class="muted small">{props.subtitle}</p>
        </Show>
      </div>
      {props.aside}
      <Show when={props.onClose}>
        <IconButton icon="close" size={14} tip={props.closeLabel ?? t("Close")} onClick={props.onClose} />
      </Show>
    </header>
  );
}

export function ModalBody(props: { class?: string; children: JSX.Element }): JSX.Element {
  return <div class={`modal-body ${props.class ?? ""}`}>{props.children}</div>;
}

export function ModalFooter(props: { class?: string; children: JSX.Element }): JSX.Element {
  return <footer class={`modal-actions ${props.class ?? ""}`}>{props.children}</footer>;
}

/** Opaque floating surface (Stage hint pills, Crop bar, HUDs) using tier-2 elevation. */
export function Island(props: {
  class?: string;
  role?: "toolbar" | "region" | "status" | "group";
  label?: string;
  children: JSX.Element;
}): JSX.Element {
  return (
    <Show
      when={props.role || props.label}
      fallback={<div class={`island ${props.class ?? ""}`}>{props.children}</div>}
    >
      <section class={`island ${props.class ?? ""}`} role={props.role ?? "region"} aria-label={props.label}>
        {props.children}
      </section>
    </Show>
  );
}

/** Structured empty state with optional custom geometric SVG art, title, 1-line guidance, and actions. */
export function EmptyState(props: {
  art?: JSX.Element;
  title: string;
  description?: string;
  actions?: JSX.Element;
  class?: string;
}): JSX.Element {
  return (
    <div class={`empty-state ${props.class ?? ""}`}>
      <Show when={props.art}>
        <div class="empty-art">{props.art}</div>
      </Show>
      <strong>{props.title}</strong>
      <Show when={props.description}>
        <p class="note">{props.description}</p>
      </Show>
      <Show when={props.actions}>
        <div class="empty-actions">{props.actions}</div>
      </Show>
    </div>
  );
}

/** Inline notice banner for GPU warnings, device enumeration errors, and recovery prompts. */
export function Banner(props: {
  kind?: "info" | "warn" | "error" | "ok";
  icon?: IconName;
  role?: "status" | "alert";
  action?: JSX.Element;
  onDismiss?: () => void;
  dismissLabel?: string;
  class?: string;
  children: JSX.Element;
}): JSX.Element {
  const defaultIcon: Record<NonNullable<typeof props.kind>, IconName> = {
    info: "info",
    warn: "warning",
    error: "warning",
    ok: "check",
  };
  const kind = () => props.kind ?? "warn";
  return (
    <div class={`ui-banner ${kind()} ${props.class ?? ""}`} role={props.role ?? (kind() === "error" ? "alert" : "status")}>
      <Icon name={props.icon ?? defaultIcon[kind()]} size={15} class="ui-banner-icon" />
      <div class="ui-banner-body">{props.children}</div>
      {props.action}
      <Show when={props.onDismiss}>
        <IconButton icon="close" size={12} class="sm" tip={props.dismissLabel ?? t("Dismiss")} onClick={props.onDismiss} />
      </Show>
    </div>
  );
}

/** Pure-CSS 180ms-delayed skeleton block for async loads (0 timers, 0 continuous shimmer). */
export function Skeleton(props: { width?: string; height?: string; radius?: string; class?: string }): JSX.Element {
  return (
    <div
      class={`skeleton ${props.class ?? ""}`}
      aria-hidden="true"
      style={{ width: props.width, height: props.height ?? "14px", "border-radius": props.radius }}
    />
  );
}

/** Selectable option card (format choices, theme presets, source modes). */
export function ChoiceCard(props: {
  selected?: boolean;
  icon?: JSX.Element;
  title: string;
  subtitle?: string;
  disabled?: boolean;
  onClick: () => void;
  class?: string;
}): JSX.Element {
  return (
    <button
      type="button"
      class={`choice-card ${props.class ?? ""}`}
      classList={{ on: !!props.selected }}
      aria-pressed={!!props.selected}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <Show when={props.icon}>
        <span class="choice-card-icon">{props.icon}</span>
      </Show>
      <span class="choice-card-text">
        <strong>{props.title}</strong>
        <Show when={props.subtitle}>
          <small>{props.subtitle}</small>
        </Show>
      </span>
    </button>
  );
}

/** Command / capture launcher card with leading icon, copy, and trailing affordance. */
export function ActionCard(props: {
  icon?: JSX.Element;
  title: string;
  subtitle?: string;
  trailing?: JSX.Element;
  primary?: boolean;
  disabled?: boolean;
  onClick: () => void;
  class?: string;
}): JSX.Element {
  return (
    <button
      type="button"
      class={`action-card ${props.class ?? ""}`}
      classList={{ primary: !!props.primary }}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <Show when={props.icon}>
        <span class="action-card-icon">{props.icon}</span>
      </Show>
      <span class="action-card-text">
        <strong>{props.title}</strong>
        <Show when={props.subtitle}>
          <small>{props.subtitle}</small>
        </Show>
      </span>
      <Show when={props.trailing}>
        <span class="action-card-end">{props.trailing}</span>
      </Show>
    </button>
  );
}

/** Translated sentence with inline JSX parts replacing {name} placeholders. */
export function Rich(props: { text: string; parts: Record<string, () => JSX.Element> }): JSX.Element {
  return (
    <>
      {props.text.split(/(\{\w+\})/).map((piece) => {
        const part = props.parts[piece.slice(1, -1)];
        return part ? part() : piece;
      })}
    </>
  );
}

/** Keyboard chip(s): "Ctrl+Shift+R" renders one keycap per key. */
export function Kbd(props: { keys: string }): JSX.Element {
  return (
    <span class="kbds">
      <For each={keyCaps(props.keys)}>{(k) => <kbd>{k}</kbd>}</For>
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

/** Positioned popup list with full keyboard navigation shared by dropdown & context menus. */
function MenuPopup(props: {
  items: MenuItem[];
  x: number;
  y: number;
  align?: "start" | "end";
  class?: string;
  onClose: (refocus: boolean) => void;
  keepOpenOn?: () => HTMLElement | undefined;
  flipAbove?: number;
}): JSX.Element {
  let menuEl: HTMLDivElement | undefined;
  const [shift, setShift] = createSignal({ x: 0, y: 0 });
  onMount(() => {
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (menuEl?.contains(target) || props.keepOpenOn?.()?.contains(target)) return;
      props.onClose(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose(true);
        return;
      }
      if (e.key === "Tab") {
        props.onClose(false);
        return;
      }
      const items = [...(menuEl?.querySelectorAll<HTMLButtonElement>(".menu-item:not(:disabled)") ?? [])];
      if (!items.length) return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        const next = e.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
        items[next].focus();
      } else if (e.key === "Home") {
        e.preventDefault();
        e.stopPropagation();
        items[0].focus();
      } else if (e.key === "End") {
        e.preventDefault();
        e.stopPropagation();
        items[items.length - 1].focus();
      }
    };
    const close = () => props.onClose(false);
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    queueMicrotask(() => {
      if (!menuEl) return;
      const pad = 8;
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const w = menuEl.offsetWidth;
      const naturalH = menuEl.scrollHeight;
      const roomBelow = Math.max(0, vh - props.y - pad);
      const anchorTop = props.flipAbove !== undefined ? props.flipAbove - 6 : props.y - 6;
      const roomAbove = Math.max(0, anchorTop - pad);
      const openBelow = naturalH <= roomBelow || roomBelow >= roomAbove;
      const maxH = Math.max(120, Math.min(vh - pad * 2, openBelow ? roomBelow : roomAbove));
      const finalH = Math.min(naturalH, maxH);
      const rawTop = openBelow ? props.y : anchorTop - finalH;
      const top = Math.max(pad, Math.min(vh - finalH - pad, rawTop));
      const rawLeft = props.align === "end" ? props.x - w : props.x;
      const clampedLeft = Math.max(pad, Math.min(vw - w - pad, rawLeft));
      const targetX = props.align === "end" ? clampedLeft + w : clampedLeft;
      menuEl.style.maxHeight = `${maxH}px`;
      menuEl.style.overflowY = naturalH > maxH ? "auto" : "";
      setShift({
        x: targetX - props.x,
        y: top - props.y,
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
            if ("separator" in it) return <hr class="menu-sep" />;
            if ("heading" in it)
              return (
                <div class="menu-heading" role="presentation">
                  {it.heading}
                </div>
              );
            return (
              <button
                type="button"
                {...(it.checked === undefined
                  ? { role: "menuitem" as const }
                  : { role: "menuitemcheckbox" as const, "aria-checked": it.checked })}
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
                  <span class="menu-kbd">{keyText(it.kbd ?? "")}</span>
                </Show>
              </button>
            );
          }}
        </For>
      </div>
    </Portal>
  );
}

/** A dropdown menu anchored to its trigger. Closes on selection, Tab, outside click, or Esc. */
export function Menu(props: {
  trigger: (p: { open: boolean; toggle: () => void; ref: (el: HTMLElement) => void }) => JSX.Element;
  items: () => MenuItem[];
  align?: "start" | "end";
  class?: string;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [pos, setPos] = createSignal({ x: 0, y: 0, top: 0 });
  let anchor: HTMLElement | undefined;
  createEffect(() => {
    anchor?.setAttribute("aria-haspopup", "menu");
    anchor?.setAttribute("aria-expanded", String(open()));
  });
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

export function TooltipLayer(): JSX.Element {
  const [tip, setTip] = createSignal<{ text: string; kbd?: string; x: number; y: number; below: boolean } | null>(null);
  let timer: number | undefined;
  let current: HTMLElement | null = null;
  const hide = () => {
    clearTimeout(timer);
    current = null;
    setTip(null);
  };
  const show = (el: HTMLElement) => {
    if (current !== el || !el.isConnected || !el.dataset.tip) return;
    if (window.innerHeight < 110) return;
    const r = el.getBoundingClientRect();
    const below = r.top < 64;
    setTip({
      text: el.dataset.tip,
      kbd: el.dataset.kbd,
      x: Math.min(Math.max(r.left + r.width / 2, 90), window.innerWidth - 90),
      y: below ? r.bottom + 8 : r.top - 8,
      below,
    });
  };
  onMount(() => {
    const over = (e: PointerEvent) => {
      const el = (e.target as HTMLElement).closest<HTMLElement>("[data-tip]");
      if (el === current) return;
      clearTimeout(timer);
      current = el;
      setTip(null);
      if (!el?.dataset.tip) return;
      timer = window.setTimeout(() => show(el), 420);
    };
    const focusIn = (e: FocusEvent) => {
      const el = (e.target as HTMLElement).closest<HTMLElement>("[data-tip]");
      if (!el?.matches(":focus-visible")) return;
      clearTimeout(timer);
      current = el;
      show(el);
    };
    document.addEventListener("focusin", focusIn);
    document.addEventListener("focusout", hide);
    document.addEventListener("pointerover", over);
    document.addEventListener("pointerdown", hide, true);
    window.addEventListener("blur", hide);
    window.addEventListener("keydown", hide, true);
    onCleanup(() => {
      document.removeEventListener("focusin", focusIn);
      document.removeEventListener("focusout", hide);
      document.removeEventListener("pointerover", over);
      document.removeEventListener("pointerdown", hide, true);
      window.removeEventListener("blur", hide);
      window.removeEventListener("keydown", hide, true);
    });
  });
  return (
    <Show when={tip()}>
      {(item) => (
        <div
          class="tooltip"
          classList={{ below: item().below }}
          role="tooltip"
          style={{ left: `${item().x}px`, top: `${item().y}px` }}
        >
          <span>{item().text}</span>
          <Show when={item().kbd}>
            <span class="tooltip-kbd">{keyText(item().kbd ?? "")}</span>
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

/** Push a toast. Pauses dismiss timer while hovered or focused. */
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
  setToasts((prev) => prev.filter((item) => item.id !== id));
}

const TOAST_ICON: Record<ToastKind, IconName> = { success: "check", error: "warning", info: "info" };

/** Renders the toast stack off-canvas in the bottom-right chrome zone. */
export function ToastHost(): JSX.Element {
  return (
    <div class="toasts" role="status" aria-live="polite">
      <For each={toasts()}>
        {(item) => (
          <div
            class="toast"
            classList={{ [item.kind]: true }}
            onPointerEnter={() => clearTimeout(toastTimers.get(item.id))}
            onPointerLeave={() => scheduleDismiss(item.id, 2000)}
            onFocusIn={() => clearTimeout(toastTimers.get(item.id))}
            onFocusOut={(e) => {
              if (!e.currentTarget.contains(e.relatedTarget as Node | null)) scheduleDismiss(item.id, 2000);
            }}
          >
            <span class="toast-icon">
              <Icon name={TOAST_ICON[item.kind]} size={14} stroke={2.2} />
            </span>
            <span class="toast-text">{item.text}</span>
            <Show when={item.action}>
              <button
                type="button"
                class="toast-action"
                onClick={() => {
                  item.action!.run();
                  dismissToast(item.id);
                }}
              >
                {item.action!.label}
              </button>
            </Show>
            <button
              type="button"
              class="toast-x"
              aria-label={t("Dismiss notification")}
              onClick={() => dismissToast(item.id)}
            >
              <Icon name="close" size={10} stroke={2.5} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
