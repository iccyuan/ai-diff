use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;
use tauri::Emitter;

/// Holds the active repo watcher; replaced when another repo is opened.
/// Dropping the watcher disconnects its channel, which ends the debounce thread.
pub struct WatcherState(pub Mutex<Option<ActiveWatch>>);

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
            let _ = app.emit("repo-changed", &emit_root);
        }
    }
}

#[tauri::command]
pub fn watch_repo(
    app: tauri::AppHandle,
    state: tauri::State<'_, WatcherState>,
    repo: String,
) -> Result<(), String> {
    let root = PathBuf::from(&repo);
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.as_ref().is_some_and(|w| w.root == root) {
        return Ok(());
    }
    *guard = None; // drop previous watcher first

    let (tx, rx) = channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx).map_err(|e| e.to_string())?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    let thread_root = root.clone();
    std::thread::spawn(move || debounce_loop(rx, app, repo, thread_root));

    *guard = Some(ActiveWatch {
        _watcher: watcher,
        root,
    });
    Ok(())
}
