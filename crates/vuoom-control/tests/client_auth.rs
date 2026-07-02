//! Integration: the blocking [`Client`] against a minimal in-process server that mimics
//! the real control server's auth handshake — the first line of a connection must be the
//! token, or the server silently drops the socket.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;

use vuoom_control::{write_message, Client, ControlRequest, ControlResponse};

/// Spawn a tiny token-checking server; answers `Ping` with `Ok`.
fn spawn_server(token: String) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let token = token.clone();
            std::thread::spawn(move || {
                let Ok(mut writer) = stream.try_clone() else {
                    return;
                };
                let mut reader = BufReader::new(stream);
                let mut auth = String::new();
                if reader.read_line(&mut auth).is_err() || auth.trim() != token {
                    return; // drop unauthenticated peers, like the real server
                }
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let resp = match serde_json::from_str::<ControlRequest>(&line) {
                        Ok(ControlRequest::Ping) => ControlResponse::Ok,
                        Ok(_) => ControlResponse::error("unsupported in test"),
                        Err(e) => ControlResponse::error(format!("bad request: {e}")),
                    };
                    if write_message(&mut writer, &resp).is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

#[test]
fn authenticated_client_round_trips() {
    let token = vuoom_control::generate_token();
    let port = spawn_server(token.clone());
    let mut client = Client::connect(port, &token).expect("connect");
    assert_eq!(
        client.call(&ControlRequest::Ping).expect("call"),
        ControlResponse::Ok
    );
    // The connection stays usable for further calls.
    assert_eq!(
        client.call(&ControlRequest::Ping).expect("second call"),
        ControlResponse::Ok
    );
}

#[test]
fn wrong_token_is_rejected() {
    let token = vuoom_control::generate_token();
    let port = spawn_server(token);
    // Connecting succeeds (TCP accepts), but the first call finds the socket closed.
    let mut client = Client::connect(port, "wrong-token").expect("tcp connect");
    let err = client
        .call(&ControlRequest::Ping)
        .expect_err("must be rejected");
    assert!(
        err.contains("closed the connection"),
        "unexpected error: {err}"
    );
}
