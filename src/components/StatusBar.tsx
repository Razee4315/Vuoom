// The status bar: semantic status indicator on the left, tabular clip facts and next-take
// capture defaults on the right.
import { Show } from "solid-js";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon } from "../icons";
import { prefs } from "../prefs";
import { FPS_CHOICES } from "../recordOptions";
import { Menu } from "../ui";

export default function StatusBar() {
  const ed = useEditor();
  const isError = () => ed.statusKind() === "error";
  const isBusy = () => ed.statusKind() === "busy" || ed.playing();
  return (
    <footer class="statusbar">
      <span class="sb-status" classList={{ error: isError() }} role="status" aria-live="polite">
        <Show
          when={isError()}
          fallback={<span class="sb-dot" classList={{ busy: isBusy() }} aria-hidden="true" />}
        >
          <Icon name="warning" size={12} class="sb-err-icon" />
        </Show>
        <span>{ed.status()}</span>
      </span>
      <span class="grow" />
      <Show when={ed.hasClip()}>
        <span class="sb-item tabular" data-tip={t("Recorded length")}>
          <Icon name="film" size={12} /> {ed.duration().toFixed(1)}s
        </span>
        <span class="sb-item tabular" data-tip={t("Zoom blocks")}>
          <Icon name="zoomIn" size={12} /> {ed.zooms().length}
        </span>
      </Show>
      <Menu
        align="end"
        items={() => [
          { heading: t("Frame rate for the next recording") },
          ...FPS_CHOICES().map((c) => ({
            label: `${c.label}: ${c.tip}`,
            checked: prefs.captureFps() === c.value,
            onSelect: () => prefs.captureFps.set(c.value),
          })),
        ]}
        trigger={(m) => (
          <button
            type="button"
            class="sb-item sb-btn tabular"
            classList={{ on: m.open }}
            ref={m.ref}
            data-tip={t("Frame rate for the next recording")}
            onClick={m.toggle}
          >
            <Icon name="monitor" size={12} /> {prefs.captureFps()} fps
          </button>
        )}
      />
      <button
        type="button"
        class="sb-item sb-btn"
        data-tip={t("Keyboard shortcuts")}
        data-kbd="?"
        onClick={() => ed.setSettingsTab("shortcuts")}
      >
        <Icon name="keyboard" size={12} /> {t("Shortcuts")}
      </button>
    </footer>
  );
}
