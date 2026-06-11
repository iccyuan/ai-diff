mod git;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            git::open_repo,
            git::get_status,
            git::get_file_diff,
            git::revert_file,
            git::revert_hunk,
            git::revert_all
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
