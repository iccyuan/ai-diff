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
import { allDirPaths, ancestorDirs, buildRows, STATUS_BADGE, type FileRow, type Row } from "../lib/fileTree";
import Spinner from "./Spinner.vue";
import { api, type ChangeKind, type FileStatus } from "../lib/api";

const repo = useRepoStore();
const settings = useSettingsStore();
const rootEl = ref<HTMLElement | null>(null);
// tighter steps keep deep trees usable in a narrow sidebar
const INDENT = 12;

// per-project dir expand state, keyed by workspace root — otherwise two repos
// sharing a subdir name (e.g. "src") would bleed each other's expand state.
// all-files view: everything collapsed unless the user (or "expand all") opened it
const expandedAllByRoot = ref(new Map<string, Set<string>>());

function dirSetFor(root: string): Set<string> {
  return expandedAllByRoot.value.get(root) ?? new Set();
}
function setDirSetFor(root: string, s: Set<string>) {
  const next = new Map(expandedAllByRoot.value);
  next.set(root, s);
  expandedAllByRoot.value = next;
}

// git never tracks empty directories, so a folder created via "新建文件夹" is
// invisible to list_files until something lands inside it. Seed it into the
// tree ourselves in the meantime — dropped once a real tracked/untracked file
// makes it redundant (this is session-local: a folder emptied out again, or
// deleted outside the app, won't self-correct until the repo is reopened).
const pendingEmptyDirs = ref(new Map<string, Set<string>>());
function pendingDirsFor(root: string): Set<string> {
  return pendingEmptyDirs.value.get(root) ?? new Set();
}
function addPendingEmptyDir(root: string, path: string) {
  const s = new Set(pendingDirsFor(root));
  s.add(path);
  const next = new Map(pendingEmptyDirs.value);
  next.set(root, s);
  pendingEmptyDirs.value = next;
}

const rows = computed<Row[]>(() => {
  const root = repo.repo?.root ?? "";
  const expanded = dirSetFor(root);
  const statusMap = new Map(repo.files.map((f) => [f.path, f]));
  const knownDirs = allDirPaths(repo.allFiles);
  const extraDirs = [...pendingDirsFor(root)].filter((d) => !knownDirs.has(d));
  return buildRows(
    repo.allFiles.map((p) => ({ path: p, status: statusMap.get(p) })),
    (p) => expanded.has(p),
    extraDirs,
  );
});

function toggleDir(path: string) {
  const root = repo.repo?.root ?? "";
  const s = new Set(dirSetFor(root));
  if (s.has(path)) s.delete(path);
  else s.add(path);
  setDirSetFor(root, s);
}

/* ----- IDEA-style tree toolbar: locate / expand all / collapse all ----- */
function expandAll() {
  const root = repo.repo?.root;
  if (!root) return;
  setDirSetFor(root, allDirPaths(repo.allFiles));
}
function collapseAll() {
  const root = repo.repo?.root;
  if (!root) return;
  setDirSetFor(root, new Set());
}
async function locateCurrentFile() {
  const root = repo.repo?.root;
  const path = repo.selectedPath;
  if (!root || !path) return;
  const s = new Set(dirSetFor(root));
  for (const d of ancestorDirs(path)) s.add(d);
  setDirSetFor(root, s);
  collapsedProjects.value = new Set([...collapsedProjects.value].filter((r) => r !== root));
  await nextTick();
  // ".active" is also used by the project-header row (li.active is the file
  // row specifically) — an unscoped selector always matched the header
  // first since it precedes the file rows in the DOM, making this a no-op.
  rootEl.value?.querySelector<HTMLElement>("li.active")?.scrollIntoView({ block: "center" });
}

/* ----- new file / new folder (right-click on a dir row or the project root) ----- */
async function newFile(dirPath: string) {
  closeMenu();
  const name = await promptDialog("新建文件", dirPath ? `在「${dirPath}」下新建文件` : "在项目根目录新建文件");
  if (!name) return;
  const root = repo.repo?.root;
  if (!root) return;
  const path = dirPath ? `${dirPath}/${name}` : name;
  try {
    await api.createFile(root, path);
    const s = new Set(dirSetFor(root));
    for (const d of ancestorDirs(path)) s.add(d);
    setDirSetFor(root, s);
    await repo.ensureAllFiles(true);
    await repo.refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}
async function newFolder(dirPath: string) {
  closeMenu();
  const name = await promptDialog("新建文件夹", dirPath ? `在「${dirPath}」下新建文件夹` : "在项目根目录新建文件夹");
  if (!name) return;
  const root = repo.repo?.root;
  if (!root) return;
  const path = dirPath ? `${dirPath}/${name}` : name;
  try {
    await api.createDir(root, path);
    addPendingEmptyDir(root, path);
    const s = new Set(dirSetFor(root));
    for (const d of ancestorDirs(path)) s.add(d);
    setDirSetFor(root, s);
  } catch (e) {
    toast(String(e), "error");
  }
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

function openRow(row: FileRow) {
  if (row.status) repo.selectFile(row.status);
  else repo.selectPath(row.path);
}

// ----- multi-select (Ctrl/Cmd toggle, Shift range) for batch operations -----
const selected = ref<Set<string>>(new Set());
let anchor: string | null = null;

function fileOrder(): string[] {
  return rows.value.filter((r): r is FileRow => r.type === "file").map((r) => r.path);
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
    return; // selection only — don't open the file
  }
  if (primaryMod(e)) {
    const s = new Set(selected.value);
    if (s.has(path)) s.delete(path);
    else s.add(path);
    selected.value = s;
    anchor = path;
    return; // toggle selection only; keep the currently-open file
  }
  // plain click only selects/highlights — opening is the double-click's job
  selected.value = new Set([path]);
  anchor = path;
}

function onRowDblClick(row: FileRow, e: MouseEvent) {
  if (e.shiftKey || primaryMod(e)) return;
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
async function revert(f: FileStatus) {
  const msg =
    f.kind === "untracked"
      ? `「${f.path}」是未跟踪文件，还原即直接删除该文件。确定吗？`
      : f.kind === "added"
        ? `「${f.path}」是新增文件，还原将把它从暂存区移除并删除。确定吗？`
        : `确定将「${f.path}」还原为 HEAD 版本吗？此操作不可撤销。`;
  if (await confirmDialog("还原文件", msg)) await repo.revertFile(f);
}

// ----- right-click context menu -----
type MenuTarget =
  | { kind: "file"; fullPath: string; relPath: string }
  | { kind: "dir"; fullPath: string; relPath: string }
  | { kind: "project"; fullPath: string };
const menu = ref<{ x: number; y: number; target: MenuTarget } | null>(null);
const menuEl = ref<HTMLElement | null>(null);

async function showMenu(e: MouseEvent, target: MenuTarget) {
  menu.value = { x: e.clientX, y: e.clientY, target };
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
  showMenu(e, { kind: "file", fullPath: root ? `${root}/${row.path}` : row.path, relPath: row.path });
}
function openDirMenu(row: Row & { type: "dir" }, e: MouseEvent) {
  const root = repo.repo?.root;
  showMenu(e, { kind: "dir", fullPath: root ? `${root}/${row.path}` : row.path, relPath: row.path });
}
function openProjMenu(root: string, e: MouseEvent) {
  showMenu(e, { kind: "project", fullPath: root });
}
function closeMenu() {
  menu.value = null;
}
function menuDirPath(): string {
  const t = menu.value?.target;
  return t && t.kind === "dir" ? t.relPath : "";
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

async function deleteTarget() {
  const t = menu.value?.target;
  closeMenu();
  if (!t || t.kind === "project") return;
  const isDir = t.kind === "dir";
  const ok = await confirmDialog(
    isDir ? "删除目录" : "删除文件",
    `确定从磁盘删除${isDir ? `目录「${t.relPath}」及其中所有文件` : `文件「${t.relPath}」`}吗？已跟踪文件会变成「已删除」更改（仍可还原），未跟踪文件将直接删除、无法找回。`,
  );
  if (!ok) return;
  await repo.deleteFiles([t.relPath]);
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
  const headers = [...document.querySelectorAll<HTMLElement>(".project-panel .proj-header")];
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
    ref="rootEl"
    class="file-list project-panel"
    :style="{ width: settings.sidebarWidth + 'px' }"
    tabindex="0"
    @keydown.up.prevent="move(-1)"
    @keydown.down.prevent="move(1)"
  >
    <div class="tree-toolbar">
      <button title="定位当前文件" :disabled="!repo.selectedPath" @click="locateCurrentFile">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.4" />
          <circle cx="8" cy="8" r="1.4" fill="currentColor" />
          <path d="M8 1v2.4M8 12.6V15M1 8h2.4M12.6 8H15" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
      </button>
      <button title="全部展开" @click="expandAll">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M2 4h5M2 8h5M2 12h5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
          <path d="M10.5 5.5 13 8l-2.5 2.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <button title="全部收起" @click="collapseAll">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M2 4h5M2 8h5M2 12h5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
          <path d="M13 5.5 10.5 8 13 10.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
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
        <span v-if="!w.repo.hasHead" class="branch warn">空仓库</span>
        <span v-if="w.repo.ahead || w.repo.behind" class="ahead-behind">
          <template v-if="w.repo.ahead">↑{{ w.repo.ahead }}</template>
          <template v-if="w.repo.behind">↓{{ w.repo.behind }}</template>
        </span>
        <Spinner v-if="(i === repo.active && repo.loadingStatus) || w.busyOp" :size="12" />
        <button
          v-if="w.repo.upstream"
          class="proj-net-btn"
          title="Fetch"
          :disabled="!!w.busyOp"
          @click.stop="fetchProject(i)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 5.2 8 8.4l4-3.2M4 9.6 8 12.8l4-3.2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <button
          v-if="w.repo.upstream"
          class="proj-net-btn"
          title="Pull"
          :disabled="!!w.busyOp"
          @click.stop="pullProject(i)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M8 2.5v7M5 6.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M3.5 11v1.5h9V11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <button
          v-if="w.repo.branch"
          class="proj-net-btn"
          title="Push"
          :disabled="!!w.busyOp"
          @click.stop="pushProject(i)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M8 12.5v-7M5 8.5l3-3 3 3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
            <path d="M3.5 11v1.5h9V11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <button class="proj-close" title="关闭项目" @click.stop="repo.closeWorkspace(i)">✕</button>
      </div>
      <template v-if="isExpanded(i, w.repo.root)">
        <TransitionGroup tag="ul" name="tree">
          <template v-for="row in rows">
            <li
              v-if="row.type === 'dir'"
              :key="'d:' + row.path"
              class="dir-row"
              :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
              @click="toggleDir(row.path)"
              @contextmenu.prevent.stop="openDirMenu(row, $event)"
            >
              <svg class="chevron" :class="{ open: !row.collapsed }" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
              </svg>
              <span class="dir-name">{{ row.label }}</span>
            </li>
            <li
              v-else
              :key="'f:' + row.path"
              :class="[row.status ? 'k-' + row.status.kind : '', { active: row.path === repo.selectedPath, selected: selected.has(row.path) }]"
              :style="{ paddingLeft: 10 + row.depth * INDENT + 'px' }"
              @click="onRowClick(row, $event)"
              @dblclick="onRowDblClick(row, $event)"
              @contextmenu.prevent.stop="openFileMenu(row, $event)"
            >
              <span class="ficon">
                <img :src="fileIcon(row.path)" alt="" />
                <span v-if="row.status" class="status-dot" :class="row.status.kind">{{ badgeOf(row.status.kind) }}</span>
              </span>
              <span class="path" :title="fileTitle(row)">{{ row.name }}</span>
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
        </TransitionGroup>
        <div v-if="!rows.length" class="list-empty">此项目没有文件</div>
      </template>
    </template>

    <div v-if="selectedChanged.length >= 2" class="sel-bar">
      <span>已选 {{ selectedChanged.length }} 项</span>
      <button class="btn danger" @click="revertSelected">还原所选</button>
      <button class="btn" @click="clearSelection">取消</button>
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
        <button @click="revealInFolder(menu.target.fullPath)">打开所在目录</button>
        <button v-if="menu.target.kind === 'file'" @click="openFileInfo(menu.target.relPath)">查看文件信息</button>
        <button v-if="menu.target.kind !== 'file'" @click="newFile(menuDirPath()); closeMenu()">新建文件</button>
        <button v-if="menu.target.kind !== 'file'" @click="newFolder(menuDirPath()); closeMenu()">新建文件夹</button>
        <template v-if="menu.target.kind !== 'project'">
          <div class="ctx-sep"></div>
          <button @click="deleteTarget()">删除</button>
        </template>
      </div>
    </Teleport>
  </aside>
</template>
