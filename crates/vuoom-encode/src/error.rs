//! Encoder error type.

/// Errors from the export encoding path.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// There were no frames to encode.
    #[error("no frames to encode")]
    NoFrames,
    /// A filesystem operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Reading or writing a PNG failed.
    #[error("png read or write failed: {0}")]
    Png(String),
    /// Native (pure-Rust) GIF encoding failed.
    #[error("gif encoding failed: {0}")]
    Gif(String),
    /// Animated WebP encoding failed.
    #[error("webp encoding failed: {0}")]
    Webp(String),
}
