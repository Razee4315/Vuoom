// The stage: the composited preview frame with the interactive annotation overlay on top
// (select / move / resize / draw), the zoom-focus crosshair, snap guides, and the inline
// text editor. The overlay mirrors the export renderer so what you see is what ships.
import { For, Index, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { LINE_HEIGHT, textBox } from "../editor/textMetrics";
import { CORNER_CURSORS, fontCss } from "../editor/constants";
import { ArrowLine, Handles } from "../EditorPrimitives";
import { cssColor } from "../format";
import { arrowHeads, v2, zoomFrame } from "../geometry";
import { Icon } from "../icons";
import { layout } from "../prefs";
import { TOOLS } from "../shortcuts";
import { stageMenu } from "./contextMenus";
import CropEditor, { CropBar } from "./CropEditor";
import type { Vec2 } from "../types";

export default function Stage() {
  const ed = useEditor();
  const hint = () => TOOLS.find((t) => t.id === ed.tool())?.hint;
  return (
    <main class="stage-wrap" classList={{ cropping: !!ed.cropEdit() }}>
      <Show when={ed.tool() !== "select" && !ed.cropEdit()}>
        <div class="stage-hint">
          <Icon name="info" size={13} />
          <span>{hint()}</span>
          <span class="stage-hint-esc">
            <kbd>Esc</kbd> done
          </span>
        </div>
      </Show>
      <div class="stage" ref={ed.refs.stage} style={{ "aspect-ratio": String(ed.frameAspect()), "--ar": String(ed.frameAspect()) }}>
        <canvas ref={ed.refs.canvas} class="stage-canvas" />
        <Show when={layout.guides() && !ed.cropEdit()}>
          <div class="stage-guides" aria-hidden="true">
            <i class="g v1" />
            <i class="g v2" />
            <i class="g h1" />
            <i class="g h2" />
            <i class="g cx" />
            <i class="g cy" />
          </div>
        </Show>
      <svg
        class="overlay"
        classList={{
          hidden: !!ed.cropEdit(),
          "tool-draw": ed.tool() !== "select" && ed.tool() !== "text" && ed.tool() !== "zoom",
          "tool-text": ed.tool() === "text",
          "tool-zoom": ed.tool() === "zoom",
        }}
        onPointerDown={(e) => void ed.onPointerDown(e)}
        onContextMenu={(e) => stageMenu(ed, e)}
        onPointerMove={ed.frameCanvas(ed.onPointerMove)}
        onPointerUp={(e) => void ed.onPointerUp(e)}
        onLostPointerCapture={() => {
          // A canceled gesture (pointercancel / capture lost) aborts cleanly:
          // create drafts are discarded, move/resize overrides are dropped.
          if (ed.drag()?.mode.startsWith("create-")) ed.setDrag(null);
          ed.setSnapX(null);
          ed.setSnapY(null);
        }}
      >
        {/* boxes */}
        <For each={ed.anns().highlights}>
          {(b) => {
            const sel = () => ed.isSelected("box", b.id);
            return (
              <Show when={ed.inView(b.range, sel())}>
                {(() => {
                  const g = () => ed.liveGeom("box", b.id);
                  const a = () => ed.px({ x: g()[0], y: g()[1] });
                  const s = () => ed.px({ x: g()[2], y: g()[3] });
                  return (
                    <g
                      opacity={ed.isGhost(b.range, sel()) ? 0.35 : 1}
                      style={{ cursor: sel() ? "move" : undefined }}
                    >
                      <Show
                        when={b.shape === "Ellipse"}
                        fallback={
                          <rect
                            x={a().x}
                            y={a().y}
                            width={s().x}
                            height={s().y}
                            fill={b.filled ? cssColor(b.color) : "none"}
                            stroke={cssColor(b.color)}
                            stroke-width={Math.max(b.thickness * ed.stage().h, 1.5)}
                          />
                        }
                      >
                        <ellipse
                          cx={a().x + s().x / 2}
                          cy={a().y + s().y / 2}
                          rx={s().x / 2}
                          ry={s().y / 2}
                          fill={b.filled ? cssColor(b.color) : "none"}
                          stroke={cssColor(b.color)}
                          stroke-width={Math.max(b.thickness * ed.stage().h, 1.5)}
                        />
                      </Show>
                      <Show when={sel()}>
                        <Handles
                          pts={[
                            { x: a().x, y: a().y },
                            { x: a().x + s().x, y: a().y },
                            { x: a().x, y: a().y + s().y },
                            { x: a().x + s().x, y: a().y + s().y },
                          ]}
                          cursors={CORNER_CURSORS}
                        />
                      </Show>
                    </g>
                  );
                })()}
              </Show>
            );
          }}
        </For>

        {/* arrows */}
        <For each={ed.anns().arrows}>
          {(ar) => {
            const sel = () => ed.isSelected("arrow", ar.id);
            return (
              <Show when={ed.inView(ar.range, sel())}>
                {(() => {
                  const g = () => ed.liveGeom("arrow", ar.id);
                  const f = () => ed.px({ x: g()[0], y: g()[1] });
                  const tp = () => ed.px({ x: g()[2], y: g()[3] });
                  return (
                    <g
                      opacity={ed.isGhost(ar.range, sel()) ? 0.35 : 1}
                      style={{ cursor: sel() ? "move" : undefined }}
                    >
                      <ArrowLine
                        from={f()}
                        to={tp()}
                        color={cssColor(ar.color)}
                        width={Math.max(ar.thickness * ed.stage().h, 1.5)}
                        headFrom={arrowHeads(ar.style).from}
                        headTo={arrowHeads(ar.style).to}
                      />
                      <Show when={sel()}>
                        <Handles pts={[f(), tp()]} cursors={["move", "move"]} />
                      </Show>
                    </g>
                  );
                })()}
              </Show>
            );
          }}
        </For>

        {/* text */}
        <For each={ed.anns().texts}>
          {(tx) => {
            const sel = () => ed.isSelected("text", tx.id);
            return (
              <Show when={ed.inView(tx.range, sel()) && ed.editingText() !== tx.id}>
                {(() => {
                  const g = () => ed.liveGeom("text", tx.id);
                  const p = () => ed.px({ x: g()[0], y: g()[1] });
                  const fs = () => ed.liveFont(tx.id, tx.font_size) * ed.stage().h;
                  const box = () => textBox(tx, fs());
                  const wbox = () => box().w;
                  return (
                    <g
                      opacity={ed.isGhost(tx.range, sel()) ? 0.35 : 1}
                      style={{ cursor: sel() ? "move" : undefined }}
                    >
                      <Show when={tx.background}>
                        <rect
                          class="text-plate"
                          x={p().x - fs() * 0.3}
                          y={p().y - fs() * 0.16}
                          width={wbox() + fs() * 0.6}
                          height={box().h + fs() * 0.32}
                          rx={fs() * 0.12}
                        />
                      </Show>
                      <text
                        x={p().x}
                        y={p().y + fs()}
                        font-size={String(fs())}
                        fill={cssColor(tx.color)}
                        style={{
                          "font-family": fontCss(tx.font),
                          "font-weight": tx.bold ? "700" : "400",
                          "font-style": tx.italic ? "italic" : "normal",
                          "white-space": "pre",
                        }}
                      >
                        <Index each={box().lines}>
                          {(line, i) => (
                            <tspan x={p().x} y={p().y + fs() + i * LINE_HEIGHT * fs()}>
                              {line() || " "}
                            </tspan>
                          )}
                        </Index>
                      </text>
                      <Show when={sel()}>
                        <rect
                          class="sel-outline"
                          x={p().x - 4}
                          y={p().y - 4}
                          width={wbox() + 8}
                          height={box().h + 8}
                        />
                        <Handles
                          pts={[
                            { x: p().x, y: p().y },
                            { x: p().x + wbox(), y: p().y },
                            { x: p().x, y: p().y + box().h },
                            { x: p().x + wbox(), y: p().y + box().h },
                          ]}
                          cursors={CORNER_CURSORS}
                        />
                      </Show>
                    </g>
                  );
                })()}
              </Show>
            );
          }}
        </For>

        {/* live creation draft */}
        <Show when={ed.drag()?.mode === "create-arrow"}>
          {(() => {
            const d = ed.drag() as { start: Vec2; cur: Vec2 };
            return <ArrowLine from={ed.px(d.start)} to={ed.px(d.cur)} color="#e5484d" />;
          })()}
        </Show>
        <Show when={ed.drag()?.mode === "create-zoom"}>
          {(() => {
            const d = ed.drag() as { start: Vec2; cur: Vec2 };
            const f = zoomFrame(d.start, d.cur, ed.zoomStrength());
            const a = ed.px({ x: f.x - f.side / 2, y: f.y - f.side / 2 });
            const w = f.side * ed.stage().w;
            const h = f.side * ed.stage().h;
            return (
              <g class="zoom-draft">
                <rect x={a.x} y={a.y} width={w} height={h} rx={6} />
                <text x={a.x + 10} y={a.y + 22}>
                  {f.amount.toFixed(1)}×
                </text>
              </g>
            );
          })()}
        </Show>
        <Show when={ed.drag()?.mode === "create-box"}>
          {(() => {
            const d = ed.drag() as { start: Vec2; cur: Vec2 };
            const a = ed.px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
            const w = Math.abs(d.cur.x - d.start.x) * ed.stage().w;
            const h = Math.abs(d.cur.y - d.start.y) * ed.stage().h;
            return (
              <rect x={a.x} y={a.y} width={w} height={h} fill="none" stroke="#ffd23f" stroke-width={2} />
            );
          })()}
        </Show>
        <Show when={ed.drag()?.mode === "create-highlight"}>
          {(() => {
            const d = ed.drag() as { start: Vec2; cur: Vec2 };
            const a = ed.px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
            const w = Math.abs(d.cur.x - d.start.x) * ed.stage().w;
            const h = Math.abs(d.cur.y - d.start.y) * ed.stage().h;
            return (
              <rect x={a.x} y={a.y} width={w} height={h} fill="rgba(255,214,63,0.3)" stroke="#ffd23f" stroke-width={1.5} />
            );
          })()}
        </Show>
        <Show when={ed.drag()?.mode === "create-mask"}>
          {(() => {
            const d = ed.drag() as { start: Vec2; cur: Vec2 };
            const a = ed.px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
            const w = Math.abs(d.cur.x - d.start.x) * ed.stage().w;
            const h = Math.abs(d.cur.y - d.start.y) * ed.stage().h;
            return (
              <rect x={a.x} y={a.y} width={w} height={h} fill="rgba(10,10,13,0.85)" stroke="#e5484d" stroke-width={1.5} stroke-dasharray="5 3" />
            );
          })()}
        </Show>
        <Show when={ed.drag()?.mode === "create-ellipse"}>
          {(() => {
            const d = ed.drag() as { start: Vec2; cur: Vec2 };
            const a = ed.px({ x: Math.min(d.start.x, d.cur.x), y: Math.min(d.start.y, d.cur.y) });
            const w = Math.abs(d.cur.x - d.start.x) * ed.stage().w;
            const h = Math.abs(d.cur.y - d.start.y) * ed.stage().h;
            return (
              <ellipse cx={a.x + w / 2} cy={a.y + h / 2} rx={w / 2} ry={h / 2} fill="none" stroke="#ffd23f" stroke-width={2} />
            );
          })()}
        </Show>

        {/* Zoom focus crosshair, ed.drag to aim the ed.selected zoom segment. */}
        <Show when={ed.selZoomFocus()}>
          {(() => {
            const f = () => ed.focusDrag() ?? ed.selZoomFocus()!;
            const p = () => ed.px(f());
            return (
              <g
                class="focus-reticle"
                onPointerDown={ed.onFocusDown}
                onPointerMove={ed.onFocusMove}
                onPointerUp={() => void ed.onFocusUp()}
              >
                <circle class="ring" cx={p().x} cy={p().y} r={16} />
                <circle class="dot" cx={p().x} cy={p().y} r={3} />
                <line x1={p().x - 26} y1={p().y} x2={p().x - 10} y2={p().y} />
                <line x1={p().x + 10} y1={p().y} x2={p().x + 26} y2={p().y} />
                <line x1={p().x} y1={p().y - 26} x2={p().x} y2={p().y - 10} />
                <line x1={p().x} y1={p().y + 10} x2={p().x} y2={p().y + 26} />
              </g>
            );
          })()}
        </Show>

        {/* Canvas alignment guides, flash when a dragged element snaps. */}
        <Show when={ed.snapX() !== null}>
          <line
            class="canvas-snap"
            x1={ed.snapX()! * ed.stage().w}
            y1={0}
            x2={ed.snapX()! * ed.stage().w}
            y2={ed.stage().h}
          />
        </Show>
        <Show when={ed.snapY() !== null}>
          <line
            class="canvas-snap"
            x1={0}
            y1={ed.snapY()! * ed.stage().h}
            x2={ed.stage().w}
            y2={ed.snapY()! * ed.stage().h}
          />
        </Show>
      </svg>

      <Show when={ed.editingTextAnn()}>
        <InlineTextEditor />
      </Show>
        <Show when={ed.cropEdit()}>
          <CropEditor />
        </Show>
      </div>
      <Show when={ed.cropEdit()}>
        <CropBar />
      </Show>
    </main>
  );
}

// The inline text editor, a component of its own so it is built once per edit: reading the
// labels in the parent Show would rebuild it (and re-select every word) on each keystroke.
function InlineTextEditor() {
  const ed = useEditor();
  // Reactive accessors so the editor box tracks the label as the canvas resizes (e.g. when
  // the inspector opens) and follows its id from temporary to real. The value stays
  // uncontrolled (seeded once) so typing never resets the caret.
  const live = () => ed.editingTextAnn();
  const initial = live()?.text ?? "";
  const p = () => {
    const t = live();
    return t ? ed.px({ x: v2(t.pos).x, y: v2(t.pos).y }) : { x: 0, y: 0 };
  };
  const fs = () => (live()?.font_size ?? 0.05) * ed.stage().h;
  // Sized to the words as they are typed (one character of slack so the caret fits).
  const box = () => {
    const t = live();
    return t ? textBox(t, fs()) : { w: fs(), h: fs() * LINE_HEIGHT, lines: [""] };
  };
  // Enter keeps the words, Escape puts back what was there; either way the blur that
  // follows must not commit a second time.
  let done = false;
  const finish = (cancel: boolean) => {
    if (done) return;
    done = true;
    void ed.finishTextEdit(cancel);
  };
  return (
    <textarea
      class="text-edit"
      classList={{ plate: !!live()?.background }}
      style={{
        left: `${p().x}px`,
        top: `${p().y}px`,
        width: `${box().w + fs() * 0.7}px`,
        height: `${box().h}px`,
        "font-size": `${fs()}px`,
        "line-height": String(LINE_HEIGHT),
        "font-family": fontCss(live()?.font ?? ""),
        "font-weight": live()?.bold ? "700" : "400",
        "font-style": live()?.italic ? "italic" : "normal",
        color: live() ? cssColor(live()!.color) : undefined,
      }}
      value={initial}
      rows={1}
      wrap="off"
      spellcheck={false}
      aria-label="Text label"
      ref={(el) =>
        queueMicrotask(() => {
          el.focus();
          el.select();
        })
      }
      onInput={(e) => ed.editTextLive(e.currentTarget.value)}
      onKeyDown={(e) => {
        // Shift+Enter starts a new line; Enter alone is done.
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          finish(false);
        } else if (e.key === "Escape") {
          e.preventDefault();
          finish(true);
        }
      }}
      onBlur={() => finish(false)}
    />
  );
}
