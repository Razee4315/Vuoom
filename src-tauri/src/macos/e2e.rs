//! End-to-end checks of the engine on a real Mac: the sample take through the Metal
//! compositor into GIF and MP4, and a real screen recording through ScreenCaptureKit and the
//! event tap. They need a GPU and (for recording) the Screen Recording permission, so they
//! are `#[ignore]`d; run them with
//! `cargo test -p vuoom --lib macos_e2e -- --ignored --nocapture`.
//! Set `VUOOM_E2E_OUT` to keep the exports.

use crate::session::{MonitorInfo, Session};
use std::path::PathBuf;
use std::time::Duration;

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("VUOOM_E2E_OUT")
        .map_or_else(|| std::env::temp_dir().join("vuoom-e2e"), PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

#[test]
#[ignore = "needs a GPU; run by hand on a Mac"]
fn sample_take_exports_gif_and_mp4() {
    let session = Session::new().expect("engine boots");
    println!("GPU: {}", session.gpu_summary());
    assert!(session.has_gpu(), "no Metal adapter");
    let summary = session.load_sample().expect("sample loads");
    println!("sample: {summary:?}");
    assert!(summary.frames > 10);
    session.seek(1.0, Some(960)).expect("a frame composites");

    let dir = out_dir();
    let gif = dir.join("sample.gif");
    session
        .export_gif(
            gif.to_string_lossy().into(),
            15,
            Some(640),
            80,
            true,
            &|_, _| {},
        )
        .expect("GIF export");
    println!("GIF {} bytes", size(&gif));
    assert!(size(&gif) > 10_000);

    let mp4 = dir.join("sample.mp4");
    session
        .export_mp4(
            mp4.to_string_lossy().into(),
            30,
            Some(1280),
            85,
            true,
            &|_, _| {},
        )
        .expect("MP4 export");
    println!("MP4 {} bytes", size(&mp4));
    assert!(size(&mp4) > 10_000);
}

#[test]
#[ignore = "records the real screen; needs the Screen Recording permission"]
fn records_a_region_of_the_main_display() {
    let session = Session::new().expect("engine boots");
    let main = crate::displays::enumerate()
        .into_iter()
        .find(|d| d.primary)
        .expect("a main display");
    session
        .set_monitor(Some(MonitorInfo {
            name: main.name.clone(),
            x: main.x,
            y: main.y,
            w: main.w,
            h: main.h,
        }))
        .unwrap();
    session
        .set_region(Some(vuoom_capture::CropRegion {
            x: 200,
            y: 200,
            w: 1280,
            h: 720,
        }))
        .unwrap();
    session.set_capture_fps(30).unwrap();
    session.start_recording().expect("recording starts");
    std::thread::sleep(Duration::from_secs(3));
    let summary = session.stop_recording().expect("recording stops");
    println!("take: {summary:?}");
    assert!(summary.frames >= 10, "{} frames", summary.frames);
    assert!(summary.duration > 2.0);

    let dir = out_dir();
    let mp4 = dir.join("take.mp4");
    session
        .export_mp4(
            mp4.to_string_lossy().into(),
            30,
            None,
            85,
            false,
            &|_, _| {},
        )
        .expect("MP4 export");
    println!("MP4 {} bytes at {}", size(&mp4), mp4.display());
    assert!(size(&mp4) > 10_000);
    // Leave no "unsaved take" behind for the real app to offer to recover.
    let _ = session.clear_recovery_storage();
}
