//! Record the main display for two seconds and report what arrived (a manual smoke test:
//! `cargo run -p vuoom-capture --example grab`).

fn main() {
    use vuoom_capture::{spawn_capture, CaptureOptions, CaptureSource};
    let opts = CaptureOptions {
        max_fps: 30,
        cursor: true,
    };
    let crop = std::env::args().nth(1).map(|_| vuoom_capture::CropRegion {
        x: 100,
        y: 100,
        w: 640,
        h: 360,
    });
    let (rx, handle) = spawn_capture(crop, &CaptureSource::Monitor { name: None }, opts);
    println!("backend: {:?}", handle.backend());
    let start = std::time::Instant::now();
    let mut n = 0;
    let mut first = None;
    while start.elapsed() < std::time::Duration::from_secs(2) {
        if let Ok(f) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
            n += 1;
            first.get_or_insert((f.width, f.height, f.bgra.len(), f.qpc));
            handle.recycle(f.bgra);
        }
    }
    handle.stop();
    println!(
        "frames: {n} in 2 s, first: {first:?}, dropped: {}",
        handle.dropped()
    );
    let now = vuoom_input::Clock::new().now();
    if let Some((_, _, _, qpc)) = first {
        println!(
            "first frame stamped {:.3} s before now",
            (now - qpc) as f64 / 1e9
        );
    }
}
