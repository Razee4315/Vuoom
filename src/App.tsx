import { createSignal, onMount, onCleanup, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import WindowControls from "./WindowControls";
import ThemeMenu from "./ThemeMenu";
import { applyTheme, initialTheme } from "./themes";
import "./App.css";

type Tool = "select" | "text" | "arrow" | "box" | "crop";

/** Mirrors vuoom_encode::GifSettings (serde). */
interface GifSettings {
  fps: number;
  width: number | null;
  quality: number;
  lossy: number | null;
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

  // Avoid the native browser context menu in app chrome (keep it for text fields).
  const onContextMenu = (e: MouseEvent) => {
    const el = e.target as HTMLElement;
    if (!el.closest("input, textarea, [contenteditable=true]")) e.preventDefault();
  };

  onMount(async () => {
    applyTheme(theme());
    document.addEventListener("contextmenu", onContextMenu);
    try {
      const [readme, hq] = await invoke<[GifSettings, GifSettings]>("gif_presets");
      setPresets([readme, hq]);
      setStatus("Engine connected");
    } catch (e) {
      setStatus(`Backend error: ${String(e)}`);
    }
  });
  onCleanup(() => document.removeEventListener("contextmenu", onContextMenu));

  return (
    <div class="editor">
      {/* Custom frameless titlebar (draggable) */}
      <header class="titlebar" data-tauri-drag-region="">
        <span class="brand">Vuoom</span>
        <div class="titlebar-right">
          <ThemeMenu current={theme()} onSelect={setTheme} />
          <WindowControls />
        </div>
      </header>

      {/* Action toolbar */}
      <div class="toolbar">
        <button class="btn record">
          <span class="dot" /> Record
        </button>
        <div class="project-title">Untitled</div>
        <button class="btn export">Export GIF</button>
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
            <div class="canvas-placeholder">
              <p class="big">Preview</p>
              <small>Record a clip to begin — auto-zoom is applied automatically.</small>
            </div>
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
          <span class="muted">Timeline — trim · zoom blocks · text &amp; annotations</span>
        </div>
        <div class="statusbar">{status()}</div>
      </footer>
    </div>
  );
}

export default App;
