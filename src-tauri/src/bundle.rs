//! `.vuoom` project folders on disk: saving one without ever leaving a half-written project
//! behind, and turning a folder that can't be opened into a message a person can act on.
//! Copying the frames, audio and camera track themselves lives in `session`.

use std::path::{Path, PathBuf};

use vuoom_project::Project;

/// What a project folder holds. A save swaps exactly these, the manifest last, so a folder
/// with a `project.json` always has the frames it describes.
const ENTRIES: [&str; 4] = ["frames", "audio", "camera", "project.json"];

/// Headroom kept free on top of what a save or open writes (the same drive holds the OS,
/// the swap file and usually the recording store).
const SPACE_MARGIN: u64 = 256 * 1024 * 1024;

/// A folder next to `dir` named `<dir name>.<tag>`, e.g. `demo.vuoom.saving`.
pub fn sibling(dir: &Path, tag: &str) -> Result<PathBuf, String> {
    let Some(name) = dir.file_name() else {
        return Err("pick a folder to save the project in".into());
    };
    let mut name = name.to_os_string();
    name.push(format!(".{tag}"));
    Ok(dir.with_file_name(name))
}

/// A byte count the way a person reads it: MB under a gigabyte, GB above.
fn size_text(bytes: u64) -> String {
    if bytes < 1_000_000_000 {
        format!("{} MB", bytes.div_ceil(1_000_000))
    } else {
        format!("{:.1} GB", bytes as f64 / 1e9)
    }
}

/// Refuse a save or open that writes about `need` bytes to a drive with `free` bytes left,
/// before anything is written. `what` finishes the sentence "Not enough disk space to …".
pub fn check_space(free: u64, need: u64, what: &str) -> Result<(), String> {
    if free >= need.saturating_add(SPACE_MARGIN) {
        return Ok(());
    }
    let need = size_text(need);
    let free = size_text(free);
    Err(format!(
        "Not enough disk space to {what}: it needs about {need} and only {free} is free \
         on that drive. Free up some space and try again."
    ))
}

/// Move a fully written project from `staging` into `dir`, replacing whatever an earlier
/// save left there. Entries move one at a time (renames on one drive: milliseconds, and
/// they work while the folder is open in Explorer), the manifest last. If a move fails,
/// the earlier save is put back and `dir` is left as it was. `staging` is removed either way.
pub fn swap_in(staging: &Path, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("save folder: {e}"))?;
    let old = sibling(dir, "old")?;
    let _ = std::fs::remove_dir_all(&old);
    std::fs::create_dir_all(&old).map_err(|e| format!("save folder: {e}"))?;
    let moved = move_entries(staging, dir, &old);
    let _ = std::fs::remove_dir_all(staging);
    if moved.is_ok() {
        let _ = std::fs::remove_dir_all(&old);
    } else {
        // Only if it's empty: a failed put-back must not delete the earlier save.
        let _ = std::fs::remove_dir(&old);
    }
    moved
}

fn move_entries(staging: &Path, dir: &Path, old: &Path) -> Result<(), String> {
    let mut aside = Vec::new();
    for name in ENTRIES {
        let from = dir.join(name);
        if !from.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(&from, old.join(name)) {
            restore(old, dir, &aside);
            return Err(format!("couldn't replace the earlier save ({name}): {e}"));
        }
        aside.push(name);
    }
    let mut placed = Vec::new();
    for name in ENTRIES {
        let from = staging.join(name);
        if !from.exists() {
            continue;
        }
        if let Err(e) = std::fs::rename(&from, dir.join(name)) {
            for placed_name in &placed {
                remove_entry(&dir.join(placed_name));
            }
            restore(old, dir, &aside);
            return Err(format!(
                "couldn't move the new save into place ({name}): {e}"
            ));
        }
        placed.push(name);
    }
    Ok(())
}

/// Put entries that were moved aside back into `dir` (best effort, each one a rename).
fn restore(old: &Path, dir: &Path, names: &[&str]) {
    for name in names {
        let _ = std::fs::rename(old.join(name), dir.join(name));
    }
}

fn remove_entry(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Read a project folder's manifest, saying in plain words why a folder can't be opened.
pub fn read_manifest(dir: &Path) -> Result<Project, String> {
    if !dir.is_dir() {
        let shown = dir.display();
        return Err(format!("This project folder no longer exists: {shown}"));
    }
    let json = match std::fs::read_to_string(dir.join("project.json")) {
        Ok(json) => json,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(missing_manifest(dir)),
        Err(e) => return Err(format!("Couldn't read this project's file: {e}")),
    };
    let parsed = Project::from_json(&json);
    parsed.map_err(|e| format!("This project's file is damaged ({e})."))
}

/// Why a folder without a `project.json` can't be opened, and where the earlier save is if
/// a save was cut off while swapping in.
fn missing_manifest(dir: &Path) -> String {
    let earlier = sibling(dir, "old")
        .ok()
        .filter(|old| old.join("project.json").is_file());
    if let Some(old) = earlier {
        return format!(
            "This project wasn't saved completely. The earlier save is intact in \"{}\", \
             open that folder instead.",
            old.display()
        );
    }
    if dir.join("frames").exists() {
        "This project wasn't saved completely: its project file is missing.".into()
    } else {
        "This folder isn't a Vuoom project (it has no project file).".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use vuoom_project::SourceInfo;

    /// A fresh, empty parent folder for one test.
    fn tmp_root(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("vuoom-bundle-{tag}-{n}-{seq}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn sibling_appends_the_tag() {
        let dir = Path::new("C:/work/demo.vuoom");
        let s = sibling(dir, "saving").unwrap();
        assert_eq!(s, Path::new("C:/work/demo.vuoom.saving"));
    }

    #[test]
    fn space_check_passes_with_room_and_explains_a_refusal() {
        let roomy = check_space(10_000_000_000, 1_000_000_000, "save");
        assert!(roomy.is_ok());
        let low = check_space(300_000_000, 2_100_000_000, "save this project");
        let err = low.unwrap_err();
        assert!(err.contains("save this project"), "{err}");
        assert!(err.contains("2.1 GB"), "{err}");
        assert!(err.contains("300 MB"), "{err}");
        // The margin counts: exactly the size free is not enough.
        assert!(check_space(500, 500, "save").is_err());
    }

    #[test]
    fn swap_replaces_an_earlier_save_and_cleans_up() {
        let root = tmp_root("replace");
        let dir = root.join("demo.vuoom");
        put(&dir.join("frames/a.bin"), "old frames");
        put(&dir.join("audio/mic.wav"), "old audio");
        put(&dir.join("project.json"), "old");
        put(&dir.join("notes.txt"), "the user's own file");
        let staging = sibling(&dir, "saving").unwrap();
        put(&staging.join("frames/b.bin"), "new frames");
        put(&staging.join("project.json"), "new");

        swap_in(&staging, &dir).unwrap();

        assert_eq!(read(&dir.join("project.json")), "new");
        assert_eq!(read(&dir.join("frames/b.bin")), "new frames");
        assert!(!dir.join("frames/a.bin").exists());
        // The new save has no audio, so the earlier one's doesn't linger.
        assert!(!dir.join("audio").exists());
        assert_eq!(read(&dir.join("notes.txt")), "the user's own file");
        assert!(!staging.exists());
        assert!(!sibling(&dir, "old").unwrap().exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn swap_into_a_new_folder() {
        let root = tmp_root("fresh");
        let dir = root.join("first.vuoom");
        let staging = sibling(&dir, "saving").unwrap();
        put(&staging.join("frames/a.bin"), "frames");
        put(&staging.join("project.json"), "p");

        swap_in(&staging, &dir).unwrap();

        assert_eq!(read(&dir.join("project.json")), "p");
        assert!(!staging.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file held open without delete sharing can't be renamed: the swap must fail and
    /// leave the earlier save exactly as it was.
    #[cfg(windows)]
    #[test]
    fn a_failed_swap_puts_the_earlier_save_back() {
        use std::os::windows::fs::OpenOptionsExt;

        let root = tmp_root("rollback");
        let dir = root.join("demo.vuoom");
        put(&dir.join("frames/a.bin"), "old frames");
        put(&dir.join("project.json"), "old");
        let staging = sibling(&dir, "saving").unwrap();
        put(&staging.join("frames/b.bin"), "new frames");
        put(&staging.join("project.json"), "new");

        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(dir.join("project.json"))
            .unwrap();
        let result = swap_in(&staging, &dir);
        drop(lock);

        assert!(result.is_err());
        assert_eq!(read(&dir.join("project.json")), "old");
        assert_eq!(read(&dir.join("frames/a.bin")), "old frames");
        assert!(!dir.join("frames/b.bin").exists());
        assert!(!sibling(&dir, "old").unwrap().exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifest_errors_say_what_is_wrong() {
        let root = tmp_root("manifest");

        let gone = read_manifest(&root.join("gone.vuoom")).unwrap_err();
        assert!(gone.contains("no longer exists"), "{gone}");

        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let err = read_manifest(&empty).unwrap_err();
        assert!(err.contains("isn't a Vuoom project"), "{err}");

        let cut = root.join("cut.vuoom");
        put(&cut.join("frames/a.bin"), "frames");
        let err = read_manifest(&cut).unwrap_err();
        assert!(err.contains("wasn't saved completely"), "{err}");

        put(&sibling(&cut, "old").unwrap().join("project.json"), "{}");
        let err = read_manifest(&cut).unwrap_err();
        assert!(err.contains("cut.vuoom.old"), "{err}");

        let bad = root.join("bad.vuoom");
        put(&bad.join("project.json"), "{ not json");
        let err = read_manifest(&bad).unwrap_err();
        assert!(err.contains("damaged"), "{err}");

        let good = root.join("good.vuoom");
        let project = Project::new(SourceInfo {
            path: String::new(),
            width: 64,
            height: 32,
            fps: 30.0,
            duration: 2.0,
        });
        put(&good.join("project.json"), &project.to_json().unwrap());
        assert_eq!(read_manifest(&good).unwrap().source.width, 64);
        let _ = std::fs::remove_dir_all(&root);
    }
}
