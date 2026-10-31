// Choose what to record: a display or a single app window. Shown when several displays
// are attached, or when the user asked to record a window.
import { For, Show } from "solid-js";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon } from "../icons";
import { Seg } from "../ui";

export default function SourcePicker() {
  const ed = useEditor();
  const close = () => ed.setShowSource(false);
  return (
    <div class="modal-backdrop" onClick={close}>
      <div
        class="modal source-modal"
        ref={(el) => dialogA11y(el, t("Choose what to record"), close)}
        onClick={(e) => e.stopPropagation()}
      >
        <header class="modal-head">
          <div>
            <h2>{t("What should Vuoom record?")}</h2>
            <p class="muted small">{t("Pick a display to frame a region on it, or one app window.")}</p>
          </div>
          <button type="button" class="ibtn" aria-label={t("Close")} onClick={close}>
            <Icon name="close" size={14} />
          </button>
        </header>
        <Show when={ed.sources().windows.length > 0 && ed.sources().displays.length > 0}>
          <Seg
            full
            value={ed.sourceTab()}
            onChange={(v) => ed.setSourceTab(v)}
            options={[
              {
                value: "display",
                label: (
                  <>
                    <Icon name="monitor" size={14} /> {t("Displays")}
                  </>
                ),
              },
              {
                value: "window",
                label: (
                  <>
                    <Icon name="window" size={14} /> {t("Windows")}
                  </>
                ),
              },
            ]}
          />
        </Show>
        <div class="source-list">
          <Show
            when={ed.sourceTab() === "display"}
            fallback={
              <For
                each={ed.sources().windows}
                fallback={<p class="note">{t("No windows available to capture.")}</p>}
              >
                {(w) => (
                  <button
                    type="button"
                    class="source-item"
                    onClick={() =>
                      void ed.beginRecordWith({ kind: "window", hwnd: w.hwnd, label: w.title })
                    }
                  >
                    <span class="source-icon">
                      <Icon name="window" size={18} />
                    </span>
                    <span class="source-meta">
                      <strong>{w.title || t("Untitled window")}</strong>
                      <small>
                        {w.w} × {w.h}
                      </small>
                    </span>
                    <Icon name="chevronRight" size={14} class="source-go" />
                  </button>
                )}
              </For>
            }
          >
            <For each={ed.sources().displays}>
              {(d) => (
                <button
                  type="button"
                  class="source-item"
                  onClick={() =>
                    void ed.beginRecordWith({ kind: "display", name: d.name, label: t("Display {index}", { index: d.index }) })
                  }
                >
                  <span class="source-icon">
                    <span class="source-screen" style={{ "aspect-ratio": `${d.w} / ${d.h}` }} />
                  </span>
                  <span class="source-meta">
                    <strong>
                      {t("Display {index}", { index: d.index })}
                      <Show when={d.primary}>
                        <span class="badge">{t("Primary")}</span>
                      </Show>
                    </strong>
                    <small>
                      {d.w} × {d.h}
                    </small>
                  </span>
                  <Icon name="chevronRight" size={14} class="source-go" />
                </button>
              )}
            </For>
          </Show>
        </div>
      </div>
    </div>
  );
}
