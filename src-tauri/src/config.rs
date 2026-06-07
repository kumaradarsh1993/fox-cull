//! Tiny Rust-side config (data-root `config.json`) read at startup, BEFORE the
//! frontend's settings store exists. Today it just records where the culling
//! catalog lives, so the user can keep it on their SSD next to the photos and
//! carry it between machines (Alienware ⇄ XPS ⇄ Mac).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    /// Absolute path to the catalog file. `None` = default (in the data root).
    pub catalog_path: Option<String>,
}

fn config_file(data_root: &Path) -> PathBuf {
    data_root.join("config.json")
}

pub fn load(data_root: &Path) -> Config {
    std::fs::read_to_string(config_file(data_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(data_root: &Path, cfg: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(config_file(data_root), s);
    }
}
