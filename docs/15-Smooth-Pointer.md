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

## Editor

Clip > Pointer toggles it and sets size (0.5 to 3×), smoothing and hide when still. The panel explains the two
confusing cases: a re-drawn pointer on a take that also captured the real one (two pointers),
and a take with no visible pointer at all.
