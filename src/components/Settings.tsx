// Settings: appearance, recording defaults, editing behavior, storage, the full shortcut
// sheet and app info, in one tabbed dialog (Ctrl+, / "?" opens it on Shortcuts).
import { createResource, For, Show } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";
import { invoke, isMock } from "../bridge";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { fmtBytes } from "../format";
import { Icon, type IconName } from "../icons";
import { layout, prefs, resetLayout } from "../prefs";
import { chooseSaveDir, openSaveDir, useSaveDir } from "../saveDir";
import { SHORTCUTS } from "../shortcuts";
import { applyTheme, THEMES } from "../themes";
import { Field, Seg, Switch } from "../ui";
import { AudioPicker } from "./AudioControls";
import { CameraPicker } from "./CameraControls";
import { CURSOR_MODES } from "../cursorMode";
import { COUNTDOWN_CHOICES, FPS_CHOICES, ZOOM_CHOICES } from "../recordOptions";

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
  const saveDir = useSaveDir();
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
                options={FPS_CHOICES}
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
            <Field label="Live preview" hint="Watch the take in the recording panel. Turn off to save processor time on a slower PC">
              <Switch
                checked={prefs.livePreview()}
                label="Live preview"
                onChange={(v) => {
                  prefs.livePreview.set(v);
                  void invoke("set_live_preview", { on: v }).catch(() => undefined);
                }}
              />
            </Field>
            <Field label="Countdown">
              <Seg
                value={prefs.countdown()}
                onChange={(v) => prefs.countdown.set(v)}
                options={COUNTDOWN_CHOICES}
              />
            </Field>
            <Field label="Zoom strength" hint="Used by Ctrl+Shift+Z while recording and by new zoom blocks">
              <Seg
                value={prefs.recordZoom()}
                onChange={(v) => prefs.recordZoom.set(v)}
                options={ZOOM_CHOICES}
              />
            </Field>
            <h3>Every new take starts with</h3>
            <Field label="Click ripples" hint="A ring at every click, so viewers see where you clicked">
              <Switch checked={prefs.newClicks()} label="Click ripples" onChange={(v) => prefs.newClicks.set(v)} />
            </Field>
            <Field label="Keystrokes" hint="Shortcuts you press appear as chips. Plain typing never shows">
              <Switch checked={prefs.newKeys()} label="Keystrokes" onChange={(v) => prefs.newKeys.set(v)} />
            </Field>
            <Field label="Frame" hint="A mat and backdrop around the recording, for a polished look">
              <Seg
                value={prefs.newFrame()}
                onChange={(v) => prefs.newFrame.set(v)}
                options={[
                  { value: "none", label: "None" },
                  { value: "subtle", label: "Subtle" },
                  { value: "studio", label: "Studio" },
                ]}
              />
            </Field>
            <Field label="Remove mic noise" hint="Filters fans, hum and hiss out of your voice. The original stays one click away">
              <Switch checked={prefs.newDenoise()} label="Remove mic noise" onChange={(v) => prefs.newDenoise.set(v)} />
            </Field>
            <p class="note">
              Each of these can be changed for a single clip in the editor. Vuoom stores frames losslessly
              compressed as they are captured, so a long take stays small on disk and survives a crash. The
              recording panel never appears in the capture.
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
            <h3>Save location</h3>
            <div class="storage-card">
              <Icon name="folder" size={20} />
              <div class="save-dir">
                <strong data-tip={saveDir() ?? ""}>{saveDir() || "Measuring…"}</strong>
                <small>{prefs.saveDir() ? "where exports and projects go" : "the default folder, in Videos"}</small>
              </div>
              <button type="button" class="btn sm" onClick={() => void chooseSaveDir()}>
                Change…
              </button>
            </div>
            <div class="save-dir-actions">
              <button type="button" class="btn sm ghost" onClick={() => void openSaveDir()}>
                <Icon name="external" size={13} /> Open folder
              </button>
              <Show when={prefs.saveDir()}>
                <button type="button" class="btn sm ghost" onClick={() => prefs.saveDir.set(null)}>
                  <Icon name="reset" size={13} /> Use default
                </button>
              </Show>
            </div>
            <Field label="Ask where to save" hint="Off: every export goes straight into this folder, never overwriting an earlier one">
              <Switch
                checked={prefs.askWhereToSave()}
                label="Ask where to save"
                onChange={(v) => prefs.askWhereToSave.set(v)}
              />
            </Field>
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
