// The window's title bar: brand + record, the File / View menus, the project name,
// history, the command palette, panel toggles, Export, and the window controls. The empty
// space is a Tauri drag region so the frameless window stays movable.
import { Show } from "solid-js";
import { useEditor } from "../editor/context";
import { Icon } from "../icons";
import { LogoMark } from "../Logo";
import { layout, resetLayout } from "../prefs";
import { IconButton, Menu, type MenuItem } from "../ui";
import WindowControls from "../WindowControls";

export default function Titlebar() {
  const ed = useEditor();

  const fileItems = (): MenuItem[] => [
    { label: "New recording", icon: "region", kbd: "Ctrl+Shift+R", onSelect: () => void ed.startRecord() },
    { label: "Record full screen", icon: "fullscreen", onSelect: () => void ed.startRecord("full") },
    { label: "Record a window", icon: "window", onSelect: () => void ed.startRecord("window") },
    { separator: true },
    { label: "Open project…", icon: "folder", kbd: "Ctrl+O", onSelect: () => void ed.onOpenProject() },
    {
      label: "Save project…",
      icon: "save",
      kbd: "Ctrl+S",
      disabled: !ed.hasClip(),
      onSelect: () => void ed.onSaveProject(),
    },
    {
      label: "Export GIF or MP4…",
      icon: "export",
      kbd: "Ctrl+E",
      disabled: !ed.hasClip(),
      onSelect: () => ed.setShowExport(true),
    },
    ...(ed.recoverable() !== null
      ? ([
          { separator: true },
          { label: "Recover last session", icon: "history", onSelect: () => void ed.onRecover() },
        ] as MenuItem[])
      : []),
    ...(ed.recents().length > 0
      ? ([
          { heading: "Recent" },
          ...ed.recents().slice(0, 5).map(
            (r): MenuItem => ({ label: r.name, icon: "film", onSelect: () => void ed.openRecent(r.dir) }),
          ),
        ] as MenuItem[])
      : []),
    { separator: true },
    { label: "Settings…", icon: "settings", kbd: "Ctrl+,", onSelect: () => ed.setSettingsTab("general") },
  ];

  const viewItems = (): MenuItem[] => [
    {
      label: "Tools",
      kbd: "Ctrl+1",
      checked: layout.railOpen(),
      onSelect: () => layout.railOpen.set(!layout.railOpen()),
    },
    {
      label: "Inspector",
      kbd: "Ctrl+2",
      checked: layout.inspectorOpen(),
      onSelect: () => layout.inspectorOpen.set(!layout.inspectorOpen()),
    },
    {
      label: "Timeline",
      kbd: "Ctrl+3",
      checked: layout.timelineOpen(),
      onSelect: () => layout.timelineOpen.set(!layout.timelineOpen()),
    },
    { separator: true },
    {
      label: "Compact tool rail",
      checked: layout.railCompact(),
      onSelect: () => layout.railCompact.set(!layout.railCompact()),
    },
    {
      label: "Annotations on one lane",
      checked: layout.compactNotes(),
      onSelect: () => layout.compactNotes.set(!layout.compactNotes()),
    },
    { separator: true },
    { label: "Reset layout", icon: "reset", kbd: "Ctrl+0", onSelect: resetLayout },
    { separator: true },
    { label: "Keyboard shortcuts", icon: "keyboard", kbd: "?", onSelect: () => ed.setSettingsTab("shortcuts") },
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
        data-tip="Start a new recording"
        data-kbd="Ctrl+Shift+R"
        onClick={() => void ed.startRecord()}
      >
        <span class="rec-dot" /> Record
      </button>

      <nav class="tb-menus" aria-label="Application menu">
        <Menu
          items={fileItems}
          trigger={(m) => (
            <button type="button" class="tb-menu" classList={{ on: m.open }} ref={m.ref} onClick={m.toggle}>
              File
            </button>
          )}
        />
        <Menu
          items={viewItems}
          trigger={(m) => (
            <button type="button" class="tb-menu" classList={{ on: m.open }} ref={m.ref} onClick={m.toggle}>
              View
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
            aria-label="Project name"
            data-tip="Rename project"
            onInput={(e) => ed.setProjectName(e.currentTarget.value)}
            onFocus={(e) => e.currentTarget.select()}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
            }}
            onBlur={(e) => {
              if (!e.currentTarget.value.trim()) ed.setProjectName("Untitled");
            }}
          />
          <Show when={ed.dirty()}>
            <span class="tb-dirty" data-tip="Unsaved changes" data-kbd="Ctrl+S" />
          </Show>
        </div>
      </Show>

      <div class="tb-drag" data-tauri-drag-region="" />

      <button
        type="button"
        class="tb-search"
        data-tip="Search every action"
        data-kbd="Ctrl+K"
        onClick={() => ed.setShowPalette(true)}
      >
        <Icon name="search" size={14} />
        <span>Search actions</span>
        <kbd>Ctrl K</kbd>
      </button>

      <div class="tb-group">
        <IconButton icon="undo" tip="Undo" kbd="Ctrl+Z" disabled={!ed.hasClip()} onClick={() => void ed.doUndo()} />
        <IconButton icon="redo" tip="Redo" kbd="Ctrl+Y" disabled={!ed.hasClip()} onClick={() => void ed.doRedo()} />
      </div>

      <Show when={ed.hasClip()}>
        <div class="tb-group">
          <IconButton
            icon="panelLeft"
            tip="Tools"
            kbd="Ctrl+1"
            active={layout.railOpen()}
            onClick={() => layout.railOpen.set(!layout.railOpen())}
          />
          <IconButton
            icon="panelBottom"
            tip="Timeline"
            kbd="Ctrl+3"
            active={layout.timelineOpen()}
            onClick={() => layout.timelineOpen.set(!layout.timelineOpen())}
          />
          <IconButton
            icon="panelRight"
            tip="Inspector"
            kbd="Ctrl+2"
            active={layout.inspectorOpen()}
            onClick={() => layout.inspectorOpen.set(!layout.inspectorOpen())}
          />
        </div>
      </Show>

      <IconButton icon="settings" tip="Settings" kbd="Ctrl+," onClick={() => ed.setSettingsTab("general")} />

      <Show when={ed.update()}>
        <button
          type="button"
          class="btn sm tb-update"
          disabled={ed.updating()}
          data-tip={`Install v${ed.update()!.version} and restart`}
          onClick={() => void ed.runUpdate()}
        >
          <Icon name="download" size={14} />
          {ed.updating() ? "Updating…" : "Update"}
        </button>
      </Show>

      <button
        type="button"
        class="btn primary sm tb-export"
        disabled={!ed.hasClip()}
        data-tip="Export a GIF or MP4"
        data-kbd="Ctrl+E"
        onClick={() => ed.setShowExport(true)}
      >
        <Icon name="export" size={14} />
        Export
      </button>

      <WindowControls />
    </header>
  );
}
