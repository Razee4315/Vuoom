// Choose what to record: a display or a single app window. Shown when several displays
// are attached, or when the user asked to record a window.
import { createMemo, createSignal, For, Show } from "solid-js";
import { invoke } from "../bridge";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon } from "../icons";
import type { DisplayInfo, WindowInfo } from "../types";
import { EmptyState, IconButton, Seg } from "../ui";
import { WindowOutlineArt } from "./StudioArt";

export default function SourcePicker() {
  const ed = useEditor();
  const close = () => ed.closeSourcePicker();
  const [query, setQuery] = createSignal("");
  const [refreshing, setRefreshing] = createSignal(false);
  const [overrideSources, setOverrideSources] = createSignal<{
    displays: DisplayInfo[];
    windows: WindowInfo[];
  } | null>(null);
  let searchEl: HTMLInputElement | undefined;

  const displays = () => overrideSources()?.displays ?? ed.sources().displays;
  const windows = () => overrideSources()?.windows ?? ed.sources().windows;

  const filteredWindows = createMemo(() => {
    const q = query().trim().toLowerCase();
    if (!q) return windows();
    return windows().filter((w) => (w.title || "").toLowerCase().includes(q));
  });

  const filteredDisplays = createMemo(() => {
    const q = query().trim().toLowerCase();
    if (!q) return displays();
    return displays().filter((d) => {
      const label = t("Display {index}", { index: d.index }).toLowerCase();
      return label.includes(q) || d.name.toLowerCase().includes(q) || `${d.w}x${d.h}`.includes(q);
    });
  });

  const refresh = async () => {
    if (refreshing()) return;
    setRefreshing(true);
    try {
      const [nextDisplays, nextWindows] = await Promise.all([
        invoke<DisplayInfo[]>("list_displays").catch(() => displays()),
        invoke<WindowInfo[]>("list_windows").catch(() => windows()),
      ]);
      setOverrideSources({ displays: nextDisplays, windows: nextWindows });
    } finally {
      setRefreshing(false);
    }
  };

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
          <IconButton icon="close" size={14} tip={t("Close")} onClick={close} />
        </header>

        <Show when={windows().length > 0 && displays().length > 0}>
          <Seg
            full
            label={t("Capture source type")}
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

        <div class="source-toolbar">
          <div class="input-wrap source-search">
            <Icon name="search" size={14} />
            <input
              ref={searchEl}
              class="input"
              type="search"
              placeholder={ed.sourceTab() === "window" ? t("Filter windows…") : t("Filter displays…")}
              aria-label={t("Filter capture sources")}
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
            />
            <Show when={query().trim().length > 0}>
              <IconButton
                icon="close"
                size={12}
                class="sm source-search-clear"
                tip={t("Clear search")}
                onClick={() => {
                  setQuery("");
                  queueMicrotask(() => searchEl?.focus());
                }}
              />
            </Show>
          </div>
          <IconButton
            icon="reset"
            size={14}
            class="source-refresh"
            tip={t("Refresh sources")}
            disabled={refreshing()}
            onClick={() => void refresh()}
          />
        </div>

        <div class="source-list">
          <Show
            when={ed.sourceTab() === "display"}
            fallback={
              <Show
                when={windows().length > 0}
                fallback={
                  <EmptyState
                    class="source-empty"
                    art={<WindowOutlineArt />}
                    title={t("No windows available")}
                    description={t("Open an application window, then refresh the list.")}
                    actions={
                      <button type="button" class="btn sm" disabled={refreshing()} onClick={() => void refresh()}>
                        <Icon name="reset" size={13} /> {t("Refresh")}
                      </button>
                    }
                  />
                }
              >
                <For
                  each={filteredWindows()}
                  fallback={
                    <EmptyState
                      class="source-empty"
                      art={<WindowOutlineArt />}
                      title={t("No matches")}
                      description={t('No windows match "{query}".', { query: query() })}
                      actions={
                        <button
                          type="button"
                          class="btn sm"
                          onClick={() => {
                            setQuery("");
                            queueMicrotask(() => searchEl?.focus());
                          }}
                        >
                          {t("Clear search")}
                        </button>
                      }
                    />
                  }
                >
                  {(w) => (
                    <button
                      type="button"
                      class="source-item"
                      onClick={() => void ed.beginRecordWith({ kind: "window", hwnd: w.hwnd, label: w.title })}
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
              </Show>
            }
          >
            <For
              each={filteredDisplays()}
              fallback={
                <EmptyState
                  class="source-empty"
                  art={<WindowOutlineArt />}
                  title={t("No matches")}
                  description={t('No displays match "{query}".', { query: query() })}
                  actions={
                    <button
                      type="button"
                      class="btn sm"
                      onClick={() => {
                        setQuery("");
                        queueMicrotask(() => searchEl?.focus());
                      }}
                    >
                      {t("Clear search")}
                    </button>
                  }
                />
              }
            >
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
