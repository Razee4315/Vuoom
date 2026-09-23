//! Disk-backed frame storage, recordings are no longer capped by RAM.
//!
//! During recording a drain thread streams every captured frame straight to
//! `%TEMP%/vuoom-recovery/<session-id>/frames.raw`, appending one fixed-size record per frame
//! to `index.bin` as it goes; the editor then reads frames back one at a time (with a
//! one-slot cache for scrubbing). Frames are stored LZ4-compressed, mostly as XOR deltas
//! against the previous frame with a keyframe every [`KEY_EVERY`] frames, which shrinks a
//! typical screencast by one to two orders of magnitude versus raw BGRA (see the codec
//! notes below). Stores written by older builds (raw BGRA) still read back unchanged.
//! Because both the bytes and their index land on disk
//! incrementally, together with a manifest written at the start of recording, a hard
//! crash mid-take is recoverable: the next launch reconstructs the frames that survived.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vuoom_capture::CapturedFrame;

/// Index entry for one stored frame: QPC timestamp, dimensions, byte range and encoding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FrameRec {
    pub qpc: i64,
    pub w: u32,
    pub h: u32,
    pub offset: u64,
    /// Payload byte count in `frames.raw` (flags stripped).
    pub len: u32,
    /// Encoding of the payload: a combination of the `FLAG_*` bits. `0` is a raw BGRA frame,
    /// which is also what every store written before compression existed contains.
    pub flags: u8,
}

/// Payload is strip-compressed (see [`encode_frame`]) instead of raw BGRA.
pub const FLAG_COMPRESSED: u8 = 0b100;
/// Payload is the XOR of this frame against the previous frame in index order, so decoding
/// needs that frame first. Only ever set together with [`FLAG_COMPRESSED`].
pub const FLAG_DELTA: u8 = 0b010;
/// Pixels are identical to the previous frame in index order. `offset`/`len` still point at
/// the previous record's bytes so crash-trimming in [`FrameStore::open`] keeps working.
pub const FLAG_DUP: u8 = 0b001;

/// The flags ride in the top three bits of the on-disk `len` field, which is otherwise far
/// larger than any frame needs (29 bits = 512 MB, an 8K BGRA frame is ~133 MB).
const LEN_MASK: u32 = (1 << 29) - 1;

impl FrameRec {
    /// True when this record can be decoded without first decoding its predecessor.
    fn is_anchor(&self) -> bool {
        self.flags & (FLAG_DELTA | FLAG_DUP) == 0
    }
}

/// On-disk size of one `index.bin` record: `qpc`(8) + `w`(4) + `h`(4) + `offset`(8) +
/// `len|flags`(4), little-endian. Fixed-width so a crash-torn tail is just an ignored partial
/// record and the whole index can be appended one frame at a time (no rewrite).
const REC_SIZE: usize = 28;

/// How often (in frames) the index buffer is pushed to the OS. At typical capture rates
/// this is a few times a second, so a hard crash leaves at most a fraction of a second of
/// frames un-indexed, without an fsync on the per-frame hot path.
const INDEX_FLUSH_EVERY: u32 = 15;

fn encode_rec(r: &FrameRec) -> [u8; REC_SIZE] {
    let mut b = [0u8; REC_SIZE];
    b[0..8].copy_from_slice(&r.qpc.to_le_bytes());
    b[8..12].copy_from_slice(&r.w.to_le_bytes());
    b[12..16].copy_from_slice(&r.h.to_le_bytes());
    b[16..24].copy_from_slice(&r.offset.to_le_bytes());
    let packed = (r.len & LEN_MASK) | (u32::from(r.flags & 0b111) << 29);
    b[24..28].copy_from_slice(&packed.to_le_bytes());
    b
}

/// Decode one `REC_SIZE`-byte record. `b` must be exactly `REC_SIZE` bytes (guaranteed by
/// the `chunks_exact` caller), so the fixed-range slices never panic.
fn decode_rec(b: &[u8]) -> FrameRec {
    let packed = u32::from_le_bytes(b[24..28].try_into().unwrap());
    FrameRec {
        qpc: i64::from_le_bytes(b[0..8].try_into().unwrap()),
        w: u32::from_le_bytes(b[8..12].try_into().unwrap()),
        h: u32::from_le_bytes(b[12..16].try_into().unwrap()),
        offset: u64::from_le_bytes(b[16..24].try_into().unwrap()),
        len: packed & LEN_MASK,
        flags: (packed >> 29) as u8,
    }
}

// ── frame codec ──────────────────────────────────────────────────────────────────
// Raw BGRA is ~8 MB per 1080p frame, i.e. ~500 MB/s at 60 fps. Screen content is extremely
// redundant: flat UI, and between two frames usually only a caret, a hover state or a
// scrolled panel changes. So each frame is stored as LZ4 of either its pixels (a keyframe)
// or its XOR against the previous frame (a delta, mostly zero bytes, which LZ4 squeezes to
// almost nothing). The frame is cut into horizontal strips compressed in parallel, which
// keeps a 4K frame well inside a 60 fps budget; strips are also decoded in parallel.

/// Force a keyframe at least this often, bounding the delta chain a random seek replays.
pub const KEY_EVERY: u32 = 30;

/// Number of horizontal strips a `h`-row frame is split into (1..=16).
fn strip_count(h: u32) -> usize {
    (h as usize / 96).clamp(1, 16)
}

/// Byte ranges of each strip inside a tightly packed `w`×`h` BGRA frame.
fn strip_ranges(w: u32, h: u32) -> Vec<std::ops::Range<usize>> {
    let n = strip_count(h);
    let row = w as usize * 4;
    let h = h as usize;
    (0..n)
        .map(|k| (k * h / n) * row..((k + 1) * h / n) * row)
        .collect()
}

/// Compress `cur` (optionally as an XOR delta against `prev`, same dimensions) into the
/// strip container: `u8 n`, `n × u32 compressed len`, then the concatenated LZ4 blocks.
fn encode_frame(cur: &[u8], prev: Option<&[u8]>, w: u32, h: u32) -> Vec<u8> {
    use rayon::prelude::*;
    let ranges = strip_ranges(w, h);
    let blocks: Vec<Vec<u8>> = ranges
        .par_iter()
        .map(|r| match prev {
            Some(p) => {
                let xored: Vec<u8> = cur[r.clone()]
                    .iter()
                    .zip(&p[r.clone()])
                    .map(|(a, b)| a ^ b)
                    .collect();
                lz4_flex::block::compress(&xored)
            }
            None => lz4_flex::block::compress(&cur[r.clone()]),
        })
        .collect();
    let body: usize = blocks.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(1 + blocks.len() * 4 + body);
    out.push(blocks.len() as u8);
    for b in &blocks {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    }
    for b in &blocks {
        out.extend_from_slice(b);
    }
    out
}

/// Decode a strip container into `out` (`w`×`h` BGRA). With `xor`, the decoded bytes are
/// XORed into `out`, which must already hold the previous frame.
fn decode_frame(payload: &[u8], w: u32, h: u32, out: &mut [u8], xor: bool) -> Result<(), String> {
    use rayon::prelude::*;
    let ranges = strip_ranges(w, h);
    let n = *payload.first().ok_or("empty frame payload")? as usize;
    if n != ranges.len() || payload.len() < 1 + n * 4 {
        return Err("corrupt frame payload".into());
    }
    let mut blocks = Vec::with_capacity(n);
    let mut at = 1 + n * 4;
    for k in 0..n {
        let len = u32::from_le_bytes(payload[1 + k * 4..5 + k * 4].try_into().unwrap()) as usize;
        let block = payload.get(at..at + len).ok_or("truncated frame payload")?;
        blocks.push(block);
        at += len;
    }
    // Split `out` into the strips' disjoint slices so they can be filled in parallel.
    let mut slices: Vec<&mut [u8]> = Vec::with_capacity(n);
    let mut rest = out;
    let mut consumed = 0;
    for r in &ranges {
        let (head, tail) = std::mem::take(&mut rest).split_at_mut(r.end - consumed);
        slices.push(head);
        rest = tail;
        consumed = r.end;
    }
    slices
        .into_par_iter()
        .zip(blocks)
        .try_for_each(|(dst, block)| -> Result<(), String> {
            if xor {
                let delta = lz4_flex::block::decompress(block, dst.len())
                    .map_err(|e| format!("frame decode: {e}"))?;
                if delta.len() != dst.len() {
                    return Err("frame decode: short delta".into());
                }
                for (d, x) in dst.iter_mut().zip(&delta) {
                    *d ^= x;
                }
            } else {
                let got = lz4_flex::block::decompress_into(block, dst)
                    .map_err(|e| format!("frame decode: {e}"))?;
                if got != dst.len() {
                    return Err("frame decode: short strip".into());
                }
            }
            Ok(())
        })
}

/// Root holding the per-session recovery subdirs. Each subdir is a full frame store
/// (gigabytes), so we retain only a couple (see [`new_session_dir`]) and prune the rest.
pub fn recovery_root() -> PathBuf {
    std::env::temp_dir().join("vuoom-recovery")
}

/// Free bytes available to the caller on the volume backing `path`. `None` if the query
/// fails or the volume can't be resolved, callers treat that as "unknown, don't block".
///
/// Recordings stream raw uncompressed BGRA here (~250 MB/s at 1080p, ~1 GB/s at 4K), so
/// without a guard a take can fill a system disk in minutes; this backs the record-start
/// free-space check and the drain's proactive low-space stop.
#[cfg(windows)]
pub fn free_space_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // `GetDiskFreeSpaceExW` needs an existing directory on the target volume, but the recovery
    // root may not be created yet, walk up to the first ancestor that exists.
    let mut dir = path;
    while !dir.exists() {
        dir = dir.parent()?;
    }
    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 path; `free` is a live out-pointer.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}

/// Non-Windows stub (the app is Windows-only; keeps the crate portable for `cargo check`).
#[cfg(not(windows))]
pub fn free_space_bytes(_path: &Path) -> Option<u64> {
    None
}

/// Fixed scratch subdir backing an opened `.vuoom` bundle. Named non-numerically so recovery
/// scanning skips it, opening a bundle must never bury the last recording's recoverable
/// session. Truncated (not rotated) on each reuse, so it holds at most one bundle's frames.
pub fn scratch_dir() -> PathBuf {
    recovery_root().join("scratch")
}

/// How many recorded sessions to retain: the current one plus the immediately previous, so a
/// crash at the very start of a new take can't lose the last good session. Bounds disk use,
/// these dirs each hold gigabytes of raw BGRA.
const KEEP_SESSIONS: usize = 2;

/// Parse a session subdir's file name back into its numeric id. Non-numeric entries (the
/// `scratch` dir, stray files) return `None` and are ignored by scanning/pruning, so junk in
/// the recovery root can't derail rotation.
fn session_id_of(path: &Path) -> Option<u128> {
    path.file_name()?.to_str()?.parse::<u128>().ok()
}

/// Existing session subdirs under the recovery root, newest id first. Junk (unparseable
/// names, stray files, the scratch dir) is skipped.
fn session_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<(u128, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(recovery_root()) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(id) = session_id_of(&p) {
                    dirs.push((id, p));
                }
            }
        }
    }
    dirs.sort_by_key(|&(id, _)| std::cmp::Reverse(id));
    dirs.into_iter().map(|(_, p)| p).collect()
}

/// A strictly-increasing session id (ms since the epoch, bumped past the newest existing id
/// if the clock didn't advance) so "newest first" ordering is always well-defined.
fn next_session_id() -> u128 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let max_existing = session_dirs()
        .first()
        .and_then(|p| session_id_of(p))
        .unwrap_or(0);
    now.max(max_existing.saturating_add(1))
}

/// Create a fresh session subdir under the recovery root, pruning old ones first so at most
/// [`KEEP_SESSIONS`] remain (counting the one being created). The newest existing session,
/// the last recording's recoverable store, is always kept, so starting a new take never
/// destroys it. Returns the new dir.
pub fn new_session_dir() -> PathBuf {
    let root = recovery_root();
    let _ = fs::create_dir_all(&root);
    // Keep only the newest `KEEP_SESSIONS - 1` existing sessions; the dir we're about to
    // create takes the last slot. Older ones (and their gigabytes) are removed now.
    for old in session_dirs().into_iter().skip(KEEP_SESSIONS - 1) {
        let _ = fs::remove_dir_all(&old);
    }
    let dir = root.join(next_session_id().to_string());
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Whether two recovery dirs refer to the same session (compared by id, falling back to a
/// path match for the non-numeric scratch dir).
fn same_dir(a: &Path, b: &Path) -> bool {
    match (session_id_of(a), session_id_of(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// The newest session subdir that holds a non-empty, openable store and isn't `exclude` (the
/// currently-loaded session). Skips empty/torn stores, a take that crashed at the very start
/// leaves only a placeholder manifest, so recovery lands on the most recent real content.
pub fn latest_recoverable(exclude: Option<&Path>) -> Option<PathBuf> {
    for dir in session_dirs() {
        if exclude.is_some_and(|e| same_dir(e, &dir)) {
            continue;
        }
        if !project_path(&dir).exists() {
            continue;
        }
        if matches!(FrameStore::open(&dir), Ok(store) if !store.is_empty()) {
            return Some(dir);
        }
    }
    None
}

/// Recursively sum the byte size of every file under `dir`. Best-effort: an entry that can't
/// be read is skipped rather than failing the whole walk. Cheap here, the recovery root only
/// ever holds a couple of session dirs plus scratch.
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            match e.file_type() {
                Ok(ft) if ft.is_dir() => total += dir_size(&e.path()),
                Ok(ft) if ft.is_file() => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

/// Total bytes held under the recovery root and the number of stored recording sessions
/// (numeric session subdirs). Backs the storage readout in the UI. The scratch dir counts
/// toward the bytes but not the session tally.
pub fn recovery_usage() -> (u64, usize) {
    (dir_size(&recovery_root()), session_dirs().len())
}

/// Delete every stored recovery dir, rotated sessions and the scratch store, except `keep`
/// (the store backing the currently-loaded clip). Returns the bytes freed. Best-effort per
/// dir: one that fails to delete is left in place and not counted.
pub fn clear_recovery(keep: Option<&Path>) -> u64 {
    let mut targets = session_dirs();
    let scratch = scratch_dir();
    if scratch.is_dir() {
        targets.push(scratch);
    }
    let mut freed = 0;
    for dir in targets {
        if keep.is_some_and(|k| same_dir(k, &dir)) {
            continue;
        }
        let size = dir_size(&dir);
        if fs::remove_dir_all(&dir).is_ok() {
            freed += size;
        }
    }
    freed
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

/// The previously written *distinct* frame, kept in RAM (one frame, already bounded). An
/// identical follow-up is stored as a [`FLAG_DUP`] record (no pixel I/O at all) and a
/// different one is encoded as an XOR delta against these bytes.
struct PrevFrame {
    bgra: Vec<u8>,
    w: u32,
    h: u32,
    offset: u64,
    len: u32,
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
    /// Last distinct frame written, for deduplication and delta encoding (see [`push`]).
    prev: Option<PrevFrame>,
    /// Delta records written since the last anchor (keyframe / raw) record.
    since_key: u32,
    /// Running totals for the compression readout (uncompressed vs. written bytes).
    stats: StoreStats,
}

/// Byte accounting for a store being written: what the frames would have cost raw versus
/// what actually reached disk. Surfaces in logs and the recording summary.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct StoreStats {
    pub frames: u64,
    pub raw_bytes: u64,
    pub stored_bytes: u64,
}

impl FrameWriter {
    /// Start a fresh store in `dir`, replacing any prior store already at that path.
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
            prev: None,
            since_key: 0,
            stats: StoreStats::default(),
        })
    }

    /// Bytes written so far versus their raw size.
    pub fn stats(&self) -> StoreStats {
        self.stats
    }

    /// Append one frame. Three shapes, cheapest first:
    /// - identical to the previous frame: a 28-byte [`FLAG_DUP`] record, no pixel I/O;
    /// - otherwise LZ4-compressed, as an XOR delta against the previous frame when the
    ///   dimensions match and the chain is shorter than [`KEY_EVERY`], else as a keyframe;
    /// - raw BGRA whenever compression would not actually save bytes (tiny or noisy frames).
    ///
    /// Takes the frame by value so a distinct frame's buffer MOVES into the `prev` slot,
    /// cloning it would add a multi-MB memcpy per frame to the drain's hot path.
    pub fn push(&mut self, f: CapturedFrame) -> Result<(), String> {
        let raw_len = f.bgra.len() as u64;
        self.stats.frames += 1;
        self.stats.raw_bytes += raw_len;
        let same_dims = self
            .prev
            .as_ref()
            .is_some_and(|p| p.w == f.width && p.h == f.height);
        let is_dup = same_dims && self.prev.as_ref().is_some_and(|p| p.bgra == f.bgra);
        let rec = if is_dup {
            // A duplicate never touches `frames.raw`, so it cannot fail on a full disk.
            let p = self.prev.as_ref().unwrap();
            FrameRec {
                qpc: f.qpc,
                w: f.width,
                h: f.height,
                offset: p.offset,
                len: p.len,
                flags: FLAG_DUP,
            }
        } else {
            let as_delta = same_dims && self.since_key < KEY_EVERY;
            let prev_px = if as_delta {
                self.prev.as_ref().map(|p| p.bgra.as_slice())
            } else {
                None
            };
            let packed = encode_frame(&f.bgra, prev_px, f.width, f.height);
            let (bytes, flags): (&[u8], u8) = if (packed.len() as u64) < raw_len {
                let delta = if prev_px.is_some() { FLAG_DELTA } else { 0 };
                (&packed, FLAG_COMPRESSED | delta)
            } else {
                (&f.bgra, 0)
            };
            self.out
                .write_all(bytes)
                .map_err(|e| format!("frame write: {e}"))?;
            let rec = FrameRec {
                qpc: f.qpc,
                w: f.width,
                h: f.height,
                offset: self.offset,
                len: bytes.len() as u32,
                flags,
            };
            self.offset += bytes.len() as u64;
            self.stats.stored_bytes += bytes.len() as u64;
            self.since_key = if flags & FLAG_DELTA != 0 {
                self.since_key + 1
            } else {
                0
            };
            // Remember this frame's pixels (its bytes live at `rec.offset`) so the next one
            // can deduplicate or delta against it.
            self.prev = Some(PrevFrame {
                bgra: f.bgra,
                w: f.width,
                h: f.height,
                offset: rec.offset,
                len: rec.len,
            });
            rec
        };
        self.idx
            .write_all(&encode_rec(&rec))
            .map_err(|e| format!("frame index: {e}"))?;
        // Cheap periodic flush (buffered, no fsync) so a crash strands at most a fraction of
        // a second of frames. If the index runs ahead of what actually reached frames.raw,
        // `open` trims the excess, so this interleaving is always safe.
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
        self.log_stats();
        drop(self.out);
        drop(self.idx);
        FrameStore::open(&self.dir)
    }

    /// Finalize after a mid-recording write failure (e.g. a full disk): keep the frames that
    /// were already written instead of losing the whole take. Best-effort, further I/O
    /// errors are tolerated. When the disk filled, the newest frame's tail may still be in the
    /// buffer and never reach disk; `open` drops any frame whose bytes aren't wholly on disk,
    /// so every frame the returned store exposes reads back cleanly.
    pub fn finish_salvage(mut self) -> Result<FrameStore, String> {
        tracing::warn!("finalizing a truncated recording after a mid-write failure, salvaging frames already on disk");
        // These may fail on a full disk; open()'s trim covers the gap. Log a flush failure so
        // the salvage attempt leaves a trace even when the writes themselves went silent.
        if let Err(e) = self.out.flush() {
            tracing::warn!("salvage flush of frame data failed (expected on a full disk): {e}");
        }
        let _ = self.idx.flush();
        drop(self.out);
        drop(self.idx);
        FrameStore::open(&self.dir)
    }

    fn log_stats(&self) {
        let s = self.stats;
        if s.stored_bytes > 0 {
            tracing::info!(
                frames = s.frames,
                raw_mb = s.raw_bytes / (1024 * 1024),
                stored_mb = s.stored_bytes / (1024 * 1024),
                "frame store written ({:.1}x smaller than raw)",
                s.raw_bytes as f64 / s.stored_bytes as f64
            );
        }
    }
}

struct ReadState {
    file: File,
    /// One-slot cache: scrubbing hits the same/neighboring frame repeatedly, and sequential
    /// playback/export decodes frame `i` as a cheap delta on top of cached frame `i - 1`.
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
    /// `frames.raw` is dropped, so the store only exposes frames that read back cleanly.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let bytes = fs::read(index_path(dir)).map_err(|e| format!("frame index: {e}"))?;
        let file = File::open(raw_path(dir)).map_err(|e| format!("frame file: {e}"))?;
        let raw_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        // Records append in capture order. A frame that writes new pixels has a strictly
        // higher offset than any before it; a deduplicated frame back-references earlier bytes
        // (a smaller offset already known to be on disk). So the *first* record whose bytes run
        // past what's on disk is always a forward-writing one and marks the end of the
        // recoverable prefix, everything after it is untrustworthy, hence `break`. A dup
        // record points backward and always passes the check, so it's never the trip wire.
        // A delta record only decodes on top of its predecessor, so the prefix must also
        // start at an anchor; a store that doesn't is treated as empty.
        let mut index = Vec::with_capacity(bytes.len() / REC_SIZE);
        for chunk in bytes.as_chunks::<REC_SIZE>().0 {
            let rec = decode_rec(chunk);
            if rec.offset + u64::from(rec.len) > raw_len {
                break;
            }
            if index.is_empty() && !rec.is_anchor() {
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
    ///
    /// Anchor records (raw / keyframe) decode on their own. Delta and duplicate records need
    /// their predecessor, so this walks back to the nearest anchor (or to the cached frame,
    /// whichever is closer) and replays forward. [`KEY_EVERY`] bounds that replay, and
    /// sequential access (playback, export) costs exactly one delta per frame.
    pub fn frame(&self, i: usize) -> Result<Arc<CapturedFrame>, String> {
        let target = *self.index.get(i).ok_or("no such frame")?;
        let mut rs = self.read.lock().unwrap_or_else(|e| e.into_inner());
        let cached = rs.cache.as_ref().map(|(ci, f)| (*ci, Arc::clone(f)));
        if let Some((ci, f)) = &cached {
            if *ci == i {
                return Ok(Arc::clone(f));
            }
        }
        // Walk back to where decoding can start.
        let mut start = i;
        let mut from_cache = None;
        loop {
            if let Some((ci, f)) = &cached {
                if *ci == start && start != i {
                    from_cache = Some(Arc::clone(f));
                    break;
                }
            }
            if self.index[start].is_anchor() {
                break;
            }
            if start == 0 {
                return Err("frame store has no keyframe".into());
            }
            start -= 1;
        }
        // Decode the starting frame into a working buffer, then replay forward to `i`.
        let mut buf = match from_cache {
            Some(f) => f.bgra.clone(),
            None => self.decode_anchor(&mut rs.file, start)?,
        };
        let mut scratch = Vec::new();
        for (k, &rec) in self.index.iter().enumerate().take(i + 1).skip(start + 1) {
            if rec.flags & FLAG_DUP != 0 {
                continue;
            }
            if rec.flags & FLAG_DELTA == 0 {
                // An anchor mid-chain can only happen when walking from the cache; restart there.
                buf = self.decode_anchor(&mut rs.file, k)?;
                continue;
            }
            if buf.len() != rec.w as usize * rec.h as usize * 4 {
                return Err("frame delta dimension mismatch".into());
            }
            read_payload(&mut rs.file, &rec, &mut scratch)?;
            decode_frame(&scratch, rec.w, rec.h, &mut buf, true)?;
        }
        let frame = Arc::new(CapturedFrame {
            width: target.w,
            height: target.h,
            bgra: buf,
            qpc: target.qpc,
        });
        rs.cache = Some((i, Arc::clone(&frame)));
        Ok(frame)
    }

    /// Decode an anchor record (raw or compressed keyframe) to BGRA.
    fn decode_anchor(&self, file: &mut File, k: usize) -> Result<Vec<u8>, String> {
        let rec = self.index[k];
        let mut payload = Vec::new();
        read_payload(file, &rec, &mut payload)?;
        if rec.flags & FLAG_COMPRESSED == 0 {
            return Ok(payload);
        }
        let mut out = vec![0u8; rec.w as usize * rec.h as usize * 4];
        decode_frame(&payload, rec.w, rec.h, &mut out, false)?;
        Ok(out)
    }

    /// Total bytes the store occupies on disk (pixels + index).
    pub fn disk_bytes(dir: &Path) -> u64 {
        let f = |p: PathBuf| fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        f(raw_path(dir)) + f(index_path(dir))
    }
}

/// Read one record's payload bytes into `buf` (resized to fit).
fn read_payload(file: &mut File, rec: &FrameRec, buf: &mut Vec<u8>) -> Result<(), String> {
    buf.resize(rec.len as usize, 0);
    file.seek(SeekFrom::Start(rec.offset))
        .map_err(|e| format!("frame seek: {e}"))?;
    file.read_exact(buf).map_err(|e| format!("frame read: {e}"))
}

/// Copy the store in `src` into `dst` (which is created), rewriting every timestamp through
/// `map_qpc`. The pixel file is copied byte-for-byte, so saving or opening a project costs
/// one file copy instead of re-encoding every frame.
pub fn copy_store(src: &Path, dst: &Path, map_qpc: impl Fn(i64) -> i64) -> Result<usize, String> {
    let store = FrameStore::open(src)?;
    fs::create_dir_all(dst).map_err(|e| format!("store dir: {e}"))?;
    let _ = fs::remove_file(legacy_index_path(dst));
    fs::copy(raw_path(src), raw_path(dst)).map_err(|e| format!("copy frames: {e}"))?;
    let mut idx = Vec::with_capacity(store.len() * REC_SIZE);
    for r in store.recs() {
        idx.extend_from_slice(&encode_rec(&FrameRec {
            qpc: map_qpc(r.qpc),
            ..*r
        }));
    }
    fs::write(index_path(dst), idx).map_err(|e| format!("write index: {e}"))?;
    Ok(store.len())
}

/// Whether `dir` holds a frame store (used to tell new project bundles from PNG ones).
pub fn has_store(dir: &Path) -> bool {
    raw_path(dir).is_file() && index_path(dir).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh, unique temp dir for one test's store (auto-cleaned at the end of the test).
    fn tmp_dir(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vuoom-fs-test-{tag}-{n}-{seq}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 2x2 BGRA frame whose 16 bytes are all `fill`.
    fn frame(fill: u8, qpc: i64) -> CapturedFrame {
        CapturedFrame {
            width: 2,
            height: 2,
            bgra: vec![fill; 2 * 2 * 4],
            qpc,
        }
    }

    fn assert_pixels(store: &FrameStore, i: usize, fill: u8, qpc: i64) {
        let f = store.frame(i).unwrap();
        assert_eq!(f.width, 2);
        assert_eq!(f.height, 2);
        assert_eq!(f.qpc, qpc);
        assert_eq!(f.bgra, vec![fill; 16], "frame {i} pixels");
    }

    #[test]
    fn dedup_stores_identical_frame_as_backreference() {
        let dir = tmp_dir("dedup");
        let mut w = FrameWriter::create(dir.clone()).unwrap();
        w.push(frame(0xAA, 10)).unwrap(); // A
        w.push(frame(0xAA, 20)).unwrap(); // A again (duplicate)
        w.push(frame(0xBB, 30)).unwrap(); // B
        let store = w.finish().unwrap();

        // Only A's and B's pixels hit frames.raw, the duplicate wrote no pixels.
        let raw_len = fs::metadata(raw_path(&dir)).unwrap().len();
        assert_eq!(raw_len, 32, "raw file holds A(16)+B(16), not the duplicate");

        // All three records still read back the correct pixels & timestamps.
        assert_eq!(store.len(), 3);
        assert_pixels(&store, 0, 0xAA, 10);
        assert_pixels(&store, 1, 0xAA, 20);
        assert_pixels(&store, 2, 0xBB, 30);

        // The duplicate record back-references A's byte range.
        let recs = store.recs();
        assert_eq!(recs[1].offset, recs[0].offset);
        assert_eq!(recs[1].len, recs[0].len);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dimension_change_is_not_deduplicated() {
        let dir = tmp_dir("dims");
        let mut w = FrameWriter::create(dir.clone()).unwrap();
        // Two frames with the same fill byte but different sizes must both be stored raw.
        w.push(CapturedFrame {
            width: 2,
            height: 2,
            bgra: vec![0xAA; 16],
            qpc: 1,
        })
        .unwrap();
        w.push(CapturedFrame {
            width: 3,
            height: 2,
            bgra: vec![0xAA; 24],
            qpc: 2,
        })
        .unwrap();
        let store = w.finish().unwrap();

        let raw_len = fs::metadata(raw_path(&dir)).unwrap().len();
        assert_eq!(raw_len, 40, "16 + 24 bytes, nothing deduplicated");
        assert_eq!(store.len(), 2);
        assert_eq!(store.frame(1).unwrap().width, 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_keeps_all_when_last_record_is_a_backreferencing_dup() {
        // A, B, B: the final record is a duplicate pointing back at B's on-disk bytes.
        let dir = tmp_dir("trim-dup-tail");
        let mut w = FrameWriter::create(dir.clone()).unwrap();
        w.push(frame(0xAA, 1)).unwrap();
        w.push(frame(0xBB, 2)).unwrap();
        w.push(frame(0xBB, 3)).unwrap(); // dup of B
        w.finish().unwrap();

        // Re-open from disk: the trailing dup's target bytes exist, so all three survive.
        let store = FrameStore::open(&dir).unwrap();
        assert_eq!(store.len(), 3);
        assert_pixels(&store, 2, 0xBB, 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_truncates_at_a_forward_record_past_eof() {
        // A, A(dup), B on disk, then simulate a crash that lost B's pixel bytes by truncating
        // frames.raw to just A's 16 bytes. B's forward record now overruns EOF and is dropped,
        // but the two A records (offset 0, within EOF) survive.
        let dir = tmp_dir("trim-forward");
        let mut w = FrameWriter::create(dir.clone()).unwrap();
        w.push(frame(0xAA, 1)).unwrap();
        w.push(frame(0xAA, 2)).unwrap(); // dup
        w.push(frame(0xBB, 3)).unwrap();
        w.finish().unwrap();

        // Chop frames.raw back to A's bytes only.
        let f = File::options().write(true).open(raw_path(&dir)).unwrap();
        f.set_len(16).unwrap();
        drop(f);

        let store = FrameStore::open(&dir).unwrap();
        assert_eq!(
            store.len(),
            2,
            "B's forward record is trimmed; both A's remain"
        );
        assert_pixels(&store, 0, 0xAA, 1);
        assert_pixels(&store, 1, 0xAA, 2);

        let _ = fs::remove_dir_all(&dir);
    }

    /// A `w`×`h` frame of mostly flat "UI" with a moving 8×8 block (screen-like, compressible).
    fn screen(w: u32, h: u32, step: u32, qpc: i64) -> CapturedFrame {
        let mut bgra = vec![0x30u8; (w * h * 4) as usize];
        let bx = (step * 5) % (w - 8);
        let by = (step * 3) % (h - 8);
        for y in by..by + 8 {
            for x in bx..bx + 8 {
                let i = ((y * w + x) * 4) as usize;
                bgra[i..i + 4].copy_from_slice(&[step as u8, 0x80, 0xF0, 0xFF]);
            }
        }
        CapturedFrame {
            width: w,
            height: h,
            bgra,
            qpc,
        }
    }

    #[test]
    fn compressed_deltas_round_trip_in_any_access_order() {
        let dir = tmp_dir("codec");
        let (w, h) = (320, 200);
        let n = KEY_EVERY as usize * 2 + 7;
        let mut w_ = FrameWriter::create(dir.clone()).unwrap();
        for i in 0..n {
            // Every 4th frame repeats its predecessor to exercise FLAG_DUP inside chains.
            let step = if i % 4 == 3 { i as u32 - 1 } else { i as u32 };
            w_.push(screen(w, h, step, i as i64)).unwrap();
        }
        let stats = w_.stats();
        let store = w_.finish().unwrap();
        assert_eq!(store.len(), n);
        assert!(
            stats.stored_bytes * 20 < stats.raw_bytes,
            "screen-like frames should compress >20x, got {stats:?}"
        );
        let flags: Vec<u8> = store.recs().iter().map(|r| r.flags).collect();
        assert_eq!(flags[0], FLAG_COMPRESSED, "first frame is a keyframe");
        assert!(flags.contains(&(FLAG_COMPRESSED | FLAG_DELTA)));
        assert!(flags.contains(&FLAG_DUP));

        // Sequential, reverse and scattered reads all reproduce the exact pixels.
        let expect = |i: usize| {
            let step = if i % 4 == 3 { i as u32 - 1 } else { i as u32 };
            screen(w, h, step, i as i64).bgra
        };
        let reopened = FrameStore::open(&dir).unwrap();
        for s in [&store, &reopened] {
            for i in (0..n).chain((0..n).rev()).chain([40, 3, 61, 0, 33, 32, 31]) {
                let f = s.frame(i).unwrap();
                assert_eq!(f.qpc, i as i64);
                assert!(f.bgra == expect(i), "frame {i} pixels");
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keyframes_bound_the_delta_chain() {
        let dir = tmp_dir("keys");
        let mut w_ = FrameWriter::create(dir.clone()).unwrap();
        for i in 0..100 {
            w_.push(screen(256, 192, i, i64::from(i))).unwrap();
        }
        let store = w_.finish().unwrap();
        let mut chain = 0;
        for r in store.recs() {
            if r.flags & FLAG_DELTA != 0 {
                chain += 1;
                assert!(chain <= KEY_EVERY, "delta chain exceeded KEY_EVERY");
            } else {
                chain = 0;
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_torn_compressed_tail_is_trimmed() {
        let dir = tmp_dir("torn");
        let mut w_ = FrameWriter::create(dir.clone()).unwrap();
        for i in 0..10 {
            w_.push(screen(256, 192, i, i64::from(i))).unwrap();
        }
        let store = w_.finish().unwrap();
        let last = *store.recs().last().unwrap();
        drop(store);
        // Lose the final frame's bytes, as a crash between index flush and data flush would.
        let f = File::options().write(true).open(raw_path(&dir)).unwrap();
        f.set_len(last.offset + u64::from(last.len) - 1).unwrap();
        drop(f);
        let store = FrameStore::open(&dir).unwrap();
        assert_eq!(store.len(), 9);
        assert!(store.frame(8).unwrap().bgra == screen(256, 192, 8, 8).bgra);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_store_rebases_timestamps_and_keeps_pixels() {
        let src = tmp_dir("copy-src");
        let dst = tmp_dir("copy-dst");
        let mut w_ = FrameWriter::create(src.clone()).unwrap();
        for i in 0..12 {
            w_.push(screen(256, 192, i, 1000 + i64::from(i))).unwrap();
        }
        w_.finish().unwrap();
        assert_eq!(copy_store(&src, &dst, |q| q - 1000).unwrap(), 12);
        assert!(has_store(&dst));
        let store = FrameStore::open(&dst).unwrap();
        for i in 0..12 {
            let f = store.frame(i).unwrap();
            assert_eq!(f.qpc, i as i64);
            assert!(f.bgra == screen(256, 192, i as u32, 0).bgra);
        }
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn record_flags_round_trip_through_the_index_encoding() {
        let r = FrameRec {
            qpc: -5,
            w: 7680,
            h: 4320,
            offset: 1 << 40,
            len: 7680 * 4320 * 4,
            flags: FLAG_COMPRESSED | FLAG_DELTA,
        };
        let d = decode_rec(&encode_rec(&r));
        assert_eq!(
            (d.qpc, d.w, d.h, d.offset, d.len, d.flags),
            (r.qpc, r.w, r.h, r.offset, r.len, r.flags)
        );
    }
}
