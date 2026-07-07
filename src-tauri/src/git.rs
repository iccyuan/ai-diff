use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// git's well-known empty tree object; used as the diff base in repos with no commits
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// mid-merge / mid-cherry-pick / mid-revert, detected from marker files under
/// the git dir (MERGE_HEAD / CHERRY_PICK_HEAD / REVERT_HEAD)
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum RepoOperation {
    None,
    Merge,
    CherryPick,
    Revert,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub root: String,
    pub branch: Option<String>,
    pub has_head: bool,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub operation: RepoOperation,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// which side(s) touched a conflicted path, from `git status --porcelain`'s
/// unmerged XY codes (DD/AU/UD/UA/DU/AA/UU)
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ConflictSide {
    BothModified,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
    BothAdded,
    BothDeleted,
}

#[derive(Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    /// line stats from numstat; None for binary files
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    /// Some(..) only when kind == Conflicted
    pub conflict: Option<ConflictSide>,
    /// true if this entry is index-vs-HEAD (staged); false if worktree-vs-index
    /// (unstaged). A partially-staged file legitimately appears twice, once
    /// with each value. Always false for untracked/conflicted/historical
    /// (commit_files) entries, where the distinction doesn't apply.
    pub staged: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
    pub additions: u32,
    pub deletions: u32,
    /// >1 means a merge commit — cherry-pick/revert need a -m mainline choice
    /// we don't offer, so the UI disables those actions for these
    pub parents: Vec<String>,
    /// branch/tag names pointing at this commit (from `%D`), cleaned of the
    /// "HEAD -> " / "tag: " decorations git adds — e.g. ["main", "origin/main"]
    pub refs: Vec<String>,
}

#[derive(Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub index: usize,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// raw hunk text starting at the "@@" line, verbatim bytes from `git diff`
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub original: Option<String>,
    pub modified: Option<String>,
    pub is_binary: bool,
    pub too_large: bool,
    /// "diff --git ..." through "+++ ..." lines, verbatim; prepended to a hunk to form a patch
    pub file_header: String,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    fn binary() -> Self {
        FileDiff {
            original: None,
            modified: None,
            is_binary: true,
            too_large: false,
            file_header: String::new(),
            hunks: Vec::new(),
        }
    }

    fn too_large() -> Self {
        FileDiff {
            original: None,
            modified: None,
            is_binary: false,
            too_large: true,
            file_header: String::new(),
            hunks: Vec::new(),
        }
    }
}

/// one recorded git invocation, shown in the Git panel's 控制台 tab
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleEntry {
    pub root: String,
    pub args: String,
    pub ok: bool,
    pub at_ms: u64,
}

const CONSOLE_CAP: usize = 500;

fn console_log() -> &'static Mutex<VecDeque<ConsoleEntry>> {
    static LOG: OnceLock<Mutex<VecDeque<ConsoleEntry>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(VecDeque::with_capacity(CONSOLE_CAP)))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn record_console(repo: &Path, args: &[&str], ok: bool) {
    let mut log = console_log().lock().unwrap();
    if log.len() >= CONSOLE_CAP {
        log.pop_front();
    }
    log.push_back(ConsoleEntry {
        root: repo.to_string_lossy().into_owned(),
        args: args.join(" "),
        ok,
        at_ms: now_ms(),
    });
}

fn git_command(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo);
    cmd.args(["-c", "core.quotepath=off"]);
    // a GUI app has no terminal to prompt into — without this, a fetch/pull/push
    // hitting a missing/expired credential hangs run_git's blocking wait_with_output
    // forever instead of failing fast with a clear stderr message
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Blocks on `wait_with_output()` with no wall-clock timeout. `GIT_TERMINAL_PROMPT=0`
/// (set in `git_command`) covers the common hang (missing/expired credentials
/// prompting for input), but a genuinely stalled TCP connection to a remote
/// (dead proxy, dropped VPN) can still block a fetch/pull/push indefinitely.
/// Killing the child on a timeout would need a dependency for cancellable
/// process waiting (std's `Child::wait` has no timeout variant) — left as a
/// known gap rather than adding one under time pressure.
fn run_git(repo: &Path, args: &[&str], stdin_data: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut cmd = git_command(repo);
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if stdin_data.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().map_err(|e| format!("无法执行 git: {e}"))?;
    if let Some(data) = stdin_data {
        let mut stdin = child.stdin.take().ok_or("无法打开 git stdin")?;
        stdin
            .write_all(data)
            .map_err(|e| format!("写入 git stdin 失败: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git 执行失败: {e}"))?;
    record_console(repo, args, out.status.success());
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn run_git_text(repo: &Path, args: &[&str]) -> Result<String, String> {
    let bytes = run_git(repo, args, None)?;
    String::from_utf8(bytes).map_err(|_| "git 输出不是有效的 UTF-8".to_string())
}

/// "HEAD" if the repo has at least one commit, otherwise the empty-tree hash,
/// so that diff/status work uniformly in brand-new repos.
fn base_ref(repo: &Path) -> String {
    match run_git(repo, &["rev-parse", "--verify", "--quiet", "HEAD"], None) {
        Ok(_) => "HEAD".to_string(),
        Err(_) => EMPTY_TREE.to_string(),
    }
}

/// Resolved `.git` directory, NOT a naive `repo.join(".git")` — required for
/// linked worktrees / submodules where `.git` is a file pointing elsewhere.
fn git_dir(repo: &Path) -> PathBuf {
    match run_git_text(repo, &["rev-parse", "--git-dir"]) {
        Ok(s) => {
            let p = PathBuf::from(s.trim());
            if p.is_absolute() {
                p
            } else {
                repo.join(p)
            }
        }
        Err(_) => repo.join(".git"),
    }
}

/// Whether the repo is mid-merge / mid-cherry-pick / mid-revert, from the
/// presence of git's own state marker files.
fn detect_operation(repo: &Path) -> RepoOperation {
    let gd = git_dir(repo);
    if gd.join("MERGE_HEAD").exists() {
        RepoOperation::Merge
    } else if gd.join("CHERRY_PICK_HEAD").exists() {
        RepoOperation::CherryPick
    } else if gd.join("REVERT_HEAD").exists() {
        RepoOperation::Revert
    } else {
        RepoOperation::None
    }
}

/// (upstream ref, ahead, behind), all None when no upstream is configured —
/// that's a normal state, not an error.
fn ahead_behind(repo: &Path) -> (Option<String>, Option<u32>, Option<u32>) {
    let upstream = run_git_text(
        repo,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    let Some(upstream) = upstream else {
        return (None, None, None);
    };
    // left-right count of `upstream...HEAD`: left = upstream-only (behind),
    // right = HEAD-only (ahead)
    let spec = format!("{upstream}...HEAD");
    let counts = run_git_text(repo, &["rev-list", "--left-right", "--count", &spec]).ok();
    let parsed = counts.and_then(|s| {
        let mut it = s.split_whitespace();
        let behind = it.next()?.parse::<u32>().ok()?;
        let ahead = it.next()?.parse::<u32>().ok()?;
        Some((ahead, behind))
    });
    match parsed {
        Some((ahead, behind)) => (Some(upstream), Some(ahead), Some(behind)),
        None => (Some(upstream), None, None),
    }
}

/// Maps `git status --porcelain`'s unmerged XY codes to which side(s) touched
/// the path. Only these 7 codes represent unmerged/conflicted entries.
fn conflict_side(xy: &str) -> Option<ConflictSide> {
    match xy {
        "DD" => Some(ConflictSide::BothDeleted),
        "AU" => Some(ConflictSide::AddedByUs),
        "UD" => Some(ConflictSide::DeletedByThem),
        "UA" => Some(ConflictSide::AddedByThem),
        "DU" => Some(ConflictSide::DeletedByUs),
        "AA" => Some(ConflictSide::BothAdded),
        "UU" => Some(ConflictSide::BothModified),
        _ => None,
    }
}

/// Conflicted (path, side) pairs from `git status --porcelain -z`. Renamed
/// entries consume an extra NUL-separated old-path token; conflicts never
/// carry a rename code, but we must still skip that token to keep the
/// zero-separated stream aligned for entries that follow.
fn conflicted_paths(repo: &Path) -> Vec<(String, ConflictSide)> {
    let out = match run_git_text(repo, &["status", "--porcelain", "-z"]) {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let mut result = Vec::new();
    let mut tokens = out.split('\0');
    while let Some(entry) = tokens.next() {
        if entry.len() < 3 {
            continue;
        }
        let xy = &entry[0..2];
        let path = &entry[3..];
        if xy.contains('R') || xy.contains('C') {
            tokens.next(); // consume the old-path token; conflicts never use R/C
        }
        if let Some(side) = conflict_side(xy) {
            result.push((path.to_string(), side));
        }
    }
    result
}

fn parse_name_status(out: &str) -> Vec<FileStatus> {
    let mut tokens = out.split('\0');
    let mut files = Vec::new();
    while let Some(status) = tokens.next() {
        if status.is_empty() {
            continue;
        }
        let code = status.chars().next().unwrap();
        match code {
            'R' | 'C' => {
                let old = match tokens.next() {
                    Some(p) if !p.is_empty() => p,
                    _ => break,
                };
                let new = match tokens.next() {
                    Some(p) if !p.is_empty() => p,
                    _ => break,
                };
                if code == 'R' {
                    files.push(FileStatus {
                        path: new.to_string(),
                        old_path: Some(old.to_string()),
                        kind: ChangeKind::Renamed,
                        additions: None,
                        deletions: None,
                        conflict: None,
                        staged: false,
                    });
                } else {
                    files.push(FileStatus {
                        path: new.to_string(),
                        old_path: None,
                        kind: ChangeKind::Added,
                        additions: None,
                        deletions: None,
                        conflict: None,
                        staged: false,
                    });
                }
            }
            _ => {
                let path = match tokens.next() {
                    Some(p) if !p.is_empty() => p,
                    _ => break,
                };
                let kind = match code {
                    'A' => ChangeKind::Added,
                    'D' => ChangeKind::Deleted,
                    _ => ChangeKind::Modified, // M, T, U(unmerged) all render as modified
                };
                files.push(FileStatus {
                    path: path.to_string(),
                    old_path: None,
                    kind,
                    additions: None,
                    deletions: None,
                    conflict: None,
                    staged: false,
                });
            }
        }
    }
    files
}

/// numstat -z: "added\tdeleted\tpath\0" per file; renames are
/// "added\tdeleted\t\0old\0new\0". "-" columns mean binary.
/// Returns (new_path, additions, deletions).
fn parse_numstat(out: &str) -> Vec<(String, Option<u32>, Option<u32>)> {
    let mut tokens = out.split('\0');
    let mut stats = Vec::new();
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        let mut cols = tok.splitn(3, '\t');
        let (Some(a), Some(d), Some(rest)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let added = a.parse::<u32>().ok();
        let deleted = d.parse::<u32>().ok();
        let path = if rest.is_empty() {
            // rename: two more NUL-separated tokens, keep the new path
            let _old = tokens.next();
            match tokens.next() {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => continue,
            }
        } else {
            rest.to_string()
        };
        stats.push((path, added, deleted));
    }
    stats
}

/// "@@ -a[,b] +c[,d] @@ ..." -> (a, b, c, d); missing count means 1
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old_part, rest) = rest.split_once(" +")?;
    let (new_part, _) = rest.split_once(" @@")?;
    let parse_pair = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (old_start, old_lines) = parse_pair(old_part)?;
    let (new_start, new_lines) = parse_pair(new_part)?;
    Some((old_start, old_lines, new_start, new_lines))
}

/// Split raw `git diff` output into (file_header, hunks, is_binary).
/// Hunk text is kept verbatim (incl. "\ No newline at end of file" markers)
/// so that file_header + hunk.text is a valid patch for `git apply -R`.
fn parse_diff(diff: &str) -> (String, Vec<Hunk>, bool) {
    if diff
        .lines()
        .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch")
    {
        return (String::new(), Vec::new(), true);
    }
    let mut header = String::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<Hunk> = None;
    let mut index = 0usize;
    for line in diff.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            let (old_start, old_lines, new_start, new_lines) =
                parse_hunk_header(line).unwrap_or((0, 0, 0, 0));
            current = Some(Hunk {
                index,
                old_start,
                old_lines,
                new_start,
                new_lines,
                text: line.to_string(),
            });
            index += 1;
        } else if let Some(h) = current.as_mut() {
            h.text.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    if let Some(h) = current.take() {
        hunks.push(h);
    }
    (header, hunks, false)
}

fn untracked_diff(repo: &Path, path: &str) -> Result<FileDiff, String> {
    let full = repo.join(path);
    let meta = std::fs::metadata(&full).map_err(|e| format!("无法读取 {path}: {e}"))?;
    if meta.len() > MAX_FILE_SIZE {
        return Ok(FileDiff::too_large());
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("无法读取 {path}: {e}"))?;
    // git's own binary heuristic: NUL byte in the first 8000 bytes
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return Ok(FileDiff::binary());
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(FileDiff {
            original: None,
            modified: Some(text),
            is_binary: false,
            too_large: false,
            file_header: String::new(),
            hunks: Vec::new(),
        }),
        Err(_) => Ok(FileDiff::binary()),
    }
}

#[derive(Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub line: u32,
    pub text: String,
}

/// Repo-wide fixed-string search via `git grep` (tracked + untracked,
/// binary files skipped). whole_word approximates symbol lookup.
#[tauri::command]
pub async fn search_text(
    repo: String,
    query: String,
    whole_word: bool,
    max: u32,
) -> Result<Vec<SearchHit>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let repo = PathBuf::from(repo);
    let mut args: Vec<&str> = vec!["grep", "-n", "-I", "--no-color", "--untracked", "-F"];
    if whole_word {
        args.push("-w");
    }
    args.push("-e");
    args.push(&query);
    let out = match run_git(&repo, &args, None) {
        Ok(o) => o,
        // exit code 1 with empty stderr = no matches
        Err(e) if e.is_empty() => Vec::new(),
        Err(e) => return Err(e),
    };
    let text = String::from_utf8_lossy(&out);
    let mut hits = Vec::new();
    for line in text.lines() {
        if hits.len() >= max as usize {
            break;
        }
        let mut parts = line.splitn(3, ':');
        let (Some(p), Some(l), Some(t)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let Ok(ln) = l.parse::<u32>() else { continue };
        hits.push(SearchHit {
            path: p.to_string(),
            line: ln,
            text: t.trim_end().chars().take(300).collect(),
        });
    }
    Ok(hits)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDiff {
    /// data URL of the HEAD version (None for added/untracked)
    pub original: Option<String>,
    /// data URL of the working-tree version (None for deleted)
    pub modified: Option<String>,
}

fn image_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Before/after data URLs for an image file (working tree vs HEAD).
#[tauri::command]
pub async fn get_image_diff(
    repo: String,
    path: String,
    old_path: Option<String>,
    kind: ChangeKind,
) -> Result<ImageDiff, String> {
    let repo = PathBuf::from(repo);
    let mime = image_mime(&path).ok_or("不支持的图片格式")?;
    let base = base_ref(&repo);
    let head_rel = old_path.unwrap_or_else(|| path.clone());

    let mut original = None;
    if kind != ChangeKind::Added && kind != ChangeKind::Untracked {
        let spec = format!("{base}:{head_rel}");
        if let Ok(bytes) = run_git(&repo, &["show", &spec], None) {
            if bytes.len() as u64 <= MAX_FILE_SIZE {
                original = Some(data_url(mime, &bytes));
            }
        }
    }
    let mut modified = None;
    if kind != ChangeKind::Deleted {
        let full = repo.join(&path);
        if let Ok(meta) = std::fs::metadata(&full) {
            if meta.len() <= MAX_FILE_SIZE {
                if let Ok(bytes) = std::fs::read(&full) {
                    modified = Some(data_url(mime, &bytes));
                }
            }
        }
    }
    Ok(ImageDiff { original, modified })
}

/// repo to open on launch, from AI_DIFF_OPEN_REPO (or legacy VITE_OPEN_REPO);
/// read on the Rust side so it works regardless of how vite resolves env.
/// Only the main window auto-opens — extra windows start empty.
#[tauri::command]
pub fn auto_open_path(window: tauri::Window) -> Option<String> {
    if window.label() != "main" {
        return None;
    }
    std::env::var("AI_DIFF_OPEN_REPO")
        .or_else(|_| std::env::var("VITE_OPEN_REPO"))
        .ok()
        .filter(|s| !s.trim().is_empty())
}

#[tauri::command]
pub async fn open_repo(path: String) -> Result<RepoInfo, String> {
    let mut p = PathBuf::from(&path);
    // dropping a file counts as its directory; rev-parse walks up to the root
    if p.is_file() {
        if let Some(parent) = p.parent() {
            p = parent.to_path_buf();
        }
    }
    if !p.is_dir() {
        return Err(format!("目录不存在: {path}"));
    }
    let root = run_git_text(&p, &["rev-parse", "--show-toplevel"])
        .map_err(|_| "该目录不是 git 仓库".to_string())?;
    let root = root.trim().to_string();
    let root_path = PathBuf::from(&root);
    let has_head = base_ref(&root_path) == "HEAD";
    let branch = run_git_text(&root_path, &["branch", "--show-current"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let (upstream, ahead, behind) = ahead_behind(&root_path);
    let operation = detect_operation(&root_path);
    Ok(RepoInfo {
        root,
        branch,
        has_head,
        upstream,
        ahead,
        behind,
        operation,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub content: Option<String>,
    pub is_binary: bool,
    pub too_large: bool,
}

/// All files in the project view: tracked (incl. staged) + untracked.
/// When `show_ignored` is false, .gitignore is respected; when true, ignored
/// files/folders (node_modules, build output, …) are listed too. The .git
/// directory is always skipped by git itself. NUL-separated, repo-relative
/// forward slashes.
#[tauri::command]
pub async fn list_files(repo: String, show_ignored: bool) -> Result<Vec<String>, String> {
    let repo = PathBuf::from(repo);
    let mut args = vec!["ls-files", "--cached", "--others"];
    if !show_ignored {
        args.push("--exclude-standard");
    }
    args.push("-z");
    let out = run_git_text(&repo, &args)?;
    let mut files: Vec<String> = out
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

#[tauri::command]
pub async fn read_file(repo: String, path: String) -> Result<FileContent, String> {
    let full = PathBuf::from(&repo).join(&path);
    let meta = std::fs::metadata(&full).map_err(|e| format!("无法读取 {path}: {e}"))?;
    if meta.len() > MAX_FILE_SIZE {
        return Ok(FileContent {
            content: None,
            is_binary: false,
            too_large: true,
        });
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("无法读取 {path}: {e}"))?;
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return Ok(FileContent {
            content: None,
            is_binary: true,
            too_large: false,
        });
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(FileContent {
            content: Some(text),
            is_binary: false,
            too_large: false,
        }),
        Err(_) => Ok(FileContent {
            content: None,
            is_binary: true,
            too_large: false,
        }),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    /// path relative to the repo root (as shown in the tree)
    pub path: String,
    /// absolute path on disk
    pub full_path: String,
    /// false for deleted files that no longer exist in the working tree
    pub exists: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub readonly: bool,
    /// modification / creation time as unix-epoch milliseconds, when available
    pub modified: Option<u64>,
    pub created: Option<u64>,
    /// best-effort line count; None for binary/large/missing files
    pub lines: Option<u32>,
}

fn system_time_millis(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()
        .and_then(|st| st.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
}

/// Filesystem metadata for the file/dir at `path` (relative to `repo`), shown
/// in the "查看文件信息" dialog. Missing files (e.g. deleted) return exists=false
/// rather than an error so the dialog can still report the path.
#[tauri::command]
pub async fn file_info(repo: String, path: String) -> Result<FileInfo, String> {
    let full = PathBuf::from(&repo).join(&path);
    let name = Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let full_path = full.to_string_lossy().into_owned();
    let is_symlink = std::fs::symlink_metadata(&full)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    match std::fs::metadata(&full) {
        Ok(meta) => Ok(FileInfo {
            name,
            path,
            full_path,
            exists: true,
            is_dir: meta.is_dir(),
            is_symlink,
            size: meta.len(),
            readonly: meta.permissions().readonly(),
            modified: system_time_millis(meta.modified()),
            created: system_time_millis(meta.created()),
            lines: if meta.is_dir() { None } else { count_lines(&full) },
        }),
        Err(_) => Ok(FileInfo {
            name,
            path,
            full_path,
            exists: false,
            is_dir: false,
            is_symlink,
            size: 0,
            readonly: false,
            modified: None,
            created: None,
            lines: None,
        }),
    }
}

/// Creates an empty file at `path` (parent dirs created as needed). Does not
/// stage it — IDEA-style "new = untracked", the user decides when to add it.
/// Errors if something already exists there.
#[tauri::command]
pub async fn create_file(repo: String, path: String) -> Result<(), String> {
    let full = PathBuf::from(&repo).join(&path);
    if full.exists() {
        return Err(format!("{path} 已存在"));
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    std::fs::write(&full, b"").map_err(|e| format!("创建文件失败: {e}"))
}

/// Creates a directory (and any missing parents) at `path`. Errors if
/// something already exists there.
#[tauri::command]
pub async fn create_dir(repo: String, path: String) -> Result<(), String> {
    let full = PathBuf::from(&repo).join(&path);
    if full.exists() {
        return Err(format!("{path} 已存在"));
    }
    std::fs::create_dir_all(&full).map_err(|e| format!("创建目录失败: {e}"))
}

/// Recent git invocations for `repo`, most-recent-last, shown in the Git
/// panel's 控制台 tab. Backed by an in-process ring buffer (capacity 500
/// across all repos) — not persisted, resets on app restart.
#[tauri::command]
pub async fn get_console_log(repo: String) -> Result<Vec<ConsoleEntry>, String> {
    let log = console_log().lock().unwrap();
    Ok(log.iter().filter(|e| e.root == repo).cloned().collect())
}

/// best-effort line count of an untracked file; None for binary/large
fn count_lines(full: &Path) -> Option<u32> {
    let meta = std::fs::metadata(full).ok()?;
    if meta.len() > 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(full).ok()?;
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return None;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count() as u32;
    Some(newlines + if bytes.last().is_some_and(|&b| b != b'\n') { 1 } else { 0 })
}

/// Refresh the index's stat cache so `git diff HEAD` doesn't report a stale
/// "clean" tree. Without this, the first status read after opening a repo can
/// miss real changes (git's stat shortcut skips content comparison when the
/// cached stat still matches); `git status` does this implicitly, we don't.
/// Exits non-zero when entries need updating — that's expected, so ignore it.
fn refresh_index(repo: &Path) {
    let _ = run_git(repo, &["update-index", "-q", "--refresh"], None);
}

/// `refs` is the ref-related prefix to `git diff` (e.g. `["--cached", "HEAD"]`,
/// `["parent", "hash"]`, or `[]` for a plain index-vs-worktree diff).
fn attach_numstat(repo: &Path, refs: &[&str], files: &mut [FileStatus]) {
    let mut args = vec!["diff"];
    args.extend_from_slice(refs);
    args.extend(["--numstat", "-z", "-M", "--no-color", "--no-ext-diff"]);
    if let Ok(out) = run_git_text(repo, &args) {
        for (path, added, deleted) in parse_numstat(&out) {
            if let Some(f) = files.iter_mut().find(|f| f.path == path) {
                f.additions = added;
                f.deletions = deleted;
            }
        }
    }
}

#[tauri::command]
pub async fn get_status(repo: String) -> Result<Vec<FileStatus>, String> {
    let repo = PathBuf::from(repo);
    refresh_index(&repo);
    // conflicted paths are reported via their own kind — their plain `diff`
    // output would otherwise be confusing (it shows one arbitrary stage),
    // so exclude them from the regular staged/unstaged passes below.
    let conflicted = conflicted_paths(&repo);
    let conflicted_set: std::collections::HashSet<&str> =
        conflicted.iter().map(|(p, _)| p.as_str()).collect();
    let base = base_ref(&repo);

    // staged: index vs HEAD (or the empty tree, in a repo with no commits yet)
    let staged_out = run_git_text(
        &repo,
        &[
            "diff",
            "--cached",
            &base,
            "--name-status",
            "-z",
            "-M",
            "--no-color",
            "--no-ext-diff",
        ],
    )?;
    let mut staged_files = parse_name_status(&staged_out);
    staged_files.retain(|f| !conflicted_set.contains(f.path.as_str()));
    attach_numstat(&repo, &["--cached", &base], &mut staged_files);
    for f in &mut staged_files {
        f.staged = true;
    }

    // unstaged: worktree vs index — deliberately no ref at all, so a
    // partially-staged file's remaining worktree edit shows up here too
    let unstaged_out = run_git_text(
        &repo,
        &["diff", "--name-status", "-z", "-M", "--no-color", "--no-ext-diff"],
    )?;
    let mut unstaged_files = parse_name_status(&unstaged_out);
    unstaged_files.retain(|f| !conflicted_set.contains(f.path.as_str()));
    attach_numstat(&repo, &[], &mut unstaged_files);

    let mut files = staged_files;
    files.append(&mut unstaged_files);

    let untracked = run_git_text(&repo, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for p in untracked.split('\0').filter(|s| !s.is_empty()) {
        if conflicted_set.contains(p) {
            continue;
        }
        files.push(FileStatus {
            path: p.to_string(),
            old_path: None,
            kind: ChangeKind::Untracked,
            additions: count_lines(&repo.join(p)),
            deletions: Some(0),
            conflict: None,
            staged: false,
        });
    }
    for (path, side) in conflicted {
        files.push(FileStatus {
            path,
            old_path: None,
            kind: ChangeKind::Conflicted,
            additions: None,
            deletions: None,
            conflict: Some(side),
            staged: false,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// first parent of a commit, or the empty tree for root commits —
/// both forms are valid `git diff` operands
fn parent_ref(repo: &Path, hash: &str) -> String {
    let spec = format!("{hash}^1");
    match run_git(repo, &["rev-parse", "--verify", "--quiet", &spec], None) {
        Ok(_) => spec,
        Err(_) => EMPTY_TREE.to_string(),
    }
}

/// Parses `%D`'s ref-name decoration string (e.g. "HEAD -> main, origin/main,
/// tag: v1.0") into cleaned individual names (["main", "origin/main", "v1.0"]).
/// A bare "HEAD" (detached) isn't a real ref, so it's dropped.
fn parse_decorations(raw: &str) -> Vec<String> {
    raw.split(", ")
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() || part == "HEAD" {
                return None;
            }
            let part = part.strip_prefix("HEAD -> ").unwrap_or(part);
            let part = part.strip_prefix("tag: ").unwrap_or(part);
            Some(part.to_string())
        })
        .collect()
}

#[tauri::command]
pub async fn log_commits(
    repo: String,
    skip: u32,
    count: u32,
    branch: Option<String>,
) -> Result<Vec<CommitInfo>, String> {
    let repo = PathBuf::from(repo);
    if base_ref(&repo) != "HEAD" {
        return Ok(Vec::new()); // empty repo: no history yet
    }
    let skip_arg = format!("--skip={skip}");
    let count_arg = format!("--max-count={count}");
    // \x01 marks each record start; --shortstat appends a
    // " N files changed, X insertions(+), Y deletions(-)" line per commit
    let mut args: Vec<&str> = vec!["log"];
    if let Some(b) = &branch {
        args.push(b); // scope the log to this branch/ref instead of HEAD
    }
    args.extend([
        &skip_arg,
        &count_arg,
        "--shortstat",
        "--date=format:%Y-%m-%d %H:%M",
        "--pretty=format:%x01%H%x00%h%x00%an%x00%ae%x00%ad%x00%P%x00%s%x00%D",
    ]);
    let out = run_git_text(&repo, &args)?;
    let mut commits = Vec::new();
    for block in out.split('\x01').filter(|b| !b.is_empty()) {
        let mut lines = block.lines();
        let Some(head) = lines.next() else { continue };
        let cols: Vec<&str> = head.splitn(8, '\0').collect();
        if cols.len() != 8 {
            continue;
        }
        let (mut additions, mut deletions) = (0u32, 0u32);
        for line in lines {
            if !line.contains(" changed") {
                continue;
            }
            for seg in line.split(',') {
                let n = seg.trim().split(' ').next().and_then(|s| s.parse::<u32>().ok());
                if seg.contains("insertion") {
                    additions = n.unwrap_or(0);
                } else if seg.contains("deletion") {
                    deletions = n.unwrap_or(0);
                }
            }
        }
        let parents = cols[5].split_whitespace().map(String::from).collect();
        commits.push(CommitInfo {
            hash: cols[0].to_string(),
            short_hash: cols[1].to_string(),
            author: cols[2].to_string(),
            email: cols[3].to_string(),
            date: cols[4].to_string(),
            subject: cols[6].to_string(),
            additions,
            deletions,
            parents,
            refs: parse_decorations(cols[7]),
        });
    }
    Ok(commits)
}

#[tauri::command]
pub async fn commit_files(repo: String, hash: String) -> Result<Vec<FileStatus>, String> {
    let repo = PathBuf::from(repo);
    let parent = parent_ref(&repo, &hash);
    let out = run_git_text(
        &repo,
        &[
            "diff",
            &parent,
            &hash,
            "--name-status",
            "-z",
            "-M",
            "--no-color",
            "--no-ext-diff",
        ],
    )?;
    let mut files = parse_name_status(&out);
    attach_numstat(&repo, &[&parent, &hash], &mut files);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Diff of one file within a historical commit (parent vs commit).
/// No hunks/file_header: history view is read-only, nothing to revert.
#[tauri::command]
pub async fn get_commit_file_diff(
    repo: String,
    hash: String,
    path: String,
    old_path: Option<String>,
    kind: ChangeKind,
) -> Result<FileDiff, String> {
    let repo = PathBuf::from(repo);
    let parent = parent_ref(&repo, &hash);
    let head_rel = old_path.unwrap_or_else(|| path.clone());

    for (skip, spec) in [
        (kind == ChangeKind::Added || kind == ChangeKind::Untracked, format!("{parent}:{head_rel}")),
        (kind == ChangeKind::Deleted, format!("{hash}:{path}")),
    ] {
        if !skip {
            if let Ok(sz) = run_git_text(&repo, &["cat-file", "-s", &spec]) {
                if sz.trim().parse::<u64>().unwrap_or(0) > MAX_FILE_SIZE {
                    return Ok(FileDiff::too_large());
                }
            }
        }
    }

    let mut original = None;
    let mut modified = None;
    let mut is_binary = false;
    if kind != ChangeKind::Added && kind != ChangeKind::Untracked {
        let spec = format!("{parent}:{head_rel}");
        match String::from_utf8(run_git(&repo, &["show", &spec], None)?) {
            Ok(s) => original = Some(s),
            Err(_) => is_binary = true,
        }
    }
    if kind != ChangeKind::Deleted && !is_binary {
        let spec = format!("{hash}:{path}");
        match String::from_utf8(run_git(&repo, &["show", &spec], None)?) {
            Ok(s) => modified = Some(s),
            Err(_) => is_binary = true,
        }
    }
    if is_binary {
        return Ok(FileDiff::binary());
    }
    Ok(FileDiff {
        original,
        modified,
        is_binary: false,
        too_large: false,
        file_header: String::new(),
        hunks: Vec::new(),
    })
}

/// `staged=true`: index vs HEAD (survives further worktree edits on top).
/// `staged=false`: worktree vs index (the "unstaged" remainder of a
/// partially-staged file). These are genuinely different comparisons, not
/// just plumbing — a file can have independent staged and unstaged hunks.
#[tauri::command]
pub async fn get_file_diff(
    repo: String,
    path: String,
    old_path: Option<String>,
    kind: ChangeKind,
    staged: bool,
) -> Result<FileDiff, String> {
    let repo = PathBuf::from(repo);

    if kind == ChangeKind::Untracked {
        return untracked_diff(&repo, &path);
    }
    refresh_index(&repo); // avoid stale-clean hunks (see get_status)

    let wt_path = repo.join(&path);
    // the "old" side's rev-spec: HEAD-relative for staged (renames live at the
    // old path there), index-relative (at the current path) for unstaged —
    // once staged, a rename has already happened in the index.
    let old_spec = if staged {
        let head_rel = old_path.clone().unwrap_or_else(|| path.clone());
        format!("{}:{head_rel}", base_ref(&repo))
    } else {
        format!(":{path}")
    };

    if kind != ChangeKind::Deleted {
        if staged {
            let new_spec = format!(":{path}");
            if let Ok(sz) = run_git_text(&repo, &["cat-file", "-s", &new_spec]) {
                if sz.trim().parse::<u64>().unwrap_or(0) > MAX_FILE_SIZE {
                    return Ok(FileDiff::too_large());
                }
            }
        } else if let Ok(meta) = std::fs::metadata(&wt_path) {
            if meta.len() > MAX_FILE_SIZE {
                return Ok(FileDiff::too_large());
            }
        }
    }
    if kind != ChangeKind::Added {
        if let Ok(sz) = run_git_text(&repo, &["cat-file", "-s", &old_spec]) {
            if sz.trim().parse::<u64>().unwrap_or(0) > MAX_FILE_SIZE {
                return Ok(FileDiff::too_large());
            }
        }
    }

    let mut args: Vec<String> = vec!["diff".into()];
    if staged {
        args.push("--cached".into());
        args.push(base_ref(&repo));
    }
    args.extend(["--no-color", "--no-ext-diff", "--unified=3", "-M", "--"].map(String::from));
    if let Some(old) = &old_path {
        args.push(old.clone());
    }
    args.push(path.clone());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let diff_bytes = run_git(&repo, &arg_refs, None)?;
    let (file_header, hunks, mut is_binary) = match String::from_utf8(diff_bytes) {
        Ok(text) => parse_diff(&text),
        Err(_) => (String::new(), Vec::new(), true),
    };

    let mut original = None;
    let mut modified = None;
    if !is_binary && kind != ChangeKind::Added {
        match String::from_utf8(run_git(&repo, &["show", &old_spec], None)?) {
            Ok(s) => original = Some(s),
            Err(_) => is_binary = true,
        }
    }
    if !is_binary && kind != ChangeKind::Deleted {
        if staged {
            let new_spec = format!(":{path}");
            match String::from_utf8(run_git(&repo, &["show", &new_spec], None)?) {
                Ok(s) => modified = Some(s),
                Err(_) => is_binary = true,
            }
        } else {
            let bytes = std::fs::read(&wt_path).map_err(|e| format!("无法读取 {path}: {e}"))?;
            match String::from_utf8(bytes) {
                Ok(s) => modified = Some(s),
                Err(_) => is_binary = true,
            }
        }
    }

    if is_binary {
        return Ok(FileDiff::binary());
    }
    Ok(FileDiff {
        original,
        modified,
        is_binary: false,
        too_large: false,
        file_header,
        hunks,
    })
}

#[tauri::command]
pub async fn revert_file(
    repo: String,
    path: String,
    old_path: Option<String>,
    kind: ChangeKind,
) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    match kind {
        ChangeKind::Modified | ChangeKind::Deleted => {
            run_git(
                &repo,
                &["restore", "--source=HEAD", "--staged", "--worktree", "--", &path],
                None,
            )?;
        }
        ChangeKind::Added => {
            run_git(&repo, &["rm", "--cached", "-f", "--", &path], None)?;
            std::fs::remove_file(repo.join(&path)).map_err(|e| format!("删除 {path} 失败: {e}"))?;
        }
        ChangeKind::Untracked => {
            std::fs::remove_file(repo.join(&path)).map_err(|e| format!("删除 {path} 失败: {e}"))?;
        }
        ChangeKind::Renamed => {
            let old = old_path.ok_or("重命名缺少原路径")?;
            run_git(
                &repo,
                &["restore", "--source=HEAD", "--staged", "--worktree", "--", &old],
                None,
            )?;
            run_git(&repo, &["rm", "--cached", "-f", "--", &path], None)?;
            std::fs::remove_file(repo.join(&path)).map_err(|e| format!("删除 {path} 失败: {e}"))?;
        }
        ChangeKind::Conflicted => {
            return Err("该文件存在合并冲突，请先解决冲突".to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn revert_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let mut patch = patch;
    if !patch.ends_with('\n') {
        patch.push('\n');
    }
    let first = run_git(
        &repo,
        &["apply", "-R", "--whitespace=nowarn", "-"],
        Some(patch.as_bytes()),
    );
    if let Err(e1) = first {
        run_git(
            &repo,
            &["apply", "-R", "--whitespace=nowarn", "--ignore-whitespace", "-"],
            Some(patch.as_bytes()),
        )
        .map_err(|e2| format!("还原失败: {e1}（宽松空白重试: {e2}）"))?;
    }
    // apply -R only touches the worktree; re-point a possibly-staged index entry at HEAD
    // so the HEAD-vs-worktree model stays consistent. Best-effort: fails in empty repos.
    if base_ref(&repo) == "HEAD" {
        let _ = run_git(&repo, &["restore", "--staged", "--", &path], None);
        // apply -R writes the blob's (clean-filtered) bytes, so under autocrlf=true a
        // CRLF worktree file comes back LF; the index entry's recorded eol-convert
        // state also goes stale, making `git status` report a phantom modification.
        // Once the file is filter-clean vs HEAD, a forced path checkout rewrites
        // worktree (through smudge) AND the index entry, fixing both.
        if run_git(&repo, &["diff", "--quiet", "HEAD", "--", &path], None).is_ok() {
            let _ = run_git(&repo, &["checkout", "HEAD", "--", &path], None);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn revert_all(repo: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if base_ref(&repo) != "HEAD" {
        return Err("仓库还没有任何提交，无法整体还原".to_string());
    }
    run_git(&repo, &["reset", "--hard", "HEAD"], None)?;
    // -fd without -x: untracked files/dirs go, .gitignore'd artifacts survive
    run_git(&repo, &["clean", "-fd"], None)?;
    Ok(())
}

/// Stage one path (or a rename's both sides) — `git add -A` so deletions and
/// untracked files stage correctly too, not just modifications.
#[tauri::command]
pub async fn stage_file(repo: String, path: String, old_path: Option<String>) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if let Some(old) = &old_path {
        run_git(&repo, &["add", "-A", "--", old, &path], None)?;
    } else {
        run_git(&repo, &["add", "-A", "--", &path], None)?;
    }
    Ok(())
}

/// Index-only inverse of staging — mirrors `revert_file`'s per-kind dispatch
/// but never touches the worktree. `kind`/`old_path` matter because a staged
/// Added file has no HEAD version to restore from, and a staged Renamed file
/// needs its old path un-deleted from the index as well as its new path
/// un-added.
#[tauri::command]
pub async fn unstage_file(
    repo: String,
    path: String,
    old_path: Option<String>,
    kind: ChangeKind,
) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    match kind {
        ChangeKind::Added => {
            run_git(&repo, &["rm", "--cached", "-f", "--", &path], None)?;
        }
        ChangeKind::Renamed => {
            let old = old_path.ok_or("重命名缺少原路径")?;
            if base_ref(&repo) == "HEAD" {
                // old_path no longer has an index entry to infer source from —
                // must say --source=HEAD explicitly (see revert_file's rename arm)
                run_git(&repo, &["restore", "--source=HEAD", "--staged", "--", &old], None)?;
            }
            run_git(&repo, &["rm", "--cached", "-f", "--", &path], None)?;
        }
        _ => {
            if base_ref(&repo) == "HEAD" {
                run_git(&repo, &["restore", "--staged", "--", &path], None)?;
            } else {
                run_git(&repo, &["rm", "--cached", "-f", "--", &path], None)?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn stage_all(repo: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    run_git(&repo, &["add", "-A"], None)?;
    Ok(())
}

#[tauri::command]
pub async fn unstage_all(repo: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if base_ref(&repo) == "HEAD" {
        run_git(&repo, &["reset"], None)?; // mixed reset to HEAD: HEAD itself doesn't move
    } else {
        run_git(&repo, &["read-tree", "--empty"], None)?;
    }
    Ok(())
}

/// Stage exactly one hunk from the *unstaged* (worktree-vs-index) diff —
/// index-only, the worktree file is untouched.
#[tauri::command]
pub async fn stage_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let mut patch = patch;
    if !patch.ends_with('\n') {
        patch.push('\n');
    }
    let first = run_git(
        &repo,
        &["apply", "--cached", "--whitespace=nowarn", "-"],
        Some(patch.as_bytes()),
    );
    if let Err(e1) = first {
        run_git(
            &repo,
            &["apply", "--cached", "--whitespace=nowarn", "--ignore-whitespace", "-"],
            Some(patch.as_bytes()),
        )
        .map_err(|e2| format!("暂存 {path} 失败: {e1}（宽松空白重试: {e2}）"))?;
    }
    Ok(())
}

/// Unstage exactly one hunk from the *staged* (index-vs-HEAD) diff —
/// index-only, the worktree file is untouched.
#[tauri::command]
pub async fn unstage_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let mut patch = patch;
    if !patch.ends_with('\n') {
        patch.push('\n');
    }
    let first = run_git(
        &repo,
        &["apply", "--cached", "-R", "--whitespace=nowarn", "-"],
        Some(patch.as_bytes()),
    );
    if let Err(e1) = first {
        run_git(
            &repo,
            &["apply", "--cached", "-R", "--whitespace=nowarn", "--ignore-whitespace", "-"],
            Some(patch.as_bytes()),
        )
        .map_err(|e2| format!("取消暂存 {path} 失败: {e1}（宽松空白重试: {e2}）"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn create_commit(repo: String, message: String, amend: bool) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if message.trim().is_empty() {
        return Err("提交信息不能为空".to_string());
    }
    let mut args = vec!["commit", "-F", "-"];
    if amend {
        args.push("--amend");
    }
    run_git(&repo, &args, Some(message.as_bytes()))?;
    Ok(())
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
}

/// "[ahead 2, behind 1]" | "[ahead 2]" | "[behind 1]" | "[gone]" | ""
fn parse_track(track: &str) -> (Option<u32>, Option<u32>) {
    if track.is_empty() || track.contains("gone") {
        return (None, None);
    }
    let inner = track.trim_start_matches('[').trim_end_matches(']');
    let mut ahead = None;
    let mut behind = None;
    for part in inner.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().ok();
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().ok();
        }
    }
    (ahead, behind)
}

/// Local + remote-tracking branches, current branch marked. Uses `\x01` as a
/// field separator — refs cannot contain control characters, so this can
/// never collide with a real branch/upstream name.
#[tauri::command]
pub async fn list_branches(repo: String) -> Result<Vec<BranchInfo>, String> {
    let repo = PathBuf::from(repo);
    let out = run_git_text(
        &repo,
        &[
            "for-each-ref",
            "refs/heads",
            "refs/remotes",
            "--format=%(refname)%01%(HEAD)%01%(upstream:short)%01%(upstream:track)",
        ],
    )?;
    let mut branches = Vec::new();
    for line in out.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, '\u{1}');
        let (Some(refname), Some(head), Some(upstream), Some(track)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (name, is_remote) = if let Some(n) = refname.strip_prefix("refs/heads/") {
            (n.to_string(), false)
        } else if let Some(n) = refname.strip_prefix("refs/remotes/") {
            (n.to_string(), true)
        } else {
            continue;
        };
        if is_remote && name.ends_with("/HEAD") {
            continue; // symbolic ref to the remote's default branch, not a real branch
        }
        let (ahead, behind) = parse_track(track);
        branches.push(BranchInfo {
            name,
            is_current: head == "*",
            is_remote,
            upstream: if upstream.is_empty() { None } else { Some(upstream.to_string()) },
            ahead,
            behind,
        });
    }
    Ok(branches)
}

#[tauri::command]
pub async fn create_branch(
    repo: String,
    name: String,
    start_point: Option<String>,
    checkout: bool,
) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if checkout {
        let mut args = vec!["switch", "-c", name.as_str()];
        if let Some(sp) = &start_point {
            args.push(sp);
        }
        run_git(&repo, &args, None)?;
    } else {
        let mut args = vec!["branch", name.as_str()];
        if let Some(sp) = &start_point {
            args.push(sp);
        }
        run_git(&repo, &args, None)?;
    }
    Ok(())
}

/// `is_remote` branches (e.g. "origin/feature-x") get a local tracking branch
/// created (named after the part past the first `/`) rather than landing in
/// detached HEAD, which is what a bare `git switch origin/feature-x` would do.
#[tauri::command]
pub async fn checkout_branch(repo: String, name: String, is_remote: bool) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if is_remote {
        let local = name.splitn(2, '/').nth(1).unwrap_or(&name);
        run_git(&repo, &["switch", "--track", "-c", local, &name], None)?;
    } else {
        run_git(&repo, &["switch", &name], None)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_branch(repo: String, name: String, force: bool) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let flag = if force { "-D" } else { "-d" };
    run_git(&repo, &["branch", flag, "--", &name], None)?;
    Ok(())
}

#[tauri::command]
pub async fn rename_branch(repo: String, old: String, new: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    run_git(&repo, &["branch", "-m", &old, &new], None)?;
    Ok(())
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

#[tauri::command]
pub async fn list_remotes(repo: String) -> Result<Vec<RemoteInfo>, String> {
    let repo = PathBuf::from(repo);
    let out = run_git_text(&repo, &["remote", "-v"])?;
    let mut seen = std::collections::HashSet::new();
    let mut remotes = Vec::new();
    for line in out.lines() {
        // "origin\thttps://example.com/repo.git (fetch)"
        let mut parts = line.splitn(2, '\t');
        let (Some(name), Some(rest)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            continue; // fetch/push URLs usually match; list each remote once
        }
        let url = rest.rsplit_once(' ').map(|(u, _)| u).unwrap_or(rest);
        remotes.push(RemoteInfo { name: name.to_string(), url: url.to_string() });
    }
    Ok(remotes)
}

#[tauri::command]
pub async fn fetch_remote(repo: String, remote: Option<String>, prune: bool) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let mut args = vec!["fetch"];
    if prune {
        args.push("--prune");
    }
    if let Some(r) = &remote {
        args.push(r);
    }
    run_git(&repo, &args, None)?;
    Ok(())
}

#[tauri::command]
pub async fn pull_branch(repo: String, remote: Option<String>) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let mut args = vec!["pull"];
    if let Some(r) = &remote {
        args.push(r);
    }
    run_git(&repo, &args, None)?;
    Ok(())
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ForceMode {
    None,
    Lease,
    Force,
}

#[tauri::command]
pub async fn push_branch(
    repo: String,
    remote: String,
    branch: String,
    set_upstream: bool,
    force: ForceMode,
) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let mut args = vec!["push".to_string()];
    match force {
        ForceMode::Lease => args.push("--force-with-lease".to_string()),
        ForceMode::Force => args.push("--force".to_string()),
        ForceMode::None => {}
    }
    if set_upstream {
        args.push("-u".to_string());
    }
    args.push(remote);
    args.push(branch);
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_git(&repo, &arg_refs, None)?;
    Ok(())
}

/// Shared by cherry-pick / revert / merge (PR6): a non-zero exit either means
/// the operation genuinely failed (bad hash, dirty worktree, ...) or that it
/// left a conflict to resolve — only the presence of `*_HEAD` after the call
/// disambiguates which one happened.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum OpOutcome {
    Applied,
    Conflict,
}

fn classify_op_result(repo: &Path, result: Result<Vec<u8>, String>) -> Result<OpOutcome, String> {
    match result {
        Ok(_) => Ok(OpOutcome::Applied),
        Err(e) => {
            if detect_operation(repo) != RepoOperation::None {
                Ok(OpOutcome::Conflict)
            } else {
                Err(e)
            }
        }
    }
}

#[tauri::command]
pub async fn cherry_pick_commit(repo: String, hash: String) -> Result<OpOutcome, String> {
    let repo = PathBuf::from(repo);
    let result = run_git(&repo, &["cherry-pick", &hash], None);
    classify_op_result(&repo, result)
}

#[tauri::command]
pub async fn revert_commit(repo: String, hash: String) -> Result<OpOutcome, String> {
    let repo = PathBuf::from(repo);
    let result = run_git(&repo, &["revert", "--no-edit", &hash], None);
    classify_op_result(&repo, result)
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[tauri::command]
pub async fn reset_to(repo: String, hash: String, mode: ResetMode) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    };
    run_git(&repo, &["reset", flag, &hash], None)?;
    Ok(())
}

/// commit count strictly between `from` (exclusive) and `to` (inclusive) —
/// used to phrase the reset --hard confirm dialog ("discards N commits")
#[tauri::command]
pub async fn count_commits_between(repo: String, from: String, to: String) -> Result<u32, String> {
    let repo = PathBuf::from(repo);
    let spec = format!("{from}..{to}");
    let out = run_git_text(&repo, &["rev-list", "--count", &spec])?;
    out.trim().parse::<u32>().map_err(|_| "无法解析提交数量".to_string())
}

#[tauri::command]
pub async fn merge_branch(repo: String, source: String, no_ff: bool) -> Result<OpOutcome, String> {
    let repo = PathBuf::from(repo);
    let mut args = vec!["merge", "--no-edit"];
    if no_ff {
        args.push("--no-ff");
    }
    args.push(&source);
    let result = run_git(&repo, &args, None);
    classify_op_result(&repo, result)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSides {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub is_binary: bool,
    pub too_large: bool,
}

enum StageContent {
    Missing,
    Binary,
    Text(String),
}

/// stage 1 = base (common ancestor), 2 = ours, 3 = theirs. A missing stage is
/// a legitimate outcome for add/add and delete/modify conflicts, not an error.
fn read_conflict_stage(repo: &Path, stage: u8, path: &str) -> StageContent {
    let spec = format!(":{stage}:{path}");
    match run_git(repo, &["show", &spec], None) {
        Err(_) => StageContent::Missing,
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => StageContent::Text(s),
            Err(_) => StageContent::Binary,
        },
    }
}

#[tauri::command]
pub async fn get_conflict_sides(repo: String, path: String) -> Result<ConflictSides, String> {
    let repo = PathBuf::from(repo);
    for stage in [1u8, 2, 3] {
        let spec = format!(":{stage}:{path}");
        if let Ok(sz) = run_git_text(&repo, &["cat-file", "-s", &spec]) {
            if sz.trim().parse::<u64>().unwrap_or(0) > MAX_FILE_SIZE {
                return Ok(ConflictSides {
                    base: None,
                    ours: None,
                    theirs: None,
                    is_binary: false,
                    too_large: true,
                });
            }
        }
    }
    let base = read_conflict_stage(&repo, 1, &path);
    let ours = read_conflict_stage(&repo, 2, &path);
    let theirs = read_conflict_stage(&repo, 3, &path);
    let is_binary = [&base, &ours, &theirs]
        .iter()
        .any(|s| matches!(s, StageContent::Binary));
    fn to_opt(s: StageContent) -> Option<String> {
        match s {
            StageContent::Text(t) => Some(t),
            _ => None,
        }
    }
    Ok(ConflictSides {
        base: to_opt(base),
        ours: to_opt(ours),
        theirs: to_opt(theirs),
        is_binary,
        too_large: false,
    })
}

/// Writes the resolved content to the worktree and stages it — the standard
/// "mark resolved" step for a text conflict the user edited in place.
#[tauri::command]
pub async fn resolve_conflict(repo: String, path: String, content: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    std::fs::write(repo.join(&path), content).map_err(|e| format!("写入 {path} 失败: {e}"))?;
    run_git(&repo, &["add", "--", &path], None)?;
    Ok(())
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ConflictTake {
    Ours,
    Theirs,
}

/// For true binary conflicts, where both stage 2 and 3 exist as blobs (unlike
/// delete-conflicts, where one side is simply absent — see resolve_conflict_delete).
#[tauri::command]
pub async fn resolve_conflict_binary(repo: String, path: String, take: ConflictTake) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let flag = match take {
        ConflictTake::Ours => "--ours",
        ConflictTake::Theirs => "--theirs",
    };
    run_git(&repo, &["checkout", flag, "--", &path], None)?;
    run_git(&repo, &["add", "--", &path], None)?;
    Ok(())
}

/// For delete/modify conflicts where keeping "the deletion" is the resolution
/// — `checkout --ours/--theirs` has nothing to check out in that case.
#[tauri::command]
pub async fn resolve_conflict_delete(repo: String, path: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    run_git(&repo, &["rm", "-f", "--", &path], None)?;
    Ok(())
}

fn run_git_with_env(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let mut cmd = git_command(repo);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    let child = cmd.spawn().map_err(|e| format!("无法执行 git: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("git 执行失败: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Dispatches to the right `--continue` for whichever operation is in
/// progress. `GIT_EDITOR=true` keeps the default commit message instead of
/// blocking on an editor that has no terminal to attach to.
#[tauri::command]
pub async fn continue_operation(repo: String) -> Result<OpOutcome, String> {
    let repo = PathBuf::from(repo);
    let args: &[&str] = match detect_operation(&repo) {
        RepoOperation::Merge => &["merge", "--continue"],
        RepoOperation::CherryPick => &["cherry-pick", "--continue"],
        RepoOperation::Revert => &["revert", "--continue"],
        RepoOperation::None => return Err("当前没有进行中的合并 / cherry-pick / revert".to_string()),
    };
    let result = run_git_with_env(&repo, args, &[("GIT_EDITOR", "true")]);
    classify_op_result(&repo, result)
}

#[tauri::command]
pub async fn abort_operation(repo: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    let args: &[&str] = match detect_operation(&repo) {
        RepoOperation::Merge => &["merge", "--abort"],
        RepoOperation::CherryPick => &["cherry-pick", "--abort"],
        RepoOperation::Revert => &["revert", "--abort"],
        RepoOperation::None => return Err("当前没有进行中的合并 / cherry-pick / revert".to_string()),
    };
    run_git(&repo, args, None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_basic() {
        let out = "M\0src/a.rs\0A\0new.txt\0D\0gone.txt\0";
        let files = parse_name_status(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].kind, ChangeKind::Modified);
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].kind, ChangeKind::Added);
        assert_eq!(files[2].path, "gone.txt");
        assert_eq!(files[2].kind, ChangeKind::Deleted);
    }

    #[test]
    fn numstat_forms() {
        let out = "3\t1\tsrc/a.rs\0-\t-\tbin.dat\05\t0\t\0old.txt\0new.txt\0";
        let stats = parse_numstat(out);
        assert_eq!(stats.len(), 3);
        assert_eq!(stats[0], ("src/a.rs".into(), Some(3), Some(1)));
        assert_eq!(stats[1], ("bin.dat".into(), None, None)); // binary
        assert_eq!(stats[2], ("new.txt".into(), Some(5), Some(0))); // rename keyed by new path
    }

    #[test]
    fn decoration_parsing() {
        assert_eq!(parse_decorations(""), Vec::<String>::new());
        assert_eq!(parse_decorations("HEAD -> main"), vec!["main"]);
        assert_eq!(
            parse_decorations("HEAD -> main, origin/main"),
            vec!["main", "origin/main"]
        );
        assert_eq!(parse_decorations("tag: v1.0"), vec!["v1.0"]);
        assert_eq!(
            parse_decorations("HEAD -> main, tag: v1.0, origin/main"),
            vec!["main", "v1.0", "origin/main"]
        );
        assert_eq!(parse_decorations("HEAD"), Vec::<String>::new());
    }

    #[test]
    fn name_status_rename_and_copy() {
        let out = "R100\0old name.txt\0new name.txt\0C75\0src.txt\0copy.txt\0T\0link\0";
        let files = parse_name_status(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].kind, ChangeKind::Renamed);
        assert_eq!(files[0].path, "new name.txt");
        assert_eq!(files[0].old_path.as_deref(), Some("old name.txt"));
        assert_eq!(files[1].kind, ChangeKind::Added);
        assert_eq!(files[1].path, "copy.txt");
        assert_eq!(files[2].kind, ChangeKind::Modified); // T -> modified
    }

    #[test]
    fn hunk_header_forms() {
        assert_eq!(
            parse_hunk_header("@@ -1,5 +1,6 @@ fn main() {\n"),
            Some((1, 5, 1, 6))
        );
        assert_eq!(parse_hunk_header("@@ -1 +1,2 @@\n"), Some((1, 1, 1, 2)));
        assert_eq!(parse_hunk_header("@@ -0,0 +1 @@\n"), Some((0, 0, 1, 1)));
        assert_eq!(parse_hunk_header("not a hunk"), None);
    }

    #[test]
    fn diff_split_two_hunks() {
        let diff = "diff --git a/a.txt b/a.txt\n\
                    index 111..222 100644\n\
                    --- a/a.txt\n\
                    +++ b/a.txt\n\
                    @@ -1,3 +1,3 @@\n line1\n-line2\n+LINE2\n line3\n\
                    @@ -8,3 +8,3 @@\n line8\n-line9\n+LINE9\n line10\n\
                    \\ No newline at end of file\n";
        let (header, hunks, binary) = parse_diff(diff);
        assert!(!binary);
        assert!(header.starts_with("diff --git"));
        assert!(header.ends_with("+++ b/a.txt\n"));
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[1].new_start, 8);
        assert!(hunks[1].text.ends_with("\\ No newline at end of file\n"));
        // header + hunk reconstructs a valid standalone patch
        let patch = format!("{}{}", header, hunks[0].text);
        assert!(patch.contains("--- a/a.txt"));
        assert!(patch.contains("-line2"));
        assert!(!patch.contains("line8"));
    }

    #[test]
    fn diff_binary_detected() {
        let diff = "diff --git a/bin.dat b/bin.dat\n\
                    index 111..222 100644\n\
                    Binary files a/bin.dat and b/bin.dat differ\n";
        let (_, hunks, binary) = parse_diff(diff);
        assert!(binary);
        assert!(hunks.is_empty());
    }
}

/// Integration tests against real temporary git repos: exercise every status
/// kind and every revert path exactly the way the frontend drives them.
#[cfg(test)]
mod repo_tests {
    use super::*;

    // The commands are async (so Tauri runs them off the UI thread); these
    // same-named sync wrappers shadow the glob import above so the existing
    // synchronous tests keep calling them unchanged.
    fn bl<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }
    fn open_repo(path: String) -> Result<RepoInfo, String> {
        bl(super::open_repo(path))
    }
    fn get_status(repo: String) -> Result<Vec<FileStatus>, String> {
        bl(super::get_status(repo))
    }
    fn list_files(repo: String, show_ignored: bool) -> Result<Vec<String>, String> {
        bl(super::list_files(repo, show_ignored))
    }
    fn read_file(repo: String, path: String) -> Result<FileContent, String> {
        bl(super::read_file(repo, path))
    }
    fn log_commits(repo: String, skip: u32, count: u32) -> Result<Vec<CommitInfo>, String> {
        bl(super::log_commits(repo, skip, count, None))
    }
    fn log_commits_on(repo: String, skip: u32, count: u32, branch: &str) -> Result<Vec<CommitInfo>, String> {
        bl(super::log_commits(repo, skip, count, Some(branch.to_string())))
    }
    fn commit_files(repo: String, hash: String) -> Result<Vec<FileStatus>, String> {
        bl(super::commit_files(repo, hash))
    }
    fn get_commit_file_diff(
        repo: String,
        hash: String,
        path: String,
        old_path: Option<String>,
        kind: ChangeKind,
    ) -> Result<FileDiff, String> {
        bl(super::get_commit_file_diff(repo, hash, path, old_path, kind))
    }
    fn get_file_diff(
        repo: String,
        path: String,
        old_path: Option<String>,
        kind: ChangeKind,
        staged: bool,
    ) -> Result<FileDiff, String> {
        bl(super::get_file_diff(repo, path, old_path, kind, staged))
    }
    fn stage_file(repo: String, path: String, old_path: Option<String>) -> Result<(), String> {
        bl(super::stage_file(repo, path, old_path))
    }
    fn unstage_file(
        repo: String,
        path: String,
        old_path: Option<String>,
        kind: ChangeKind,
    ) -> Result<(), String> {
        bl(super::unstage_file(repo, path, old_path, kind))
    }
    fn stage_all(repo: String) -> Result<(), String> {
        bl(super::stage_all(repo))
    }
    fn unstage_all(repo: String) -> Result<(), String> {
        bl(super::unstage_all(repo))
    }
    fn stage_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
        bl(super::stage_hunk(repo, path, patch))
    }
    fn unstage_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
        bl(super::unstage_hunk(repo, path, patch))
    }
    fn create_commit(repo: String, message: String, amend: bool) -> Result<(), String> {
        bl(super::create_commit(repo, message, amend))
    }
    fn list_branches(repo: String) -> Result<Vec<BranchInfo>, String> {
        bl(super::list_branches(repo))
    }
    fn create_branch(
        repo: String,
        name: String,
        start_point: Option<String>,
        checkout: bool,
    ) -> Result<(), String> {
        bl(super::create_branch(repo, name, start_point, checkout))
    }
    fn checkout_branch(repo: String, name: String, is_remote: bool) -> Result<(), String> {
        bl(super::checkout_branch(repo, name, is_remote))
    }
    fn delete_branch(repo: String, name: String, force: bool) -> Result<(), String> {
        bl(super::delete_branch(repo, name, force))
    }
    fn rename_branch(repo: String, old: String, new: String) -> Result<(), String> {
        bl(super::rename_branch(repo, old, new))
    }
    fn list_remotes(repo: String) -> Result<Vec<RemoteInfo>, String> {
        bl(super::list_remotes(repo))
    }
    fn fetch_remote(repo: String, remote: Option<String>, prune: bool) -> Result<(), String> {
        bl(super::fetch_remote(repo, remote, prune))
    }
    fn pull_branch(repo: String, remote: Option<String>) -> Result<(), String> {
        bl(super::pull_branch(repo, remote))
    }
    fn push_branch(
        repo: String,
        remote: String,
        branch: String,
        set_upstream: bool,
        force: ForceMode,
    ) -> Result<(), String> {
        bl(super::push_branch(repo, remote, branch, set_upstream, force))
    }
    fn cherry_pick_commit(repo: String, hash: String) -> Result<OpOutcome, String> {
        bl(super::cherry_pick_commit(repo, hash))
    }
    fn revert_commit(repo: String, hash: String) -> Result<OpOutcome, String> {
        bl(super::revert_commit(repo, hash))
    }
    fn reset_to(repo: String, hash: String, mode: ResetMode) -> Result<(), String> {
        bl(super::reset_to(repo, hash, mode))
    }
    fn count_commits_between(repo: String, from: String, to: String) -> Result<u32, String> {
        bl(super::count_commits_between(repo, from, to))
    }
    fn merge_branch(repo: String, source: String, no_ff: bool) -> Result<OpOutcome, String> {
        bl(super::merge_branch(repo, source, no_ff))
    }
    fn get_conflict_sides(repo: String, path: String) -> Result<ConflictSides, String> {
        bl(super::get_conflict_sides(repo, path))
    }
    fn resolve_conflict(repo: String, path: String, content: String) -> Result<(), String> {
        bl(super::resolve_conflict(repo, path, content))
    }
    fn resolve_conflict_binary(repo: String, path: String, take: ConflictTake) -> Result<(), String> {
        bl(super::resolve_conflict_binary(repo, path, take))
    }
    fn resolve_conflict_delete(repo: String, path: String) -> Result<(), String> {
        bl(super::resolve_conflict_delete(repo, path))
    }
    fn continue_operation(repo: String) -> Result<OpOutcome, String> {
        bl(super::continue_operation(repo))
    }
    fn abort_operation(repo: String) -> Result<(), String> {
        bl(super::abort_operation(repo))
    }
    fn revert_file(
        repo: String,
        path: String,
        old_path: Option<String>,
        kind: ChangeKind,
    ) -> Result<(), String> {
        bl(super::revert_file(repo, path, old_path, kind))
    }
    fn revert_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
        bl(super::revert_hunk(repo, path, patch))
    }
    fn revert_all(repo: String) -> Result<(), String> {
        bl(super::revert_all(repo))
    }
    fn search_text(
        repo: String,
        query: String,
        whole_word: bool,
        max: u32,
    ) -> Result<Vec<SearchHit>, String> {
        bl(super::search_text(repo, query, whole_word, max))
    }
    fn create_file(repo: String, path: String) -> Result<(), String> {
        bl(super::create_file(repo, path))
    }
    fn create_dir(repo: String, path: String) -> Result<(), String> {
        bl(super::create_dir(repo, path))
    }
    fn get_console_log(repo: String) -> Result<Vec<ConsoleEntry>, String> {
        bl(super::get_console_log(repo))
    }

    struct TempRepo(PathBuf);

    impl TempRepo {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "ai-diff-test-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            run_git(&dir, &["init", "-q"], None).unwrap();
            run_git(&dir, &["config", "user.email", "t@test"], None).unwrap();
            run_git(&dir, &["config", "user.name", "t"], None).unwrap();
            TempRepo(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn root(&self) -> String {
            self.0.to_string_lossy().to_string()
        }

        fn write(&self, rel: &str, content: &[u8]) {
            std::fs::write(self.0.join(rel), content).unwrap();
        }

        fn commit_all(&self) {
            run_git(&self.0, &["add", "-A"], None).unwrap();
            run_git(&self.0, &["commit", "-qm", "c"], None).unwrap();
        }

        /// Sets up a real both-modified merge conflict on `path`: a base
        /// commit, a diverging `theirs` branch, a diverging commit back on
        /// the starting branch, then `git merge theirs` — expected to fail
        /// and leave MERGE_HEAD + conflict markers in place, exactly like a
        /// real IDE merge flow.
        fn merge_conflict(&self, path: &str, base: &str, ours: &str, theirs: &str) {
            self.write(path, base.as_bytes());
            self.commit_all();
            let start_branch =
                String::from_utf8(run_git(&self.0, &["branch", "--show-current"], None).unwrap())
                    .unwrap()
                    .trim()
                    .to_string();
            run_git(&self.0, &["checkout", "-qb", "theirs"], None).unwrap();
            self.write(path, theirs.as_bytes());
            self.commit_all();
            run_git(&self.0, &["checkout", "-q", &start_branch], None).unwrap();
            self.write(path, ours.as_bytes());
            self.commit_all();
            let _ = run_git(&self.0, &["merge", "theirs"], None); // expected to fail (conflict)
        }

        /// `git status --porcelain` — the strictest cleanliness oracle: also
        /// catches index-vs-worktree drift that `git diff HEAD` cannot see
        /// (e.g. stale eol-convert state causing phantom modifications)
        fn porcelain(&self) -> String {
            String::from_utf8(run_git(&self.0, &["status", "--porcelain"], None).unwrap())
                .unwrap()
        }

        fn find<'a>(files: &'a [FileStatus], path: &str) -> &'a FileStatus {
            files
                .iter()
                .find(|f| f.path == path)
                .unwrap_or_else(|| panic!("{path} not in status"))
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn status_kinds_and_all_revert_paths() {
        let r = TempRepo::new("full");
        // 20 lines so that edits at line 2 and line 16 stay two separate hunks
        // (unified=3 context regions must not touch, or git merges them)
        let base_lines: String = (1..=20).map(|i| format!("line{i}\n")).collect();
        r.write("a.txt", base_lines.as_bytes());
        r.write("b.txt", b"keep me\n");
        r.write("c.txt", b"rename me, keep this content identical\n");
        let bin: Vec<u8> = (0u16..64).map(|i| (i % 256) as u8).collect();
        r.write("bin.dat", &bin);
        r.commit_all();

        // mutate: 2 separate hunks in a.txt, delete, rename, untracked, binary edit, staged add
        let edited = base_lines
            .replace("line2\n", "LINE2-EDIT\n")
            .replace("line16\n", "LINE16-EDIT\n");
        r.write("a.txt", edited.as_bytes());
        std::fs::remove_file(r.path().join("b.txt")).unwrap();
        run_git(r.path(), &["mv", "c.txt", "c-renamed.txt"], None).unwrap();
        r.write("untracked.txt", b"brand new\n");
        let bin2: Vec<u8> = (0u16..64).rev().map(|i| (i % 256) as u8).collect();
        r.write("bin.dat", &bin2);
        r.write("staged.txt", b"staged change\n");
        run_git(r.path(), &["add", "staged.txt"], None).unwrap();

        let files = get_status(r.root()).unwrap();
        assert_eq!(files.len(), 6);
        assert_eq!(TempRepo::find(&files, "a.txt").kind, ChangeKind::Modified);
        assert_eq!(TempRepo::find(&files, "b.txt").kind, ChangeKind::Deleted);
        let renamed = TempRepo::find(&files, "c-renamed.txt");
        assert_eq!(renamed.kind, ChangeKind::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("c.txt"));
        assert_eq!(TempRepo::find(&files, "bin.dat").kind, ChangeKind::Modified);
        assert_eq!(TempRepo::find(&files, "staged.txt").kind, ChangeKind::Added);
        assert_eq!(
            TempRepo::find(&files, "untracked.txt").kind,
            ChangeKind::Untracked
        );

        // binary detection
        let bd = get_file_diff(r.root(), "bin.dat".into(), None, ChangeKind::Modified, false).unwrap();
        assert!(bd.is_binary);
        assert!(bd.hunks.is_empty());

        // hunk revert + offset drift: revert hunk 0, re-fetch, revert remaining hunk
        let d1 = get_file_diff(r.root(), "a.txt".into(), None, ChangeKind::Modified, false).unwrap();
        assert_eq!(d1.hunks.len(), 2);
        let patch1 = format!("{}{}", d1.file_header, d1.hunks[0].text);
        revert_hunk(r.root(), "a.txt".into(), patch1).unwrap();
        let d2 = get_file_diff(r.root(), "a.txt".into(), None, ChangeKind::Modified, false).unwrap();
        assert_eq!(d2.hunks.len(), 1, "one hunk must remain after first revert");
        let patch2 = format!("{}{}", d2.file_header, d2.hunks[0].text);
        revert_hunk(r.root(), "a.txt".into(), patch2).unwrap();
        let files = get_status(r.root()).unwrap();
        assert!(
            !files.iter().any(|f| f.path == "a.txt"),
            "a.txt must be clean after both hunks reverted"
        );

        // file-level reverts
        revert_file(r.root(), "b.txt".into(), None, ChangeKind::Deleted).unwrap();
        assert!(r.path().join("b.txt").is_file());

        revert_file(
            r.root(),
            "c-renamed.txt".into(),
            Some("c.txt".into()),
            ChangeKind::Renamed,
        )
        .unwrap();
        assert!(r.path().join("c.txt").is_file());
        assert!(!r.path().join("c-renamed.txt").exists());

        revert_file(r.root(), "staged.txt".into(), None, ChangeKind::Added).unwrap();
        assert!(!r.path().join("staged.txt").exists());

        revert_file(r.root(), "untracked.txt".into(), None, ChangeKind::Untracked).unwrap();
        assert!(!r.path().join("untracked.txt").exists());

        revert_file(r.root(), "bin.dat".into(), None, ChangeKind::Modified).unwrap();
        assert_eq!(std::fs::read(r.path().join("bin.dat")).unwrap(), bin);

        assert!(get_status(r.root()).unwrap().is_empty());
        assert_eq!(r.porcelain(), "", "git status must be fully clean");
    }

    /// Under autocrlf=false (CRLF stored raw) and autocrlf=true (LF stored,
    /// CRLF worktree) a hunk revert must leave the file clean AND keep CRLF.
    /// Under autocrlf=input LF is the canonical worktree form, so only
    /// cleanliness is asserted there.
    #[test]
    fn crlf_hunk_revert_keeps_line_endings() {
        for autocrlf in ["false", "true", "input"] {
            let r = TempRepo::new(&format!("crlf-{autocrlf}"));
            run_git(r.path(), &["config", "core.autocrlf", autocrlf], None).unwrap();
            r.write("crlf.txt", b"crlf1\r\ncrlf2\r\ncrlf3\r\n");
            r.commit_all();
            r.write("crlf.txt", b"crlf1\r\nCRLF2-EDIT\r\ncrlf3\r\n");

            let d =
                get_file_diff(r.root(), "crlf.txt".into(), None, ChangeKind::Modified, false).unwrap();
            assert_eq!(d.hunks.len(), 1, "autocrlf={autocrlf}");
            let patch = format!("{}{}", d.file_header, d.hunks[0].text);
            revert_hunk(r.root(), "crlf.txt".into(), patch).unwrap();

            assert!(
                get_status(r.root()).unwrap().is_empty(),
                "autocrlf={autocrlf}: git must see the file as clean after CRLF hunk revert"
            );
            assert_eq!(
                r.porcelain(),
                "",
                "autocrlf={autocrlf}: no phantom modification may remain in git status"
            );
            if autocrlf != "input" {
                let bytes = std::fs::read(r.path().join("crlf.txt")).unwrap();
                assert!(
                    bytes.windows(2).any(|w| w == b"\r\n"),
                    "autocrlf={autocrlf}: CRLF endings must survive the revert"
                );
            }
        }
    }

    #[test]
    fn revert_all_spares_ignored_files() {
        let r = TempRepo::new("revertall");
        r.write(".gitignore", b"ignored.txt\n");
        r.write("tracked.txt", b"v1\n");
        r.commit_all();

        r.write("tracked.txt", b"v2\n");
        r.write("untracked.txt", b"junk\n");
        r.write("ignored.txt", b"build artifact\n");
        r.write("staged-add.txt", b"staged\n");
        run_git(r.path(), &["add", "staged-add.txt"], None).unwrap();

        revert_all(r.root()).unwrap();

        assert!(get_status(r.root()).unwrap().is_empty());
        assert_eq!(std::fs::read(r.path().join("tracked.txt")).unwrap(), b"v1\n");
        assert!(!r.path().join("untracked.txt").exists());
        assert!(!r.path().join("staged-add.txt").exists());
        assert!(
            r.path().join("ignored.txt").is_file(),
            "clean -fd (no -x) must not delete .gitignore'd files"
        );
    }

    #[test]
    fn empty_repo_no_commits() {
        let r = TempRepo::new("empty");
        r.write("new.txt", b"hello\n");
        r.write("staged.txt", b"staged\n");
        run_git(r.path(), &["add", "staged.txt"], None).unwrap();

        let info = open_repo(r.root()).unwrap();
        assert!(!info.has_head);

        let files = get_status(r.root()).unwrap();
        assert_eq!(
            TempRepo::find(&files, "staged.txt").kind,
            ChangeKind::Added
        );
        assert_eq!(
            TempRepo::find(&files, "new.txt").kind,
            ChangeKind::Untracked
        );

        let d = get_file_diff(r.root(), "staged.txt".into(), None, ChangeKind::Added, true).unwrap();
        assert_eq!(d.modified.as_deref(), Some("staged\n"));
        assert!(d.original.is_none());

        assert!(revert_all(r.root()).is_err(), "revert_all must refuse without HEAD");

        revert_file(r.root(), "staged.txt".into(), None, ChangeKind::Added).unwrap();
        assert!(!r.path().join("staged.txt").exists());
    }

    #[test]
    fn history_log_files_and_diff() {
        let r = TempRepo::new("history");
        r.write("a.txt", b"one\ntwo\n");
        r.commit_all();
        r.write("a.txt", b"one\nTWO\nthree\n");
        r.write("b.txt", b"new\n");
        r.commit_all();

        let commits = log_commits(r.root(), 0, 10).unwrap();
        assert_eq!(commits.len(), 2);
        assert!(!commits[0].hash.is_empty());
        assert_eq!(commits[0].subject, "c");
        assert_eq!(commits[0].email, "t@test");
        // newest commit: a.txt +2 -1, b.txt +1 => totals +3 -1
        assert_eq!(commits[0].additions, 3);
        assert_eq!(commits[0].deletions, 1);
        assert_eq!(commits[1].additions, 2); // root commit adds a.txt (2 lines)
        assert_eq!(commits[1].deletions, 0);

        // paging
        let page2 = log_commits(r.root(), 1, 10).unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].hash, commits[1].hash);

        // newest commit: a.txt modified (+2 -1), b.txt added
        let files = commit_files(r.root(), commits[0].hash.clone()).unwrap();
        let a = TempRepo::find(&files, "a.txt");
        assert_eq!(a.kind, ChangeKind::Modified);
        assert_eq!(a.additions, Some(2));
        assert_eq!(a.deletions, Some(1));
        assert_eq!(TempRepo::find(&files, "b.txt").kind, ChangeKind::Added);

        let d = get_commit_file_diff(
            r.root(),
            commits[0].hash.clone(),
            "a.txt".into(),
            None,
            ChangeKind::Modified,
        )
        .unwrap();
        assert_eq!(d.original.as_deref(), Some("one\ntwo\n"));
        assert_eq!(d.modified.as_deref(), Some("one\nTWO\nthree\n"));
        assert!(d.hunks.is_empty());

        // root commit diffs against the empty tree
        let root_files = commit_files(r.root(), commits[1].hash.clone()).unwrap();
        assert_eq!(TempRepo::find(&root_files, "a.txt").kind, ChangeKind::Added);

        // working-tree status carries numstat too
        r.write("a.txt", b"one\nTWO\nthree\nfour\n");
        let st = get_status(r.root()).unwrap();
        let a = TempRepo::find(&st, "a.txt");
        assert_eq!(a.additions, Some(1));
        assert_eq!(a.deletions, Some(0));
    }

    #[test]
    fn list_files_and_read_file() {
        let r = TempRepo::new("listing");
        r.write(".gitignore", b"ignored.txt\n");
        r.write("tracked.txt", b"hello\n");
        r.commit_all();
        r.write("untracked.txt", b"new\n");
        r.write("ignored.txt", b"artifact\n");
        let bin: Vec<u8> = vec![0, 1, 2, 3];
        r.write("bin.dat", &bin);

        let files = list_files(r.root(), false).unwrap();
        assert!(files.contains(&"tracked.txt".to_string()));
        assert!(files.contains(&"untracked.txt".to_string()));
        assert!(files.contains(&"bin.dat".to_string()));
        assert!(
            !files.contains(&"ignored.txt".to_string()),
            ".gitignore'd files must not appear in the all-files view"
        );

        // show_ignored=true surfaces the .gitignore'd file too
        let with_ignored = list_files(r.root(), true).unwrap();
        assert!(with_ignored.contains(&"tracked.txt".to_string()));
        assert!(
            with_ignored.contains(&"ignored.txt".to_string()),
            "show_ignored must reveal .gitignore'd files"
        );

        let c = read_file(r.root(), "tracked.txt".into()).unwrap();
        assert_eq!(c.content.as_deref(), Some("hello\n"));
        assert!(!c.is_binary);

        let b = read_file(r.root(), "bin.dat".into()).unwrap();
        assert!(b.is_binary);
        assert!(b.content.is_none());

        assert!(read_file(r.root(), "missing.txt".into()).is_err());
    }

    #[test]
    fn search_text_basics() {
        let r = TempRepo::new("search");
        r.write("a.ts", b"function hello() {}\nconst helloWorld = 1;\n");
        r.commit_all();
        r.write("b.ts", b"hello();\n"); // untracked must be searched too

        let hits = search_text(r.root(), "hello".into(), false, 100).unwrap();
        assert_eq!(hits.len(), 3);

        let word_hits = search_text(r.root(), "hello".into(), true, 100).unwrap();
        assert_eq!(word_hits.len(), 2, "-w must exclude helloWorld");
        assert!(word_hits.iter().any(|h| h.path == "b.ts" && h.line == 1));

        assert!(search_text(r.root(), "nomatch_xyz".into(), false, 100).unwrap().is_empty());
        assert_eq!(search_text(r.root(), "hello".into(), false, 1).unwrap().len(), 1);
    }

    #[test]
    fn open_repo_rejects_non_repo() {
        let dir = std::env::temp_dir().join(format!("ai-diff-norepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(open_repo(dir.to_string_lossy().to_string()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_upstream_reports_none_not_error() {
        let r = TempRepo::new("no-upstream");
        r.write("a.txt", b"hi\n");
        r.commit_all();
        let info = open_repo(r.root()).unwrap();
        assert_eq!(info.upstream, None);
        assert_eq!(info.ahead, None);
        assert_eq!(info.behind, None);
        assert_eq!(info.operation, RepoOperation::None);
    }

    #[test]
    fn ahead_behind_against_upstream() {
        // bare repo as the "remote" — no working tree, only reachable via push/fetch
        let bare_dir = std::env::temp_dir().join(format!("ai-diff-test-bare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bare_dir);
        std::fs::create_dir_all(&bare_dir).unwrap();
        run_git(&bare_dir, &["init", "-q", "--bare"], None).unwrap();
        let bare = bare_dir.to_string_lossy().to_string();

        let local = TempRepo::new("ahead-behind-local");
        local.write("a.txt", b"one\n");
        local.commit_all();
        let branch =
            String::from_utf8(run_git(local.path(), &["branch", "--show-current"], None).unwrap())
                .unwrap()
                .trim()
                .to_string();
        run_git(local.path(), &["remote", "add", "origin", &bare], None).unwrap();
        run_git(local.path(), &["push", "-q", "-u", "origin", &branch], None).unwrap();

        // a second clone pushes a commit `local` doesn't have yet (-> behind=1)
        let other_dir = std::env::temp_dir().join(format!("ai-diff-test-other-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&other_dir);
        run_git(
            &std::env::temp_dir(),
            &["clone", "-q", &bare, other_dir.to_str().unwrap()],
            None,
        )
        .unwrap();
        run_git(&other_dir, &["config", "user.email", "t@test"], None).unwrap();
        run_git(&other_dir, &["config", "user.name", "t"], None).unwrap();
        std::fs::write(other_dir.join("b.txt"), b"other commit\n").unwrap();
        run_git(&other_dir, &["add", "-A"], None).unwrap();
        run_git(&other_dir, &["commit", "-qm", "other"], None).unwrap();
        run_git(&other_dir, &["push", "-q"], None).unwrap();
        let _ = std::fs::remove_dir_all(&other_dir);

        // local commits its own unpushed change (-> ahead=1)
        local.write("c.txt", b"local only\n");
        run_git(local.path(), &["add", "-A"], None).unwrap();
        run_git(local.path(), &["commit", "-qm", "local only"], None).unwrap();

        // fetch (not pull/merge) so the remote-tracking ref advances without
        // touching local HEAD — that's what makes ahead AND behind both nonzero
        run_git(local.path(), &["fetch", "-q", "origin"], None).unwrap();

        let info = open_repo(local.root()).unwrap();
        assert!(info.upstream.is_some());
        assert_eq!(info.ahead, Some(1));
        assert_eq!(info.behind, Some(1));

        let _ = std::fs::remove_dir_all(&bare_dir);
    }

    #[test]
    fn merge_conflict_detected_as_operation_and_conflicted_path() {
        let r = TempRepo::new("merge-conflict");
        r.merge_conflict("a.txt", "base\n", "ours\n", "theirs\n");

        let info = open_repo(r.root()).unwrap();
        assert_eq!(info.operation, RepoOperation::Merge);

        let files = get_status(r.root()).unwrap();
        let conflicted = TempRepo::find(&files, "a.txt");
        assert_eq!(conflicted.kind, ChangeKind::Conflicted);
        assert_eq!(conflicted.conflict, Some(ConflictSide::BothModified));

        // abort restores a clean, non-conflicted state
        run_git(r.path(), &["merge", "--abort"], None).unwrap();
        let info2 = open_repo(r.root()).unwrap();
        assert_eq!(info2.operation, RepoOperation::None);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn stage_unstage_file_round_trip() {
        let r = TempRepo::new("stage-file");
        r.write("a.txt", b"base\n");
        r.commit_all();
        r.write("a.txt", b"changed\n");

        let before = get_status(r.root()).unwrap();
        assert!(!TempRepo::find(&before, "a.txt").staged);

        stage_file(r.root(), "a.txt".into(), None).unwrap();
        let staged = get_status(r.root()).unwrap();
        assert_eq!(staged.len(), 1, "fully staged, no unstaged remainder");
        assert!(TempRepo::find(&staged, "a.txt").staged);

        unstage_file(r.root(), "a.txt".into(), None, ChangeKind::Modified).unwrap();
        let after = get_status(r.root()).unwrap();
        assert_eq!(after.len(), 1);
        assert!(!TempRepo::find(&after, "a.txt").staged);
        assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"changed\n");
    }

    #[test]
    fn stage_and_unstage_rename() {
        let r = TempRepo::new("stage-rename");
        r.write("old.txt", b"content\n");
        r.commit_all();
        // a plain filesystem rename (NOT `git mv`, which stages immediately) so
        // the rename starts out unstaged, matching what `stage_file` needs to handle
        std::fs::rename(r.path().join("old.txt"), r.path().join("new.txt")).unwrap();

        stage_file(r.root(), "new.txt".into(), Some("old.txt".into())).unwrap();
        let files = get_status(r.root()).unwrap();
        let renamed = TempRepo::find(&files, "new.txt");
        assert_eq!(renamed.kind, ChangeKind::Renamed);
        assert!(renamed.staged);

        unstage_file(r.root(), "new.txt".into(), Some("old.txt".into()), ChangeKind::Renamed).unwrap();
        // index-only: worktree still only has new.txt (git mv already moved it on disk)
        assert!(r.path().join("new.txt").is_file());
        assert!(!r.path().join("old.txt").is_file());
        // index now matches HEAD again: old.txt reads as an unstaged deletion,
        // new.txt as untracked — the plain (non-rename) inverse of staging a move
        let after = get_status(r.root()).unwrap();
        assert_eq!(TempRepo::find(&after, "old.txt").kind, ChangeKind::Deleted);
        assert_eq!(TempRepo::find(&after, "new.txt").kind, ChangeKind::Untracked);
    }

    #[test]
    fn partial_stage_shows_twice_then_commit_only_staged_hunk() {
        let r = TempRepo::new("partial-stage");
        let base: String = (1..=20).map(|i| format!("line{i}\n")).collect();
        r.write("a.txt", base.as_bytes());
        r.commit_all();

        // two separate hunks
        let edited = base.replace("line2\n", "LINE2\n").replace("line18\n", "LINE18\n");
        r.write("a.txt", edited.as_bytes());

        let unstaged_diff = get_file_diff(r.root(), "a.txt".into(), None, ChangeKind::Modified, false).unwrap();
        assert_eq!(unstaged_diff.hunks.len(), 2);
        let hunk0_patch = format!("{}{}", unstaged_diff.file_header, unstaged_diff.hunks[0].text);
        stage_hunk(r.root(), "a.txt".into(), hunk0_patch).unwrap();

        let files = get_status(r.root()).unwrap();
        let a_entries: Vec<_> = files.iter().filter(|f| f.path == "a.txt").collect();
        assert_eq!(a_entries.len(), 2, "partially-staged file appears once per side");
        assert!(a_entries.iter().any(|f| f.staged));
        assert!(a_entries.iter().any(|f| !f.staged));

        create_commit(r.root(), "stage hunk 0".into(), false).unwrap();
        // still one unstaged hunk left (line18) — repo isn't clean yet
        let after_commit = get_status(r.root()).unwrap();
        assert_eq!(after_commit.len(), 1);
        assert!(!after_commit[0].staged);

        // read the actual committed tree, not the worktree file — the worktree
        // still carries the unstaged line18 edit on top regardless of what got committed
        let committed = run_git_text(r.path(), &["show", "HEAD:a.txt"]).unwrap();
        assert!(committed.contains("LINE2\n"), "staged hunk must be committed");
        assert!(committed.contains("line18\n"), "unstaged hunk must NOT be committed yet");

        stage_all(r.root()).unwrap();
        create_commit(r.root(), "stage the rest".into(), false).unwrap();
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn unstage_hunk_round_trip() {
        let r = TempRepo::new("unstage-hunk");
        r.write("a.txt", b"one\ntwo\nthree\n");
        r.commit_all();
        r.write("a.txt", b"one\nTWO\nthree\n");
        stage_file(r.root(), "a.txt".into(), None).unwrap();

        let staged_diff = get_file_diff(r.root(), "a.txt".into(), None, ChangeKind::Modified, true).unwrap();
        assert_eq!(staged_diff.hunks.len(), 1);
        let patch = format!("{}{}", staged_diff.file_header, staged_diff.hunks[0].text);
        unstage_hunk(r.root(), "a.txt".into(), patch).unwrap();

        let files = get_status(r.root()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(!files[0].staged, "unstaged after unstage_hunk");
        // worktree untouched by an index-only operation
        assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"one\nTWO\nthree\n");
    }

    #[test]
    fn amend_changes_message_not_tree() {
        let r = TempRepo::new("amend");
        r.write("a.txt", b"content\n");
        r.commit_all();
        let before = log_commits(r.root(), 0, 1).unwrap();
        let tree_before =
            run_git_text(r.path(), &["rev-parse", "HEAD^{tree}"]).unwrap();

        create_commit(r.root(), "amended message".into(), true).unwrap();

        let after = log_commits(r.root(), 0, 1).unwrap();
        let tree_after = run_git_text(r.path(), &["rev-parse", "HEAD^{tree}"]).unwrap();
        assert_eq!(after[0].subject, "amended message");
        assert_ne!(after[0].subject, before[0].subject);
        assert_eq!(tree_before, tree_after, "amend must not change the tree");
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn stage_all_unstage_all_round_trip() {
        let r = TempRepo::new("stage-all");
        r.write("a.txt", b"base\n");
        r.commit_all();
        r.write("a.txt", b"changed\n");
        r.write("b.txt", b"new file\n");

        stage_all(r.root()).unwrap();
        let staged = get_status(r.root()).unwrap();
        assert_eq!(staged.len(), 2);
        assert!(staged.iter().all(|f| f.staged));

        unstage_all(r.root()).unwrap();
        let unstaged = get_status(r.root()).unwrap();
        assert_eq!(unstaged.len(), 2);
        assert!(unstaged.iter().all(|f| !f.staged));
    }

    #[test]
    fn create_commit_rejects_empty_message() {
        let r = TempRepo::new("empty-message");
        r.write("a.txt", b"content\n");
        stage_all(r.root()).unwrap();
        assert!(create_commit(r.root(), "   ".into(), false).is_err());
    }

    fn current_branch(r: &TempRepo) -> String {
        String::from_utf8(run_git(r.path(), &["branch", "--show-current"], None).unwrap())
            .unwrap()
            .trim()
            .to_string()
    }

    #[test]
    fn create_checkout_delete_branch_round_trip() {
        let r = TempRepo::new("branch-lifecycle");
        r.write("a.txt", b"one\n");
        r.commit_all();
        let start = current_branch(&r);

        create_branch(r.root(), "feature".into(), None, true).unwrap();
        assert_eq!(current_branch(&r), "feature");

        let branches = list_branches(r.root()).unwrap();
        assert!(branches.iter().any(|b| b.name == "feature" && b.is_current && !b.is_remote));
        assert!(branches.iter().any(|b| b.name == start && !b.is_current));

        checkout_branch(r.root(), start.clone(), false).unwrap();
        assert_eq!(current_branch(&r), start);

        delete_branch(r.root(), "feature".into(), false).unwrap();
        let after = list_branches(r.root()).unwrap();
        assert!(!after.iter().any(|b| b.name == "feature"));
    }

    #[test]
    fn delete_unmerged_branch_requires_force() {
        let r = TempRepo::new("branch-unmerged");
        r.write("a.txt", b"one\n");
        r.commit_all();
        let start = current_branch(&r);

        create_branch(r.root(), "unmerged".into(), None, true).unwrap();
        r.write("a.txt", b"two\n");
        r.commit_all();
        checkout_branch(r.root(), start, false).unwrap();

        assert!(
            delete_branch(r.root(), "unmerged".into(), false).is_err(),
            "plain -d must refuse to drop unmerged commits"
        );
        delete_branch(r.root(), "unmerged".into(), true).unwrap();
        let after = list_branches(r.root()).unwrap();
        assert!(!after.iter().any(|b| b.name == "unmerged"));
    }

    #[test]
    fn checkout_blocked_by_dirty_worktree() {
        let r = TempRepo::new("branch-dirty-checkout");
        r.write("a.txt", b"one\n");
        r.commit_all();
        let start = current_branch(&r);

        // "other" must commit a DIFFERENT a.txt, or its content wouldn't
        // actually conflict with the dirty change below and switch would
        // succeed (git only blocks when checkout would clobber the diff)
        create_branch(r.root(), "other".into(), None, true).unwrap();
        r.write("a.txt", b"other-version\n");
        r.commit_all();
        checkout_branch(r.root(), start, false).unwrap();

        r.write("a.txt", b"dirty, uncommitted\n");
        assert!(
            checkout_branch(r.root(), "other".into(), false).is_err(),
            "git itself must refuse to switch over conflicting dirty changes"
        );
    }

    #[test]
    fn rename_branch_works() {
        let r = TempRepo::new("branch-rename");
        r.write("a.txt", b"one\n");
        r.commit_all();
        let start = current_branch(&r);
        rename_branch(r.root(), start.clone(), "renamed".into()).unwrap();
        assert_eq!(current_branch(&r), "renamed");
        let branches = list_branches(r.root()).unwrap();
        assert!(!branches.iter().any(|b| b.name == start));
        assert!(branches.iter().any(|b| b.name == "renamed" && b.is_current));
    }

    #[test]
    fn branch_names_with_slashes_parse_correctly() {
        let r = TempRepo::new("branch-slash");
        r.write("a.txt", b"one\n");
        r.commit_all();
        create_branch(r.root(), "feature/foo/bar".into(), None, false).unwrap();
        let branches = list_branches(r.root()).unwrap();
        assert!(branches.iter().any(|b| b.name == "feature/foo/bar" && !b.is_remote));
    }

    #[test]
    fn checkout_remote_branch_creates_local_tracking_branch() {
        let bare_dir = std::env::temp_dir().join(format!("ai-diff-test-bare-branch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bare_dir);
        std::fs::create_dir_all(&bare_dir).unwrap();
        run_git(&bare_dir, &["init", "-q", "--bare"], None).unwrap();
        let bare = bare_dir.to_string_lossy().to_string();

        let seed = TempRepo::new("branch-remote-seed");
        seed.write("a.txt", b"one\n");
        seed.commit_all();
        let default_branch = current_branch(&seed);
        run_git(seed.path(), &["remote", "add", "origin", &bare], None).unwrap();
        run_git(seed.path(), &["push", "-q", "-u", "origin", &default_branch], None).unwrap();
        // a second branch the clone does NOT auto-checkout (unlike the default
        // branch, which `git clone` already creates a local tracking branch
        // for) — this is the one we actually test checkout_branch against
        run_git(seed.path(), &["switch", "-c", "feature"], None).unwrap();
        seed.write("b.txt", b"feature work\n");
        seed.commit_all();
        run_git(seed.path(), &["push", "-q", "-u", "origin", "feature"], None).unwrap();

        let local_dir = std::env::temp_dir().join(format!("ai-diff-test-clone-branch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&local_dir);
        run_git(&std::env::temp_dir(), &["clone", "-q", &bare, local_dir.to_str().unwrap()], None).unwrap();
        run_git(&local_dir, &["config", "user.email", "t@test"], None).unwrap();
        run_git(&local_dir, &["config", "user.name", "t"], None).unwrap();

        let remote_name = "origin/feature".to_string();
        let branches = list_branches(local_dir.to_string_lossy().to_string()).unwrap();
        assert!(branches.iter().any(|b| b.name == remote_name && b.is_remote));
        assert!(!branches.iter().any(|b| b.name == "feature" && !b.is_remote));

        checkout_branch(local_dir.to_string_lossy().to_string(), remote_name, true).unwrap();
        let local_branch = String::from_utf8(
            run_git(&local_dir, &["branch", "--show-current"], None).unwrap(),
        )
        .unwrap()
        .trim()
        .to_string();
        assert_eq!(local_branch, "feature");

        let _ = std::fs::remove_dir_all(&bare_dir);
        let _ = std::fs::remove_dir_all(&local_dir);
    }

    /// Bare repo as "remote" + a local clone with a commit already pushed and
    /// upstream tracking configured. Returns (bare_dir, local TempRepo, branch).
    fn bare_remote_with_clone(tag: &str) -> (PathBuf, TempRepo, String) {
        let bare_dir = std::env::temp_dir().join(format!("ai-diff-test-bare-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bare_dir);
        std::fs::create_dir_all(&bare_dir).unwrap();
        run_git(&bare_dir, &["init", "-q", "--bare"], None).unwrap();
        let bare = bare_dir.to_string_lossy().to_string();

        let seed = TempRepo::new(&format!("{tag}-seed"));
        seed.write("a.txt", b"one\n");
        seed.commit_all();
        let branch = current_branch(&seed);
        run_git(seed.path(), &["push", "-q", &bare, &format!("HEAD:{branch}")], None).unwrap();

        let local = TempRepo::new(&format!("{tag}-local"));
        run_git(local.path(), &["remote", "add", "origin", &bare], None).unwrap();
        run_git(local.path(), &["fetch", "-q", "origin"], None).unwrap();
        run_git(local.path(), &["checkout", "-q", "-b", &branch, "--track", &format!("origin/{branch}")], None)
            .unwrap();
        (bare_dir, local, branch)
    }

    #[test]
    fn fetch_updates_remote_tracking_ref_without_touching_head() {
        let (bare_dir, local, branch) = bare_remote_with_clone("fetch");

        // a second clone pushes a commit the first clone doesn't have yet
        let other_dir = std::env::temp_dir().join(format!("ai-diff-test-other-fetch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&other_dir);
        run_git(&std::env::temp_dir(), &["clone", "-q", bare_dir.to_str().unwrap(), other_dir.to_str().unwrap()], None).unwrap();
        run_git(&other_dir, &["config", "user.email", "t@test"], None).unwrap();
        run_git(&other_dir, &["config", "user.name", "t"], None).unwrap();
        std::fs::write(other_dir.join("b.txt"), b"other\n").unwrap();
        run_git(&other_dir, &["add", "-A"], None).unwrap();
        run_git(&other_dir, &["commit", "-qm", "other"], None).unwrap();
        run_git(&other_dir, &["push", "-q"], None).unwrap();

        let head_before = run_git_text(local.path(), &["rev-parse", "HEAD"]).unwrap();
        fetch_remote(local.root(), None, false).unwrap();
        let head_after = run_git_text(local.path(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head_before, head_after, "fetch must never move local HEAD");

        let remote_ref = run_git_text(local.path(), &["rev-parse", &format!("origin/{branch}")]).unwrap();
        let other_head = run_git_text(&other_dir, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(remote_ref, other_head, "remote-tracking ref must reflect the fetched commit");

        let _ = std::fs::remove_dir_all(&bare_dir);
        let _ = std::fs::remove_dir_all(&other_dir);
    }

    #[test]
    fn pull_fast_forwards_local_branch() {
        let (bare_dir, local, branch) = bare_remote_with_clone("pull");

        let other_dir = std::env::temp_dir().join(format!("ai-diff-test-other-pull-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&other_dir);
        run_git(&std::env::temp_dir(), &["clone", "-q", bare_dir.to_str().unwrap(), other_dir.to_str().unwrap()], None).unwrap();
        run_git(&other_dir, &["config", "user.email", "t@test"], None).unwrap();
        run_git(&other_dir, &["config", "user.name", "t"], None).unwrap();
        std::fs::write(other_dir.join("b.txt"), b"other\n").unwrap();
        run_git(&other_dir, &["add", "-A"], None).unwrap();
        run_git(&other_dir, &["commit", "-qm", "other"], None).unwrap();
        run_git(&other_dir, &["push", "-q"], None).unwrap();

        pull_branch(local.root(), None).unwrap();
        assert!(local.path().join("b.txt").is_file(), "pull must merge the remote commit into the worktree");
        assert_eq!(local.porcelain(), "");
        let _ = branch;

        let _ = std::fs::remove_dir_all(&bare_dir);
        let _ = std::fs::remove_dir_all(&other_dir);
    }

    #[test]
    fn push_uploads_commit_to_remote() {
        let (bare_dir, local, branch) = bare_remote_with_clone("push");

        local.write("c.txt", b"local work\n");
        run_git(local.path(), &["add", "-A"], None).unwrap();
        run_git(local.path(), &["commit", "-qm", "local work"], None).unwrap();
        push_branch(local.root(), "origin".into(), branch.clone(), false, ForceMode::None).unwrap();

        let bare_head = run_git_text(&bare_dir, &["rev-parse", &branch]).unwrap();
        let local_head = run_git_text(local.path(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(bare_head, local_head, "pushed commit must land on the bare remote");

        let _ = std::fs::remove_dir_all(&bare_dir);
    }

    #[test]
    fn push_rejected_on_diverge_then_force_with_lease_succeeds() {
        let (bare_dir, local, branch) = bare_remote_with_clone("force-push");

        // someone else pushes to the remote first, diverging history
        let other_dir = std::env::temp_dir().join(format!("ai-diff-test-other-force-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&other_dir);
        run_git(&std::env::temp_dir(), &["clone", "-q", bare_dir.to_str().unwrap(), other_dir.to_str().unwrap()], None).unwrap();
        run_git(&other_dir, &["config", "user.email", "t@test"], None).unwrap();
        run_git(&other_dir, &["config", "user.name", "t"], None).unwrap();
        std::fs::write(other_dir.join("b.txt"), b"other\n").unwrap();
        run_git(&other_dir, &["add", "-A"], None).unwrap();
        run_git(&other_dir, &["commit", "-qm", "other"], None).unwrap();
        run_git(&other_dir, &["push", "-q"], None).unwrap();

        // local also commits, unaware of the remote's new commit
        local.write("c.txt", b"local, conflicting history\n");
        run_git(local.path(), &["add", "-A"], None).unwrap();
        run_git(local.path(), &["commit", "-qm", "local diverged"], None).unwrap();

        assert!(
            push_branch(local.root(), "origin".into(), branch.clone(), false, ForceMode::None).is_err(),
            "a plain push must be rejected on non-fast-forward"
        );
        // --force-with-lease refuses on stale remote-tracking knowledge (by
        // design — that's the whole safety property) so a real fetch has to
        // happen first, exactly like a real user would after a rejected push
        fetch_remote(local.root(), None, false).unwrap();
        push_branch(local.root(), "origin".into(), branch, false, ForceMode::Lease).unwrap();

        let bare_head = run_git_text(&bare_dir, &["rev-parse", "HEAD"]).unwrap();
        let local_head = run_git_text(local.path(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(bare_head, local_head, "force-with-lease push must win and update the remote");

        let _ = std::fs::remove_dir_all(&bare_dir);
        let _ = std::fs::remove_dir_all(&other_dir);
    }

    #[test]
    fn list_remotes_reports_name_and_url() {
        let (bare_dir, local, _branch) = bare_remote_with_clone("list-remotes");
        let remotes = list_remotes(local.root()).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].url, bare_dir.to_string_lossy());
        let _ = std::fs::remove_dir_all(&bare_dir);
    }

    #[test]
    fn cherry_pick_applies_cleanly() {
        let r = TempRepo::new("cherry-clean");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("b.txt", b"feature file\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "add b"], None).unwrap();
        let feature_commit = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        checkout_branch(r.root(), start, false).unwrap();

        let outcome = cherry_pick_commit(r.root(), feature_commit).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        assert!(r.path().join("b.txt").is_file());
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn cherry_pick_conflict_then_abort() {
        let r = TempRepo::new("cherry-conflict");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("a.txt", b"feature-version\n");
        r.commit_all();
        let feature_commit = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        checkout_branch(r.root(), start, false).unwrap();
        r.write("a.txt", b"main-version\n");
        r.commit_all();

        let outcome = cherry_pick_commit(r.root(), feature_commit).unwrap();
        assert_eq!(outcome, OpOutcome::Conflict);
        let info = open_repo(r.root()).unwrap();
        assert_eq!(info.operation, RepoOperation::CherryPick);

        run_git(r.path(), &["cherry-pick", "--abort"], None).unwrap();
        let info2 = open_repo(r.root()).unwrap();
        assert_eq!(info2.operation, RepoOperation::None);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn revert_commit_applies_cleanly() {
        let r = TempRepo::new("revert-clean");
        r.write("a.txt", b"base\n");
        r.commit_all();
        r.write("a.txt", b"changed\n");
        r.commit_all();
        let commit_to_revert = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();

        let outcome = revert_commit(r.root(), commit_to_revert).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"base\n");
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn revert_commit_conflict_then_abort() {
        let r = TempRepo::new("revert-conflict");
        r.write("a.txt", b"orig\n");
        r.commit_all();
        r.write("a.txt", b"v1\n");
        r.commit_all();
        let first_edit = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        r.write("a.txt", b"v2\n");
        r.commit_all();

        let outcome = revert_commit(r.root(), first_edit).unwrap();
        assert_eq!(outcome, OpOutcome::Conflict);
        let info = open_repo(r.root()).unwrap();
        assert_eq!(info.operation, RepoOperation::Revert);

        run_git(r.path(), &["revert", "--abort"], None).unwrap();
        let info2 = open_repo(r.root()).unwrap();
        assert_eq!(info2.operation, RepoOperation::None);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn reset_soft_mixed_hard_semantics() {
        // soft: HEAD moves, but index AND worktree keep the newer content staged
        {
            let r = TempRepo::new("reset-soft");
            r.write("a.txt", b"v1\n");
            r.commit_all();
            let first = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
            r.write("a.txt", b"v2\n");
            r.commit_all();
            reset_to(r.root(), first.clone(), ResetMode::Soft).unwrap();
            assert_eq!(run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim(), first);
            assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"v2\n", "worktree keeps the newer content");
            let status = r.porcelain();
            assert!(status.starts_with("M "), "soft reset must leave the diff staged: {status:?}");
        }
        // mixed: HEAD moves, worktree keeps content, index un-stages it
        {
            let r = TempRepo::new("reset-mixed");
            r.write("a.txt", b"v1\n");
            r.commit_all();
            let first = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
            r.write("a.txt", b"v2\n");
            r.commit_all();
            reset_to(r.root(), first.clone(), ResetMode::Mixed).unwrap();
            assert_eq!(run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim(), first);
            assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"v2\n");
            let status = r.porcelain();
            assert!(status.starts_with(" M"), "mixed reset must leave the diff unstaged: {status:?}");
        }
        // hard: HEAD moves, index AND worktree revert to match it exactly
        {
            let r = TempRepo::new("reset-hard");
            r.write("a.txt", b"v1\n");
            r.commit_all();
            let first = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
            r.write("a.txt", b"v2\n");
            r.commit_all();
            reset_to(r.root(), first.clone(), ResetMode::Hard).unwrap();
            assert_eq!(run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim(), first);
            assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"v1\n");
            assert_eq!(r.porcelain(), "");
        }
    }

    #[test]
    fn count_commits_between_works() {
        let r = TempRepo::new("count-commits");
        r.write("a.txt", b"1\n");
        r.commit_all();
        let first = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        r.write("a.txt", b"2\n");
        r.commit_all();
        r.write("a.txt", b"3\n");
        r.commit_all();
        let head = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        let n = count_commits_between(r.root(), first, head).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn log_commits_reports_parents() {
        let r = TempRepo::new("parents");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("b.txt", b"feature\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "feature commit"], None).unwrap();
        checkout_branch(r.root(), start.clone(), false).unwrap();
        run_git(r.path(), &["merge", "--no-ff", "-m", "merge feature", "feature"], None).unwrap();

        let commits = log_commits(r.root(), 0, 10).unwrap();
        assert_eq!(commits[0].parents.len(), 2, "merge commit must report 2 parents");
        assert_eq!(commits.last().unwrap().parents.len(), 0, "root commit has no parents");

        assert!(
            commits[0].refs.iter().any(|name| name == &start),
            "the merge commit (HEAD) should be decorated with the current branch name: {:?}",
            commits[0].refs
        );
        let feature_commit = commits.iter().find(|c| c.subject == "feature commit").unwrap();
        assert!(
            feature_commit.refs.iter().any(|name| name == "feature"),
            "the feature branch's tip commit should be decorated with its branch name: {:?}",
            feature_commit.refs
        );
        assert!(
            commits.last().unwrap().refs.is_empty(),
            "the root commit has no ref pointing at it directly"
        );
    }

    #[test]
    fn log_commits_scoped_to_branch() {
        let r = TempRepo::new("branch-scope");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("b.txt", b"feature\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "feature commit"], None).unwrap();
        checkout_branch(r.root(), start.clone(), false).unwrap();
        r.write("c.txt", b"main only\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "main-only commit"], None).unwrap();

        let feature_log = log_commits_on(r.root(), 0, 10, "feature").unwrap();
        assert!(feature_log.iter().any(|c| c.subject == "feature commit"));
        assert!(!feature_log.iter().any(|c| c.subject == "main-only commit"));

        let main_log = log_commits_on(r.root(), 0, 10, &start).unwrap();
        assert!(main_log.iter().any(|c| c.subject == "main-only commit"));
        assert!(!main_log.iter().any(|c| c.subject == "feature commit"));

        // no branch filter (None) logs HEAD, which is `start` right now
        let head_log = log_commits(r.root(), 0, 10).unwrap();
        assert_eq!(head_log.len(), main_log.len());
    }

    #[test]
    fn merge_fast_forward_applies_cleanly() {
        let r = TempRepo::new("merge-ff");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("b.txt", b"feature\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "feature work"], None).unwrap();
        checkout_branch(r.root(), start, false).unwrap();

        let outcome = merge_branch(r.root(), "feature".into(), false).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        assert!(r.path().join("b.txt").is_file());
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn merge_no_ff_creates_merge_commit() {
        let r = TempRepo::new("merge-noff");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("b.txt", b"feature\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "feature work"], None).unwrap();
        checkout_branch(r.root(), start, false).unwrap();

        let outcome = merge_branch(r.root(), "feature".into(), true).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        let parents = run_git_text(r.path(), &["rev-parse", "HEAD^1", "HEAD^2"]);
        assert!(parents.is_ok(), "--no-ff must create an actual 2-parent merge commit");
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn merge_conflict_full_resolve_continue_cycle() {
        let r = TempRepo::new("merge-resolve");
        r.merge_conflict("a.txt", "base\n", "ours\n", "theirs\n");
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::Merge);

        let sides = get_conflict_sides(r.root(), "a.txt".into()).unwrap();
        assert_eq!(sides.base.as_deref(), Some("base\n"));
        assert_eq!(sides.ours.as_deref(), Some("ours\n"));
        assert_eq!(sides.theirs.as_deref(), Some("theirs\n"));
        assert!(!sides.is_binary && !sides.too_large);

        resolve_conflict(r.root(), "a.txt".into(), "resolved\n".into()).unwrap();
        let files = get_status(r.root()).unwrap();
        assert!(
            !files.iter().any(|f| f.kind == ChangeKind::Conflicted),
            "no more conflicted entries once resolved and staged"
        );
        assert!(TempRepo::find(&files, "a.txt").staged, "resolved content is staged, ready for continue");

        let outcome = continue_operation(r.root()).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::None);
        assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"resolved\n");
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn cherry_pick_conflict_resolve_continue() {
        let r = TempRepo::new("cherry-resolve");
        r.write("a.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("a.txt", b"feature-version\n");
        r.commit_all();
        let feature_commit = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        checkout_branch(r.root(), start, false).unwrap();
        r.write("a.txt", b"main-version\n");
        r.commit_all();

        assert_eq!(cherry_pick_commit(r.root(), feature_commit).unwrap(), OpOutcome::Conflict);
        resolve_conflict(r.root(), "a.txt".into(), "resolved\n".into()).unwrap();
        let outcome = continue_operation(r.root()).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::None);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn revert_conflict_resolve_continue() {
        let r = TempRepo::new("revert-resolve");
        r.write("a.txt", b"orig\n");
        r.commit_all();
        r.write("a.txt", b"v1\n");
        r.commit_all();
        let first_edit = run_git_text(r.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        r.write("a.txt", b"v2\n");
        r.commit_all();

        assert_eq!(revert_commit(r.root(), first_edit).unwrap(), OpOutcome::Conflict);
        resolve_conflict(r.root(), "a.txt".into(), "resolved\n".into()).unwrap();
        let outcome = continue_operation(r.root()).unwrap();
        assert_eq!(outcome, OpOutcome::Applied);
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::None);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn abort_operation_restores_clean_state() {
        let r = TempRepo::new("abort-op");
        r.merge_conflict("a.txt", "base\n", "ours\n", "theirs\n");
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::Merge);

        abort_operation(r.root()).unwrap();
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::None);
        assert_eq!(r.porcelain(), "");
        assert_eq!(std::fs::read(r.path().join("a.txt")).unwrap(), b"ours\n");
    }

    #[test]
    fn add_add_conflict_has_no_base() {
        let r = TempRepo::new("add-add-conflict");
        r.write("base.txt", b"base\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("new.txt", b"feature version\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "add new (feature)"], None).unwrap();
        checkout_branch(r.root(), start, false).unwrap();
        r.write("new.txt", b"main version\n");
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "add new (main)"], None).unwrap();

        let _ = run_git(r.path(), &["merge", "feature"], None); // expected to conflict
        assert_eq!(open_repo(r.root()).unwrap().operation, RepoOperation::Merge);
        let files = get_status(r.root()).unwrap();
        let conflicted = TempRepo::find(&files, "new.txt");
        assert_eq!(conflicted.conflict, Some(ConflictSide::BothAdded));

        let sides = get_conflict_sides(r.root(), "new.txt".into()).unwrap();
        assert!(sides.base.is_none(), "add/add conflicts have no common-ancestor version");
        assert_eq!(sides.ours.as_deref(), Some("main version\n"));
        assert_eq!(sides.theirs.as_deref(), Some("feature version\n"));

        resolve_conflict(r.root(), "new.txt".into(), "merged\n".into()).unwrap();
        assert_eq!(continue_operation(r.root()).unwrap(), OpOutcome::Applied);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn delete_modify_conflict_keep_deletion() {
        let r = TempRepo::new("delete-modify-conflict");
        r.write("a.txt", b"base\n");
        r.write("keep.txt", b"keep\n");
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("a.txt", b"modified on feature\n");
        r.commit_all();
        checkout_branch(r.root(), start, false).unwrap();
        std::fs::remove_file(r.path().join("a.txt")).unwrap();
        run_git(r.path(), &["add", "-A"], None).unwrap();
        run_git(r.path(), &["commit", "-qm", "delete a.txt on main"], None).unwrap();

        let _ = run_git(r.path(), &["merge", "feature"], None); // expected to conflict
        let files = get_status(r.root()).unwrap();
        let conflicted = TempRepo::find(&files, "a.txt");
        assert_eq!(conflicted.conflict, Some(ConflictSide::DeletedByUs));

        let sides = get_conflict_sides(r.root(), "a.txt".into()).unwrap();
        assert!(sides.ours.is_none(), "deleted-by-us: no stage 2 version");
        assert_eq!(sides.theirs.as_deref(), Some("modified on feature\n"));

        // "keep mine" = confirm the deletion
        resolve_conflict_delete(r.root(), "a.txt".into()).unwrap();
        assert_eq!(continue_operation(r.root()).unwrap(), OpOutcome::Applied);
        assert!(!r.path().join("a.txt").exists());
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn binary_conflict_resolution_keeps_chosen_side() {
        let r = TempRepo::new("binary-conflict");
        let ours_bytes: Vec<u8> = vec![0u8, 1, 2, 3, 255];
        let theirs_bytes: Vec<u8> = vec![255u8, 254, 253, 0];
        r.write("bin.dat", &[0u8, 9, 9]);
        r.commit_all();
        let start = current_branch(&r);
        create_branch(r.root(), "feature".into(), None, true).unwrap();
        r.write("bin.dat", &theirs_bytes);
        r.commit_all();
        checkout_branch(r.root(), start, false).unwrap();
        r.write("bin.dat", &ours_bytes);
        r.commit_all();

        let _ = run_git(r.path(), &["merge", "feature"], None); // expected to conflict
        let sides = get_conflict_sides(r.root(), "bin.dat".into()).unwrap();
        assert!(sides.is_binary);

        resolve_conflict_binary(r.root(), "bin.dat".into(), ConflictTake::Ours).unwrap();
        assert_eq!(continue_operation(r.root()).unwrap(), OpOutcome::Applied);
        assert_eq!(std::fs::read(r.path().join("bin.dat")).unwrap(), ours_bytes);
        assert_eq!(r.porcelain(), "");
    }

    #[test]
    fn create_file_and_dir() {
        let r = TempRepo::new("create-entries");
        r.write("seed.txt", b"seed\n");
        r.commit_all();

        create_file(r.root(), "new.txt".into()).unwrap();
        assert!(r.path().join("new.txt").exists());
        assert_eq!(std::fs::read(r.path().join("new.txt")).unwrap(), b"");

        // parent directories are created as needed
        create_file(r.root(), "nested/dir/file.txt".into()).unwrap();
        assert!(r.path().join("nested/dir/file.txt").exists());

        create_dir(r.root(), "empty/subdir".into()).unwrap();
        assert!(r.path().join("empty/subdir").is_dir());

        // creating on top of an existing path is an error, not a silent overwrite
        assert!(create_file(r.root(), "new.txt".into()).is_err());
        assert!(create_dir(r.root(), "empty/subdir".into()).is_err());

        // new files are untracked, never auto-staged
        let files = list_files(r.root(), false).unwrap();
        assert!(files.contains(&"new.txt".to_string()));
        assert!(files.contains(&"nested/dir/file.txt".to_string()));
        let status = get_status(r.root()).unwrap();
        assert!(status.iter().all(|f| f.kind != ChangeKind::Added || f.path != "new.txt"));
    }

    #[test]
    fn console_log_records_git_invocations_per_repo() {
        let a = TempRepo::new("console-a");
        let b = TempRepo::new("console-b");
        a.write("seed.txt", b"seed\n");
        a.commit_all();
        b.write("seed.txt", b"seed\n");
        b.commit_all();

        let log_a = get_console_log(a.root()).unwrap();
        assert!(!log_a.is_empty(), "commands run against repo a should be recorded");
        assert!(log_a.iter().all(|e| e.root == a.root()), "must not leak entries from other repos");
        assert!(log_a.iter().any(|e| e.args.contains("commit")));
        assert!(log_a.iter().all(|e| e.ok));

        let log_b = get_console_log(b.root()).unwrap();
        assert!(log_b.iter().all(|e| e.root == b.root()));

        // a failing command is recorded too, with ok=false
        let _ = checkout_branch(a.root(), "does-not-exist".into(), false);
        let log_a2 = get_console_log(a.root()).unwrap();
        assert!(log_a2.iter().any(|e| !e.ok));
    }
}
