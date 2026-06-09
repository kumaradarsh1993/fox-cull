mod catalog;
mod commands;
mod config;
mod log;
mod media;
mod thumbs;
mod video;

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::Manager;

use crate::catalog::Catalog;
use crate::commands::AppState;

/// Resolve the app-data root. If a folder named `fox-cull-data` sits next to the
/// executable, run **portable** — keep the catalog, cache and config there so the
/// whole app + its data can live on a USB stick / SSD. Otherwise use the OS
/// app-data dir.
fn resolve_data_root(app: &tauri::App) -> std::io::Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let portable = dir.join("fox-cull-data");
            if portable.is_dir() {
                return Ok(portable);
            }
        }
    }
    let p = app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    Ok(p)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Keep a single fox-cull window per machine; focus the existing one if the
    // user launches again.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    }

    let result = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            // Data root holds the catalog DB, thumbnail cache and config — in
            // app-data normally (never the user's SSD, so rating works on a
            // read-only mount), or beside the exe in portable mode.
            let data_dir = resolve_data_root(app)?;
            std::fs::create_dir_all(&data_dir)?;

            log::init(data_dir.join("fox-cull.log"));

            // Startup catalog: honor a legacy relocated catalog if it still
            // EXISTS (used as the migration seed), else the app-data default.
            // The real per-drive library is activated on the first folder open
            // (set_library_root), which swaps the catalog/cache/recycle.
            let cfg = config::load(&data_dir);
            let default_catalog = data_dir.join("catalog.sqlite");
            let catalog_path = cfg
                .catalog_path
                .as_ref()
                .map(PathBuf::from)
                .filter(|p| p.is_file())
                .unwrap_or_else(|| default_catalog.clone());

            // Cache + recycle live beside the catalog (a `_FoxCull` library
            // folder once a drive is active; app-data before that).
            let cache_dir = commands::cache_dir_for(&catalog_path);
            std::fs::create_dir_all(&cache_dir)?;
            let lib_dir = catalog_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| data_dir.clone());
            let recycle_dir = lib_dir.join("recycle");

            let catalog = Catalog::open(&catalog_path)
                .map_err(|e| format!("failed to open catalog: {e}"))?;
            app.manage(catalog);

            // Serve cached thumbnails/previews to the webview via the asset
            // protocol. The library root is added later in set_library_root.
            let _ = app.asset_protocol_scope().allow_directory(&cache_dir, true);

            app.manage(AppState {
                root: Mutex::new(None),
                cache_dir: Mutex::new(cache_dir),
                data_root: data_dir,
                catalog_path: Mutex::new(catalog_path),
                lib_dir: Mutex::new(lib_dir),
                recycle_dir: Mutex::new(recycle_dir),
                ffmpeg: video::ffmpeg_path(),
                warm_gen: Arc::new(AtomicU64::new(0)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::set_library_root,
            commands::list_drives,
            commands::list_tree,
            commands::list_folder_media,
            commands::thumbnail,
            commands::warm_thumbnails,
            commands::loupe_src,
            commands::video_poster,
            commands::video_filmstrip,
            commands::capture_dates,
            commands::set_rating,
            commands::set_label,
            commands::set_flag,
            commands::set_rating_many,
            commands::set_label_many,
            commands::set_flag_many,
            commands::add_tag,
            commands::remove_tag,
            commands::list_tags,
            commands::get_trim,
            commands::set_trim,
            commands::clear_trim,
            commands::trim_video,
            commands::list_rejected,
            commands::dispose_rejected,
            commands::list_trash,
            commands::restore_trash,
            commands::purge_trash,
            commands::library_info,
            commands::reveal,
            commands::open_external,
            commands::folder_writable,
            commands::log_event,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("error while running tauri application: {e}");
    }
}
