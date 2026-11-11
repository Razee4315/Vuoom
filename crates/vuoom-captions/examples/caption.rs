//! Transcribe a WAV with a whisper.cpp model and print the timed text (a manual smoke test:
//! `cargo run -p vuoom-captions --release --example caption -- MODEL.bin SPEECH.wav`).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (model, wav) = (
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
    );
    let pcm = vuoom_audio::wav::read(wav).expect("wav");
    let audio = vuoom_captions::to_16k_mono(&pcm);
    let start = std::time::Instant::now();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out = vuoom_captions::transcribe(model, &audio, Some("en"), |_| {}, cancel);
    println!("{:.2} s: {out:?}", start.elapsed().as_secs_f64());
}
