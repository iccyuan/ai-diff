<script setup lang="ts">
import { computed, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { fileIcon } from "../lib/fileIcons";
import Spinner from "./Spinner.vue";
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

function wsTotals(files: FileStatus[]): { add: number; del: number } {
  let add = 0;
  let del = 0;
  for (const f of files) {
    add += f.additions ?? 0;
    del += f.deletions ?? 0;
  }
  return { add, del };
}

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

function projName(root: string): string {
  return root.split("/").pop() ?? root;
}

// collapsed project sections; clicking the active header toggles its tree
const collapsedProjects = ref(new Set<string>());

function isExpanded(i: number, root: string): boolean {
  return i === repo.active && !collapsedProjects.value.has(root);
}

function onProjectClick(i: number, root: string) {
  const s = new Set(collapsedProjects.value);
  if (i === repo.active) {
    if (s.has(root)) s.delete(root);
    else s.add(root);
  } else {
    repo.activateWorkspace(i);
    s.delete(root); // activating always reveals the tree
  }
  collapsedProjects.value = s;
}

/* drag-to-reorder project sections via pointer events on window (HTML5
   DnD is consumed by Tauri's native file drag-drop handler on Windows) */
const drag = ref<{ index: number; root: string; startY: number; moved: boolean } | null>(null);

function onWinPointerMove(e: PointerEvent) {
  const s = drag.value;
  if (!s) return;
  if (!s.moved && Math.abs(e.clientY - s.startY) < 6) return;
  s.moved = true;
  const headers = [...document.querySelectorAll<HTMLElement>(".file-list .proj-header")];
  const over = headers.findIndex((h) => {
    const r = h.getBoundingClientRect();
    return e.clientY >= r.top && e.clientY <= r.bottom;
  });
  if (over >= 0 && over !== s.index) {
    repo.moveWorkspace(s.index, over);
    s.index = over;
  }
}

function onWinPointerUp() {
  window.removeEventListener("pointermove", onWinPointerMove);
  const s = drag.value;
  drag.value = null;
  if (!s || s.moved) return;
  // no movement: treat as a plain click on that project header
  const idx = repo.workspaces.findIndex((w) => w.repo.root === s.root);
  if (idx >= 0) onProjectClick(idx, s.root);
}

function onProjPointerDown(i: number, root: string, e: PointerEvent) {
  if (e.button !== 0 || (e.target as HTMLElement).closest(".proj-close")) return;
  drag.value = { index: i, root, startY: e.clientY, moved: false };
  window.addEventListener("pointermove", onWinPointerMove);
  window.addEventListener("pointerup", onWinPointerUp, { once: true });
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
      <button :class="{ active: repo.mode === 'changes' }" @click="repo.setMode('changes')">更改</button>
      <button :class="{ active: repo.mode === 'all' }" @click="repo.setMode('all')">全部文件</button>
    </div>
    <div v-if="!repo.workspaces.length" class="list-empty">打开或拖入一个 git 仓库开始 review</div>
    <template v-for="(w, i) in repo.workspaces" :key="w.repo.root">
      <div
        class="proj-header"
        :class="{ active: i === repo.active, dragging: drag?.moved && drag.index === i }"
        :title="w.repo.root"
        @pointerdown="onProjPointerDown(i, w.repo.root, $event)"
      >
        <svg class="chevron" :class="{ open: isExpanded(i, w.repo.root) }" viewBox="0 0 16 16" aria-hidden="true">
          <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
        </svg>
        <span class="proj-name">{{ projName(w.repo.root) }}</span>
        <span v-if="w.repo.branch" class="branch">⎇ {{ w.repo.branch }}</span>
        <span v-else-if="!w.repo.hasHead" class="branch warn">空仓库</span>
        <Spinner v-if="i === repo.active && repo.loadingStatus" :size="12" />
        <span v-if="repo.mode === 'changes' && w.files.length" class="stats proj-stats">
          <span class="add">+{{ wsTotals(w.files).add }}</span>
          <span class="del">−{{ wsTotals(w.files).del }}</span>
        </span>
        <button class="proj-close" title="关闭项目" @click.stop="repo.closeWorkspace(i)">✕</button>
      </div>
      <template v-if="isExpanded(i, w.repo.root)">
        <div v-if="repo.mode === 'changes' && !repo.loadingStatus && !repo.files.length" class="list-empty">
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
          :class="{ active: row.path === repo.selectedPath, untracked: row.status?.kind === 'untracked' }"
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
      </template>
    </template>
  </aside>
</template>
