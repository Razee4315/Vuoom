// Shared editor types. These mirror the src-tauri / vuoom_* serde shapes and are
// imported across the frontend. Keep names identical to their App.tsx origins.

export type Tool = "select" | "text" | "arrow" | "line" | "shape" | "highlight";
export type Vec2 = { x: number; y: number };

/** Mirrors src-tauri session::RecordingSummary. */
export interface RecordingSummary {
  duration: number;
  frames: number;
  zooms: number;
  /** Set when the take was truncated (e.g. the disk filled mid-recording). */
  warning?: string | null;
}

export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}
export interface TimeRange {
  start: number;
  end: number;
  fade_in: number;
  fade_out: number;
}
// glam DVec2 serializes as a [x, y] array.
export type SerVec = [number, number];
export interface TextAnn {
  id: number;
  text: string;
  pos: SerVec;
  font_size: number;
  color: Color;
  bold: boolean;
  italic: boolean;
  background: boolean;
  font: string;
  range: TimeRange;
}
export interface ArrowAnn {
  id: number;
  from: SerVec;
  to: SerVec;
  color: Color;
  thickness: number;
  /// Mirrors vuoom_project::ArrowStyle — externally tagged unit variants serialize as the name.
  style?: "Arrow" | "Line" | "DoubleArrow";
  range: TimeRange;
}
export interface BoxAnn {
  id: number;
  rect: { x: number; y: number; w: number; h: number };
  color: Color;
  thickness: number;
  filled: boolean;
  shape: "Rect" | "Ellipse";
  range: TimeRange;
}
export interface AnnotationSet {
  texts: TextAnn[];
  arrows: ArrowAnn[];
  highlights: BoxAnn[];
}

/** How a zoom picks its focus — mirrors vuoom_zoom::ZoomMode's serde shape. */
export type ZoomMode = "Auto" | { Manual: { pos: SerVec } };
/** Easing preset for a zoom — mirrors vuoom_zoom::ZoomStyle (externally tagged unit variants). */
export type ZoomStyle = "Smooth" | "Snappy" | "Slow";
/** Mirrors vuoom_zoom::ZoomKeyframe. */
export interface ZoomSeg {
  start: number;
  end: number;
  amount: number;
  mode: ZoomMode;
  style: ZoomStyle;
}
export interface SpeedRegion {
  start: number;
  end: number;
  factor: number;
}
export interface Trim {
  start: number;
  end: number;
}
/** Mirrors src-tauri session::ClipState. */
export interface ClipState {
  duration: number;
  trim: Trim | null;
  speed_regions: SpeedRegion[];
  cuts: Trim[];
  zooms: ZoomSeg[];
  show_clicks: boolean;
  show_keys: boolean;
  frame_preset: string;
  background_preset: string;
}

export type Kind = "text" | "arrow" | "box";
export interface Selection {
  kind: Kind;
  id: number;
}

// Drag state for the interactive overlay.
export type Drag =
  | { mode: "create-arrow"; start: Vec2; cur: Vec2 }
  | { mode: "create-line"; start: Vec2; cur: Vec2 }
  | { mode: "create-box"; start: Vec2; cur: Vec2 }
  | { mode: "create-ellipse"; start: Vec2; cur: Vec2 }
  | { mode: "create-highlight"; start: Vec2; cur: Vec2 }
  // `group` carries the OTHER selected annotations so a canvas drag of any member
  // translates the whole multi-selection rigidly (empty/undefined for a lone selection).
  | {
      mode: "move";
      kind: Kind;
      id: number;
      grab: Vec2;
      orig: number[];
      geom: number[];
      group?: { kind: Kind; id: number; orig: number[]; geom: number[] }[];
    }
  | { mode: "resize"; kind: Kind; id: number; handle: string; orig: number[]; geom: number[] }
  // Corner-dragging a text label scales its font size (anchored to the opposite corner),
  // so text scales typographically instead of stretching.
  | { mode: "scale-text"; id: number; anchor: Vec2; startFont: number; startDist: number; cur: number }
  | null;
