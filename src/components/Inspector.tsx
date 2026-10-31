// The inspector: a resizable, collapsible right panel with two tabs.
//  - Selection: every property of whatever is selected (annotation, zoom, speed, cut), or
//    the armed tool's card, in collapsible sections that remember their state.
//  - Clip: whole-recording settings: frame + backdrop, crop, camera (auto zooms), pacing
//    (skim idle), click ripples + keystrokes, and clip facts.
import { createEffect, createSignal, For, Index, on, onCleanup, onMount, Show, type JSX } from "solid-js";
import { trackLabel } from "../editor/audio";
import {
  CAPTION_LANGUAGES,
  CAPTION_MAX_SIZE,
  CAPTION_MIN_SIZE,
  jobFraction,
  jobLabel,
} from "../editor/captions";
import { useEditor } from "../editor/context";
import { PRESET_COLORS, TEXT_FONTS } from "../editor/constants";
import { fmt, hexRgb, rgbHex } from "../format";
import { isMarker, PEN_LOOKS } from "../editor/strokes";
import { outputDuration } from "../geometry";
import { t } from "../i18n";
import { Icon, type IconName } from "../icons";
import { layout, prefs, setInspectorW } from "../prefs";
import ScrubField from "../ScrubField";
import { TOOLS } from "../shortcuts";
import type { CameraCorner, ClickEffect, CropRect, PointerLook } from "../types";

/** The webcam bubble's corners, in reading order for the 2×2 picker. */
const CAMERA_CORNERS = (): { value: CameraCorner; label: string }[] => [
  { value: "top-left", label: t("Top left") },
  { value: "top-right", label: t("Top right") },
  { value: "bottom-left", label: t("Bottom left") },
  { value: "bottom-right", label: t("Bottom right") },
];
import { Field, IconButton, Rich, Section, Seg, sectionQuery, setSectionQuery, Slider, Switch } from "../ui";

export default function Inspector() {
  const ed = useEditor();
  const [tab, setTab] = createSignal<"selection" | "clip">("clip");
  const busy = () => ed.somethingSelected() || ed.drawingToolActive();
  // Follow the user's focus: selecting something (or arming a tool) shows its properties,
  // clearing it returns to the clip settings.
  createEffect(on(busy, (b) => setTab(b ? "selection" : "clip")));

  let dragging = false;
  const onResizeDown = (e: PointerEvent) => {
    dragging = true;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    document.body.classList.add("resizing-x");
  };
  const onResizeMove = (e: PointerEvent) => {
    if (dragging) setInspectorW(window.innerWidth - e.clientX - 8);
  };
  const onResizeUp = () => {
    dragging = false;
    document.body.classList.remove("resizing-x");
  };

  return (
    <aside class="inspector panel" style={{ width: `min(${layout.inspectorW()}px, 36vw)` }} aria-label={t("Inspector")}>
      <div
        class="resizer resizer-x"
        aria-hidden="true"
        onPointerDown={onResizeDown}
        onPointerMove={onResizeMove}
        onPointerUp={onResizeUp}
        onDblClick={() => layout.inspectorW.reset()}
      />
      <header class="insp-head">
        <div class="insp-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            class="insp-tab"
            classList={{ on: tab() === "selection" }}
            aria-selected={tab() === "selection"}
            onClick={() => setTab("selection")}
          >
            {t("Selection")}
            <Show when={ed.selCount() > 1}>
              <span class="badge">{ed.selCount()}</span>
            </Show>
          </button>
          <button
            type="button"
            role="tab"
            class="insp-tab"
            classList={{ on: tab() === "clip" }}
            aria-selected={tab() === "clip"}
            onClick={() => setTab("clip")}
          >
            {t("Clip")}
          </button>
        </div>
        <IconButton
          icon="panelRight"
          tip={t("Hide inspector")}
          kbd="Ctrl+2"
          class="sm"
          onClick={() => layout.inspectorOpen.set(false)}
        />
      </header>
      <div class="insp-scroll">
        <Show when={tab() === "selection"} fallback={<ClipPanel />}>
          <SelectionPanel />
        </Show>
      </div>
    </aside>
  );
}

// ── selection ────────────────────────────────────────────────────────────────

function PanelTitle(props: { icon: IconName; title: string; sub?: string; actions?: JSX.Element }) {
  return (
    <div class="insp-title">
      <span class="insp-title-icon">
        <Icon name={props.icon} size={16} />
      </span>
      <div class="insp-title-text">
        <strong>{props.title}</strong>
        <Show when={props.sub}>
          <small>{props.sub}</small>
        </Show>
      </div>
      <Show when={props.actions}>
        <div class="insp-title-actions">{props.actions}</div>
      </Show>
    </div>
  );
}

/** Quick picks for the re-drawn pointer's color; the first is the classic white. */
const POINTER_COLORS = ["#ffffff", "#0e0e0f", "#e5484d", "#ffd23f", "#30a46c", "#6ea8ff"];
/** The pointer's looks, in the order of their cards. */
const POINTER_LOOKS = (): { value: PointerLook; label: string; tip: string }[] => [
  { value: "classic", label: t("Classic"), tip: t("Vuoom's pointer: an arrow, a hand over links, a beam over text") },
  { value: "dot", label: t("Dot"), tip: t("A round dot, like a presenter's laser") },
  { value: "image", label: t("Picture"), tip: t("Your own picture as the pointer") },
];
/** The click effects, in the order of their cards. */
const CLICK_EFFECTS = (): { value: ClickEffect; label: string; tip: string }[] => [
  { value: "ripple", label: t("Ripple"), tip: t("One ring that grows and fades") },
  { value: "rings", label: t("Rings"), tip: t("Two rings, one after the other") },
  { value: "pulse", label: t("Pulse"), tip: t("A soft disc that swells and fades") },
  { value: "burst", label: t("Burst"), tip: t("Dots that fly outward") },
];
/** Click colors: white (the default), then the pointer's palette without its black. */
const CLICK_COLORS = ["#ffffff", "#ffd23f", "#e5484d", "#30a46c", "#6ea8ff", "#b48cff"];

/** A click effect for its card: still at rest, playing while the card is hovered and
 *  twice when it is chosen (see .fx-art in inspector.css). */
function ClickEffectArt(props: { effect: ClickEffect }): JSX.Element {
  const dots = [0, 1, 2, 3, 4, 5, 6, 7].map((i) => {
    const a = (i * Math.PI) / 4;
    return { x: 12 + 8 * Math.cos(a), y: 12 + 8 * Math.sin(a) };
  });
  return (
    <svg
      class="fx-art"
      viewBox="0 0 24 24"
      width="26"
      height="26"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      aria-hidden="true"
    >
      <Show when={props.effect === "ripple"}>
        <circle class="fx-a" cx="12" cy="12" r="7.5" />
      </Show>
      <Show when={props.effect === "rings"}>
        <circle class="fx-a" cx="12" cy="12" r="9" />
        <circle class="fx-b" cx="12" cy="12" r="9" />
      </Show>
      <Show when={props.effect === "pulse"}>
        <circle class="fx-a" cx="12" cy="12" r="8" fill="currentColor" fill-opacity="0.32" />
      </Show>
      <Show when={props.effect === "burst"}>
        <g class="fx-a">
          <For each={dots}>{(d) => <circle cx={d.x} cy={d.y} r="1.5" fill="currentColor" stroke="none" />}</For>
        </g>
      </Show>
    </svg>
  );
}
const pointerHex = (c: [number, number, number] | null | undefined) =>
  c ? rgbHex({ r: c[0], g: c[1], b: c[2], a: 1 }) : POINTER_COLORS[0];
/** The look a pointer is drawn with: a picture look without a picture is the classic one. */
const pointerLook = (c: { look?: PointerLook; image?: unknown }): PointerLook =>
  c.look === "image" && !c.image ? "classic" : (c.look ?? "classic");

/** The roundest a spotlight's corners go (a fraction of the height). Mirrors
 *  vuoom_project::HighlightBox::MAX_RADIUS. */
const SPOTLIGHT_MAX_RADIUS = 0.25;

const KIND_ICON: Record<string, IconName> = {
  Text: "text",
  Arrow: "arrow",
  Line: "line",
  Box: "shape",
  Ellipse: "shape",
  Highlight: "highlight",
  "Hidden area": "eyeOff",
  Spotlight: "spotlight",
  Pen: "pen",
  Marker: "pen",
};

function SelectionPanel() {
  const ed = useEditor();
  return (
    <>
      <Show when={ed.selected()}>
        <Show when={ed.selCount() > 1} fallback={<AnnotationProps />}>
          <PanelTitle
            icon="layers"
            title={t("{count} annotations", { count: ed.selCount() })}
            sub={t("Drag any one on the canvas to move them together")}
          />
          <div class="insp-pad">
            <button type="button" class="btn danger block" onClick={() => void ed.deleteSelection()}>
              <Icon name="trash" size={14} /> {t("Delete {count} annotations", { count: ed.selCount() })}
            </button>
          </div>
        </Show>
      </Show>
      <Show when={ed.selZoom() !== null && ed.selectedZoom()}>
        <ZoomProps />
      </Show>
      <Show when={ed.selSpeed() !== null && ed.selectedSpeed()}>
        <SpeedProps />
      </Show>
      <Show when={ed.selCut() !== null && ed.selectedCut()}>
        <CutProps />
      </Show>
      <Show when={ed.selectedCaption()}>
        <CaptionProps />
      </Show>
      <Show when={ed.drawingToolActive() && !ed.somethingSelected()}>
        <ToolCard />
      </Show>
      <Show when={!ed.somethingSelected() && !ed.drawingToolActive()}>
        <div class="insp-empty">
          <span class="insp-empty-icon">
            <Icon name="cursor" size={20} />
          </span>
          <strong>{t("Nothing selected")}</strong>
          <p>{t("Click something on the video or the timeline to edit it, or pick a tool to draw.")}</p>
          <div class="insp-empty-keys">
            <span>
              <kbd>Z</kbd> {t("Zoom")}
            </span>
            <span>
              <kbd>C</kbd> {t("Cut")}
            </span>
            <span>
              <kbd>T</kbd> {t("Text")}
            </span>
            <span>
              <kbd>A</kbd> {t("Arrow")}
            </span>
          </div>
        </div>
      </Show>
    </>
  );
}

function ToolCard() {
  const ed = useEditor();
  const tool = () => TOOLS().find((x) => x.id === ed.tool());
  return (
    <Show when={ed.tool() !== "pen"} fallback={<PenCard />}>
      <PanelTitle icon="wand" title={t("{label} tool", { label: tool()?.label ?? "" })} sub={tool()?.hint} />
      <div class="insp-pad">
        <Show when={ed.tool() === "shape"}>
          <Field label={t("Draw")}>
            <Seg
              value={ed.shapeEllipse() ? "ellipse" : "box"}
              onChange={(v) => ed.setShapeEllipse(v === "ellipse")}
              options={[
                { value: "box", label: t("Box") },
                { value: "ellipse", label: t("Ellipse") },
              ]}
            />
          </Field>
        </Show>
        <p class="note">
          <Rich
            text={t("Options appear here once you place one. Hold {shift} for clean angles and squares, {esc} returns to Select.")}
            parts={{ shift: () => <kbd>Shift</kbd>, esc: () => <kbd>Esc</kbd> }}
          />
        </p>
        <Field label={t("Draw several")} hint={t("Keep this tool after each one instead of returning to Select")}>
          <Switch checked={ed.toolLock()} label={t("Draw several")} onChange={(v) => ed.setToolLock(v)} />
        </Field>
      </div>
    </Show>
  );
}

// The Pen tool's card: how the next line looks. The pen stays in hand until Esc or V.
function PenCard() {
  const ed = useEditor();
  const look = () => ed.penLook();
  const pick = (marker: boolean) => ed.setPenLook({ marker, ...(marker ? PEN_LOOKS.marker : PEN_LOOKS.pen) });
  return (
    <>
      <PanelTitle icon="pen" title={t("Pen")} sub={t("Drag on the video to draw")} />
      <div class="insp-pad">
        <Field label={t("Draw with")}>
          <Seg
            value={look().marker ? "marker" : "pen"}
            onChange={(v) => pick(v === "marker")}
            options={[
              { value: "pen", label: t("Pen"), tip: t("A fine solid line") },
              { value: "marker", label: t("Marker"), tip: t("A wide see-through stroke, for marking up text") },
            ]}
          />
        </Field>
        <Field label={t("Width")}>
          <ScrubField
            value={look().width}
            min={0.002}
            max={0.05}
            step={0.001}
            displayScale={100}
            suffix="%"
            title={t("Line width as a percent of height")}
            onInput={(v) => ed.setPenLook({ ...look(), width: v })}
            onCommit={(v) => ed.setPenLook({ ...look(), width: v })}
          />
        </Field>
        <Field label={t("Color")} stack>
          <div class="color-row">
          <For each={PRESET_COLORS}>
            {(c) => (
              <button
                type="button"
                class="swatch"
                classList={{ on: rgbHex(look().color) === c }}
                style={{ background: c }}
                aria-label={t("Color {color}", { color: c })}
                data-tip={c}
                onClick={() => ed.setPenLook({ ...look(), color: { ...hexRgb(c), a: look().color.a } })}
              />
            )}
          </For>
          </div>
        </Field>
        <p class="note">
          <Rich
            text={t("Draw as many lines as you like. {esc} puts the pen down, then click a line to move or change it.")}
            parts={{ esc: () => <kbd>Esc</kbd> }}
          />
        </p>
      </div>
    </>
  );
}

function ColorRow() {
  const ed = useEditor();
  return (
    <div class="color-row">
      <For each={PRESET_COLORS}>
        {(c) => (
          <button
            type="button"
            class="swatch"
            classList={{ on: rgbHex(ed.selectedColor()!) === c }}
            style={{ background: c }}
            aria-label={t("Color {color}", { color: c })}
            data-tip={c}
            onClick={() => ed.setColor(c)}
          />
        )}
      </For>
      <label class="swatch swatch-custom" data-tip={t("Custom color")}>
        <input
          type="color"
          value={rgbHex(ed.selectedColor()!)}
          aria-label={t("Custom color")}
          onInput={(e) => ed.setColor(e.currentTarget.value)}
        />
      </label>
    </div>
  );
}

function TimingSection(props: { id: string }) {
  const ed = useEditor();
  const r = () => ed.selectedRange()!;
  return (
    <Section id={`${props.id}-timing`} title={t("Timing")} icon="timer">
      <Field label={t("Appears")}>
        <ScrubField
          value={Number(r().start.toFixed(2))}
          min={0}
          max={ed.duration()}
          step={0.05}
          suffix="s"
          title={t("When this appears. Drag to scrub, click to type")}
          onCommit={(v) => ed.editRange(v, r().end)}
        />
      </Field>
      <Field label={t("Disappears")}>
        <ScrubField
          value={Number(r().end.toFixed(2))}
          min={0}
          max={ed.duration()}
          step={0.05}
          suffix="s"
          title={t("When this disappears. Drag to scrub, click to type")}
          onCommit={(v) => ed.editRange(r().start, v)}
        />
      </Field>
      <Field label={t("On screen")}>
        <span class="readout">{(r().end - r().start).toFixed(2)}s</span>
      </Field>
      <Field label={t("Fade in")} hint={t("0 pops in instantly")}>
        <ScrubField
          value={Number(r().fade_in.toFixed(2))}
          min={0}
          max={2}
          step={0.05}
          suffix="s"
          title={t("Fade-in length. Drag to scrub, click to type")}
          onInput={(v) => ed.editFades(v, r().fade_out)}
          onCommit={(v) => ed.editFades(v, r().fade_out)}
        />
      </Field>
      <Field label={t("Fade out")} hint={t("0 disappears instantly")}>
        <ScrubField
          value={Number(r().fade_out.toFixed(2))}
          min={0}
          max={2}
          step={0.05}
          suffix="s"
          title={t("Fade-out length. Drag to scrub, click to type")}
          onInput={(v) => ed.editFades(r().fade_in, v)}
          onCommit={(v) => ed.editFades(r().fade_in, v)}
        />
      </Field>
      <div class="btn-row">
        <button type="button" class="btn sm" data-tip={t("Start at the playhead")} onClick={() => {
          const len = r().end - r().start;
          ed.editRange(ed.playhead(), Math.min(ed.duration(), ed.playhead() + len));
        }}>
          {t("Move to playhead")}
        </button>
        <button type="button" class="btn sm" data-tip={t("Show until the end of the clip")} onClick={() => ed.editRange(r().start, ed.duration())}>
          {t("Until end")}
        </button>
      </div>
      <Show when={ed.isGhost(r(), true)}>
        <p class="note">
          {t("Hidden at the playhead (shown dimmed so you can edit it). It is on screen from {start} to {end}.", {
            start: fmt(r().start),
            end: fmt(r().end),
          })}
        </p>
      </Show>
    </Section>
  );
}

function ArrangeSection() {
  const ed = useEditor();
  return (
    <Section id="ann-arrange" title={t("Arrange")} icon="layers" defaultOpen={false}>
      <div class="btn-grid">
        <button type="button" class="btn sm" data-kbd="Ctrl+]" data-tip={t("Bring forward")} onClick={() => void ed.reorderSelected("forward")}>
          <Icon name="bringForward" size={14} /> {t("Forward")}
        </button>
        <button type="button" class="btn sm" data-kbd="Ctrl+[" data-tip={t("Send backward")} onClick={() => void ed.reorderSelected("backward")}>
          <Icon name="sendBackward" size={14} /> {t("Backward")}
        </button>
        <button type="button" class="btn sm" data-kbd="Ctrl+Shift+]" data-tip={t("Bring to front")} onClick={() => void ed.reorderSelected("front")}>
          {t("To front")}
        </button>
        <button type="button" class="btn sm" data-kbd="Ctrl+Shift+[" data-tip={t("Send to back")} onClick={() => void ed.reorderSelected("back")}>
          {t("To back")}
        </button>
      </div>
      <p class="note">{t("Stacking applies within a kind: boxes sit under pen lines, lines under arrows, arrows under text.")}</p>
    </Section>
  );
}

function AnnotationProps() {
  const ed = useEditor();
  const title = () => ed.inspTitle();
  const actions = (
    <>
      <IconButton icon="duplicate" tip={t("Duplicate")} kbd="Ctrl+D" class="sm" onClick={() => void ed.duplicateSelected()} />
      <IconButton icon="copy" tip={t("Copy")} kbd="Ctrl+C" class="sm" onClick={() => void ed.copySelected()} />
      <IconButton icon="trash" tip={t("Delete")} kbd="Del" class="sm danger" onClick={() => void ed.deleteSelection()} />
    </>
  );
  return (
    <>
      <PanelTitle
        icon={KIND_ICON[title()] ?? "shape"}
        title={title()}
        sub={t("Drag to move, handles to resize")}
        actions={actions}
      />

      <Show when={ed.selectedText()}>
        {(ann) => (
          <>
            <Section id="ann-text" title={t("Text")} icon="text">
              <textarea
                class="input text-field"
                rows={Math.min(4, Math.max(1, ann().text.split("\n").length))}
                spellcheck={false}
                placeholder={t("Label text")}
                aria-label={t("Label text")}
                data-tip={t("Shift+Enter starts a new line. Double-click the label to type on the video")}
                ref={ed.refs.contentInput}
                onInput={(e) => ed.editText(e.currentTarget.value)}
                onKeyDown={(e) => {
                  // Enter alone leaves the field; Shift+Enter is a new line.
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    e.currentTarget.blur();
                  }
                }}
              />
              <Field label={t("Size")}>
                <ScrubField
                  value={ann().font_size}
                  min={0.02}
                  max={0.2}
                  step={0.005}
                  displayScale={100}
                  suffix="%"
                  title={t("Font size (percent of height). Drag to scrub, click to type")}
                  onInput={(v) => ed.editFontSize(v)}
                  onCommit={(v) => ed.editFontSize(v)}
                />
              </Field>
              <Field label={t("Style")}>
                <div class="toggle-group">
                  <button type="button" class="tgl" classList={{ on: ann().bold }} data-tip={t("Bold")} aria-pressed={ann().bold} onClick={() => ed.editTextStyle({ bold: !ann().bold })}>
                    <b>B</b>
                  </button>
                  <button type="button" class="tgl" classList={{ on: ann().italic }} data-tip={t("Italic")} aria-pressed={ann().italic} onClick={() => ed.editTextStyle({ italic: !ann().italic })}>
                    <i>I</i>
                  </button>
                  <button type="button" class="tgl wide" classList={{ on: ann().background }} data-tip={t("Legible plate behind the text")} aria-pressed={ann().background} onClick={() => ed.editTextStyle({ background: !ann().background })}>
                    {t("Plate")}
                  </button>
                </div>
              </Field>
            </Section>
            <Section id="ann-font" title={t("Typeface")} icon="text">
              <div class="font-grid">
                <For each={TEXT_FONTS()}>
                  {(f) => (
                    <button
                      type="button"
                      class="font-card"
                      classList={{ on: (ann().font || "") === f.id }}
                      style={{ "font-family": f.css }}
                      onClick={() => ed.editTextStyle({ font: f.id })}
                    >
                      <span class="font-sample">Aa</span>
                      <span class="font-name">{f.label}</span>
                    </button>
                  )}
                </For>
              </div>
            </Section>
          </>
        )}
      </Show>

      <Show when={ed.isSpotlight()}>
        <Section id="ann-spotlight" title={t("Spotlight")} icon="spotlight">
          <Field label={t("Corners")} hint={t("Round the lit area's corners. All the way up, its short ends become half circles")}>
            <Slider
              value={ed.selectedBox()?.radius ?? 0}
              min={0}
              max={SPOTLIGHT_MAX_RADIUS}
              step={0.005}
              label={t("Corner radius")}
              format={(v) => (v === 0 ? t("Square") : `${Math.round(v * 100)}%`)}
              onInput={(v) => ed.editStyle({ radius: v })}
            />
          </Field>
          <p class="note">
            {t("Everything around this area goes dark, so eyes go straight to it. Opacity sets how dark; drag its handles to reframe it.")}
          </p>
        </Section>
      </Show>

      <Show when={ed.selectedBox() && !ed.isMask() && !ed.isSpotlight()}>
        <Section id="ann-shape" title={t("Shape")} icon="shape">
          <Field label={t("Shape")}>
            <Seg
              value={ed.selectedBox()!.shape === "Ellipse" ? "ellipse" : "rect"}
              onChange={(v) => ed.setShape(v === "ellipse")}
              options={[
                { value: "rect", label: t("Rectangle") },
                { value: "ellipse", label: t("Ellipse") },
              ]}
            />
          </Field>
          <Field label={t("Fill")}>
            <Seg
              value={ed.selectedBox()!.filled ? "filled" : "outline"}
              onChange={(v) => ed.editStyle({ filled: v === "filled" })}
              options={[
                { value: "outline", label: t("Outline") },
                { value: "filled", label: t("Filled") },
              ]}
            />
          </Field>
          <Show when={!ed.selectedBox()!.filled}>
            <Field label={t("Stroke")}>
              <ScrubField
                value={ed.selectedBox()!.thickness}
                min={0.002}
                max={0.02}
                step={0.001}
                displayScale={100}
                suffix="%"
                title={t("Outline thickness as a percent of height")}
                onInput={(v) => ed.editStyle({ thickness: v })}
                onCommit={(v) => ed.editStyle({ thickness: v })}
              />
            </Field>
          </Show>
        </Section>
      </Show>

      <Show when={ed.isMask()}>
        <Section id="ann-mask" title={t("Hide")} icon="eyeOff">
          <Field label={t("Cover")}>
            <Seg
              value={ed.selectedBox()!.pixelate ? "pixelate" : "black"}
              onChange={(v) => ed.setMaskPixelate(v === "pixelate")}
              options={[
                { value: "black", label: t("Black"), tip: t("Solid black. Nothing of what's under it remains") },
                { value: "pixelate", label: t("Pixelate"), tip: t("Large blocks. Softer on the eye; use Black for passwords") },
              ]}
            />
          </Field>
          <p class="note">
            {t("Covers the same part of the recording through every zoom, and never fades. Drag its bar on the timeline to choose when it covers the frame.")}
          </p>
        </Section>
      </Show>

      <Show when={ed.selectedArrow()}>
        {(a) => (
          <Section id="ann-arrow" title={t("Stroke")} icon="arrow">
            <Field label={t("Ends")}>
              <Seg
                value={a().style === "Line" ? "line" : a().style === "DoubleArrow" ? "double" : "arrow"}
                onChange={(v) => ed.setArrowStyle(v)}
                options={[
                  { value: "arrow", label: t("Arrow") },
                  { value: "double", label: t("Double") },
                  { value: "line", label: t("Line") },
                ]}
              />
            </Field>
            <Field label={t("Weight")}>
              <ScrubField
                value={a().thickness}
                min={0.002}
                max={0.05}
                step={0.001}
                displayScale={100}
                suffix="%"
                title={t("Stroke thickness as a percent of height")}
                onInput={(v) => ed.editStyle({ thickness: v })}
                onCommit={(v) => ed.editStyle({ thickness: v })}
              />
            </Field>
            <Field label={t("Curve")} hint={t("Bow the arrow to either side, to swing around something")}>
              <ScrubField
                value={a().curve ?? 0}
                min={-0.6}
                max={0.6}
                step={0.01}
                displayScale={100}
                suffix="%"
                title={t("How far the middle bows off the straight line")}
                onInput={(v) => ed.editStyle({ curve: v })}
                onCommit={(v) => ed.editStyle({ curve: v })}
              />
            </Field>
          </Section>
        )}
      </Show>

      <Show when={ed.selectedStroke()}>
        {(st) => (
          <Section id="ann-stroke" title={t("Line")} icon="pen">
            <Field label={t("Look")}>
              <Seg
                value={isMarker(st()) ? "marker" : "pen"}
                onChange={(v) => ed.setStrokeLook(v === "marker")}
                options={[
                  { value: "pen", label: t("Pen") },
                  { value: "marker", label: t("Marker") },
                ]}
              />
            </Field>
            <Field label={t("Width")}>
              <ScrubField
                value={st().thickness}
                min={0.002}
                max={0.05}
                step={0.001}
                displayScale={100}
                suffix="%"
                title={t("Line width as a percent of height")}
                onInput={(v) => ed.editStyle({ thickness: v })}
                onCommit={(v) => ed.editStyle({ thickness: v })}
              />
            </Field>
          </Section>
        )}
      </Show>

      <Show when={ed.selectedColor() && !ed.isMask()}>
        <Section id="ann-color" title={t("Color")} icon="palette">
          <ColorRow />
          <Field label={t("Opacity")}>
            <Slider
              value={ed.selectedColor()!.a ?? 1}
              min={0.1}
              max={1}
              step={0.05}
              label={t("Opacity")}
              format={(v) => `${Math.round(v * 100)}%`}
              onInput={(v) => ed.setOpacity(v)}
            />
          </Field>
        </Section>
      </Show>

      <Show when={ed.selectedRange()}>
        <TimingSection id="ann" />
      </Show>
      <ArrangeSection />
    </>
  );
}

function ZoomProps() {
  const ed = useEditor();
  const z = () => ed.selectedZoom()!;
  const i = () => ed.selZoom()!;
  return (
    <>
      <PanelTitle
        icon="zoomIn"
        title={t("Zoom")}
        sub={t("{amount}× from {start}s to {end}s", { amount: z().amount.toFixed(1), start: z().start.toFixed(1), end: z().end.toFixed(1) })}
        actions={<IconButton icon="trash" tip={t("Delete zoom")} kbd="Del" class="sm danger" onClick={() => void ed.deleteSelectedZoom()} />}
      />
      <Section id="zoom-aim" title={t("Strength and aim")} icon="zoomIn">
        <Field label={t("Strength")} stack>
          <Slider
            value={z().amount}
            min={1.2}
            max={4}
            step={0.1}
            label={t("Zoom strength")}
            format={(v) => `${v.toFixed(1)}×`}
            onInput={(v) => void ed.applyZoomEdit(i(), z().start, z().end, v)}
          />
        </Field>
        <Field label={t("Aim")} stack>
          <Seg
            full
            value={ed.selZoomFocus() ? "fixed" : "follow"}
            onChange={(v) => void ed.applyZoomFocus(v === "fixed" ? (ed.selZoomFocus() ?? { x: 0.5, y: 0.5 }) : null)}
            options={[
              { value: "follow", label: t("Follow pointer"), tip: t("The zoom follows your recorded mouse pointer") },
              { value: "fixed", label: t("Fixed point"), tip: t("Hold one spot. Drag the crosshair to aim") },
            ]}
          />
        </Field>
        <Show when={ed.selZoomFocus()}>
          <p class="note">{t("Drag the crosshair on the video to aim this zoom.")}</p>
        </Show>
        <Field label={t("Motion")} stack>
          <Seg
            full
            value={z().style}
            onChange={(v) => void ed.applyZoomStyle(v)}
            options={[
              { value: "Smooth", label: t("Smooth"), tip: t("Cinematic glide (default)") },
              { value: "Snappy", label: t("Snappy"), tip: t("Settles faster") },
              { value: "Slow", label: t("Slow"), tip: t("Gentle drift") },
            ]}
          />
        </Field>
      </Section>
      <Section id="zoom-timing" title={t("Timing")} icon="timer">
        <Field label={t("Start")}>
          <ScrubField
            value={Number(z().start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("When this zoom starts")}
            onCommit={(v) => void ed.applyZoomEdit(i(), v, z().end, z().amount)}
          />
        </Field>
        <Field label={t("End")}>
          <ScrubField
            value={Number(z().end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("When this zoom ends")}
            onCommit={(v) => void ed.applyZoomEdit(i(), z().start, v, z().amount)}
          />
        </Field>
        <Field label={t("Length")}>
          <span class="readout">{(z().end - z().start).toFixed(2)}s</span>
        </Field>
      </Section>
    </>
  );
}

const SPEEDS = [1.5, 2, 3, 4, 6, 8];

function SpeedProps() {
  const ed = useEditor();
  const r = () => ed.selectedSpeed()!;
  const i = () => ed.selSpeed()!;
  return (
    <>
      <PanelTitle
        icon="speed"
        title={t("Speed up")}
        sub={t("{length}s plays in {played}s", { length: (r().end - r().start).toFixed(1), played: ((r().end - r().start) / r().factor).toFixed(1) })}
        actions={<IconButton icon="trash" tip={t("Delete speed region")} kbd="Del" class="sm danger" onClick={() => void ed.deleteSelectedSpeed()} />}
      />
      <Section id="speed-rate" title={t("Rate")} icon="speed">
        <Field label={t("Rate")} stack>
          <Slider
            value={r().factor}
            min={0.25}
            max={8}
            step={0.25}
            label={t("Playback rate")}
            format={(v) => `${v}×`}
            onInput={(v) => void ed.applySpeedEdit(i(), r().start, r().end, v)}
          />
        </Field>
        <div class="chip-row">
          <For each={SPEEDS}>
            {(f) => (
              <button
                type="button"
                class="chip"
                aria-pressed={r().factor === f}
                onClick={() => void ed.applySpeedEdit(i(), r().start, r().end, f)}
              >
                {f}×
              </button>
            )}
          </For>
        </div>
      </Section>
      <Section id="speed-timing" title={t("Timing")} icon="timer">
        <Field label={t("Start")}>
          <ScrubField
            value={Number(r().start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("When this speed region starts")}
            onCommit={(v) => void ed.applySpeedEdit(i(), v, r().end, r().factor)}
          />
        </Field>
        <Field label={t("End")}>
          <ScrubField
            value={Number(r().end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("When this speed region ends")}
            onCommit={(v) => void ed.applySpeedEdit(i(), r().start, v, r().factor)}
          />
        </Field>
      </Section>
    </>
  );
}

function CutProps() {
  const ed = useEditor();
  const c = () => ed.selectedCut()!;
  const i = () => ed.selCut()!;
  return (
    <>
      <PanelTitle icon="cut" title={t("Cut")} sub={t("{length}s removed from the export", { length: (c().end - c().start).toFixed(2) })} />
      <Section id="cut-timing" title={t("Removed span")} icon="timer">
        <Field label={t("From")}>
          <ScrubField
            value={Number(c().start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("Where the removed section starts")}
            onCommit={(v) => void ed.applyCutEdit(i(), v, c().end)}
          />
        </Field>
        <Field label={t("To")}>
          <ScrubField
            value={Number(c().end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("Where the removed section ends")}
            onCommit={(v) => void ed.applyCutEdit(i(), c().start, v)}
          />
        </Field>
        <p class="note">{t("Playback and export jump straight over this span.")}</p>
        <button type="button" class="btn block" onClick={() => void ed.deleteSelectedCut()}>
          <Icon name="reset" size={14} /> {t("Restore this section")}
        </button>
      </Section>
    </>
  );
}

function CaptionProps() {
  const ed = useEditor();
  const c = () => ed.selectedCaption()!;
  const setStart = (v: number) => void ed.captions.setRange(c().id, v, Math.max(c().range.end, v + 0.2));
  const setEnd = (v: number) => void ed.captions.setRange(c().id, Math.min(c().range.start, v - 0.2), v);
  return (
    <>
      <PanelTitle
        icon="captions"
        title={t("Caption")}
        sub={t("On screen for {length}s", { length: (c().range.end - c().range.start).toFixed(1) })}
        actions={
          <IconButton
            icon="trash"
            tip={t("Delete caption")}
            kbd="Del"
            class="sm danger"
            onClick={() => void ed.deleteSelectedCaption()}
          />
        }
      />
      <Section id="caption-words" title={t("Words")} icon="text">
        <textarea
          class="input cap-text"
          rows={3}
          spellcheck={true}
          placeholder={t("Type what was said")}
          aria-label={t("Caption words")}
          value={c().text}
          onInput={(e) => ed.captions.setText(c().id, e.currentTarget.value)}
        />
        <p class="note">{t("Two short lines read best. Drag the ends of the bar on the timeline to retime it.")}</p>
      </Section>
      <Section id="caption-timing" title={t("Timing")} icon="timer">
        <Field label={t("Shows at")}>
          <ScrubField
            value={Number(c().range.start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("When this caption appears")}
            onCommit={setStart}
          />
        </Field>
        <Field label={t("Hides at")}>
          <ScrubField
            value={Number(c().range.end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title={t("When this caption goes away")}
            onCommit={setEnd}
          />
        </Field>
      </Section>
    </>
  );
}

// ── clip ─────────────────────────────────────────────────────────────────────

const FRAMES = (): { id: string; label: string; tip: string }[] => [
  { id: "none", label: t("None"), tip: t("Edge to edge, exactly what you recorded") },
  { id: "subtle", label: t("Subtle"), tip: t("A little padding, soft corners and shadow") },
  { id: "studio", label: t("Studio"), tip: t("Generous padding on a backdrop, product-shot style") },
];

// Mirrors the values session::set_frame_preset writes for each preset.
const FRAME_VALUES: Record<string, { padding: number; radius: number; shadow: number }> = {
  none: { padding: 0, radius: 0, shadow: 0 },
  subtle: { padding: 0.04, radius: 0.012, shadow: 0.3 },
  studio: { padding: 0.075, radius: 0.02, shadow: 0.5 },
};
const bgHex = (c: [number, number, number]) => rgbHex({ r: c[0], g: c[1], b: c[2], a: 1 });

const CROPS = (): { id: string; label: string; ratio: number | null }[] => [
  { id: "full", label: t("Full"), ratio: null },
  { id: "16:9", label: "16:9", ratio: 16 / 9 },
  { id: "4:3", label: "4:3", ratio: 4 / 3 },
  { id: "1:1", label: "1:1", ratio: 1 },
  { id: "4:5", label: "4:5", ratio: 4 / 5 },
  { id: "9:16", label: "9:16", ratio: 9 / 16 },
];

const SKIMS = [2, 3, 4, 6, 8];

function ClipPanel() {
  const ed = useEditor();
  const cropMatches = (c: CropRect | null, ratio: number | null) => {
    if (!ratio) return !c;
    if (!c) return false;
    const want = ed.centeredCrop(ratio);
    return Math.abs(c.w - want.w) < 0.002 && Math.abs(c.h - want.h) < 0.002;
  };
  const outDur = () => outputDuration(ed.duration(), ed.trim(), ed.speed(), ed.cuts());
  // A preset card is lit only when the frame matches that preset exactly; any slider
  // tweak turns the frame "Custom".
  const framePresetExact = () => {
    const fi = ed.frameInfo();
    if (!fi) return ed.framePreset();
    const near = (a: number, b: number) => Math.abs(a - b) < 1e-3;
    for (const [id, v] of Object.entries(FRAME_VALUES)) {
      if (near(fi.padding, v.padding) && near(fi.corner_radius, v.radius) && near(fi.shadow, v.shadow)) return id;
    }
    return "";
  };
  const isCustomFrame = () => framePresetExact() === "";
  const isPicture = () => ed.frameInfo()?.bg_kind === "image";
  // The current backdrop as CSS, for the frame cards' little scenes.
  const backdropCss = () => {
    const fi = ed.frameInfo();
    const sw = ed.BG_SWATCHES().find((s) => s.name === ed.bgPreset());
    if (sw) return sw.css;
    if (!fi || fi.bg_kind === "image") return "linear-gradient(135deg,#3b4a5c,#171c24)";
    if (fi.bg_kind === "solid") return bgHex(fi.bg_from);
    return `linear-gradient(${fi.bg_angle + 90}deg,${bgHex(fi.bg_from)},${bgHex(fi.bg_to)})`;
  };
  const frameName = () => FRAMES().find((f) => f.id === framePresetExact())?.label ?? t("Custom");
  const cropName = () => CROPS().find((c) => cropMatches(ed.crop(), c.ratio))?.label ?? t("Custom");
  const pictureName = () => (ed.frameInfo()?.bg_image ?? "").split(/[\\/]/).pop() ?? "";

  // Settings search: filters the sections below as you type. Ctrl+F jumps here.
  let searchEl: HTMLInputElement | undefined;
  const [query, setQuery] = createSignal("");
  const [noMatch, setNoMatch] = createSignal(false);
  const search = (q: string) => {
    setQuery(q);
    setSectionQuery(q);
    requestAnimationFrame(() => {
      const shown = document.querySelectorAll(".insp-scroll .sect[data-sect^='clip-']:not(.sect-miss)");
      setNoMatch(!!sectionQuery() && shown.length === 0);
    });
  };
  const onFind = (e: KeyboardEvent) => {
    if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyF" && searchEl && !ed.isModal()) {
      e.preventDefault();
      searchEl.focus();
      searchEl.select();
    }
  };
  // In the capture phase, next to the editor's own key guard: that guard swallows Ctrl+F as
  // a browser shortcut and stops it there, so a listener later in the path never hears it.
  onMount(() => window.addEventListener("keydown", onFind, true));
  onCleanup(() => {
    window.removeEventListener("keydown", onFind, true);
    setSectionQuery("");
  });

  return (
    <>
      <div class="clip-search">
        <div class="input-wrap">
          <Icon name="search" size={14} />
          <input
            ref={searchEl}
            class="input"
            type="search"
            placeholder={t("Search settings")}
            aria-label={t("Search clip settings")}
            spellcheck={false}
            value={query()}
            onInput={(e) => search(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && query()) {
                e.preventDefault();
                e.stopPropagation();
                search("");
              }
            }}
          />
          <Show when={query()}>
            <button type="button" class="clip-search-x" aria-label={t("Clear search")} onClick={() => search("")}>
              <Icon name="close" size={12} stroke={2.2} />
            </button>
          </Show>
        </div>
      </div>
      <Show when={noMatch()}>
        <div class="clip-search-empty">
          <Icon name="search" size={18} />
          <span>{t('Nothing matches "{query}".', { query: query() })}</span>
          <button type="button" class="btn sm ghost" onClick={() => search("")}>
            {t("Clear search")}
          </button>
        </div>
      </Show>

      <Section
        id="clip-frame"
        title={t("Frame")}
        icon="frame"
        defaultOpen={false}
        keywords={t("padding corners rounded radius shadow backdrop background wallpaper picture image photo color gradient border")}
        aside={<span class="badge">{isCustomFrame() ? t("Custom") : frameName()}</span>}
      >
        <div class="frame-cards" style={{ "--bd": backdropCss() }}>
          <For each={FRAMES()}>
            {(f) => (
              <button
                type="button"
                class="frame-card"
                classList={{ on: framePresetExact() === f.id, [f.id]: true }}
                aria-pressed={framePresetExact() === f.id}
                data-tip={f.tip}
                onClick={() => ed.applyFramePreset(f.id)}
              >
                <span class="frame-card-art">
                  <span class="frame-card-shot">
                    <i />
                  </span>
                </span>
                <span class="frame-card-label">{f.label}</span>
              </button>
            )}
          </For>
        </div>
        <Show when={ed.frameInfo()}>
          {(fi) => (
            <>
              <Field label={t("Padding")}>
                <Slider
                  value={fi().padding}
                  min={0}
                  max={0.2}
                  step={0.005}
                  label={t("Padding")}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={(v) => ed.applyFrameStyle({ padding: v })}
                />
              </Field>
              <Field label={t("Corners")}>
                <Slider
                  value={fi().corner_radius}
                  min={0}
                  max={0.08}
                  step={0.002}
                  label={t("Corner radius")}
                  format={(v) => `${(v * 100).toFixed(1)}%`}
                  onInput={(v) => ed.applyFrameStyle({ radius: v })}
                />
              </Field>
              <Field label={t("Shadow")}>
                <Slider
                  value={fi().shadow}
                  min={0}
                  max={1}
                  step={0.05}
                  label={t("Shadow strength")}
                  disabled={fi().padding <= 0}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={(v) => ed.applyFrameStyle({ shadow: v })}
                />
              </Field>
            </>
          )}
        </Show>
        <Show when={ed.framePreset() !== "none"}>
          <Field label={t("Backdrop")} stack>
            <div class="swatches">
              <For each={ed.BG_SWATCHES()}>
                {(sw) => (
                  <button
                    type="button"
                    class="swatch"
                    style={{ background: sw.css }}
                    aria-label={t("Backdrop: {label}", { label: sw.label })}
                    aria-pressed={ed.bgPreset() === sw.name}
                    data-tip={sw.label}
                    onClick={() => ed.applyBackground(sw.name)}
                  />
                )}
              </For>
              <button
                type="button"
                class="swatch swatch-custom"
                aria-label={t("Custom backdrop")}
                aria-pressed={ed.bgPreset() === "" && !isPicture()}
                data-tip={t("Your own colors")}
                onClick={() => {
                  const fi = ed.frameInfo();
                  if (fi && (ed.bgPreset() !== "" || isPicture()))
                    ed.applyBackgroundCustom(bgHex(fi.bg_from), fi.bg_kind === "gradient" ? bgHex(fi.bg_to) : null, fi.bg_angle);
                }}
              />
              <button
                type="button"
                class="swatch swatch-picture"
                aria-label={t("Picture backdrop")}
                aria-pressed={isPicture()}
                data-tip={t("Your own picture")}
                onClick={() => void ed.chooseBackdropPicture()}
              >
                <Icon name="image" size={14} />
              </button>
            </div>
          </Field>
          <Show when={isPicture()}>
            <div class="bg-picture">
              <span class="bg-picture-icon">
                <Icon name="image" size={16} />
              </span>
              <span class="bg-picture-name" data-tip={ed.frameInfo()?.bg_image ?? ""}>
                {pictureName() || t("Picture")}
              </span>
              <button type="button" class="btn sm" onClick={() => void ed.chooseBackdropPicture()}>
                {t("Change")}
              </button>
            </div>
          </Show>
          <Show when={ed.bgPreset() === "" && !isPicture() && ed.frameInfo()}>
            {(fi) => (
              <div class="bg-custom">
                <Seg
                  full
                  value={fi().bg_kind}
                  onChange={(k) =>
                    ed.applyBackgroundCustom(bgHex(fi().bg_from), k === "gradient" ? bgHex(fi().bg_to) : null, fi().bg_angle)
                  }
                  options={[
                    { value: "solid", label: t("Solid") },
                    { value: "gradient", label: t("Gradient") },
                  ]}
                />
                <div class="bg-colors">
                  <label class="color-well" data-tip={fi().bg_kind === "gradient" ? t("Start color") : t("Color")}>
                    <span style={{ background: bgHex(fi().bg_from) }} />
                    <input
                      type="color"
                      value={bgHex(fi().bg_from)}
                      aria-label={t("Backdrop color")}
                      onInput={(e) =>
                        ed.applyBackgroundCustom(
                          e.currentTarget.value,
                          fi().bg_kind === "gradient" ? bgHex(fi().bg_to) : null,
                          fi().bg_angle,
                        )
                      }
                    />
                  </label>
                  <Show when={fi().bg_kind === "gradient"}>
                    <label class="color-well" data-tip={t("End color")}>
                      <span style={{ background: bgHex(fi().bg_to) }} />
                      <input
                        type="color"
                        value={bgHex(fi().bg_to)}
                        aria-label={t("Backdrop end color")}
                        onInput={(e) => ed.applyBackgroundCustom(bgHex(fi().bg_from), e.currentTarget.value, fi().bg_angle)}
                      />
                    </label>
                    <Slider
                      value={fi().bg_angle}
                      min={0}
                      max={360}
                      step={5}
                      label={t("Gradient angle")}
                      format={(v) => `${Math.round(v)}°`}
                      onInput={(v) => ed.applyBackgroundCustom(bgHex(fi().bg_from), bgHex(fi().bg_to), v)}
                    />
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </Show>
      </Section>

      <Section
        id="clip-crop"
        title={t("Crop")}
        icon="crop"
        defaultOpen={false}
        keywords={t("aspect ratio 16:9 square vertical portrait 9:16 reel shorts tiktok edges")}
        aside={<span class="badge">{cropName()}</span>}
      >
        <div class="crop-grid">
          <For each={CROPS()}>
            {(c) => (
              <button
                type="button"
                class="chip"
                aria-pressed={cropMatches(ed.crop(), c.ratio)}
                onClick={() => void ed.applyCrop(c.ratio ? ed.centeredCrop(c.ratio) : null)}
              >
                {c.label}
              </button>
            )}
          </For>
        </div>
        <button
          type="button"
          class="btn sm block"
          data-tip={t("Drag a crop rectangle on the video")}
          onClick={() => void ed.beginCropEdit()}
        >
          <Icon name="crop" size={14} /> {t("Adjust on canvas…")}
        </button>
        <Show when={ed.crop()} fallback={<p class="note">{t("Centered crops re-frame the whole clip. Annotations keep their on-screen spot.")}</p>}>
          <p class="note">
            {t("Showing {w}% × {h}% of the recording.", {
              w: Math.round(ed.crop()!.w * 100),
              h: Math.round(ed.crop()!.h * 100),
            })}
          </p>
        </Show>
      </Section>

      <Section
        id="clip-zoom"
        title={t("Zooms")}
        icon="zoomIn"
        defaultOpen={false}
        keywords={t("zoom auto camera motion blur strength close magnify")}
        aside={<span class="badge">{ed.zooms().length === 1 ? t("1 zoom") : t("{n} zooms", { n: ed.zooms().length })}</span>}
      >
        <Field label={t("Auto zoom strength")} stack>
          <Slider
            value={ed.zoomStrength()}
            min={1.2}
            max={3}
            step={0.1}
            label={t("Auto zoom strength")}
            format={(v) => `${v.toFixed(1)}×`}
            onInput={(v) => ed.setZoomStrength(v)}
          />
        </Field>
        <div class="btn-row">
          <button
            type="button"
            class="btn sm grow"
            data-tip={t("Rebuild zooms from your recorded clicks. Ctrl+Z restores your own")}
            onClick={() => void ed.planZoomAuto()}
          >
            <Icon name="sparkle" size={14} /> {t("Auto zooms")}
          </button>
          <button type="button" class="btn sm grow" data-tip={t("Add a zoom at the playhead. To aim it, use the Zoom tool (Z)")} onClick={() => void ed.addZoomAt()}>
            <Icon name="plus" size={14} /> {t("Zoom here")}
          </button>
        </div>
        <Field label={t("Motion blur")} hint={t("Softens fast zooms and pans the way a camera's shutter does")}>
          <Switch checked={ed.motionBlur()} label={t("Motion blur")} onChange={() => ed.toggleMotionBlur()} />
        </Field>
      </Section>

      <Section
        id="clip-pacing"
        title={t("Pacing")}
        icon="speed"
        defaultOpen={false}
        keywords={t("speed skim idle fast faster silence cut length duration")}
        aside={<span class="badge">{outDur().toFixed(1)}s</span>}
      >
        <Field label={t("Skim idle stretches")} hint={t("Plays the moments where nothing happens faster")}>
          <Switch checked={ed.speed().length > 0} label={t("Skim idle stretches")} onChange={() => ed.toggleSkim()} />
        </Field>
        <Field label={t("Skim speed")} stack>
          <Seg
            full
            value={ed.skimFactor()}
            onChange={(v) => ed.setSkimFactor(v)}
            options={SKIMS.map((f) => ({ value: f, label: `${f}×` }))}
          />
        </Field>
        <div class="btn-row">
          <button type="button" class="btn sm grow" data-kbd="X" data-tip={t("Speed up 2s at the playhead")} onClick={() => void ed.addSpeedAtPlayhead()}>
            <Icon name="speed" size={14} /> {t("Speed up")}
          </button>
          <button type="button" class="btn sm grow" data-kbd="C" data-tip={t("Remove 1s at the playhead")} onClick={() => void ed.addCutAtPlayhead()}>
            <Icon name="cut" size={14} /> {t("Cut")}
          </button>
        </div>
        <Field label={t("Final length")}>
          <span class="readout">
            {outDur().toFixed(1)}s
            <Show when={Math.abs(outDur() - ed.duration()) > 0.05}>
              <span class="readout-sub"> {t("of {total}s", { total: ed.duration().toFixed(1) })}</span>
            </Show>
          </span>
        </Field>
      </Section>

      <Section
        id="clip-audio"
        title={t("Audio")}
        icon="waves"
        defaultOpen={false}
        keywords={t("sound microphone mic system volume mute noise voice narration")}
        aside={<span class="badge">{ed.audio.hasAudio() ? ed.audio.tracks().length === 1 ? t("1 track") : t("{n} tracks", { n: ed.audio.tracks().length }) : t("Silent")}</span>}
      >
        <Show
          when={ed.audio.hasAudio()}
          fallback={
            <p class="note">
              {t("This take is silent. Turn on the microphone or system sound before recording to add narration.")}
            </p>
          }
        >
          <Index each={ed.audio.tracks()}>
            {(track) => (
              <div class="audio-row" classList={{ muted: track().muted }}>
                <div class="audio-row-head">
                  <Icon name={track().kind === "mic" ? "mic" : "volume"} size={14} />
                  <span>{trackLabel(track().kind)}</span>
                  <IconButton
                    icon={track().muted ? "volumeOff" : "volume"}
                    tip={track().muted ? t("Unmute") : t("Mute")}
                    active={track().muted}
                    onClick={() => ed.audio.toggleMute(track().kind)}
                  />
                </div>
                <Slider
                  value={track().gain}
                  min={0}
                  max={2}
                  step={0.05}
                  disabled={track().muted}
                  label={t("{track} volume", { track: trackLabel(track().kind) })}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={(v) => void ed.audio.setGain(track().kind, v)}
                />
                <Show when={track().kind === "mic"}>
                  <Field label={t("Remove noise")} hint={t("Filters out fans, hum and hiss while keeping your voice")}>
                    <Switch
                      checked={!!track().denoise}
                      label={t("Remove noise")}
                      onChange={(v) => void ed.audio.setDenoise(track().kind, v)}
                    />
                  </Field>
                  <Field label={t("Even out volume")} hint={t("Brings quiet and loud passages to one steady level")}>
                    <Switch
                      checked={!!track().level}
                      label={t("Even out volume")}
                      onChange={(v) => void ed.audio.setLevel(track().kind, v)}
                    />
                  </Field>
                  <Show when={ed.audio.cleaning()[track().kind]}>
                    <p class="note audio-cleaning" role="status">
                      {t("Cleaning up the voice…")}
                    </p>
                  </Show>
                </Show>
              </div>
            )}
          </Index>
          <p class="note">{t("Sped-up stretches play silent and cuts remove their sound. MP4 carries the audio; GIFs are always silent.")}</p>
        </Show>
      </Section>

      <CaptionsSection />

      <Section
        id="clip-pointer"
        title={t("Pointer")}
        icon="cursor"
        defaultOpen={false}
        keywords={t("cursor mouse arrow size smooth hide still color colour custom highlight halo picture image upload dot opacity transparent shadow")}
        aside={<span class="badge">{ed.cursorStyle() ? t("Smooth") : ed.pointerCaptured() ? t("Recorded") : t("Hidden")}</span>}
      >
        <Field label={t("Smooth pointer")} hint={t("A clean pointer that glides, drawn from your movements")}>
          <Switch checked={!!ed.cursorStyle()} label={t("Smooth pointer")} onChange={() => ed.toggleCursor()} />
        </Field>
        <Show when={ed.cursorStyle()}>
          {(c) => (
            <>
              <Field label={t("Look")} stack>
                <div class="look-cards">
                  <For each={POINTER_LOOKS()}>
                    {(l) => (
                      <button
                        type="button"
                        class="look-card"
                        classList={{ on: pointerLook(c()) === l.value }}
                        aria-pressed={pointerLook(c()) === l.value}
                        data-tip={l.tip}
                        onClick={() => ed.setCursorLook(l.value)}
                      >
                        <span class="look-card-art">
                          <Show when={l.value === "classic"}>
                            <Icon name="cursor" size={22} />
                          </Show>
                          <Show when={l.value === "dot"}>
                            <span class="look-dot" />
                          </Show>
                          <Show when={l.value === "image"}>
                            <Show when={c().image && ed.pointerPicture()} fallback={<Icon name="image" size={20} />}>
                              {(pic) => <img src={pic().preview} alt="" />}
                            </Show>
                          </Show>
                        </span>
                        {l.label}
                      </button>
                    )}
                  </For>
                </div>
              </Field>
              <Show when={pointerLook(c()) === "image" && c().image}>
                {(img) => (
                  <div class="ptr-picture">
                    <Show
                      when={ed.pointerPicture()}
                      fallback={
                        <span class="ptr-picture-missing">
                          <Icon name="image" size={18} />
                        </span>
                      }
                    >
                      {(pic) => (
                        <button
                          type="button"
                          class="ptr-hotspot"
                          aria-label={t("Set where the picture points")}
                          data-tip={t("Click where it should point")}
                          onClick={(e) => {
                            // A keyboard "click" has no position: the buttons beside it serve that.
                            if (e.detail === 0) return;
                            const r = e.currentTarget.getBoundingClientRect();
                            ed.setCursorHotspot((e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
                          }}
                        >
                          <img src={pic().preview} alt="" draggable={false} />
                          <span
                            class="ptr-hotspot-mark"
                            style={{ left: `${img().hotspot[0] * 100}%`, top: `${img().hotspot[1] * 100}%` }}
                          />
                        </button>
                      )}
                    </Show>
                    <div class="ptr-picture-side">
                      <span class="ptr-picture-name" data-tip={img().path}>
                        {img().path.split(/[\\/]/).pop() || t("Picture")}
                      </span>
                      <span class="ptr-picture-hint">
                        {ed.pointerPicture() ? t("Click the picture where it should point") : t("This picture can't be found")}
                      </span>
                      <div class="ptr-picture-actions">
                        <button
                          type="button"
                          class="btn sm"
                          data-tip={t("Points with its top-left corner, like an arrow")}
                          aria-pressed={img().hotspot[0] === 0 && img().hotspot[1] === 0}
                          onClick={() => ed.setCursorHotspot(0, 0)}
                        >
                          {t("Corner")}
                        </button>
                        <button
                          type="button"
                          class="btn sm"
                          data-tip={t("Points with its middle, like a dot or a ring")}
                          aria-pressed={img().hotspot[0] === 0.5 && img().hotspot[1] === 0.5}
                          onClick={() => ed.setCursorHotspot(0.5, 0.5)}
                        >
                          {t("Center")}
                        </button>
                        <button type="button" class="btn sm" onClick={() => void ed.chooseCursorPicture()}>
                          {t("Change")}
                        </button>
                      </div>
                    </div>
                  </div>
                )}
              </Show>
              <Field label={t("Size")} stack>
                <Slider
                  value={c().size}
                  min={0.5}
                  max={3}
                  step={0.05}
                  label={t("Pointer size")}
                  format={(v) => `${v.toFixed(1)}×`}
                  onInput={ed.setCursorSize}
                />
              </Field>
              <Field label={t("Smoothing")} stack>
                <Slider
                  value={c().smoothing}
                  min={0}
                  max={0.2}
                  step={0.01}
                  label={t("Pointer smoothing")}
                  format={(v) => (v === 0 ? t("Off") : v < 0.04 ? t("Light") : v < 0.1 ? t("Medium") : t("Heavy"))}
                  onInput={ed.setCursorSmoothing}
                />
              </Field>
              <Field label={t("Hide when still")} hint={t("Fades out while the mouse rests and is back before it moves")}>
                <Switch
                  checked={!!c().hide_idle}
                  label={t("Hide pointer when still")}
                  onChange={(v) => ed.setCursorHideIdle(v)}
                />
              </Field>
              <Field label={t("Opacity")} stack>
                <Slider
                  value={c().opacity ?? 1}
                  min={0}
                  max={1}
                  step={0.05}
                  label={t("Pointer opacity")}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={ed.setCursorOpacity}
                />
              </Field>
              <Field label={t("Shadow")} hint={t("A soft shadow under the pointer")}>
                <Switch checked={c().shadow ?? true} label={t("Pointer shadow")} onChange={(v) => ed.setCursorShadow(v)} />
              </Field>
              <Show when={pointerLook(c()) !== "image"}>
                <Field label={t("Color")} stack>
                  <div class="color-row">
                    <For each={POINTER_COLORS}>
                      {(hex) => (
                        <button
                          type="button"
                          class="swatch"
                          classList={{ on: pointerHex(c().color) === hex }}
                          style={{ background: hex }}
                          aria-label={t("Pointer color {hex}", { hex })}
                          aria-pressed={pointerHex(c().color) === hex}
                          data-tip={hex === POINTER_COLORS[0] ? t("White, the classic pointer") : hex}
                          onClick={() => ed.setCursorColor(hex === POINTER_COLORS[0] ? null : hex)}
                        />
                      )}
                    </For>
                    <label class="swatch swatch-custom" data-tip={t("Custom color")}>
                      <input
                        type="color"
                        value={pointerHex(c().color)}
                        aria-label={t("Custom pointer color")}
                        onInput={(e) => ed.setCursorColor(e.currentTarget.value)}
                      />
                    </label>
                  </div>
                </Field>
              </Show>
              <Field label={t("Highlight")} hint={t("A soft yellow disc under the pointer, so viewers never lose it")}>
                <Switch checked={!!c().halo} label={t("Highlight the pointer")} onChange={(v) => ed.setCursorHalo(v)} />
              </Field>
            </>
          )}
        </Show>
        <Show when={ed.cursorStyle() && ed.pointerCaptured()}>
          <p class="note">
            {t("The real pointer is also in this take, so two show. Set the pointer to Smooth before recording for a clean result.")}
          </p>
        </Show>
        <Show when={!ed.cursorStyle() && !ed.pointerCaptured()}>
          <p class="note">{t("This take has no visible pointer. Turn on the smooth pointer to show one.")}</p>
        </Show>
      </Section>

      <Show when={ed.cameraOverlay()}>
        {(c) => (
          <Section
            id="clip-camera"
            title={t("Camera")}
            icon="camera"
            defaultOpen={false}
            keywords={t("webcam face bubble corner mirror circle")}
            aside={<span class="badge">{c().visible ? t("On") : t("Hidden")}</span>}
          >
            <Field label={t("Show camera")} hint={t("Hide the bubble without losing the recording")}>
              <Switch checked={c().visible} label={t("Show camera")} onChange={(v) => ed.updateCamera({ visible: v })} />
            </Field>
            <Field label={t("Corner")}>
              <div class="corner-pick">
                <For each={CAMERA_CORNERS()}>
                  {(k) => (
                    <button
                      type="button"
                      class="corner-cell"
                      classList={{ on: c().corner === k.value }}
                      aria-pressed={c().corner === k.value}
                      aria-label={k.label}
                      data-tip={k.label}
                      disabled={!c().visible}
                      onClick={() => ed.updateCamera({ corner: k.value })}
                    >
                      <span class="corner-dot" />
                    </button>
                  )}
                </For>
              </div>
            </Field>
            <Field label={t("Size")} stack>
              <Slider
                value={c().size}
                min={0.12}
                max={0.6}
                step={0.01}
                disabled={!c().visible}
                label={t("Camera size")}
                format={(v) => `${Math.round(v * 100)}%`}
                onInput={(v) => ed.updateCamera({ size: v })}
              />
            </Field>
            <Field label={t("Shape")} stack>
              <Seg
                full
                value={c().shape}
                disabled={!c().visible}
                label={t("Camera shape")}
                onChange={(v) => ed.updateCamera({ shape: v })}
                options={[
                  { value: "circle", label: t("Circle") },
                  { value: "square", label: t("Square") },
                  { value: "wide", label: t("Wide"), tip: t("A 16:9 frame") },
                ]}
              />
            </Field>
            <Field label={t("Mirror")} hint={t("Flip the camera the way you see yourself")}>
              <Switch
                checked={c().mirror}
                disabled={!c().visible}
                label={t("Mirror camera")}
                onChange={(v) => ed.updateCamera({ mirror: v })}
              />
            </Field>
          </Section>
        )}
      </Show>

      <Section
        id="clip-overlays"
        title={t("Overlays")}
        icon="clicks"
        defaultOpen={false}
        keywords={t("clicks ripples keystrokes keys shortcuts keyboard click effect animation rings pulse burst color size speed")}
        aside={
          <Show when={ed.showClicks() || ed.showKeys()}>
            <span class="badge">{[ed.showClicks() && t("Clicks"), ed.showKeys() && t("Keys")].filter(Boolean).join(" · ")}</span>
          </Show>
        }
      >
        <Field label={t("Click effect")} hint={t("An animation at every recorded click")}>
          <Switch checked={ed.showClicks()} label={t("Click effect")} onChange={() => ed.toggleClicks()} />
        </Field>
        <Show when={ed.showClicks()}>
          <Field label={t("Animation")} stack>
            <div class="look-cards four">
              <For each={CLICK_EFFECTS()}>
                {(fx) => (
                  <button
                    type="button"
                    class="look-card"
                    classList={{ on: ed.clickStyle().effect === fx.value }}
                    aria-pressed={ed.clickStyle().effect === fx.value}
                    data-tip={fx.tip}
                    onClick={() => ed.updateClickStyle({ effect: fx.value })}
                  >
                    <span class="look-card-art" style={{ color: pointerHex(ed.clickStyle().color) }}>
                      <ClickEffectArt effect={fx.value} />
                    </span>
                    {fx.label}
                  </button>
                )}
              </For>
            </div>
          </Field>
          <Field label={t("Click color")} stack>
            <div class="color-row">
              <For each={CLICK_COLORS}>
                {(hex) => (
                  <button
                    type="button"
                    class="swatch"
                    classList={{ on: pointerHex(ed.clickStyle().color) === hex }}
                    style={{ background: hex }}
                    aria-label={t("Click color {hex}", { hex })}
                    aria-pressed={pointerHex(ed.clickStyle().color) === hex}
                    data-tip={hex === CLICK_COLORS[0] ? t("White") : hex}
                    onClick={() => ed.setClickColor(hex === CLICK_COLORS[0] ? null : hex)}
                  />
                )}
              </For>
              <label class="swatch swatch-custom" data-tip={t("Custom color")}>
                <input
                  type="color"
                  value={pointerHex(ed.clickStyle().color)}
                  aria-label={t("Custom click color")}
                  onInput={(e) => ed.setClickColor(e.currentTarget.value)}
                />
              </label>
            </div>
          </Field>
          <Field label={t("Click size")} stack>
            <Slider
              value={ed.clickStyle().size}
              min={0.5}
              max={3}
              step={0.05}
              label={t("Click effect size")}
              format={(v) => `${v.toFixed(1)}×`}
              onInput={(v) => ed.updateClickStyle({ size: v })}
            />
          </Field>
          <Field label={t("Click length")} stack>
            <Slider
              value={ed.clickStyle().duration}
              min={0.2}
              max={1.5}
              step={0.05}
              label={t("Click effect length")}
              format={(v) => `${Math.round(v * 1000)} ms`}
              onInput={(v) => ed.updateClickStyle({ duration: v })}
            />
          </Field>
          <Field label={t("Click opacity")} stack>
            <Slider
              value={ed.clickStyle().opacity}
              min={0.1}
              max={1}
              step={0.05}
              label={t("Click effect opacity")}
              format={(v) => `${Math.round(v * 100)}%`}
              onInput={(v) => ed.updateClickStyle({ opacity: v })}
            />
          </Field>
        </Show>
        <Field label={t("Keystrokes")} hint={t("Shortcuts you pressed appear as chips. Plain typing never shows")}>
          <Switch checked={ed.showKeys()} label={t("Keystrokes")} onChange={() => ed.toggleKeys()} />
        </Field>
      </Section>

      <Section id="clip-info" title={t("Clip")} icon="info" defaultOpen={false} keywords={t("facts details duration length aspect")}>
        <dl class="facts">
          <dt>{t("Recorded")}</dt>
          <dd>{ed.duration().toFixed(2)}s</dd>
          <dt>{t("Exports as")}</dt>
          <dd>{outDur().toFixed(2)}s</dd>
          <dt>{t("Aspect")}</dt>
          <dd>{ed.frameAspect().toFixed(3)}</dd>
          <dt>{t("Zooms")}</dt>
          <dd>{ed.zooms().length}</dd>
          <dt>{t("Annotations")}</dt>
          <dd>
            {ed.anns().texts.length + ed.anns().arrows.length + ed.anns().highlights.length + ed.anns().strokes.length}
          </dd>
          <dt>{t("Cuts")}</dt>
          <dd>{ed.cuts().length}</dd>
        </dl>
      </Section>
    </>
  );
}

function CaptionsSection() {
  const ed = useEditor();
  const c = ed.captions;
  onMount(() => void c.loadStatus());
  const count = () => c.list().length;
  const supported = () => c.status()?.supported ?? true;
  const needsDownload = () => !!c.status() && !c.status()!.model_ready;
  const canGenerate = () => supported() && ed.audio.hasAudio();
  const style = () => c.style();
  return (
    <Section
      id="clip-captions"
      title={t("Captions")}
      icon="captions"
      defaultOpen={false}
      keywords={t("subtitles srt words speech transcribe text language")}
      aside={
        <Show when={count() > 0}>
          <span class="badge">{count()}</span>
        </Show>
      }
    >
      <Show
        when={c.job()}
        fallback={
          <>
            <Show when={!supported()}>
              <p class="note">
                {t("Making captions needs a newer processor (most PCs from 2015 on). You can still type them in with Add caption.")}
              </p>
            </Show>
            <Show when={supported() && !ed.audio.hasAudio()}>
              <p class="note">
                {t("This take has no sound to make captions from. You can still type them in with Add caption.")}
              </p>
            </Show>
            <Show when={canGenerate()}>
              <Field label={t("Spoken language")} stack>
                <select
                  class="input"
                  aria-label={t("Spoken language")}
                  value={prefs.captionLanguage()}
                  onChange={(e) => prefs.captionLanguage.set(e.currentTarget.value)}
                >
                  <For each={CAPTION_LANGUAGES()}>{(l) => <option value={l.value}>{l.label}</option>}</For>
                </select>
              </Field>
              <button type="button" class="btn primary block" onClick={() => void c.generate()}>
                <Icon name="captions" size={14} /> {count() > 0 ? t("Make captions again") : t("Make captions")}
              </button>
              <p class="note">
                <Show
                  when={needsDownload()}
                  fallback={t("Vuoom listens to your narration on this computer. Nothing is uploaded.")}
                >
                  {t("The first time, Vuoom downloads its speech model ({mb} MB). After that, captions are made on this computer. Nothing is uploaded.", {
                    mb: Math.round((c.status()?.model_bytes ?? 0) / 1_000_000),
                  })}
                </Show>
              </p>
            </Show>
          </>
        }
      >
        {(j) => (
          <div class="cap-job" role="status">
            <div class="cap-bar" classList={{ busy: j().step === "start" }}>
              <i style={{ width: `${jobFraction(j()) * 100}%` }} />
            </div>
            <p class="note">{jobLabel(j())}</p>
            <button type="button" class="btn block" onClick={c.cancel}>
              {t("Cancel")}
            </button>
          </div>
        )}
      </Show>

      <Show when={count() > 0}>
        <Field label={t("Show captions")} hint={t("Hide them in the video but keep them, and the .srt file")}>
          <Switch checked={style().visible} label={t("Show captions")} onChange={(v) => c.updateStyle({ visible: v })} />
        </Field>
        <Field label={t("Size")} stack>
          <Slider
            value={style().size}
            min={CAPTION_MIN_SIZE}
            max={CAPTION_MAX_SIZE}
            step={0.0025}
            disabled={!style().visible}
            label={t("Caption size")}
            format={(v) => (v < 0.037 ? t("Small") : v < 0.055 ? t("Medium") : t("Large"))}
            onInput={(v) => c.updateStyle({ size: v })}
          />
        </Field>
        <Field label={t("Position")} stack>
          <Seg
            full
            value={style().position}
            disabled={!style().visible}
            label={t("Caption position")}
            onChange={(v) => c.updateStyle({ position: v })}
            options={[
              { value: "bottom", label: t("Bottom") },
              { value: "top", label: t("Top"), tip: t("Out of the way of subtitles a player adds") },
            ]}
          />
        </Field>
      </Show>
      <div class="cap-actions">
        <button
          type="button"
          class="btn"
          disabled={!!c.job()}
          data-tip={t("Add a caption at the playhead and type it in")}
          onClick={() => void ed.addCaptionAtPlayhead()}
        >
          <Icon name="plus" size={14} /> {t("Add caption")}
        </button>
        <button
          type="button"
          class="btn"
          disabled={count() === 0}
          data-tip={t("A subtitle file for YouTube and video players, timed to your export")}
          onClick={() => void c.saveSrt()}
        >
          <Icon name="download" size={14} /> {t("Save .srt")}
        </button>
      </div>
      <Show when={count() > 0 && !c.job()}>
        <button type="button" class="btn ghost block" onClick={() => void c.clear()}>
          <Icon name="trash" size={14} /> {t("Remove all captions")}
        </button>
      </Show>
    </Section>
  );
}
