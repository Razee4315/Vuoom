# Smooth pointer: a clean cursor re-drawn from the input log

A real mouse pointer in a screen recording jitters, stutters where frames were skipped, and
is often too small to follow once the video is scaled down for a README. Vuoom can instead
record **without** the pointer and draw a clean one afterwards. Code:
`crates/vuoom-render/src/cursor.rs`, `scene.rs`, `shapes.rs`.

## Recording

The pointer setting has three modes (Settings > Recording, and the region HUD's options):

| Mode | Capture | Project |
|---|---|---|
| **Smooth** (default for new installs) | WGC leaves the pointer out | `pointer_captured = false`, `cursor = Some(default)` |
| Recorded | the real pointer is in the frames | `pointer_captured = true`, `cursor = None` |
| Hidden | no pointer | `pointer_captured = false`, `cursor = None` |

Existing installs that had chosen show or hide keep that. The pointer's movement is in the
input log either way (zooms already follow it), so nothing else changes about recording. The
placeholder manifest written at record start carries both fields, so a crash-recovered take
still gets its pointer.

## The path

- **Interpolation that respects rest.** The mouse hook only fires on movement, so a gap in the
  log means the pointer was still. Between samples closer than 60 ms the path is interpolated;
  across a longer gap the pointer holds its position (a naive lerp would drift it across the
  screen during every pause).
- **Centered smoothing, no lag.** Export knows the future of the path, so the smoothed
  position is a Gaussian-weighted average of the path at nine points spanning ±2σ around the
  frame's time. Hand jitter averages out and the pointer never trails behind. σ is the
  "Smoothing" slider (0 to 0.2 s, default 0.05 s).
- **Cheap per frame.** Every lookup is a binary search into the time-sorted log, so a long
  take with hundreds of thousands of moves still resolves each frame in microseconds.
- **Clicks press it.** A click dips the pointer to 86% for 60 ms and eases it back over 220 ms.

## Drawing

The pointer is a 7-point arrow polygon (five triangles) drawn by the same shape pipeline as
arrows and ripples, last so it sits above what it points at: a soft offset shadow, a dark
outline (the polygon grown along miter normals), then the white body. It is mapped through
the camera like click ripples, so it stays glued to the content, and it **scales with the
zoom**, because a real pointer is part of the picture. When the camera has moved away from it,
it isn't drawn. Size is relative to the recorded screen: 1.0 is about a real pointer on a
1080p display, and the default is 1.5 so it stays legible in a scaled-down GIF.

## Hide when still

A pointer parked on the screen while you talk is clutter. With **Hide when still** on
(`CursorStyle.hide_idle`, off by default), the pointer fades out after 1.5 s without moving
or clicking, over 0.35 s, shrinking slightly as it goes. Export knows when the next movement
comes, so the pointer fades back in over the 0.2 s *before* it, and is fully there the moment
it moves: it never pops in late or starts a gesture invisible. Typing doesn't count as
movement. Before the first logged event, rest is counted from the start of the take.
(`idle_opacity` in `cursor.rs`; the opacity rides on `ResolvedCursor` into `shapes.rs`.)

## Looks

`CursorStyle.look` picks what is drawn:

- **Classic**: the arrow above, following the real pointer's shape (hand, text beam...).
- **Dot**: a round dot centered on the point, with the same outline and shadow.
- **Picture**: the user's own file (`CursorStyle.image`: its path and a hotspot, the point
  that clicks, as fractions of its width and height). The file is read through WIC with its
  transparency, premultiplied, shrunk to 512 px at most and uploaded with a full mip chain,
  since a pointer is mostly drawn far smaller than its picture. It is drawn as tall as the
  classic pointer would be, by `shaders/sprite.wgsl`, which also casts a soft shadow from
  the picture's own shape. A picture that can no longer be read (moved, deleted) falls back
  to the classic pointer, and the log says why. The picture replaces every pointer shape.

The color applies to the classic pointer and the dot; the shadow (`shadow`, on by default)
and the highlight disc apply to all three.

## Opacity

`CursorStyle.opacity` (0 to 1) fades the whole pointer. The drawn pointer is layers on top of
each other (shadow, outline, body), so fading each one would let the outline show through
the body. Instead, a see-through pointer (from the opacity or the idle fade) is drawn opaque
into its own texture first and that layer is blended onto the frame once, by the same sprite
shader as the picture. An opaque pointer skips the extra pass.

## Click effects

With click effects on (`Project.show_clicks`), every recorded click plays
`Project.click_style`: one of four animations (**Ripple**, the classic single ring;
**Rings**, a second ring a third of the way behind; **Pulse**, a soft disc under a crisp
edge; **Burst**, eight dots flying outward), in any color, at 0.5 to 3× size, over 0.2 to
1.5 s, with a starting opacity. They are plain circles in the shape pipeline
(`click_marks` in `scene.rs`), mapped through the camera so they stay on the content.

## Editor

Clip > Pointer toggles it and sets the look, size (0.5 to 3×), smoothing, hide when still,
opacity, shadow, color and highlight. A picture's hotspot is set by clicking on its preview.
Clip > Overlays holds the click effect. The panel explains the two confusing cases: a
re-drawn pointer on a take that also captured the real one (two pointers), and a take with
no visible pointer at all.

The pointer's look and the click style last chosen are remembered (`pointer-look` and
`click-style` preferences) and sent with the take defaults, so a new take starts with them
and a custom pointer is set up once.
