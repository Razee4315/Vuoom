// The window's title bar: brand + record, the File / View menus, the project name,
// history, the command palette, panel toggles, Export, and the window controls. The empty
// space is a Tauri drag region so the frameless window stays movable.
import { Show } from "solid-js";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon } from "../icons";
import { LogoMark } from "../Logo";
import { layout, resetLayout } from "../prefs";
import { IconButton, Menu, type MenuItem } from "../ui";
import WindowControls from "../WindowControls";

export default function Titlebar() {
  const ed = useEditor();

  const fileItems = (): MenuItem[] => [
    { label: t("New recording"), icon: "region", kbd: "Ctrl+Shift+R", onSelect: () => void ed.startRecord() },
    { label: t("Record full screen"), icon: "fullscreen", onSelect: () => void ed.startRecord("full") },
    { label: t("Record a window"), icon: "window", onSelect: () => void ed.startRecord("window") },
    {
      label: t("Add a take to this clip…"),
      icon: "plus",
      disabled: !ed.hasClip(),
      onSelect: () => void ed.addTake(),
    },
    { label: t("Try a sample clip"), icon: "wand", onSelect: () => void ed.trySample() },
    { separator: true },
    { label: t("Open project…"), icon: "folder", kbd: "Ctrl+O", onSelect: () => void ed.onOpenProject() },
    {
      label: ed.projectDir() ? t("Save project") : t("Save project…"),
      icon: "save",
      kbd: "Ctrl+S",
      disabled: !ed.hasClip(),
      onSelect: () => void ed.onSaveProject(),
    },
    {
      label: t("Save project as…"),
      icon: "save",
      kbd: "Ctrl+Shift+S",
      disabled: !ed.hasClip(),
      onSelect: () => void ed.onSaveProjectAs(),
    },
    {
      label: t("Export GIF or MP4…"),
      icon: "export",
      kbd: "Ctrl+E",
      disabled: !ed.hasClip(),
      onSelect: () => ed.setShowExport(true),
    },
    ...(ed.recoverable() !== null
      ? ([
          { separator: true },
          { label: t("Recover last session"), icon: "history", onSelect: () => void ed.onRecover() },
        ] as MenuItem[])
      : []),
    ...(ed.recents().length > 0
      ? ([
          { heading: t("Recent") },
          ...ed.recents().slice(0, 5).map(
            (r): MenuItem => ({ label: r.name, icon: "film", onSelect: () => void ed.openRecent(r.dir) }),
          ),
        ] as MenuItem[])
      : []),
    { separator: true },
    { label: t("Settings…"), icon: "settings", kbd: "Ctrl+,", onSelect: () => ed.setSettingsTab("general") },
  ];

  const viewItems = (): MenuItem[] => [
    {
      label: t("Tools"),
      kbd: "Ctrl+1",
      checked: layout.railOpen(),
      onSelect: () => layout.railOpen.set(!layout.railOpen()),
    },
    {
      label: t("Inspector"),
      kbd: "Ctrl+2",
      checked: layout.inspectorOpen(),
      onSelect: () => layout.inspectorOpen.set(!layout.inspectorOpen()),
    },
    {
      label: t("Timeline"),
      kbd: "Ctrl+3",
      checked: layout.timelineOpen(),
      onSelect: () => layout.timelineOpen.set(!layout.timelineOpen()),
    },
    { separator: true },
    {
      label: t("Compact tool rail"),
      checked: layout.railCompact(),
      onSelect: () => layout.railCompact.set(!layout.railCompact()),
    },
    {
      label: t("Composition guides"),
      kbd: "G",
      checked: layout.guides(),
      onSelect: () => layout.guides.set(!layout.guides()),
    },
    {
      label: t("Frame thumbnails"),
      checked: layout.filmstrip(),
      onSelect: () => layout.filmstrip.set(!layout.filmstrip()),
    },
    {
      label: t("Annotations on one lane"),
      checked: layout.compactNotes(),
      onSelect: () => layout.compactNotes.set(!layout.compactNotes()),
    },
    { separator: true },
    { label: t("Reset layout"), icon: "reset", kbd: "Ctrl+0", onSelect: resetLayout },
    { separator: true },
    { label: t("Keyboard shortcuts"), icon: "keyboard", kbd: "?", onSelect: () => ed.setSettingsTab("shortcuts") },
  ];

  return (
    <header class="titlebar" data-tauri-drag-region="">
      <div class="tb-brand" data-tauri-drag-region="">
        <LogoMark size={18} />
      </div>
      <button
        type="button"
        class="btn record sm tb-record"
        ref={ed.refs.recordBtn}
        data-tip={t("Start a new recording")}
        data-kbd="Ctrl+Shift+R"
        onClick={() => void ed.startRecord()}
      >
        <span class="rec-dot" /> {t("Record")}
      </button>

      <nav class="tb-menus" aria-label={t("Application menu")}>
        <Menu
          items={fileItems}
          trigger={(m) => (
            <button type="button" class="tb-menu" classList={{ on: m.open }} ref={m.ref} onClick={m.toggle}>
              {t("File")}
            </button>
          )}
        />
        <Menu
          items={viewItems}
          trigger={(m) => (
            <button type="button" class="tb-menu" classList={{ on: m.open }} ref={m.ref} onClick={m.toggle}>
              {t("View")}
            </button>
          )}
        />
      </nav>

      <Show when={ed.hasClip()}>
        <div class="tb-project">
          <input
            class="tb-name"
            value={ed.projectName()}
            spellcheck={false}
            aria-label={t("Project name")}
            data-tip={t("Rename project")}
            onInput={(e) => ed.setProjectName(e.currentTarget.value)}
            onFocus={(e) => e.currentTarget.select()}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
            }}
            onBlur={(e) => {
              if (!e.currentTarget.value.trim()) ed.setProjectName(t("Untitled"));
            }}
          />
          <Show when={ed.dirty()}>
            <span class="tb-dirty" data-tip={t("Unsaved changes")} data-kbd="Ctrl+S" />
          </Show>
        </div>
      </Show>

      <div class="tb-drag" data-tauri-drag-region="" />

      <button
        type="button"
        class="tb-search"
        data-tip={t("Search every action")}
        data-kbd="Ctrl+K"
        onClick={() => ed.setShowPalette(true)}
      >
        <Icon name="search" size={14} />
        <span>{t("Search actions")}</span>
        <kbd>Ctrl K</kbd>
      </button>

      <div class="tb-group">
        <IconButton icon="undo" tip={t("Undo")} kbd="Ctrl+Z" disabled={!ed.hasClip()} onClick={() => void ed.doUndo()} />
        <IconButton icon="redo" tip={t("Redo")} kbd="Ctrl+Y" disabled={!ed.hasClip()} onClick={() => void ed.doRedo()} />
      </div>

      <Show when={ed.hasClip()}>
        <div class="tb-group">
          <IconButton
            icon="panelLeft"
            tip={t("Tools")}
            kbd="Ctrl+1"
            active={layout.railOpen()}
            onClick={() => layout.railOpen.set(!layout.railOpen())}
          />
          <IconButton
            icon="panelBottom"
            tip={t("Timeline")}
            kbd="Ctrl+3"
            active={layout.timelineOpen()}
            onClick={() => layout.timelineOpen.set(!layout.timelineOpen())}
          />
          <IconButton
            icon="panelRight"
            tip={t("Inspector")}
            kbd="Ctrl+2"
            active={layout.inspectorOpen()}
            onClick={() => layout.inspectorOpen.set(!layout.inspectorOpen())}
          />
        </div>
      </Show>

      <IconButton icon="settings" tip={t("Settings")} kbd="Ctrl+," onClick={() => ed.setSettingsTab("general")} />

      <Show when={ed.update()}>
        <button
          type="button"
          class="btn sm tb-update"
          disabled={ed.updating()}
          data-tip={t("Install v{version} and restart", { version: ed.update()!.version })}
          onClick={() => void ed.runUpdate()}
        >
          <Icon name="download" size={14} />
          {ed.updating() ? t("Updating…") : t("Update")}
        </button>
      </Show>

      <button
        type="button"
        class="btn primary sm tb-export"
        disabled={!ed.hasClip()}
        data-tip={t("Export a GIF or MP4")}
        data-kbd="Ctrl+E"
        onClick={() => ed.setShowExport(true)}
      >
        <Icon name="export" size={14} />
        {t("Export")}
      </button>

      <WindowControls />
    </header>
  );
}
