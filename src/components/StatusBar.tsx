// The status bar: the latest engine/status message on the left, clip facts and the
// capture settings for the next take on the right.
import { Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon } from "../icons";
import { prefs } from "../prefs";

export default function StatusBar() {
  const ed = useEditor();
  return (
    <footer class="statusbar">
      <span class="sb-status" role="status" aria-live="polite">
        <span class="sb-dot" classList={{ busy: ed.playing() }} />
        {ed.status()}
      </span>
      <span class="grow" />
      <Show when={ed.hasClip()}>
        <span class="sb-item" data-tip="Recorded length">
          <Icon name="film" size={12} /> {ed.duration().toFixed(1)}s
        </span>
        <span class="sb-item" data-tip="Zoom blocks">
          <Icon name="zoomIn" size={12} /> {ed.zooms().length}
        </span>
      </Show>
      <button
        type="button"
        class="sb-item sb-btn"
        data-tip="Capture rate for the next recording (click to switch)"
        onClick={() => prefs.captureFps.set(prefs.captureFps() === 60 ? 30 : 60)}
      >
        <Icon name="monitor" size={12} /> {prefs.captureFps()} fps
      </button>
      <button
        type="button"
        class="sb-item sb-btn"
        data-tip="Keyboard shortcuts"
        data-kbd="?"
        onClick={() => ed.setSettingsTab("shortcuts")}
      >
        <Icon name="keyboard" size={12} /> Shortcuts
      </button>
    </footer>
  );
}
