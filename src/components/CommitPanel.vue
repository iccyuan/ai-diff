<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { promptDialog } from "../lib/prompt";
import { showFileInfo } from "../lib/fileInfo";
import { toast } from "../lib/toast";
import { fileIcon } from "../lib/fileIcons";
import { primaryMod } from "../lib/platform";
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

// IDEA-style commit panel: one flat list of every change, no staged/unstaged
// split. A partially-staged file has two status entries — prefer the
// working-tree one, so what's shown (and committed) is what's on disk.
const changedFiles = computed<FileStatus[]>(() => {
  const byPath = new Map<string, FileStatus>();
  for (const f of repo.files) {
    const prev = byPath.get(f.path);
    if (!prev || (prev.staged && !f.staged)) byPath.set(f.path, f);
  }
  return [...byPath.values()];
});
const rows = computed<Row[]>(() => {
  const root = repo.repo?.root ?? "";
  const collapsed = collapsedFor(root);
  return buildRows(
    changedFiles.value.map((f) => ({ path: f.path, status: f })),
    (p) => !collapsed.has(p),
  );
});

/* ----- IDEA-style checkboxes: ticking a file only marks it as "include in
 * the next commit" — nothing touches the git index until 提交 is clicked.
 * The *unchecked* set is what's tracked, so newly appearing files default to
 * checked, and unticks survive refreshes. ----- */
const uncheckedByRoot = ref(new Map<string, Set<string>>());
function uncheckedFor(root: string): Set<string> {
  return uncheckedByRoot.value.get(root) ?? new Set();
}
function isChecked(path: string): boolean {
  return !uncheckedFor(repo.repo?.root ?? "").has(path);
}
function toggleChecked(path: string) {
  const root = repo.repo?.root ?? "";
  const s = new Set(uncheckedFor(root));
  if (s.has(path)) s.delete(path);
  else s.add(path);
  const next = new Map(uncheckedByRoot.value);
  next.set(root, s);
  uncheckedByRoot.value = next;
}
const checkedFiles = computed<FileStatus[]>(() =>
  changedFiles.value.filter((f) => f.kind !== "conflicted" && isChecked(f.path)),
);
const allChecked = computed(() => checkedFiles.value.length === changedFiles.value.filter((f) => f.kind !== "conflicted").length);
function toggleCheckAll() {
  const root = repo.repo?.root ?? "";
  const next = new Map(uncheckedByRoot.value);
  next.set(root, allChecked.value ? new Set(changedFiles.value.map((f) => f.path)) : new Set());
  uncheckedByRoot.value = next;
}

function openRow(row: FileRow) {
  if (row.status) repo.selectFile(row.status);
  else repo.selectPath(row.path);
}

/* ----- unified selection: the checkbox IS the selection. Ticking a box,
 * Ctrl/⌘-clicking a row and Shift-clicking a range all edit the same checked
 * set — 提交 / 还原 / 删除 act on that set. A plain click only opens the file
 * and never touches the selection. ----- */
let anchor: string | null = null;

function fileOrder(): string[] {
  return rows.value.filter((r): r is FileRow => r.type === "file").map((r) => r.path);
}

function setCheckedTo(paths: Set<string>) {
  const root = repo.repo?.root ?? "";
  const next = new Map(uncheckedByRoot.value);
  next.set(root, new Set(changedFiles.value.filter((f) => !paths.has(f.path)).map((f) => f.path)));
  uncheckedByRoot.value = next;
}

function selectWith(path: string, e: MouseEvent) {
  if (e.shiftKey && anchor) {
    // Shift: replace the checked set with the anchor→here range
    const order = fileOrder();
    const a = order.indexOf(anchor);
    const b = order.indexOf(path);
    if (a >= 0 && b >= 0) {
      const [lo, hi] = a < b ? [a, b] : [b, a];
      setCheckedTo(new Set(order.slice(lo, hi + 1)));
    }
    return;
  }
  toggleChecked(path);
  anchor = path;
}

// single click = the same toggle as the checkbox (unified selection logic);
// opening the diff is the double-click's job, like the project file tree
function onRowClick(row: FileRow, e: MouseEvent) {
  selectWith(row.path, e);
}

function onRowDblClick(row: FileRow, e: MouseEvent) {
  if (e.shiftKey || primaryMod(e)) return;
  openRow(row);
}

// files staged by external tools still get an「已暂存」pill for transparency
const stagedPaths = computed(() => new Set(repo.files.filter((f) => f.staged).map((f) => f.path)));

/* ----- IDEA-style shelve: park the files' changes on the git stash shelf;
 * entries live in the Git 面板's 搁置 tab (ShelfPanel), which watches
 * refreshSeq and picks the new entry up automatically ----- */
async function shelveFiles(files: FileStatus[]) {
  if (!files.length) return;
  const now = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  const defaultName = `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())} ${p(now.getHours())}:${p(now.getMinutes())} 的搁置`;
  const name = await promptDialog("搁置更改", `将这 ${files.length} 个文件的更改移入搁置架，之后可随时恢复。搁置名称：`, defaultName);
  if (name == null) return; // 取消
  const paths = files.flatMap((f) => (f.oldPath ? [f.oldPath, f.path] : [f.path]));
  await repo.shelveFiles(name.trim() || defaultName, paths);
}

async function revertFilesUi(files: FileStatus[]) {
  if (!files.length) return;
  const ok = await confirmDialog(
    "还原文件",
    `确定还原这 ${files.length} 个文件吗？未跟踪文件将被删除，其余还原为 HEAD 版本。此操作不可撤销。`,
  );
  if (!ok) return;
  await repo.revertFiles(files);
}

async function deleteFilesUi(files: FileStatus[]) {
  if (!files.length) return;
  const ok = await confirmDialog(
    "删除文件",
    `确定从磁盘删除这 ${files.length} 个文件吗？已跟踪文件会变成「已删除」更改（仍可还原），未跟踪文件将直接删除、无法找回。`,
  );
  if (!ok) return;
  await repo.deleteFiles(files.map((f) => f.path));
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

// ----- right-click context menu; batch actions target the checked set when
// the clicked row is checked, otherwise just the clicked file (IDEA-like) -----
const menu = ref<{ x: number; y: number; fullPath: string; relPath: string } | null>(null);
const menuEl = ref<HTMLElement | null>(null);

const menuFiles = computed<FileStatus[]>(() => {
  const rel = menu.value?.relPath;
  if (!rel) return [];
  if (checkedFiles.value.some((f) => f.path === rel)) return checkedFiles.value;
  const f = changedFiles.value.find((c) => c.path === rel);
  return f ? [f] : [];
});

// snapshot the target files BEFORE closing the menu — closing nulls menuFiles
function menuAction(fn: (files: FileStatus[]) => void) {
  const files = menuFiles.value;
  closeMenu();
  fn(files);
}

async function openFileMenu(row: FileRow, e: MouseEvent) {
  // Explorer-style: right-clicking a file that isn't checked makes it the
  // (only) selection first, so the menu's targets always match the ticks
  if (row.status && row.status.kind !== "conflicted" && !checkedFiles.value.some((f) => f.path === row.path)) {
    setCheckedTo(new Set([row.path]));
    anchor = row.path;
  }
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
  const fileRows = rows.value.filter((r): r is FileRow => r.type === "file");
  if (!fileRows.length) return;
  const idx = fileRows.findIndex((r) => r.path === repo.selectedPath);
  const next = idx < 0 ? 0 : Math.min(Math.max(idx + delta, 0), fileRows.length - 1);
  const row = fileRows[next];
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

/* ----- commit message box ----- */
const message = ref("");
const committing = ref(false);

const hasConflicts = computed(() => repo.files.some((f) => f.kind === "conflicted"));

const canCommit = computed(
  () => !committing.value && !hasConflicts.value && message.value.trim().length > 0 && checkedFiles.value.length > 0,
);

async function doCommit() {
  if (!canCommit.value) return;
  committing.value = true;
  try {
    // renames need the old path staged too for git to see the pair
    const paths = checkedFiles.value.flatMap((f) => (f.oldPath ? [f.oldPath, f.path] : [f.path]));
    const ok = await repo.commitPaths(message.value.trim(), paths);
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

          <ul>
            <template v-for="row in rows">
              <li
                v-if="row.type === 'dir'"
                :key="'d:' + row.path"
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
                :key="'f:' + row.path"
                :class="[row.status ? 'k-' + row.status.kind : '', { active: row.path === repo.selectedPath }]"
                :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
                @click="onRowClick(row, $event)"
                @dblclick="onRowDblClick(row, $event)"
                @contextmenu.prevent.stop="openFileMenu(row, $event)"
              >
                <span
                  v-if="row.status && row.status.kind !== 'conflicted'"
                  class="stage-check"
                  :class="{ checked: isChecked(row.path) }"
                  role="checkbox"
                  :aria-checked="isChecked(row.path)"
                  title="勾选：加入提交 / 批量还原、删除（Ctrl/⌘ 点击行等效，Shift 范围选择）"
                  @click.stop="selectWith(row.path, $event)"
                ></span>
                <span class="ficon">
                  <img :src="fileIcon(row.path)" alt="" />
                  <span v-if="row.status" class="status-dot" :class="row.status.kind">{{ badgeOf(row.status.kind) }}</span>
                </span>
                <span class="path" :title="fileTitle(row)">{{ row.name }}</span>
                <span v-if="stagedPaths.has(row.path)" class="staged-flag" title="该文件在 git 暂存区中">已暂存</span>
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

    </div>

    <div class="commit-summary">
      <span v-if="changedFiles.length">已勾选 {{ checkedFiles.length }} / {{ changedFiles.length }} 个文件</span>
      <span v-else>没有更改</span>
      <button v-if="changedFiles.length" class="btn-link" @click="toggleCheckAll">{{ allChecked ? "全不选" : "全选" }}</button>
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
        <button @click="menuAction(shelveFiles)">搁置（{{ menuFiles.length }} 个文件）</button>
        <button @click="menuAction(revertFilesUi)">还原（{{ menuFiles.length }} 个文件）</button>
        <button @click="menuAction(deleteFilesUi)">删除（{{ menuFiles.length }} 个文件）</button>
        <div class="ctx-sep"></div>
        <button @click="revealInFolder(menu.fullPath)">打开所在目录</button>
        <button @click="openFileInfo(menu.relPath)">查看文件信息</button>
      </div>
    </Teleport>
  </div>
</template>
