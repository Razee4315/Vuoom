// The timeline panel: transport + insert tools in the header, then a ruler and three
// colour-coded tracks (camera zooms, annotations, speed & cuts) with named track headers.
// Resizable from its top edge, collapsible to just the header, zoomable with Ctrl+wheel.
import { For, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { isSwallowed } from "../editor/constants";
import { fmt, fmtT } from "../format";
import { outputDuration } from "../geometry";
import { Icon } from "../icons";
import { layout, prefs, setTimelineH } from "../prefs";
import { IconButton } from "../ui";

export default function Timeline() {
  const ed = useEditor();
  const open = () => layout.timelineOpen();
  const w = (s: number, e: number) => `${Math.max(ed.pct(e) - ed.pct(s), 0.18)}%`;
  const outDur = () => outputDuration(ed.duration(), ed.trim(), ed.speed(), ed.cuts());
  const noteLanes = () => (layout.compactNotes() ? 1 : Math.max(1, ed.annBars().length));

  let resizing = false;
  let startY = 0;
  let startH = 0;
  const onResizeDown = (e: PointerEvent) => {
    resizing = true;
    startY = e.clientY;
    startH = layout.timelineH();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    document.body.classList.add("resizing-y");
  };
  const onResizeMove = (e: PointerEvent) => {
    if (resizing) setTimelineH(startH + (startY - e.clientY));
  };
  const onResizeUp = () => {
    resizing = false;
    document.body.classList.remove("resizing-y");
  };

  return (
    <section
      class="timeline panel"
      classList={{ collapsed: !open() }}
      style={{ height: open() ? `${layout.timelineH()}px` : undefined }}
      aria-label="Timeline"
    >
      <Show when={open()}>
        <div
          class="resizer resizer-y"
        aria-hidden="true"
          onPointerDown={onResizeDown}
          onPointerMove={onResizeMove}
          onPointerUp={onResizeUp}
          onDblClick={() => layout.timelineH.reset()}
        />
      </Show>

      <header class="tl-head">
        <div class="tl-transport">
          <IconButton icon="toStart" tip="Back to start" kbd="Home" onClick={ed.restart} />
          <button
            type="button"
            class="tl-play"
            aria-label={ed.playing() ? "Pause" : "Play"}
            data-tip={ed.playing() ? "Pause" : "Play"}
            data-kbd="Space"
            onClick={ed.togglePlay}
          >
            <Icon name={ed.playing() ? "pause" : "play"} size={15} />
          </button>
          <IconButton
            icon="loop"
            tip="Loop playback"
            active={ed.looping()}
            onClick={() => {
              ed.setLooping(!ed.looping());
              prefs.loop.set(ed.looping());
            }}
          />
          <div class="tl-time" aria-live="off">
            <span class="tl-time-now">{fmtT(ed.playhead())}</span>
            <span class="tl-time-sep">/</span>
            <span>{fmt(ed.duration())}</span>
            <Show when={ed.trim() || ed.speed().length > 0 || ed.cuts().length > 0}>
              <span class="tl-time-out" data-tip="Final length after trim, speed-ups and cuts">
                <Icon name="chevronRight" size={11} />
                {outDur().toFixed(1)}s
              </span>
            </Show>
          </div>
        </div>

        <div class="tl-insert">
          <button type="button" class="tl-ins zoom" data-tip="Add a zoom at the playhead" data-kbd="Z" onClick={() => void ed.addZoomAt()}>
            <Icon name="zoomIn" size={14} /> Zoom
          </button>
          <button type="button" class="tl-ins speed" data-tip={`Speed up 2s at ${ed.skimFactor()}×`} data-kbd="X" onClick={() => void ed.addSpeedAtPlayhead()}>
            <Icon name="speed" size={14} /> Speed
          </button>
          <button type="button" class="tl-ins cut" data-tip="Remove 1s at the playhead" data-kbd="C" onClick={() => void ed.addCutAtPlayhead()}>
            <Icon name="cut" size={14} /> Cut
          </button>
        </div>

        <div class="tl-tools">
          <IconButton
            icon="magnet"
            tip={prefs.snapping() ? "Snapping on (hold Alt to bypass)" : "Snapping off"}
            active={prefs.snapping()}
            onClick={() => prefs.snapping.set(!prefs.snapping())}
          />
          <IconButton
            icon={layout.compactNotes() ? "expand" : "compact"}
            tip={layout.compactNotes() ? "One lane per annotation" : "All annotations on one lane"}
            active={layout.compactNotes()}
            onClick={() => layout.compactNotes.set(!layout.compactNotes())}
          />
          <span class="tl-sep" />
          <IconButton icon="minus" tip="Zoom timeline out" disabled={ed.tlScale() === null} onClick={() => ed.zoomTimeline(-1)} />
          <button
            type="button"
            class="tl-fit"
            classList={{ on: ed.tlScale() === null }}
            data-tip="Fit the whole clip"
            onClick={() => ed.setTlScale(null)}
          >
            Fit
          </button>
          <IconButton icon="plus" tip="Zoom timeline in (Ctrl+scroll)" onClick={() => ed.zoomTimeline(1)} />
          <span class="tl-sep" />
          <IconButton
            icon={open() ? "chevronDown" : "chevronUp"}
            tip={open() ? "Collapse timeline" : "Expand timeline"}
            kbd="Ctrl+3"
            onClick={() => layout.timelineOpen.set(!open())}
          />
        </div>
      </header>

      <Show when={open()}>
        <div class="tl-body">
          <div class="tl-heads" aria-hidden="true">
            <div class="tl-headcell ruler" />
            <div class="tl-headcell zoom">
              <span class="tl-swatch" />
              <span>Camera</span>
              <span class="tl-count">{ed.zooms().length}</span>
            </div>
            <div class="tl-headcell notes" style={{ height: `calc(var(--lane-h) * ${noteLanes()})` }}>
              <span class="tl-swatch" />
              <span>Notes</span>
              <span class="tl-count">{ed.annBars().length}</span>
            </div>
            <div class="tl-headcell effects">
              <span class="tl-swatch" />
              <span>Speed · Cuts</span>
            </div>
          </div>

          <div
            class="tl"
            ref={ed.refs.timeline}
            onPointerDown={ed.onTlDown}
            onPointerMove={ed.frameTl(ed.onTlPointerMove)}
            onPointerCancel={ed.onTlCancel}
            onPointerUp={ed.onTlUp}
            onPointerLeave={ed.onTlHoverLeave}
          >
            <div class="tl-scroll" ref={ed.refs.tlScroll} onScroll={ed.invalidateTlRect}>
              <div class="tl-inner" ref={ed.refs.tlTrack} style={{ width: ed.trackWidth() }}>
                <div class="tl-ruler">
                  <For each={ed.tickMarks()}>
                    {(m) => (
                      <span class="tl-tick" classList={{ major: m.major, mid: m.mid }} style={{ left: `${ed.pct(m.t)}%` }}>
                        <i />
                        <Show when={m.major}>
                          <b>{fmt(m.t)}</b>
                        </Show>
                      </span>
                    )}
                  </For>
                </div>

                {/* Camera: zoom blocks. Click empty lane space to add one. */}
                <div class="tl-lane tl-zoomlane" onPointerLeave={() => ed.setGhostT(null)}>
                  <Show when={ed.zooms().length === 0 && prefs.hints()}>
                    <div class="tl-lanehint">Click anywhere in this lane to zoom in at that moment</div>
                  </Show>
                  <Show when={ed.ghostT() !== null && !ed.zoomDrag() && !ed.speedDrag() && !ed.cutDrag()}>
                    {(() => {
                      const gw = () => (ed.duration() > 0 ? Math.min(100, (1.6 / ed.duration()) * 100) : 10);
                      const left = () => Math.max(0, Math.min(100 - gw(), ed.pct(ed.ghostT()!) - gw() / 2));
                      return (
                        <div class="tl-ghost" style={{ left: `${left()}%`, width: `${gw()}%` }}>
                          <Icon name="plus" size={10} stroke={2.5} /> Zoom
                        </div>
                      );
                    })()}
                  </Show>
                  <For each={ed.zooms()}>
                    {(z, i) => {
                      const g = () => ed.zoomGeom(i(), z);
                      const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                      return (
                        <button
                          type="button"
                          class="tl-seg tl-zoom"
                          classList={{ selected: ed.selZoom() === i(), swallowed: gone() }}
                          style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                          aria-label={`Zoom ${z.amount.toFixed(1)} times, ${z.start.toFixed(1)} to ${z.end.toFixed(1)} seconds`}
                          aria-pressed={ed.selZoom() === i()}
                          data-tip={gone() ? "Hidden by a cut, never exported" : undefined}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              ed.setSelZoom(i());
                              ed.scrub(g().start);
                            }
                          }}
                          onPointerDown={ed.onZoomDown(i(), z, "move")}
                          onPointerMove={ed.frameZoom(ed.onZoomMove)}
                          onPointerUp={() => void ed.onZoomUp()}
                          onLostPointerCapture={() => void ed.onZoomUp()}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onZoomDown(i(), z, "l")} onPointerMove={ed.frameZoom(ed.onZoomMove)} onPointerUp={() => void ed.onZoomUp()} onLostPointerCapture={() => void ed.onZoomUp()} />
                          <Icon name="zoomIn" size={11} />
                          <span class="tl-seg-label">{z.amount.toFixed(1)}×</span>
                          <Show when={typeof z.mode === "object"}>
                            <Icon name="target" size={10} />
                          </Show>
                          <span class="tl-handle r" onPointerDown={ed.onZoomDown(i(), z, "r")} onPointerMove={ed.frameZoom(ed.onZoomMove)} onPointerUp={() => void ed.onZoomUp()} onLostPointerCapture={() => void ed.onZoomUp()} />
                        </button>
                      );
                    }}
                  </For>
                </div>

                {/* Notes: one lane per annotation (or all on one lane in compact mode). */}
                <div class="tl-notes" style={{ height: `calc(var(--lane-h) * ${noteLanes()})` }}>
                  <Show when={ed.annBars().length === 0 && prefs.hints()}>
                    <div class="tl-lanehint">Text, arrows and boxes you draw get a bar here</div>
                  </Show>
                  <For each={ed.annBars()}>
                    {(b, i) => {
                      const g = () => ed.annGeom(b);
                      const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                      return (
                        <button
                          type="button"
                          class={`tl-seg tl-ann tl-${b.kind}`}
                          classList={{ selected: ed.isSelected(b.kind, b.id), swallowed: gone() }}
                          style={{
                            left: `${ed.pct(g().start)}%`,
                            width: w(g().start, g().end),
                            top: `calc(var(--lane-h) * ${layout.compactNotes() ? 0 : i()} + 3px)`,
                          }}
                          data-tip={gone() ? "Hidden by a cut, never exported" : undefined}
                          onPointerDown={ed.onAnnDown(b, "move")}
                          onPointerMove={ed.frameAnn(ed.onAnnMove)}
                          onPointerUp={() => void ed.onAnnUp()}
                          onLostPointerCapture={() => void ed.onAnnUp()}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onAnnDown(b, "l")} onPointerMove={ed.frameAnn(ed.onAnnMove)} onPointerUp={() => void ed.onAnnUp()} onLostPointerCapture={() => void ed.onAnnUp()} />
                          <Icon name={b.kind === "text" ? "text" : b.kind === "arrow" ? "arrow" : "shape"} size={11} />
                          <span class="tl-seg-label">{b.label}</span>
                          <span class="tl-handle r" onPointerDown={ed.onAnnDown(b, "r")} onPointerMove={ed.frameAnn(ed.onAnnMove)} onPointerUp={() => void ed.onAnnUp()} onLostPointerCapture={() => void ed.onAnnUp()} />
                        </button>
                      );
                    }}
                  </For>
                </div>

                {/* Speed + cuts: amber means faster, striped red means removed. */}
                <div class="tl-lane tl-effects">
                  <Show when={ed.speed().length === 0 && ed.cuts().length === 0 && prefs.hints()}>
                    <div class="tl-lanehint">Speed-ups and cuts land here. Try Skim idle in the Clip panel</div>
                  </Show>
                  <For each={ed.speed()}>
                    {(r, i) => {
                      const g = () => ed.speedGeom(i(), r);
                      const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                      return (
                        <div
                          class="tl-seg tl-speed"
                          classList={{ selected: ed.selSpeed() === i(), swallowed: gone() }}
                          style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onSpeedDown(i(), r, "l")} onPointerMove={ed.frameSpeed(ed.onSpeedMove)} onPointerUp={() => void ed.onSpeedUp()} onLostPointerCapture={() => void ed.onSpeedUp()} />
                          <button
                            type="button"
                            class="tl-seg-body"
                            aria-label={`Plays at ${r.factor} times`}
                            onPointerDown={ed.onSpeedDown(i(), r, "move")}
                            onPointerMove={ed.frameSpeed(ed.onSpeedMove)}
                            onPointerUp={() => void ed.onSpeedUp()}
                            onLostPointerCapture={() => void ed.onSpeedUp()}
                          >
                            <Icon name="speed" size={11} />
                            <span class="tl-seg-label">{r.factor}×</span>
                          </button>
                          <span class="tl-handle r" onPointerDown={ed.onSpeedDown(i(), r, "r")} onPointerMove={ed.frameSpeed(ed.onSpeedMove)} onPointerUp={() => void ed.onSpeedUp()} onLostPointerCapture={() => void ed.onSpeedUp()} />
                        </div>
                      );
                    }}
                  </For>
                  <For each={ed.cuts()}>
                    {(c, i) => {
                      const g = () => ed.cutGeom(i(), c);
                      return (
                        <div
                          class="tl-seg tl-cut"
                          classList={{ selected: ed.selCut() === i() }}
                          style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onCutDown(i(), c, "l")} onPointerMove={ed.frameCut(ed.onCutMove)} onPointerUp={() => void ed.onCutUp()} onLostPointerCapture={() => void ed.onCutUp()} />
                          <button
                            type="button"
                            class="tl-seg-body"
                            aria-label="Removed section"
                            data-tip="Removed from the export. Delete restores it"
                            onPointerDown={ed.onCutDown(i(), c, "move")}
                            onPointerMove={ed.frameCut(ed.onCutMove)}
                            onPointerUp={() => void ed.onCutUp()}
                            onLostPointerCapture={() => void ed.onCutUp()}
                          >
                            <Icon name="cut" size={11} />
                            <span class="tl-seg-label">Cut</span>
                          </button>
                          <span class="tl-handle r" onPointerDown={ed.onCutDown(i(), c, "r")} onPointerMove={ed.frameCut(ed.onCutMove)} onPointerUp={() => void ed.onCutUp()} onLostPointerCapture={() => void ed.onCutUp()} />
                        </div>
                      );
                    }}
                  </For>
                </div>

                {/* Trim: shaded outside, draggable in/out handles. */}
                <div class="tl-shade" style={{ left: "0", width: `${ed.pct(ed.tStart())}%` }} />
                <div class="tl-shade" style={{ left: `${ed.pct(ed.tEnd())}%`, right: "0" }} />
                <div
                  class="tl-trim start"
                  style={{ left: `${ed.pct(ed.tStart())}%` }}
                  data-tip="Trim start"
                  onPointerDown={ed.onTrimDown("start")}
                  onPointerMove={ed.frameTrim(ed.onTrimMove)}
                  onPointerUp={() => void ed.onTrimUp()}
                  onLostPointerCapture={() => void ed.onTrimUp()}
                />
                <div
                  class="tl-trim end"
                  style={{ left: `${ed.pct(ed.tEnd())}%` }}
                  data-tip="Trim end"
                  onPointerDown={ed.onTrimDown("end")}
                  onPointerMove={ed.frameTrim(ed.onTrimMove)}
                  onPointerUp={() => void ed.onTrimUp()}
                  onLostPointerCapture={() => void ed.onTrimUp()}
                />

                <Show when={ed.snapLine() !== null}>
                  <div class="tl-snapline" style={{ left: `${ed.pct(ed.snapLine()!)}%` }} />
                </Show>
                <Show when={ed.hoverT() !== null}>
                  <div class="tl-hoverline" style={{ left: `${ed.pct(ed.hoverT()!)}%` }}>
                    <span class="tl-hoverchip">{fmtT(ed.hoverT()!)}</span>
                  </div>
                </Show>
                <div class="tl-playhead" style={{ left: `${ed.pct(ed.playhead())}%` }}>
                  <i />
                </div>
              </div>
            </div>
          </div>
        </div>
      </Show>
    </section>
  );
}
