//! Grouping recognized words into caption cues: short lines that break where the speaker
//! does (a sentence end or a pause), never run too long to read, and stay up long enough to
//! be read.

/// A recognized word and when it was spoken (seconds from the start of the audio). A word
/// that continues the previous one (punctuation, or the rest of a split word) has no leading
/// space, as whisper reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// One caption: a line of text and when it is on screen (seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A cue's longest line in characters (subtitle guidelines put a line at about 42).
pub const MAX_CHARS: usize = 42;
/// A cue never covers more speech than this (seconds).
pub const MAX_SECS: f64 = 5.0;
/// A pause this long between words starts a new cue (seconds).
pub const PAUSE_SECS: f64 = 0.6;
/// A short cue stays up at least this long, when the next one leaves room (seconds).
pub const MIN_SECS: f64 = 1.0;

/// Group words, in order, into cues.
#[must_use]
pub fn group(words: &[Word]) -> Vec<Cue> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut cur: Option<Cue> = None;
    for w in words {
        let text = w.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(c) = cur.as_mut() {
            // A continuation (no leading space) always stays with its word.
            if !w.text.starts_with(char::is_whitespace) {
                c.text.push_str(text);
                c.end = c.end.max(w.end);
                continue;
            }
            let len = c.text.chars().count() + 1 + text.chars().count();
            let fits = len <= MAX_CHARS && w.end - c.start <= MAX_SECS;
            let sentence_ended = c.text.ends_with(['.', '?', '!']);
            let paused = w.start - c.end >= PAUSE_SECS;
            if fits && !sentence_ended && !paused {
                c.text.push(' ');
                c.text.push_str(text);
                c.end = c.end.max(w.end);
                continue;
            }
            cues.extend(cur.take());
        }
        cur = Some(Cue {
            start: w.start,
            end: w.end,
            text: text.to_string(),
        });
    }
    cues.extend(cur);
    hold(&mut cues);
    cues
}

/// Keep each cue up for at least [`MIN_SECS`], without running into the next one.
fn hold(cues: &mut [Cue]) {
    let next_starts: Vec<f64> = cues.iter().skip(1).map(|c| c.start).collect();
    let limits = next_starts.into_iter().chain([f64::INFINITY]);
    for (c, limit) in cues.iter_mut().zip(limits) {
        if c.end - c.start < MIN_SECS {
            c.end = (c.start + MIN_SECS).min(limit).max(c.end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64) -> Word {
        Word {
            text: text.to_string(),
            start,
            end,
        }
    }

    fn texts(cues: &[Cue]) -> Vec<&str> {
        cues.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn words_join_into_one_cue() {
        let words = [
            w(" Hello", 0.0, 0.4),
            w(" there", 0.4, 0.8),
            w(",", 0.8, 0.8),
        ];
        let cues = group(&words);
        assert_eq!(texts(&cues), ["Hello there,"]);
        assert_eq!(cues[0].start, 0.0);
        // Held to the minimum on-screen time.
        assert_eq!(cues[0].end, MIN_SECS);
    }

    #[test]
    fn a_sentence_end_or_a_pause_starts_a_new_cue() {
        let cues = group(&[
            w(" Click", 0.0, 0.3),
            w(" Save.", 0.3, 0.7),
            w(" Then", 0.8, 1.0),
            w(" wait", 1.0, 1.3),
            w(" here", 2.2, 2.5),
        ]);
        assert_eq!(texts(&cues), ["Click Save.", "Then wait", "here"]);
    }

    #[test]
    fn long_speech_is_split_at_the_line_limit() {
        let words: Vec<Word> = (0..30)
            .map(|i| w(" word", f64::from(i) * 0.2, f64::from(i) * 0.2 + 0.2))
            .collect();
        let cues = group(&words);
        assert!(cues.len() > 1);
        for c in &cues {
            assert!(c.text.chars().count() <= MAX_CHARS, "{:?}", c.text);
            assert!(c.end - c.start <= MAX_SECS + 1e-9);
        }
        // Nothing is lost or reordered.
        let all: Vec<&str> = cues.iter().flat_map(|c| c.text.split(' ')).collect();
        assert_eq!(all.len(), 30);
    }

    #[test]
    fn a_short_cue_is_held_but_never_overlaps_the_next() {
        let cues = group(&[w(" Yes.", 0.0, 0.2), w(" No.", 0.5, 0.7)]);
        assert_eq!(cues[0].end, 0.5);
        assert_eq!(cues[1].end, 0.5 + MIN_SECS);
    }

    #[test]
    fn blank_words_are_skipped() {
        let cues = group(&[w(" ", 0.0, 0.1), w("", 0.1, 0.2)]);
        assert!(cues.is_empty());
    }
}
