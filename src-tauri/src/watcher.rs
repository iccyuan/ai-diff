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
}

/// How a changed path matters to what we show.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    /// objects, logs, lock churn from our own git calls — noise
    Irrelevant,
    /// `.git/index` — also rewritten by our own read-only refreshes
    /// (`git status` refreshes stat info), so the frontend treats an
    /// index-only event right after its own refresh as an echo
    Index,
    /// HEAD / refs / worktree files — always someone else's doing
    Other,
}

/// Inside .git only index/HEAD/refs affect what we show; everything else
/// (objects, logs, lock churn from our own git calls) is noise.
fn classify(root: &Path, p: &Path) -> Change {
    let rel = match p.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => return Change::Irrelevant,
    };
    let mut comps = rel.components();
    match comps.next() {
        Some(c) if c.as_os_str() == ".git" => {
            let rest: PathBuf = comps.collect();
            let s = rest.to_string_lossy().replace('\\', "/");
            if s == "index" {
                Change::Index
            } else if s == "HEAD"
                || s == "MERGE_HEAD"
                || s == "CHERRY_PICK_HEAD"
                || s == "REVERT_HEAD"
                || s == "MERGE_MSG"
                || s.starts_with("refs/")
            {
                Change::Other
            } else {
                Change::Irrelevant
            }
        }
        Some(_) => Change::Other,
        None => Change::Irrelevant,
    }
}

/// the strongest classification across an event's paths
fn classify_event(root: &Path, e: &notify::Event) -> Change {
    e.paths
        .iter()
        .map(|p| classify(root, p))
        .fold(Change::Irrelevant, |acc, c| match (acc, c) {
            (Change::Other, _) | (_, Change::Other) => Change::Other,
            (Change::Index, _) | (_, Change::Index) => Change::Index,
            _ => Change::Irrelevant,
        })
}

/// `repo-changed` payload. `index_only` lets the frontend tell its own
/// refresh's echo (only `.git/index` rewritten) from a real external change
/// (HEAD moved, refs updated, files edited) that must never be dropped.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoChanged {
    root: String,
    index_only: bool,
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
        let mut pending = first.ok().map_or(Change::Irrelevant, |e| classify_event(&root, &e));
        // coalesce until the repo has been quiet for 400ms
        loop {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(Ok(e)) => {
                    let c = classify_event(&root, &e);
                    if c == Change::Other || (c == Change::Index && pending == Change::Irrelevant) {
                        pending = c;
                    }
                }
                Ok(Err(_)) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if pending != Change::Irrelevant {
            let payload = RepoChanged {
                root: emit_root.clone(),
                index_only: pending == Change::Index,
            };
            let _ = app.emit_to(&window_label, "repo-changed", &payload);
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

    guard.insert(key, ActiveWatch { _watcher: watcher });
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
