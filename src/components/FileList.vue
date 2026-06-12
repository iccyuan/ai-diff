<script setup lang="ts">
import { computed, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { fileIcon } from "../lib/fileIcons";
import type { ChangeKind, FileStatus } from "../lib/api";

const repo = useRepoStore();
const settings = useSettingsStore();
// tighter steps keep deep trees usable in a narrow sidebar
const INDENT = 12;

const BADGE: Record<string, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

interface DirRow {
  type: "dir";
  path: string;
  label: string;
  depth: number;
  collapsed: boolean;
}
interface FileRow {
  type: "file";
  path: string;
  name: string;
  depth: number;
  status?: FileStatus;
}
type Row = DirRow | FileRow;

// changes view: everything expanded unless user collapsed it
const collapsedChanges = ref(new Set<string>());
// all-files view: everything collapsed unless user expanded it
const expandedAll = ref(new Set<string>());

interface Node {
  dirs: Map<string, Node>;
  files: { path: string; status?: FileStatus }[];
}

function buildRows(
  entries: { path: string; status?: FileStatus }[],
  isExpanded: (dirPath: string) => boolean,
): Row[] {
  const root: Node = { dirs: new Map(), files: [] };
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
  const walk = (n: Node, prefix: string, depth: number) => {
    for (const name of [...n.dirs.keys()].sort((a, b) => a.localeCompare(b))) {
      let label = name;
      let path = prefix ? `${prefix}/${name}` : name;
      let child = n.dirs.get(name)!;
      // compact chains of single-child dirs without files (src/components/ style)
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

const rows = computed<Row[]>(() => {
  if (repo.mode === "changes") {
    return buildRows(
      repo.files.map((f) => ({ path: f.path, status: f })),
      (p) => !collapsedChanges.value.has(p),
    );
  }
  const statusMap = new Map(repo.files.map((f) => [f.path, f]));
  return buildRows(
    repo.allFiles.map((p) => ({ path: p, status: statusMap.get(p) })),
    (p) => expandedAll.value.has(p),
  );
});

function toggleDir(path: string) {
  const set = repo.mode === "changes" ? collapsedChanges : expandedAll;
  const s = new Set(set.value);
  if (s.has(path)) s.delete(path);
  else s.add(path);
  set.value = s;
}

function onRowClick(row: FileRow) {
  if (row.status) repo.selectFile(row.status);
  else repo.selectPath(row.path);
}

function fileTitle(row: FileRow): string {
  return row.status?.oldPath ? `${row.status.oldPath} → ${row.path}` : row.path;
}

function badgeOf(kind: ChangeKind | undefined): string {
  return kind ? BADGE[kind] : "";
}

const totals = computed(() => {
  let add = 0;
  let del = 0;
  for (const f of repo.files) {
    add += f.additions ?? 0;
    del += f.deletions ?? 0;
  }
  return { add, del };
});

async function revert(f: FileStatus) {
  const msg =
    f.kind === "untracked"
      ? `「${f.path}」是未跟踪文件，还原即直接删除该文件。确定吗？`
      : f.kind === "added"
        ? `「${f.path}」是新增文件，还原将把它从暂存区移除并删除。确定吗？`
        : `确定将「${f.path}」还原为 HEAD 版本吗？此操作不可撤销。`;
  if (await confirmDialog("还原文件", msg)) await repo.revertFile(f);
}

function move(delta: number) {
  const fileRows = rows.value.filter((r): r is FileRow => r.type === "file");
  if (!fileRows.length) return;
  const idx = fileRows.findIndex((r) => r.path === repo.selectedPath);
  const next = idx < 0 ? 0 : Math.min(Math.max(idx + delta, 0), fileRows.length - 1);
  onRowClick(fileRows[next]);
}
</script>

<template>
  <aside
    class="file-list"
    :style="{ width: settings.sidebarWidth + 'px' }"
    tabindex="0"
    @keydown.up.prevent="move(-1)"
    @keydown.down.prevent="move(1)"
  >
    <div class="tabs">
      <button :class="{ active: repo.mode === 'changes' }" @click="repo.setMode('changes')">
        更改 <span class="count">{{ repo.files.length }}</span>
      </button>
      <button :class="{ active: repo.mode === 'all' }" @click="repo.setMode('all')">全部文件</button>
      <span v-if="repo.loadingStatus" class="muted">刷新中…</span>
    </div>
    <div v-if="repo.mode === 'changes' && repo.files.length" class="totals">
      共 {{ repo.files.length }} 个文件
      <span class="stats">
        <span class="add">+{{ totals.add }}</span>
        <span class="del">−{{ totals.del }}</span>
      </span>
    </div>
    <div v-if="repo.repo && repo.mode === 'changes' && !repo.loadingStatus && !repo.files.length" class="list-empty">
      工作区干净，没有未提交的更改
    </div>
    <ul>
      <template v-for="row in rows" :key="row.type === 'dir' ? 'd:' + row.path : 'f:' + row.path">
        <li
          v-if="row.type === 'dir'"
          class="dir-row"
          :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
          @click="toggleDir(row.path)"
        >
          <svg class="chevron" :class="{ open: !row.collapsed }" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
          </svg>
          <span class="dir-name">{{ row.label }}</span>
        </li>
        <li
          v-else
          :class="{ active: row.path === repo.selectedPath }"
          :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
          @click="onRowClick(row)"
        >
          <span class="ficon">
            <img :src="fileIcon(row.path)" alt="" />
            <span v-if="row.status" class="status-dot" :class="row.status.kind">{{
              badgeOf(row.status.kind)
            }}</span>
          </span>
          <span class="path" :title="fileTitle(row)">{{ row.name }}</span>
          <span v-if="row.status && repo.mode === 'changes'" class="stats">
            <span v-if="row.status.additions != null" class="add">+{{ row.status.additions }}</span>
            <span v-if="row.status.deletions != null" class="del">−{{ row.status.deletions }}</span>
          </span>
          <button
            v-if="row.status"
            class="row-revert"
            :title="row.status.kind === 'untracked' ? '删除此文件' : '还原此文件'"
            @click.stop="revert(row.status)"
          >
            {{ row.status.kind === "untracked" ? "✕" : "↶" }}
          </button>
        </li>
      </template>
    </ul>
  </aside>
</template>
