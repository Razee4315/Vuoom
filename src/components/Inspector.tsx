// The inspector: a resizable, collapsible right panel with two tabs.
//  - Selection: every property of whatever is selected (annotation, zoom, speed, cut), or
//    the armed tool's card, in collapsible sections that remember their state.
//  - Clip: whole-recording settings: frame + backdrop, crop, camera (auto zooms), pacing
//    (skim idle), click ripples + keystrokes, and clip facts.
import { createEffect, createSignal, For, Index, on, Show, type JSX } from "solid-js";
import { trackLabel } from "../editor/audio";
import { useEditor } from "../editor/context";
import { PRESET_COLORS, TEXT_FONTS } from "../editor/constants";
import { fmt, rgbHex } from "../format";
import { outputDuration } from "../geometry";
import { Icon, type IconName } from "../icons";
import { layout, setInspectorW } from "../prefs";
import ScrubField from "../ScrubField";
import { TOOLS } from "../shortcuts";
import type { CropRect } from "../types";
import { Field, IconButton, Section, Seg, Slider, Switch } from "../ui";

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
    <aside class="inspector panel" style={{ width: `min(${layout.inspectorW()}px, 36vw)` }} aria-label="Inspector">
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
            Selection
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
            Clip
          </button>
        </div>
        <IconButton
          icon="panelRight"
          tip="Hide inspector"
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

const KIND_ICON: Record<string, IconName> = {
  Text: "text",
  Arrow: "arrow",
  Line: "line",
  Box: "shape",
  Ellipse: "shape",
  Highlight: "highlight",
  Mask: "mask",
};

function SelectionPanel() {
  const ed = useEditor();
  return (
    <>
      <Show when={ed.selected()}>
        <Show when={ed.selCount() > 1} fallback={<AnnotationProps />}>
          <PanelTitle
            icon="layers"
            title={`${ed.selCount()} annotations`}
            sub="Drag any one on the canvas to move them together"
          />
          <div class="insp-pad">
            <button type="button" class="btn danger block" onClick={() => void ed.deleteSelection()}>
              <Icon name="trash" size={14} /> Delete {ed.selCount()} annotations
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
      <Show when={ed.drawingToolActive() && !ed.somethingSelected()}>
        <ToolCard />
      </Show>
      <Show when={!ed.somethingSelected() && !ed.drawingToolActive()}>
        <div class="insp-empty">
          <span class="insp-empty-icon">
            <Icon name="cursor" size={20} />
          </span>
          <strong>Nothing selected</strong>
          <p>Click something on the video or the timeline to edit it, or pick a tool to draw.</p>
          <div class="insp-empty-keys">
            <span>
              <kbd>T</kbd> Text
            </span>
            <span>
              <kbd>A</kbd> Arrow
            </span>
            <span>
              <kbd>S</kbd> Box
            </span>
            <span>
              <kbd>Z</kbd> Zoom here
            </span>
          </div>
        </div>
      </Show>
    </>
  );
}

function ToolCard() {
  const ed = useEditor();
  const t = () => TOOLS.find((x) => x.id === ed.tool());
  return (
    <>
      <PanelTitle icon="wand" title={`${t()?.label} tool`} sub={t()?.hint} />
      <div class="insp-pad">
        <p class="note">
          Options appear here once you place one. Double click a tool, or turn on the lock at the
          bottom of the rail, to draw several in a row. <kbd>Esc</kbd> returns to Select.
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
            aria-label={`Color ${c}`}
            data-tip={c}
            onClick={() => ed.setColor(c)}
          />
        )}
      </For>
      <label class="swatch swatch-custom" data-tip="Custom color">
        <input
          type="color"
          value={rgbHex(ed.selectedColor()!)}
          aria-label="Custom color"
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
    <Section id={`${props.id}-timing`} title="Timing" icon="timer">
      <Field label="Appears">
        <ScrubField
          value={Number(r().start.toFixed(2))}
          min={0}
          max={ed.duration()}
          step={0.05}
          suffix="s"
          title="When this appears. Drag to scrub, click to type"
          onCommit={(v) => ed.editRange(v, r().end)}
        />
      </Field>
      <Field label="Disappears">
        <ScrubField
          value={Number(r().end.toFixed(2))}
          min={0}
          max={ed.duration()}
          step={0.05}
          suffix="s"
          title="When this disappears. Drag to scrub, click to type"
          onCommit={(v) => ed.editRange(r().start, v)}
        />
      </Field>
      <Field label="On screen">
        <span class="readout">{(r().end - r().start).toFixed(2)}s</span>
      </Field>
      <Field label="Fade in" hint="0 pops in instantly">
        <ScrubField
          value={Number(r().fade_in.toFixed(2))}
          min={0}
          max={2}
          step={0.05}
          suffix="s"
          title="Fade-in length. Drag to scrub, click to type"
          onInput={(v) => ed.editFades(v, r().fade_out)}
          onCommit={(v) => ed.editFades(v, r().fade_out)}
        />
      </Field>
      <Field label="Fade out" hint="0 disappears instantly">
        <ScrubField
          value={Number(r().fade_out.toFixed(2))}
          min={0}
          max={2}
          step={0.05}
          suffix="s"
          title="Fade-out length. Drag to scrub, click to type"
          onInput={(v) => ed.editFades(r().fade_in, v)}
          onCommit={(v) => ed.editFades(r().fade_in, v)}
        />
      </Field>
      <div class="btn-row">
        <button type="button" class="btn sm" data-tip="Start at the playhead" onClick={() => {
          const len = r().end - r().start;
          ed.editRange(ed.playhead(), Math.min(ed.duration(), ed.playhead() + len));
        }}>
          Move to playhead
        </button>
        <button type="button" class="btn sm" data-tip="Show until the end of the clip" onClick={() => ed.editRange(r().start, ed.duration())}>
          Until end
        </button>
      </div>
      <Show when={ed.isGhost(r(), true)}>
        <p class="note">
          Hidden at the playhead (shown dimmed so you can edit it). It is on screen from {fmt(r().start)} to{" "}
          {fmt(r().end)}.
        </p>
      </Show>
    </Section>
  );
}

function ArrangeSection() {
  const ed = useEditor();
  return (
    <Section id="ann-arrange" title="Arrange" icon="layers" defaultOpen={false}>
      <div class="btn-grid">
        <button type="button" class="btn sm" data-kbd="Ctrl+]" data-tip="Bring forward" onClick={() => void ed.reorderSelected("forward")}>
          <Icon name="bringForward" size={14} /> Forward
        </button>
        <button type="button" class="btn sm" data-kbd="Ctrl+[" data-tip="Send backward" onClick={() => void ed.reorderSelected("backward")}>
          <Icon name="sendBackward" size={14} /> Backward
        </button>
        <button type="button" class="btn sm" data-kbd="Ctrl+Shift+]" data-tip="Bring to front" onClick={() => void ed.reorderSelected("front")}>
          To front
        </button>
        <button type="button" class="btn sm" data-kbd="Ctrl+Shift+[" data-tip="Send to back" onClick={() => void ed.reorderSelected("back")}>
          To back
        </button>
      </div>
      <p class="note">Stacking applies within a kind: boxes sit under arrows, arrows under text.</p>
    </Section>
  );
}

function AnnotationProps() {
  const ed = useEditor();
  const title = () => ed.inspTitle();
  const actions = (
    <>
      <IconButton icon="duplicate" tip="Duplicate" kbd="Ctrl+D" class="sm" onClick={() => void ed.duplicateSelected()} />
      <IconButton icon="copy" tip="Copy" kbd="Ctrl+C" class="sm" onClick={() => void ed.copySelected()} />
      <IconButton icon="trash" tip="Delete" kbd="Del" class="sm danger" onClick={() => void ed.deleteSelection()} />
    </>
  );
  return (
    <>
      <PanelTitle
        icon={KIND_ICON[title()] ?? "shape"}
        title={title()}
        sub="Drag to move, handles to resize"
        actions={actions}
      />

      <Show when={ed.selectedText()}>
        {(t) => (
          <>
            <Section id="ann-text" title="Text" icon="text">
              <input
                class="input"
                type="text"
                spellcheck={false}
                placeholder="Label text"
                aria-label="Label text"
                ref={ed.refs.contentInput}
                onInput={(e) => ed.editText(e.currentTarget.value)}
              />
              <Field label="Size">
                <ScrubField
                  value={t().font_size}
                  min={0.02}
                  max={0.2}
                  step={0.005}
                  displayScale={100}
                  suffix="%"
                  title="Font size (percent of height). Drag to scrub, click to type"
                  onInput={(v) => ed.editFontSize(v)}
                  onCommit={(v) => ed.editFontSize(v)}
                />
              </Field>
              <Field label="Style">
                <div class="toggle-group">
                  <button type="button" class="tgl" classList={{ on: t().bold }} data-tip="Bold" aria-pressed={t().bold} onClick={() => ed.editTextStyle({ bold: !t().bold })}>
                    <b>B</b>
                  </button>
                  <button type="button" class="tgl" classList={{ on: t().italic }} data-tip="Italic" aria-pressed={t().italic} onClick={() => ed.editTextStyle({ italic: !t().italic })}>
                    <i>I</i>
                  </button>
                  <button type="button" class="tgl wide" classList={{ on: t().background }} data-tip="Legible plate behind the text" aria-pressed={t().background} onClick={() => ed.editTextStyle({ background: !t().background })}>
                    Plate
                  </button>
                </div>
              </Field>
            </Section>
            <Section id="ann-font" title="Typeface" icon="text">
              <div class="font-grid">
                <For each={TEXT_FONTS}>
                  {(f) => (
                    <button
                      type="button"
                      class="font-card"
                      classList={{ on: (t().font || "") === f.id }}
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

      <Show when={ed.selectedBox() && !ed.isMask()}>
        <Section id="ann-shape" title="Shape" icon="shape">
          <Field label="Shape">
            <Seg
              value={ed.selectedBox()!.shape === "Ellipse" ? "ellipse" : "rect"}
              onChange={(v) => ed.setShape(v === "ellipse")}
              options={[
                { value: "rect", label: "Rectangle" },
                { value: "ellipse", label: "Ellipse" },
              ]}
            />
          </Field>
          <Field label="Fill">
            <Seg
              value={ed.selectedBox()!.filled ? "filled" : "outline"}
              onChange={(v) => ed.editStyle({ filled: v === "filled" })}
              options={[
                { value: "outline", label: "Outline" },
                { value: "filled", label: "Filled" },
              ]}
            />
          </Field>
          <Show when={!ed.selectedBox()!.filled}>
            <Field label="Stroke">
              <ScrubField
                value={ed.selectedBox()!.thickness}
                min={0.002}
                max={0.02}
                step={0.001}
                displayScale={100}
                suffix="%"
                title="Outline thickness as a percent of height"
                onInput={(v) => ed.editStyle({ thickness: v })}
                onCommit={(v) => ed.editStyle({ thickness: v })}
              />
            </Field>
          </Show>
        </Section>
      </Show>

      <Show when={ed.isMask()}>
        <Section id="ann-mask" title="Redaction" icon="mask">
          <p class="note">
            The masked area renders as solid black in the export, whatever was underneath. Drag its bar on
            the timeline to control when it covers the frame.
          </p>
        </Section>
      </Show>

      <Show when={ed.selectedArrow()}>
        {(a) => (
          <Section id="ann-arrow" title="Stroke" icon="arrow">
            <Field label="Ends">
              <Seg
                value={a().style === "Line" ? "line" : a().style === "DoubleArrow" ? "double" : "arrow"}
                onChange={(v) => ed.setArrowStyle(v)}
                options={[
                  { value: "arrow", label: "Arrow" },
                  { value: "double", label: "Double" },
                  { value: "line", label: "Line" },
                ]}
              />
            </Field>
            <Field label="Weight">
              <ScrubField
                value={a().thickness}
                min={0.002}
                max={0.02}
                step={0.001}
                displayScale={100}
                suffix="%"
                title="Stroke thickness as a percent of height"
                onInput={(v) => ed.editStyle({ thickness: v })}
                onCommit={(v) => ed.editStyle({ thickness: v })}
              />
            </Field>
          </Section>
        )}
      </Show>

      <Show when={ed.selectedColor() && !ed.isMask()}>
        <Section id="ann-color" title="Color" icon="palette">
          <ColorRow />
          <Field label="Opacity">
            <Slider
              value={ed.selectedColor()!.a ?? 1}
              min={0.1}
              max={1}
              step={0.05}
              label="Opacity"
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
        title="Zoom"
        sub={`${z().amount.toFixed(1)}× from ${z().start.toFixed(1)}s to ${z().end.toFixed(1)}s`}
        actions={<IconButton icon="trash" tip="Delete zoom" kbd="Del" class="sm danger" onClick={() => void ed.deleteSelectedZoom()} />}
      />
      <Section id="zoom-camera" title="Camera" icon="zoomIn">
        <Field label="Strength" stack>
          <Slider
            value={z().amount}
            min={1.2}
            max={4}
            step={0.1}
            label="Zoom strength"
            format={(v) => `${v.toFixed(1)}×`}
            onInput={(v) => void ed.applyZoomEdit(i(), z().start, z().end, v)}
          />
        </Field>
        <Field label="Aim" stack>
          <Seg
            full
            value={ed.selZoomFocus() ? "fixed" : "follow"}
            onChange={(v) => void ed.applyZoomFocus(v === "fixed" ? (ed.selZoomFocus() ?? { x: 0.5, y: 0.5 }) : null)}
            options={[
              { value: "follow", label: "Follow cursor", tip: "The camera tracks your recorded cursor" },
              { value: "fixed", label: "Fixed point", tip: "Hold one spot. Drag the crosshair to aim" },
            ]}
          />
        </Field>
        <Show when={ed.selZoomFocus()}>
          <p class="note">Drag the crosshair on the video to aim this zoom.</p>
        </Show>
        <Field label="Motion" stack>
          <Seg
            full
            value={z().style}
            onChange={(v) => void ed.applyZoomStyle(v)}
            options={[
              { value: "Smooth", label: "Smooth", tip: "Cinematic glide (default)" },
              { value: "Snappy", label: "Snappy", tip: "Settles faster" },
              { value: "Slow", label: "Slow", tip: "Gentle drift" },
            ]}
          />
        </Field>
      </Section>
      <Section id="zoom-timing" title="Timing" icon="timer">
        <Field label="Start">
          <ScrubField
            value={Number(z().start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title="When this zoom starts"
            onCommit={(v) => void ed.applyZoomEdit(i(), v, z().end, z().amount)}
          />
        </Field>
        <Field label="End">
          <ScrubField
            value={Number(z().end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title="When this zoom ends"
            onCommit={(v) => void ed.applyZoomEdit(i(), z().start, v, z().amount)}
          />
        </Field>
        <Field label="Length">
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
        title="Speed up"
        sub={`${(r().end - r().start).toFixed(1)}s plays in ${((r().end - r().start) / r().factor).toFixed(1)}s`}
        actions={<IconButton icon="trash" tip="Delete speed region" kbd="Del" class="sm danger" onClick={() => void ed.deleteSelectedSpeed()} />}
      />
      <Section id="speed-rate" title="Rate" icon="speed">
        <Field label="Rate" stack>
          <Slider
            value={r().factor}
            min={1.25}
            max={8}
            step={0.25}
            label="Playback rate"
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
      <Section id="speed-timing" title="Timing" icon="timer">
        <Field label="Start">
          <ScrubField
            value={Number(r().start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title="When this speed region starts"
            onCommit={(v) => void ed.applySpeedEdit(i(), v, r().end, r().factor)}
          />
        </Field>
        <Field label="End">
          <ScrubField
            value={Number(r().end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title="When this speed region ends"
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
      <PanelTitle icon="cut" title="Cut" sub={`${(c().end - c().start).toFixed(2)}s removed from the export`} />
      <Section id="cut-timing" title="Removed span" icon="timer">
        <Field label="From">
          <ScrubField
            value={Number(c().start.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title="Where the removed section starts"
            onCommit={(v) => void ed.applyCutEdit(i(), v, c().end)}
          />
        </Field>
        <Field label="To">
          <ScrubField
            value={Number(c().end.toFixed(2))}
            min={0}
            max={ed.duration()}
            step={0.05}
            suffix="s"
            title="Where the removed section ends"
            onCommit={(v) => void ed.applyCutEdit(i(), c().start, v)}
          />
        </Field>
        <p class="note">Playback and export jump straight over this span.</p>
        <button type="button" class="btn block" onClick={() => void ed.deleteSelectedCut()}>
          <Icon name="reset" size={14} /> Restore this section
        </button>
      </Section>
    </>
  );
}

// ── clip ─────────────────────────────────────────────────────────────────────

const FRAMES: { id: string; label: string; tip: string }[] = [
  { id: "none", label: "None", tip: "Edge to edge, exactly what you recorded" },
  { id: "subtle", label: "Subtle", tip: "A little padding, soft corners and shadow" },
  { id: "studio", label: "Studio", tip: "Generous padding on a backdrop, product-shot style" },
];

// Mirrors the values session::set_frame_preset writes for each preset.
const FRAME_VALUES: Record<string, { padding: number; radius: number; shadow: number }> = {
  none: { padding: 0, radius: 0, shadow: 0 },
  subtle: { padding: 0.04, radius: 0.012, shadow: 0.3 },
  studio: { padding: 0.075, radius: 0.02, shadow: 0.5 },
};
const bgHex = (c: [number, number, number]) => rgbHex({ r: c[0], g: c[1], b: c[2], a: 1 });

const CROPS: { id: string; label: string; ratio: number | null }[] = [
  { id: "full", label: "Full", ratio: null },
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

  return (
    <>
      <Section
        id="clip-frame"
        title="Frame"
        icon="frame"
        aside={
          <Show when={isCustomFrame()}>
            <span class="badge">Custom</span>
          </Show>
        }
      >
        <div class="frame-cards">
          <For each={FRAMES}>
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
                  <span class="frame-card-shot" />
                </span>
                <span>{f.label}</span>
              </button>
            )}
          </For>
        </div>
        <Show when={ed.frameInfo()}>
          {(fi) => (
            <>
              <Field label="Padding">
                <Slider
                  value={fi().padding}
                  min={0}
                  max={0.2}
                  step={0.005}
                  label="Padding"
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={(v) => ed.applyFrameStyle({ padding: v })}
                />
              </Field>
              <Field label="Corners">
                <Slider
                  value={fi().corner_radius}
                  min={0}
                  max={0.08}
                  step={0.002}
                  label="Corner radius"
                  format={(v) => `${(v * 100).toFixed(1)}%`}
                  onInput={(v) => ed.applyFrameStyle({ radius: v })}
                />
              </Field>
              <Field label="Shadow">
                <Slider
                  value={fi().shadow}
                  min={0}
                  max={1}
                  step={0.05}
                  label="Shadow strength"
                  disabled={fi().padding <= 0}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={(v) => ed.applyFrameStyle({ shadow: v })}
                />
              </Field>
            </>
          )}
        </Show>
        <Show when={ed.framePreset() !== "none"}>
          <Field label="Backdrop" stack>
            <div class="swatches">
              <For each={ed.BG_SWATCHES}>
                {(sw) => (
                  <button
                    type="button"
                    class="swatch"
                    style={{ background: sw.css }}
                    aria-label={`Backdrop: ${sw.label}`}
                    aria-pressed={ed.bgPreset() === sw.name}
                    data-tip={sw.label}
                    onClick={() => ed.applyBackground(sw.name)}
                  />
                )}
              </For>
              <button
                type="button"
                class="swatch swatch-custom"
                aria-label="Custom backdrop"
                aria-pressed={ed.bgPreset() === ""}
                data-tip="Custom colors"
                onClick={() => {
                  const fi = ed.frameInfo();
                  if (fi && ed.bgPreset() !== "") ed.applyBackgroundCustom(bgHex(fi.bg_from), bgHex(fi.bg_to), fi.bg_angle);
                }}
              />
            </div>
          </Field>
          <Show when={ed.bgPreset() === "" && ed.frameInfo()}>
            {(fi) => (
              <div class="bg-custom">
                <Seg
                  full
                  value={fi().bg_kind}
                  onChange={(k) =>
                    ed.applyBackgroundCustom(bgHex(fi().bg_from), k === "gradient" ? bgHex(fi().bg_to) : null, fi().bg_angle)
                  }
                  options={[
                    { value: "solid", label: "Solid" },
                    { value: "gradient", label: "Gradient" },
                  ]}
                />
                <div class="bg-colors">
                  <label class="color-well" data-tip={fi().bg_kind === "gradient" ? "Start color" : "Color"}>
                    <span style={{ background: bgHex(fi().bg_from) }} />
                    <input
                      type="color"
                      value={bgHex(fi().bg_from)}
                      aria-label="Backdrop color"
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
                    <label class="color-well" data-tip="End color">
                      <span style={{ background: bgHex(fi().bg_to) }} />
                      <input
                        type="color"
                        value={bgHex(fi().bg_to)}
                        aria-label="Backdrop end color"
                        onInput={(e) => ed.applyBackgroundCustom(bgHex(fi().bg_from), e.currentTarget.value, fi().bg_angle)}
                      />
                    </label>
                    <Slider
                      value={fi().bg_angle}
                      min={0}
                      max={360}
                      step={5}
                      label="Gradient angle"
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

      <Section id="clip-crop" title="Crop" icon="crop">
        <div class="chip-row">
          <For each={CROPS}>
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
          data-tip="Drag a crop rectangle on the video"
          onClick={() => void ed.beginCropEdit()}
        >
          <Icon name="crop" size={14} /> Adjust on canvas…
        </button>
        <Show when={ed.crop()} fallback={<p class="note">Centered crops re-frame the whole clip. Annotations keep their on-screen spot.</p>}>
          <p class="note">
            Showing {Math.round(ed.crop()!.w * 100)}% × {Math.round(ed.crop()!.h * 100)}% of the recording.
          </p>
        </Show>
      </Section>

      <Section
        id="clip-camera"
        title="Camera"
        icon="zoomIn"
        aside={<span class="badge">{ed.zooms().length} zoom{ed.zooms().length === 1 ? "" : "s"}</span>}
      >
        <Field label="Auto zoom strength" stack>
          <Slider
            value={ed.zoomStrength()}
            min={1.2}
            max={3}
            step={0.1}
            label="Auto zoom strength"
            format={(v) => `${v.toFixed(1)}×`}
            onInput={(v) => ed.setZoomStrength(v)}
          />
        </Field>
        <div class="btn-row">
          <button
            type="button"
            class="btn sm grow"
            data-tip="Rebuild zooms from your recorded clicks. Ctrl+Z restores your own"
            onClick={() => void ed.planZoomAuto()}
          >
            <Icon name="sparkle" size={14} /> Auto zooms
          </button>
          <button type="button" class="btn sm grow" data-kbd="Z" data-tip="Add a zoom at the playhead" onClick={() => void ed.addZoomAt()}>
            <Icon name="plus" size={14} /> Zoom here
          </button>
        </div>
      </Section>

      <Section id="clip-pacing" title="Pacing" icon="speed">
        <Field label="Skim idle stretches" hint="Plays the moments where nothing happens faster">
          <Switch checked={ed.speed().length > 0} label="Skim idle stretches" onChange={() => ed.toggleSkim()} />
        </Field>
        <Field label="Skim speed" stack>
          <Seg
            full
            value={ed.skimFactor()}
            onChange={(v) => ed.setSkimFactor(v)}
            options={SKIMS.map((f) => ({ value: f, label: `${f}×` }))}
          />
        </Field>
        <div class="btn-row">
          <button type="button" class="btn sm grow" data-kbd="X" data-tip="Speed up 2s at the playhead" onClick={() => void ed.addSpeedAtPlayhead()}>
            <Icon name="speed" size={14} /> Speed up
          </button>
          <button type="button" class="btn sm grow" data-kbd="C" data-tip="Remove 1s at the playhead" onClick={() => void ed.addCutAtPlayhead()}>
            <Icon name="cut" size={14} /> Cut
          </button>
        </div>
        <Field label="Final length">
          <span class="readout">
            {outDur().toFixed(1)}s
            <Show when={Math.abs(outDur() - ed.duration()) > 0.05}>
              <span class="readout-sub"> of {ed.duration().toFixed(1)}s</span>
            </Show>
          </span>
        </Field>
      </Section>

      <Section id="clip-audio" title="Audio" icon="waves">
        <Show
          when={ed.audio.hasAudio()}
          fallback={
            <p class="note">
              This take is silent. Turn on the microphone or system sound before recording to add narration.
            </p>
          }
        >
          <Index each={ed.audio.tracks()}>
            {(t) => (
              <div class="audio-row" classList={{ muted: t().muted }}>
                <div class="audio-row-head">
                  <Icon name={t().kind === "mic" ? "mic" : "volume"} size={14} />
                  <span>{trackLabel(t().kind)}</span>
                  <IconButton
                    icon={t().muted ? "volumeOff" : "volume"}
                    tip={t().muted ? "Unmute" : "Mute"}
                    active={t().muted}
                    onClick={() => ed.audio.toggleMute(t().kind)}
                  />
                </div>
                <Slider
                  value={t().gain}
                  min={0}
                  max={2}
                  step={0.05}
                  disabled={t().muted}
                  label={`${trackLabel(t().kind)} volume`}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onInput={(v) => void ed.audio.setGain(t().kind, v)}
                />
                <Show when={t().kind === "mic"}>
                  <Field label="Remove noise" hint="Filters out fans, hum and hiss while keeping your voice">
                    <Switch
                      checked={!!t().denoise}
                      label="Remove noise"
                      onChange={(v) => void ed.audio.setDenoise(t().kind, v)}
                    />
                  </Field>
                  <Field label="Even out volume" hint="Brings quiet and loud passages to one steady level">
                    <Switch
                      checked={!!t().level}
                      label="Even out volume"
                      onChange={(v) => void ed.audio.setLevel(t().kind, v)}
                    />
                  </Field>
                  <Show when={ed.audio.cleaning()[t().kind]}>
                    <p class="note audio-cleaning" role="status">
                      Cleaning up the voice…
                    </p>
                  </Show>
                </Show>
              </div>
            )}
          </Index>
          <p class="note">Sped-up stretches play silent and cuts remove their sound. MP4 carries the audio; GIFs are always silent.</p>
        </Show>
      </Section>

      <Section id="clip-pointer" title="Pointer" icon="cursor">
        <Field label="Smooth pointer" hint="A clean pointer that glides, drawn from your movements">
          <Switch checked={!!ed.cursorStyle()} label="Smooth pointer" onChange={() => ed.toggleCursor()} />
        </Field>
        <Show when={ed.cursorStyle()}>
          {(c) => (
            <>
              <Field label="Size" stack>
                <Slider
                  value={c().size}
                  min={0.5}
                  max={3}
                  step={0.05}
                  label="Pointer size"
                  format={(v) => `${v.toFixed(1)}×`}
                  onInput={ed.setCursorSize}
                />
              </Field>
              <Field label="Smoothing" stack>
                <Slider
                  value={c().smoothing}
                  min={0}
                  max={0.2}
                  step={0.01}
                  label="Pointer smoothing"
                  format={(v) => (v === 0 ? "Off" : v < 0.04 ? "Light" : v < 0.1 ? "Medium" : "Heavy")}
                  onInput={ed.setCursorSmoothing}
                />
              </Field>
              <Field label="Hide when still" hint="Fades out while the mouse rests and is back before it moves">
                <Switch
                  checked={!!c().hide_idle}
                  label="Hide pointer when still"
                  onChange={(v) => ed.setCursorHideIdle(v)}
                />
              </Field>
            </>
          )}
        </Show>
        <Show when={ed.cursorStyle() && ed.pointerCaptured()}>
          <p class="note">
            The real pointer is also in this take, so two show. Set the pointer to Smooth before recording for a
            clean result.
          </p>
        </Show>
        <Show when={!ed.cursorStyle() && !ed.pointerCaptured()}>
          <p class="note">This take has no visible pointer. Turn on the smooth pointer to show one.</p>
        </Show>
      </Section>

      <Section id="clip-overlays" title="Overlays" icon="clicks">
        <Field label="Click ripples" hint="An expanding ring at every recorded click">
          <Switch checked={ed.showClicks()} label="Click ripples" onChange={() => ed.toggleClicks()} />
        </Field>
        <Field label="Keystrokes" hint="Shortcuts you pressed appear as chips. Plain typing never shows">
          <Switch checked={ed.showKeys()} label="Keystrokes" onChange={() => ed.toggleKeys()} />
        </Field>
      </Section>

      <Section id="clip-info" title="Clip" icon="info" defaultOpen={false}>
        <dl class="facts">
          <dt>Recorded</dt>
          <dd>{ed.duration().toFixed(2)}s</dd>
          <dt>Exports as</dt>
          <dd>{outDur().toFixed(2)}s</dd>
          <dt>Aspect</dt>
          <dd>{ed.frameAspect().toFixed(3)}</dd>
          <dt>Zooms</dt>
          <dd>{ed.zooms().length}</dd>
          <dt>Annotations</dt>
          <dd>{ed.anns().texts.length + ed.anns().arrows.length + ed.anns().highlights.length}</dd>
          <dt>Cuts</dt>
          <dd>{ed.cuts().length}</dd>
        </dl>
      </Section>
    </>
  );
}
