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
    // Clamped very low: this is I/O-bound, not CPU-bound, and the worst case is a
    // single spinning SATA disk (the user's internal drives), where every extra
    // concurrent original read makes the mechanical head seek-thrash and stalls
    // the foreground thumbnails the user is actually looking at. 1–2 keeps the
    // read queue shallow on an HDD while still using both heads of an SSD.
    (cores / 4).clamp(1, 2)
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
    /// Current catalog file path (lives inside the active drive's library dir).
    pub catalog_path: Mutex<PathBuf>,
    /// The active library folder (`<drive>/_FoxCull`, or an app-data fallback for
    /// a read-only mount). Holds the catalog, the `thumbs` cache and `recycle`.
    pub lib_dir: Mutex<PathBuf>,
    /// Active per-drive recycle folder (`<libDir>/recycle`) — where folder-mode
    /// deletes land so the in-app Trash can list / restore / purge them.
    pub recycle_dir: Mutex<PathBuf>,
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

/// Bundled per-drive library folder name: `<drive>/_FoxCull` holds that drive's
/// catalog, thumbnail cache and recycle bin, so everything for a drive travels
/// with it.
const LIB_DIRNAME: &str = "_FoxCull";

struct Library {
    dir: PathBuf,
    catalog: PathBuf,
    cache: PathBuf,
    recycle: PathBuf,
    on_drive: bool,
}

/// Can we create files directly in `dir`? Decides on-drive vs app-data fallback.
fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".foxcull_write_test.tmp");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Filesystem-safe id for a drive root, for the app-data fallback dir name.
fn drive_id(root: &Path) -> String {
    let id: String = root
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let id = id.trim_matches('_').to_string();
    if id.is_empty() {
        "root".into()
    } else {
        id
    }
}

/// Resolve the library for a drive: on the drive itself (`<drive>/_FoxCull`) when
/// writable, else a per-drive folder under app-data (read-only mounts — e.g. NTFS
/// on a Mac without Paragon — so rating/culling still works there).
fn resolve_library(data_root: &Path, root: &Path) -> Library {
    let (dir, on_drive) = if is_writable(root) {
        (root.join(LIB_DIRNAME), true)
    } else {
        (data_root.join("libraries").join(drive_id(root)), false)
    };
    Library {
        catalog: dir.join("catalog.sqlite"),
        cache: dir.join("thumbs"),
        recycle: dir.join("recycle"),
        dir,
        on_drive,
    }
}

/// Seconds since the Unix epoch.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// If `path` is taken, append " (2)", " (3)", … before the extension — used so a
/// recycle move or a restore never clobbers an existing file.
fn uniquify(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_string());
    let mut n = 2;
    loop {
        let name = match &ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let cand = parent.join(name);
        if !cand.exists() {
            return cand;
        }
        n += 1;
    }
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

#[derive(Serialize)]
pub struct LibraryInfo {
    /// The drive/volume root — the library root that catalog keys are relative to.
    pub root: String,
    /// The active library folder (`<drive>/_FoxCull` or app-data fallback).
    pub dir: String,
    pub catalog: String,
    pub recycle: String,
    /// True when the library lives on the drive itself; false = app-data fallback
    /// (the drive root wasn't writable, e.g. a read-only mount or `C:\`).
    pub on_drive: bool,
    /// Whether the opened drive root is writable (proxy for "can delete here").
    pub writable: bool,
}

/// Activate the library for the drive that `root` lives on, switching the catalog,
/// thumbnail cache and recycle folder to that drive's bundled `_FoxCull` folder
/// (auto per-drive). Idempotent — re-activating the same drive is a no-op. Adds
/// the drive + cache + recycle to the asset-protocol scope so originals and
/// cached previews can be served to the webview.
///
/// Migration is **data-loss-safe**: a fresh per-drive catalog is seeded by COPYING
/// (never moving) the current catalog or adopting a legacy `<drive>/fox-cull.catalog`,
/// and the legacy source is only removed AFTER the new catalog is open — so a
/// disconnect mid-operation can never lose ratings.
#[tauri::command]
pub fn set_library_root(
    app: AppHandle,
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    root: String,
) -> Result<LibraryInfo, String> {
    let p = PathBuf::from(&root);
    if !p.is_dir() {
        return Err(format!("not a directory: {root}"));
    }
    let drive = drive_root(&root);
    let lib = resolve_library(&state.data_root, &drive);

    let current = state.catalog_path.lock().clone();
    if current != lib.catalog {
        std::fs::create_dir_all(&lib.dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&lib.recycle).ok();

        // Seed/adopt a fresh per-drive catalog without ever destroying the source.
        let mut legacy_to_remove: Option<PathBuf> = None;
        if !lib.catalog.exists() {
            let legacy = drive.join("fox-cull.catalog");
            if lib.on_drive && legacy.is_file() {
                catalog.checkpoint(); // fold WAL into the file before copying
                if std::fs::copy(&legacy, &lib.catalog).is_ok() {
                    legacy_to_remove = Some(legacy);
                    // Adopt the legacy `<drive>/thumbs` cache too, if present.
                    let legacy_thumbs = drive.join("thumbs");
                    if legacy_thumbs.is_dir() && !lib.cache.exists() {
                        let _ = std::fs::rename(&legacy_thumbs, &lib.cache);
                    }
                }
            } else if current.is_file() {
                catalog.checkpoint();
                let _ = std::fs::copy(&current, &lib.catalog);
            }
        }
        std::fs::create_dir_all(&lib.cache).ok();

        catalog
            .reopen(&lib.catalog)
            .map_err(|e| format!("failed to open catalog: {e}"))?;
        *state.catalog_path.lock() = lib.catalog.clone();
        *state.cache_dir.lock() = lib.cache.clone();
        *state.lib_dir.lock() = lib.dir.clone();
        *state.recycle_dir.lock() = lib.recycle.clone();
        let _ = app.asset_protocol_scope().allow_directory(&lib.cache, true);
        let _ = app.asset_protocol_scope().allow_directory(&lib.recycle, true);

        // The new catalog is open (old connection closed) → the legacy file is
        // unlocked and safe to remove. Clear any stale config override too.
        if let Some(legacy) = legacy_to_remove {
            let ls = legacy.to_string_lossy().to_string();
            let _ = std::fs::remove_file(&legacy);
            let _ = std::fs::remove_file(format!("{ls}-wal"));
            let _ = std::fs::remove_file(format!("{ls}-shm"));
            let mut cfg = crate::config::load(&state.data_root);
            if cfg.catalog_path.is_some() {
                cfg.catalog_path = None;
                crate::config::save(&state.data_root, &cfg);
            }
        }
    }

    *state.root.lock() = Some(drive.clone());
    let _ = app.asset_protocol_scope().allow_directory(&drive, true);

    Ok(LibraryInfo {
        root: drive.to_string_lossy().to_string(),
        dir: lib.dir.to_string_lossy().to_string(),
        catalog: lib.catalog.to_string_lossy().to_string(),
        recycle: lib.recycle.to_string_lossy().to_string(),
        on_drive: lib.on_drive,
        writable: is_writable(&drive),
    })
}

/// Immediate subdirectories of `dir` (for the lazy folder tree). Dotfolders and
/// our own `_FoxCull` library are hidden.
///
/// `has_children` is reported **optimistically** (always true): probing it
/// eagerly meant an extra `read_dir` PER child, an N+1 stat storm that made
/// expanding a folder on the USB SSD take seconds. We instead show the expand
/// chevron for every folder and let the UI hide it the moment an expand turns up
/// no subfolders — one cheap `read_dir` per expand instead of one per sibling.
#[tauri::command]
pub fn list_tree(dir: String) -> Result<Vec<TreeDir>, String> {
    let p = Path::new(&dir);
    let read = std::fs::read_dir(p).map_err(|e| format!("read_dir failed: {e}"))?;
    let mut out: Vec<TreeDir> = read
        .filter_map(|e| e.ok())
        // file_type() is free on Windows (cached from the enumeration) — no stat.
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.eq_ignore_ascii_case(LIB_DIRNAME) {
                return None;
            }
            Some(TreeDir {
                name,
                path: e.path().to_string_lossy().to_string(),
                has_children: true,
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

/// Recursively count media files under `dir` (extension classification only — no
/// metadata reads), skipping dotfolders, our own `_FoxCull` library and Windows
/// reparse points, exactly like `collect`. Powers the left-pane folder badges.
fn count_media(dir: &Path) -> usize {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return 0,
    };
    let mut n = 0usize;
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(f) => f,
            Err(_) => continue,
        };
        if ft.is_dir() {
            let dname = entry.file_name().to_string_lossy().to_string();
            if dname.starts_with('.') || dname.to_ascii_lowercase().starts_with("_foxcull") {
                continue;
            }
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
            n += count_media(&entry.path());
        } else if ft.is_file() && media::is_media(&entry.path()) {
            n += 1;
        }
    }
    n
}

#[derive(Serialize)]
pub struct FolderCount {
    pub path: String,
    pub count: i64,
}

/// Recursive media counts for a set of folders (the left-pane badges). Returns
/// cached counts instantly; missing ones are computed in parallel on the bounded
/// warm pool and cached, so the FIRST expand of a folder fills its children's
/// badges a moment later and every later run is instant. `recompute` ignores the
/// cache (the manual "↻ recount" path). Keyed by absolute path — a regenerable
/// local cache, never auto-invalidated, so stale counts only change on refresh.
#[tauri::command]
pub async fn folder_counts(
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    recompute: bool,
) -> Result<Vec<FolderCount>, String> {
    let cached = if recompute {
        HashMap::new()
    } else {
        catalog.get_counts()
    };
    let mut out: Vec<FolderCount> = Vec::with_capacity(paths.len());
    let mut need: Vec<String> = Vec::new();
    for p in paths {
        match cached.get(&p) {
            Some(c) => out.push(FolderCount { path: p, count: *c }),
            None => need.push(p),
        }
    }
    if !need.is_empty() {
        let computed: Vec<(String, i64)> = tauri::async_runtime::spawn_blocking(move || {
            warm_pool().install(|| {
                need.par_iter()
                    .map(|abs| (abs.clone(), count_media(Path::new(abs)) as i64))
                    .collect()
            })
        })
        .await
        .map_err(|e| e.to_string())?;
        let _ = catalog.set_counts(&computed);
        for (path, count) in computed {
            out.push(FolderCount { path, count });
        }
    }
    Ok(out)
}

/// Drop every cached folder count so the badges recompute (the tree's ↻ button).
#[tauri::command]
pub fn clear_folder_counts(catalog: State<'_, Catalog>) {
    catalog.clear_counts();
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

#[derive(Serialize)]
pub struct FilmstripInfo {
    /// Filesystem path of the sprite JPEG (frontend converts via convertFileSrc).
    pub src: String,
    pub cols: u32,
    pub rows: u32,
    pub count: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub duration: f64,
}

/// Build (or fetch the cached) filmstrip sprite for a video — a tiled grid of
/// frames the loupe shows under the scrub cursor for instant, decode-free
/// scrubbing. Generated lazily on first open; cached beside the poster on the
/// SSD. Errors (no ffmpeg, unreadable duration) leave the timeline as a plain
/// seek bar with no hover preview.
#[tauri::command]
pub async fn video_filmstrip(
    state: State<'_, AppState>,
    path: String,
) -> Result<FilmstripInfo, String> {
    let cache_dir = state.cache_dir.lock().clone();
    let ffmpeg = state.ffmpeg.clone();
    let src = PathBuf::from(&path);
    tauri::async_runtime::spawn_blocking(move || {
        video::ensure_filmstrip(&cache_dir, ffmpeg.as_deref(), &src).map(|(sprite, fs)| {
            FilmstripInfo {
                src: sprite.to_string_lossy().to_string(),
                cols: fs.cols,
                rows: fs.rows,
                count: fs.count,
                tile_w: fs.tile_w,
                tile_h: fs.tile_h,
                duration: fs.duration,
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// How many items the background warmer pre-generates per folder. Bounded so a
/// huge folder can't keep the USB SSD's read queue saturated for a minute — past
/// this the viewport-prioritized on-demand loader handles whatever you scroll to.
const WARM_CAP: usize = 600;

/// Proactively generate (and disk-cache) grid thumbnails for the FIRST part of a
/// folder, so initial scrolling is smooth instead of decoding lazily under the
/// cursor. Fire-and-forget from the frontend right after a folder loads; cancels
/// itself when the user switches folders (the generation token moved on).
///
/// CRITICAL: this only warms ordinary images, and only the first `WARM_CAP` of
/// them. Videos (ffmpeg poster extraction) and RAW (whole-file ~25 MB reads) are
/// deliberately LEFT to on-demand loading — pre-reading a whole folder of those
/// pinned the USB SSD's serial command queue for ~a minute and starved the
/// foreground thumbnails you were actually looking at (the "not responding that
/// recovers if you wait" bug). On-demand stays bounded to the viewport, so heavy
/// reads only happen for the handful of RAW/video cells actually on screen.
#[tauri::command]
pub async fn warm_thumbnails(
    state: State<'_, AppState>,
    paths: Vec<String>,
    max: u32,
) -> Result<(), String> {
    let my_gen = state.warm_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let gen = state.warm_gen.clone();
    let cache_dir = state.cache_dir.lock().clone();
    // Only the cheap kind (ordinary images), only the first WARM_CAP.
    let work: Vec<PathBuf> = paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| matches!(media::classify(p), Kind::Image))
        .take(WARM_CAP)
        .collect();
    let total = work.len();
    let t0 = Instant::now();
    crate::log::line(&format!(
        "WARM start images={} (of {} files) max={} gen={}",
        total,
        paths.len(),
        max,
        my_gen
    ));
    let done = tauri::async_runtime::spawn_blocking(move || {
        let count = std::sync::atomic::AtomicUsize::new(0);
        // Run on the small dedicated pool so we never monopolize the cores the
        // foreground (loupe + visible cells) needs.
        warm_pool().install(|| {
            work.par_iter().for_each(|p| {
                // Abandon the moment a newer folder selection supersedes us.
                if gen.load(Ordering::SeqCst) != my_gen {
                    return;
                }
                if thumbs::ensure(&cache_dir, p, Kind::Image, max).is_ok() {
                    count.fetch_add(1, Ordering::Relaxed);
                }
            });
        });
        count.load(Ordering::Relaxed)
    })
    .await
    .unwrap_or(0);
    crate::log::line(&format!(
        "WARM done warmed={}/{} elapsed={}ms gen={}{}",
        done,
        total,
        t0.elapsed().as_millis(),
        my_gen,
        if state.warm_gen.load(Ordering::SeqCst) != my_gen {
            " (cancelled)"
        } else {
            ""
        }
    ));
    Ok(())
}

/// Abandon any in-flight background warming (bump the generation token). Called
/// when the user enters Focus / starts a video, so previews and video playback
/// get the USB SSD's read bandwidth to themselves instead of stuttering behind
/// the warmer.
#[tauri::command]
pub fn cancel_warm(state: State<'_, AppState>) {
    state.warm_gen.fetch_add(1, Ordering::SeqCst);
}

#[derive(Serialize)]
pub struct CaptureDate {
    pub path: String,
    pub captured: i64,
}

/// Real capture timestamps for a set of files — EXIF DateTimeOriginal for
/// images/RAW, container `creation_time` for video, falling back to the file
/// mtime when neither is present. Results are cached in the catalog (validated
/// by mtime+size), so the FIRST call on a folder does the extraction (kept OFF
/// the folder-open path, which never reads EXIF) and every later call returns
/// instantly. Extraction runs on the bounded warm pool so it never floods the
/// USB SSD or starves the foreground.
#[tauri::command]
pub async fn capture_dates(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    dir: String,
    paths: Vec<String>,
) -> Result<Vec<CaptureDate>, String> {
    let root = state.root.lock().clone();
    let prefix = rel_of(&root, &dir);
    let cached = catalog.captures_under(&prefix);
    let ffmpeg = state.ffmpeg.clone();

    struct Pending {
        path: String,
        rel: String,
        mtime: i64,
        size: i64,
        kind: Kind,
    }
    let mut results: Vec<CaptureDate> = Vec::with_capacity(paths.len());
    let mut pending: Vec<Pending> = Vec::new();
    for path in &paths {
        let p = Path::new(path);
        let rel = rel_of(&root, path);
        let (mtime, size) = match std::fs::metadata(p) {
            Ok(m) => (
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                m.len() as i64,
            ),
            Err(_) => (0, 0),
        };
        if let Some(&(captured, cm, cs)) = cached.get(&rel) {
            if cm == mtime && cs == size {
                results.push(CaptureDate {
                    path: path.clone(),
                    captured,
                });
                continue;
            }
        }
        pending.push(Pending {
            path: path.clone(),
            rel,
            mtime,
            size,
            kind: media::classify(p),
        });
    }

    if !pending.is_empty() {
        let extracted: Vec<(CaptureDate, (String, i64, i64, i64))> =
            tauri::async_runtime::spawn_blocking(move || {
                warm_pool().install(|| {
                    pending
                        .par_iter()
                        .map(|pd| {
                            let p = Path::new(&pd.path);
                            let captured = match pd.kind {
                                Kind::Image | Kind::Raw => media::capture_date(p),
                                Kind::Video => ffmpeg
                                    .as_deref()
                                    .and_then(|ff| video::creation_time(ff, p)),
                                Kind::Other => None,
                            }
                            .unwrap_or(pd.mtime);
                            (
                                CaptureDate {
                                    path: pd.path.clone(),
                                    captured,
                                },
                                (pd.rel.clone(), captured, pd.mtime, pd.size),
                            )
                        })
                        .collect()
                })
            })
            .await
            .map_err(|e| e.to_string())?;

        let rows: Vec<(String, i64, i64, i64)> =
            extracted.iter().map(|(_, r)| r.clone()).collect();
        let _ = catalog.set_capture_many(&rows);
        for (cd, _) in extracted {
            results.push(cd);
        }
    }

    Ok(results)
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

// ── video trim (in/out points + lossless cut) ───────────────────────────────

#[tauri::command]
pub fn get_trim(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    path: String,
) -> Option<(f64, f64)> {
    let rel = rel_of(&state.root.lock().clone(), &path);
    catalog.get_trim(&rel)
}

#[tauri::command]
pub fn set_trim(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    path: String,
    in_s: f64,
    out_s: f64,
) -> Result<(), String> {
    let rel = rel_of(&state.root.lock().clone(), &path);
    catalog.set_trim(&rel, in_s, out_s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_trim(state: State<'_, AppState>, catalog: State<'_, Catalog>, path: String) {
    let rel = rel_of(&state.root.lock().clone(), &path);
    catalog.clear_trim(&rel);
}

/// Export a lossless cut of `path` between in/out (seconds) next to the original
/// as `<name>_cut.<ext>` (uniquified). No re-encode. Returns the new file path.
#[tauri::command]
pub async fn trim_video(
    state: State<'_, AppState>,
    path: String,
    in_s: f64,
    out_s: f64,
) -> Result<String, String> {
    let ffmpeg = state.ffmpeg.clone().ok_or("ffmpeg not available")?;
    let src = PathBuf::from(&path);
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "clip".into());
    let ext = src
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".into());
    // Pick a non-colliding destination in the same folder.
    let mut dest = src.with_file_name(format!("{stem}_cut.{ext}"));
    let mut n = 2;
    while dest.exists() {
        dest = src.with_file_name(format!("{stem}_cut{n}.{ext}"));
        n += 1;
    }
    tauri::async_runtime::spawn_blocking(move || {
        video::trim(&ffmpeg, &src, in_s, out_s, &dest)
            .map(|_| dest.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
    // Filmstrip sprite + its geometry sidecar, so a deleted clip leaves no orphan.
    let strip = video::filmstrip_path(cache_dir, p);
    out.push(strip.with_extension("json"));
    out.push(strip);
    out
}

/// Volume/drive root of an absolute path: `C:\` on Windows; `/Volumes/<name>`
/// (or `/`) on macOS/Linux. Also the library root for catalog keys, and the base
/// the recycle folder mirrors structure from
/// (`C:\…\alpha\beta\x.jpg` → `<recycle>\alpha\beta\x.jpg`).
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

/// Move one file into the per-drive recycle folder, mirroring its path from the
/// drive root so shots from different subfolders never collide. Returns
/// `(stored, orig)` — the path within the recycle dir, and the original
/// drive-relative path (for Restore). Fast rename first; copy+remove across
/// volumes. Never clobbers an existing file (uniquifies).
fn move_into_recycle(recycle: &Path, src: &str) -> Result<(String, String), String> {
    let rel = rel_from_drive(src);
    let orig = rel.to_string_lossy().replace('\\', "/");
    let target = uniquify(recycle.join(&rel));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if std::fs::rename(src, &target).is_err() {
        std::fs::copy(src, &target).map_err(|e| e.to_string())?;
        std::fs::remove_file(src).map_err(|e| e.to_string())?;
    }
    let stored = target
        .strip_prefix(recycle)
        .unwrap_or(&rel)
        .to_string_lossy()
        .replace('\\', "/");
    Ok((stored, orig))
}

/// Dispose of rejected files. `mode` = "recycle" (OS Recycle Bin / Trash) or
/// "folder" (move into the active drive's `_FoxCull/recycle`, tracked by the
/// in-app Trash so it can be previewed, restored or purged). Drops catalog
/// decision rows for disposed files; records folder-mode deletes in `trash`.
#[tauri::command]
pub fn dispose_rejected(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    paths: Vec<String>,
    mode: String,
) -> TrashOutcome {
    let root = state.root.lock().clone();
    let cache_dir = state.cache_dir.lock().clone();
    let recycle = state.recycle_dir.lock().clone();
    let folder = mode == "folder";
    let at = now();
    let mut deleted = 0usize;
    let mut failed = Vec::new();
    let mut errors = Vec::new();
    let mut forget = Vec::new();
    let mut trash_rows: Vec<(String, String, String, i64)> = Vec::new();
    for p in &paths {
        // Compute the cache files NOW, while the original still exists (the keys
        // hash its metadata) — we remove them only after a successful dispose.
        let caches = cache_files_for(&cache_dir, p);
        let result: Result<Option<(String, String)>, String> = if folder {
            move_into_recycle(&recycle, p).map(Some)
        } else {
            trash::delete(p).map(|_| None).map_err(|e| e.to_string())
        };
        match result {
            Ok(stored) => {
                deleted += 1;
                forget.push(rel_of(&root, p));
                for c in caches {
                    let _ = std::fs::remove_file(c);
                }
                if let Some((stored, orig)) = stored {
                    let name = Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    trash_rows.push((stored, orig, name, at));
                }
            }
            Err(e) => {
                failed.push(p.clone());
                errors.push(e);
            }
        }
    }
    catalog.forget(&forget);
    if !trash_rows.is_empty() {
        let _ = catalog.add_trash_many(&trash_rows);
    }
    TrashOutcome {
        deleted,
        failed,
        errors,
    }
}

// ── in-app Trash (per-drive recycle folder) ─────────────────────────────────

#[derive(Serialize)]
pub struct TrashItem {
    pub stored: String,
    pub orig: String,
    /// Absolute path of the file in the recycle dir (for the thumbnail/preview).
    pub path: String,
    pub name: String,
    pub kind: String,
    pub ext: String,
    pub deleted_at: i64,
}

/// Everything in the active drive's Trash, most recently rejected first. Prunes
/// rows whose file has vanished (e.g. emptied outside the app).
#[tauri::command]
pub fn list_trash(state: State<'_, AppState>, catalog: State<'_, Catalog>) -> Vec<TrashItem> {
    let recycle = state.recycle_dir.lock().clone();
    let mut stale: Vec<String> = Vec::new();
    let items: Vec<TrashItem> = catalog
        .list_trash()
        .into_iter()
        .filter_map(|r| {
            let path = recycle.join(&r.stored);
            if !path.exists() {
                stale.push(r.stored);
                return None;
            }
            Some(TrashItem {
                kind: media::classify(&path).as_str().to_string(),
                ext: media::ext_lower(&path),
                path: path.to_string_lossy().to_string(),
                stored: r.stored,
                orig: r.orig,
                name: r.name,
                deleted_at: r.deleted_at,
            })
        })
        .collect();
    if !stale.is_empty() {
        catalog.remove_trash(&stale);
    }
    items
}

#[derive(Serialize)]
pub struct RestoreOutcome {
    pub restored: usize,
    pub failed: Vec<String>,
}

/// Move trashed files back to their original location on the drive. Uniquifies
/// if something now occupies the original path. Removes restored trash rows.
#[tauri::command]
pub fn restore_trash(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    stored: Vec<String>,
) -> RestoreOutcome {
    let recycle = state.recycle_dir.lock().clone();
    let drive = match state.root.lock().clone() {
        Some(r) => r,
        None => {
            return RestoreOutcome {
                restored: 0,
                failed: stored,
            }
        }
    };
    let orig_of: HashMap<String, String> = catalog
        .list_trash()
        .into_iter()
        .map(|r| (r.stored, r.orig))
        .collect();
    let mut restored = 0usize;
    let mut failed = Vec::new();
    let mut done = Vec::new();
    for s in &stored {
        let from = recycle.join(s);
        let orig = orig_of.get(s).cloned().unwrap_or_else(|| s.clone());
        let to = uniquify(drive.join(&orig));
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let ok = std::fs::rename(&from, &to).is_ok()
            || (std::fs::copy(&from, &to).is_ok() && std::fs::remove_file(&from).is_ok());
        if ok {
            restored += 1;
            done.push(s.clone());
        } else {
            failed.push(s.clone());
        }
    }
    catalog.remove_trash(&done);
    RestoreOutcome { restored, failed }
}

/// Permanently delete trashed files (and their cached thumbs/posters). Returns
/// the number removed.
#[tauri::command]
pub fn purge_trash(
    state: State<'_, AppState>,
    catalog: State<'_, Catalog>,
    stored: Vec<String>,
) -> usize {
    let recycle = state.recycle_dir.lock().clone();
    let cache_dir = state.cache_dir.lock().clone();
    let mut n = 0usize;
    let mut done = Vec::new();
    for s in &stored {
        let p = recycle.join(s);
        let caches = cache_files_for(&cache_dir, &p.to_string_lossy());
        if std::fs::remove_file(&p).is_ok() || !p.exists() {
            n += 1;
            done.push(s.clone());
            for c in caches {
                let _ = std::fs::remove_file(c);
            }
        }
    }
    catalog.remove_trash(&done);
    n
}

/// Where the active library lives (catalog + cache + recycle), and whether it's
/// on the drive or an app-data fallback.
#[tauri::command]
pub fn library_info(state: State<'_, AppState>) -> LibraryInfo {
    let dir = state.lib_dir.lock().clone();
    let catalog = state.catalog_path.lock().clone();
    let recycle = state.recycle_dir.lock().clone();
    let root = state.root.lock().clone();
    let writable = root.as_ref().map(|r| is_writable(r)).unwrap_or(false);
    let on_drive = !dir.starts_with(&state.data_root);
    LibraryInfo {
        root: root
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_default(),
        dir: dir.to_string_lossy().to_string(),
        catalog: catalog.to_string_lossy().to_string(),
        recycle: recycle.to_string_lossy().to_string(),
        on_drive,
        writable,
    }
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
