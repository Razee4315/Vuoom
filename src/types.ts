// Shared editor types. These mirror the src-tauri / vuoom_* serde shapes and are
// imported across the frontend. Keep names identical to their App.tsx origins.

export type Tool = "select" | "zoom" | "text" | "arrow" | "shape" | "highlight" | "mask";
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
  /// Mirrors vuoom_project::ArrowStyle, externally tagged unit variants serialize as the name.
  style?: "Arrow" | "Line" | "DoubleArrow";
  range: TimeRange;
}
export interface BoxAnn {
  id: number;
  rect: { x: number; y: number; w: number; h: number };
  color: Color;
  thickness: number;
  filled: boolean;
  shape: "Rect" | "Ellipse" | "Mask";
  range: TimeRange;
}
export interface AnnotationSet {
  texts: TextAnn[];
  arrows: ArrowAnn[];
  highlights: BoxAnn[];
}

/** How a zoom picks its focus, mirrors vuoom_zoom::ZoomMode's serde shape. */
export type ZoomMode = "Auto" | { Manual: { pos: SerVec } };
/** Easing preset for a zoom, mirrors vuoom_zoom::ZoomStyle (externally tagged unit variants). */
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
/** Mirrors vuoom_project::CropRect (normalized source-space rect). */
export interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Mirrors src-tauri session::FrameInfo: exact frame values (fractions of output height). */
export interface FrameInfo {
  padding: number;
  corner_radius: number;
  shadow: number;
  bg_kind: "solid" | "gradient";
  bg_from: [number, number, number];
  bg_to: [number, number, number];
  bg_angle: number;
}

/** Mirrors src-tauri session::ClipState. */
export interface ClipState {
  duration: number;
  trim: Trim | null;
  speed_regions: SpeedRegion[];
  cuts: Trim[];
  zooms: ZoomSeg[];
  show_clicks: boolean;
  /** Blur along camera moves (absent from engines that predate it: treated as on). */
  motion_blur?: boolean;
  show_keys: boolean;
  crop: CropRect | null;
  frame_preset: string;
  background_preset: string;
  frame: FrameInfo;
  /** Recorded audio tracks (absent from engines that predate audio). */
  audio?: AudioTrack[];
  /** The webcam bubble, when the take recorded the camera. */
  camera?: CameraOverlay | null;
  /** The re-drawn pointer, if on. */
  cursor?: CursorStyle | null;
  /** Whether the real pointer is in the recorded frames. */
  pointer_captured?: boolean;
  /** Timed captions (absent from engines that predate them). */
  captions?: Caption[];
  caption_style?: CaptionStyle;
}

/** Mirrors vuoom_project::Caption: one caption cue, in source time. */
export interface Caption {
  id: number;
  text: string;
  range: TimeRange;
}

/** Mirrors vuoom_project::CaptionStyle. */
export interface CaptionStyle {
  visible: boolean;
  /** Text height as a fraction of the output height, 0.025..0.09. */
  size: number;
  position: "bottom" | "top";
}

/** Mirrors src-tauri commands::CaptionsStatus. */
export interface CaptionsStatus {
  /** Whether this processor can run the speech model. */
  supported: boolean;
  /** Whether the speech model is already downloaded. */
  model_ready: boolean;
  model_bytes: number;
}

/** Mirrors src-tauri commands::CaptionsProgress (the `captions-progress` event). */
export interface CaptionsProgress {
  step: "download" | "listen";
  done: number;
  total: number;
}

/** Mirrors vuoom_project::CursorStyle. */
export interface CursorStyle {
  /** Size multiplier, 0.5..3. */
  size: number;
  /** Path smoothing in seconds, 0..0.2. */
  smoothing: number;
  /** Fade out while resting, back in before moving (absent from older engines). */
  hide_idle?: boolean;
}

/** How new takes handle the mouse pointer. */
export type CursorMode = "smooth" | "show" | "hide";

/** Which corner of the framed recording the webcam bubble sits in. */
export type CameraCorner = "top-left" | "top-right" | "bottom-left" | "bottom-right";
export type CameraShape = "circle" | "square" | "wide";

/** Mirrors vuoom_project::CameraOverlay. */
export interface CameraOverlay {
  visible: boolean;
  corner: CameraCorner;
  /** Bubble height as a fraction of the output height, 0.12..0.6. */
  size: number;
  shape: CameraShape;
  mirror: boolean;
  /** Source time of the camera track's clock zero (engine-managed). */
  offset: number;
}

/** Mirrors src-tauri camera::CameraDevice. */
export interface CameraDevice {
  id: string;
  name: string;
}

/** Where an audio track was recorded from. */
export type AudioKind = "mic" | "system";

/** Mirrors vuoom_project::AudioTrack. */
export interface AudioTrack {
  kind: AudioKind;
  /** Source time (s) of the track's first sample. */
  offset: number;
  /** Linear volume, 0..4. */
  gain: number;
  muted: boolean;
  /** Voice clean-up: remove background noise (absent from engines that predate it). */
  denoise?: boolean;
  /** Voice clean-up: even out the volume. */
  level?: boolean;
}

/** Mirrors src-tauri audio::AudioDevices. */
export interface AudioDevices {
  inputs: { id: string; name: string; is_default: boolean }[];
  has_output: boolean;
}

/** Mirrors src-tauri displays::DisplayInfo. */
export interface DisplayInfo {
  name: string;
  index: number;
  x: number;
  y: number;
  w: number;
  h: number;
  primary: boolean;
}

/** Mirrors the src-tauri window enumeration (window capture source). */
export interface WindowInfo {
  hwnd: number;
  title: string;
  w: number;
  h: number;
}

export type Kind = "text" | "arrow" | "box";
export interface Selection {
  kind: Kind;
  id: number;
}

// Drag state for the interactive overlay.
export type Drag =
  | { mode: "create-arrow"; start: Vec2; cur: Vec2 }
  | { mode: "create-zoom"; start: Vec2; cur: Vec2 }
  | { mode: "create-box"; start: Vec2; cur: Vec2 }
  | { mode: "create-ellipse"; start: Vec2; cur: Vec2 }
  | { mode: "create-highlight"; start: Vec2; cur: Vec2 }
  | { mode: "create-mask"; start: Vec2; cur: Vec2 }
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
