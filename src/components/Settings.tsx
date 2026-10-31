// Settings: appearance, recording defaults, editing behavior, storage, the full shortcut
// sheet and app info, in one tabbed dialog (Ctrl+, / "?" opens it on Shortcuts).
import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";
import { invoke, isMock } from "../bridge";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { fmtBytes } from "../format";
import {
  chordFromEvent,
  pushHotkeys,
  RECORD_KEYS,
  STOP_DEFAULT,
  stopKeys,
  ZOOM_DEFAULT,
  zoomKeys,
} from "../hotkeys";
import { type Locale, locale, setLocale, t } from "../i18n";
import { Icon, type IconName } from "../icons";
import { layout, type Persisted, prefs, resetLayout } from "../prefs";
import { chooseSaveDir, openSaveDir, useSaveDir } from "../saveDir";
import { SHORTCUTS } from "../shortcuts";
import { applyTheme, THEMES } from "../themes";
import { Field, Kbd, Seg, Switch } from "../ui";
import { AudioPicker } from "./AudioControls";
import { CameraPicker } from "./CameraControls";
import { CURSOR_MODES } from "../cursorMode";
import { COUNTDOWN_CHOICES, FPS_CHOICES, ZOOM_CHOICES } from "../recordOptions";

type Tab = "general" | "recording" | "editing" | "storage" | "shortcuts" | "about";
const TABS = (): { id: Tab; label: string; icon: IconName }[] => [
  { id: "general", label: t("Appearance"), icon: "palette" },
  { id: "recording", label: t("Recording"), icon: "region" },
  { id: "editing", label: t("Editing"), icon: "sliders" },
  { id: "storage", label: t("Storage"), icon: "drive" },
  { id: "shortcuts", label: t("Shortcuts"), icon: "keyboard" },
  { id: "about", label: t("About"), icon: "info" },
];

// Swatch colors for the theme cards (backdrop, panel, text).
const THEME_ART: Record<string, [string, string, string]> = {
  "mono-dark": ["#08080a", "#16161a", "#f5f5f6"],
  "mono-light": ["#ececf0", "#ffffff", "#141417"],
  graphite: ["#0f1215", "#1b2025", "#eaedf0"],
  paper: ["#e9e5dc", "#f7f5f0", "#28261f"],
  midnight: ["#060910", "#111722", "#e8edf5"],
};

/** One rebindable hotkey: shows the chord; click it, then press the new one. */
function HotkeyField(props: {
  label: string;
  hint: string;
  pref: Persisted<string>;
  fallback: string;
  /** Chords this one may not take, with what already uses each. */
  taken: () => { keys: string; clash: string }[];
}) {
  const [listening, setListening] = createSignal(false);
  const [problem, setProblem] = createSignal("");
  // Listens ahead of everything else, so the keys pressed here reach neither the editor's
  // shortcuts nor the dialog (Esc cancels the change, it doesn't close Settings).
  const onKey = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopImmediatePropagation();
    if (e.key === "Escape") {
      stop();
      return;
    }
    const got = chordFromEvent(e);
    if ("pending" in got) return;
    if ("error" in got) {
      setProblem(got.error);
      return;
    }
    const taken = props.taken().find((k) => k.keys === got.chord);
    if (taken) {
      setProblem(taken.clash);
      return;
    }
    props.pref.set(got.chord);
    pushHotkeys();
    stop();
  };
  const stop = () => {
    window.removeEventListener("keydown", onKey, true);
    setListening(false);
    setProblem("");
  };
  const listen = () => {
    if (listening()) return stop();
    setListening(true);
    window.addEventListener("keydown", onKey, true);
  };
  onCleanup(() => window.removeEventListener("keydown", onKey, true));

  return (
    <Field label={props.label} hint={props.hint}>
      <div class="hotkey-field">
        <button
          type="button"
          class="btn sm hotkey-btn"
          classList={{ on: listening() }}
          aria-label={t("{label}: {pref}. Click to change", { label: props.label, pref: props.pref() })}
          onClick={listen}
        >
          <Show when={!listening()} fallback={t("Press the new keys…")}>
            <Kbd keys={props.pref()} />
          </Show>
        </button>
        <Show when={props.pref() !== props.fallback && !listening()}>
          <button
            type="button"
            class="btn sm ghost"
            onClick={() => {
              props.pref.set(props.fallback);
              pushHotkeys();
            }}
          >
            {t("Reset")}
          </button>
        </Show>
        <Show when={problem()}>
          <span class="hotkey-problem" role="alert">
            {problem()}
          </span>
        </Show>
      </div>
    </Field>
  );
}

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
        ref={(el) => dialogA11y(el, t("Settings"), close)}
        onClick={(e) => e.stopPropagation()}
      >
        <nav class="settings-nav" aria-label={t("Settings sections")}>
          <h2>{t("Settings")}</h2>
          <For each={TABS()}>
            {(t) => (
              <button
                type="button"
                class="settings-tab"
                classList={{ on: tab() === t.id }}
                aria-current={tab() === t.id ? "page" : undefined}
                onClick={() => ed.setSettingsTab(t.id)}
              >
                <Icon name={t.icon} size={15} />
                {t.label}
              </button>
            )}
          </For>
        </nav>
        <div class="settings-body">
          <button type="button" class="ibtn settings-close" aria-label={t("Close settings")} onClick={close}>
            <Icon name="close" size={14} />
          </button>

          <Show when={tab() === "general"}>
            <h3>{t("Theme")}</h3>
            <div class="theme-grid">
              <For each={THEMES()}>
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
            <h3>{t("Language")}</h3>
            <Seg
              label={t("Language")}
              value={locale()}
              onChange={(l: Locale) => void setLocale(l)}
              options={[
                { value: "en", label: "English" },
                { value: "zh-CN", label: "简体中文" },
              ]}
            />
            <h3>{t("Interface")}</h3>
            <Field label={t("Reduce motion")} hint={t("Turn off panel and control animations")}>
              <Switch checked={prefs.reduceMotion()} label={t("Reduce motion")} onChange={(v) => prefs.reduceMotion.set(v)} />
            </Field>
            <Field label={t("Show hints")} hint={t("Lane hints and helper notes for newcomers")}>
              <Switch checked={prefs.hints()} label={t("Show hints")} onChange={(v) => prefs.hints.set(v)} />
            </Field>
            <Field label={t("Compact tool rail")}>
              <Switch checked={layout.railCompact()} label={t("Compact tool rail")} onChange={(v) => layout.railCompact.set(v)} />
            </Field>
            <Field label={t("Panel layout")}>
              <button type="button" class="btn sm" onClick={resetLayout}>
                <Icon name="reset" size={13} /> {t("Reset layout")}
              </button>
            </Field>
          </Show>

          <Show when={tab() === "recording"}>
            <h3>{t("New recordings")}</h3>
            <Field label={t("Frame rate")} hint={t("30 fps keeps files and GIFs lean; 60 fps is smoother for MP4")}>
              <Seg
                value={prefs.captureFps()}
                onChange={(v) => prefs.captureFps.set(v)}
                options={FPS_CHOICES()}
              />
            </Field>
            <Field
              label={t("Mouse pointer")}
              hint={t("Smooth hides the real pointer and draws a clean one that glides, sized in the editor. Zooms follow it either way")}
            >
              <Seg
                value={prefs.cursorMode()}
                onChange={(v) => prefs.cursorMode.set(v)}
                options={CURSOR_MODES().map((c) => ({ value: c.value, label: c.label }))}
              />
            </Field>
            <Field label={t("Audio")} hint={t("Microphone narration and computer sound, each on its own track")}>
              <AudioPicker />
            </Field>
            <Field label={t("Camera")} hint={t("Your webcam as a bubble over the recording, placed and shaped in the editor")}>
              <CameraPicker />
            </Field>
            <Field label={t("Live preview")} hint={t("Watch the take in the recording panel. Turn off to save processor time on a slower PC")}>
              <Switch
                checked={prefs.livePreview()}
                label={t("Live preview")}
                onChange={(v) => {
                  prefs.livePreview.set(v);
                  void invoke("set_live_preview", { on: v }).catch(() => undefined);
                }}
              />
            </Field>
            <Field label={t("Countdown")}>
              <Seg
                value={prefs.countdown()}
                onChange={(v) => prefs.countdown.set(v)}
                options={COUNTDOWN_CHOICES()}
              />
            </Field>
            <Field label={t("Zoom strength")} hint={t("Used by {keys} while recording and by new zoom blocks", { keys: zoomKeys() })}>
              <Seg
                value={prefs.recordZoom()}
                onChange={(v) => prefs.recordZoom.set(v)}
                options={ZOOM_CHOICES()}
              />
            </Field>
            <Field
              label={t("Auto zoom")}
              hint={t("A take where you don't press {keys} gets a few calm zooms where you clicked", { keys: zoomKeys() })}
            >
              <Switch
                checked={prefs.recordAutoZoom()}
                label={t("Auto zoom")}
                onChange={(v) => prefs.recordAutoZoom.set(v)}
              />
            </Field>
            <h3>{t("Every new take starts with")}</h3>
            <Field label={t("Click ripples")} hint={t("A ring at every click, so viewers see where you clicked")}>
              <Switch checked={prefs.newClicks()} label={t("Click ripples")} onChange={(v) => prefs.newClicks.set(v)} />
            </Field>
            <Field label={t("Keystrokes")} hint={t("Shortcuts you press appear as chips. Plain typing never shows")}>
              <Switch checked={prefs.newKeys()} label={t("Keystrokes")} onChange={(v) => prefs.newKeys.set(v)} />
            </Field>
            <Field label={t("Frame")} hint={t("A mat and backdrop around the recording, for a polished look")}>
              <Seg
                value={prefs.newFrame()}
                onChange={(v) => prefs.newFrame.set(v)}
                options={[
                  { value: "none", label: t("None") },
                  { value: "subtle", label: t("Subtle") },
                  { value: "studio", label: t("Studio") },
                ]}
              />
            </Field>
            <Field label={t("Remove mic noise")} hint={t("Filters fans, hum and hiss out of your voice. The original stays one click away")}>
              <Switch checked={prefs.newDenoise()} label={t("Remove mic noise")} onChange={(v) => prefs.newDenoise.set(v)} />
            </Field>
            <p class="note">
              {t("Each of these can be changed for a single clip in the editor. Vuoom stores frames losslessly compressed as they are captured, so a long take stays small on disk and survives a crash. The recording panel never appears in the capture.")}
            </p>
          </Show>

          <Show when={tab() === "editing"}>
            <h3>{t("Timeline")}</h3>
            <Field label={t("Magnetic snapping")} hint={t("Edges snap to the playhead, other edges and the grid. Hold Alt to bypass")}>
              <Switch checked={prefs.snapping()} label={t("Magnetic snapping")} onChange={(v) => prefs.snapping.set(v)} />
            </Field>
            <Field label={t("Annotations on one lane")}>
              <Switch checked={layout.compactNotes()} label={t("Annotations on one lane")} onChange={(v) => layout.compactNotes.set(v)} />
            </Field>
            <Field label={t("Loop playback")}>
              <Switch
                checked={prefs.loop()}
                label={t("Loop playback")}
                onChange={(v) => {
                  prefs.loop.set(v);
                  ed.setLooping(v);
                }}
              />
            </Field>
            <h3>{t("Export")}</h3>
            <Field label={t("Default format")}>
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
            <h3>{t("Save location")}</h3>
            <div class="storage-card">
              <Icon name="folder" size={20} />
              <div class="save-dir">
                <strong data-tip={saveDir() ?? ""}>{saveDir() || t("Measuring…")}</strong>
                <small>{prefs.saveDir() ? t("where exports and projects go") : t("the default folder, in Videos")}</small>
              </div>
              <button type="button" class="btn sm" onClick={() => void chooseSaveDir()}>
                {t("Change…")}
              </button>
            </div>
            <div class="save-dir-actions">
              <button type="button" class="btn sm ghost" onClick={() => void openSaveDir()}>
                <Icon name="external" size={13} /> {t("Open folder")}
              </button>
              <Show when={prefs.saveDir()}>
                <button type="button" class="btn sm ghost" onClick={() => prefs.saveDir.set(null)}>
                  <Icon name="reset" size={13} /> {t("Use default")}
                </button>
              </Show>
            </div>
            <Field label={t("Ask where to save")} hint={t("Off: every export goes straight into this folder, never overwriting an earlier one")}>
              <Switch
                checked={prefs.askWhereToSave()}
                label={t("Ask where to save")}
                onChange={(v) => prefs.askWhereToSave.set(v)}
              />
            </Field>
            <h3>{t("Crash recovery")}</h3>
            <p class="note">
              {t("Every take streams to a recovery folder while you record, so a crash or an accidental close never loses it. Vuoom keeps the last two sessions.")}
            </p>
            <div class="storage-card">
              <Icon name="drive" size={20} />
              <div>
                <strong>{ed.recoveryBytes() === null ? t("Measuring…") : fmtBytes(ed.recoveryBytes()!)}</strong>
                <small>{t("used by recovery data")}</small>
              </div>
              <button
                type="button"
                class="btn sm danger"
                disabled={ed.clearingStorage() || !ed.recoveryBytes()}
                onClick={() => void ed.clearStorage()}
              >
                {ed.clearingStorage() ? t("Clearing…") : t("Clear")}
              </button>
            </div>
            <p class="note">{t("Clearing keeps the clip that is open right now.")}</p>
          </Show>

          <Show when={tab() === "shortcuts"}>
            <h3>{t("While recording")}</h3>
            <HotkeyField
              label={t("Zoom in or out")}
              hint={t("Works while another app has the keyboard. That app never sees the keys")}
              pref={prefs.hotkeyZoom}
              fallback={ZOOM_DEFAULT}
              taken={() => [
                { keys: stopKeys(), clash: t("{keys} is already the stop hotkey", { keys: stopKeys() }) },
                { keys: RECORD_KEYS, clash: t("{keys} already starts a recording", { keys: RECORD_KEYS }) },
              ]}
            />
            <HotkeyField
              label={t("Stop recording")}
              hint={t("Stops the take and opens the editor")}
              pref={prefs.hotkeyStop}
              fallback={STOP_DEFAULT}
              taken={() => [
                { keys: zoomKeys(), clash: t("{keys} is already the zoom hotkey", { keys: zoomKeys() }) },
                { keys: RECORD_KEYS, clash: t("{keys} already starts a recording", { keys: RECORD_KEYS }) },
              ]}
            />
            <div class="shortcut-grid">
              <For each={SHORTCUTS()}>
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
              <p class="note">{t("Screen recordings that zoom where it matters. Free and open source, Apache-2.0.")}</p>
              <Show
                when={ed.update()}
                fallback={
                  <button
                    type="button"
                    class="btn sm"
                    disabled={ed.checkingUpdate()}
                    onClick={() => void ed.checkForUpdate(true)}
                  >
                    <Icon name="reset" size={13} /> {ed.checkingUpdate() ? t("Checking…") : t("Check for updates")}
                  </button>
                }
              >
                <button type="button" class="btn primary sm" disabled={ed.updating()} onClick={() => void ed.runUpdate()}>
                  <Icon name="download" size={13} /> {t("Install v{version}", { version: ed.update()!.version })}
                </button>
              </Show>
              <Show when={ed.update()?.body?.trim()}>
                {(notes) => (
                  <div class="update-notes">
                    <h4>{t("What's new in v{version}", { version: ed.update()!.version })}</h4>
                    <p>{notes()}</p>
                  </div>
                )}
              </Show>
              <Show when={ed.updateNote()}>
                <p class="note" classList={{ "update-error": !!ed.updateNote()!.error }} role="status">
                  {ed.updateNote()!.text}
                </p>
              </Show>
              <div class="about-report">
                <h4>{t("Something went wrong?")}</h4>
                <p class="note">
                  {t("Copy a report with your Vuoom and Windows versions, GPU, displays and the last lines of the log, then paste it into a bug report. Nothing is sent from here.")}
                </p>
                <div class="about-report-actions">
                  <button type="button" class="btn sm" onClick={() => void ed.copyDiagnostics()}>
                    <Icon name="copy" size={13} /> {t("Copy report")}
                  </button>
                  <button type="button" class="btn ghost sm" onClick={() => void ed.openLogsFolder()}>
                    <Icon name="folder" size={13} /> {t("Open logs folder")}
                  </button>
                </div>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
