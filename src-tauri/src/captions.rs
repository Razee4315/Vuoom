//! Captions in the app: the speech model, downloaded once on first use and checked before
//! it is kept. The speech recognition itself runs in the `vuoom-captions` crate (see
//! `Session::generate_captions`).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use vuoom_captions::{CANCELLED, MODEL};

/// What the user sees when the download can't reach the server.
const OFFLINE: &str =
    "Couldn't download the speech model. Check your internet connection and try again.";
/// What the user sees when the downloaded file isn't the model it should be.
const DAMAGED: &str = "The speech model didn't download correctly. Please try again.";

/// Where the speech model lives: `<app local data>/models/<file>`.
pub fn model_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("no app data folder: {e}"))?;
    Ok(dir.join("models").join(MODEL.file))
}

/// Whether the model is already downloaded (it is only ever kept whole and checked).
pub fn model_ready(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == MODEL.bytes)
}

/// Download the model to `path`. `progress(done, total)` gets bytes as they arrive (once
/// per percent); setting `cancel` stops it with `Err(CANCELLED)`. The file is written
/// aside, checked against its SHA-256 and only then renamed into place.
pub async fn download(
    path: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<(), String> {
    let save_err = |e: std::io::Error| format!("couldn't save the speech model: {e}");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(save_err)?;
    }
    // reqwest is built without a TLS crypto provider (the updater shares it); install ring
    // unless something else already has.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("couldn't start the download: {e}"))?;
    let offline = |e: reqwest::Error| {
        tracing::warn!("model download failed: {e}");
        String::from(OFFLINE)
    };
    let mut res = client
        .get(MODEL.url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(offline)?;

    let part = path.with_extension("bin.part");
    let mut file = std::fs::File::create(&part).map_err(save_err)?;
    let mut hasher = Sha256::new();
    let mut done = 0_u64;
    let mut shown = u64::MAX;
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err(CANCELLED.to_string());
        }
        let chunk = match res.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break Ok(()),
            Err(e) => break Err(offline(e)),
        };
        if let Err(e) = file.write_all(&chunk) {
            break Err(save_err(e));
        }
        hasher.update(&chunk);
        done += chunk.len() as u64;
        let percent = done * 100 / MODEL.bytes.max(1);
        if percent != shown {
            shown = percent;
            progress(done, MODEL.bytes);
        }
    };
    let result = result.and_then(|()| file.flush().map_err(save_err));
    drop(file);
    let checked = result.and_then(|()| {
        let hash = format!("{:x}", hasher.finalize());
        if done == MODEL.bytes && hash == MODEL.sha256 {
            Ok(())
        } else {
            tracing::warn!("model download mismatch: {done} bytes, sha256 {hash}");
            Err(String::from(DAMAGED))
        }
    });
    let kept = checked.and_then(|()| std::fs::rename(&part, path).map_err(save_err));
    if kept.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    kept
}
