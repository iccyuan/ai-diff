<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { showFileInfo } from "../lib/fileInfo";
import { toast } from "../lib/toast";
import { fileIcon } from "../lib/fileIcons";
import Spinner from "./Spinner.vue";
import CommitPanel from "./CommitPanel.vue";
import BranchSwitcher from "./BranchSwitcher.vue";
import { api, type ChangeKind, type FileStatus } from "../lib/api";

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
  conflicted: "!",
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

// per-project dir expand state, keyed by workspace root — otherwise two repos
// sharing a subdir name (e.g. "src") would bleed each other's expand state
// changes view: everything expanded unless user collapsed it
const collapsedChangesByRoot = ref(new Map<string, Set<string>>());
// all-files view: everything collapsed unless user expanded it
const expandedAllByRoot = ref(new Map<string, Set<string>>());

function dirSetFor(map: Map<string, Set<string>>, root: string): Set<string> {
  return map.get(root) ?? new Set();
}

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

// changes mode splits into staged/unstaged IDEA-style groups; `rows` is always
// the unstaged (or, in all-files mode, the plain full-tree) list, `stagedRows`
// is only ever populated in changes mode.
const rows = computed<Row[]>(() => {
  const root = repo.repo?.root ?? "";
  if (repo.mode === "changes") {
    const collapsed = dirSetFor(collapsedChangesByRoot.value, root);
    const unstaged = repo.files.filter((f) => !f.staged);
    return buildRows(
      unstaged.map((f) => ({ path: f.path, status: f })),
      (p) => !collapsed.has(p),
    );
  }
  const expanded = dirSetFor(expandedAllByRoot.value, root);
  const statusMap = new Map(repo.files.map((f) => [f.path, f]));
  return buildRows(
    repo.allFiles.map((p) => ({ path: p, status: statusMap.get(p) })),
    (p) => expanded.has(p),
  );
});

const hasStaged = computed(() => repo.mode === "changes" && repo.files.some((f) => f.staged));

const stagedRows = computed<Row[]>(() => {
  if (repo.mode !== "changes") return [];
  const root = repo.repo?.root ?? "";
  const collapsed = dirSetFor(collapsedChangesByRoot.value, root);
  const staged = repo.files.filter((f) => f.staged);
  return buildRows(
    staged.map((f) => ({ path: f.path, status: f })),
    (p) => !collapsed.has(p),
  );
});

// staged rows first, then unstaged — matches on-screen top-to-bottom order,
// used for cross-group keyboard nav (move) and shift-click range select
const allRows = computed<Row[]>(() => [...stagedRows.value, ...rows.value]);

function toggleStage(f: FileStatus) {
  if (f.staged) repo.unstageFile(f);
  else repo.stageFile(f);
}

// fetch/pull/push act on the active workspace's store actions, so switch to
// the clicked project first (mirrors BranchSwitcher's activate-then-act)
async function fetchProject(i: number) {
  if (repo.active !== i) repo.activateWorkspace(i);
  await repo.fetchRemote();
}
async function pullProject(i: number) {
  if (repo.active !== i) repo.activateWorkspace(i);
  await repo.pullBranch();
}
async function pushProject(i: number) {
  if (repo.active !== i) repo.activateWorkspace(i);
  const branch = repo.repo?.branch;
  if (!branch) return;
  const ok = await repo.pushBranch("origin", branch, !repo.repo?.upstream, "none");
  if (!ok) {
    const retry = await confirmDialog(
      "强制推送",
      `推送被拒绝（远程可能有本地没有的新提交）。要先 fetch 最新状态，再用 --force-with-lease 强制推送到 origin/${branch} 吗？这会覆盖远程分支的历史，可能导致他人的提交丢失，且不可撤销。`,
    );
    if (retry) {
      await repo.fetchRemote();
      await repo.pushBranch("origin", branch, false, "lease");
    }
  }
}

function toggleDir(path: string) {
  const root = repo.repo?.root ?? "";
  const map = repo.mode === "changes" ? collapsedChangesByRoot : expandedAllByRoot;
  const s = new Set(dirSetFor(map.value, root));
  if (s.has(path)) s.delete(path);
  else s.add(path);
  const next = new Map(map.value);
  next.set(root, s);
  map.value = next;
}

function openRow(row: FileRow) {
  if (row.status) repo.selectFile(row.status);
  else repo.selectPath(row.path);
}

// ----- multi-select (Ctrl/Cmd toggle, Shift range) for batch operations -----
const selected = ref<Set<string>>(new Set());
let anchor: string | null = null;

function fileOrder(): string[] {
  return allRows.value.filter((r): r is FileRow => r.type === "file").map((r) => r.path);
}

function onRowClick(row: FileRow, e: MouseEvent) {
  const path = row.path;
  if (e.shiftKey && anchor) {
    const order = fileOrder();
    const a = order.indexOf(anchor);
    const b = order.indexOf(path);
    if (a >= 0 && b >= 0) {
      const [lo, hi] = a < b ? [a, b] : [b, a];
      selected.value = new Set(order.slice(lo, hi + 1));
    }
    openRow(row);
    return;
  }
  if (e.ctrlKey || e.metaKey) {
    const s = new Set(selected.value);
    if (s.has(path)) s.delete(path);
    else s.add(path);
    selected.value = s;
    anchor = path;
    return; // toggle selection only; keep the currently-open file
  }
  // plain click: single select + open
  selected.value = new Set([path]);
  anchor = path;
  openRow(row);
}

// selected files that are actually changed (only those can be reverted)
const selectedChanged = computed<FileStatus[]>(() => {
  const map = new Map(repo.files.map((f) => [f.path, f]));
  return [...selected.value].map((p) => map.get(p)).filter((f): f is FileStatus => !!f);
});

function clearSelection() {
  selected.value = new Set();
  anchor = null;
}

async function revertSelected() {
  const files = selectedChanged.value;
  if (!files.length) return;
  const ok = await confirmDialog(
    "还原所选文件",
    `确定还原所选 ${files.length} 个文件吗？未跟踪文件将被删除，其余还原为 HEAD 版本。此操作不可撤销。`,
  );
  if (!ok) return;
  await repo.revertFiles(files);
  clearSelection();
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

// ----- right-click context menu (open containing folder / file info) -----
// relPath is null for project rows — they show "打开所在目录" only, no file info
const menu = ref<{ x: number; y: number; fullPath: string; relPath: string | null } | null>(null);
const menuEl = ref<HTMLElement | null>(null);

async function showMenu(e: MouseEvent, target: { fullPath: string; relPath: string | null }) {
  menu.value = { x: e.clientX, y: e.clientY, ...target };
  // keep the menu on screen — rows near the bottom would clip it otherwise
  await nextTick();
  const el = menuEl.value;
  if (!el || !menu.value) return;
  const r = el.getBoundingClientRect();
  const pad = 8;
  const x = Math.max(pad, Math.min(menu.value.x, window.innerWidth - r.width - pad));
  const y = Math.max(pad, Math.min(menu.value.y, window.innerHeight - r.height - pad));
  menu.value = { ...menu.value, x, y };
}

function openFileMenu(row: FileRow, e: MouseEvent) {
  const root = repo.repo?.root;
  showMenu(e, { fullPath: root ? `${root}/${row.path}` : row.path, relPath: row.path });
}

function openProjMenu(root: string, e: MouseEvent) {
  showMenu(e, { fullPath: root, relPath: null });
}

function closeMenu() {
  menu.value = null;
}

async function revealInFolder(full: string) {
  closeMenu();
  try {
    await revealItemInDir(full);
  } catch (err) {
    toast(`无法打开所在目录：${err}`, "error");
  }
}

async function openFileInfo(path: string) {
  closeMenu();
  const root = repo.repo?.root;
  if (!root) return;
  try {
    showFileInfo(await api.fileInfo(root, path));
  } catch (err) {
    toast(`无法读取文件信息：${err}`, "error");
  }
}

onMounted(() => {
  window.addEventListener("click", closeMenu);
  window.addEventListener("blur", closeMenu);
});
onBeforeUnmount(() => {
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("blur", closeMenu);
});

function move(delta: number) {
  const fileRows = allRows.value.filter((r): r is FileRow => r.type === "file");
  if (!fileRows.length) return;
  const idx = fileRows.findIndex((r) => r.path === repo.selectedPath);
  const next = idx < 0 ? 0 : Math.min(Math.max(idx + delta, 0), fileRows.length - 1);
  const row = fileRows[next];
  selected.value = new Set([row.path]);
  anchor = row.path;
  openRow(row);
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
        @contextmenu.prevent.stop="openProjMenu(w.repo.root, $event)"
      >
        <svg class="chevron" :class="{ open: isExpanded(i, w.repo.root) }" viewBox="0 0 16 16" aria-hidden="true">
          <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
        </svg>
        <span class="proj-name">{{ projName(w.repo.root) }}</span>
        <BranchSwitcher v-if="w.repo.branch" :workspace="w" :index="i" />
        <span v-else-if="!w.repo.hasHead" class="branch warn">空仓库</span>
        <span v-if="w.repo.ahead || w.repo.behind" class="ahead-behind">
          <template v-if="w.repo.ahead">↑{{ w.repo.ahead }}</template>
          <template v-if="w.repo.behind">↓{{ w.repo.behind }}</template>
        </span>
        <Spinner v-if="(i === repo.active && repo.loadingStatus) || w.busyOp" :size="12" />
        <span v-if="repo.mode === 'changes' && w.files.length" class="stats proj-stats">
          <span class="add">+{{ wsTotals(w.files).add }}</span>
          <span class="del">−{{ wsTotals(w.files).del }}</span>
        </span>
        <button
          v-if="w.repo.upstream"
          class="proj-net-btn"
          title="Fetch"
          :disabled="!!w.busyOp"
          @click.stop="fetchProject(i)"
        >⇣</button>
        <button
          v-if="w.repo.upstream"
          class="proj-net-btn"
          title="Pull"
          :disabled="!!w.busyOp"
          @click.stop="pullProject(i)"
        >↓</button>
        <button
          v-if="w.repo.branch"
          class="proj-net-btn"
          title="Push"
          :disabled="!!w.busyOp"
          @click.stop="pushProject(i)"
        >↑</button>
        <button class="proj-close" title="关闭项目" @click.stop="repo.closeWorkspace(i)">✕</button>
      </div>
      <template v-if="isExpanded(i, w.repo.root)">
        <div v-if="repo.mode === 'changes' && !repo.loadingStatus && !repo.files.length" class="list-empty">
          工作区干净，没有未提交的更改
        </div>

        <template v-if="hasStaged">
          <div class="section-head">
            <span>已暂存的更改</span>
            <button class="section-action" title="全部取消暂存" @click.stop="repo.unstageAll()">取消暂存全部</button>
          </div>
          <ul>
      <template v-for="row in stagedRows" :key="row.type === 'dir' ? 'd:' + row.path : 's:' + row.path">
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
          :class="[
            row.status ? 'k-' + row.status.kind : '',
            { active: row.path === repo.selectedPath && repo.activeTabStaged, selected: selected.has(row.path) },
          ]"
          :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
          @click="onRowClick(row, $event)"
          @contextmenu.prevent.stop="openFileMenu(row, $event)"
        >
          <input
            v-if="row.status && row.status.kind !== 'conflicted'"
            type="checkbox"
            class="stage-check"
            checked
            title="取消暂存"
            @click.stop="toggleStage(row.status)"
          />
          <span class="ficon">
            <img :src="fileIcon(row.path)" alt="" />
            <span v-if="row.status" class="status-dot" :class="row.status.kind">{{
              badgeOf(row.status.kind)
            }}</span>
          </span>
          <span class="path" :title="fileTitle(row)">{{ row.name }}</span>
          <span v-if="row.status" class="stats">
            <span v-if="row.status.additions != null" class="add">+{{ row.status.additions }}</span>
            <span v-if="row.status.deletions != null" class="del">−{{ row.status.deletions }}</span>
          </span>
        </li>
        </template>
          </ul>
          <div class="section-head">
            <span>未暂存的更改</span>
            <button class="section-action" title="全部暂存" @click.stop="repo.stageAll()">暂存全部</button>
          </div>
        </template>

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
          :class="[
            row.status ? 'k-' + row.status.kind : '',
            { active: row.path === repo.selectedPath && !repo.activeTabStaged, selected: selected.has(row.path) },
          ]"
          :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
          @click="onRowClick(row, $event)"
          @contextmenu.prevent.stop="openFileMenu(row, $event)"
        >
          <input
            v-if="row.status && repo.mode === 'changes' && row.status.kind !== 'conflicted'"
            type="checkbox"
            class="stage-check"
            title="暂存"
            @click.stop="toggleStage(row.status)"
          />
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

    <div v-if="selectedChanged.length >= 2" class="sel-bar">
      <span>已选 {{ selectedChanged.length }} 项</span>
      <button class="btn danger" @click="revertSelected">还原所选</button>
      <button class="btn" @click="clearSelection">取消</button>
    </div>

    <CommitPanel />

    <Teleport to="body">
      <div
        v-if="menu"
        ref="menuEl"
        class="ctx-menu"
        :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
        @click.stop
        @contextmenu.prevent
      >
        <button @click="revealInFolder(menu.fullPath)">打开所在目录</button>
        <button v-if="menu.relPath" @click="openFileInfo(menu.relPath)">查看文件信息</button>
      </div>
    </Teleport>
  </aside>
</template>
