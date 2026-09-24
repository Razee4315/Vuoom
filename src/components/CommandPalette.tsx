// Ctrl+K: every action in the app, searchable by name, runnable from the keyboard.
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { Icon, type IconName } from "../icons";
import { layout, prefs, resetLayout } from "../prefs";
import { FPS_CHOICES } from "../recordOptions";
import { TOOLS } from "../shortcuts";
import { applyTheme, THEMES } from "../themes";

interface Command {
  id: string;
  label: string;
  group: string;
  icon: IconName;
  kbd?: string;
  run: () => void;
  when?: () => boolean;
}

/** Subsequence match; lower score is better, -1 means no match. */
function score(text: string, q: string): number {
  if (!q) return 0;
  const t = text.toLowerCase();
  const direct = t.indexOf(q);
  if (direct >= 0) return direct;
  let ti = 0;
  let gaps = 0;
  for (const ch of q) {
    const at = t.indexOf(ch, ti);
    if (at < 0) return -1;
    gaps += at - ti;
    ti = at + 1;
  }
  // Scattered letters across the whole label are noise, not a match.
  return gaps <= q.length * 2 ? 100 + gaps : -1;
}

export default function CommandPalette() {
  const ed = useEditor();
  const [q, setQ] = createSignal("");
  const [active, setActive] = createSignal(0);
  const close = () => ed.setShowPalette(false);
  const clip = () => ed.hasClip();

  const all: Command[] = [
    { id: "rec", label: "New recording", group: "Record", icon: "region", kbd: "Ctrl+Shift+R", run: () => void ed.startRecord() },
    { id: "rec-full", label: "Record full screen", group: "Record", icon: "fullscreen", run: () => void ed.startRecord("full") },
    { id: "rec-region", label: "Record a region", group: "Record", icon: "region", run: () => void ed.startRecord("region") },
    { id: "rec-win", label: "Record a window", group: "Record", icon: "window", run: () => void ed.startRecord("window") },
    ...FPS_CHOICES.map((c) => ({
      id: `fps${c.value}`,
      label: `Record at ${c.label}`,
      group: "Record",
      icon: "monitor" as const,
      run: () => prefs.captureFps.set(c.value),
    })),
    { id: "open", label: "Open project…", group: "Project", icon: "folder", kbd: "Ctrl+O", run: () => void ed.onOpenProject() },
    { id: "save", label: "Save project…", group: "Project", icon: "save", kbd: "Ctrl+S", run: () => void ed.onSaveProject(), when: clip },
    { id: "export", label: "Export GIF or MP4…", group: "Project", icon: "export", kbd: "Ctrl+E", run: () => ed.setShowExport(true), when: clip },
    { id: "recover", label: "Recover last session", group: "Project", icon: "history", run: () => void ed.onRecover(), when: () => ed.recoverable() !== null },
    { id: "play", label: "Play / pause", group: "Playback", icon: "play", kbd: "Space", run: ed.togglePlay, when: clip },
    { id: "start", label: "Go to start", group: "Playback", icon: "toStart", kbd: "Home", run: ed.restart, when: clip },
    { id: "loop", label: "Toggle loop", group: "Playback", icon: "loop", run: () => ed.setLooping(!ed.looping()), when: clip },
    { id: "zoom", label: "Add zoom at playhead", group: "Edit", icon: "zoomIn", kbd: "Z", run: () => void ed.addZoomAt(), when: clip },
    { id: "speed", label: "Add speed-up at playhead", group: "Edit", icon: "speed", kbd: "X", run: () => void ed.addSpeedAtPlayhead(), when: clip },
    { id: "cut", label: "Cut at playhead", group: "Edit", icon: "cut", kbd: "C", run: () => void ed.addCutAtPlayhead(), when: clip },
    { id: "auto", label: "Auto zooms from clicks", group: "Edit", icon: "sparkle", run: () => void ed.planZoomAuto(), when: clip },
    { id: "skim", label: "Toggle skim idle", group: "Edit", icon: "speed", run: () => ed.toggleSkim(), when: clip },
    { id: "clicks", label: "Toggle click ripples", group: "Edit", icon: "clicks", run: () => ed.toggleClicks(), when: clip },
    { id: "keys", label: "Toggle keystroke overlay", group: "Edit", icon: "keys", run: () => ed.toggleKeys(), when: clip },
    { id: "undo", label: "Undo", group: "Edit", icon: "undo", kbd: "Ctrl+Z", run: () => void ed.doUndo(), when: clip },
    { id: "redo", label: "Redo", group: "Edit", icon: "redo", kbd: "Ctrl+Y", run: () => void ed.doRedo(), when: clip },
    ...TOOLS.map(
      (t): Command => ({
        id: `tool-${t.id}`,
        label: `${t.label} tool`,
        group: "Tools",
        icon: "cursor",
        kbd: t.key,
        run: () => ed.setTool(t.id),
        when: clip,
      }),
    ),
    { id: "frame-none", label: "Frame: none", group: "Clip", icon: "frame", run: () => ed.applyFramePreset("none"), when: clip },
    { id: "frame-subtle", label: "Frame: subtle", group: "Clip", icon: "frame", run: () => ed.applyFramePreset("subtle"), when: clip },
    { id: "frame-studio", label: "Frame: studio", group: "Clip", icon: "frame", run: () => ed.applyFramePreset("studio"), when: clip },
    { id: "crop-full", label: "Crop: full frame", group: "Clip", icon: "crop", run: () => void ed.applyCrop(null), when: clip },
    { id: "crop-169", label: "Crop: 16:9", group: "Clip", icon: "crop", run: () => void ed.applyCrop(ed.centeredCrop(16 / 9)), when: clip },
    { id: "crop-11", label: "Crop: 1:1", group: "Clip", icon: "crop", run: () => void ed.applyCrop(ed.centeredCrop(1)), when: clip },
    { id: "crop-916", label: "Crop: 9:16", group: "Clip", icon: "crop", run: () => void ed.applyCrop(ed.centeredCrop(9 / 16)), when: clip },
    { id: "v-rail", label: "Toggle tools panel", group: "View", icon: "panelLeft", kbd: "Ctrl+1", run: () => layout.railOpen.set(!layout.railOpen()) },
    { id: "v-insp", label: "Toggle inspector", group: "View", icon: "panelRight", kbd: "Ctrl+2", run: () => layout.inspectorOpen.set(!layout.inspectorOpen()) },
    { id: "v-tl", label: "Toggle timeline", group: "View", icon: "panelBottom", kbd: "Ctrl+3", run: () => layout.timelineOpen.set(!layout.timelineOpen()) },
    { id: "v-reset", label: "Reset layout", group: "View", icon: "reset", kbd: "Ctrl+0", run: resetLayout },
    { id: "v-guides", label: "Toggle composition guides", group: "View", icon: "grid", kbd: "G", run: () => layout.guides.set(!layout.guides()) },
    ...[0.5, 1, 2].map(
      (r): Command => ({
        id: `rate-${r}`,
        label: `Preview at ${r === 1 ? "normal speed" : `${r}×`}`,
        group: "Playback",
        icon: "play",
        run: () => prefs.previewRate.set(r),
      }),
    ),
    { id: "v-snap", label: "Toggle timeline snapping", group: "View", icon: "magnet", run: () => prefs.snapping.set(!prefs.snapping()) },
    ...THEMES.map(
      (t): Command => ({
        id: `theme-${t.id}`,
        label: `Theme: ${t.name}`,
        group: "View",
        icon: "palette",
        run: () => {
          applyTheme(t.id);
          ed.setTheme(t.id);
        },
      }),
    ),
    { id: "settings", label: "Settings", group: "App", icon: "settings", kbd: "Ctrl+,", run: () => ed.setSettingsTab("general") },
    { id: "shortcuts", label: "Keyboard shortcuts", group: "App", icon: "keyboard", kbd: "?", run: () => ed.setSettingsTab("shortcuts") },
  ];

  const results = createMemo(() => {
    const query = q().trim().toLowerCase();
    return all
      .filter((c) => !c.when || c.when())
      .map((c) => ({ c, s: Math.min(...[score(c.label, query), score(`${c.group} ${c.label}`, query)].filter((x) => x >= 0), Infinity) }))
      .filter((r) => r.s !== Infinity)
      .sort((a, b) => a.s - b.s)
      .map((r) => r.c);
  });

  const run = (c: Command | undefined) => {
    if (!c) return;
    close();
    queueMicrotask(c.run);
  };

  let listEl: HTMLDivElement | undefined;
  const move = (d: number) => {
    const n = results().length;
    if (!n) return;
    setActive((a) => (a + d + n) % n);
    queueMicrotask(() => listEl?.querySelector(".pal-item.on")?.scrollIntoView({ block: "nearest" }));
  };

  let inputEl: HTMLInputElement | undefined;
  onMount(() => queueMicrotask(() => inputEl?.focus()));

  return (
    <div class="modal-backdrop palette-backdrop" onClick={close}>
      <div class="palette" ref={(el) => dialogA11y(el, "Command palette", close)} onClick={(e) => e.stopPropagation()}>
        <div class="pal-search">
          <Icon name="search" size={16} />
          <input
            ref={inputEl}
            placeholder="Type a command…"
            aria-label="Search commands"
            value={q()}
            onInput={(e) => {
              setQ(e.currentTarget.value);
              setActive(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                move(1);
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                move(-1);
              } else if (e.key === "Enter") {
                e.preventDefault();
                run(results()[active()]);
              }
            }}
          />
          <kbd>Esc</kbd>
        </div>
        <div class="pal-list" ref={listEl} role="listbox">
          <For each={results()} fallback={<div class="pal-empty">No matching command</div>}>
            {(c, i) => (
              <button
                type="button"
                role="option"
                class="pal-item"
                classList={{ on: i() === active() }}
                aria-selected={i() === active()}
                onPointerMove={() => setActive(i())}
                onClick={() => run(c)}
              >
                <Icon name={c.icon} size={15} />
                <span class="pal-label">{c.label}</span>
                <span class="pal-group">{c.group}</span>
                <Show when={c.kbd}>
                  <span class="pal-kbd">{c.kbd}</span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
