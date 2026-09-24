// Settings: appearance, recording defaults, editing behavior, storage, the full shortcut
// sheet and app info, in one tabbed dialog (Ctrl+, / "?" opens it on Shortcuts).
import { createResource, For, Show } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";
import { isMock } from "../bridge";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { fmtBytes } from "../format";
import { Icon, type IconName } from "../icons";
import { layout, prefs, resetLayout } from "../prefs";
import { SHORTCUTS } from "../shortcuts";
import { applyTheme, THEMES } from "../themes";
import { Field, Seg, Switch } from "../ui";
import { AudioPicker } from "./AudioControls";
import { CameraPicker } from "./CameraControls";
import { CURSOR_MODES } from "../cursorMode";

type Tab = "general" | "recording" | "editing" | "storage" | "shortcuts" | "about";
const TABS: { id: Tab; label: string; icon: IconName }[] = [
  { id: "general", label: "Appearance", icon: "palette" },
  { id: "recording", label: "Recording", icon: "region" },
  { id: "editing", label: "Editing", icon: "sliders" },
  { id: "storage", label: "Storage", icon: "drive" },
  { id: "shortcuts", label: "Shortcuts", icon: "keyboard" },
  { id: "about", label: "About", icon: "info" },
];

// Swatch colors for the theme cards (backdrop, panel, text).
const THEME_ART: Record<string, [string, string, string]> = {
  "mono-dark": ["#08080a", "#16161a", "#f5f5f6"],
  "mono-light": ["#ececf0", "#ffffff", "#141417"],
  graphite: ["#0f1215", "#1b2025", "#eaedf0"],
  paper: ["#e9e5dc", "#f7f5f0", "#28261f"],
  midnight: ["#060910", "#111722", "#e8edf5"],
};

export default function Settings() {
  const ed = useEditor();
  const tab = () => ed.settingsTab() as Tab;
  const close = () => ed.setSettingsTab(null);
  const [version] = createResource(async () => (isMock ? "dev" : await getVersion().catch(() => "?")));

  return (
    <div class="modal-backdrop" onClick={close}>
      <div
        class="modal settings"
        ref={(el) => dialogA11y(el, "Settings", close)}
        onClick={(e) => e.stopPropagation()}
      >
        <nav class="settings-nav" aria-label="Settings sections">
          <h2>Settings</h2>
          <For each={TABS}>
            {(t) => (
              <button
                type="button"
                class="settings-tab"
                classList={{ on: tab() === t.id }}
                onClick={() => ed.setSettingsTab(t.id)}
              >
                <Icon name={t.icon} size={15} />
                {t.label}
              </button>
            )}
          </For>
        </nav>
        <div class="settings-body">
          <button type="button" class="ibtn settings-close" aria-label="Close settings" onClick={close}>
            <Icon name="close" size={14} />
          </button>

          <Show when={tab() === "general"}>
            <h3>Theme</h3>
            <div class="theme-grid">
              <For each={THEMES}>
                {(t) => {
                  const [bg, panel, text] = THEME_ART[t.id] ?? ["#000", "#111", "#fff"];
                  return (
                    <button
                      type="button"
                      class="theme-card"
                      classList={{ on: ed.theme() === t.id }}
                      aria-pressed={ed.theme() === t.id}
                      onClick={() => {
                        applyTheme(t.id);
                        ed.setTheme(t.id);
                      }}
                    >
                      <span class="theme-art" style={{ background: bg }}>
                        <span class="theme-art-panel" style={{ background: panel }}>
                          <span style={{ background: text }} />
                          <span style={{ background: text, opacity: 0.4 }} />
                          <span class="theme-art-rec" />
                        </span>
                      </span>
                      <span>{t.name}</span>
                    </button>
                  );
                }}
              </For>
            </div>
            <h3>Interface</h3>
            <Field label="Reduce motion" hint="Turn off panel and control animations">
              <Switch checked={prefs.reduceMotion()} label="Reduce motion" onChange={(v) => prefs.reduceMotion.set(v)} />
            </Field>
            <Field label="Show hints" hint="Lane hints and helper notes for newcomers">
              <Switch checked={prefs.hints()} label="Show hints" onChange={(v) => prefs.hints.set(v)} />
            </Field>
            <Field label="Compact tool rail">
              <Switch checked={layout.railCompact()} label="Compact tool rail" onChange={(v) => layout.railCompact.set(v)} />
            </Field>
            <Field label="Panel layout">
              <button type="button" class="btn sm" onClick={resetLayout}>
                <Icon name="reset" size={13} /> Reset layout
              </button>
            </Field>
          </Show>

          <Show when={tab() === "recording"}>
            <h3>New recordings</h3>
            <Field label="Frame rate" hint="30 fps keeps files and GIFs lean; 60 fps is smoother for MP4">
              <Seg
                value={prefs.captureFps()}
                onChange={(v) => prefs.captureFps.set(v)}
                options={[
                  { value: 24, label: "24" },
                  { value: 30, label: "30" },
                  { value: 60, label: "60" },
                ]}
              />
            </Field>
            <Field
              label="Mouse pointer"
              hint="Smooth hides the real pointer and draws a clean one that glides, sized in the editor. Zooms follow it either way"
            >
              <Seg
                value={prefs.cursorMode()}
                onChange={(v) => prefs.cursorMode.set(v)}
                options={CURSOR_MODES.map((c) => ({ value: c.value, label: c.label }))}
              />
            </Field>
            <Field label="Audio" hint="Microphone narration and computer sound, each on its own track">
              <AudioPicker />
            </Field>
            <Field label="Camera" hint="Your webcam as a bubble over the recording, placed and shaped in the editor">
              <CameraPicker />
            </Field>
            <Field label="Countdown">
              <Seg
                value={prefs.countdown()}
                onChange={(v) => prefs.countdown.set(v)}
                options={[
                  { value: 0, label: "None" },
                  { value: 3, label: "3s" },
                  { value: 5, label: "5s" },
                  { value: 10, label: "10s" },
                ]}
              />
            </Field>
            <Field label="Zoom strength" hint="Used by Ctrl+Shift+Z while recording and by new zoom blocks">
              <Seg
                value={prefs.recordZoom()}
                onChange={(v) => prefs.recordZoom.set(v)}
                options={[
                  { value: 1, label: "Off" },
                  { value: 1.5, label: "1.5×" },
                  { value: 1.8, label: "1.8×" },
                  { value: 2, label: "2×" },
                  { value: 2.5, label: "2.5×" },
                ]}
              />
            </Field>
            <p class="note">
              Vuoom stores frames losslessly compressed as they are captured, so a long take stays small on
              disk and survives a crash. The recording window is excluded from the capture.
            </p>
          </Show>

          <Show when={tab() === "editing"}>
            <h3>Timeline</h3>
            <Field label="Magnetic snapping" hint="Edges snap to the playhead, other edges and the grid. Hold Alt to bypass">
              <Switch checked={prefs.snapping()} label="Magnetic snapping" onChange={(v) => prefs.snapping.set(v)} />
            </Field>
            <Field label="Annotations on one lane">
              <Switch checked={layout.compactNotes()} label="Annotations on one lane" onChange={(v) => layout.compactNotes.set(v)} />
            </Field>
            <Field label="Loop playback">
              <Switch
                checked={prefs.loop()}
                label="Loop playback"
                onChange={(v) => {
                  prefs.loop.set(v);
                  ed.setLooping(v);
                }}
              />
            </Field>
            <h3>Export</h3>
            <Field label="Default format">
              <Seg
                value={prefs.exportFormat()}
                onChange={(v) => prefs.exportFormat.set(v)}
                options={[
                  { value: "gif", label: "GIF" },
                  { value: "mp4", label: "MP4" },
                ]}
              />
            </Field>
          </Show>

          <Show when={tab() === "storage"}>
            <h3>Crash recovery</h3>
            <p class="note">
              Every take streams to a recovery folder while you record, so a crash or an accidental close
              never loses it. Vuoom keeps the last two sessions.
            </p>
            <div class="storage-card">
              <Icon name="drive" size={20} />
              <div>
                <strong>{ed.recoveryBytes() === null ? "Measuring…" : fmtBytes(ed.recoveryBytes()!)}</strong>
                <small>used by recovery data</small>
              </div>
              <button
                type="button"
                class="btn sm danger"
                disabled={ed.clearingStorage() || !ed.recoveryBytes()}
                onClick={() => void ed.clearStorage()}
              >
                {ed.clearingStorage() ? "Clearing…" : "Clear"}
              </button>
            </div>
            <p class="note">Clearing keeps the clip that is open right now.</p>
          </Show>

          <Show when={tab() === "shortcuts"}>
            <div class="shortcut-grid">
              <For each={SHORTCUTS}>
                {(g) => (
                  <section>
                    <h3>{g.group}</h3>
                    <For each={g.items}>
                      {(s) => (
                        <div class="shortcut-row">
                          <span>{s.label}</span>
                          <span class="kbds">
                            <For each={s.keys}>{(k) => <kbd>{k}</kbd>}</For>
                          </span>
                        </div>
                      )}
                    </For>
                  </section>
                )}
              </For>
            </div>
          </Show>

          <Show when={tab() === "about"}>
            <div class="about">
              <div class="about-mark">
                <Icon name="film" size={26} />
              </div>
              <h3>Vuoom {version() ? `v${version()}` : ""}</h3>
              <p class="note">Screen recordings that zoom where it matters. Free and open source, Apache-2.0.</p>
              <Show
                when={ed.update()}
                fallback={
                  <button type="button" class="btn sm" onClick={() => void ed.checkForUpdate()}>
                    <Icon name="reset" size={13} /> Check for updates
                  </button>
                }
              >
                <button type="button" class="btn primary sm" disabled={ed.updating()} onClick={() => void ed.runUpdate()}>
                  <Icon name="download" size={13} /> Install v{ed.update()!.version}
                </button>
              </Show>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
