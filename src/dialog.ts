import { onCleanup } from "solid-js";

// Modal a11y, wired via a ref callback: tags the node role="dialog"/aria-modal, moves
// focus into it on open, keeps Tab/Shift+Tab cycling inside it, closes on Esc, and hands
// focus back to whatever opened it on close. onCleanup runs in the ref's owner (the <Show>),
// so restore fires when the dialog unmounts.
export const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';
export function dialogA11y(el: HTMLElement, label: string, onClose: () => void) {
  el.setAttribute("role", "dialog");
  el.setAttribute("aria-modal", "true");
  el.setAttribute("aria-label", label);
  if (!el.hasAttribute("tabindex")) el.tabIndex = -1;
  const opener = document.activeElement as HTMLElement | null;
  const items = () =>
    Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((n) => n.offsetParent !== null);
  queueMicrotask(() => (items()[0] ?? el).focus());
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
    opener?.focus?.();
  });
}
