// The home screen shown while no clip is loaded: pick what to record (with the options
// that matter set right here), pick up a crashed session, or reopen a recent project.
import { createMemo, For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon, type IconName } from "../icons";
import { prefs } from "../prefs";
import { COUNTDOWN_CHOICES, FPS_CHOICES, ZOOM_CHOICES } from "../recordOptions";
import { Kbd, Seg } from "../ui";
import { AudioPicker } from "./AudioControls";
import { CameraPicker } from "./CameraControls";

const SOURCES: { mode: "region" | "full" | "window"; icon: IconName; title: string; sub: string }[] = [
  { mode: "region", icon: "region", title: "Region", sub: "Any area, any aspect ratio" },
  { mode: "full", icon: "fullscreen", title: "Full screen", sub: "Everything on one display" },
  { mode: "window", icon: "window", title: "Window", sub: "Follow a single app window" },
];

export default function Home() {
  const ed = useEditor();
  const filtered = createMemo(() => {
    const q = ed.recentSearch().trim().toLowerCase();
    return q ? ed.recents().filter((r) => r.name.toLowerCase().includes(q)) : ed.recents();
  });

  return (
    <main class="home">
      <div class="home-inner">
        <section class="home-hero">
          <div class="home-eyebrow">
            <span class="rec-dot live" /> Ready to record
          </div>
          <h1 class="home-title">Record something worth showing.</h1>
          <p class="home-sub">
            Press Ctrl+Shift+Z and the camera glides in. Trim the fumbles, add a label or an arrow, and export a
            crisp GIF or MP4 in one go.
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
                  <span class="source-card-go">
                    <span class="rec-dot" />
                  </span>
                </button>
              )}
            </For>
          </div>

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
              <span>Audio</span>
              <AudioPicker />
            </div>
            <div class="home-opt">
              <span>Camera</span>
              <CameraPicker />
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

          <div class="home-keys">
            <span>
              <Kbd keys="Ctrl+Shift+R" /> record
            </span>
            <span>
              <Kbd keys="Ctrl+Shift+Z" /> zoom while recording
            </span>
            <span>
              <Kbd keys="Ctrl+Shift+X" /> stop
            </span>
          </div>
        </section>

        <Show when={ed.recoverable() !== null}>
          <button type="button" class="recover-card" onClick={() => void ed.onRecover()}>
            <span class="recover-icon">
              <Icon name="history" size={18} />
            </span>
            <span class="recover-text">
              <strong>Your last take wasn't saved</strong>
              <small>
                {ed.recoverable()!.toFixed(1)}s recording from a previous session, with the edits you made. Pick
                it back up.
              </small>
            </span>
            <span class="btn sm">Recover</span>
          </button>
        </Show>

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
            <button type="button" class="btn sm" onClick={() => void ed.onOpenProject()}>
              <Icon name="folder" size={14} /> Open project
            </button>
          </header>
          <Show
            when={ed.recents().length > 0}
            fallback={
              <div class="recents-empty">
                <Icon name="film" size={20} />
                <span>Projects you save show up here, ready to reopen.</span>
              </div>
            }
          >
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
          </Show>
        </section>
      </div>
    </main>
  );
}
