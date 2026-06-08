//! End-to-end GIF export: RGBA frames → temp PNGs → gifski → optional gifsicle.
//!
//! `frames` are the already-selected output frames (use [`crate::plan_frames`] upstream to
//! pick them for the target fps). gifski downscales to `settings.width`. Everything runs
//! out-of-process (gifski/gifsicle are AGPL/GPL binaries we invoke, never link).

use crate::error::EncodeError;
use crate::gifski::{run_gifsicle, run_gifski};
use crate::image::{write_png, RgbaImage};
use crate::settings::GifSettings;
use std::path::Path;

/// Encode `frames` into a GIF at `out` using the gifski binary, with an optional gifsicle
/// shrink pass when `settings.lossy` and `gifsicle_bin` are both set.
///
/// # Errors
/// Returns [`EncodeError`] if there are no frames, a PNG/temp write fails, or an external
/// tool cannot be spawned or exits non-zero.
pub fn export_gif(
    frames: &[RgbaImage],
    settings: &GifSettings,
    gifski_bin: &Path,
    gifsicle_bin: Option<&Path>,
    out: &Path,
) -> Result<(), EncodeError> {
    if frames.is_empty() {
        return Err(EncodeError::NoFrames);
    }

    let dir = tempfile::tempdir()?;
    let mut paths = Vec::with_capacity(frames.len());
    for (i, frame) in frames.iter().enumerate() {
        let p = dir.path().join(format!("frame_{i:05}.png"));
        write_png(&p, frame)?;
        paths.push(p.to_string_lossy().into_owned());
    }

    let out_str = out.to_string_lossy().into_owned();
    run_gifski(gifski_bin, settings, &paths, &out_str)?;

    if let (Some(bin), Some(lossy)) = (gifsicle_bin, settings.lossy) {
        run_gifsicle(bin, lossy, &out_str)?;
    }
    // `dir` is removed when it drops here.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_frames_is_no_frames() {
        let err = export_gif(
            &[],
            &GifSettings::readme(),
            Path::new("gifski"),
            None,
            Path::new("o.gif"),
        );
        assert!(matches!(err, Err(EncodeError::NoFrames)));
    }

    #[test]
    fn missing_gifski_binary_is_spawn_error() {
        // Writes the temp PNGs, then fails to launch a nonexistent gifski.
        let frames = vec![RgbaImage::new(2, 2, vec![255u8; 16])];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("o.gif");
        let err = export_gif(
            &frames,
            &GifSettings::readme(),
            Path::new("vuoom-no-such-gifski-binary"),
            None,
            &out,
        );
        assert!(matches!(err, Err(EncodeError::Spawn { .. })));
    }
}
