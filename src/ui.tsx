// Small shared UI primitives for the redesigned interface: a segmented control,
// keyboard chips, and a toast stack for action feedback.

import { createSignal, For, Show, type JSX } from "solid-js";

/** Segmented control: a group of exclusive options rendered as one pill-shaped switcher.
 *  options: value/label pairs (label may be any JSX). Uses the toggle-button pattern
 *  (aria-pressed) so it stays accessible without radio-input semantics. */
export function Seg<T extends string>(props: {
  options: { value: T; label: JSX.Element; title?: string }[];
  value: T;
  onChange: (v: T) => void;
  class?: string;
  disabled?: boolean;
}): JSX.Element {
  return (
    <div class={`seg ${props.class ?? ""}`} classList={{ disabled: !!props.disabled }}>
      <For each={props.options}>
        {(o) => (
          <button
            type="button"
            aria-pressed={props.value === o.value}
            title={o.title}
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

/** Keyboard chip. */
export function Kbd(props: { children: JSX.Element }): JSX.Element {
  return <kbd>{props.children}</kbd>;
}

// ── toasts ─────────────────────────────────────────────────────────────────────
export type ToastKind = "info" | "success" | "error";
export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
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
 *  toast pauses the timer, and every toast has a real, focusable dismiss button. */
export function toast(text: string, kind: ToastKind = "info", ttl = 3800): void {
  const id = nextToastId++;
  setToasts((prev) => [...prev.slice(-3), { id, kind, text }]);
  scheduleDismiss(id, kind === "error" ? ttl + 2200 : ttl);
}

export function dismissToast(id: number): void {
  clearTimeout(toastTimers.get(id));
  toastTimers.delete(id);
  setToasts((prev) => prev.filter((t) => t.id !== id));
}

/** Renders the toast stack. Mount once, near the end of the app tree. */
export function ToastHost(): JSX.Element {
  // Pause auto-dismiss timers while hovered is skipped: toasts are read-only, short lived,
  // and the stack keeps the last 3. Exit animation runs via the CSS `out` class.
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
            <Show when={t.kind === "success"}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M4.5 12.5l5 5 10-11" />
              </svg>
            </Show>
            <Show when={t.kind === "error"}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <circle cx="12" cy="12" r="9" />
                <path d="M12 7.5v5.5M12 16.5h.01" />
              </svg>
            </Show>
            <Show when={t.kind === "info"}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <circle cx="12" cy="12" r="9" />
                <path d="M12 11v5M12 7.5h.01" />
              </svg>
            </Show>
            <span>{t.text}</span>
            <button
              type="button"
              class="toast-x"
              aria-label="Dismiss notification"
              onClick={() => dismissToast(t.id)}
            >
              <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
