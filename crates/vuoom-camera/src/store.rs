//! The camera track on disk: timestamped JPEG frames appended to one file.
//!
//! `camera.vcam` is an 8-byte header (`VCAM` and a version) followed by one record per
//! frame: `u32 length | f64 time | JPEG bytes`, little endian, the time in seconds on the
//! recording clock. Records are only ever appended, so a crash mid-take loses at most the
//! frame being written: the reader stops at the first record that runs past the end of
//! the file.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

const MAGIC: &[u8; 4] = b"VCAM";
const VERSION: u32 = 1;
const HEADER_LEN: u64 = 8;
/// Length and time before each frame's bytes.
const RECORD_HEAD: u64 = 12;
/// A frame claiming to be larger than this is corruption, not data.
const MAX_FRAME: u32 = 16 << 20;

/// Appends frames to a camera track.
pub struct TrackWriter {
    out: BufWriter<File>,
    frames: u64,
    last_t: f64,
}

impl TrackWriter {
    /// Create `path` (truncating) and write the header.
    ///
    /// # Errors
    /// Returns a message if the file can't be created or written.
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = File::create(path).map_err(|e| format!("camera file: {e}"))?;
        let mut out = BufWriter::with_capacity(512 * 1024, file);
        out.write_all(MAGIC)
            .and_then(|()| out.write_all(&VERSION.to_le_bytes()))
            .map_err(|e| format!("camera file: {e}"))?;
        Ok(Self {
            out,
            frames: 0,
            last_t: f64::NEG_INFINITY,
        })
    }

    /// Append one frame shown from `t` seconds. Frames must arrive in time order: one that
    /// doesn't (or an empty or oversized one) is skipped and `false` returned.
    ///
    /// # Errors
    /// Returns a message if the disk write fails.
    pub fn push(&mut self, t: f64, jpeg: &[u8]) -> Result<bool, String> {
        let Ok(len) = u32::try_from(jpeg.len()) else {
            return Ok(false);
        };
        if len == 0 || len > MAX_FRAME || !t.is_finite() || t < self.last_t {
            return Ok(false);
        }
        let mut head = [0u8; RECORD_HEAD as usize];
        head[..4].copy_from_slice(&len.to_le_bytes());
        head[4..].copy_from_slice(&t.to_le_bytes());
        self.out
            .write_all(&head)
            .and_then(|()| self.out.write_all(jpeg))
            .map_err(|e| format!("camera write: {e}"))?;
        self.frames += 1;
        self.last_t = t;
        Ok(true)
    }

    /// Frames written so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Push buffered frames to disk (so a crash keeps them).
    ///
    /// # Errors
    /// Returns a message if the flush fails.
    pub fn flush(&mut self) -> Result<(), String> {
        self.out.flush().map_err(|e| format!("camera flush: {e}"))
    }

    /// Flush and close. Returns the frames written.
    ///
    /// # Errors
    /// Returns a message if the final flush fails.
    pub fn finish(mut self) -> Result<u64, String> {
        self.flush()?;
        Ok(self.frames)
    }
}

/// Where one frame sits in the file.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    t: f64,
    offset: u64,
    len: u32,
}

/// Random access to a camera track's frames.
pub struct TrackReader {
    file: Mutex<File>,
    index: Vec<Entry>,
}

impl TrackReader {
    /// Open a track and index its frames, stopping at a torn final record.
    ///
    /// # Errors
    /// Returns a message if the file can't be read or isn't a camera track.
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("camera file: {e}"))?;
        let size = file
            .metadata()
            .map_err(|e| format!("camera file: {e}"))?
            .len();
        let mut header = [0u8; HEADER_LEN as usize];
        file.read_exact(&mut header)
            .map_err(|_| "not a camera track".to_string())?;
        if &header[..4] != MAGIC {
            return Err("not a camera track".into());
        }
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if version != VERSION {
            return Err(format!("camera track version {version} is not supported"));
        }
        let mut index = Vec::new();
        let mut pos = HEADER_LEN;
        let mut head = [0u8; RECORD_HEAD as usize];
        while pos + RECORD_HEAD <= size {
            if file.read_exact(&mut head).is_err() {
                break;
            }
            let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
            let mut t_bytes = [0u8; 8];
            t_bytes.copy_from_slice(&head[4..]);
            let t = f64::from_le_bytes(t_bytes);
            let body = pos + RECORD_HEAD;
            if len == 0 || len > MAX_FRAME || !t.is_finite() || body + u64::from(len) > size {
                break;
            }
            index.push(Entry {
                t,
                offset: body,
                len,
            });
            pos = body + u64::from(len);
            if file.seek(SeekFrom::Start(pos)).is_err() {
                break;
            }
        }
        Ok(Self {
            file: Mutex::new(file),
            index,
        })
    }

    /// Number of frames.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// When frame `i` starts showing (seconds on the recording clock).
    #[must_use]
    pub fn time(&self, i: usize) -> Option<f64> {
        self.index.get(i).map(|e| e.t)
    }

    /// The frame showing at `t`: the latest one at or before it, or the first frame before
    /// the track starts. `None` for an empty track.
    #[must_use]
    pub fn index_at(&self, t: f64) -> Option<usize> {
        if self.index.is_empty() {
            return None;
        }
        Some(self.index.partition_point(|e| e.t <= t).saturating_sub(1))
    }

    /// The JPEG bytes of frame `i`.
    ///
    /// # Errors
    /// Returns a message if `i` is out of range or the read fails.
    pub fn read(&self, i: usize) -> Result<Vec<u8>, String> {
        let e = self.index.get(i).ok_or("no such camera frame")?;
        let mut file = self.file.lock().unwrap_or_else(|p| p.into_inner());
        let mut buf = vec![0u8; e.len as usize];
        file.seek(SeekFrom::Start(e.offset))
            .and_then(|_| file.read_exact(&mut buf))
            .map_err(|err| format!("camera read: {err}"))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(frames: &[(f64, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("camera.vcam");
        let mut w = TrackWriter::create(&path).unwrap();
        for (t, bytes) in frames {
            w.push(*t, bytes.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn frames_round_trip_and_are_found_by_time() {
        let (_dir, path) = track(&[(0.0, "aa"), (0.033, "bbb"), (0.066, "c")]);
        let r = TrackReader::open(&path).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r.read(1).unwrap(), b"bbb");
        assert_eq!(r.time(2), Some(0.066));
        // Between frames: the one already showing. Before the first: the first.
        assert_eq!(r.index_at(0.05), Some(1));
        assert_eq!(r.index_at(0.033), Some(1));
        assert_eq!(r.index_at(-1.0), Some(0));
        assert_eq!(r.index_at(99.0), Some(2));
    }

    #[test]
    fn out_of_order_empty_and_bad_times_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("camera.vcam");
        let mut w = TrackWriter::create(&path).unwrap();
        assert!(w.push(1.0, b"x").unwrap());
        assert!(!w.push(0.5, b"y").unwrap());
        assert!(!w.push(2.0, b"").unwrap());
        assert!(!w.push(f64::NAN, b"z").unwrap());
        assert_eq!(w.finish().unwrap(), 1);
        assert_eq!(TrackReader::open(&path).unwrap().len(), 1);
    }

    #[test]
    fn a_torn_last_frame_is_dropped() {
        let (_dir, path) = track(&[(0.0, "first"), (0.5, "second")]);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 3]).unwrap();
        let r = TrackReader::open(&path).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r.read(0).unwrap(), b"first");
    }

    #[test]
    fn other_files_are_rejected_and_empty_tracks_have_no_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.vcam");
        std::fs::write(&path, b"RIFF....").unwrap();
        assert!(TrackReader::open(&path).is_err());
        let (_d, empty) = track(&[]);
        let r = TrackReader::open(&empty).unwrap();
        assert!(r.is_empty());
        assert_eq!(r.index_at(1.0), None);
    }
}
