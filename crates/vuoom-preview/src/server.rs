//! Localhost WebSocket server that streams composited preview frames ("latest wins").
//!
//! Binds an ephemeral `127.0.0.1` port (returned to the webview), and pushes the most
//! recent packed frame to each connected client via a `watch` channel — so a slow client
//! never backs up the compositor. See `docs/05-Compositing-and-Preview.md`.

use futures_util::SinkExt;
use std::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;

/// Generate a 128-bit random session token, hex-encoded to 32 ASCII chars.
///
/// Any local process can reach the preview port, so the frontend must prove it is the
/// legitimate client by presenting this token in the WS upgrade path. 128 bits from the OS
/// CSPRNG is far beyond guessable for a short-lived localhost socket.
fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    // The OS RNG is the same source the WS handshake itself relies on; a failure here means
    // the platform has no entropy source, in which case refusing to start is correct.
    getrandom::getrandom(&mut bytes).expect("OS RNG unavailable for preview auth token");
    let mut token = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // char::from_digit is infallible for a radix-16 nibble (0..=15).
        token.push(char::from_digit(u32::from(b >> 4), 16).unwrap());
        token.push(char::from_digit(u32::from(b & 0x0f), 16).unwrap());
    }
    token
}

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
    token: String,
    sink: FrameSink,
}

impl PreviewServer {
    /// Bind an ephemeral localhost port and start accepting preview clients.
    ///
    /// Each instance mints a random session token; a client must present it in the WS
    /// upgrade path (`/ws/{token}`) or the connection is refused before it upgrades. This
    /// stops any other local process from opening the socket and reading the user's screen.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the listener cannot bind.
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let token = generate_token();
        let (tx, _rx) = watch::channel(Vec::new());
        let sink = FrameSink { tx: tx.clone() };

        let accept_token = token.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        tokio::spawn(serve_client(stream, tx.subscribe(), accept_token.clone()));
                    }
                    Err(e) => tracing::warn!("preview accept failed: {e}"),
                }
            }
        });

        Ok(Self { port, token, sink })
    }

    /// The bound port — pass it to the webview so it can connect.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The per-session auth token the webview must include in the WS URL path.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// A cloneable handle for publishing frames from the compositor.
    #[must_use]
    pub fn sink(&self) -> FrameSink {
        self.sink.clone()
    }
}

async fn serve_client(stream: TcpStream, mut rx: watch::Receiver<Vec<u8>>, token: String) {
    let expected_path = format!("/ws/{token}");
    // Validate the token during the WS upgrade: on mismatch we return 403 and the handshake
    // is rejected before it ever upgrades, so an unauthorized peer never receives a frame.
    // The Result<Response, ErrorResponse> shape is tungstenite's `Callback` contract — the
    // large Err variant is not ours to box.
    #[allow(clippy::result_large_err)]
    let check_token = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        if req.uri().path() == expected_path {
            Ok(resp)
        } else {
            let mut err = ErrorResponse::new(Some("invalid preview token".to_string()));
            *err.status_mut() = StatusCode::FORBIDDEN;
            Err(err)
        }
    };
    let mut ws = match tokio_tungstenite::accept_hdr_async(stream, check_token).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::debug!("preview handshake rejected: {e}");
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

    #[test]
    fn tokens_are_128_bit_hex_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32, "128 bits hex-encodes to 32 chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "each token must be distinct");
    }
}
