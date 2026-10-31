// The home screen shown while no clip is loaded. Kept calm on purpose: pick what to record,
// the sound and camera next to it, and the rarer choices (frame rate, zoom, countdown)
// folded behind Options. A crashed session and recent projects show only when they exist.
import { createMemo, createSignal, For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon, type IconName } from "../icons";
import { RECORD_KEYS, stopKeys, zoomKeys } from "../hotkeys";
import { prefs } from "../prefs";
import { COUNTDOWN_CHOICES, FPS_CHOICES, ZOOM_CHOICES, type Choice } from "../recordOptions";
import { Kbd, Rich, Seg } from "../ui";
import { AudioPicker } from "./AudioControls";
import { CameraPicker } from "./CameraControls";

const SOURCES = (): { mode: "region" | "full" | "window"; icon: IconName; title: string; sub: string }[] => [
  { mode: "region", icon: "region", title: t("Region"), sub: t("Any area you drag out") },
  { mode: "full", icon: "fullscreen", title: t("Full screen"), sub: t("Everything on one display") },
  { mode: "window", icon: "window", title: t("Window"), sub: t("One app, wherever it goes") },
];

const labelOf = (choices: Choice[], v: number) => choices.find((c) => c.value === v)?.label ?? String(v);

export default function Home() {
  const ed = useEditor();
  const [more, setMore] = createSignal(false);
  const filtered = createMemo(() => {
    const q = ed.recentSearch().trim().toLowerCase();
    return q ? ed.recents().filter((r) => r.name.toLowerCase().includes(q)) : ed.recents();
  });
  // One line that says how the next take is set up, so the options can stay folded.
  const summary = () => {
    const strength = labelOf(ZOOM_CHOICES(), prefs.recordZoom());
    const zoom =
      prefs.recordZoom() <= 1 ? t("No zoom") : prefs.recordAutoZoom() ? t("Auto zoom {strength}", { strength }) : t("{strength} zoom", { strength });
    const count = prefs.countdown() > 0 ? t("{countdown}s countdown", { countdown: prefs.countdown() }) : t("No countdown");
    return `${labelOf(FPS_CHOICES(), prefs.captureFps())} · ${zoom} · ${count}`;
  };

  return (
    <main class="home">
      <div class="home-inner">
        <section class="home-hero">
          <h1 class="home-title">{t("Record something worth showing.")}</h1>
          <p class="home-sub">
            <Show
              when={prefs.recordAutoZoom() && prefs.recordZoom() > 1}
              fallback={
                <Rich
                  text={t("Choose what to capture. Press {keys} while you record to zoom in.")}
                  parts={{ keys: () => <Kbd keys={zoomKeys()} /> }}
                />
              }
            >
              <Rich
                text={t("Choose what to capture. Vuoom zooms in where you click, or press {keys} to zoom yourself.")}
                parts={{ keys: () => <Kbd keys={zoomKeys()} /> }}
              />
            </Show>
          </p>

          <div class="source-cards">
            <For each={SOURCES()}>
              {(s) => (
                <button
                  type="button"
                  class="source-card"
                  classList={{ primary: s.mode === ed.recordMode() }}
                  onClick={() => void ed.startRecord(s.mode)}
                >
                  <span class="source-card-icon">
                    <Icon name={s.icon} size={22} />
                  </span>
                  <span class="source-card-text">
                    <strong>{s.title}</strong>
                    <small>{s.sub}</small>
                  </span>
                  <span class="source-card-go" aria-hidden="true">
                    <span class="rec-dot" />
                  </span>
                </button>
              )}
            </For>
          </div>

          <div class="home-bar">
            <AudioPicker />
            <CameraPicker />
            <button
              type="button"
              class="home-more"
              classList={{ on: more() }}
              aria-expanded={more()}
              onClick={() => setMore(!more())}
            >
              <Icon name="sliders" size={14} />
              <span>{summary()}</span>
              <Icon name="chevronDown" size={12} class="home-more-chev" />
            </button>
          </div>

          <Show when={more()}>
            <div class="home-opts">
              <div class="home-opt">
                <span>{t("Frame rate")}</span>
                <Seg
                  label={t("Frame rate")}
                  value={prefs.captureFps()}
                  onChange={(v) => prefs.captureFps.set(v)}
                  options={FPS_CHOICES()}
                />
              </div>
              <div class="home-opt">
                <span>{t("Zooms")}</span>
                <Seg
                  label={t("How zooms are made")}
                  value={prefs.recordAutoZoom() ? "auto" : "hotkey"}
                  onChange={(v) => prefs.recordAutoZoom.set(v === "auto")}
                  options={[
                    { value: "auto", label: t("Auto"), tip: t("Calm zooms where you click, unless you use the hotkey") },
                    { value: "hotkey", label: t("Hotkey only"), tip: t("Zoom only when you press {keys}", { keys: zoomKeys() }) },
                  ]}
                />
              </div>
              <div class="home-opt">
                <span>{t("Zoom strength")}</span>
                <Seg
                  label={t("Zoom strength")}
                  value={prefs.recordZoom()}
                  onChange={(v) => prefs.recordZoom.set(v)}
                  options={ZOOM_CHOICES()}
                />
              </div>
              <div class="home-opt">
                <span>{t("Countdown")}</span>
                <Seg
                  label={t("Countdown")}
                  value={prefs.countdown()}
                  onChange={(v) => prefs.countdown.set(v)}
                  options={COUNTDOWN_CHOICES()}
                />
              </div>
            </div>
          </Show>
        </section>

        <Show when={ed.recoverable() !== null}>
          <button type="button" class="recover-card" onClick={() => void ed.onRecover()}>
            <span class="recover-icon">
              <Icon name="history" size={18} />
            </span>
            <span class="recover-text">
              <strong>{t("Your last take wasn't saved")}</strong>
              <small>{t("{seconds}s with the edits you made. Pick it back up.", { seconds: ed.recoverable()!.toFixed(1) })}</small>
            </span>
            <span class="btn sm">{t("Recover")}</span>
          </button>
        </Show>

        <Show
          when={ed.recents().length > 0}
          fallback={
            <div class="home-foot">
              <button type="button" class="btn ghost sm" onClick={() => void ed.onOpenProject()}>
                <Icon name="folder" size={14} /> {t("Open a saved project")}
              </button>
              <button type="button" class="btn ghost sm" onClick={() => void ed.trySample()}>
                <Icon name="wand" size={14} /> {t("Try a sample clip")}
              </button>
              <span class="home-keys">
                <Kbd keys={RECORD_KEYS} /> {t("record")} <span class="home-keys-sep" /> <Kbd keys={stopKeys()} /> {t("stop")}
              </span>
            </div>
          }
        >
          <section class="home-recents">
            <header class="home-recents-head">
              <h2>{t("Recent projects")}</h2>
              <Show when={ed.recents().length > 3}>
                <div class="input-wrap home-search">
                  <Icon name="search" size={14} />
                  <input
                    class="input"
                    type="search"
                    placeholder={t("Search")}
                    aria-label={t("Search recent projects")}
                    value={ed.recentSearch()}
                    onInput={(e) => ed.setRecentSearch(e.currentTarget.value)}
                  />
                </div>
              </Show>
              <button type="button" class="btn sm ghost" onClick={() => void ed.onOpenProject()}>
                <Icon name="folder" size={14} /> {t("Open project")}
              </button>
              <button type="button" class="btn sm ghost" onClick={() => void ed.trySample()}>
                <Icon name="wand" size={14} /> {t("Try a sample clip")}
              </button>
            </header>
            <div class="recents-grid">
              <For
                each={filtered()}
                fallback={<div class="recents-empty">{t('No projects match "{query}".', { query: ed.recentSearch() })}</div>}
              >
                {(r) => (
                  <div class="recent">
                    <button type="button" class="recent-open" onClick={() => void ed.openRecent(r.dir)} data-tip={r.dir}>
                      <div class="recent-thumb">
                        <Show when={r.thumb} fallback={<Icon name="film" size={22} />}>
                          <img src={r.thumb} alt="" draggable={false} />
                        </Show>
                      </div>
                      <div class="recent-meta">
                        <strong>{r.name.replace(/\.vuoom$/i, "")}</strong>
                        <small>{ed.fmtAgo(r.ts)}</small>
                      </div>
                    </button>
                    <button
                      type="button"
                      class="recent-remove"
                      aria-label={t("Remove {name} from recents", { name: r.name })}
                      data-tip={t("Remove from recents")}
                      onClick={() => ed.removeRecent(r.dir)}
                    >
                      <Icon name="close" size={10} stroke={2.4} />
                    </button>
                  </div>
                )}
              </For>
            </div>
          </section>
        </Show>
      </div>
    </main>
  );
}
