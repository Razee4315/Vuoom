// The timeline panel: transport + insert tools in the header, then a ruler and three
// colour-coded tracks (camera zooms, annotations, speed & cuts) with named track headers.
// Resizable from its top edge, collapsible to just the header, zoomable with Ctrl+wheel.
import { For, Index, Show } from "solid-js";
import { useEditor } from "../editor/context";
import { isSwallowed } from "../editor/constants";
import { fmt, fmtT } from "../format";
import { outputDuration } from "../geometry";
import { t } from "../i18n";
import { Icon } from "../icons";
import { layout, prefs, setTimelineH } from "../prefs";
import { trackLabel } from "../editor/audio";
import { IconButton, Menu } from "../ui";
import { annBarMenu, captionMenu, cutMenu, speedMenu, timelineMenu, zoomMenu } from "./contextMenus";
import Waveform from "./Waveform";

export default function Timeline() {
  const ed = useEditor();
  const open = () => layout.timelineOpen();
  const w = (s: number, e: number) => `${Math.max(ed.pct(e) - ed.pct(s), 0.18)}%`;
  const outDur = () => outputDuration(ed.duration(), ed.trim(), ed.speed(), ed.cuts());
  const trackPx = () => (ed.tlScale() ? ed.duration() * ed.tlScale()! : ed.tlWidth());
  const isNarrow = (s: number, e: number) => ((ed.pct(e) - ed.pct(s)) / 100) * Math.max(trackPx(), 200) < 42;
  // As many thumbnails as fit at their real aspect across the (possibly zoomed) track,
  // each picked from the fetched set at its slot's moment.
  const filmSlots = () => {
    const all = ed.thumbs();
    if (all.length === 0) return [];
    const slotPx = 32 * Math.max(0.3, ed.frameAspect());
    const n = Math.max(1, Math.min(all.length, Math.floor(trackPx() / slotPx)));
    return Array.from({ length: n }, (_, j) => all[Math.min(all.length - 1, Math.floor(((j + 0.5) / n) * all.length))]);
  };
  const noteLanes = () => (layout.compactNotes() ? 1 : Math.max(1, ed.annBars().length));

  const onTrimKey = (which: "start" | "end") => (e: KeyboardEvent) => {
    const dur = ed.duration();
    if (dur <= 0) return;
    const step = e.shiftKey ? 1 : 0.1;
    let next: number | null = null;
    const cur = which === "start" ? ed.tStart() : ed.tEnd();
    if (e.key === "ArrowLeft") next = cur - step;
    else if (e.key === "ArrowRight") next = cur + step;
    else if (e.key === "Home") next = which === "start" ? 0 : Math.min(dur, ed.tStart() + 0.2);
    else if (e.key === "End") next = which === "end" ? dur : Math.max(0, ed.tEnd() - 0.2);
    if (next === null) return;
    e.preventDefault();
    e.stopPropagation();
    if (which === "start") {
      const s = Math.max(0, Math.min(next, ed.tEnd() - 0.2));
      void ed.commitTrim(s, ed.tEnd());
      ed.scrub(s);
    } else {
      const en = Math.min(dur, Math.max(next, ed.tStart() + 0.2));
      void ed.commitTrim(ed.tStart(), en);
      ed.scrub(en);
    }
  };

  const onPlayheadKey = (e: KeyboardEvent) => {
    const dur = ed.duration();
    if (dur <= 0) return;
    const step = e.shiftKey ? 1 : 0.1;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      ed.scrub(Math.max(ed.tStart(), ed.playhead() - step));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      ed.scrub(Math.min(ed.tEnd(), ed.playhead() + step));
    } else if (e.key === "Home") {
      e.preventDefault();
      e.stopPropagation();
      ed.scrub(ed.tStart());
    } else if (e.key === "End") {
      e.preventDefault();
      e.stopPropagation();
      ed.scrub(ed.tEnd());
    }
  };

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
      aria-label={t("Timeline")}
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
          <IconButton icon="toStart" tip={t("Back to start")} kbd="Home" onClick={ed.restart} />
          <button
            type="button"
            class="tl-play"
            aria-label={ed.playing() ? t("Pause") : t("Play")}
            data-tip={ed.playing() ? t("Pause") : t("Play")}
            data-kbd="Space"
            onClick={ed.togglePlay}
          >
            <Icon name={ed.playing() ? "pause" : "play"} size={15} />
          </button>
          <IconButton
            icon="loop"
            tip={t("Loop playback")}
            active={ed.looping()}
            onClick={() => {
              ed.setLooping(!ed.looping());
              prefs.loop.set(ed.looping());
            }}
          />
          <Menu
            items={() =>
              [0.25, 0.5, 1, 1.5, 2].map((r) => ({
                label: r === 1 ? t("Normal speed") : `${r}×`,
                checked: prefs.previewRate() === r,
                onSelect: () => prefs.previewRate.set(r),
              }))
            }
            trigger={(m) => (
              <button
                type="button"
                class="tl-rate"
                classList={{ on: m.open, off: prefs.previewRate() !== 1 }}
                ref={m.ref}
                data-tip={t("Preview speed (export is unaffected)")}
                onClick={m.toggle}
              >
                {prefs.previewRate()}×
              </button>
            )}
          />
          <div class="tl-time" aria-live="off">
            <span class="tl-time-now">{fmtT(ed.playhead())}</span>
            <span class="tl-time-sep">/</span>
            <span>{fmt(ed.duration())}</span>
            <Show when={ed.trim() || ed.speed().length > 0 || ed.cuts().length > 0}>
              <span class="tl-time-out" data-tip={t("Final length after trim, speed-ups and cuts")}>
                <Icon name="chevronRight" size={11} />
                {outDur().toFixed(1)}s
              </span>
            </Show>
          </div>
        </div>

        <div class="tl-insert">
          <div class="tl-insert-inline">
            <button type="button" class="tl-ins zoom" data-tip={t("Add a zoom at the playhead. To aim it, use the Zoom tool (Z)")} onClick={() => void ed.addZoomAt()}>
              <Icon name="zoomIn" size={14} /> <span>{t("Zoom")}</span>
            </button>
            <button type="button" class="tl-ins speed" data-tip={t("Speed up 2s at {factor}×", { factor: ed.skimFactor() })} data-kbd="X" onClick={() => void ed.addSpeedAtPlayhead()}>
              <Icon name="speed" size={14} /> <span>{t("Speed")}</span>
            </button>
            <button type="button" class="tl-ins cut" data-tip={t("Remove 1s at the playhead")} data-kbd="C" onClick={() => void ed.addCutAtPlayhead()}>
              <Icon name="cut" size={14} /> <span>{t("Cut")}</span>
            </button>
          </div>
          <div class="tl-insert-compact">
            <Menu
              items={() => [
                { label: t("Zoom"), icon: "zoomIn", kbd: "Z", onSelect: () => void ed.addZoomAt() },
                { label: t("Speed"), icon: "speed", kbd: "X", onSelect: () => void ed.addSpeedAtPlayhead() },
                { label: t("Cut"), icon: "cut", kbd: "C", onSelect: () => void ed.addCutAtPlayhead() },
              ]}
              trigger={(m) => (
                <button
                  type="button"
                  class="tl-ins tl-ins-menu"
                  classList={{ on: m.open }}
                  ref={m.ref}
                  aria-label={t("Insert")}
                  aria-expanded={m.open}
                  onClick={m.toggle}
                >
                  <Icon name="plus" size={14} /> <span>{t("Insert")}</span>
                </button>
              )}
            />
          </div>
        </div>

        <div class="tl-tools">
          <div class="tl-view-inline">
            <IconButton
              icon="magnet"
              tip={prefs.snapping() ? t("Snapping on (hold Alt to bypass)") : t("Snapping off")}
              active={prefs.snapping()}
              onClick={() => prefs.snapping.set(!prefs.snapping())}
            />
            <IconButton
              icon="film"
              tip={layout.filmstrip() ? t("Hide frame thumbnails") : t("Show frame thumbnails")}
              active={layout.filmstrip()}
              onClick={() => layout.filmstrip.set(!layout.filmstrip())}
            />
            <IconButton
              icon={layout.compactNotes() ? "expand" : "compact"}
              tip={layout.compactNotes() ? t("One lane per annotation") : t("All annotations on one lane")}
              active={layout.compactNotes()}
              onClick={() => layout.compactNotes.set(!layout.compactNotes())}
            />
          </div>
          <div class="tl-view-compact">
            <Menu
              align="end"
              items={() => [
                {
                  label: t("Magnetic snapping"),
                  icon: "magnet",
                  checked: prefs.snapping(),
                  onSelect: () => prefs.snapping.set(!prefs.snapping()),
                },
                {
                  label: t("Frame thumbnails"),
                  icon: "film",
                  checked: layout.filmstrip(),
                  onSelect: () => layout.filmstrip.set(!layout.filmstrip()),
                },
                {
                  label: t("All annotations on one lane"),
                  icon: layout.compactNotes() ? "expand" : "compact",
                  checked: layout.compactNotes(),
                  onSelect: () => layout.compactNotes.set(!layout.compactNotes()),
                },
                { separator: true },
                { heading: t("Preview speed") },
                ...[0.25, 0.5, 1, 1.5, 2].map((r) => ({
                  label: r === 1 ? t("Normal speed") : `${r}×`,
                  checked: prefs.previewRate() === r,
                  onSelect: () => prefs.previewRate.set(r),
                })),
              ]}
              trigger={(m) => (
                <IconButton
                  icon="sliders"
                  tip={t("Timeline view")}
                  active={m.open}
                  ref={m.ref}
                  onClick={m.toggle}
                />
              )}
            />
          </div>
          <span class="tl-sep" />
          <IconButton icon="minus" tip={t("Zoom timeline out")} disabled={ed.tlScale() === null} onClick={() => ed.zoomTimeline(-1)} />
          <button
            type="button"
            class="tl-fit"
            classList={{ on: ed.tlScale() === null }}
            data-tip={t("Fit the whole clip")}
            onClick={() => ed.setTlScale(null)}
          >
            {t("Fit")}
          </button>
          <IconButton icon="plus" tip={t("Zoom timeline in (Ctrl+scroll)")} onClick={() => ed.zoomTimeline(1)} />
          <span class="tl-sep" />
          <IconButton
            icon={open() ? "chevronDown" : "chevronUp"}
            tip={open() ? t("Collapse timeline") : t("Expand timeline")}
            kbd="Ctrl+3"
            onClick={() => layout.timelineOpen.set(!open())}
          />
        </div>
      </header>

      <Show when={open()}>
        <div class="tl-body">
          <div class="tl-heads">
            <div class="tl-headcell ruler" aria-hidden="true" />
            <Show when={layout.filmstrip()}>
              <div class="tl-headcell film" aria-hidden="true">
                <Icon name="film" size={12} />
                <span>{t("Frames")}</span>
              </div>
            </Show>
            <div class="tl-headcell zoom" aria-hidden="true">
              <span class="tl-swatch" />
              <span>{t("Zooms")}</span>
              <span class="tl-count">{ed.zooms().length}</span>
            </div>
            <div class="tl-headcell notes" aria-hidden="true" style={{ height: `calc(var(--lane-h) * ${noteLanes()})` }}>
              <span class="tl-swatch" />
              <span>{t("Notes")}</span>
              <span class="tl-count">{ed.annBars().length}</span>
            </div>
            <div class="tl-headcell effects" aria-hidden="true">
              <span class="tl-swatch" />
              <span>{t("Speed · Cuts")}</span>
            </div>
            <Show when={ed.captions.list().length > 0}>
              <div class="tl-headcell captions" aria-hidden="true">
                <span class="tl-swatch" />
                <span>{t("Captions")}</span>
                <span class="tl-count">{ed.captions.list().length}</span>
              </div>
            </Show>
            <Index each={ed.audio.tracks()}>
              {(track) => (
                <div class="tl-headcell audio" classList={{ muted: track().muted }}>
                  <button
                    type="button"
                    class="tl-mute"
                    aria-pressed={track().muted}
                    aria-label={track().muted ? t("Unmute {track}", { track: trackLabel(track().kind) }) : t("Mute {track}", { track: trackLabel(track().kind) })}
                    data-tip={track().muted ? t("Unmute") : t("Mute")}
                    onClick={() => ed.audio.toggleMute(track().kind)}
                  >
                    <Icon name={track().muted ? "volumeOff" : track().kind === "mic" ? "mic" : "volume"} size={12} />
                  </button>
                  <span>{track().kind === "mic" ? t("Mic") : t("System")}</span>
                </div>
              )}
            </Index>
          </div>

          <div
            class="tl"
            ref={ed.refs.timeline}
            onPointerDown={ed.onTlDown}
            onPointerMove={ed.frameTl(ed.onTlPointerMove)}
            onPointerCancel={ed.onTlCancel}
            onPointerUp={ed.onTlUp}
            onPointerLeave={ed.onTlHoverLeave}
            onContextMenu={(e) => timelineMenu(ed, e)}
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

                <Show when={layout.filmstrip()}>
                  <div class="tl-film">
                    <For each={filmSlots()}>
                      {(src, k) => (
                        <img
                          src={src}
                          alt=""
                          draggable={false}
                          style={{
                            left: `${(k() / filmSlots().length) * 100}%`,
                            width: `${100 / filmSlots().length}%`,
                          }}
                        />
                      )}
                    </For>
                  </div>
                </Show>

                {/* Camera: zoom blocks. Click empty lane space to add one. */}
                <div class="tl-lane tl-zoomlane" onPointerLeave={() => ed.setGhostT(null)}>
                  <Show when={ed.zooms().length === 0 && prefs.hints()}>
                    <div class="tl-lanehint">{t("Click anywhere in this lane to zoom in at that moment")}</div>
                  </Show>
                  <Show when={ed.ghostT() !== null && !ed.zoomDrag() && !ed.speedDrag() && !ed.cutDrag()}>
                    {(() => {
                      const gw = () => (ed.duration() > 0 ? Math.min(100, (1.6 / ed.duration()) * 100) : 10);
                      const left = () => Math.max(0, Math.min(100 - gw(), ed.pct(ed.ghostT()!) - gw() / 2));
                      return (
                        <div class="tl-ghost" style={{ left: `${left()}%`, width: `${gw()}%` }}>
                          <Icon name="plus" size={10} stroke={2.5} /> {t("Zoom")}
                        </div>
                      );
                    })()}
                  </Show>
                  <For each={ed.zooms()}>
                    {(z, i) => {
                      const g = () => ed.zoomGeom(i(), z);
                      const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                      return (
                        <div
                          class="tl-seg tl-zoom"
                          classList={{ selected: ed.selZoom() === i(), swallowed: gone(), narrow: isNarrow(g().start, g().end) }}
                          style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onZoomDown(i(), z, "l")} onPointerMove={ed.frameZoom(ed.onZoomMove)} onPointerUp={() => void ed.onZoomUp()} onLostPointerCapture={() => void ed.onZoomUp()} />
                          <button
                            type="button"
                            class="tl-seg-body"
                            aria-label={t("Zoom {amount} times, {start} to {end} seconds", { amount: z.amount.toFixed(1), start: z.start.toFixed(1), end: z.end.toFixed(1) })}
                            aria-pressed={ed.selZoom() === i()}
                            data-tip={gone() ? t("Hidden by a cut, never exported") : undefined}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                e.stopPropagation();
                                ed.selectSegment("zoom", i());
                                ed.scrub(g().start);
                              }
                            }}
                            onContextMenu={(e) => zoomMenu(ed, e, i())}
                            onPointerDown={ed.onZoomDown(i(), z, "move")}
                            onPointerMove={ed.frameZoom(ed.onZoomMove)}
                            onPointerUp={() => void ed.onZoomUp()}
                            onLostPointerCapture={() => void ed.onZoomUp()}
                          >
                            <Icon name="zoomIn" size={11} />
                            <span class="tl-seg-label">{z.amount.toFixed(1)}×</span>
                            <Show when={typeof z.mode === "object"}>
                              <Icon name="target" size={10} />
                            </Show>
                          </button>
                          <span class="tl-handle r" onPointerDown={ed.onZoomDown(i(), z, "r")} onPointerMove={ed.frameZoom(ed.onZoomMove)} onPointerUp={() => void ed.onZoomUp()} onLostPointerCapture={() => void ed.onZoomUp()} />
                        </div>
                      );
                    }}
                  </For>
                </div>

                {/* Notes: one lane per annotation (or all on one lane in compact mode). */}
                <div class="tl-notes" style={{ height: `calc(var(--lane-h) * ${noteLanes()})` }}>
                  <Show when={ed.annBars().length === 0 && prefs.hints()}>
                    <div class="tl-lanehint">{t("Text, arrows and boxes you draw get a bar here")}</div>
                  </Show>
                  <For each={ed.annBars()}>
                    {(b, i) => {
                      const g = () => ed.annGeom(b);
                      const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                      return (
                        <div
                          class={`tl-seg tl-ann tl-${b.kind}`}
                          classList={{ selected: ed.isSelected(b.kind, b.id), swallowed: gone(), narrow: isNarrow(g().start, g().end) }}
                          style={{
                            left: `${ed.pct(g().start)}%`,
                            width: w(g().start, g().end),
                            top: `calc(var(--lane-h) * ${layout.compactNotes() ? 0 : i()} + 2px)`,
                          }}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onAnnDown(b, "l")} onPointerMove={ed.frameAnn(ed.onAnnMove)} onPointerUp={() => void ed.onAnnUp()} onLostPointerCapture={() => void ed.onAnnUp()} />
                          <button
                            type="button"
                            class="tl-seg-body"
                            aria-label={t("{label}, {start} to {end} seconds", { label: b.label, start: g().start.toFixed(1), end: g().end.toFixed(1) })}
                            aria-pressed={ed.isSelected(b.kind, b.id)}
                            data-tip={gone() ? t("Hidden by a cut, never exported") : undefined}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                e.stopPropagation();
                                ed.selectAnnotation(b.kind, b.id);
                                ed.scrub(g().start);
                              }
                            }}
                            onContextMenu={(e) => annBarMenu(ed, e, b.kind, b.id)}
                            onPointerDown={ed.onAnnDown(b, "move")}
                            onPointerMove={ed.frameAnn(ed.onAnnMove)}
                            onPointerUp={() => void ed.onAnnUp()}
                            onLostPointerCapture={() => void ed.onAnnUp()}
                          >
                            <Icon name={b.kind === "text" ? "text" : b.kind === "arrow" ? "arrow" : b.kind === "stroke" ? "pen" : "shape"} size={11} />
                            <span class="tl-seg-label">{b.label}</span>
                          </button>
                          <span class="tl-handle r" onPointerDown={ed.onAnnDown(b, "r")} onPointerMove={ed.frameAnn(ed.onAnnMove)} onPointerUp={() => void ed.onAnnUp()} onLostPointerCapture={() => void ed.onAnnUp()} />
                        </div>
                      );
                    }}
                  </For>
                </div>

                {/* Speed + cuts: amber means faster, striped red means removed. */}
                <div class="tl-lane tl-effects">
                  <Show when={ed.speed().length === 0 && ed.cuts().length === 0 && prefs.hints()}>
                    <div class="tl-lanehint">{t("Speed-ups and cuts land here. Try Skim idle in the Clip panel")}</div>
                  </Show>
                  <For each={ed.speed()}>
                    {(r, i) => {
                      const g = () => ed.speedGeom(i(), r);
                      const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                      return (
                        <div
                          class="tl-seg tl-speed"
                          classList={{ selected: ed.selSpeed() === i(), swallowed: gone(), narrow: isNarrow(g().start, g().end) }}
                          style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onSpeedDown(i(), r, "l")} onPointerMove={ed.frameSpeed(ed.onSpeedMove)} onPointerUp={() => void ed.onSpeedUp()} onLostPointerCapture={() => void ed.onSpeedUp()} />
                          <button
                            type="button"
                            class="tl-seg-body"
                            aria-label={t("Plays at {factor} times, {start} to {end} seconds", { factor: r.factor, start: g().start.toFixed(1), end: g().end.toFixed(1) })}
                            aria-pressed={ed.selSpeed() === i()}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                e.stopPropagation();
                                ed.selectSegment("speed", i());
                                ed.scrub(g().start);
                              }
                            }}
                            onContextMenu={(e) => speedMenu(ed, e, i())}
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
                          classList={{ selected: ed.selCut() === i(), narrow: isNarrow(g().start, g().end) }}
                          style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                        >
                          <span class="tl-handle l" onPointerDown={ed.onCutDown(i(), c, "l")} onPointerMove={ed.frameCut(ed.onCutMove)} onPointerUp={() => void ed.onCutUp()} onLostPointerCapture={() => void ed.onCutUp()} />
                          <button
                            type="button"
                            class="tl-seg-body"
                            aria-label={t("Removed section, {start} to {end} seconds", { start: g().start.toFixed(1), end: g().end.toFixed(1) })}
                            aria-pressed={ed.selCut() === i()}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                e.stopPropagation();
                                ed.selectSegment("cut", i());
                                ed.scrub(g().start);
                              }
                            }}
                            onContextMenu={(e) => cutMenu(ed, e, i())}
                            data-tip={t("Removed from the export. Delete restores it")}
                            onPointerDown={ed.onCutDown(i(), c, "move")}
                            onPointerMove={ed.frameCut(ed.onCutMove)}
                            onPointerUp={() => void ed.onCutUp()}
                            onLostPointerCapture={() => void ed.onCutUp()}
                          >
                            <Icon name="cut" size={11} />
                            <span class="tl-seg-label">{t("Cut")}</span>
                          </button>
                          <span class="tl-handle r" onPointerDown={ed.onCutDown(i(), c, "r")} onPointerMove={ed.frameCut(ed.onCutMove)} onPointerUp={() => void ed.onCutUp()} onLostPointerCapture={() => void ed.onCutUp()} />
                        </div>
                      );
                    }}
                  </For>
                </div>

                {/* Captions: one bar per cue. Click to edit the words, drag to retime. */}
                <Show when={ed.captions.list().length > 0}>
                  <div class="tl-lane tl-captions">
                    <For each={ed.captions.list()}>
                      {(c) => {
                        const g = () => ed.captionGeom(c);
                        const gone = () => isSwallowed(g().start, g().end, ed.cuts(), ed.trim());
                        return (
                          <div
                            class="tl-seg tl-caption"
                            classList={{
                              selected: ed.selCaption() === c.id,
                              swallowed: gone(),
                              empty: !c.text.trim(),
                              narrow: isNarrow(g().start, g().end),
                            }}
                            style={{ left: `${ed.pct(g().start)}%`, width: w(g().start, g().end) }}
                          >
                            <span class="tl-handle l" onPointerDown={ed.onCaptionDown(c, "l")} onPointerMove={ed.frameCaption(ed.onCaptionMove)} onPointerUp={() => void ed.onCaptionUp()} onLostPointerCapture={() => void ed.onCaptionUp()} />
                            <button
                              type="button"
                              class="tl-seg-body"
                              aria-label={t("Caption: {text}", { text: c.text || t("empty") })}
                              aria-pressed={ed.selCaption() === c.id}
                              data-tip={gone() ? t("Hidden by a cut, never exported") : c.text || t("Empty caption")}
                              onKeyDown={(e) => {
                                if (e.key === "Enter" || e.key === " ") {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  ed.selectCaption(c.id);
                                  ed.scrub(c.range.start + 0.01);
                                }
                              }}
                              onContextMenu={(e) => captionMenu(ed, e, c.id)}
                              onPointerDown={ed.onCaptionDown(c, "move")}
                              onPointerMove={ed.frameCaption(ed.onCaptionMove)}
                              onPointerUp={() => void ed.onCaptionUp()}
                              onLostPointerCapture={() => void ed.onCaptionUp()}
                            >
                              <span class="tl-seg-label">{c.text.trim() || t("Empty")}</span>
                            </button>
                            <span class="tl-handle r" onPointerDown={ed.onCaptionDown(c, "r")} onPointerMove={ed.frameCaption(ed.onCaptionMove)} onPointerUp={() => void ed.onCaptionUp()} onLostPointerCapture={() => void ed.onCaptionUp()} />
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>

                {/* Audio: one waveform lane per recorded track. */}
                <Index each={ed.audio.tracks()}>
                  {(t) => (
                    <div class="tl-lane tl-audio" classList={{ muted: t().muted }}>
                      <Waveform
                        data={ed.audio.data()[t().kind]}
                        track={t()}
                        duration={ed.duration()}
                        trim={ed.trim()}
                        cuts={ed.cuts()}
                        speed={ed.speed()}
                        theme={ed.theme()}
                      />
                    </div>
                  )}
                </Index>

                {/* Trim: shaded outside, draggable + keyboard slider in/out handles. */}
                <div class="tl-shade" style={{ left: "0", width: `${ed.pct(ed.tStart())}%` }} />
                <div class="tl-shade" style={{ left: `${ed.pct(ed.tEnd())}%`, right: "0" }} />
                <div
                  class="tl-trim start"
                  role="slider"
                  tabIndex={0}
                  aria-label={t("Trim start")}
                  aria-valuemin={0}
                  aria-valuemax={Number(Math.max(0, ed.tEnd() - 0.2).toFixed(2))}
                  aria-valuenow={Number(ed.tStart().toFixed(2))}
                  aria-valuetext={fmtT(ed.tStart())}
                  style={{ left: `${ed.pct(ed.tStart())}%` }}
                  data-tip={t("Trim start")}
                  onKeyDown={onTrimKey("start")}
                  onPointerDown={ed.onTrimDown("start")}
                  onPointerMove={ed.frameTrim(ed.onTrimMove)}
                  onPointerUp={() => void ed.onTrimUp()}
                  onLostPointerCapture={() => void ed.onTrimUp()}
                />
                <div
                  class="tl-trim end"
                  role="slider"
                  tabIndex={0}
                  aria-label={t("Trim end")}
                  aria-valuemin={Number(Math.min(ed.duration(), ed.tStart() + 0.2).toFixed(2))}
                  aria-valuemax={Number(ed.duration().toFixed(2))}
                  aria-valuenow={Number(ed.tEnd().toFixed(2))}
                  aria-valuetext={fmtT(ed.tEnd())}
                  style={{ left: `${ed.pct(ed.tEnd())}%` }}
                  data-tip={t("Trim end")}
                  onKeyDown={onTrimKey("end")}
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
                  <button
                    type="button"
                    class="tl-playhead-cap"
                    role="slider"
                    aria-label={t("Playhead")}
                    aria-valuemin={0}
                    aria-valuemax={Number(ed.duration().toFixed(2))}
                    aria-valuenow={Number(ed.playhead().toFixed(2))}
                    aria-valuetext={fmtT(ed.playhead())}
                    onKeyDown={onPlayheadKey}
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      </Show>
    </section>
  );
}
