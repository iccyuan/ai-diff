mod git;
mod watcher;

use std::sync::atomic::{AtomicU32, Ordering};

static WINDOW_SEQ: AtomicU32 = AtomicU32::new(1);

/// extra viewer window so several repos can be reviewed side by side
#[tauri::command]
async fn new_window(app: tauri::AppHandle) -> Result<(), String> {
    let label = format!("repo-{}", WINDOW_SEQ.fetch_add(1, Ordering::Relaxed));
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::default())
        .title("ai-diff")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .visible(false) // the frontend shows it once the theme is applied
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(watcher::WatcherState(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )))
        .invoke_handler(tauri::generate_handler![
            git::open_repo,
            git::get_status,
            git::get_file_diff,
            git::revert_file,
            git::revert_hunk,
            git::revert_all,
            git::list_files,
            git::read_file,
            git::log_commits,
            git::commit_files,
            git::get_commit_file_diff,
            git::auto_open_path,
            git::search_text,
            watcher::watch_repo,
            watcher::unwatch_repo,
            new_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
