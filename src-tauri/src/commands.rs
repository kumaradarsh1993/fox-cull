use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};

use parking_lot::Mutex;
use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::catalog::Catalog;
use crate::media::{self, Kind};
use crate::{thumbs, video};

/// Long edge (px) of grid/filmstrip thumbnails (matches the frontend <Thumb>).
const GRID_MAX: u32 = 320;

/// Long edge (px) of the capped loupe/focus preview. 1920 is sharp full-screen
/// while letting the JPEG decoder pick a 1/2 DCT scale on 12MP phone shots
/// (4000px → 2000px ≥ 1920), so the sharp decode is ~4x cheaper than full-res.
const LOUPE_MAX: u32 = 1920;

/// Threads the background thumbnail warmer may use. Deliberately small and
/// machine-aware: v0.3.0 warmed across ALL cores, and a dozen simultaneous
/// multi-MB reads thrashed the external SSD so badly that individual reads
/// stalled 50+ seconds and starved the photo the user was looking at. We cap at
/// ~half the cores, clamped to 2..=4 — leaving the foreground (loupe + visible
/// cells) plenty of CPU, and keeping the USB-SSD read queue shallow. On the thin
/// XPS 13 (4 cores) this is 2; on the Alienware (12) it's 4.
fn warm_threads() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (cores / 2).clamp(2, 4)
}

/// Dedicated, size-bounded rayon pool for warming (NOT the global pool, which
/// would grab every core).
fn warm_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(warm_threads())
            .build()
            .expect("warm pool")
    })
}

/// Process-wide state: the currently selected library root and the on-disk
/// thumbnail cache directory (in app-data, never on the user's SSD).
pub struct AppState {
    pub root: Mutex<Option<PathBuf>>,
    /// Thumbnail + poster cache directory. Follows the catalog: when the catalog
    /// is relocated onto the SSD, the cache moves next to it (`<catalogDir>/thumbs`)
    /// so posters/thumbs are generated ONCE and reused on every machine that reads
    /// the SSD — never a separate per-computer cache.
    pub cache_dir: Mutex<PathBuf>,
    /// Where app data (config, default catalog, cache) lives — app-data normally,
    /// or a `fox-cull-data` folder next to the exe in portable mode.
    pub data_root: PathBuf,
    /// Current catalog file path (relocatable onto the user's SSD).
    pub catalog_path: Mutex<PathBuf>,
    /// Path to the bundled ffmpeg (next to our exe), or None on a dev build
    /// without it — then video posters fall back to the film placeholder.
    pub ffmpeg: Option<PathBuf>,
    /// Bumped on every folder switch so an in-flight background warming pass for
    /// the previous folder abandons itself instead of fighting for cores.
    pub warm_gen: Arc<AtomicU64>,
}

/// `<catalogDir>/thumbs` — the cache lives beside whatever catalog file is in use.
pub fn cache_dir_for(catalog_path: &Path) -> PathBuf {
    catalog_path
        .parent()
        .map(|d| d.join("thumbs"))
        .unwrap_or_else(|| PathBuf::from("thumbs"))
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
    pub size: u64,  // file size in bytes — for the Details view + size sorting
    pub rating: i64,
    pub label: Option<String>,
    pub flag: Option<String>,
    pub tags: Vec<String>,
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
fn collect(dir: &Path, recursive: bool, out: &mut Vec<(PathBuf, i64, u64)>) {
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
            let dname = entry.file_name().to_string_lossy().to_string();
            // Skip dotfolders and our own folders (the SSD cache + recycle bin),
            // so cached posters/thumbnails and discarded files never appear as
            // photos to cull.
            if !recursive
                || dname.starts_with('.')
                || dname.to_ascii_lowercase().starts_with("_foxcull")
            {
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
                let md = entry.metadata().ok();
                let mtime = md
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
                out.push((path, mtime, size));
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
    // Cancel any in-flight warming for the folder we're leaving, so a rapid
    // folder switch can't leave two warm floods thrashing the disk at once.
    state.warm_gen.fetch_add(1, Ordering::SeqCst);
    let root = state.root.lock().clone();

    let t0 = Instant::now();
    let mut paths: Vec<(PathBuf, i64, u64)> = Vec::new();
    collect(p, recursive, &mut paths);
    let walk_ms = t0.elapsed().as_millis();
    let file_count = paths.len();

    let mut items: Vec<MediaItem> = paths
        .into_iter()
        .map(|(path, mtime, size)| {
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
                size,
                path: abs,
                rating: 0,
                label: None,
                flag: None,
                tags: Vec::new(),
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
    // Attach tags (separate many-to-many table) in one query for the subtree.
    let mut tagmap = catalog.tags_under(&prefix);
    for item in &mut items {
        if let Some(tags) = tagmap.remove(&item.rel) {
            item.tags = tags;
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
    let cache_dir = state.cache_dir.lock().clone();
    // Run the CPU-bound decode/resize on the blocking pool so concurrent
    // thumbnail requests genuinely parallelize across cores instead of
    // serializing on a runtime worker.
    tauri::async_runtime::spawn_blocking(move || {
        thumbs::ensure(&cache_dir, &p, kind, max).map(|o| o.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Source path for the large loupe/focus view. Images and RAW serve a generated,
/// orientation-baked, **capped** preview (long edge <= LOUPE_MAX): RAW because the
/// webview can't render `.NEF`, and ordinary images because handing the webview a
/// 50MP original makes it paint top-down over several seconds — the capped preview
/// decodes via the DCT fast path and appears at once. Videos serve the original.
#[tauri::command]
pub async fn loupe_src(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let kind = media::classify(&p);
    match kind {
        Kind::Raw | Kind::Image => {
            let cache_dir = state.cache_dir.lock().clone();
            tauri::async_runtime::spawn_blocking(move || {
                thumbs::ensure(&cache_dir, &p, kind, LOUPE_MAX)
                    .map(|o| o.to_string_lossy().to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        }
        _ => Ok(path),
    }
}

/// Cached poster frame for a video (grid/strip/loupe). Generated by the bundled
/// ffmpeg and cached beside the catalog (on the SSD), so it's made once and
/// reused across machines. Errors (no ffmpeg, read-only cache on a Mac, an
/// undecodable clip) leave the frontend showing the film placeholder.
#[tauri::command]
pub async fn video_poster(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let cache_dir = state.cache_dir.lock().clone();
    let ffmpeg = state.ffmpeg.clone();
    let src = PathBuf::from(&path);
    tauri::async_runtime::spawn_blocking(move || {
        video::ensure_poster(&cache_dir, ffmpeg.as_deref(), &src)
            .map(|o| o.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Proactively generate (and disk-cache) grid thumbnails for a whole folder in
/// parallel across all cores, so scrolling the grid/filmstrip is smooth instead
/// of decoding lazily under the user's cursor. Fire-and-forget from the frontend
/// right after a folder loads. Cancels itself if the user switches folders (the
/// generation token moved on). Videos/unknowns are skipped.
#[tauri::command]
pub async fn warm_thumbnails(
    state: State<'_, AppState>,
    paths: Vec<String>,
    max: u32,
) -> Result<(), String> {
    let my_gen = state.warm_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let gen = state.warm_gen.clone();
    let cache_dir = state.cache_dir.lock().clone();
    let ffmpeg = state.ffmpeg.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        // Run on the small dedicated pool so we never monopolize the cores the
        // foreground (loupe + visible cells) needs.
        warm_pool().install(|| {
            paths.par_iter().for_each(|path| {
                // Abandon the moment a newer folder selection supersedes us.
                if gen.load(Ordering::SeqCst) != my_gen {
                    return;
                }
                let p = PathBuf::from(path);
                match media::classify(&p) {
                    Kind::Other => {}
                    // Videos: pre-extract a poster frame (cached on the SSD).
                    Kind::Video => {
                        let _ = video::ensure_poster(&cache_dir, ffmpeg.as_deref(), &p);
                    }
                    kind => {
                        let _ = thumbs::ensure(&cache_dir, &p, kind, max);
                    }
                }
            });
        });
    })
    .await;
    Ok(())
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

#[tauri::command]
pub fn add_tag(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    tag: String,
) -> Result<(), String> {
    let tag = tag.trim().to_string();
    if tag.is_empty() {
        return Ok(());
    }
    catalog
        .add_tag_many(&rels_for(&state, &paths), &tag)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_tag(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    tag: String,
) -> Result<(), String> {
    catalog
        .remove_tag_many(&rels_for(&state, &paths), &tag)
        .map_err(|e| e.to_string())
}

/// Distinct tags with usage counts, for the filter UI.
#[tauri::command]
pub fn list_tags(catalog: State<'_, Catalog>) -> Vec<(String, i64)> {
    catalog.all_tags()
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

/// Every cache file fox-cull may have generated for `src` (grid thumb, loupe
/// preview, video poster). Computed while the original still exists so the
/// content-hashed keys resolve, then removed after the file is disposed — so the
/// cache never accumulates orphaned "ghost" thumbnails.
fn cache_files_for(cache_dir: &Path, src: &str) -> Vec<PathBuf> {
    let p = Path::new(src);
    let mut out = Vec::new();
    for max in [GRID_MAX, LOUPE_MAX] {
        if let Some(cp) = thumbs::cache_path(cache_dir, p, max) {
            out.push(cp);
        }
    }
    out.push(video::poster_path(cache_dir, p));
    out
}

/// Name of the auto-created, per-drive reject folder (the "recycle bin alt").
const REJECT_DIRNAME: &str = "_FoxCull Recycle Bin";

/// Volume/drive root of an absolute path: `C:\` on Windows; `/Volumes/<name>`
/// (or `/`) on macOS/Linux. Used to mirror the folder structure from the drive
/// root into the reject folder, exactly as the user described
/// (`C:\…\alpha\beta\gamma\x.jpg` → `C:\_FoxCull Recycle Bin\alpha\beta\gamma\x.jpg`).
fn drive_root(path: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let b = path.as_bytes();
        if b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
            return PathBuf::from(format!("{}:\\", b[0] as char));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(rest) = path.strip_prefix("/Volumes/") {
            if let Some(name) = rest.split('/').next() {
                if !name.is_empty() {
                    return PathBuf::from(format!("/Volumes/{name}"));
                }
            }
        }
    }
    PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" })
}

/// Path of `src` relative to its drive root (with the drive prefix stripped),
/// so it can be re-rooted under the reject folder without collisions.
fn rel_from_drive(src: &str) -> PathBuf {
    let root = drive_root(src);
    Path::new(src)
        .strip_prefix(&root)
        .map(|r| r.to_path_buf())
        .unwrap_or_else(|_| PathBuf::from(Path::new(src).file_name().unwrap_or_default()))
}

/// Move one file into the reject folder, mirroring its path from the drive root
/// so shots from different subfolders never collide. `dest` is the chosen reject
/// folder, or `None` to auto-target `<driveRoot>/_FoxCull Recycle Bin` per file
/// (handles selections spanning multiple drives). Fast rename first; copy+remove
/// across volumes.
fn move_into(dest: &Option<PathBuf>, src: &str) -> Result<(), String> {
    let base = match dest {
        Some(d) => d.clone(),
        None => drive_root(src).join(REJECT_DIRNAME),
    };
    let target = base.join(rel_from_drive(src));
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
    let cache_dir = state.cache_dir.lock().clone();
    // An explicit destination overrides; otherwise "folder" mode auto-targets a
    // per-drive `_FoxCull Recycle Bin` (computed inside move_into).
    let dest_path = dest
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let mut deleted = 0usize;
    let mut failed = Vec::new();
    let mut errors = Vec::new();
    let mut forget = Vec::new();
    for p in &paths {
        // Compute the cache files NOW, while the original still exists (the keys
        // hash its metadata) — we remove them only after a successful dispose.
        let caches = cache_files_for(&cache_dir, p);
        let result = if mode == "folder" {
            move_into(&dest_path, p)
        } else {
            trash::delete(p).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                deleted += 1;
                forget.push(rel_of(&root, p));
                for c in caches {
                    let _ = std::fs::remove_file(c);
                }
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

#[derive(Serialize)]
pub struct CatalogInfo {
    pub path: String,
    pub data_root: String,
    pub is_default: bool,
}

/// Where the catalog currently lives, and whether it's the default location.
#[tauri::command]
pub fn catalog_info(state: State<'_, AppState>) -> CatalogInfo {
    let path = state.catalog_path.lock().clone();
    let default = state.data_root.join("catalog.sqlite");
    CatalogInfo {
        path: path.to_string_lossy().to_string(),
        data_root: state.data_root.to_string_lossy().to_string(),
        is_default: path == default,
    }
}

/// Relocate the catalog into `dir` (e.g. onto the SSD beside the photos). If a
/// `fox-cull.catalog` already exists there, adopt it; otherwise migrate the
/// current one. Returns the new catalog path. Portable-friendly: one file the
/// user can back up or carry between machines.
/// Point the cache dir at `<catalogDir>/thumbs`, create it, and add it to the
/// asset-protocol scope so the webview can load posters/thumbnails from there.
fn relocate_cache(app: &AppHandle, state: &State<'_, AppState>, catalog_path: &Path) {
    let cache = cache_dir_for(catalog_path);
    let _ = std::fs::create_dir_all(&cache);
    let _ = app.asset_protocol_scope().allow_directory(&cache, true);
    *state.cache_dir.lock() = cache;
}

#[tauri::command]
pub fn set_catalog_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    dir: String,
) -> Result<String, String> {
    let dir = PathBuf::from(&dir);
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", dir.display()));
    }
    let new_path = dir.join("fox-cull.catalog");
    let old_path = state.catalog_path.lock().clone();
    if new_path != old_path {
        if !new_path.exists() {
            catalog.checkpoint(); // fold WAL into the main file before copying
            std::fs::copy(&old_path, &new_path).map_err(|e| e.to_string())?;
        }
        catalog.reopen(&new_path).map_err(|e| e.to_string())?;
        *state.catalog_path.lock() = new_path.clone();
        relocate_cache(&app, &state, &new_path);
        let mut cfg = crate::config::load(&state.data_root);
        cfg.catalog_path = Some(new_path.to_string_lossy().to_string());
        crate::config::save(&state.data_root, &cfg);
    }
    Ok(new_path.to_string_lossy().to_string())
}

/// Move the catalog back to the default app-data location.
#[tauri::command]
pub fn reset_catalog_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
) -> Result<String, String> {
    let def = state.data_root.join("catalog.sqlite");
    let old_path = state.catalog_path.lock().clone();
    if def != old_path {
        if !def.exists() {
            catalog.checkpoint();
            std::fs::copy(&old_path, &def).map_err(|e| e.to_string())?;
        }
        catalog.reopen(&def).map_err(|e| e.to_string())?;
        *state.catalog_path.lock() = def.clone();
        relocate_cache(&app, &state, &def);
        let mut cfg = crate::config::load(&state.data_root);
        cfg.catalog_path = None;
        crate::config::save(&state.data_root, &cfg);
    }
    Ok(def.to_string_lossy().to_string())
}

/// Reveal a file in the OS file manager (Explorer / Finder), selected.
#[tauri::command]
pub fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}

/// Open a file in the user's default application (e.g. a system video player for
/// HEVC clips the webview can't decode — the Osmo Pocket 3 footage).
#[tauri::command]
pub fn open_external(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
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
