mod git;
mod watcher;

use std::sync::atomic::{AtomicU32, Ordering};

static WINDOW_SEQ: AtomicU32 = AtomicU32::new(1);

/// extra viewer window so several repos can be reviewed side by side
#[tauri::command]
async fn new_window(app: tauri::AppHandle) -> Result<(), String> {
    let label = format!("repo-{}", WINDOW_SEQ.fetch_add(1, Ordering::Relaxed));
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::default())
        .title("GitPrism")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .visible(false) // the frontend shows it once the theme is applied
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().expect("app icon").clone())
        .tooltip("GitPrism")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        // closing the main window hides to tray; the tray menu quits for real
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(watcher::WatcherState(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )));

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .invoke_handler(tauri::generate_handler![
            git::open_repo,
            git::get_status,
            git::get_file_diff,
            git::revert_file,
            git::delete_paths,
            git::list_authors,
            git::list_stashes,
            git::stash_push,
            git::stash_pop,
            git::stash_drop,
            git::revert_hunk,
            git::revert_all,
            git::stage_file,
            git::unstage_file,
            git::stage_all,
            git::unstage_all,
            git::stage_hunk,
            git::unstage_hunk,
            git::create_commit,
            git::commit_paths,
            git::list_branches,
            git::create_branch,
            git::checkout_branch,
            git::delete_branch,
            git::rename_branch,
            git::list_remotes,
            git::fetch_remote,
            git::pull_branch,
            git::push_branch,
            git::cherry_pick_commit,
            git::revert_commit,
            git::reset_to,
            git::count_commits_between,
            git::merge_branch,
            git::get_conflict_sides,
            git::resolve_conflict,
            git::resolve_conflict_binary,
            git::resolve_conflict_delete,
            git::continue_operation,
            git::abort_operation,
            git::list_files,
            git::read_file,
            git::write_file,
            git::repo_stats,
            git::log_commits,
            git::commit_files,
            git::get_commit_message,
            git::blame_file,
            git::get_commit_file_diff,
            git::auto_open_path,
            git::search_text,
            git::get_image_diff,
            git::file_info,
            git::create_file,
            git::create_dir,
            git::get_console_log,
            git::drop_commit,
            git::search_commits,
            git::clone_repo,
            watcher::watch_repo,
            watcher::unwatch_repo,
            new_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
