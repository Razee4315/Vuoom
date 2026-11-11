//! Record two seconds of the microphone and of system sound and report levels and sizes
//! (a manual smoke test: `cargo run -p vuoom-audio --example listen`).

fn main() {
    use vuoom_audio::{Anchor, Recorder, Source};
    println!("inputs: {:?}", vuoom_audio::list_inputs());
    let clock = vuoom_input::Clock::new();
    let dir = std::env::temp_dir();
    for (name, source) in [("mic", Source::Mic(None)), ("system", Source::System)] {
        let anchor = Anchor {
            start_qpc: clock.now(),
            freq: clock.freq(),
        };
        let path = dir.join(format!("vuoom-listen-{name}.wav"));
        match Recorder::start(source, Some(path.clone()), anchor) {
            Ok(rec) => {
                let mut peak = 0f32;
                for _ in 0..20 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    peak = peak.max(rec.level());
                }
                let frames = rec.finish();
                println!("{name}: frames {frames:?}, peak {peak:.3}");
                // A recording of the room is nobody's business: don't leave it lying around.
                let _ = std::fs::remove_file(&path);
            }
            Err(e) => println!("{name}: failed to start: {e}"),
        }
    }
}
