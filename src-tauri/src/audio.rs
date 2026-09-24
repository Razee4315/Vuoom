//! Session-side audio: starting a take's captures, turning them into project tracks,
//! carrying the WAVs through bundles and crash recovery, and mixing them for export.
//!
//! Each take stores its audio next to its frames (`mic.wav`, `system.wav`) plus a small
//! `audio.json` recording the counter-clock instant sample 0 belongs to. A track with voice
//! clean-up plays a processed copy cached beside the recording (`mic.denoise.wav`, ...).
//! See `docs/14-Audio.md` and `docs/16-Voice-Cleanup.md`.

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use vuoom_audio::{Anchor, MixTrack, Pcm, Plan, Recorder, Segment, Source};
use vuoom_project::{output_segments, AudioKind, AudioTrack, Project, SpeedRegion, Trim};

/// Sidecar holding the take's audio anchor (for re-aligning a recovered take).
const AUDIO_META: &str = "audio.json";

/// Which audio the next recording captures.
#[derive(Debug, Clone, Default)]
pub struct AudioChoice {
    pub mic: bool,
    /// Microphone endpoint id; `None` = the system default.
    pub mic_device: Option<String>,
    pub system: bool,
}

/// One microphone, as the recording UI lists it.
#[derive(Debug, Clone, Serialize)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// The recording UI's audio options.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevices {
    pub inputs: Vec<InputDevice>,
    /// Whether system sound can be recorded (a default output device exists).
    pub has_output: bool,
}

/// List microphones (default first) and whether system sound is available.
pub fn devices() -> Result<AudioDevices, String> {
    let inputs = vuoom_audio::list_inputs()?
        .into_iter()
        .map(|d| InputDevice {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        })
        .collect();
    Ok(AudioDevices {
        inputs,
        has_output: vuoom_audio::has_output(),
    })
}

/// A take's running captures.
pub type Captures = Vec<(AudioKind, Recorder)>;

/// Start the chosen captures for a take stored in `dir`. A source that can't be opened is
/// skipped with a warning: a recording never fails over audio.
pub fn start(choice: &AudioChoice, dir: &Path, anchor: Anchor) -> (Captures, Vec<String>) {
    let mut wanted = Vec::new();
    if choice.mic {
        wanted.push((AudioKind::Mic, Source::Mic(choice.mic_device.clone())));
    }
    if choice.system {
        wanted.push((AudioKind::System, Source::System));
    }
    let mut started = Vec::new();
    let mut failed = Vec::new();
    for (kind, source) in wanted {
        match Recorder::start(source, Some(dir.join(kind.file_name())), anchor) {
            Ok(r) => started.push((kind, r)),
            Err(e) => {
                tracing::warn!("{kind:?} audio not recorded: {e}");
                failed.push(e);
            }
        }
    }
    if !started.is_empty() {
        let meta = serde_json::json!({ "start_qpc": anchor.start_qpc, "freq": anchor.freq });
        let _ = std::fs::write(dir.join(AUDIO_META), meta.to_string());
    }
    (started, failed)
}

/// Stop a take's captures. Returns the tracks that recorded something, and failures.
pub fn finish(captures: Captures) -> (Vec<AudioTrack>, Vec<String>) {
    let mut tracks = Vec::new();
    let mut failed = Vec::new();
    for (kind, rec) in captures {
        match rec.finish() {
            Ok(frames) if frames > 0 => tracks.push(AudioTrack::new(kind)),
            Ok(_) => tracing::warn!("{kind:?} audio captured no samples"),
            Err(e) => {
                tracing::warn!("{kind:?} audio failed: {e}");
                failed.push(e);
            }
        }
    }
    (tracks, failed)
}

/// The user-facing note for audio that couldn't be recorded, if any.
pub fn warning(failed: &[String]) -> Option<String> {
    if failed.is_empty() {
        None
    } else {
        Some(format!("Audio was not recorded: {}.", failed.join("; ")))
    }
}

/// Peak level of the capture of `kind`, if one is running.
pub fn level(captures: &Captures, kind: AudioKind) -> f32 {
    captures
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(0.0, |(_, r)| r.level())
}

/// Copy the WAVs behind `tracks` from one directory to another (bundle save/open). A
/// cleaned-up copy travels too when there is one, so it needn't be made again.
pub fn copy_tracks(from: &Path, to: &Path, tracks: &[AudioTrack]) -> Result<(), String> {
    for t in tracks {
        let cleaned = t.cleaned_file_name();
        let names = [Some(t.kind.file_name()), cleaned.as_deref()];
        for name in names.into_iter().flatten() {
            let src = from.join(name);
            if src.is_file() {
                std::fs::create_dir_all(to)
                    .and_then(|()| std::fs::copy(&src, to.join(name)))
                    .map_err(|e| format!("audio: {e}"))?;
            }
        }
    }
    Ok(())
}

/// One clean-up at a time: preview and export asking for the same track at once wait for
/// a single pass and share its cached result.
static CLEANING: Mutex<()> = Mutex::new(());

/// A track's audio as it plays: the recording itself or, with voice clean-up on, its
/// processed copy (made on first use and cached beside the recording).
pub fn track_pcm(dir: &Path, t: &AudioTrack) -> Result<Pcm, String> {
    let raw = dir.join(t.kind.file_name());
    let Some(name) = t.cleaned_file_name() else {
        return vuoom_audio::wav::read(&raw);
    };
    let cached = dir.join(name);
    let _one = CLEANING.lock().unwrap_or_else(|e| e.into_inner());
    if is_fresh(&cached, &raw) {
        match vuoom_audio::wav::read(&cached) {
            Ok(pcm) => return Ok(pcm),
            Err(e) => tracing::warn!("cached clean-up unreadable, redoing it: {e}"),
        }
    }
    let pcm = vuoom_audio::clean::clean(&vuoom_audio::wav::read(&raw)?, t.denoise, t.level);
    // Written aside and renamed, so a half-written copy is never mistaken for a cache.
    let part = cached.with_extension("wav.part");
    let saved = std::fs::write(&part, vuoom_audio::wav::encode(&pcm))
        .and_then(|()| std::fs::rename(&part, &cached));
    if let Err(e) = saved {
        tracing::warn!("{:?} clean-up not cached: {e}", t.kind);
        let _ = std::fs::remove_file(&part);
    }
    Ok(pcm)
}

/// Whether `cache` exists and was written no earlier than `source` last changed.
fn is_fresh(cache: &Path, source: &Path) -> bool {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    matches!((modified(cache), modified(source)), (Some(c), Some(s)) if c >= s)
}

/// Re-attach a recovered take's audio. A crash leaves only the startup manifest, so tracks
/// are rediscovered from the WAVs on disk; and since a recovered timeline starts at the
/// first frame rather than the original start instant, every track is shifted by the gap.
pub fn recover(dir: &Path, project: &mut Project, first_frame_qpc: i64) {
    if project.audio.is_empty() {
        project.audio = [AudioKind::Mic, AudioKind::System]
            .into_iter()
            .filter(|k| dir.join(k.file_name()).is_file())
            .map(AudioTrack::new)
            .collect();
    }
    let meta = std::fs::read_to_string(dir.join(AUDIO_META))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let anchor = meta.and_then(|m| Some((m["start_qpc"].as_i64()?, m["freq"].as_i64()?)));
    if let Some((start_qpc, freq)) = anchor.filter(|&(_, f)| f > 0) {
        let offset = -((first_frame_qpc - start_qpc) as f64 / freq as f64);
        for t in &mut project.audio {
            t.offset = offset;
        }
    }
}

/// Read one track for mixing, with its gain and offset; `None` (logged) if unreadable.
fn load_track(dir: &Path, t: &AudioTrack) -> Option<(Pcm, f32, f64)> {
    match track_pcm(dir, t) {
        Ok(pcm) => Some((pcm, t.effective_gain(), t.offset)),
        Err(e) => {
            tracing::warn!("{:?} audio skipped in export: {e}", t.kind);
            None
        }
    }
}

/// The audible tracks of a project, loaded and ready to mix for the played timeline.
pub struct Mix {
    tracks: Vec<(Pcm, f32, f64)>,
    plan: Plan,
}

impl Mix {
    /// Load the project's audible tracks from `dir` and lay them out on the output timeline
    /// described by the export mapping (`t0`, `span`, trim-local `regions` and `cuts`).
    /// `None` when nothing audible is left (no tracks, all muted, or unreadable files).
    pub fn load(
        dir: &Path,
        project: &Project,
        t0: f64,
        span: f64,
        regions: &[SpeedRegion],
        cuts: &[Trim],
    ) -> Option<Self> {
        let tracks: Vec<(Pcm, f32, f64)> = project
            .audio
            .iter()
            .filter(|t| t.effective_gain() > 0.0)
            .filter_map(|t| load_track(dir, t))
            .collect();
        if tracks.is_empty() {
            return None;
        }
        let segments: Vec<Segment> = output_segments(span, regions, cuts)
            .into_iter()
            .map(|(s, e, factor)| Segment {
                src_start: t0 + s,
                src_end: t0 + e,
                factor,
            })
            .collect();
        Some(Self {
            tracks,
            plan: Plan::new(&segments, vuoom_audio::OUTPUT_RATE),
        })
    }

    /// Total output frames at [`vuoom_audio::OUTPUT_RATE`].
    pub fn frames(&self) -> u64 {
        self.plan.frames()
    }

    /// Interleaved stereo samples for output frames `[start, start + n)`.
    pub fn render(&self, start: u64, n: usize) -> Vec<i16> {
        let tracks: Vec<MixTrack<'_>> = self
            .tracks
            .iter()
            .map(|(pcm, gain, offset)| MixTrack {
                pcm,
                gain: *gain,
                offset: *offset,
            })
            .collect();
        self.plan.render(&tracks, start, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vuoom_project::SourceInfo;

    fn project() -> Project {
        Project::new(SourceInfo {
            path: String::new(),
            width: 100,
            height: 100,
            fps: 30.0,
            duration: 4.0,
        })
    }

    #[test]
    fn recovery_rediscovers_tracks_and_shifts_them() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mic.wav"), b"x").unwrap();
        std::fs::write(
            dir.path().join(AUDIO_META),
            r#"{"start_qpc": 1000, "freq": 1000}"#,
        )
        .unwrap();
        let mut p = project();
        // The first frame landed 50 ms after the original start.
        recover(dir.path(), &mut p, 1050);
        assert_eq!(p.audio.len(), 1);
        assert_eq!(p.audio[0].kind, AudioKind::Mic);
        assert!((p.audio[0].offset + 0.05).abs() < 1e-9);
    }

    #[test]
    fn mix_skips_muted_and_missing_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = project();
        p.audio = vec![AudioTrack::new(AudioKind::System)];
        assert!(Mix::load(dir.path(), &p, 0.0, 4.0, &[], &[]).is_none());

        let pcm = Pcm {
            rate: 8_000,
            channels: 1,
            samples: vec![1_000; 32_000],
        };
        std::fs::write(
            dir.path().join("system.wav"),
            vuoom_audio::wav::encode(&pcm),
        )
        .unwrap();
        let mix = Mix::load(dir.path(), &p, 0.0, 4.0, &[], &[]).unwrap();
        assert_eq!(mix.frames(), 4 * u64::from(vuoom_audio::OUTPUT_RATE));
        assert!(mix.render(96_000, 1)[0] > 0);

        p.audio[0].muted = true;
        assert!(Mix::load(dir.path(), &p, 0.0, 4.0, &[], &[]).is_none());
    }

    #[test]
    fn trim_and_cuts_shape_the_mix() {
        let dir = tempfile::tempdir().unwrap();
        let pcm = Pcm {
            rate: 8_000,
            channels: 1,
            samples: vec![1_000; 32_000],
        };
        std::fs::write(dir.path().join("mic.wav"), vuoom_audio::wav::encode(&pcm)).unwrap();
        let mut p = project();
        p.audio = vec![AudioTrack::new(AudioKind::Mic)];
        // Trimmed to [1, 3] with a trim-local cut at [0.5, 1.0]: 1.5 s plays.
        let cuts = [Trim {
            start: 0.5,
            end: 1.0,
        }];
        let mix = Mix::load(dir.path(), &p, 1.0, 2.0, &[], &cuts).unwrap();
        assert_eq!(mix.frames(), 72_000);
    }

    #[test]
    fn cleaned_tracks_are_made_once_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let pcm = Pcm {
            rate: 16_000,
            channels: 1,
            samples: vec![0; 16_000],
        };
        std::fs::write(dir.path().join("mic.wav"), vuoom_audio::wav::encode(&pcm)).unwrap();
        let mut t = AudioTrack::new(AudioKind::Mic);
        // No clean-up: the recording as it is.
        assert_eq!(track_pcm(dir.path(), &t).unwrap(), pcm);
        t.denoise = true;
        let cleaned = track_pcm(dir.path(), &t).unwrap();
        assert_eq!((cleaned.rate, cleaned.frames()), (48_000, 48_000));
        let cache = dir.path().join("mic.denoise.wav");
        assert!(cache.is_file());
        // A second request reads the cache (a marker sample proves it).
        let mut marked = cleaned.clone();
        marked.samples[0] = 123;
        std::fs::write(&cache, vuoom_audio::wav::encode(&marked)).unwrap();
        assert_eq!(track_pcm(dir.path(), &t).unwrap().samples[0], 123);
        // Other settings get their own copy.
        t.level = true;
        assert_eq!(track_pcm(dir.path(), &t).unwrap().samples[0], 0);
        assert!(dir.path().join("mic.denoise-level.wav").is_file());
    }

    #[test]
    fn failures_become_one_warning() {
        assert_eq!(warning(&[]), None);
        let w = warning(&["microphone unavailable".into()]).unwrap();
        assert!(w.contains("microphone unavailable"));
    }
}
