mod catalog;
mod commands;
mod log;
mod media;
mod thumbs;

use parking_lot::Mutex;
use tauri::Manager;

use crate::catalog::Catalog;
use crate::commands::AppState;

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
            // App-data dir holds the catalog DB and the thumbnail cache — never
            // the user's SSD, so culling/rating works even on a read-only mount.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let cache_dir = data_dir.join("thumbs");
            std::fs::create_dir_all(&cache_dir)?;

            log::init(data_dir.join("fox-cull.log"));

            let catalog = Catalog::open(&data_dir.join("catalog.sqlite"))
                .map_err(|e| format!("failed to open catalog: {e}"))?;
            app.manage(catalog);

            // Serve cached thumbnails/previews to the webview via the asset
            // protocol. The library root is added later in set_library_root.
            let _ = app.asset_protocol_scope().allow_directory(&cache_dir, true);

            app.manage(AppState {
                root: Mutex::new(None),
                cache_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::set_library_root,
            commands::list_tree,
            commands::list_folder_media,
            commands::thumbnail,
            commands::loupe_src,
            commands::set_rating,
            commands::set_label,
            commands::set_flag,
            commands::set_rating_many,
            commands::set_label_many,
            commands::set_flag_many,
            commands::list_rejected,
            commands::delete_to_trash,
            commands::folder_writable,
            commands::log_event,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("error while running tauri application: {e}");
    }
}
