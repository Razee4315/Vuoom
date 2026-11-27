import { onCleanup } from "solid-js";

// Modal accessibility: manages background .app inert/aria-hidden state via a reference count,
// tags dialogs role="dialog"/aria-modal, traps Tab/Shift+Tab focus, closes on Esc, and safely
// restores focus when a dialog unmounts.
export const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

let openModalCount = 0;

function setAppInert(inert: boolean): void {
  const app = document.querySelector<HTMLElement>(".app");
  if (!app) return;
  app.inert = inert;
  if (inert) app.setAttribute("aria-hidden", "true");
  else app.removeAttribute("aria-hidden");
}

/** Increment the modal/overlay inert lock on .app and return an idempotent release fn. */
export function lockAppInert(): () => void {
  let unlocked = false;
  openModalCount++;
  if (openModalCount === 1) setAppInert(true);
  return () => {
    if (unlocked) return;
    unlocked = true;
    openModalCount = Math.max(0, openModalCount - 1);
    if (openModalCount === 0) setAppInert(false);
  };
}

export function dialogA11y(
  el: HTMLElement,
  label: string,
  onClose: () => void,
  opts?: { initialFocus?: () => HTMLElement | null },
) {
  el.setAttribute("role", "dialog");
  el.setAttribute("aria-modal", "true");
  el.setAttribute("aria-label", label);
  if (!el.hasAttribute("tabindex")) el.tabIndex = -1;
  const opener = document.activeElement as HTMLElement | null;
  const unlockInert = lockAppInert();
  const items = () =>
    Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((n) => n.offsetParent !== null);
  queueMicrotask(() => (opts?.initialFocus?.() ?? items()[0] ?? el).focus());
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const f = items();
    if (f.length === 0) {
      e.preventDefault();
      el.focus();
      return;
    }
    const first = f[0];
    const last = f[f.length - 1];
    const active = document.activeElement;
    if (!el.contains(active)) {
      e.preventDefault();
      (e.shiftKey ? last : first).focus();
      return;
    }
    if (e.shiftKey && (active === first || active === el)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  };
  el.addEventListener("keydown", onKey);
  onCleanup(() => {
    el.removeEventListener("keydown", onKey);
    unlockInert();
    queueMicrotask(() => {
      if (openModalCount > 0) return;
      if (opener?.isConnected && opener.offsetParent !== null) {
        opener.focus();
      } else {
        document
          .querySelector<HTMLElement>(".app button:not([disabled]), .app input:not([disabled])")
          ?.focus();
      }
    });
  });
}
