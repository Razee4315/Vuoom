import { createSignal, onMount, onCleanup, For, Show, type JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import WindowControls from "./WindowControls";
import ThemeMenu from "./ThemeMenu";
import { applyTheme, initialTheme } from "./themes";
import { PreviewClient } from "./preview";
import logoWhite from "./assets/logo-white.png";
import logoBlack from "./assets/logo-black.png";
import emptyState from "./assets/empty-state.png";
import "./App.css";

const LIGHT_THEMES = new Set(["mono-light", "paper"]);
const isLight = (id: string) => LIGHT_THEMES.has(id);

type Tool = "select" | "text" | "arrow" | "box";
type Vec2 = { x: number; y: number };

/** Mirrors src-tauri session::RecordingSummary. */
interface RecordingSummary {
  duration: number;
  frames: number;
  zooms: number;
}

interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}
interface TimeRange {
  start: number;
  end: number;
  fade_in: number;
  fade_out: number;
}
// glam DVec2 serializes as a [x, y] array.
type SerVec = [number, number];
interface TextAnn {
  id: number;
  text: string;
  pos: SerVec;
  font_size: number;
  color: Color;
  range: TimeRange;
}
interface ArrowAnn {
  id: number;
  from: SerVec;
  to: SerVec;
  color: Color;
  thickness: number;
  range: TimeRange;
}
interface BoxAnn {
  id: number;
  rect: { x: number; y: number; w: number; h: number };
  color: Color;
  thickness: number;
  filled: boolean;
  range: TimeRange;
}
interface AnnotationSet {
  texts: TextAnn[];
  arrows: ArrowAnn[];
  highlights: BoxAnn[];
}

type Kind = "text" | "arrow" | "box";
interface Selection {
  kind: Kind;
  id: number;
}

const TOOLS: { id: Tool; label: string; hint: string }[] = [
  { id: "select", label: "Select", hint: "Click an element to select, drag to move, drag a handle to resize." },
  { id: "text", label: "Text", hint: "Click on the video to drop a text label." },
  { id: "arrow", label: "Arrow", hint: "Drag on the video to draw an arrow." },
  { id: "box", label: "Box", hint: "Drag on the video to draw a highlight box." },
];

// ── small helpers ──────────────────────────────────────────────────────────────
const clamp01 = (n: number) => Math.min(1, Math.max(0, n));
const v2 = (p: SerVec | Vec2): Vec2 => (Array.isArray(p) ? { x: p[0], y: p[1] } : p);
const cssColor = (c: Color) =>
  `rgba(${Math.round(c.r * 255)},${Math.round(c.g * 255)},${Math.round(c.b * 255)},${c.a})`;
const h2 = (n: number) => Math.round(clamp01(n) * 255).toString(16).padStart(2, "0");
const rgbHex = (c: Color) => `#${h2(c.r)}${h2(c.g)}${h2(c.b)}`;
const hexRgb = (hex: string) => {
  const n = parseInt(hex.slice(1), 16);
  return { r: ((n >> 16) & 255) / 255, g: ((n >> 8) & 255) / 255, b: (n & 255) / 255 };
};
const fmt = (t: number) => {
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
};
const distToSeg = (p: Vec2, a: Vec2, b: Vec2) => {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy || 1e-9;
  let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
  t = Math.min(1, Math.max(0, t));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
};

// Drag state for the interactive overlay.
type Drag =
  | { mode: "create-arrow"; start: Vec2; cur: Vec2 }
  | { mode: "create-box"; start: Vec2; cur: Vec2 }
  | { mode: "move"; kind: Kind; id: number; grab: Vec2; orig: number[]; geom: number[] }
  | { mode: "resize"; kind: Kind; id: number; handle: string; orig: number[]; geom: number[] }
  | null;

function App() {
  const [tool, setTool] = createSignal<Tool>("select");
  const [status, setStatus] = createSignal("Ready");
  const [projectName, setProjectName] = createSignal("Untitled");
  const [editingText, setEditingText] = createSignal<number | null>(null);
  const [theme, setTheme] = createSignal(initialTheme());
  const [hasClip, setHasClip] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [playhead, setPlayhead] = createSignal(0);
  const [playing, setPlaying] = createSignal(false);

  const [anns, setAnns] = createSignal<AnnotationSet>({ texts: [], arrows: [], highlights: [] });
  const [selected, setSelected] = createSignal<Selection | null>(null);
  const [drag, setDrag] = createSignal<Drag>(null);
  const [stage, setStage] = createSignal({ w: 1, h: 1 });
  const [frameAspect, setFrameAspect] = createSignal(16 / 9);
  const [showExport, setShowExport] = createSignal(false);

  const preview = new PreviewClient();
  let canvasEl: HTMLCanvasElement | undefined;
  let stageEl: HTMLDivElement | undefined;
  let unlistenFinished: (() => void) | undefined;

  const onContextMenu = (e: MouseEvent) => {
    const el = e.target as HTMLElement;
    if (!el.closest("input, textarea, [contenteditable=true]")) e.preventDefault();
  };

  onMount(async () => {
    applyTheme(theme());
    document.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("keydown", onKey);
    if (canvasEl) preview.attach(canvasEl);
    preview.onAspectChange((a) => setFrameAspect(a));
    if (stageEl) {
      const ro = new ResizeObserver(() => {
        if (stageEl) setStage({ w: stageEl.clientWidth, h: stageEl.clientHeight });
      });
      ro.observe(stageEl);
      onCleanup(() => ro.disconnect());
    }
    unlistenFinished = await listen<RecordingSummary>("recording-finished", (e) =>
      void loadFinishedClip(e.payload),
    );
    try {
      const port = await invoke<number>("preview_port");
      preview.connect(port);
      setStatus("Engine connected · record to begin");
    } catch (e) {
      setStatus(`Backend error: ${String(e)}`);
    }
  });
  onCleanup(() => {
    document.removeEventListener("contextmenu", onContextMenu);
    window.removeEventListener("keydown", onKey);
    unlistenFinished?.();
    preview.disconnect();
  });

  // ── seek throttling (shared by scrubbing, playback, live edits) ────────────────
  let seekBusy = false;
  let seekPending: number | null = null;
  const pushSeek = async (t: number) => {
    if (seekBusy) {
      seekPending = t;
      return;
    }
    seekBusy = true;
    try {
      await invoke("seek", { t });
    } catch {
      /* no clip yet */
    }
    seekBusy = false;
    if (seekPending !== null) {
      const n = seekPending;
      seekPending = null;
      void pushSeek(n);
    }
  };
  const scrub = (t: number) => {
    setPlayhead(t);
    void pushSeek(t);
  };

  const refresh = async () => {
    try {
      setAnns(await invoke<AnnotationSet>("list_annotations"));
    } catch {
      /* no recording */
    }
  };

  // ── playback transport ─────────────────────────────────────────────────────────
  let raf = 0;
  let lastTs = 0;
  const tick = (ts: number) => {
    if (!playing()) return;
    if (lastTs) {
      let t = playhead() + (ts - lastTs) / 1000;
      if (t >= duration()) {
        t = duration();
        setPlaying(false);
      }
      setPlayhead(t);
      void pushSeek(t);
    }
    lastTs = ts;
    if (playing()) raf = requestAnimationFrame(tick);
  };
  const togglePlay = () => {
    if (!hasClip()) return;
    if (playing()) {
      setPlaying(false);
      cancelAnimationFrame(raf);
    } else {
      if (playhead() >= duration() - 1e-3) scrub(0);
      setPlaying(true);
      lastTs = 0;
      raf = requestAnimationFrame(tick);
    }
  };
  const restart = () => {
    setPlaying(false);
    cancelAnimationFrame(raf);
    scrub(0);
  };

  const onKey = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest("input, textarea")) return;
    if ((e.key === "Delete" || e.key === "Backspace") && selected()) {
      e.preventDefault();
      void deleteSelected();
    } else if (e.key === "Escape" && selected()) {
      setSelected(null);
    } else if (e.code === "Space" && hasClip()) {
      e.preventDefault();
      togglePlay();
    }
  };

  // ── coordinate mapping ──────────────────────────────────────────────────────────
  const norm = (e: PointerEvent): Vec2 => {
    const r = stageEl!.getBoundingClientRect();
    return { x: clamp01((e.clientX - r.left) / r.width), y: clamp01((e.clientY - r.top) / r.height) };
  };
  const px = (n: Vec2) => ({ x: n.x * stage().w, y: n.y * stage().h });

  // visible at the current playhead (or selected so it stays editable when scrubbed past)
  const inView = (r: TimeRange, sel: boolean) =>
    sel || (playhead() >= r.start && playhead() < r.end);

  // ── live edit (update + re-composite), throttled ─────────────────────────────────
  let editBusy = false;
  let editPending: (() => Promise<void>) | null = null;
  const pushEdit = async (op: () => Promise<void>) => {
    if (editBusy) {
      editPending = op;
      return;
    }
    editBusy = true;
    try {
      await op();
      await invoke("seek", { t: playhead() });
    } catch {
      /* ignore */
    }
    editBusy = false;
    if (editPending) {
      const n = editPending;
      editPending = null;
      void pushEdit(n);
    }
  };

  // Geometry of an annotation as a flat number[] (for the drag override + live updates).
  const geomOf = (kind: Kind, id: number): number[] => {
    if (kind === "box") {
      const b = anns().highlights.find((a) => a.id === id)!;
      return [b.rect.x, b.rect.y, b.rect.w, b.rect.h];
    }
    if (kind === "arrow") {
      const a = anns().arrows.find((x) => x.id === id)!;
      return [a.from[0], a.from[1], a.to[0], a.to[1]];
    }
    const t = anns().texts.find((x) => x.id === id)!;
    return [t.pos[0], t.pos[1]];
  };
  // The geometry the overlay should draw for an item (drag override if it is being dragged).
  const liveGeom = (kind: Kind, id: number): number[] => {
    const d = drag();
    if (d && (d.mode === "move" || d.mode === "resize") && d.kind === kind && d.id === id) return d.geom;
    return geomOf(kind, id);
  };
  const applyGeom = async (kind: Kind, id: number, g: number[]) => {
    if (kind === "box") await invoke("update_box", { id, x: g[0], y: g[1], w: g[2], h: g[3] });
    else if (kind === "arrow")
      await invoke("update_arrow", { id, fx: g[0], fy: g[1], tx: g[2], ty: g[3] });
    else await invoke("update_text", { id, x: g[0], y: g[1] });
  };
  // throttled live update while dragging
  const commitGeom = (kind: Kind, g: number[], id: number) =>
    pushEdit(() => applyGeom(kind, id, g));

  // ── hit testing (normalized) ─────────────────────────────────────────────────────
  const TOL = () => 8 / Math.max(stage().w, stage().h); // ~8px in normalized space
  const handleAt = (p: Vec2): string | null => {
    const s = selected();
    if (!s) return null;
    const g = liveGeom(s.kind, s.id);
    const near = (hx: number, hy: number) => Math.hypot(p.x - hx, p.y - hy) <= TOL() * 1.4;
    if (s.kind === "box") {
      const [x, y, w, h] = g;
      if (near(x, y)) return "nw";
      if (near(x + w, y)) return "ne";
      if (near(x, y + h)) return "sw";
      if (near(x + w, y + h)) return "se";
    } else if (s.kind === "arrow") {
      if (near(g[0], g[1])) return "from";
      if (near(g[2], g[3])) return "to";
    }
    return null;
  };
  const hitTest = (p: Vec2): Selection | null => {
    for (const b of anns().highlights) {
      if (!inView(b.range, false)) continue;
      const [x, y, w, h] = [b.rect.x, b.rect.y, b.rect.w, b.rect.h];
      if (p.x >= x - TOL() && p.x <= x + w + TOL() && p.y >= y - TOL() && p.y <= y + h + TOL())
        return { kind: "box", id: b.id };
    }
    for (const a of anns().arrows) {
      if (!inView(a.range, false)) continue;
      if (distToSeg(p, v2(a.from), v2(a.to)) <= TOL() * 1.5) return { kind: "arrow", id: a.id };
    }
    for (const t of anns().texts) {
      if (!inView(t.range, false)) continue;
      const pos = v2(t.pos);
      const wApprox = (t.text.length * t.font_size * 0.6) || 0.05;
      if (p.x >= pos.x - TOL() && p.x <= pos.x + wApprox && p.y >= pos.y - t.font_size && p.y <= pos.y + TOL())
        return { kind: "text", id: t.id };
    }
    return null;
  };

  // ── pointer interaction on the overlay ───────────────────────────────────────────
  const onPointerDown = async (e: PointerEvent) => {
    if (!hasClip()) return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    const p = norm(e);
    const t = tool();

    if (t === "text") {
      const id = await invoke<number>("add_text", { text: "Text", x: p.x, y: p.y, t: playhead() });
      await refresh();
      await pushSeek(playhead());
      setSelected({ kind: "text", id });
      setEditingText(id);
      setTool("select");
      return;
    }
    if (t === "arrow") {
      setDrag({ mode: "create-arrow", start: p, cur: p });
      return;
    }
    if (t === "box") {
      setDrag({ mode: "create-box", start: p, cur: p });
      return;
    }

    // select tool: handle → resize, body → move, empty → deselect
    const h = handleAt(p);
    if (h && selected()) {
      const s = selected()!;
      const g = geomOf(s.kind, s.id);
      setDrag({ mode: "resize", kind: s.kind, id: s.id, handle: h, orig: g, geom: g.slice() });
      return;
    }
    const hit = hitTest(p);
    setSelected(hit);
    if (hit) {
      const g = geomOf(hit.kind, hit.id);
      setDrag({ mode: "move", kind: hit.kind, id: hit.id, grab: p, orig: g, geom: g.slice() });
    }
  };

  const onPointerMove = (e: PointerEvent) => {
    const d = drag();
    if (!d) return;
    const p = norm(e);
    if (d.mode === "create-arrow" || d.mode === "create-box") {
      setDrag({ ...d, cur: p });
      return;
    }
    if (d.mode === "move") {
      const og = d.orig;
      const dx = p.x - d.grab.x;
      const dy = p.y - d.grab.y;
      let g: number[];
      if (d.kind === "box") g = [clamp01(og[0] + dx), clamp01(og[1] + dy), og[2], og[3]];
      else if (d.kind === "arrow")
        g = [clamp01(og[0] + dx), clamp01(og[1] + dy), clamp01(og[2] + dx), clamp01(og[3] + dy)];
      else g = [clamp01(og[0] + dx), clamp01(og[1] + dy)];
      setDrag({ ...d, geom: g });
      void commitGeom(d.kind, g, d.id);
    } else if (d.mode === "resize") {
      const og = d.orig;
      let g = og.slice();
      if (d.kind === "box") {
        let [x, y, w, h] = og;
        let x2 = x + w;
        let y2 = y + h;
        if (d.handle.includes("w")) x = p.x;
        if (d.handle.includes("e")) x2 = p.x;
        if (d.handle.includes("n")) y = p.y;
        if (d.handle.includes("s")) y2 = p.y;
        g = [Math.min(x, x2), Math.min(y, y2), Math.abs(x2 - x), Math.abs(y2 - y)];
      } else if (d.kind === "arrow") {
        g = d.handle === "from" ? [p.x, p.y, og[2], og[3]] : [og[0], og[1], p.x, p.y];
      }
      setDrag({ ...d, geom: g });
      void commitGeom(d.kind, g, d.id);
    }
  };

  const onPointerUp = async (e: PointerEvent) => {
    const d = drag();
    setDrag(null);
    if (!d) return;
    const p = norm(e);
    if (d.mode === "create-arrow") {
      if (Math.hypot(p.x - d.start.x, p.y - d.start.y) > 0.01) {
        const id = await invoke<number>("add_arrow", {
          fx: d.start.x,
          fy: d.start.y,
          tx: p.x,
          ty: p.y,
          t: playhead(),
        });
        await refresh();
        await pushSeek(playhead());
        setSelected({ kind: "arrow", id });
        setTool("select");
      }
    } else if (d.mode === "create-box") {
      const x = Math.min(d.start.x, p.x);
      const y = Math.min(d.start.y, p.y);
      const w = Math.abs(p.x - d.start.x);
      const h = Math.abs(p.y - d.start.y);
      if (w > 0.01 && h > 0.01) {
        const id = await invoke<number>("add_box", { x, y, w, h, t: playhead() });
        await refresh();
        await pushSeek(playhead());
        setSelected({ kind: "box", id });
        setTool("select");
      }
    } else {
      // Final authoritative commit (the throttle may have dropped the last move),
      // then refresh the source of truth.
      await applyGeom(d.kind, d.id, d.geom);
      await pushSeek(playhead());
      await refresh();
    }
  };

  // ── selected-element editing ─────────────────────────────────────────────────────
  const selectedText = () => {
    const s = selected();
    return s?.kind === "text" ? anns().texts.find((t) => t.id === s.id) : undefined;
  };
  const selectedColor = (): Color | undefined => {
    const s = selected();
    if (!s) return undefined;
    if (s.kind === "text") return anns().texts.find((t) => t.id === s.id)?.color;
    if (s.kind === "arrow") return anns().arrows.find((a) => a.id === s.id)?.color;
    return anns().highlights.find((b) => b.id === s.id)?.color;
  };
  const setColor = async (hex: string) => {
    const s = selected();
    if (!s) return;
    const c = hexRgb(hex);
    await invoke("set_annotation_color", { id: s.id, r: c.r, g: c.g, b: c.b });
    await refresh();
    await pushSeek(playhead());
  };
  const editText = async (text: string) => {
    const s = selected();
    if (s?.kind !== "text") return;
    await invoke("update_text", { id: s.id, text });
    await refresh();
    await pushSeek(playhead());
  };
  const editFontSize = async (size: number) => {
    const s = selected();
    if (s?.kind !== "text") return;
    await invoke("update_text", { id: s.id, fontSize: size });
    await refresh();
    await pushSeek(playhead());
  };
  const deleteSelected = async () => {
    const s = selected();
    if (!s) return;
    await invoke("delete_annotation", { id: s.id });
    setSelected(null);
    await refresh();
    await pushSeek(playhead());
  };

  // ── inline text editing ──────────────────────────────────────────────────────────
  const editingTextAnn = () => {
    const id = editingText();
    return id === null ? undefined : anns().texts.find((t) => t.id === id);
  };
  const editTextLive = (text: string) => {
    const id = editingText();
    if (id === null) return;
    void pushEdit(() => invoke("update_text", { id, text }));
  };
  const finishTextEdit = async () => {
    const id = editingText();
    setEditingText(null);
    if (id === null) return;
    await refresh(); // sync the live-typed value before deciding
    const ann = anns().texts.find((t) => t.id === id);
    if (ann && ann.text.trim() === "") {
      await invoke("delete_annotation", { id });
      setSelected(null);
      await refresh();
    }
    await pushSeek(playhead());
  };

  // ── recording / export ───────────────────────────────────────────────────────────
  // Recording happens in dedicated overlay windows (region selector → countdown → stop
  // bar), all hidden from the capture. The editor just kicks off the flow and waits for
  // the finished clip.
  const startRecord = async () => {
    try {
      setStatus("Choose the area to record…");
      await invoke("start_record_flow");
    } catch (e) {
      setStatus(`Error: ${String(e)}`);
    }
  };

  const loadFinishedClip = async (summary: RecordingSummary) => {
    setHasClip(true);
    setDuration(summary.duration);
    setSelected(null);
    setStatus(`Recorded ${summary.duration.toFixed(1)}s · ${summary.zooms} zooms`);
    await refresh();
    scrub(0);
  };

  return (
    <div class="editor">
      <header class="titlebar" data-tauri-drag-region="">
        <img class="brand-logo" src={isLight(theme()) ? logoBlack : logoWhite} alt="Vuoom" />
        <div class="titlebar-right">
          <ThemeMenu current={theme()} onSelect={setTheme} />
          <WindowControls />
        </div>
      </header>

      <div class="toolbar">
        <button class="btn record" onClick={() => void startRecord()}>
          <span class="dot" /> Record
        </button>
        <input
          class="project-name"
          value={projectName()}
          spellcheck={false}
          aria-label="Project name"
          onInput={(e) => setProjectName(e.currentTarget.value || "Untitled")}
          onFocus={(e) => e.currentTarget.select()}
        />
        <button class="btn export" disabled={!hasClip()} onClick={() => setShowExport(true)}>
          Export GIF
        </button>
      </div>

      <div class="workspace" classList={{ "has-inspector": !!selected() }}>
        <nav class="toolrail">
          <For each={TOOLS}>
            {(t) => (
              <button
                classList={{ tool: true, active: tool() === t.id }}
                onClick={() => setTool(t.id)}
                title={t.hint}
              >
                <ToolIcon tool={t.id} />
                <span>{t.label}</span>
              </button>
            )}
          </For>
        </nav>

        <main class="canvas">
          <div class="tool-hint">{TOOLS.find((t) => t.id === tool())?.hint}</div>
          <div
            class="canvas-frame"
            ref={(el) => (stageEl = el)}
            style={{ "aspect-ratio": String(frameAspect()) }}
          >
            <canvas
              ref={(el) => (canvasEl = el)}
              class="preview-canvas"
              classList={{ hidden: !hasClip() }}
            />
            <Show when={!hasClip()}>
              <div class="canvas-placeholder">
                <img class="empty-illustration" src={emptyState} alt="" />
                <p class="big">Ready when you are</p>
                <small>Press Record to frame your shot — auto-zoom follows the cursor. Ctrl+Shift+Z to zoom.</small>
              </div>
            </Show>

            <Show when={hasClip()}>
              <svg
                class="overlay"
                classList={{ "tool-draw": tool() !== "select" }}
                onPointerDown={(e) => void onPointerDown(e)}
                onPointerMove={onPointerMove}
                onPointerUp={(e) => void onPointerUp(e)}
                onDblClick={(e) => {
                  const hit = hitTest(norm(e as unknown as PointerEvent));
                  if (hit?.kind === "text") {
                    setSelected(hit);
                    setEditingText(hit.id);
                  }
                }}
              >
                {/* boxes */}
                <For each={anns().highlights}>
                  {(b) => {
                    const sel = () => selected()?.kind === "box" && selected()?.id === b.id;
                    return (
                      <Show when={inView(b.range, sel())}>
                        {(() => {
                          const g = () => liveGeom("box", b.id);
                          const a = () => px({ x: g()[0], y: g()[1] });
                          const s = () => px({ x: g()[2], y: g()[3] });
                          return (
                            <>
                              <rect
                                x={a().x}
                                y={a().y}
                                width={s().x}
                                height={s().y}
                                fill={b.filled ? cssColor(b.color) : "none"}
                                stroke={cssColor(b.color)}
                                stroke-width={2}
                              />
                              <Show when={sel()}>
                                <Handles
                                  pts={[
                                    { x: a().x, y: a().y },
                                    { x: a().x + s().x, y: a().y },
                                    { x: a().x, y: a().y + s().y },
                                    { x: a().x + s().x, y: a().y + s().y },
                                  ]}
                                />
                              </Show>
                            </>
                          );
                        })()}
                      </Show>
                    );
                  }}
                </For>

                {/* arrows */}
                <For each={anns().arrows}>
                  {(ar) => {
                    const sel = () => selected()?.kind === "arrow" && selected()?.id === ar.id;
                    return (
                      <Show when={inView(ar.range, sel())}>
                        {(() => {
                          const g = () => liveGeom("arrow", ar.id);
                          const f = () => px({ x: g()[0], y: g()[1] });
                          const tp = () => px({ x: g()[2], y: g()[3] });
                          return (
                            <>
                              <ArrowLine from={f()} to={tp()} color={cssColor(ar.color)} />
                              <Show when={sel()}>
                                <Handles pts={[f(), tp()]} />
                              </Show>
                            </>
                          );
                        })()}
                      </Show>
                    );
                  }}
                </For>

                {/* text */}
                <For each={anns().texts}>
                  {(tx) => {
                    const sel = () => selected()?.kind === "text" && selected()?.id === tx.id;
                    return (
                      <Show when={inView(tx.range, sel())}>
                        {(() => {
                          const g = () => liveGeom("text", tx.id);
                          const p = () => px({ x: g()[0], y: g()[1] });
                          const fs = () => tx.font_size * stage().h;
                          return (
                            <>
                              <text
                                x={p().x}
                                y={p().y + fs()}
                                font-size={String(fs())}
                                fill={cssColor(tx.color)}
                                style={{ "font-family": "Inter, sans-serif", "font-weight": "600" }}
                              >
                                {tx.text}
                              </text>
                              <Show when={sel()}>
                                <rect
                                  class="sel-outline"
                                  x={p().x - 4}
                                  y={p().y - 4}
                                  width={Math.max(40, tx.text.length * fs() * 0.6) + 8}
                                  height={fs() + 8}
                                />
                              </Show>
                            </>
                          );
                        })()}
                      </Show>
                    );
                  }}
                </For>

                {/* live creation draft */}
                <Show when={drag()?.mode === "create-arrow"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    return <ArrowLine from={px(d.start)} to={px(d.cur)} color="#e5484d" />;
                  })()}
                </Show>
                <Show when={drag()?.mode === "create-box"}>
                  {(() => {
                    const d = drag() as { start: Vec2; cur: Vec2 };
                    const a = px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
                    const w = Math.abs(d.cur.x - d.start.x) * stage().w;
                    const h = Math.abs(d.cur.y - d.start.y) * stage().h;
                    return (
                      <rect x={a.x} y={a.y} width={w} height={h} fill="none" stroke="#ffd23f" stroke-width={2} />
                    );
                  })()}
                </Show>
              </svg>

              <Show when={editingTextAnn()}>
                {(() => {
                  const ta = editingTextAnn()!;
                  const p = px({ x: v2(ta.pos).x, y: v2(ta.pos).y });
                  const fs = ta.font_size * stage().h;
                  return (
                    <input
                      class="text-edit"
                      style={{ left: `${p.x}px`, top: `${p.y}px`, "font-size": `${fs}px` }}
                      value={ta.text}
                      spellcheck={false}
                      ref={(el) => queueMicrotask(() => { el.focus(); el.select(); })}
                      onInput={(e) => editTextLive(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === "Escape") {
                          e.preventDefault();
                          e.currentTarget.blur();
                        }
                      }}
                      onBlur={() => void finishTextEdit()}
                    />
                  );
                })()}
              </Show>
            </Show>
          </div>
        </main>

        <Show when={selected()}>
          <aside class="properties">
            <div class="inspector-head">
              <h2>{selected()!.kind[0].toUpperCase() + selected()!.kind.slice(1)}</h2>
              <button class="icon-btn" title="Done" onClick={() => setSelected(null)}>
                ✕
              </button>
            </div>

            <Show when={selectedText()}>
              <label class="field">
                <span>Text</span>
                <input
                  type="text"
                  value={selectedText()!.text}
                  onInput={(e) => void editText(e.currentTarget.value)}
                />
              </label>
              <label class="field">
                <span>Size</span>
                <input
                  type="range"
                  min="0.02"
                  max="0.2"
                  step="0.005"
                  value={selectedText()!.font_size}
                  onInput={(e) => void editFontSize(Number(e.currentTarget.value))}
                />
              </label>
            </Show>

            <Show when={selectedColor()}>
              <label class="field">
                <span>Color</span>
                <input
                  type="color"
                  value={rgbHex(selectedColor()!)}
                  onInput={(e) => void setColor(e.currentTarget.value)}
                />
              </label>
            </Show>

            <p class="muted small">Drag to move · drag a handle to resize · Delete to remove.</p>
            <button class="btn danger" onClick={() => void deleteSelected()}>
              Delete element
            </button>
          </aside>
        </Show>
      </div>

      <footer class="timeline">
        <div class="transport">
          <button class="tbtn" title="Restart" disabled={!hasClip()} onClick={restart}>
            ⏮
          </button>
          <button class="tbtn play" title="Play / Pause" disabled={!hasClip()} onClick={togglePlay}>
            {playing() ? "⏸" : "▶"}
          </button>
          <span class="time">
            {fmt(playhead())} / {fmt(duration())}
          </span>
          <div class="timeline-track">
            <Show
              when={hasClip()}
              fallback={<span class="muted">Record a clip to begin</span>}
            >
              <input
                class="scrubber"
                type="range"
                min="0"
                max={duration()}
                step="0.01"
                value={playhead()}
                onInput={(e) => scrub(Number(e.currentTarget.value))}
              />
            </Show>
          </div>
        </div>
        <div class="statusbar">{status()}</div>
      </footer>

      <Show when={showExport()}>
        <ExportDialog name={projectName()} onClose={() => setShowExport(false)} onStatus={setStatus} />
      </Show>
    </div>
  );
}

// ── sub-components ────────────────────────────────────────────────────────────────

function ToolIcon(props: { tool: Tool }): JSX.Element {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "1.8", "stroke-linecap": "round" as const, "stroke-linejoin": "round" as const };
  switch (props.tool) {
    case "select":
      return <svg {...common}><path d="M5 3l6 16 2-6 6-2z" /></svg>;
    case "text":
      return <svg {...common}><path d="M5 6V4h14v2M12 4v16M9 20h6" /></svg>;
    case "arrow":
      return <svg {...common}><path d="M5 19L19 5M11 5h8v8" /></svg>;
    case "box":
      return <svg {...common}><rect x="4" y="6" width="16" height="12" rx="1" /></svg>;
  }
}

function Handles(props: { pts: Vec2[] }): JSX.Element {
  return (
    <For each={props.pts}>
      {(p) => <rect class="handle" x={p.x - 5} y={p.y - 5} width={10} height={10} />}
    </For>
  );
}

function ArrowLine(props: { from: Vec2; to: Vec2; color: string }): JSX.Element {
  const ang = () => Math.atan2(props.to.y - props.from.y, props.to.x - props.from.x);
  const head = (off: number) => ({
    x: props.to.x - 14 * Math.cos(ang() - off),
    y: props.to.y - 14 * Math.sin(ang() - off),
  });
  return (
    <g stroke={props.color} fill={props.color} stroke-width={3} stroke-linecap="round">
      <line x1={props.from.x} y1={props.from.y} x2={props.to.x} y2={props.to.y} />
      <polygon
        points={`${props.to.x},${props.to.y} ${head(0.5).x},${head(0.5).y} ${head(-0.5).x},${head(-0.5).y}`}
        stroke="none"
      />
    </g>
  );
}

function ExportDialog(props: {
  name: string;
  onClose: () => void;
  onStatus: (s: string) => void;
}): JSX.Element {
  const [preset, setPreset] = createSignal<"readme" | "hq" | "custom">("readme");
  const [fps, setFps] = createSignal(15);
  const [width, setWidth] = createSignal(1000);
  const [quality, setQuality] = createSignal(80);
  const [busy, setBusy] = createSignal(false);

  const applyPreset = (p: "readme" | "hq" | "custom") => {
    setPreset(p);
    if (p === "readme") {
      setFps(15);
      setWidth(1000);
      setQuality(80);
    } else if (p === "hq") {
      setFps(20);
      setWidth(1280);
      setQuality(95);
    }
  };

  const doExport = async () => {
    try {
      const safe = props.name.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "vuoom";
      const path = await save({
        defaultPath: `${safe}.gif`,
        filters: [{ name: "GIF", extensions: ["gif"] }],
      });
      if (!path) return;
      setBusy(true);
      props.onStatus("Exporting GIF…");
      await invoke("export_gif", { path, fps: fps(), width: width(), quality: quality() });
      props.onStatus(`Exported ${path}`);
      props.onClose();
    } catch (e) {
      props.onStatus(`Export failed: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="modal-backdrop" onClick={props.onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Export GIF</h2>
        <div class="preset-row">
          <button classList={{ chip: true, active: preset() === "readme" }} onClick={() => applyPreset("readme")}>
            README<small>small · 15fps · 1000px</small>
          </button>
          <button classList={{ chip: true, active: preset() === "hq" }} onClick={() => applyPreset("hq")}>
            High quality<small>crisp · 20fps · 1280px</small>
          </button>
          <button classList={{ chip: true, active: preset() === "custom" }} onClick={() => applyPreset("custom")}>
            Custom<small>tune it yourself</small>
          </button>
        </div>

        <label class="field">
          <span>Frame rate · {fps()} fps</span>
          <input type="range" min="8" max="30" step="1" value={fps()} onInput={(e) => { setFps(Number(e.currentTarget.value)); setPreset("custom"); }} />
        </label>
        <label class="field">
          <span>Max width · {width()} px</span>
          <input type="range" min="400" max="1920" step="20" value={width()} onInput={(e) => { setWidth(Number(e.currentTarget.value)); setPreset("custom"); }} />
        </label>
        <label class="field">
          <span>Quality · {quality()}</span>
          <input type="range" min="40" max="100" step="1" value={quality()} onInput={(e) => { setQuality(Number(e.currentTarget.value)); setPreset("custom"); }} />
        </label>

        <div class="modal-actions">
          <button class="btn" onClick={props.onClose} disabled={busy()}>
            Cancel
          </button>
          <button class="btn export" onClick={() => void doExport()} disabled={busy()}>
            {busy() ? "Exporting…" : "Choose location & export"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default App;
