// The home screen shown while no clip is loaded. Kept calm on purpose: pick what to record,
// the sound and camera next to it, and the rarer choices (frame rate, zoom, countdown)
// folded behind Options. A crashed session and recent projects show only when they exist.
import { createMemo, createSignal, For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon, type IconName } from "../icons";
import { prefs } from "../prefs";
import { COUNTDOWN_CHOICES, FPS_CHOICES, ZOOM_CHOICES, type Choice } from "../recordOptions";
import { Kbd, Seg } from "../ui";
import { AudioPicker } from "./AudioControls";
import { CameraPicker } from "./CameraControls";

const SOURCES: { mode: "region" | "full" | "window"; icon: IconName; title: string; sub: string }[] = [
  { mode: "region", icon: "region", title: "Region", sub: "Any area you drag out" },
  { mode: "full", icon: "fullscreen", title: "Full screen", sub: "Everything on one display" },
  { mode: "window", icon: "window", title: "Window", sub: "One app, wherever it goes" },
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
    const zoom = prefs.recordZoom() > 1 ? `${labelOf(ZOOM_CHOICES, prefs.recordZoom())} zoom` : "No zoom";
    const count = prefs.countdown() > 0 ? `${prefs.countdown()}s countdown` : "No countdown";
    return `${labelOf(FPS_CHOICES, prefs.captureFps())} · ${zoom} · ${count}`;
  };

  return (
    <main class="home">
      <div class="home-inner">
        <section class="home-hero">
          <h1 class="home-title">Record something worth showing.</h1>
          <p class="home-sub">
            Choose what to capture. Press <Kbd keys="Ctrl+Shift+Z" /> while you record to zoom in.
          </p>

          <div class="source-cards">
            <For each={SOURCES}>
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
                <span>Frame rate</span>
                <Seg
                  label="Frame rate"
                  value={prefs.captureFps()}
                  onChange={(v) => prefs.captureFps.set(v)}
                  options={FPS_CHOICES}
                />
              </div>
              <div class="home-opt">
                <span>Zoom on Ctrl+Shift+Z</span>
                <Seg
                  label="Zoom strength"
                  value={prefs.recordZoom()}
                  onChange={(v) => prefs.recordZoom.set(v)}
                  options={ZOOM_CHOICES}
                />
              </div>
              <div class="home-opt">
                <span>Countdown</span>
                <Seg
                  label="Countdown"
                  value={prefs.countdown()}
                  onChange={(v) => prefs.countdown.set(v)}
                  options={COUNTDOWN_CHOICES}
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
              <strong>Your last take wasn't saved</strong>
              <small>{ed.recoverable()!.toFixed(1)}s with the edits you made. Pick it back up.</small>
            </span>
            <span class="btn sm">Recover</span>
          </button>
        </Show>

        <Show
          when={ed.recents().length > 0}
          fallback={
            <div class="home-foot">
              <button type="button" class="btn ghost sm" onClick={() => void ed.onOpenProject()}>
                <Icon name="folder" size={14} /> Open a saved project
              </button>
              <span class="home-keys">
                <Kbd keys="Ctrl+Shift+R" /> record <span class="home-keys-sep" /> <Kbd keys="Ctrl+Shift+X" /> stop
              </span>
            </div>
          }
        >
          <section class="home-recents">
            <header class="home-recents-head">
              <h2>Recent projects</h2>
              <Show when={ed.recents().length > 3}>
                <div class="input-wrap home-search">
                  <Icon name="search" size={14} />
                  <input
                    class="input"
                    type="search"
                    placeholder="Search"
                    aria-label="Search recent projects"
                    value={ed.recentSearch()}
                    onInput={(e) => ed.setRecentSearch(e.currentTarget.value)}
                  />
                </div>
              </Show>
              <button type="button" class="btn sm ghost" onClick={() => void ed.onOpenProject()}>
                <Icon name="folder" size={14} /> Open project
              </button>
            </header>
            <div class="recents-grid">
              <For
                each={filtered()}
                fallback={<div class="recents-empty">No projects match "{ed.recentSearch()}".</div>}
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
                      aria-label={`Remove ${r.name} from recents`}
                      data-tip="Remove from recents"
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
