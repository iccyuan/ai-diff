use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

/// One active watcher per window label, so several windows can watch
/// different repos at the same time. Dropping a watcher disconnects its
/// channel, which ends the matching debounce thread.
pub struct WatcherState(pub Mutex<HashMap<String, ActiveWatch>>);

pub struct ActiveWatch {
    _watcher: RecommendedWatcher,
    root: PathBuf,
}

/// Inside .git only index/HEAD/refs affect what we show; everything else
/// (objects, logs, lock churn from our own git calls) is noise.
fn relevant(root: &Path, p: &Path) -> bool {
    let rel = match p.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let mut comps = rel.components();
    match comps.next() {
        Some(c) if c.as_os_str() == ".git" => {
            let rest: PathBuf = comps.collect();
            let s = rest.to_string_lossy().replace('\\', "/");
            s == "index" || s == "HEAD" || s.starts_with("refs/")
        }
        Some(_) => true,
        None => false,
    }
}

fn debounce_loop(
    rx: Receiver<notify::Result<notify::Event>>,
    app: tauri::AppHandle,
    window_label: String,
    emit_root: String,
    root: PathBuf,
) {
    loop {
        let first = match rx.recv() {
            Ok(e) => e,
            Err(_) => return, // watcher dropped
        };
        let mut pending = first
            .ok()
            .is_some_and(|e| e.paths.iter().any(|p| relevant(&root, p)));
        // coalesce until the repo has been quiet for 400ms
        loop {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(Ok(e)) => {
                    if e.paths.iter().any(|p| relevant(&root, p)) {
                        pending = true;
                    }
                }
                Ok(Err(_)) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if pending {
            let _ = app.emit_to(&window_label, "repo-changed", &emit_root);
        }
    }
}

#[tauri::command]
pub fn watch_repo(
    window: tauri::Window,
    state: tauri::State<'_, WatcherState>,
    repo: String,
) -> Result<(), String> {
    let label = window.label().to_string();
    let root = PathBuf::from(&repo);
    // one watcher per (window, repo): a window can hold several workspaces
    let key = format!("{label}|{repo}");
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.contains_key(&key) {
        return Ok(());
    }

    let (tx, rx) = channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx).map_err(|e| e.to_string())?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    let app = window.app_handle().clone();
    let thread_root = root.clone();
    std::thread::spawn(move || debounce_loop(rx, app, label, repo, thread_root));

    guard.insert(
        key,
        ActiveWatch {
            _watcher: watcher,
            root,
        },
    );
    Ok(())
}

#[tauri::command]
pub fn unwatch_repo(
    window: tauri::Window,
    state: tauri::State<'_, WatcherState>,
    repo: String,
) -> Result<(), String> {
    let key = format!("{}|{repo}", window.label());
    state.0.lock().map_err(|e| e.to_string())?.remove(&key);
    Ok(())
}
