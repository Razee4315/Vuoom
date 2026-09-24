//! SubRip (`.srt`) output: the caption format nearly every video site and player reads.

use std::fmt::Write;

use crate::cues::Cue;

/// Write cues as an SRT file (numbered from 1, `HH:MM:SS,mmm` times).
#[must_use]
pub fn srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        let (a, b) = (stamp(c.start), stamp(c.end));
        let _ = write!(out, "{}\n{a} --> {b}\n{}\n\n", i + 1, c.text);
    }
    out
}

/// `HH:MM:SS,mmm`, rounded to the millisecond.
fn stamp(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    let (h, m) = (ms / 3_600_000, ms / 60_000 % 60);
    let (s, f) = (ms / 1000 % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02},{f:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_numbered_and_timed() {
        let cues = [
            Cue {
                start: 0.0,
                end: 1.5,
                text: "Hello there.".into(),
            },
            Cue {
                start: 3_725.042_4,
                end: 3_726.0,
                text: "Later on".into(),
            },
        ];
        let expected = "1\n00:00:00,000 --> 00:00:01,500\nHello there.\n\n\
                        2\n01:02:05,042 --> 01:02:06,000\nLater on\n\n";
        assert_eq!(srt(&cues), expected);
    }

    #[test]
    fn no_cues_is_an_empty_file() {
        assert_eq!(srt(&[]), "");
    }
}
