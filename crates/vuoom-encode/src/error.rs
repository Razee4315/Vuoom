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
    /// PNG encoding failed.
    #[error("png encoding failed: {0}")]
    Png(String),
    /// Native (pure-Rust) GIF encoding failed.
    #[error("gif encoding failed: {0}")]
    Gif(String),
    /// The external tool could not be launched (e.g. binary missing).
    #[error("failed to spawn {tool}")]
    Spawn {
        tool: String,
        #[source]
        source: std::io::Error,
    },
    /// The external tool ran but exited non-zero.
    #[error("{tool} exited unsuccessfully ({status})")]
    Failed { tool: String, status: String },
}
