//! Encoder error type.

/// Errors from the GIF export path.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// There were no frames to encode.
    #[error("no frames to encode")]
    NoFrames,
    /// A filesystem operation failed.
    #[error("io error")]
    Io(#[from] std::io::Error),
    /// Reading or writing a PNG failed.
    #[error("png read or write failed: {0}")]
    Png(String),
    /// Native (pure-Rust) GIF encoding failed.
    #[error("gif encoding failed: {0}")]
    Gif(String),
}
