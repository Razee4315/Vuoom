import { createSignal, onMount, onCleanup, For } from "solid-js";
import { THEMES, applyTheme } from "./themes";

/** A compact dropdown for picking the editor theme. */
export default function ThemeMenu(props: {
  current: string;
  onSelect: (id: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  let root: HTMLDivElement | undefined;

  const onDocClick = (e: MouseEvent) => {
    if (root && !root.contains(e.target as Node)) setOpen(false);
  };
  onMount(() => document.addEventListener("click", onDocClick));
  onCleanup(() => document.removeEventListener("click", onDocClick));

  const pick = (id: string) => {
    applyTheme(id);
    props.onSelect(id);
    setOpen(false);
  };

  return (
    <div class="thememenu" ref={(el) => (root = el)}>
      <button
        class="winbtn"
        title="Theme"
        aria-label="Theme"
        onClick={(e) => {
          e.stopPropagation();
          setOpen(!open());
        }}
      >
        <svg width="12" height="12" viewBox="0 0 12 12">
          <circle cx="6" cy="6" r="4.5" />
          <path d="M6 1.5 A4.5 4.5 0 0 1 6 10.5 Z" class="fill" />
        </svg>
      </button>

      {open() && (
        <div class="thememenu-list">
          <For each={THEMES}>
            {(t) => (
              <button
                classList={{ "theme-item": true, active: props.current === t.id }}
                onClick={() => pick(t.id)}
              >
                <span class={`swatch s-${t.id}`} />
                {t.name}
              </button>
            )}
          </For>
        </div>
      )}
    </div>
  );
}
