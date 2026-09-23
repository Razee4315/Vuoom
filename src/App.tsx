// Vuoom's root: creates the editor store, lays out the workspace, and mounts the dialogs.
// All state and behavior lives in editor/createEditor.ts; every piece of UI lives in
// components/ and reads the store through EditorContext.
import { createEffect, onCleanup, onMount, Show } from "solid-js";
import CommandPalette from "./components/CommandPalette";
import Home from "./components/Home";
import Inspector from "./components/Inspector";
import Settings from "./components/Settings";
import SourcePicker from "./components/SourcePicker";
import Stage from "./components/Stage";
import StatusBar from "./components/StatusBar";
import Timeline from "./components/Timeline";
import Titlebar from "./components/Titlebar";
import ToolRail from "./components/ToolRail";
import Welcome from "./components/Welcome";
import { EditorContext } from "./editor/context";
import { createEditor } from "./editor/createEditor";
import { ExportDialog } from "./ExportDialog";
import { GPU_FAILED_MSG } from "./format";
import { Icon } from "./icons";
import { layout, prefs, resetLayout } from "./prefs";
import RecordOverlay from "./RecordOverlay";
import { ToastHost, TooltipLayer } from "./ui";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/shell.css";
import "./styles/home.css";
import "./styles/stage.css";
import "./styles/inspector.css";
import "./styles/timeline.css";
import "./styles/dialogs.css";

function Workspace() {
  return (
    <div class="workspace">
      <div class="center">
        <div class="upper">
          <Show when={layout.railOpen()}>
            <ToolRail />
          </Show>
          <Stage />
        </div>
        <Timeline />
      </div>
      <Show when={layout.inspectorOpen()}>
        <Inspector />
      </Show>
    </div>
  );
}

export default function App() {
  const ed = createEditor();

  createEffect(() => {
    document.documentElement.dataset.motion = prefs.reduceMotion() ? "reduced" : "";
  });

  // Workspace-level shortcuts: the palette, settings, and panel toggles.
  const onKey = (e: KeyboardEvent) => {
    if (!e.ctrlKey || e.altKey) return;
    if (e.code === "KeyK" && !e.shiftKey) {
      e.preventDefault();
      ed.setShowPalette(!ed.showPalette());
      return;
    }
    if (e.code === "Comma" && !e.shiftKey) {
      e.preventDefault();
      ed.setSettingsTab(ed.settingsTab() ? null : "general");
      return;
    }
    if (ed.isModal() || e.shiftKey) return;
    const toggles: Record<string, () => void> = {
      Digit1: () => layout.railOpen.set(!layout.railOpen()),
      Digit2: () => layout.inspectorOpen.set(!layout.inspectorOpen()),
      Digit3: () => layout.timelineOpen.set(!layout.timelineOpen()),
      Digit0: resetLayout,
    };
    const fn = toggles[e.code];
    if (fn) {
      e.preventDefault();
      fn();
    }
  };
  onMount(() => window.addEventListener("keydown", onKey, true));
  onCleanup(() => window.removeEventListener("keydown", onKey, true));

  return (
    <EditorContext.Provider value={ed}>
      <div class="app">
        <Titlebar />
        <Show when={ed.gpuLost()}>
          <div class="banner" role="alert">
            <Icon name="warning" size={14} />
            <span>{GPU_FAILED_MSG}</span>
            <button type="button" class="ibtn sm" aria-label="Dismiss" onClick={() => ed.setGpuLost(false)}>
              <Icon name="close" size={12} />
            </button>
          </div>
        </Show>
        <Show when={ed.hasClip()} fallback={<Home />}>
          <Workspace />
        </Show>
        <StatusBar />
      </div>

      <Show when={ed.showExport()}>
        <ExportDialog
          name={ed.projectName()}
          duration={ed.duration()}
          aspect={ed.frameAspect()}
          trim={ed.trim()}
          speed={ed.speed()}
          cuts={ed.cuts()}
          onClose={() => ed.setShowExport(false)}
          onStatus={ed.setStatus}
          onExported={() => ed.setDirty(false)}
        />
      </Show>

      <Show when={ed.recordPhase() === "active"}>
        <RecordOverlay
          backdrop={ed.backdrop()}
          zoom={ed.zoomAmount()}
          target={ed.recordTarget()}
          initialMode={ed.recordMode() === "full" ? "full" : "region"}
          onZoomChange={ed.setZoomAmount}
          onFinished={(s) => void ed.onRecordFinished(s)}
          onCancel={ed.onRecordCancel}
          onFailed={ed.onRecordFailed}
        />
      </Show>

      <Show when={ed.showSource()}>
        <SourcePicker />
      </Show>
      <Show when={ed.settingsTab() !== null}>
        <Settings />
      </Show>
      <Show when={ed.showPalette()}>
        <CommandPalette />
      </Show>
      <Welcome />
      <TooltipLayer />
      <ToastHost />
    </EditorContext.Provider>
  );
}
