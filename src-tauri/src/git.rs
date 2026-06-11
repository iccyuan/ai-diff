use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// git's well-known empty tree object; used as the diff base in repos with no commits
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub root: String,
    pub branch: Option<String>,
    pub has_head: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

#[derive(Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
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

fn git_command(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo);
    cmd.args(["-c", "core.quotepath=off"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

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
                    });
                } else {
                    files.push(FileStatus {
                        path: new.to_string(),
                        old_path: None,
                        kind: ChangeKind::Added,
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
                });
            }
        }
    }
    files
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

#[tauri::command]
pub fn open_repo(path: String) -> Result<RepoInfo, String> {
    let p = PathBuf::from(&path);
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
    Ok(RepoInfo {
        root,
        branch,
        has_head,
    })
}

#[tauri::command]
pub fn get_status(repo: String) -> Result<Vec<FileStatus>, String> {
    let repo = PathBuf::from(repo);
    let base = base_ref(&repo);
    let tracked = run_git_text(
        &repo,
        &[
            "diff",
            &base,
            "--name-status",
            "-z",
            "-M",
            "--no-color",
            "--no-ext-diff",
        ],
    )?;
    let mut files = parse_name_status(&tracked);
    let untracked = run_git_text(&repo, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for p in untracked.split('\0').filter(|s| !s.is_empty()) {
        files.push(FileStatus {
            path: p.to_string(),
            old_path: None,
            kind: ChangeKind::Untracked,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[tauri::command]
pub fn get_file_diff(
    repo: String,
    path: String,
    old_path: Option<String>,
    kind: ChangeKind,
) -> Result<FileDiff, String> {
    let repo = PathBuf::from(repo);

    if kind == ChangeKind::Untracked {
        return untracked_diff(&repo, &path);
    }

    let base = base_ref(&repo);
    let wt_path = repo.join(&path);
    // HEAD-side path: for renames the blob lives at the old path
    let head_rel = old_path.clone().unwrap_or_else(|| path.clone());

    if kind != ChangeKind::Deleted {
        if let Ok(meta) = std::fs::metadata(&wt_path) {
            if meta.len() > MAX_FILE_SIZE {
                return Ok(FileDiff::too_large());
            }
        }
    }
    if kind != ChangeKind::Added {
        let spec = format!("{base}:{head_rel}");
        if let Ok(sz) = run_git_text(&repo, &["cat-file", "-s", &spec]) {
            if sz.trim().parse::<u64>().unwrap_or(0) > MAX_FILE_SIZE {
                return Ok(FileDiff::too_large());
            }
        }
    }

    let mut args: Vec<String> = vec![
        "diff".into(),
        base.clone(),
        "--no-color".into(),
        "--no-ext-diff".into(),
        "--unified=3".into(),
        "-M".into(),
        "--".into(),
    ];
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
        let spec = format!("{base}:{head_rel}");
        match String::from_utf8(run_git(&repo, &["show", &spec], None)?) {
            Ok(s) => original = Some(s),
            Err(_) => is_binary = true,
        }
    }
    if !is_binary && kind != ChangeKind::Deleted {
        let bytes = std::fs::read(&wt_path).map_err(|e| format!("无法读取 {path}: {e}"))?;
        match String::from_utf8(bytes) {
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
        file_header,
        hunks,
    })
}

#[tauri::command]
pub fn revert_file(
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
    }
    Ok(())
}

#[tauri::command]
pub fn revert_hunk(repo: String, path: String, patch: String) -> Result<(), String> {
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
pub fn revert_all(repo: String) -> Result<(), String> {
    let repo = PathBuf::from(repo);
    if base_ref(&repo) != "HEAD" {
        return Err("仓库还没有任何提交，无法整体还原".to_string());
    }
    run_git(&repo, &["reset", "--hard", "HEAD"], None)?;
    // -fd without -x: untracked files/dirs go, .gitignore'd artifacts survive
    run_git(&repo, &["clean", "-fd"], None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_basic() {
        let out = "M\0src/a.rs\0A\0new.txt\0D\0gone.txt\0";
        let files = parse_name_status(out);
        assert_eq!(
            files,
            vec![
                FileStatus {
                    path: "src/a.rs".into(),
                    old_path: None,
                    kind: ChangeKind::Modified
                },
                FileStatus {
                    path: "new.txt".into(),
                    old_path: None,
                    kind: ChangeKind::Added
                },
                FileStatus {
                    path: "gone.txt".into(),
                    old_path: None,
                    kind: ChangeKind::Deleted
                },
            ]
        );
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
        let bd = get_file_diff(r.root(), "bin.dat".into(), None, ChangeKind::Modified).unwrap();
        assert!(bd.is_binary);
        assert!(bd.hunks.is_empty());

        // hunk revert + offset drift: revert hunk 0, re-fetch, revert remaining hunk
        let d1 = get_file_diff(r.root(), "a.txt".into(), None, ChangeKind::Modified).unwrap();
        assert_eq!(d1.hunks.len(), 2);
        let patch1 = format!("{}{}", d1.file_header, d1.hunks[0].text);
        revert_hunk(r.root(), "a.txt".into(), patch1).unwrap();
        let d2 = get_file_diff(r.root(), "a.txt".into(), None, ChangeKind::Modified).unwrap();
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
                get_file_diff(r.root(), "crlf.txt".into(), None, ChangeKind::Modified).unwrap();
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

        let d = get_file_diff(r.root(), "staged.txt".into(), None, ChangeKind::Added).unwrap();
        assert_eq!(d.modified.as_deref(), Some("staged\n"));
        assert!(d.original.is_none());

        assert!(revert_all(r.root()).is_err(), "revert_all must refuse without HEAD");

        revert_file(r.root(), "staged.txt".into(), None, ChangeKind::Added).unwrap();
        assert!(!r.path().join("staged.txt").exists());
    }

    #[test]
    fn open_repo_rejects_non_repo() {
        let dir = std::env::temp_dir().join(format!("ai-diff-norepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(open_repo(dir.to_string_lossy().to_string()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
