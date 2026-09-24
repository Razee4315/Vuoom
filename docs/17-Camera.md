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
- **Live preview.** The latest frame is kept as a JPEG for the recording UI's bubble. A
  recorder started without a file does only that.

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

## Tests

CI runs the format choice, sizing, downscaling, clock tie, the track format (round trip,
ordering, torn tails, foreign files) and a JPEG round trip through WIC. CI machines have no
camera, so capture itself is checked on real hardware.
