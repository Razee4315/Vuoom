//! Disk-backed frame storage — recordings are no longer capped by RAM.
//!
//! During recording a drain thread streams every captured frame straight to
//! `%TEMP%/vuoom-recovery/frames.raw` (raw BGRA) with a JSON index sidecar; the editor
//! then reads frames back one at a time (with a one-slot cache for scrubbing). Because
//! the bytes are already on disk — together with `project.json` written at stop — a crash
//! or accidental close loses nothing: the next launch can offer to recover the session.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use vuoom_capture::CapturedFrame;

/// Index entry for one stored frame: QPC timestamp, dimensions, and byte range.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FrameRec {
    pub qpc: i64,
    pub w: u32,
    pub h: u32,
    pub offset: u64,
    pub len: u32,
}

/// The fixed per-user directory holding the latest session's frames + project manifest.
/// One session at a time: starting a new recording replaces it.
pub fn recovery_dir() -> PathBuf {
    std::env::temp_dir().join("vuoom-recovery")
}

fn raw_path(dir: &Path) -> PathBuf {
    dir.join("frames.raw")
}
fn index_path(dir: &Path) -> PathBuf {
    dir.join("index.json")
}

/// The project manifest saved alongside the frames (written at stop time).
pub fn project_path(dir: &Path) -> PathBuf {
    dir.join("project.json")
}

/// Append-only writer used by the recording drain thread (and bundle open).
pub struct FrameWriter {
    dir: PathBuf,
    out: BufWriter<File>,
    index: Vec<FrameRec>,
    offset: u64,
}

impl FrameWriter {
    /// Start a fresh store in `dir`, replacing any previous session.
    pub fn create(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|e| format!("recovery dir: {e}"))?;
        // A stale manifest must not pair with new frames.
        let _ = fs::remove_file(project_path(&dir));
        let _ = fs::remove_file(index_path(&dir));
        let file = File::create(raw_path(&dir)).map_err(|e| format!("frame file: {e}"))?;
        Ok(Self {
            dir,
            out: BufWriter::with_capacity(1 << 20, file),
            index: Vec::new(),
            offset: 0,
        })
    }

    /// Append one frame's raw BGRA bytes.
    pub fn push(&mut self, f: &CapturedFrame) -> Result<(), String> {
        self.out
            .write_all(&f.bgra)
            .map_err(|e| format!("frame write: {e}"))?;
        self.index.push(FrameRec {
            qpc: f.qpc,
            w: f.width,
            h: f.height,
            offset: self.offset,
            len: f.bgra.len() as u32,
        });
        self.offset += f.bgra.len() as u64;
        Ok(())
    }

    /// How many frames have been written so far.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Flush, persist the index sidecar, and reopen the store for reading.
    pub fn finish(mut self) -> Result<FrameStore, String> {
        self.out.flush().map_err(|e| format!("frame flush: {e}"))?;
        drop(self.out);
        let json = serde_json::to_string(&self.index).map_err(|e| e.to_string())?;
        fs::write(index_path(&self.dir), json).map_err(|e| format!("frame index: {e}"))?;
        FrameStore::open(&self.dir)
    }
}

struct ReadState {
    file: File,
    /// One-slot cache: scrubbing hits the same/neighboring frame repeatedly.
    cache: Option<(usize, Arc<CapturedFrame>)>,
}

/// Read side of the store: random access by frame number.
pub struct FrameStore {
    index: Vec<FrameRec>,
    read: Mutex<ReadState>,
}

impl FrameStore {
    /// Open the store in `dir` (`frames.raw` + `index.json`).
    pub fn open(dir: &Path) -> Result<Self, String> {
        let json = fs::read_to_string(index_path(dir)).map_err(|e| format!("frame index: {e}"))?;
        let index: Vec<FrameRec> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let file = File::open(raw_path(dir)).map_err(|e| format!("frame file: {e}"))?;
        Ok(Self {
            index,
            read: Mutex::new(ReadState { file, cache: None }),
        })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The per-frame metadata (for time lookups without touching the disk).
    pub fn recs(&self) -> &[FrameRec] {
        &self.index
    }

    /// Load frame `i` (cached for repeat hits).
    pub fn frame(&self, i: usize) -> Result<Arc<CapturedFrame>, String> {
        let rec = *self.index.get(i).ok_or("no such frame")?;
        let mut rs = self.read.lock().map_err(|_| "lock poisoned")?;
        if let Some((ci, f)) = &rs.cache {
            if *ci == i {
                return Ok(Arc::clone(f));
            }
        }
        let mut bgra = vec![0u8; rec.len as usize];
        rs.file
            .seek(SeekFrom::Start(rec.offset))
            .map_err(|e| format!("frame seek: {e}"))?;
        rs.file
            .read_exact(&mut bgra)
            .map_err(|e| format!("frame read: {e}"))?;
        let frame = Arc::new(CapturedFrame {
            width: rec.w,
            height: rec.h,
            bgra,
            qpc: rec.qpc,
        });
        rs.cache = Some((i, Arc::clone(&frame)));
        Ok(frame)
    }
}
