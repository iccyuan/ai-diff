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
};
