//! 16-bit PCM WAV storage for recorded audio tracks.
//!
//! The writer streams samples to disk as they arrive and patches the RIFF sizes when it
//! finishes. A crash mid-recording leaves those sizes at zero, so the reader never trusts
//! them: it takes the `data` chunk to the end of the file, which keeps a crashed take's
//! audio recoverable alongside its frames.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// Interleaved 16-bit PCM held in memory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    /// Interleaved samples, `channels` per frame.
    pub samples: Vec<i16>,
}

impl Pcm {
    /// Number of sample frames (one sample per channel each).
    #[must_use]
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / usize::from(self.channels)
        }
    }

    /// Length in seconds.
    #[must_use]
    pub fn duration(&self) -> f64 {
        if self.rate == 0 {
            0.0
        } else {
            self.frames() as f64 / f64::from(self.rate)
        }
    }
}

/// Size of the canonical 44-byte header this module writes.
const HEADER_LEN: u64 = 44;

fn header(rate: u32, channels: u16, data_len: u32) -> [u8; 44] {
    let block = u32::from(channels) * 2;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_len.saturating_add(36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * block).to_le_bytes());
    h[32..34].copy_from_slice(&(block as u16).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Streaming 16-bit PCM WAV writer.
pub struct WavWriter {
    out: BufWriter<File>,
    rate: u32,
    channels: u16,
    frames: u64,
}

impl WavWriter {
    /// Create `path` (truncating) and write a header with zero sizes.
    ///
    /// # Errors
    /// Returns a message if the file cannot be created or written.
    pub fn create(path: &Path, rate: u32, channels: u16) -> Result<Self, String> {
        if rate == 0 || channels == 0 {
            return Err("invalid audio format".into());
        }
        let file = File::create(path).map_err(|e| format!("audio file: {e}"))?;
        let mut out = BufWriter::with_capacity(256 * 1024, file);
        out.write_all(&header(rate, channels, 0))
            .map_err(|e| format!("audio file: {e}"))?;
        Ok(Self {
            out,
            rate,
            channels,
            frames: 0,
        })
    }

    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Sample frames written so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Append interleaved samples (a whole number of frames).
    ///
    /// # Errors
    /// Returns a message if the disk write fails.
    pub fn write(&mut self, samples: &[i16]) -> Result<(), String> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.out
            .write_all(&bytes)
            .map_err(|e| format!("audio write: {e}"))?;
        self.frames += (samples.len() / usize::from(self.channels)) as u64;
        Ok(())
    }

    /// Append `frames` frames of silence.
    ///
    /// # Errors
    /// Returns a message if the disk write fails.
    pub fn write_silence(&mut self, frames: u64) -> Result<(), String> {
        const CHUNK: usize = 4096;
        let zeros = vec![0i16; CHUNK * usize::from(self.channels)];
        let mut left = frames;
        while left > 0 {
            let n = left.min(CHUNK as u64) as usize;
            self.write(&zeros[..n * usize::from(self.channels)])?;
            left -= n as u64;
        }
        Ok(())
    }

    /// Flush and patch the RIFF/data sizes.
    ///
    /// # Errors
    /// Returns a message if the final flush or header patch fails.
    pub fn finish(mut self) -> Result<u64, String> {
        self.out.flush().map_err(|e| format!("audio flush: {e}"))?;
        let data_len = self.frames * u64::from(self.channels) * 2;
        let data_len = u32::try_from(data_len).unwrap_or(u32::MAX);
        let head = header(self.rate, self.channels, data_len);
        let file = self.out.get_mut();
        let patched = file
            .seek(SeekFrom::Start(0))
            .and_then(|_| file.write_all(&head));
        patched.map_err(|e| format!("audio header: {e}"))?;
        Ok(self.frames)
    }
}

/// Parse a WAV file's bytes. Accepts 16-bit PCM (what this module writes); the `data`
/// chunk is read to the end of the buffer when its declared size is missing or too large.
///
/// # Errors
/// Returns a message for anything that isn't a 16-bit PCM WAV.
pub fn parse(bytes: &[u8]) -> Result<Pcm, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut pos = 12;
    let mut fmt: Option<(u16, u32, u16)> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if body + 16 > bytes.len() {
                return Err("truncated WAV format".into());
            }
            let tag = u16_at(body);
            let channels = u16_at(body + 2);
            let rate = u32_at(body + 4);
            let bits = u16_at(body + 14);
            if !(tag == 1 || tag == 0xFFFE) || bits != 16 {
                return Err("unsupported WAV encoding (16-bit PCM expected)".into());
            }
            fmt = Some((channels, rate, bits));
        } else if id == b"data" {
            let (channels, rate, _) = fmt.ok_or("WAV data before format")?;
            if channels == 0 || rate == 0 {
                return Err("invalid WAV format".into());
            }
            let avail = bytes.len() - body;
            let len = if size == 0 || size > avail {
                avail
            } else {
                size
            };
            let block = usize::from(channels) * 2;
            let len = len - len % block;
            let samples = bytes[body..body + len]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            return Ok(Pcm {
                rate,
                channels,
                samples,
            });
        }
        // Chunks are word-aligned.
        pos = body.saturating_add(size).saturating_add(size & 1);
    }
    Err("WAV has no audio data".into())
}

/// Read and parse a WAV file.
///
/// # Errors
/// Returns a message if the file can't be read or isn't 16-bit PCM.
pub fn read(path: &Path) -> Result<Pcm, String> {
    parse(&std::fs::read(path).map_err(|e| format!("audio file: {e}"))?)
}

/// A WAV file's bytes with correct RIFF/data sizes, for consumers (like a browser's audio
/// decoder) that trust the header. Repairs the header of a take that crashed mid-write.
///
/// # Errors
/// Returns a message if the file can't be read or isn't 16-bit PCM.
pub fn normalized_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let pcm = read(path)?;
    Ok(encode(&pcm))
}

/// Serialize PCM as a canonical WAV.
#[must_use]
pub fn encode(pcm: &Pcm) -> Vec<u8> {
    let data_len = u32::try_from(pcm.samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(HEADER_LEN as usize + pcm.samples.len() * 2);
    out.extend_from_slice(&header(pcm.rate, pcm.channels, data_len));
    for s in &pcm.samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.write(&[1, -1, 2, -2]).unwrap();
        w.write_silence(3).unwrap();
        assert_eq!(w.finish().unwrap(), 5);
        let pcm = read(&path).unwrap();
        assert_eq!(pcm.rate, 48_000);
        assert_eq!(pcm.channels, 2);
        assert_eq!(pcm.samples, vec![1, -1, 2, -2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(pcm.frames(), 5);
    }

    #[test]
    fn unfinished_file_is_still_readable() {
        // A crash never patches the header; the data runs to the end of the file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.wav");
        let mut w = WavWriter::create(&path, 44_100, 1).unwrap();
        w.write(&[7, 8, 9]).unwrap();
        drop(w); // BufWriter flushes on drop; no header patch
        let pcm = read(&path).unwrap();
        assert_eq!(pcm.samples, vec![7, 8, 9]);
        let fixed = normalized_bytes(&path).unwrap();
        assert_eq!(u32::from_le_bytes(fixed[40..44].try_into().unwrap()), 6);
        assert_eq!(parse(&fixed).unwrap(), pcm);
    }

    #[test]
    fn a_partial_trailing_frame_is_dropped() {
        let mut bytes = encode(&Pcm {
            rate: 8_000,
            channels: 2,
            samples: vec![1, 2, 3, 4],
        });
        bytes[40..44].copy_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&5i16.to_le_bytes()); // half a frame
        assert_eq!(parse(&bytes).unwrap().samples, vec![1, 2, 3, 4]);
    }

    #[test]
    fn rejects_non_wav_and_other_encodings() {
        assert!(parse(b"hello world, not audio").is_err());
        let mut bytes = encode(&Pcm {
            rate: 8_000,
            channels: 1,
            samples: vec![0],
        });
        bytes[34] = 32; // 32-bit samples
        assert!(parse(&bytes).is_err());
    }
}
