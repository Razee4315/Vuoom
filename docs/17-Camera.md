# Camera: a webcam bubble over the recording

Screen demos land better with a face in the corner. Vuoom records the webcam alongside the
screen as its own track, and draws it at export as a bubble you can place, size and shape
after the fact. Code: `crates/vuoom-camera`.

## Capture

`CameraRecorder` runs one thread per camera on Media Foundation's source reader, the same
platform stack MP4 export already uses, so there is nothing new to bundle.

- **Devices** are enumerated with `MFEnumDeviceSources` (video capture) and identified by
  their symbolic link, which stays stable across reboots and USB ports.
- **Format.** Of the formats the camera offers, `choose_format` takes one that runs at
  24 fps or better, the widest that is at most 960 px wide, then the rate nearest 30 fps.
  A 960×540 bubble source is plenty for an overlay a quarter of the frame tall, and keeps
  the track small. If a camera only offers larger formats, frames are area-averaged down
  (`fit`, `downscale`).
- **Conversion.** The reader is asked for RGB32 with video processing on, so Media
  Foundation converts from whatever the camera sends (MJPG, NV12, YUY2). Frames are read
  from 2D buffers when available (the pitch says which way up they are), else from the
  plain buffer using the default stride.
- **Time.** Each frame gets a time on the recording clock (the performance counter the
  screen frames and input log use). The reader's timestamps are steady but count from when
  the camera started, so `Pin` ties them to the recording clock at the first frame's
  arrival, and ties them again if they drift more than 0.25 s from arrival time.
- **Stopping never hangs a recording.** A camera that stops delivering (a privacy shutter,
  a driver hiccup) would block the reader thread, so `finish` waits at most 3 s, then
  keeps the frames already on disk and lets the thread end on its own. Shutting the media
  source down releases the device and turns its light off.
- **Open early, record instantly.** Opening a camera takes a moment (sometimes a second
  or two), so `CameraRecorder::open` runs while the user frames the shot, feeding the
  recording UI's live bubble with the latest frame as a JPEG. `record(path, clock)` then
  starts writing frames to a track from the very next frame: the recording never waits
  for the device.

## Storage

Frames are JPEG-encoded through the Windows Imaging Component (built into Windows, so no
codec crate or license to carry; the pure-Rust encoder carries the IJG license, which the
project does not allow) and appended to `camera.vcam` in the take's folder:

```
"VCAM" u32 version
{ u32 length | f64 time (s) | JPEG bytes } ...
```

Records are only appended, and the writer flushes every 30 frames, so a crash loses at
most the last second. The reader indexes records until the first one that runs past the end
of the file, so a torn final frame is dropped rather than failing the take. At 960×540 a
frame is about 50 KB: roughly 1.5 MB per second of recording.

`TrackReader::index_at(t)` returns the frame showing at `t` (the latest at or before it),
which is what preview and export draw.

## The bubble

`Project.camera: Option<CameraOverlay>` exists when the take recorded frames: `visible`,
`corner` (default bottom right), `size` (height as a fraction of the output, 0.12 to 0.6,
default 0.28), `shape` (circle, rounded square, rounded 16:9) and `mirror` (on by default,
since people expect to see themselves mirrored). It also holds `offset`: the source time of
the track's clock zero. That is 0 for a normal take and negative for a recovered one,
whose timeline starts at the first surviving frame, exactly like audio.

- **Placement** (`scene::place_camera`): the bubble sits in its corner of the framed
  recording, 3.5% of the output height in from the edges. It does not follow the zoom.
  The scene carries the camera-track time to show (`t - offset`).
- **Drawing** (`shaders/camera.wgsl`): a quad around the bubble plus its shadow. The frame
  is center-cropped to the bubble's aspect (mirroring is a right-to-left crop), clipped by
  the same rounded-box SDF as the recording (a circle at half the height), anti-aliased,
  with a soft drop shadow and a thin light rim so it separates from dark content. It's
  blended premultiplied, after the recording and before annotations and the pointer.
- **Frames**: preview and export look up the frame showing at that time
  (`TrackReader::index_at`), decode it through WIC, and keep the last decoded frame, since
  consecutive output frames usually show the same camera frame. The preview's reader lives
  with the loaded clip and is dropped with it, so no file handle outlives the clip.
- **Takes and projects**: recording writes `camera.json` (the clock origin) beside the
  track for crash recovery. Bundles carry the track in a `camera` folder.

## The recording UI

- **Choosing a camera**: a Camera picker on Home and in Settings (off, or a device), and
  a compact camera button in the record HUD beside Audio. The choice is a preference
  (`recordCamera`, `cameraDevice`) pushed to the engine before every take.
- **The live bubble**: while the user frames the shot, the HUD opens the chosen camera and
  shows its picture in a bubble at the bottom right of the selected area, at the size a
  new take gives it (28% of the height, 3.5% in from the edges), so what you see is where
  you'll appear. The preview asks the engine for the latest frame about 15 times a second
  (`camera_preview_frame`, JPEG bytes into an object URL), shows a pulsing camera icon
  while the device starts, and takes no pointer input, so the selection stays draggable.
  Picking another device pushes the choice first, then reopens the preview.
- **The editor**: Clip > Camera shows the bubble's controls when the take has a camera:
  Show camera, a corner picker (a miniature frame with one cell per corner), Size (12% to
  60%), Shape (circle, square, wide) and Mirror. Slider drags coalesce into one undo step.
- **HUD fit**: with the camera button added, the HUD's FRAME and ZOOM captions are hidden
  below 1400 px wide, which keeps it to one row at 1280 px.

## Tests

CI runs the format choice, sizing, downscaling, clock tie, the track format (round trip,
ordering, torn tails, foreign files) and a JPEG round trip through WIC. CI machines have no
camera, so capture itself is checked on real hardware.
