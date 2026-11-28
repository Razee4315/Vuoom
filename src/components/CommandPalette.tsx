// Ctrl+K: every action in the app, searchable by name, runnable from the keyboard.
import { createEffect, createMemo, createSignal, For, onMount, Show } from "solid-js";
import { dialogA11y } from "../dialog";
import { useEditor } from "../editor/context";
import { t } from "../i18n";
import { Icon, type IconName } from "../icons";
import { layout, prefs, resetLayout } from "../prefs";
import { FPS_CHOICES } from "../recordOptions";
import { TOOLS } from "../shortcuts";
import { applyTheme, THEMES } from "../themes";
import type { Tool } from "../types";
import { Kbd } from "../ui";

interface Command {
  id: string;
  label: string;
  group: string;
  icon: IconName;
  kbd?: string;
  run: () => void;
  when?: () => boolean;
}

const TOOL_ICON: Record<Tool, IconName> = {
  select: "cursor",
  zoom: "zoomIn",
  text: "text",
  arrow: "arrow",
  step: "step",
  pen: "pen",
  shape: "shape",
  highlight: "highlight",
  spotlight: "spotlight",
  mask: "eyeOff",
};

/** Subsequence match; lower score is better, -1 means no match. */
function score(text: string, q: string): number {
  if (!q) return 0;
  const lower = text.toLowerCase();
  const direct = lower.indexOf(q);
  if (direct >= 0) return direct;
  let ti = 0;
  let gaps = 0;
  for (const ch of q) {
    const at = lower.indexOf(ch, ti);
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
  const [activeId, setActiveId] = createSignal<string | null>(null);
  const close = () => ed.setShowPalette(false);
  const clip = () => ed.hasClip();

  const all: Command[] = [
    { id: "rec", label: t("New recording"), group: t("Record"), icon: "region", kbd: "Ctrl+Shift+R", run: () => void ed.startRecord() },
    { id: "rec-full", label: t("Record full screen"), group: t("Record"), icon: "fullscreen", run: () => void ed.startRecord("full") },
    { id: "rec-region", label: t("Record a region"), group: t("Record"), icon: "region", run: () => void ed.startRecord("region") },
    { id: "rec-win", label: t("Record a window"), group: t("Record"), icon: "window", run: () => void ed.startRecord("window") },
    { id: "sample", label: t("Try a sample clip"), group: t("Record"), icon: "wand", run: () => void ed.trySample() },
    { id: "rec-add", label: t("Add a take to this clip"), group: t("Record"), icon: "plus", run: () => void ed.addTake(), when: clip },
    ...FPS_CHOICES().map((c) => ({
      id: `fps${c.value}`,
      label: t("Record at {label}", { label: c.label }),
      group: t("Record"),
      icon: "monitor" as const,
      run: () => prefs.captureFps.set(c.value),
    })),
    { id: "open", label: t("Open project…"), group: t("Project"), icon: "folder", kbd: "Ctrl+O", run: () => void ed.onOpenProject() },
    { id: "save", label: t("Save project"), group: t("Project"), icon: "save", kbd: "Ctrl+S", run: () => void ed.onSaveProject(), when: clip },
    { id: "save-as", label: t("Save project as…"), group: t("Project"), icon: "save", kbd: "Ctrl+Shift+S", run: () => void ed.onSaveProjectAs(), when: clip },
    { id: "export", label: t("Export GIF, WebP or MP4…"), group: t("Project"), icon: "export", kbd: "Ctrl+E", run: () => ed.setShowExport(true), when: clip },
    { id: "recover", label: t("Recover last session"), group: t("Project"), icon: "history", run: () => void ed.onRecover(), when: () => ed.recoverable() !== null },
    { id: "play", label: t("Play / pause"), group: t("Playback"), icon: "play", kbd: "Space", run: ed.togglePlay, when: clip },
    { id: "start", label: t("Go to start"), group: t("Playback"), icon: "toStart", kbd: "Home", run: ed.restart, when: clip },
    {
      id: "loop",
      label: t("Toggle loop"),
      group: t("Playback"),
      icon: "loop",
      run: () => {
        ed.setLooping(!ed.looping());
        prefs.loop.set(ed.looping());
      },
      when: clip,
    },
    { id: "zoom", label: t("Add zoom at playhead"), group: t("Edit"), icon: "zoomIn", run: () => void ed.addZoomAt(), when: clip },
    { id: "speed", label: t("Add speed-up at playhead"), group: t("Edit"), icon: "speed", kbd: "X", run: () => void ed.addSpeedAtPlayhead(), when: clip },
    { id: "cut", label: t("Cut at playhead"), group: t("Edit"), icon: "cut", kbd: "C", run: () => void ed.addCutAtPlayhead(), when: clip },
    { id: "auto", label: t("Auto zooms from clicks"), group: t("Edit"), icon: "sparkle", run: () => void ed.planZoomAuto(), when: clip },
    { id: "skim", label: t("Toggle skim idle"), group: t("Edit"), icon: "speed", run: () => ed.toggleSkim(), when: clip },
    { id: "captions", label: t("Make captions from the narration"), group: t("Captions"), icon: "captions", run: () => void ed.captions.generate(), when: () => clip() && ed.audio.hasAudio() },
    { id: "caption-add", label: t("Add caption at playhead"), group: t("Captions"), icon: "plus", run: () => void ed.addCaptionAtPlayhead(), when: clip },
    { id: "caption-srt", label: t("Save captions as .srt…"), group: t("Captions"), icon: "download", run: () => void ed.captions.saveSrt(), when: () => ed.captions.list().length > 0 },
    { id: "clicks", label: t("Toggle click ripples"), group: t("Edit"), icon: "clicks", run: () => ed.toggleClicks(), when: clip },
    { id: "keys", label: t("Toggle keystroke overlay"), group: t("Edit"), icon: "keyboard", run: () => ed.toggleKeys(), when: clip },
    { id: "undo", label: t("Undo"), group: t("Edit"), icon: "undo", kbd: "Ctrl+Z", run: () => void ed.doUndo(), when: clip },
    { id: "redo", label: t("Redo"), group: t("Edit"), icon: "redo", kbd: "Ctrl+Y", run: () => void ed.doRedo(), when: clip },
    ...TOOLS().map(
      (tool): Command => ({
        id: `tool-${tool.id}`,
        label: t("{label} tool", { label: tool.label }),
        group: t("Tools"),
        icon: TOOL_ICON[tool.id] ?? "cursor",
        kbd: tool.key,
        run: () => ed.setTool(tool.id),
        when: clip,
      }),
    ),
    { id: "frame-none", label: t("Frame: none"), group: t("Clip"), icon: "frame", run: () => ed.applyFramePreset("none"), when: clip },
    { id: "frame-subtle", label: t("Frame: subtle"), group: t("Clip"), icon: "frame", run: () => ed.applyFramePreset("subtle"), when: clip },
    { id: "frame-studio", label: t("Frame: studio"), group: t("Clip"), icon: "frame", run: () => ed.applyFramePreset("studio"), when: clip },
    { id: "crop-full", label: t("Crop: full frame"), group: t("Clip"), icon: "crop", run: () => void ed.applyCrop(null), when: clip },
    { id: "crop-169", label: t("Crop: 16:9"), group: t("Clip"), icon: "crop", run: () => void ed.applyCrop(ed.centeredCrop(16 / 9)), when: clip },
    { id: "crop-11", label: t("Crop: 1:1"), group: t("Clip"), icon: "crop", run: () => void ed.applyCrop(ed.centeredCrop(1)), when: clip },
    { id: "crop-916", label: t("Crop: 9:16"), group: t("Clip"), icon: "crop", run: () => void ed.applyCrop(ed.centeredCrop(9 / 16)), when: clip },
    { id: "v-rail", label: t("Toggle tools panel"), group: t("View"), icon: "panelLeft", kbd: "Ctrl+1", run: () => layout.railOpen.set(!layout.railOpen()) },
    { id: "v-insp", label: t("Toggle inspector"), group: t("View"), icon: "panelRight", kbd: "Ctrl+2", run: () => layout.inspectorOpen.set(!layout.inspectorOpen()) },
    { id: "v-tl", label: t("Toggle timeline"), group: t("View"), icon: "panelBottom", kbd: "Ctrl+3", run: () => layout.timelineOpen.set(!layout.timelineOpen()) },
    { id: "v-reset", label: t("Reset layout"), group: t("View"), icon: "reset", kbd: "Ctrl+0", run: resetLayout },
    { id: "v-guides", label: t("Toggle composition guides"), group: t("View"), icon: "grid", kbd: "G", run: () => layout.guides.set(!layout.guides()) },
    ...[0.5, 1, 2].map(
      (r): Command => ({
        id: `rate-${r}`,
        label: r === 1 ? t("Preview at normal speed") : t("Preview at {rate}×", { rate: r }),
        group: t("Playback"),
        icon: "play",
        run: () => prefs.previewRate.set(r),
      }),
    ),
    { id: "v-snap", label: t("Toggle timeline snapping"), group: t("View"), icon: "magnet", run: () => prefs.snapping.set(!prefs.snapping()) },
    ...THEMES().map(
      (theme): Command => ({
        id: `theme-${theme.id}`,
        label: t("Theme: {name}", { name: theme.name }),
        group: t("View"),
        icon: "palette",
        run: () => {
          applyTheme(theme.id);
          ed.setTheme(theme.id);
        },
      }),
    ),
    { id: "settings", label: t("Settings"), group: t("App"), icon: "settings", kbd: "Ctrl+,", run: () => ed.setSettingsTab("general") },
    { id: "shortcuts", label: t("Keyboard shortcuts"), group: t("App"), icon: "keyboard", kbd: "?", run: () => ed.setSettingsTab("shortcuts") },
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

  // Reconcile active option whenever filtered results change; clear on zero matches.
  createEffect(() => {
    const list = results();
    if (list.length === 0) {
      setActiveId(null);
      return;
    }
    const cur = activeId();
    if (!cur || !list.some((c) => c.id === cur)) {
      setActiveId(list[0].id);
    }
  });

  const activeIndex = () => {
    const cur = activeId();
    if (!cur) return -1;
    return results().findIndex((c) => c.id === cur);
  };

  const run = (c: Command | undefined) => {
    if (!c) return;
    close();
    queueMicrotask(c.run);
  };

  let listEl: HTMLDivElement | undefined;
  let lastPointerX = -1;
  let lastPointerY = -1;
  const scrollActiveIntoView = () =>
    queueMicrotask(() => listEl?.querySelector(".pal-item.on")?.scrollIntoView({ block: "nearest" }));
  const move = (d: number) => {
    const list = results();
    if (!list.length) return;
    const cur = Math.max(0, activeIndex());
    const next = list[(cur + d + list.length) % list.length];
    setActiveId(next.id);
    scrollActiveIntoView();
  };

  let inputEl: HTMLInputElement | undefined;
  onMount(() => queueMicrotask(() => inputEl?.focus()));

  return (
    <div class="modal-backdrop palette-backdrop" onClick={close}>
      <div
        class="palette"
        ref={(el) => dialogA11y(el, t("Command palette"), close, { initialFocus: () => inputEl ?? null })}
        onClick={(e) => e.stopPropagation()}
      >
        <div class="pal-search">
          <Icon name="search" size={16} />
          <input
            ref={inputEl}
            role="combobox"
            aria-autocomplete="list"
            aria-expanded="true"
            aria-controls="pal-listbox"
            aria-activedescendant={activeId() ? `pal-opt-${activeId()}` : undefined}
            placeholder={t("Type a command…")}
            aria-label={t("Search commands")}
            value={q()}
            onInput={(e) => setQ(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                move(1);
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                move(-1);
              } else if (e.key === "Home") {
                const first = results()[0];
                if (first) {
                  e.preventDefault();
                  setActiveId(first.id);
                  scrollActiveIntoView();
                }
              } else if (e.key === "End") {
                const last = results()[results().length - 1];
                if (last) {
                  e.preventDefault();
                  setActiveId(last.id);
                  scrollActiveIntoView();
                }
              } else if (e.key === "Enter") {
                const idx = activeIndex();
                if (idx >= 0) {
                  e.preventDefault();
                  run(results()[idx]);
                }
              }
            }}
          />
          <Kbd keys="Esc" />
        </div>
        <div id="pal-listbox" class="pal-list" ref={listEl} role="listbox" aria-label={t("Commands")}>
          <For each={results()} fallback={<div class="pal-empty">{t("No matching command")}</div>}>
            {(c) => (
              <button
                id={`pal-opt-${c.id}`}
                type="button"
                role="option"
                tabindex={-1}
                class="pal-item"
                classList={{ on: c.id === activeId() }}
                aria-selected={c.id === activeId()}
                onPointerMove={(e) => {
                  if (e.clientX === lastPointerX && e.clientY === lastPointerY) return;
                  lastPointerX = e.clientX;
                  lastPointerY = e.clientY;
                  setActiveId(c.id);
                }}
                onClick={() => run(c)}
              >
                <Icon name={c.icon} size={15} />
                <span class="pal-label">{c.label}</span>
                <span class="pal-group">{c.group}</span>
                <Show when={c.kbd}>{(k) => <Kbd keys={k()} />}</Show>
              </button>
            )}
          </For>
        </div>
        <footer class="pal-foot" aria-hidden="true">
          <span>
            <kbd>↑</kbd> <kbd>↓</kbd> {t("Navigate")}
          </span>
          <span>
            <kbd>Enter</kbd> {t("Run")}
          </span>
          <span>
            <kbd>Esc</kbd> {t("Close")}
          </span>
        </footer>
      </div>
    </div>
  );
}
