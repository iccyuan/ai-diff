<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { showFileInfo } from "../lib/fileInfo";
import { toast } from "../lib/toast";
import { fileIcon } from "../lib/fileIcons";
import { buildRows, STATUS_BADGE, type FileRow, type Row } from "../lib/fileTree";
import { api, type ChangeKind, type FileStatus } from "../lib/api";
import Spinner from "./Spinner.vue";

const repo = useRepoStore();
const settings = useSettingsStore();
const INDENT = 12;

// changes mode: everything expanded unless the user collapsed it
const collapsedByRoot = ref(new Map<string, Set<string>>());
function collapsedFor(root: string): Set<string> {
  return collapsedByRoot.value.get(root) ?? new Set();
}
function toggleDir(path: string) {
  const root = repo.repo?.root ?? "";
  const s = new Set(collapsedFor(root));
  if (s.has(path)) s.delete(path);
  else s.add(path);
  const next = new Map(collapsedByRoot.value);
  next.set(root, s);
  collapsedByRoot.value = next;
}

const rows = computed<Row[]>(() => {
  const root = repo.repo?.root ?? "";
  const collapsed = collapsedFor(root);
  const unstaged = repo.files.filter((f) => !f.staged);
  return buildRows(
    unstaged.map((f) => ({ path: f.path, status: f })),
    (p) => !collapsed.has(p),
  );
});
const hasStaged = computed(() => repo.files.some((f) => f.staged));
const stagedRows = computed<Row[]>(() => {
  const root = repo.repo?.root ?? "";
  const collapsed = collapsedFor(root);
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
    return;
  }
  selected.value = new Set([path]);
  anchor = path;
  openRow(row);
}

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
  return kind ? STATUS_BADGE[kind] : "";
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
const menu = ref<{ x: number; y: number; fullPath: string; relPath: string } | null>(null);
const menuEl = ref<HTMLElement | null>(null);

async function openFileMenu(row: FileRow, e: MouseEvent) {
  const root = repo.repo?.root;
  menu.value = { x: e.clientX, y: e.clientY, fullPath: root ? `${root}/${row.path}` : row.path, relPath: row.path };
  await nextTick();
  const el = menuEl.value;
  if (!el || !menu.value) return;
  const r = el.getBoundingClientRect();
  const pad = 8;
  const x = Math.max(pad, Math.min(menu.value.x, window.innerWidth - r.width - pad));
  const y = Math.max(pad, Math.min(menu.value.y, window.innerHeight - r.height - pad));
  menu.value = { ...menu.value, x, y };
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

// collapsed project sections; clicking the active header toggles its tree.
// no drag-to-reorder here — that's a Project-panel-only affordance.
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
    s.delete(root);
  }
  collapsedProjects.value = s;
}

function stageAllUnstaged() {
  repo.stageAll();
}

/* ----- commit message box ----- */
const message = ref("");
const committing = ref(false);

const stagedCount = computed(() => repo.files.filter((f) => f.staged).length);
const unstagedCount = computed(() => repo.files.filter((f) => !f.staged && f.kind !== "conflicted").length);
const hasConflicts = computed(() => repo.files.some((f) => f.kind === "conflicted"));

const canCommit = computed(
  () => !committing.value && !hasConflicts.value && message.value.trim().length > 0 && stagedCount.value > 0,
);

async function doCommit() {
  if (!canCommit.value) return;
  committing.value = true;
  try {
    const ok = await repo.createCommit(message.value.trim(), false);
    if (ok) message.value = "";
  } finally {
    committing.value = false;
  }
}

// custom drag handle above the textarea, replacing the native resize grip so
// it matches the sidebar/git-panel resizers instead of the browser's own style
function startResizeMessage(e: PointerEvent) {
  const startY = e.clientY;
  const startH = settings.commitMessageHeight;
  const el = e.target as HTMLElement;
  el.setPointerCapture(e.pointerId);
  const move = (ev: PointerEvent) => {
    settings.commitMessageHeight = Math.min(500, Math.max(50, startH - (ev.clientY - startY)));
  };
  const up = (ev: PointerEvent) => {
    el.releasePointerCapture(ev.pointerId);
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    settings.saveCommitMessageHeight();
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
}
</script>

<template>
  <div v-if="repo.repo" class="commit-panel" :style="{ width: settings.sidebarWidth + 'px' }">
    <div
      class="commit-tree file-list"
      tabindex="0"
      @keydown.up.prevent="move(-1)"
      @keydown.down.prevent="move(1)"
    >
      <div v-if="!repo.workspaces.length" class="list-empty">打开或拖入一个 git 仓库开始 review</div>
      <template v-for="(w, i) in repo.workspaces" :key="w.repo.root">
        <div
          class="proj-header commit-proj-header"
          :class="{ active: i === repo.active }"
          :title="w.repo.root"
          @click="onProjectClick(i, w.repo.root)"
        >
          <svg class="chevron" :class="{ open: isExpanded(i, w.repo.root) }" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
          </svg>
          <span class="proj-name">{{ projName(w.repo.root) }}</span>
          <span v-if="w.repo.branch" class="branch-label muted">⎇ {{ w.repo.branch }}</span>
          <Spinner v-if="i === repo.active && repo.loadingStatus" :size="12" />
          <span v-if="w.files.length" class="stats proj-stats">
            <span class="add">+{{ wsTotals(w.files).add }}</span>
            <span class="del">−{{ wsTotals(w.files).del }}</span>
          </span>
        </div>
        <template v-if="isExpanded(i, w.repo.root)">
          <div v-if="!repo.loadingStatus && !repo.files.length" class="list-empty">工作区干净，没有未提交的更改</div>

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
                  :class="[row.status ? 'k-' + row.status.kind : '', { active: row.path === repo.selectedPath && repo.activeTabStaged, selected: selected.has(row.path) }]"
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
                    <span v-if="row.status" class="status-dot" :class="row.status.kind">{{ badgeOf(row.status.kind) }}</span>
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
                :class="[row.status ? 'k-' + row.status.kind : '', { active: row.path === repo.selectedPath && !repo.activeTabStaged, selected: selected.has(row.path) }]"
                :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
                @click="onRowClick(row, $event)"
                @contextmenu.prevent.stop="openFileMenu(row, $event)"
              >
                <input
                  v-if="row.status && row.status.kind !== 'conflicted'"
                  type="checkbox"
                  class="stage-check"
                  title="暂存"
                  @click.stop="toggleStage(row.status)"
                />
                <span class="ficon">
                  <img :src="fileIcon(row.path)" alt="" />
                  <span v-if="row.status" class="status-dot" :class="row.status.kind">{{ badgeOf(row.status.kind) }}</span>
                </span>
                <span class="path" :title="fileTitle(row)">{{ row.name }}</span>
                <span v-if="row.status" class="stats">
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
    </div>

    <div class="commit-summary">
      <span v-if="stagedCount">已暂存 {{ stagedCount }} 个文件</span>
      <span v-else-if="unstagedCount">{{ unstagedCount }} 个文件未暂存</span>
      <span v-else>没有更改</span>
      <button v-if="unstagedCount && !stagedCount" class="btn-link" @click="stageAllUnstaged">全部暂存</button>
    </div>
    <div class="commit-message-wrap">
      <div class="commit-message-resizer" title="拖动调整高度" @pointerdown="startResizeMessage"></div>
      <textarea
        v-model="message"
        class="commit-message"
        :style="{ height: settings.commitMessageHeight + 'px' }"
        placeholder="提交信息…"
        @keydown.ctrl.enter="doCommit"
        @keydown.meta.enter="doCommit"
      ></textarea>
    </div>
    <div class="commit-actions">
      <button class="btn primary" :disabled="!canCommit" @click="doCommit">提交</button>
    </div>

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
        <button @click="openFileInfo(menu.relPath)">查看文件信息</button>
      </div>
    </Teleport>
  </div>
</template>
