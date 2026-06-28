//! Invoking gifski (and optionally gifsicle) as **separate processes**.
//!
//! gifski is AGPL; running it as an unmodified external binary (mere aggregation) keeps
//! Vuoom's own code Apache-2.0. We never link the gifski crate. See `docs/10-Licensing.md`.
//!
//! The argument builders are pure and unit-tested; the spawn helpers are thin wrappers
//! over `std::process::Command`.

use crate::error::EncodeError;
use crate::settings::GifSettings;
use std::path::Path;
use std::process::Command;

/// Build the gifski CLI arguments for the given settings, PNG frame paths, and output.
#[must_use]
pub fn build_gifski_args(settings: &GifSettings, frames: &[String], out: &str) -> Vec<String> {
    // gifski rejects quality outside 1..=100 with an opaque error; clamp defensively.
    let quality = settings.quality.clamp(1, 100);
    let mut args = vec![
        "-o".to_string(),
        out.to_string(),
        "--fps".to_string(),
        settings.fps.to_string(),
        "--quality".to_string(),
        quality.to_string(),
        "--quiet".to_string(),
    ];
    if let Some(w) = settings.width {
        args.push("--width".to_string());
        args.push(w.to_string());
    }
    args.extend(frames.iter().cloned());
    args
}

/// Build a gifsicle "shrink more" second-pass command (lossy + dedup), editing in place.
#[must_use]
pub fn build_gifsicle_args(lossy: u8, gif_path: &str) -> Vec<String> {
    // gifsicle's `--lossy` tops out around 200; clamp so a stray value can't fail the pass.
    let lossy = lossy.min(200);
    vec![
        "-O3".to_string(),
        format!("--lossy={lossy}"),
        "--colors".to_string(),
        "256".to_string(),
        "-b".to_string(),
        gif_path.to_string(),
    ]
}

/// Encode `frames` (PNG paths) into a GIF at `out` using the gifski binary at `gifski_bin`.
///
/// # Errors
/// Returns [`EncodeError`] if there are no frames, the process cannot be spawned, or it
/// exits with a non-zero status.
pub fn run_gifski(
    gifski_bin: &Path,
    settings: &GifSettings,
    frames: &[String],
    out: &str,
) -> Result<(), EncodeError> {
    if frames.is_empty() {
        return Err(EncodeError::NoFrames);
    }
    let args = build_gifski_args(settings, frames, out);
    run(gifski_bin, &args, "gifski")
}

/// Run the optional gifsicle second pass over an existing GIF, in place.
///
/// # Errors
/// Returns [`EncodeError`] if gifsicle cannot be spawned or exits non-zero.
pub fn run_gifsicle(gifsicle_bin: &Path, lossy: u8, gif_path: &str) -> Result<(), EncodeError> {
    let args = build_gifsicle_args(lossy, gif_path);
    run(gifsicle_bin, &args, "gifsicle")
}

fn run(bin: &Path, args: &[String], tool: &'static str) -> Result<(), EncodeError> {
    let status = Command::new(bin)
        .args(args)
        .status()
        .map_err(|source| EncodeError::Spawn {
            tool: tool.to_string(),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(EncodeError::Failed {
            tool: tool.to_string(),
            status: status.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gifski_args_include_settings_and_frames() {
        let s = GifSettings::readme();
        let frames = vec!["f0.png".to_string(), "f1.png".to_string()];
        let args = build_gifski_args(&s, &frames, "out.gif");
        assert!(args.windows(2).any(|w| w[0] == "-o" && w[1] == "out.gif"));
        assert!(args.windows(2).any(|w| w[0] == "--fps" && w[1] == "15"));
        assert!(args.windows(2).any(|w| w[0] == "--width" && w[1] == "1000"));
        assert_eq!(args.last().unwrap(), "f1.png");
    }

    #[test]
    fn gifski_args_omit_width_when_none() {
        let s = GifSettings {
            width: None,
            ..GifSettings::readme()
        };
        let args = build_gifski_args(&s, &["f.png".to_string()], "o.gif");
        assert!(!args.iter().any(|a| a == "--width"));
    }

    #[test]
    fn gifsicle_args_are_lossy_in_place() {
        let args = build_gifsicle_args(80, "out.gif");
        assert!(args.contains(&"--lossy=80".to_string()));
        assert!(args.contains(&"-b".to_string()));
        assert_eq!(args.last().unwrap(), "out.gif");
    }

    #[test]
    fn quality_is_clamped_to_gifski_range() {
        let s = GifSettings {
            quality: 250,
            ..GifSettings::readme()
        };
        let args = build_gifski_args(&s, &["f.png".to_string()], "o.gif");
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--quality" && w[1] == "100"));
    }

    #[test]
    fn lossy_is_clamped_to_gifsicle_range() {
        let args = build_gifsicle_args(255, "o.gif");
        assert!(args.contains(&"--lossy=200".to_string()));
    }

    #[test]
    fn no_frames_is_an_error() {
        let err = run_gifski(Path::new("gifski"), &GifSettings::readme(), &[], "o.gif");
        assert!(matches!(err, Err(EncodeError::NoFrames)));
    }
}
