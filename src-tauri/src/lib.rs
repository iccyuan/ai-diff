mod git;
mod watcher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(watcher::WatcherState(std::sync::Mutex::new(None)))
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
            watcher::watch_repo
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
