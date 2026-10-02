//! What a bug report needs, gathered on this PC for the user to paste: the app and Windows
//! versions, the GPU, the displays, storage use and the end of the log. Nothing is sent
//! anywhere; the text reaches the clipboard only when the user asks for it.
//!
//! Also notices a run that didn't end normally (a crash, a kill, a power cut): a marker file
//! exists while Vuoom runs and is removed on a normal exit, so finding it at launch means
//! the last run was cut short.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Log lines included in a report.
const LOG_LINES: usize = 200;

/// Present while Vuoom runs (see the module docs).
const RUNNING_MARKER: &str = "running.marker";

/// Whether the previous run ended without a normal exit, noted at launch.
pub struct LastRun(AtomicBool);

impl LastRun {
    /// Note that this run has started (writing the marker into `dir`) and whether the one
    /// before it ended normally.
    pub fn begin(dir: &Path) -> Self {
        let marker = dir.join(RUNNING_MARKER);
        let cut_short = marker.exists();
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&marker, std::process::id().to_string());
        Self(AtomicBool::new(cut_short))
    }

    /// Whether the last run was cut short. True on the first call only, so the UI says so once.
    pub fn take_cut_short(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

/// This run is ending normally: remove the marker.
pub fn end_run(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(RUNNING_MARKER));
}

/// The plain-text report. `gpu` describes the adapter in use (or why there is none);
/// `log_dir` holds the daily log files.
pub fn report(version: &str, gpu: &str, log_dir: &Path) -> String {
    let windows = windows_version();
    let arch = std::env::consts::ARCH;
    let mut out = format!("Vuoom {version}\n{windows}, {arch}\nGPU: {gpu}\n");
    for d in crate::displays::enumerate() {
        let primary = if d.primary { ", primary" } else { "" };
        out.push_str(&format!("Display {}: {}x{}{primary}\n", d.index, d.w, d.h));
    }
    let (bytes, takes) = crate::frame_store::recovery_usage();
    let mb = bytes / 1_000_000;
    out.push_str(&format!("Recovery storage: {mb} MB in {takes} takes\n"));

    match newest_log(log_dir) {
        Some(path) => {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let name = name.unwrap_or_default();
            out.push_str(&format!("\nLast {LOG_LINES} log lines ({name}):\n"));
            for line in tail(&text, LOG_LINES) {
                out.push_str(line);
                out.push('\n');
            }
        }
        None => out.push_str("\nNo log file found.\n"),
    }
    match std::env::var("USERPROFILE") {
        Ok(home) if !home.is_empty() => hide_home(&out, &home),
        _ => out,
    }
}

/// The report without the user's profile folder (and so their Windows user name) in paths.
fn hide_home(text: &str, home: &str) -> String {
    text.replace(home, "%USERPROFILE%")
}

/// The last `n` lines of `text`.
fn tail(text: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(n);
    lines[from..].to_vec()
}

/// The most recently written daily log file in `dir`.
fn newest_log(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let is_log = name.to_string_lossy().starts_with("vuoom.log");
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if is_log && newest.as_ref().is_none_or(|(t, _)| modified > *t) {
            newest = Some((modified, entry.path()));
        }
    }
    newest.map(|(_, path)| path)
}

/// "Windows 11 (10.0.26100.4061)", from `ver`, run without a console window.
fn windows_version() -> String {
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "ver"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let text = cmd
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    describe_windows(&text)
}

/// Turn `ver` output ("Microsoft Windows [Version 10.0.26100.4061]") into a readable line.
/// Windows 11 still reports 10.0, so the build number (22000 and up) tells them apart.
fn describe_windows(ver: &str) -> String {
    let Some(version) = ver.split("Version ").nth(1) else {
        return "Windows (version unknown)".into();
    };
    let version = version.trim().trim_end_matches(']');
    let third = version.split('.').nth(2).unwrap_or("0");
    let build: u32 = third.parse().unwrap_or(0);
    let name = if build >= 22000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    format!("{name} ({version})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_name_comes_from_the_build() {
        let w11 = describe_windows("\r\nMicrosoft Windows [Version 10.0.26100.4061]\r\n");
        assert_eq!(w11, "Windows 11 (10.0.26100.4061)");
        let w10 = describe_windows("Microsoft Windows [Version 10.0.19045.5737]");
        assert_eq!(w10, "Windows 10 (10.0.19045.5737)");
        assert_eq!(describe_windows(""), "Windows (version unknown)");
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail("a\nb\nc\nd", 2), ["c", "d"]);
        assert_eq!(tail("a\nb", 5), ["a", "b"]);
        assert!(tail("", 3).is_empty());
    }

    #[test]
    fn the_profile_folder_is_hidden() {
        let text = r"saved C:\Users\sam\Videos\demo.vuoom";
        let hidden = hide_home(text, r"C:\Users\sam");
        assert_eq!(hidden, r"saved %USERPROFILE%\Videos\demo.vuoom");
    }

    #[test]
    fn the_marker_notices_a_run_that_was_cut_short() {
        let dir = std::env::temp_dir().join(format!("vuoom-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let first = LastRun::begin(&dir);
        assert!(!first.take_cut_short(), "a first run has nothing before it");
        // No end_run: this run "crashes".
        let second = LastRun::begin(&dir);
        assert!(second.take_cut_short());
        assert!(!second.take_cut_short(), "said once");
        end_run(&dir);
        let third = LastRun::begin(&dir);
        assert!(!third.take_cut_short(), "a normal exit clears it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
