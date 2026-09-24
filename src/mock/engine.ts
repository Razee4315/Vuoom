// Browser mock of the Rust engine. It activates only when the app runs outside Tauri
// (or with ?mock=1) so the whole UI can be developed and screenshot in a plain browser
// without building the native backend. Command handlers mirror the serde shapes in
// src/types.ts, which mirror the real src-tauri session structs.

import type {
  AnnotationSet,
  ArrowAnn,
  AudioKind,
  AudioTrack,
  BoxAnn,
  CameraOverlay,
  CursorStyle,
  ClipState,
  Color,
  FrameInfo,
  RecordingSummary,
  SpeedRegion,
  TextAnn,
  Trim,
  ZoomSeg,
  ZoomStyle,
} from "../types";
import { defaultOverlay } from "./camera";

const DEMO_DURATION = 14.4;

const color = (hex: string, a = 1): Color => ({
  r: parseInt(hex.slice(1, 3), 16),
  g: parseInt(hex.slice(3, 5), 16),
  b: parseInt(hex.slice(5, 7), 16),
  a,
});
const range = (start: number, end: number) => ({ start, end, fade_in: 0.15, fade_out: 0.25 });

// Cursor path the synthetic desktop follows (normalized units). Clicks land on this path.
const cursorAt = (t: number) => ({
  x: 0.5 + 0.33 * Math.sin(t * 0.85 + 0.4),
  y: 0.46 + 0.26 * Math.sin(t * 0.55 + 1.7),
});
const CLICK_TIMES = [1.3, 2.9, 5.1, 5.25, 7.6, 10.0, 12.2];

// A demo take used by recover / open-project / fresh recordings: annotations, zooms,
// speed regions and a cut so every timeline affordance is visible at once.
function demoAnns(): AnnotationSet {
  return {
    texts: [
      {
        id: 1,
        text: "Click here first",
        pos: [0.2, 0.18],
        font_size: 0.055,
        color: color("#ffffff"),
        bold: true,
        italic: false,
        background: true,
        font: "",
        range: range(0.6, 4.4),
      },
    ],
    arrows: [
      {
        id: 2,
        from: [0.24, 0.34],
        to: [0.42, 0.52],
        color: color("#e5484d"),
        thickness: 0.006,
        style: "Arrow",
        range: range(1.0, 3.6),
      },
    ],
    highlights: [
      {
        id: 3,
        rect: { x: 0.52, y: 0.3, w: 0.24, h: 0.16 },
        color: color("#ffd23f", 0.4),
        thickness: 0.004,
        filled: true,
        shape: "Rect",
        range: range(4.6, 8.2),
      },
    ],
  };
}

function demoZooms(): ZoomSeg[] {
  return [
    { start: 2.4, end: 4.6, amount: 1.8, mode: "Auto", style: "Smooth" },
    { start: 6.8, end: 9.4, amount: 2.2, mode: { Manual: { pos: [0.62, 0.4] } }, style: "Snappy" },
  ];
}

type Snapshot = string;

const track = (kind: AudioKind): AudioTrack => ({ kind, offset: 0, gain: 1, muted: false, denoise: false, level: false });

type ClipItem =
  | ({ kind: "text" } & TextAnn)
  | ({ kind: "arrow" } & ArrowAnn)
  | ({ kind: "box" } & BoxAnn);

class MockEngine {
  duration = 0;
  hasClip = false;
  anns: AnnotationSet = { texts: [], arrows: [], highlights: [] };
  zooms: ZoomSeg[] = [];
  trim: Trim | null = null;
  speed: SpeedRegion[] = [];
  cuts: Trim[] = [];
  showClicks = false;
  motionBlur = true;
  /** What the next mock take starts with (set_take_defaults). */
  takeDefaults = { clicks: false, keys: false, frame: "none", denoise: false };
  showKeys = false;
  crop: { x: number; y: number; w: number; h: number } | null = null;
  audio: AudioTrack[] = [];
  /** The re-drawn pointer (mirrors Project::cursor) and whether the real one was captured. */
  cursor: CursorStyle | null = null;
  pointerCaptured = true;
  /** Pointer handling for the next mock take (set_capture_cursor). */
  captureShow = true;
  captureSmooth = false;
  /** The webcam bubble (mirrors Project::camera). */
  camera: CameraOverlay | null = null;
  /** The camera the next mock take records, and whether its live bubble is open. */
  cameraChoice: { on: boolean; device: string | null } = { on: false, device: null };
  cameraPreview = false;
  /** Audio the next mock recording "captures" (set_capture_audio). */
  audioChoice = { mic: false, system: false };
  micCheck = false;
  framePreset = "none";
  bgPreset = "";
  /** Exact frame values behind the preset names (mirrors session::FrameInfo). */
  frame: FrameInfo = { ...FRAME_NONE, bg_kind: "solid", bg_from: [0, 0, 0], bg_to: [0, 0, 0], bg_angle: 45 };
  playhead = 0;
  zoomAmount = 1.8;
  live = false;
  liveStartedAt = 0;
  livePaused = false;
  livePausedAt = 0;
  private nextId = 100;
  private undoStack: Snapshot[] = [];
  private redoStack: Snapshot[] = [];
  private lastTag = "";
  private lastTagAt = 0;
  private exporting = false;
  private cancelRequested = false;
  private progressCbs = new Set<(p: { done: number; total: number }) => void>();
  private backdropDataUrl: string | null = null;
  /** Bumped on every state change so preview clients can repaint only when needed. */
  sceneVersion = 0;
  private dirtyCbs = new Set<() => void>();

  onDirty(cb: () => void): () => void {
    this.dirtyCbs.add(cb);
    return () => this.dirtyCbs.delete(cb);
  }
  private markDirty() {
    this.sceneVersion++;
    for (const cb of this.dirtyCbs) cb();
  }
  /** Move the playhead; preview clients repaint the new moment. */
  seek(t: number) {
    this.playhead = Math.max(0, Math.min(this.duration || 0, t));
    this.markDirty();
  }

  // ── state helpers ────────────────────────────────────────────────────────────
  private snap(): Snapshot {
    return JSON.stringify({
      anns: this.anns,
      zooms: [...this.zooms],
      trim: this.trim,
      speed: this.speed,
      cuts: [...this.cuts],
      showClicks: this.showClicks,
      motionBlur: this.motionBlur,
      showKeys: this.showKeys,
      crop: this.crop ? { ...this.crop } : null,
      framePreset: this.framePreset,
      bgPreset: this.bgPreset,
      frame: this.frame,
      duration: this.duration,
      audio: this.audio,
      cursor: this.cursor,
      camera: this.camera,
    });
  }
  private mutate(tag?: string, fn?: () => void) {
    const now = Date.now();
    const coalesce = tag && tag === this.lastTag && now - this.lastTagAt < 1200;
    this.lastTag = tag ?? "";
    this.lastTagAt = now;
    if (!coalesce) {
      this.undoStack.push(this.snap());
      if (this.undoStack.length > 100) this.undoStack.shift();
      this.redoStack = [];
    }
    fn?.();
    this.markDirty();
  }
  private restore(s: Snapshot) {
    this.sceneVersion++;
    const st = JSON.parse(s);
    this.anns = st.anns;
    this.zooms = st.zooms;
    this.trim = st.trim;
    this.speed = st.speed;
    this.cuts = st.cuts;
    this.showClicks = st.showClicks;
    this.motionBlur = st.motionBlur ?? true;
    this.showKeys = st.showKeys;
    this.framePreset = st.framePreset;
    this.bgPreset = st.bgPreset;
    this.frame = st.frame;
    this.duration = st.duration;
    this.audio = st.audio ?? [];
    this.cursor = st.cursor ?? null;
    this.camera = st.camera ?? null;
  }

  private loadDemo() {
    this.hasClip = true;
    this.duration = DEMO_DURATION;
    this.anns = demoAnns();
    this.zooms = demoZooms();
    this.trim = null;
    this.speed = [{ start: 9.8, end: 11.6, factor: 3 }];
    this.cuts = [{ start: 12.6, end: 13.4 }];
    this.showClicks = true;
    this.showKeys = true;
    this.framePreset = "subtle";
    this.bgPreset = "graphite";
    this.frame = { ...FRAME_SUBTLE, ...bgInfo("graphite") };
    this.audio = [track("mic"), track("system")];
    this.cursor = { size: 1.5, smoothing: 0.05 };
    this.pointerCaptured = false;
    this.camera = defaultOverlay();
    this.playhead = 0;
    this.undoStack = [];
    this.redoStack = [];
  }

  private nextAnnId() {
    return ++this.nextId;
  }

  private outDuration() {
    let out = (this.trim?.end ?? this.duration) - (this.trim?.start ?? 0);
    for (const c of this.cuts) out -= Math.max(0, c.end - c.start);
    for (const r of this.speed) {
      const len = Math.max(0, Math.min(r.end, this.duration) - r.start);
      out -= len - len / r.factor;
    }
    return Math.max(0.1, out);
  }

  clipState(): ClipState {
    return {
      duration: this.duration,
      trim: this.trim,
      speed_regions: [...this.speed],
      cuts: [...this.cuts],
      zooms: [...this.zooms],
      show_clicks: this.showClicks,
      motion_blur: this.motionBlur,
      show_keys: this.showKeys,
      crop: this.crop ? { ...this.crop } : null,
      frame_preset: this.frame.padding <= 0 ? "none" : this.frame.padding < 0.06 ? "subtle" : "studio",
      background_preset: this.bgPreset,
      frame: structuredClone(this.frame),
      audio: structuredClone(this.audio),
      cursor: this.cursor ? { ...this.cursor } : null,
      pointer_captured: this.pointerCaptured,
      camera: this.camera ? { ...this.camera } : null,
    };
  }

  setCameraOverlay(overlay: CameraOverlay) {
    if (!this.camera) return;
    const offset = this.camera.offset;
    this.mutate("camera-overlay", () => {
      this.camera = { ...overlay, size: Math.max(0.12, Math.min(0.6, overlay.size)), offset };
    });
  }

  setCursorStyle(style: CursorStyle | null) {
    this.mutate("cursor-style", () => {
      this.cursor = style
        ? {
            size: Math.max(0.5, Math.min(3, style.size)),
            smoothing: Math.max(0, Math.min(0.2, style.smoothing)),
            hide_idle: !!style.hide_idle,
          }
        : null;
    });
  }

  setAudioTrack(kind: AudioKind, gain: number, muted: boolean) {
    this.mutate("audio-track", () => {
      this.audio = this.audio.map((t) => (t.kind === kind ? { ...t, gain: Math.max(0, Math.min(4, gain)), muted } : t));
    });
  }

  setAudioCleanup(kind: AudioKind, denoise: boolean, level: boolean) {
    this.mutate(undefined, () => {
      this.audio = this.audio.map((t) => (t.kind === kind ? { ...t, denoise, level } : t));
    });
  }

  setMotionBlur(on: boolean) {
    this.mutate(undefined, () => {
      this.motionBlur = on;
    });
  }

  setFramePreset(preset: string) {
    this.mutate(undefined, () => {
      this.framePreset = preset;
      const vals = preset === "studio" ? FRAME_STUDIO : preset === "subtle" ? FRAME_SUBTLE : FRAME_NONE;
      this.frame = { ...this.frame, ...vals };
      if (preset !== "none" && this.bgPreset === "" && this.frame.bg_from.every((c) => c === 0)) {
        this.bgPreset = "graphite";
        this.frame = { ...this.frame, ...bgInfo("graphite") };
      }
    });
  }
  setFrameStyle(padding: number, radius: number, shadow: number) {
    this.mutate("frame-style", () => {
      this.frame = {
        ...this.frame,
        padding: Math.min(0.2, Math.max(0, padding)),
        corner_radius: Math.min(0.08, Math.max(0, radius)),
        shadow: Math.min(1, Math.max(0, shadow)),
      };
    });
  }
  setBackgroundPreset(name: string) {
    this.mutate(undefined, () => {
      this.bgPreset = name;
      this.frame = { ...this.frame, ...bgInfo(name) };
    });
  }
  setBackgroundCustom(from: [number, number, number], to: [number, number, number] | null, angle: number) {
    this.mutate("bg-custom", () => {
      this.bgPreset = "";
      this.frame = {
        ...this.frame,
        bg_kind: to ? "gradient" : "solid",
        bg_from: from,
        bg_to: to ?? from,
        bg_angle: angle,
      };
    });
  }

  setCrop(crop: { x: number; y: number; w: number; h: number } | null) {
    this.mutate("crop", () => {
      const full =
        !crop || (crop.x <= 1e-3 && crop.y <= 1e-3 && crop.w >= 0.999 && crop.h >= 0.999);
      this.crop = full ? null : { ...crop };
    });
  }

  planZoomAuto(amount: number): ZoomSeg[] {
    this.mutate("zoomplan", () => {
      // Re-derive zooms from the scripted click times at the requested strength.
      const spans: ZoomSeg[] = [];
      for (const ct of CLICK_TIMES) {
        const start = Math.max(0, ct - 0.35);
        const end = Math.min(this.duration, ct + 1.4);
        if (end - start < 0.6) continue;
        const last = spans[spans.length - 1];
        // Merge only genuine click bursts (overlapping spans), never chain distant
        // clusters, so the demo plans several distinct zooms like the Rust planner.
        if (last && start < last.end) {
          last.end = Math.max(last.end, end);
          continue;
        }
        spans.push({ start, end, amount, mode: "Auto", style: "Smooth" });
      }
      this.zooms = spans;
    });
    return [...this.zooms];
  }

  summary(): RecordingSummary {
    return {
      duration: this.duration,
      frames: Math.round(this.duration * 30),
      zooms: this.zooms.length,
      warning: null,
    };
  }

  // ── recording flow ───────────────────────────────────────────────────────────
  async enterOverlay(): Promise<string> {
    if (!this.backdropDataUrl) this.backdropDataUrl = paintDesktopToDataUrl(960, 540);
    return this.backdropDataUrl;
  }
  startRecording() {
    this.live = true;
    this.livePaused = false;
    this.liveStartedAt = Date.now();
  }
  setRecordPaused(paused: boolean) {
    if (paused && !this.livePaused) {
      this.livePaused = true;
      this.livePausedAt = Date.now();
    } else if (!paused && this.livePaused) {
      this.liveStartedAt += Date.now() - this.livePausedAt;
      this.livePaused = false;
    }
  }
  liveElapsed() {
    if (!this.live) return 0;
    const end = this.livePaused ? this.livePausedAt : Date.now();
    return Math.max(0, (end - this.liveStartedAt) / 1000);
  }
  finishRecording(): RecordingSummary {
    const elapsed = Math.max(4, Math.round(this.liveElapsed() * 10) / 10);
    this.live = false;
    this.hasClip = true;
    this.duration = elapsed;
    this.trim = null;
    this.playhead = 0;
    this.undoStack = [];
    this.redoStack = [];
    if (this.zoomAmount > 1) {
      this.zooms = [
        {
          start: Math.min(1.2, elapsed * 0.25),
          end: Math.min(3.2, elapsed * 0.55),
          amount: this.zoomAmount,
          mode: "Auto",
          style: "Smooth",
        },
      ];
    } else {
      this.zooms = [];
    }
    this.speed = [];
    this.cuts = [];
    this.audio = [
      ...(this.audioChoice.mic ? [track("mic")] : []),
      ...(this.audioChoice.system ? [track("system")] : []),
    ];
    this.pointerCaptured = this.captureShow;
    this.cursor = this.captureSmooth ? { size: 1.5, smoothing: 0.05 } : null;
    this.camera = this.cameraChoice.on ? defaultOverlay() : null;
    this.showClicks = this.takeDefaults.clicks;
    this.showKeys = this.takeDefaults.keys;
    this.framePreset = this.takeDefaults.frame;
    const frame = this.takeDefaults.frame === "studio" ? FRAME_STUDIO : this.takeDefaults.frame === "subtle" ? FRAME_SUBTLE : FRAME_NONE;
    this.frame = { ...this.frame, ...frame, ...(frame === FRAME_NONE ? {} : bgInfo("graphite")) };
    this.audio = this.audio.map((t) => (t.kind === "mic" ? { ...t, denoise: this.takeDefaults.denoise } : t));
    return this.summary();
  }
  recoverSession(): RecordingSummary {
    this.loadDemo();
    return this.summary();
  }
  openProject(): RecordingSummary {
    this.loadDemo();
    return this.summary();
  }

  // ── undo / redo ──────────────────────────────────────────────────────────────
  undo(): boolean {
    const s = this.undoStack.pop();
    if (!s) return false;
    this.redoStack.push(this.snap());
    this.restore(s);
    return true;
  }
  redo(): boolean {
    const s = this.redoStack.pop();
    if (!s) return false;
    this.undoStack.push(this.snap());
    this.restore(s);
    return true;
  }

  // ── annotations ──────────────────────────────────────────────────────────────
  addText(a: { text: string; x: number; y: number; t: number }): number {
    const id = this.nextAnnId();
    this.mutate(undefined, () => {
      this.anns.texts.push({
        id,
        text: a.text,
        pos: [a.x, a.y],
        font_size: 0.05,
        color: color("#ffffff"),
        bold: false,
        italic: false,
        background: false,
        font: "",
        range: range(Math.max(0, a.t - 0.2), Math.min(this.duration, a.t + 2.8)),
      });
    });
    return id;
  }
  addArrow(a: { fx: number; fy: number; tx: number; ty: number; t: number }): number {
    const id = this.nextAnnId();
    this.mutate(undefined, () => {
      this.anns.arrows.push({
        id,
        from: [a.fx, a.fy],
        to: [a.tx, a.ty],
        color: color("#e5484d"),
        thickness: 0.005,
        style: "Arrow",
        range: range(Math.max(0, a.t - 0.2), Math.min(this.duration, a.t + 2.8)),
      });
    });
    return id;
  }
  addBox(a: {
    x: number;
    y: number;
    w: number;
    h: number;
    t: number;
    ellipse?: boolean;
    highlight?: boolean;
    mask?: boolean;
  }): number {
    const id = this.nextAnnId();
    this.mutate(undefined, () => {
      const mask = !!a.mask;
      this.anns.highlights.push({
        id,
        rect: { x: a.x, y: a.y, w: a.w, h: a.h },
        color: mask ? color("#0a0a0d") : a.highlight ? color("#ffd23f", 0.4) : color("#ffd23f"),
        thickness: 0.0,
        filled: mask || !!a.highlight,
        shape: mask ? "Mask" : a.ellipse ? "Ellipse" : "Rect",
        range: mask
          ? {
              start: Math.max(0, a.t - 0.2),
              end: Math.min(this.duration, a.t + 2.8),
              fade_in: 0,
              fade_out: 0,
            }
          : range(Math.max(0, a.t - 0.2), Math.min(this.duration, a.t + 2.8)),
      });
    });
    return id;
  }
  private findAnn(kind: string, id: number): TextAnn | ArrowAnn | BoxAnn | undefined {
    if (kind === "text") return this.anns.texts.find((t) => t.id === id);
    if (kind === "arrow") return this.anns.arrows.find((t) => t.id === id);
    return this.anns.highlights.find((t) => t.id === id);
  }
  updateText(id: number, patch: Partial<TextAnn>) {
    this.mutate(`text:${id}`, () => {
      const t = this.anns.texts.find((x) => x.id === id);
      if (t) Object.assign(t, patch);
    });
  }
  updateBox(id: number, patch: { x?: number; y?: number; w?: number; h?: number }) {
    this.mutate(`box:${id}`, () => {
      const b = this.anns.highlights.find((x) => x.id === id);
      if (b && patch.x !== undefined) {
        b.rect = { x: patch.x, y: patch.y ?? b.rect.y, w: patch.w ?? b.rect.w, h: patch.h ?? b.rect.h };
      }
    });
  }
  updateArrow(id: number, fx: number, fy: number, tx: number, ty: number) {
    this.mutate(`arrow:${id}`, () => {
      const a = this.anns.arrows.find((x) => x.id === id);
      if (a) {
        a.from = [fx, fy];
        a.to = [tx, ty];
      }
    });
  }
  setAnnColor(id: number, r: number, g: number, b: number) {
    this.mutate(`col:${id}`, () => {
      const a = ["text", "arrow", "box"]
        .map((k) => this.findAnn(k, id))
        .find((x) => x !== undefined) as { color: Color } | undefined;
      if (a) a.color = { r, g, b, a: a.color.a };
    });
  }
  setAnnOpacity(id: number, a: number) {
    this.mutate(`opa:${id}`, () => {
      const ann = ["text", "arrow", "box"]
        .map((k) => this.findAnn(k, id))
        .find((x) => x !== undefined) as { color: Color } | undefined;
      if (ann) ann.color.a = a;
    });
  }
  setAnnStyle(id: number, patch: { thickness?: number; filled?: boolean }) {
    this.mutate(`sty:${id}`, () => {
      const b = this.anns.highlights.find((x) => x.id === id);
      if (b) {
        if (patch.thickness !== undefined) b.thickness = patch.thickness;
        if (patch.filled !== undefined) b.filled = patch.filled;
      }
      const ar = this.anns.arrows.find((x) => x.id === id);
      if (ar && patch.thickness !== undefined) ar.thickness = patch.thickness;
    });
  }
  setHighlightShape(id: number, ellipse: boolean) {
    this.mutate(`shp:${id}`, () => {
      const b = this.anns.highlights.find((x) => x.id === id);
      if (b) b.shape = ellipse ? "Ellipse" : "Rect";
    });
  }
  setArrowStyle(id: number, style: "arrow" | "line" | "double") {
    this.mutate(`ars:${id}`, () => {
      const a = this.anns.arrows.find((x) => x.id === id);
      if (a) a.style = style === "arrow" ? "Arrow" : style === "line" ? "Line" : "DoubleArrow";
    });
  }
  updateAnnRange(id: number, start: number, end: number) {
    this.mutate(`rng:${id}`, () => {
      const ann = ["text", "arrow", "box"]
        .map((k) => this.findAnn(k, id))
        .find((x) => x !== undefined) as { range: { start: number; end: number; fade_in: number; fade_out: number } } | undefined;
      if (ann) ann.range = { ...ann.range, start, end };
    });
  }
  setAnnFades(id: number, fadeIn: number, fadeOut: number) {
    this.mutate(`fades:${id}`, () => {
      const ann = ["text", "arrow", "box"]
        .map((k) => this.findAnn(k, id))
        .find((x) => x !== undefined) as { range: { start: number; end: number; fade_in: number; fade_out: number } } | undefined;
      if (!ann) return;
      const half = Math.max(0, (ann.range.end - ann.range.start) / 2);
      ann.range = {
        ...ann.range,
        fade_in: Math.min(Math.max(0, fadeIn), half),
        fade_out: Math.min(Math.max(0, fadeOut), half),
      };
    });
  }
  duplicateAnn(id: number): number {
    const newId = this.nextAnnId();
    this.mutate(undefined, () => {
      const t = this.anns.texts.find((x) => x.id === id);
      if (t) this.anns.texts.push({ ...structuredClone(t), id: newId, pos: [t.pos[0] + 0.04, t.pos[1] + 0.05] });
      const a = this.anns.arrows.find((x) => x.id === id);
      if (a)
        this.anns.arrows.push({
          ...structuredClone(a),
          id: newId,
          from: [a.from[0] + 0.04, a.from[1] + 0.05],
          to: [a.to[0] + 0.04, a.to[1] + 0.05],
        });
      const b = this.anns.highlights.find((x) => x.id === id);
      if (b)
        this.anns.highlights.push({
          ...structuredClone(b),
          id: newId,
          rect: { ...b.rect, x: b.rect.x + 0.04, y: b.rect.y + 0.05 },
        });
    });
    return newId;
  }
  pasteAnns(items: ClipItem[], at: number): { kind: string; id: number }[] {
    const refs: { kind: string; id: number }[] = [];
    this.mutate(undefined, () => {
      const earliest = Math.min(...items.map((it) => it.range.start));
      for (const src of items) {
        const id = this.nextAnnId();
        const shift = at - earliest;
        const copy = structuredClone(src) as ClipItem;
        copy.id = id;
        copy.range = { ...copy.range, start: copy.range.start + shift, end: copy.range.end + shift };
        if (copy.kind === "text") this.anns.texts.push({ ...copy, kind: undefined } as unknown as TextAnn);
        else if (copy.kind === "arrow")
          this.anns.arrows.push({ ...copy, kind: undefined } as unknown as ArrowAnn);
        else this.anns.highlights.push({ ...copy, kind: undefined } as unknown as BoxAnn);
        refs.push({ kind: copy.kind, id });
      }
    });
    return refs;
  }
  reorderAnn(id: number, dir: "forward" | "backward" | "front" | "back") {
    this.mutate(`reo:${id}`, () => {
      const lists: (TextAnn | ArrowAnn | BoxAnn)[][] = [this.anns.texts, this.anns.arrows, this.anns.highlights];
      for (const list of lists) {
        const i = list.findIndex((x) => x.id === id);
        if (i < 0) continue;
        const [item] = list.splice(i, 1);
        const j =
          dir === "forward"
            ? Math.min(list.length, i + 1)
            : dir === "backward"
              ? Math.max(0, i - 1)
              : dir === "front"
                ? list.length
                : 0;
        list.splice(j, 0, item);
      }
    });
  }
  deleteAnn(id: number, tag?: string) {
    this.mutate(tag, () => {
      this.anns.texts = this.anns.texts.filter((x) => x.id !== id);
      this.anns.arrows = this.anns.arrows.filter((x) => x.id !== id);
      this.anns.highlights = this.anns.highlights.filter((x) => x.id !== id);
    });
  }

  // ── zooms / speed / cuts / trim ──────────────────────────────────────────────
  addZoom(t: number): ZoomSeg[] {
    this.mutate(undefined, () => {
      const start = Math.max(0, Math.min(t, this.duration - 0.5));
      const end = Math.min(this.duration, start + 1.6);
      this.zooms.push({ start, end, amount: this.zoomAmount, mode: "Auto", style: "Smooth" });
      this.zooms.sort((a, b) => a.start - b.start);
    });
    return [...this.zooms];
  }
  updateZoom(index: number, start: number, end: number, amount: number): ZoomSeg[] {
    this.mutate(`zoom:${index}`, () => {
      const z = this.zooms[index];
      if (z) {
        z.start = Math.min(start, end);
        z.end = Math.max(start, end);
        z.amount = amount;
      }
      this.zooms.sort((a, b) => a.start - b.start);
    });
    return [...this.zooms];
  }
  setZoomFocus(index: number, focus?: { x: number; y: number }): ZoomSeg[] {
    this.mutate(`zfoc:${index}`, () => {
      const z = this.zooms[index];
      if (z) z.mode = focus ? { Manual: { pos: [focus.x, focus.y] } } : "Auto";
    });
    return [...this.zooms];
  }
  setZoomStyle(index: number, style: ZoomStyle): ZoomSeg[] {
    this.mutate(`zsty:${index}`, () => {
      const z = this.zooms[index];
      if (z) z.style = style;
    });
    return [...this.zooms];
  }
  deleteZoom(index: number): ZoomSeg[] {
    this.mutate(undefined, () => {
      this.zooms.splice(index, 1);
    });
    return [...this.zooms];
  }
  addSpeed(start: number, end: number, factor: number): SpeedRegion[] {
    this.mutate(undefined, () => {
      this.speed.push({ start, end, factor });
      this.speed.sort((a, b) => a.start - b.start);
    });
    return [...this.speed];
  }
  updateSpeed(index: number, start: number, end: number, factor: number): SpeedRegion[] {
    this.mutate(`speed:${index}`, () => {
      const r = this.speed[index];
      if (r) {
        r.start = Math.min(start, end);
        r.end = Math.max(start, end);
        r.factor = factor;
      }
      this.speed.sort((a, b) => a.start - b.start);
    });
    return [...this.speed];
  }
  deleteSpeed(index: number): SpeedRegion[] {
    this.mutate(undefined, () => {
      this.speed.splice(index, 1);
    });
    return [...this.speed];
  }
  autoSpeed(factor: number): SpeedRegion[] {
    this.mutate(undefined, () => {
      this.speed = [
        { start: 6.2, end: 9.4, factor },
        { start: 11.4, end: Math.max(11.8, this.duration - 0.4), factor },
      ].filter((r) => r.end > r.start && r.start < this.duration);
    });
    return [...this.speed];
  }
  clearSpeed() {
    this.mutate(undefined, () => {
      this.speed = [];
    });
  }
  addCut(start: number, end: number): Trim[] {
    this.mutate(undefined, () => {
      this.cuts.push({ start, end });
      this.cuts.sort((a, b) => a.start - b.start);
    });
    return [...this.cuts];
  }
  updateCut(index: number, start: number, end: number): Trim[] {
    this.mutate(`cut:${index}`, () => {
      const c = this.cuts[index];
      if (c) {
        c.start = Math.min(start, end);
        c.end = Math.max(start, end);
      }
      this.cuts.sort((a, b) => a.start - b.start);
    });
    return [...this.cuts];
  }
  deleteCut(index: number): Trim[] {
    this.mutate(undefined, () => {
      this.cuts.splice(index, 1);
    });
    return [...this.cuts];
  }
  setTrim(start: number, end: number) {
    this.mutate(`trim`, () => {
      const full = start <= 0.01 && end >= this.duration - 0.01;
      this.trim = full ? null : { start, end };
    });
  }

  // ── export ───────────────────────────────────────────────────────────────────
  estimateGif(fps: number, width: number, quality: number): number {
    const h = Math.round((width * 9) / 16);
    const outDur = this.outDuration();
    return Math.round(width * h * fps * outDur * (0.05 + (quality / 100) * 0.12));
  }
  onProgress(cb: (p: { done: number; total: number }) => void): () => void {
    this.progressCbs.add(cb);
    return () => this.progressCbs.delete(cb);
  }
  cancelExport() {
    if (this.exporting) this.cancelRequested = true;
  }
  async runExport(path: string, _fps: number, _width: number, _quality: number): Promise<string> {
    this.exporting = true;
    this.cancelRequested = false;
    const total = 24;
    try {
      for (let i = 1; i <= total; i++) {
        await new Promise((r) => setTimeout(r, 90));
        if (this.cancelRequested) throw new Error("export cancelled");
        for (const cb of this.progressCbs) cb({ done: i, total });
      }
      return path;
    } finally {
      this.exporting = false;
    }
  }
}

// ── synthetic desktop painter ────────────────────────────────────────────────
// A fake "code editor over a browser" scene with a scripted cursor and click ripples,
// so scrubbing, zooms and ripples are all visible in the preview without the engine.

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function paintDesktop(ctx: CanvasRenderingContext2D, w: number, h: number, t: number, opts: { live?: boolean } = {}) {
  // backdrop
  const bg = ctx.createLinearGradient(0, 0, 0, h);
  bg.addColorStop(0, "#181b24");
  bg.addColorStop(1, "#0f1118");
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, w, h);

  // code editor window
  const ew = w * 0.62;
  const eh = h * 0.72;
  const ex = w * 0.05;
  const ey = h * 0.12;
  ctx.fillStyle = "#1e2230";
  roundRect(ctx, ex, ey, ew, eh, 10);
  ctx.fill();
  ctx.strokeStyle = "#2c3245";
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.fillStyle = "#262b3c";
  roundRect(ctx, ex, ey, ew, 26, 10);
  ctx.fill();
  for (let i = 0; i < 3; i++) {
    ctx.fillStyle = ["#ff5f57", "#febc2e", "#28c840"][i];
    ctx.beginPath();
    ctx.arc(ex + 16 + i * 16, ey + 13, 5, 0, Math.PI * 2);
    ctx.fill();
  }
  const lineCols = ["#7a8199", "#c586c0", "#9cdcfe", "#6a9955", "#dcdcaa", "#7a8199"];
  for (let i = 0; i < 12; i++) {
    const ly = ey + 48 + i * ((eh - 60) / 12);
    const segs = 1 + ((i * 7) % 3);
    let lx = ex + 20;
    for (let sIdx = 0; sIdx < segs; sIdx++) {
      const lw = 26 + ((i * 37 + sIdx * 53) % 90);
      ctx.fillStyle = lineCols[(i + sIdx) % lineCols.length];
      ctx.globalAlpha = 0.7;
      roundRect(ctx, lx, ly, lw, 7, 3);
      ctx.fill();
      ctx.globalAlpha = 1;
      lx += lw + 10;
    }
  }

  // browser window
  const bx = w * 0.42;
  const by = h * 0.34;
  const bw = w * 0.52;
  const bh = h * 0.54;
  ctx.fillStyle = "#232838";
  roundRect(ctx, bx, by, bw, bh, 10);
  ctx.fill();
  ctx.strokeStyle = "#333a52";
  ctx.stroke();
  ctx.fillStyle = "#2b3145";
  roundRect(ctx, bx, by, bw, 24, 10);
  ctx.fill();
  roundRect(ctx, bx + 14, by + 5, bw * 0.6, 14, 7);
  ctx.fillStyle = "#1a1e2b";
  ctx.fill();
  // content blocks
  ctx.fillStyle = "#3a415c";
  roundRect(ctx, bx + 16, by + 44, bw - 32, 34, 6);
  ctx.fill();
  for (let i = 0; i < 4; i++) {
    ctx.fillStyle = i % 2 ? "#333a52" : "#3a415c";
    roundRect(ctx, bx + 16, by + 92 + i * 26, (bw - 32) * (0.9 - i * 0.12), 14, 5);
    ctx.fill();
  }

  // taskbar
  ctx.fillStyle = "#12141c";
  ctx.fillRect(0, h - 26, w, 26);
  ctx.fillStyle = "#5b627a";
  ctx.font = "11px Inter, sans-serif";
  ctx.textAlign = "right";
  const secs = Math.floor(t);
  const mm = String(Math.floor(secs / 60)).padStart(2, "0");
  const ss = String(secs % 60).padStart(2, "0");
  ctx.fillText(`rec ${mm}:${ss}`, w - 12, h - 9);
  ctx.textAlign = "left";

  if (t <= 0 && !opts.live) return; // frozen backdrop: no cursor, no clock chip

  // click ripples at scripted click times
  if (opts.live !== false) {
    for (const ct of CLICK_TIMES) {
      const age = t - ct;
      if (age >= 0 && age < 0.7) {
        const c = cursorAt(ct);
        ctx.strokeStyle = `rgba(229,72,77,${1 - age / 0.7})`;
        ctx.lineWidth = 2.5;
        ctx.beginPath();
        ctx.arc(c.x * w, c.y * h, 6 + age * 46, 0, Math.PI * 2);
        ctx.stroke();
      }
    }
  }

  // The pointer: the real one when the take captured it, the re-drawn one on top when the
  // project has it (sized like the engine's: 2.1% of the screen height per size unit).
  const c = cursorAt(Math.max(0, t));
  const cx = c.x * w;
  const cy = c.y * h;
  if (opts.live || mockEngine.pointerCaptured) {
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(1.15, 1.15);
    ctx.fillStyle = "#ffffff";
    ctx.strokeStyle = "#111111";
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(0, 16);
    ctx.lineTo(4.4, 12.4);
    ctx.lineTo(7.4, 18.6);
    ctx.lineTo(10.2, 17.2);
    ctx.lineTo(7.2, 11.2);
    ctx.lineTo(12.4, 10.6);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }
  const style = mockEngine.cursor;
  if (!opts.live && style) {
    const press = CLICK_TIMES.some((ct) => t - ct >= 0 && t - ct < 0.28) ? 0.86 : 1;
    const s = (style.size * 0.021 * h * press) / 17.3;
    ctx.save();
    ctx.translate(cx, cy);
    ctx.scale(s, s);
    ctx.shadowColor = "rgba(0,0,0,0.35)";
    ctx.shadowBlur = 3;
    ctx.shadowOffsetY = 1;
    ctx.fillStyle = "#ffffff";
    ctx.strokeStyle = "#0a0a0d";
    ctx.lineWidth = 1.6;
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(0, 14.9);
    ctx.lineTo(3.6, 11.6);
    ctx.lineTo(6.2, 17.3);
    ctx.lineTo(8.6, 16.3);
    ctx.lineTo(6, 10.7);
    ctx.lineTo(10.7, 10.5);
    ctx.closePath();
    ctx.stroke();
    ctx.fill();
    ctx.restore();
  }
}

let backdropCanvas: HTMLCanvasElement | null = null;
function paintDesktopToDataUrl(w: number, h: number): string {
  if (!backdropCanvas) {
    backdropCanvas = document.createElement("canvas");
    backdropCanvas.width = w;
    backdropCanvas.height = h;
    paintDesktop(backdropCanvas.getContext("2d")!, w, h, 0);
  }
  return backdropCanvas.toDataURL("image/png");
}

const FRAME_NONE = { padding: 0, corner_radius: 0, shadow: 0 };
const FRAME_SUBTLE = { padding: 0.04, corner_radius: 0.012, shadow: 0.3 };
const FRAME_STUDIO = { padding: 0.075, corner_radius: 0.02, shadow: 0.5 };
// Mirrors vuoom_project::Background::preset (RGB 0..1).
const BG_PRESETS: Record<string, [number[], number[]] | [number[]]> = {
  graphite: [[0.16, 0.16, 0.17], [0.04, 0.04, 0.05]],
  slate: [[0.2, 0.24, 0.3], [0.07, 0.09, 0.12]],
  teal: [[0.06, 0.2, 0.21], [0.02, 0.08, 0.09]],
  dusk: [[0.17, 0.19, 0.26], [0.06, 0.06, 0.1]],
  paper: [[0.96, 0.95, 0.92], [0.85, 0.83, 0.78]],
  midnight: [[0.06, 0.07, 0.1], [0.01, 0.01, 0.02]],
  solid: [[0.09, 0.09, 0.1]],
};
function bgInfo(name: string): Pick<FrameInfo, "bg_kind" | "bg_from" | "bg_to" | "bg_angle"> {
  const p = BG_PRESETS[name] ?? BG_PRESETS.graphite;
  const from = p[0] as [number, number, number];
  const to = (p[1] ?? p[0]) as [number, number, number];
  return { bg_kind: p.length > 1 ? "gradient" : "solid", bg_from: from, bg_to: to, bg_angle: 45 };
}

export { MockEngine, paintDesktop };
export const mockEngine = new MockEngine();
