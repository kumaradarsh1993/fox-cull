use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::catalog::Catalog;
use crate::media::{self, Kind};
use crate::thumbs;

/// Process-wide state: the currently selected library root and the on-disk
/// thumbnail cache directory (in app-data, never on the user's SSD).
pub struct AppState {
    pub root: Mutex<Option<PathBuf>>,
    pub cache_dir: PathBuf,
}

#[derive(Serialize)]
pub struct TreeDir {
    pub name: String,
    pub path: String,
    pub has_children: bool,
}

#[derive(Serialize)]
pub struct MediaItem {
    pub name: String,
    pub path: String,
    pub rel: String,
    pub kind: String,
    pub ext: String,
    pub mtime: i64, // file modified time (epoch secs) — for date sorting
    pub rating: i64,
    pub label: Option<String>,
    pub flag: Option<String>,
}

#[derive(Serialize)]
pub struct TrashOutcome {
    pub deleted: usize,
    pub failed: Vec<String>,
    pub errors: Vec<String>,
}

/// Path relative to the active library root, using `/` separators so the same
/// catalog key is produced on Windows and macOS.
fn rel_of(root: &Option<PathBuf>, abs: &str) -> String {
    if let Some(r) = root {
        if let Ok(rel) = Path::new(abs).strip_prefix(r) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    abs.replace('\\', "/")
}

/// Set the library root. Adds it (recursively) to the asset-protocol scope so
/// originals under it can be served to the webview for the loupe / video player.
#[tauri::command]
pub fn set_library_root(app: AppHandle, state: State<'_, AppState>, root: String) -> Result<(), String> {
    let p = PathBuf::from(&root);
    if !p.is_dir() {
        return Err(format!("not a directory: {root}"));
    }
    let _ = app.asset_protocol_scope().allow_directory(&p, true);
    *state.root.lock() = Some(p);
    Ok(())
}

/// Immediate subdirectories of `dir` (for the lazy folder tree). Dotfolders are
/// hidden; each entry reports whether it has subfolders so the UI can show an
/// expand affordance without eagerly walking the whole tree.
#[tauri::command]
pub fn list_tree(dir: String) -> Result<Vec<TreeDir>, String> {
    let p = Path::new(&dir);
    let read = std::fs::read_dir(p).map_err(|e| format!("read_dir failed: {e}"))?;
    let mut out: Vec<TreeDir> = read
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let path = e.path();
            let has_children = std::fs::read_dir(&path)
                .map(|mut rd| rd.any(|c| c.map(|c| c.path().is_dir()).unwrap_or(false)))
                .unwrap_or(false);
            Some(TreeDir {
                name,
                path: path.to_string_lossy().to_string(),
                has_children,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Recursively gather media file paths under `dir`. Uses `file_type()` (free on
/// Windows, no extra stat) and does NOT follow symlinks, so symlink loops can't
/// hang the walk. Hidden folders (dotfolders) are skipped.
fn collect(dir: &Path, recursive: bool, out: &mut Vec<(PathBuf, i64)>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(f) => f,
            Err(_) => continue,
        };
        if ft.is_dir() {
            if !recursive || entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // Skip Windows junctions / reparse points so browsing a whole drive
            // (e.g. C:\) can't loop forever or re-scan the same data.
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const REPARSE: u32 = 0x400;
                if let Ok(md) = entry.metadata() {
                    if md.file_attributes() & REPARSE != 0 {
                        continue;
                    }
                }
            }
            collect(&entry.path(), true, out);
        } else if ft.is_file() {
            let path = entry.path();
            if media::is_media(&path) {
                // metadata() is cached from the dir enumeration on Windows (free).
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                out.push((path, mtime));
            }
        }
    }
}

/// Top-level browse roots: drive letters on Windows, mounted volumes + home on
/// macOS/Linux. Lets the left pane map the whole machine, not just one folder.
#[tauri::command]
pub fn list_drives() -> Vec<TreeDir> {
    let mut out: Vec<TreeDir> = Vec::new();
    #[cfg(windows)]
    {
        for c in b'A'..=b'Z' {
            let p = format!("{}:\\", c as char);
            if Path::new(&p).is_dir() {
                out.push(TreeDir {
                    name: format!("{}:\\", c as char),
                    path: p,
                    has_children: true,
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            out.push(TreeDir {
                name: "Home".into(),
                path: home,
                has_children: true,
            });
        }
        out.push(TreeDir {
            name: "/".into(),
            path: "/".into(),
            has_children: true,
        });
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    out.push(TreeDir {
                        name: e.file_name().to_string_lossy().to_string(),
                        path: e.path().to_string_lossy().to_string(),
                        has_children: true,
                    });
                }
            }
        }
    }
    out
}

/// All media under `dir` (optionally recursing into subfolders, Lightroom-style),
/// with stored culling decisions joined in via a single catalog query. Folders
/// are excluded — the tree handles navigation. This is the import path; it only
/// enumerates paths (no decode), so even a 10k-image year folder returns quickly.
#[tauri::command]
pub fn list_folder_media(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    dir: String,
    recursive: bool,
) -> Result<Vec<MediaItem>, String> {
    let p = Path::new(&dir);
    if !p.is_dir() {
        return Err(format!("not a directory: {dir}"));
    }
    let root = state.root.lock().clone();

    let t0 = Instant::now();
    let mut paths: Vec<(PathBuf, i64)> = Vec::new();
    collect(p, recursive, &mut paths);
    let walk_ms = t0.elapsed().as_millis();
    let file_count = paths.len();

    let mut items: Vec<MediaItem> = paths
        .into_iter()
        .map(|(path, mtime)| {
            let abs = path.to_string_lossy().to_string();
            MediaItem {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                rel: rel_of(&root, &abs),
                kind: media::classify(&path).as_str().to_string(),
                ext: media::ext_lower(&path),
                mtime,
                path: abs,
                rating: 0,
                label: None,
                flag: None,
            }
        })
        .collect();

    // Sort by full path (groups each subfolder's shots together, ordered within).
    items.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));

    // One query for the whole subtree, then attach.
    let prefix = rel_of(&root, &dir);
    let decisions = catalog.get_under(&prefix);
    let map: HashMap<&str, _> = decisions.iter().map(|d| (d.rel.as_str(), d)).collect();
    for item in &mut items {
        if let Some(d) = map.get(item.rel.as_str()) {
            item.rating = d.rating;
            item.label = d.label.clone();
            item.flag = d.flag.clone();
        }
    }
    crate::log::line(&format!(
        "SCAN dir={:?} recursive={} files={} walk={}ms total={}ms",
        Path::new(&dir).file_name().unwrap_or_default(),
        recursive,
        file_count,
        walk_ms,
        t0.elapsed().as_millis()
    ));
    Ok(items)
}

/// Frontend-side timing/diagnostic events, funneled into the same logfile.
#[tauri::command]
pub fn log_event(msg: String) {
    crate::log::line(&format!("UI {msg}"));
}

/// Cached, orientation-corrected thumbnail for the grid/filmstrip. Returns a
/// filesystem path the frontend converts via `convertFileSrc`.
#[tauri::command]
pub async fn thumbnail(
    state: State<'_, AppState>,
    path: String,
    max: u32,
) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let kind = media::classify(&p);
    // Never decode videos/unknowns for a thumbnail (poster frames are phase 2);
    // the frontend renders a placeholder instead and shouldn't even call this.
    if matches!(kind, Kind::Video | Kind::Other) {
        return Err("no thumbnail for this kind".into());
    }
    let cache_dir = state.cache_dir.clone();
    // Run the CPU-bound decode/resize on the blocking pool so concurrent
    // thumbnail requests genuinely parallelize across cores instead of
    // serializing on a runtime worker.
    tauri::async_runtime::spawn_blocking(move || {
        thumbs::ensure(&cache_dir, &p, kind, max).map(|o| o.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Source path for the large loupe view. Images/videos serve the original
/// (the webview honors EXIF orientation for `<img>`); RAW serves a generated,
/// orientation-baked full-res preview since the webview can't render `.NEF`.
#[tauri::command]
pub async fn loupe_src(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    match media::classify(&p) {
        Kind::Raw => {
            let cache_dir = state.cache_dir.clone();
            tauri::async_runtime::spawn_blocking(move || {
                thumbs::ensure(&cache_dir, &p, Kind::Raw, 2560)
                    .map(|o| o.to_string_lossy().to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        }
        _ => Ok(path),
    }
}

#[tauri::command]
pub fn set_rating(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    path: String,
    rating: i64,
) -> Result<(), String> {
    let rel = rel_of(&state.root.lock().clone(), &path);
    catalog.set_rating(&rel, rating).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_label(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    path: String,
    label: Option<String>,
) -> Result<(), String> {
    let rel = rel_of(&state.root.lock().clone(), &path);
    catalog.set_label(&rel, label).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_flag(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    path: String,
    flag: Option<String>,
) -> Result<(), String> {
    let rel = rel_of(&state.root.lock().clone(), &path);
    catalog.set_flag(&rel, flag).map_err(|e| e.to_string())
}

fn rels_for(state: &State<'_, AppState>, paths: &[String]) -> Vec<String> {
    let root = state.root.lock().clone();
    paths.iter().map(|p| rel_of(&root, p)).collect()
}

#[tauri::command]
pub fn set_rating_many(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    rating: i64,
) -> Result<(), String> {
    catalog
        .set_rating_many(&rels_for(&state, &paths), rating)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_label_many(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    label: Option<String>,
) -> Result<(), String> {
    catalog
        .set_label_many(&rels_for(&state, &paths), label)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_flag_many(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    flag: Option<String>,
) -> Result<(), String> {
    catalog
        .set_flag_many(&rels_for(&state, &paths), flag)
        .map_err(|e| e.to_string())
}

/// Absolute paths of every file currently flagged `reject`, across the whole
/// catalog — the input to the delete sweep.
#[tauri::command]
pub fn list_rejected(state: State<'_, AppState>, catalog: State<'_, Catalog>) -> Vec<String> {
    let root = state.root.lock().clone();
    catalog
        .list_by_flag("reject")
        .into_iter()
        .map(|rel| match &root {
            Some(r) => r.join(&rel).to_string_lossy().to_string(),
            None => rel,
        })
        .collect()
}

/// Move one file into `dest`, preserving its path relative to the library root
/// so files from different subfolders don't collide. Tries a fast rename first,
/// falls back to copy+remove across volumes.
fn move_into(root: &Option<PathBuf>, dest: &Path, src: &str) -> Result<(), String> {
    let rel = rel_of(root, src);
    let target = dest.join(&rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if std::fs::rename(src, &target).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, &target).map_err(|e| e.to_string())?;
    std::fs::remove_file(src).map_err(|e| e.to_string())?;
    Ok(())
}

/// Dispose of rejected files. `mode` = "recycle" (OS Recycle Bin / Trash,
/// recoverable) or "folder" (move into `dest`, preserving structure — handy for
/// a reversible "review later" sweep). Drops catalog rows for moved files.
#[tauri::command]
pub fn dispose_rejected(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    mode: String,
    dest: Option<String>,
) -> TrashOutcome {
    let root = state.root.lock().clone();
    let dest_path = dest.as_deref().map(PathBuf::from);
    let mut deleted = 0usize;
    let mut failed = Vec::new();
    let mut errors = Vec::new();
    let mut forget = Vec::new();
    for p in &paths {
        let result = if mode == "folder" {
            match &dest_path {
                Some(d) => move_into(&root, d, p),
                None => Err("no destination folder set".into()),
            }
        } else {
            trash::delete(p).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                deleted += 1;
                forget.push(rel_of(&root, p));
            }
            Err(e) => {
                failed.push(p.clone());
                errors.push(e);
            }
        }
    }
    catalog.forget(&forget);
    TrashOutcome {
        deleted,
        failed,
        errors,
    }
}

/// Whether `dir` is writable — used to detect a read-only mount (e.g. NTFS on
/// macOS) so the UI can disable the delete sweep with an explanation.
#[tauri::command]
pub fn folder_writable(dir: String) -> bool {
    let probe = Path::new(&dir).join(".foxcull_write_test.tmp");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}
