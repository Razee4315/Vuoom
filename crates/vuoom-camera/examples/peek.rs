//! List cameras and grab two seconds from the default one (a manual smoke test:
//! `cargo run -p vuoom-camera --example peek`).

fn main() {
    println!("cameras: {:?}", vuoom_camera::list_cameras());
    let rec = match vuoom_camera::CameraRecorder::open(None) {
        Ok(r) => r,
        Err(e) => {
            println!("open failed: {e}");
            return;
        }
    };
    println!("size: {:?}", rec.size());
    let path = std::env::temp_dir().join("vuoom-peek.vcam");
    let clock = vuoom_input::Clock::new();
    let cam_clock = vuoom_camera::Clock {
        start_qpc: clock.now(),
        freq: clock.freq(),
    };
    rec.record(&path, cam_clock).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    let latest = rec.latest_jpeg().map(|j| j.len());
    if let Some(j) = rec.latest_jpeg() {
        let _ = std::fs::write(std::env::temp_dir().join("vuoom-peek.jpg"), &*j);
    }
    println!("latest jpeg: {latest:?} bytes, frames: {:?}", rec.finish());
}
