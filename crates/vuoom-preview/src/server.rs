//! Localhost WebSocket server that streams composited preview frames ("latest wins").
//!
//! Binds an ephemeral `127.0.0.1` port (returned to the webview), and pushes the most
//! recent packed frame to each connected client via a `watch` channel — so a slow client
//! never backs up the compositor. See `docs/05-Compositing-and-Preview.md`.

use futures_util::SinkExt;
use std::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

/// A handle for publishing the latest packed preview frame.
#[derive(Clone)]
pub struct FrameSink {
    tx: watch::Sender<Vec<u8>>,
}

impl FrameSink {
    /// Publish a packed frame (see [`crate::pack_frame`]). With no connected clients the
    /// frame is simply dropped.
    pub fn publish(&self, frame: Vec<u8>) {
        let _ = self.tx.send(frame);
    }
}

/// A `127.0.0.1` WebSocket server streaming composited frames to the webview.
pub struct PreviewServer {
    port: u16,
    sink: FrameSink,
}

impl PreviewServer {
    /// Bind an ephemeral localhost port and start accepting preview clients.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the listener cannot bind.
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let (tx, _rx) = watch::channel(Vec::new());
        let sink = FrameSink { tx: tx.clone() };

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        tokio::spawn(serve_client(stream, tx.subscribe()));
                    }
                    Err(e) => tracing::warn!("preview accept failed: {e}"),
                }
            }
        });

        Ok(Self { port, sink })
    }

    /// The bound port — pass it to the webview so it can connect.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// A cloneable handle for publishing frames from the compositor.
    #[must_use]
    pub fn sink(&self) -> FrameSink {
        self.sink.clone()
    }
}

async fn serve_client(stream: TcpStream, mut rx: watch::Receiver<Vec<u8>>) {
    let mut ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::debug!("preview handshake failed: {e}");
            return;
        }
    };
    while rx.changed().await.is_ok() {
        let frame = rx.borrow_and_update().clone();
        if frame.is_empty() {
            continue;
        }
        if ws.send(Message::Binary(frame)).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_binds_an_ephemeral_port() {
        let server = PreviewServer::start().await.expect("bind");
        assert!(server.port() > 0);
        // Publishing with no clients connected must not panic.
        server.sink().publish(vec![1, 2, 3]);
    }
}
