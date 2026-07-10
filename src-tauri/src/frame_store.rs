//! Disk-backed frame storage — recordings are no longer capped by RAM.
//!
//! During recording a drain thread streams every captured frame straight to
//! `%TEMP%/vuoom-recovery/frames.raw` (raw BGRA), appending one fixed-size record per
//! frame to `index.bin` as it goes; the editor then reads frames back one at a time (with
//! a one-slot cache for scrubbing). Because both the bytes and their index land on disk
//! incrementally — together with a manifest written at the start of recording — a hard
//! crash mid-take is recoverable: the next launch reconstructs the frames that survived.

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

/// On-disk size of one `index.bin` record: `qpc`(8) + `w`(4) + `h`(4) + `offset`(8) +
/// `len`(4), little-endian. Fixed-width so a crash-torn tail is just an ignored partial
/// record and the whole index can be appended one frame at a time (no rewrite).
const REC_SIZE: usize = 28;

/// How often (in frames) the index buffer is pushed to the OS. At typical capture rates
/// this is a few times a second, so a hard crash leaves at most a fraction of a second of
/// frames un-indexed — without an fsync on the per-frame hot path.
const INDEX_FLUSH_EVERY: u32 = 15;

fn encode_rec(r: &FrameRec) -> [u8; REC_SIZE] {
    let mut b = [0u8; REC_SIZE];
    b[0..8].copy_from_slice(&r.qpc.to_le_bytes());
    b[8..12].copy_from_slice(&r.w.to_le_bytes());
    b[12..16].copy_from_slice(&r.h.to_le_bytes());
    b[16..24].copy_from_slice(&r.offset.to_le_bytes());
    b[24..28].copy_from_slice(&r.len.to_le_bytes());
    b
}

/// Decode one `REC_SIZE`-byte record. `b` must be exactly `REC_SIZE` bytes (guaranteed by
/// the `chunks_exact` caller), so the fixed-range slices never panic.
fn decode_rec(b: &[u8]) -> FrameRec {
    FrameRec {
        qpc: i64::from_le_bytes(b[0..8].try_into().unwrap()),
        w: u32::from_le_bytes(b[8..12].try_into().unwrap()),
        h: u32::from_le_bytes(b[12..16].try_into().unwrap()),
        offset: u64::from_le_bytes(b[16..24].try_into().unwrap()),
        len: u32::from_le_bytes(b[24..28].try_into().unwrap()),
    }
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
    dir.join("index.bin")
}
/// Older builds wrote a single-blob JSON index; remove it so a stale one can't be paired
/// with new frames (the reader only understands the append-only binary index now).
fn legacy_index_path(dir: &Path) -> PathBuf {
    dir.join("index.json")
}

/// The project manifest saved alongside the frames (written at stop time).
pub fn project_path(dir: &Path) -> PathBuf {
    dir.join("project.json")
}

/// Append-only writer used by the recording drain thread (and bundle open). Both the pixel
/// file and the index grow incrementally, so a hard crash mid-take leaves a recoverable
/// store rather than gigabytes of un-indexed pixels.
pub struct FrameWriter {
    dir: PathBuf,
    out: BufWriter<File>,
    /// Append-only index: one `REC_SIZE` record per frame, flushed a few times a second.
    idx: BufWriter<File>,
    offset: u64,
    /// Frames appended since the index buffer was last pushed to the OS.
    since_flush: u32,
}

impl FrameWriter {
    /// Start a fresh store in `dir`, replacing any previous session.
    pub fn create(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|e| format!("recovery dir: {e}"))?;
        // A stale manifest / index must not pair with new frames.
        let _ = fs::remove_file(project_path(&dir));
        let _ = fs::remove_file(index_path(&dir));
        let _ = fs::remove_file(legacy_index_path(&dir));
        let file = File::create(raw_path(&dir)).map_err(|e| format!("frame file: {e}"))?;
        let idx = File::create(index_path(&dir)).map_err(|e| format!("frame index: {e}"))?;
        Ok(Self {
            dir,
            out: BufWriter::with_capacity(1 << 20, file),
            idx: BufWriter::new(idx),
            offset: 0,
            since_flush: 0,
        })
    }

    /// Append one frame's raw BGRA bytes and its index record.
    pub fn push(&mut self, f: &CapturedFrame) -> Result<(), String> {
        self.out
            .write_all(&f.bgra)
            .map_err(|e| format!("frame write: {e}"))?;
        let rec = FrameRec {
            qpc: f.qpc,
            w: f.width,
            h: f.height,
            offset: self.offset,
            len: f.bgra.len() as u32,
        };
        self.idx
            .write_all(&encode_rec(&rec))
            .map_err(|e| format!("frame index: {e}"))?;
        self.offset += f.bgra.len() as u64;
        // Cheap periodic flush (buffered, no fsync) so a crash strands at most a fraction of
        // a second of frames. If the index runs ahead of what actually reached frames.raw,
        // `open` trims the excess — so this interleaving is always safe.
        self.since_flush += 1;
        if self.since_flush >= INDEX_FLUSH_EVERY {
            let _ = self.idx.flush();
            self.since_flush = 0;
        }
        Ok(())
    }

    /// Flush both files and reopen the store for reading.
    pub fn finish(mut self) -> Result<FrameStore, String> {
        self.out.flush().map_err(|e| format!("frame flush: {e}"))?;
        self.idx.flush().map_err(|e| format!("frame index: {e}"))?;
        drop(self.out);
        drop(self.idx);
        FrameStore::open(&self.dir)
    }

    /// Finalize after a mid-recording write failure (e.g. a full disk): keep the frames that
    /// were already written instead of losing the whole take. Best-effort — further I/O
    /// errors are tolerated. When the disk filled, the newest frame's tail may still be in the
    /// buffer and never reach disk; `open` drops any frame whose bytes aren't wholly on disk,
    /// so every frame the returned store exposes reads back cleanly.
    pub fn finish_salvage(mut self) -> Result<FrameStore, String> {
        let _ = self.out.flush(); // may fail on a full disk; open()'s trim covers the gap
        let _ = self.idx.flush();
        drop(self.out);
        drop(self.idx);
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
    /// Open the store in `dir` (`frames.raw` + `index.bin`).
    ///
    /// Robust against a crash mid-recording: fixed-size records mean a torn trailing record
    /// is simply ignored (`chunks_exact`), and any frame whose bytes didn't fully reach
    /// `frames.raw` is dropped — so the store only exposes frames that read back cleanly.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let bytes = fs::read(index_path(dir)).map_err(|e| format!("frame index: {e}"))?;
        let file = File::open(raw_path(dir)).map_err(|e| format!("frame file: {e}"))?;
        let raw_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        // Records are appended in capture order with monotonically increasing offsets, so the
        // first one that runs past what's on disk marks the end of the recoverable prefix.
        let mut index = Vec::with_capacity(bytes.len() / REC_SIZE);
        for chunk in bytes.chunks_exact(REC_SIZE) {
            let rec = decode_rec(chunk);
            if rec.offset + u64::from(rec.len) > raw_len {
                break;
            }
            index.push(rec);
        }
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
        let mut rs = self.read.lock().unwrap_or_else(|e| e.into_inner());
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
