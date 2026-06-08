import { createSignal, onMount, onCleanup, Show } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Custom minimize / maximize / close controls for the frameless window. */
export default function WindowControls() {
  const appWindow = getCurrentWindow();
  const [maximized, setMaximized] = createSignal(false);

  onMount(async () => {
    try {
      setMaximized(await appWindow.isMaximized());
      const unlisten = await appWindow.onResized(async () => {
        setMaximized(await appWindow.isMaximized());
      });
      onCleanup(unlisten);
    } catch {
      // Not running inside a Tauri window (e.g. browser dev) — ignore.
    }
  });

  return (
    <div class="winctl">
      <button
        class="winbtn"
        title="Minimize"
        aria-label="Minimize"
        onClick={() => void appWindow.minimize()}
      >
        <svg width="11" height="11" viewBox="0 0 11 11">
          <line x1="1" y1="6" x2="10" y2="6" />
        </svg>
      </button>

      <button
        class="winbtn"
        title="Maximize"
        aria-label="Maximize"
        onClick={() => void appWindow.toggleMaximize()}
      >
        <Show
          when={maximized()}
          fallback={
            <svg width="11" height="11" viewBox="0 0 11 11">
              <rect x="1.5" y="1.5" width="8" height="8" />
            </svg>
          }
        >
          <svg width="11" height="11" viewBox="0 0 11 11">
            <rect x="1.5" y="3" width="6" height="6" />
            <path d="M3.5 3 V1.5 H9.5 V7.5 H8" />
          </svg>
        </Show>
      </button>

      <button
        class="winbtn close"
        title="Close"
        aria-label="Close"
        onClick={() => void appWindow.close()}
      >
        <svg width="11" height="11" viewBox="0 0 11 11">
          <line x1="1.5" y1="1.5" x2="9.5" y2="9.5" />
          <line x1="9.5" y1="1.5" x2="1.5" y2="9.5" />
        </svg>
      </button>
    </div>
  );
}
