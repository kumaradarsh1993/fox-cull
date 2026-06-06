//! Tiny file logger for perf diagnosis. Writes timestamped lines to
//! `%APPDATA%/com.foxcull.app/fox-cull.log` (truncated each launch) AND to
//! stderr (the `tauri dev` terminal). Low overhead; only hot paths log.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

static LOGFILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn init(path: PathBuf) {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .ok();
    let _ = LOGFILE.set(Mutex::new(file));
    line(&format!("=== fox-cull session start; log at {} ===", path.display()));
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn line(msg: &str) {
    eprintln!("[fox-cull] {msg}");
    if let Some(m) = LOGFILE.get() {
        if let Some(f) = m.lock().as_mut() {
            let _ = writeln!(f, "{} {}", now_ms(), msg);
            let _ = f.flush();
        }
    }
}
