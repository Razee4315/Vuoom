import { createSignal, onMount, onCleanup, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import WindowControls from "./WindowControls";
import ThemeMenu from "./ThemeMenu";
import { applyTheme, initialTheme } from "./themes";
import { PreviewClient } from "./preview";
import "./App.css";

type Tool = "select" | "text" | "arrow" | "box" | "crop";

/** Mirrors vuoom_encode::GifSettings (serde). */
interface GifSettings {
  fps: number;
  width: number | null;
  quality: number;
  lossy: number | null;
}

/** Mirrors src-tauri session::RecordingSummary. */
interface RecordingSummary {
  duration: number;
  frames: number;
  zooms: number;
}

const TOOLS: { id: Tool; label: string }[] = [
  { id: "select", label: "Select" },
  { id: "text", label: "Text" },
  { id: "arrow", label: "Arrow" },
  { id: "box", label: "Box" },
  { id: "crop", label: "Crop" },
];

function App() {
  const [tool, setTool] = createSignal<Tool>("select");
  const [status, setStatus] = createSignal("Ready");
  const [presets, setPresets] = createSignal<GifSettings[]>([]);
  const [theme, setTheme] = createSignal(initialTheme());
  const [recording, setRecording] = createSignal(false);
  const [hasClip, setHasClip] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [playhead, setPlayhead] = createSignal(0);

  const preview = new PreviewClient();
  let canvasEl: HTMLCanvasElement | undefined;

  const onContextMenu = (e: MouseEvent) => {
    const el = e.target as HTMLElement;
    if (!el.closest("input, textarea, [contenteditable=true]")) e.preventDefault();
  };

  onMount(async () => {
    applyTheme(theme());
    document.addEventListener("contextmenu", onContextMenu);
    if (canvasEl) preview.attach(canvasEl);
    try {
      const [readme, hq] = await invoke<[GifSettings, GifSettings]>("gif_presets");
      setPresets([readme, hq]);
      const port = await invoke<number>("preview_port");
      preview.connect(port);
      setStatus("Engine connected");
    } catch (e) {
      setStatus(`Backend error: ${String(e)}`);
    }
  });
  onCleanup(() => {
    document.removeEventListener("contextmenu", onContextMenu);
    preview.disconnect();
  });

  const scrub = async (t: number) => {
    setPlayhead(t);
    try {
      await invoke("seek", { t });
    } catch {
      // no clip yet — ignore
    }
  };

  // Annotation placement on the preview canvas.
  let dragStart: { x: number; y: number } | null = null;

  const canvasNorm = (e: MouseEvent): { x: number; y: number } => {
    const r = (e.currentTarget as HTMLCanvasElement).getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)),
    };
  };

  const onCanvasDown = async (e: MouseEvent) => {
    if (!hasClip()) return;
    const p = canvasNorm(e);
    const t = playhead();
    if (tool() === "text") {
      const text = window.prompt("Text label:");
      if (text) {
        await invoke("add_text", { text, x: p.x, y: p.y, t });
        await scrub(t);
      }
    } else if (tool() === "arrow" || tool() === "box") {
      dragStart = p;
    }
  };

  const onCanvasUp = async (e: MouseEvent) => {
    if (!dragStart || !hasClip()) return;
    const s = dragStart;
    dragStart = null;
    const p = canvasNorm(e);
    const t = playhead();
    if (tool() === "arrow") {
      await invoke("add_arrow", { fx: s.x, fy: s.y, tx: p.x, ty: p.y, t });
      await scrub(t);
    } else if (tool() === "box") {
      const x = Math.min(s.x, p.x);
      const y = Math.min(s.y, p.y);
      const w = Math.abs(p.x - s.x);
      const h = Math.abs(p.y - s.y);
      if (w > 0.005 && h > 0.005) {
        await invoke("add_box", { x, y, w, h, t });
        await scrub(t);
      }
    }
  };

  const toggleRecord = async () => {
    try {
      if (!recording()) {
        await invoke("start_recording");
        setRecording(true);
        setStatus("Recording… press Ctrl+Shift+Z to zoom where it matters · Stop when done");
      } else {
        const summary = await invoke<RecordingSummary>("stop_recording");
        setRecording(false);
        setHasClip(true);
        setDuration(summary.duration);
        setStatus(`Recorded ${summary.duration.toFixed(1)}s · ${summary.zooms} auto-zooms`);
        await scrub(0);
      }
    } catch (e) {
      setRecording(false);
      setStatus(`Error: ${String(e)}`);
    }
  };

  const onExport = async () => {
    try {
      setStatus("Exporting GIF…");
      await invoke("export_gif", { path: "vuoom-demo.gif", fps: 15, width: 1000 });
      setStatus("Exported vuoom-demo.gif");
    } catch (e) {
      setStatus(`Export failed: ${String(e)}`);
    }
  };

  return (
    <div class="editor">
      <header class="titlebar" data-tauri-drag-region="">
        <span class="brand">Vuoom</span>
        <div class="titlebar-right">
          <ThemeMenu current={theme()} onSelect={setTheme} />
          <WindowControls />
        </div>
      </header>

      <div class="toolbar">
        <button class="btn record" classList={{ active: recording() }} onClick={toggleRecord}>
          <span class="dot" /> {recording() ? "Stop" : "Record"}
        </button>
        <div class="project-title">Untitled</div>
        <button class="btn export" disabled={!hasClip()} onClick={onExport}>
          Export GIF
        </button>
      </div>

      <div class="workspace">
        <nav class="toolrail">
          <For each={TOOLS}>
            {(t) => (
              <button
                classList={{ tool: true, active: tool() === t.id }}
                onClick={() => setTool(t.id)}
                title={t.label}
              >
                {t.label}
              </button>
            )}
          </For>
        </nav>

        <main class="canvas">
          <div class="canvas-frame">
            <canvas
              ref={(el) => (canvasEl = el)}
              class="preview-canvas"
              classList={{ hidden: !hasClip() }}
              onMouseDown={(e) => void onCanvasDown(e)}
              onMouseUp={(e) => void onCanvasUp(e)}
            />
            <Show when={!hasClip()}>
              <div class="canvas-placeholder">
                <p class="big">Preview</p>
                <small>Record a clip to begin — auto-zoom is applied automatically.</small>
              </div>
            </Show>
          </div>
        </main>

        <aside class="properties">
          <h2>Properties</h2>
          <p class="muted">Select an element on the canvas to edit it.</p>

          <h3>Export presets</h3>
          <For each={presets()}>
            {(p, i) => (
              <div class="preset">
                <strong>{i() === 0 ? "README" : "High quality"}</strong>
                <span>
                  {p.fps} fps · {p.width ?? "source"}px · q{p.quality}
                </span>
              </div>
            )}
          </For>
        </aside>
      </div>

      <footer class="timeline">
        <div class="timeline-track">
          <Show
            when={hasClip()}
            fallback={<span class="muted">Timeline — record a clip to begin</span>}
          >
            <input
              class="scrubber"
              type="range"
              min="0"
              max={duration()}
              step="0.01"
              value={playhead()}
              onInput={(e) => void scrub(Number(e.currentTarget.value))}
            />
          </Show>
        </div>
        <div class="statusbar">{status()}</div>
      </footer>
    </div>
  );
}

export default App;
