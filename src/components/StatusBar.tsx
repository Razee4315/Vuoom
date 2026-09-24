// The status bar: the latest engine/status message on the left, clip facts and the
// capture settings for the next take on the right.
import { Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon } from "../icons";
import { prefs } from "../prefs";
import { FPS_CHOICES } from "../recordOptions";
import { Menu } from "../ui";

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
      <Menu
        align="end"
        items={() => [
          { heading: "Frame rate for the next recording" },
          ...FPS_CHOICES.map((c) => ({
            label: `${c.label}: ${c.tip}`,
            checked: prefs.captureFps() === c.value,
            onSelect: () => prefs.captureFps.set(c.value),
          })),
        ]}
        trigger={(m) => (
          <button
            type="button"
            class="sb-item sb-btn"
            classList={{ on: m.open }}
            ref={m.ref}
            aria-haspopup="menu"
            aria-expanded={m.open}
            data-tip="Frame rate for the next recording"
            onClick={m.toggle}
          >
            <Icon name="monitor" size={12} /> {prefs.captureFps()} fps
          </button>
        )}
      />
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
