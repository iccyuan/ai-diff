import { invoke } from "@tauri-apps/api/core";

export type ChangeKind = "modified" | "added" | "deleted" | "renamed" | "untracked";

export interface RepoInfo {
  root: string;
  branch: string | null;
  hasHead: boolean;
}

export interface FileStatus {
  path: string;
  oldPath: string | null;
  kind: ChangeKind;
  additions: number | null;
  deletions: number | null;
}

export interface CommitInfo {
  hash: string;
  shortHash: string;
  author: string;
  date: string;
  subject: string;
  additions: number;
  deletions: number;
}

export interface Hunk {
  index: number;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  text: string;
}

export interface FileDiff {
  original: string | null;
  modified: string | null;
  isBinary: boolean;
  tooLarge: boolean;
  fileHeader: string;
  hunks: Hunk[];
}

export interface FileContent {
  content: string | null;
  isBinary: boolean;
  tooLarge: boolean;
}

export const api = {
  openRepo: (path: string) => invoke<RepoInfo>("open_repo", { path }),
  getStatus: (repo: string) => invoke<FileStatus[]>("get_status", { repo }),
  getFileDiff: (repo: string, f: FileStatus) =>
    invoke<FileDiff>("get_file_diff", { repo, path: f.path, oldPath: f.oldPath, kind: f.kind }),
  revertFile: (repo: string, f: FileStatus) =>
    invoke<void>("revert_file", { repo, path: f.path, oldPath: f.oldPath, kind: f.kind }),
  revertHunk: (repo: string, path: string, patch: string) =>
    invoke<void>("revert_hunk", { repo, path, patch }),
  revertAll: (repo: string) => invoke<void>("revert_all", { repo }),
  watchRepo: (repo: string) => invoke<void>("watch_repo", { repo }),
  listFiles: (repo: string) => invoke<string[]>("list_files", { repo }),
  readFile: (repo: string, path: string) => invoke<FileContent>("read_file", { repo, path }),
  logCommits: (repo: string, skip: number, count: number) =>
    invoke<CommitInfo[]>("log_commits", { repo, skip, count }),
  commitFiles: (repo: string, hash: string) => invoke<FileStatus[]>("commit_files", { repo, hash }),
  getCommitFileDiff: (repo: string, hash: string, f: FileStatus) =>
    invoke<FileDiff>("get_commit_file_diff", {
      repo,
      hash,
      path: f.path,
      oldPath: f.oldPath,
      kind: f.kind,
    }),
};
