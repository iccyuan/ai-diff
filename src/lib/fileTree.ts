import type { FileStatus } from "./api";

export const STATUS_BADGE: Record<string, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflicted: "!",
};

export interface DirRow {
  type: "dir";
  path: string;
  label: string;
  depth: number;
  collapsed: boolean;
}
export interface FileRow {
  type: "file";
  path: string;
  name: string;
  depth: number;
  status?: FileStatus;
}
export type Row = DirRow | FileRow;

interface TreeNode {
  dirs: Map<string, TreeNode>;
  files: { path: string; status?: FileStatus }[];
}

/** Builds a collapsible dir/file tree, compacting single-child dir chains
 * (src/components/ style) the way both the Project and Commit panels expect.
 * `extraDirs` seeds directories with no files yet (e.g. just-created empty
 * folders) so they still show up — git itself never tracks empty dirs. */
export function buildRows(
  entries: { path: string; status?: FileStatus }[],
  isExpanded: (dirPath: string) => boolean,
  extraDirs: string[] = [],
): Row[] {
  const root: TreeNode = { dirs: new Map(), files: [] };
  const ensureDir = (path: string) => {
    let n = root;
    for (const part of path.split("/")) {
      let child = n.dirs.get(part);
      if (!child) {
        child = { dirs: new Map(), files: [] };
        n.dirs.set(part, child);
      }
      n = child;
    }
  };
  for (const d of extraDirs) ensureDir(d);
  for (const e of entries) {
    const parts = e.path.split("/");
    let n = root;
    for (let i = 0; i < parts.length - 1; i++) {
      let child = n.dirs.get(parts[i]);
      if (!child) {
        child = { dirs: new Map(), files: [] };
        n.dirs.set(parts[i], child);
      }
      n = child;
    }
    n.files.push(e);
  }

  const out: Row[] = [];
  const walk = (n: TreeNode, prefix: string, depth: number) => {
    for (const name of [...n.dirs.keys()].sort((a, b) => a.localeCompare(b))) {
      let label = name;
      let path = prefix ? `${prefix}/${name}` : name;
      let child = n.dirs.get(name)!;
      while (child.files.length === 0 && child.dirs.size === 1) {
        const only = [...child.dirs.keys()][0];
        label += "/" + only;
        path += "/" + only;
        child = child.dirs.get(only)!;
      }
      const open = isExpanded(path);
      out.push({ type: "dir", path, label, depth, collapsed: !open });
      if (open) walk(child, path, depth + 1);
    }
    for (const e of [...n.files].sort((a, b) => a.path.localeCompare(b.path))) {
      out.push({ type: "file", path: e.path, name: e.path.split("/").pop()!, depth, status: e.status });
    }
  };
  walk(root, "", 0);
  return out;
}

/** every directory path implied by a flat file-path list — used by "expand all" */
export function allDirPaths(paths: string[]): Set<string> {
  const dirs = new Set<string>();
  for (const p of paths) {
    const parts = p.split("/");
    for (let i = 1; i < parts.length; i++) dirs.add(parts.slice(0, i).join("/"));
  }
  return dirs;
}

/** every ancestor dir path of `path`, e.g. "a/b/c.ts" -> ["a", "a/b"] */
export function ancestorDirs(path: string): string[] {
  const parts = path.split("/");
  const out: string[] = [];
  for (let i = 1; i < parts.length; i++) out.push(parts.slice(0, i).join("/"));
  return out;
}
