<script setup lang="ts">
import { computed, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import type { FileStatus } from "../lib/api";

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
  file: FileStatus;
  name: string;
  depth: number;
}
type Row = DirRow | FileRow;

const collapsed = ref(new Set<string>());

interface Node {
  dirs: Map<string, Node>;
  files: FileStatus[];
}

const rows = computed<Row[]>(() => {
  const root: Node = { dirs: new Map(), files: [] };
  for (const f of repo.files) {
    const parts = f.path.split("/");
    let n = root;
    for (let i = 0; i < parts.length - 1; i++) {
      let child = n.dirs.get(parts[i]);
      if (!child) {
        child = { dirs: new Map(), files: [] };
        n.dirs.set(parts[i], child);
      }
      n = child;
    }
    n.files.push(f);
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
      const isCollapsed = collapsed.value.has(path);
      out.push({ type: "dir", path, label, depth, collapsed: isCollapsed });
      if (!isCollapsed) walk(child, path, depth + 1);
    }
    for (const f of [...n.files].sort((a, b) => a.path.localeCompare(b.path))) {
      out.push({ type: "file", file: f, name: f.path.split("/").pop()!, depth });
    }
  };
  walk(root, "", 0);
  return out;
});

function toggleDir(path: string) {
  const s = new Set(collapsed.value);
  if (s.has(path)) s.delete(path);
  else s.add(path);
  collapsed.value = s;
}

function fileTitle(f: FileStatus): string {
  return f.oldPath ? `${f.oldPath} → ${f.path}` : f.path;
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
  const idx = fileRows.findIndex((r) => r.file.path === repo.selectedPath);
  const next = idx < 0 ? 0 : Math.min(Math.max(idx + delta, 0), fileRows.length - 1);
  repo.selectFile(fileRows[next].file);
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
    <div class="list-header">
      <span>更改的文件</span>
      <span class="count">{{ repo.files.length }}</span>
      <span v-if="repo.loadingStatus" class="muted">刷新中…</span>
    </div>
    <div v-if="repo.repo && !repo.loadingStatus && !repo.files.length" class="list-empty">
      工作区干净，没有未提交的更改
    </div>
    <ul>
      <template v-for="row in rows" :key="row.type === 'dir' ? 'd:' + row.path : 'f:' + row.file.path">
        <li
          v-if="row.type === 'dir'"
          class="dir-row"
          :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
          @click="toggleDir(row.path)"
        >
          <span class="chevron">{{ row.collapsed ? "▸" : "▾" }}</span>
          <span class="dir-name">{{ row.label }}</span>
        </li>
        <li
          v-else
          :class="{ active: row.file.path === repo.selectedPath }"
          :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
          @click="repo.selectFile(row.file)"
        >
          <span class="badge" :class="row.file.kind">{{ BADGE[row.file.kind] }}</span>
          <span class="path" :title="fileTitle(row.file)">{{ row.name }}</span>
          <button
            class="row-revert"
            :title="row.file.kind === 'untracked' ? '删除此文件' : '还原此文件'"
            @click.stop="revert(row.file)"
          >
            {{ row.file.kind === "untracked" ? "✕" : "↶" }}
          </button>
        </li>
      </template>
    </ul>
  </aside>
</template>
