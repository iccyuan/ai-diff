<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type Ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { fileIcon } from "../lib/fileIcons";
import { confirmDialog } from "../lib/confirm";
import { promptDialog } from "../lib/prompt";
import { resetDialog } from "../lib/resetDialog";
import { toast } from "../lib/toast";
import { buildCommitGraph, graphWidth, type GraphRow } from "../lib/commitGraph";
import Spinner from "./Spinner.vue";
import { api, type BranchInfo, type CommitInfo, type FileStatus } from "../lib/api";

const repo = useRepoStore();
const settings = useSettingsStore();
const scroller = ref<HTMLElement | null>(null);

/* ----- branch sidebar: click filters the log to that branch, double-click
 * checks it out — merged in from what used to be a separate 分支 tab, like
 * IDEA's Log tab does */
const branchFilter = ref("");
const creatingBranch = ref(false);

const filteredBranches = computed(() => {
  const q = branchFilter.value.trim().toLowerCase();
  const all = repo.ws?.branches ?? [];
  const list = q ? all.filter((b) => b.name.toLowerCase().includes(q)) : all;
  return { local: list.filter((b) => !b.isRemote), remote: list.filter((b) => b.isRemote) };
});

/* ----- log search: debounced, searches the full history server-side
 * (not just what's been paged in) via search_commits ----- */
const searchInput = ref("");
let searchDebounce: ReturnType<typeof setTimeout> | undefined;
watch(searchInput, (v) => {
  clearTimeout(searchDebounce);
  searchDebounce = setTimeout(() => repo.setLogSearchQuery(v), 300);
});
function clearSearch() {
  clearTimeout(searchDebounce);
  searchInput.value = "";
  repo.setLogSearchQuery(null);
}

function onBranchClick(name: string) {
  repo.setLogBranchFilter(repo.logBranchFilter === name ? null : name);
}
async function onBranchDblClick(name: string, isRemote: boolean, isCurrent: boolean) {
  if (isCurrent) return;
  await repo.checkoutBranch(name, isRemote);
}
async function onNewBranchClick() {
  if (creatingBranch.value) return;
  const name = await promptDialog("新建分支", "输入新分支的名称");
  if (!name) return;
  creatingBranch.value = true;
  try {
    await repo.createBranch(name, null, true);
  } finally {
    creatingBranch.value = false;
  }
}
async function onMergeBranch(name: string) {
  const ok = await confirmDialog("合并分支", `确定将「${name}」合并到当前分支「${repo.repo?.branch}」吗？`);
  if (!ok) return;
  await repo.mergeBranch(name, false);
}
async function onDeleteBranch(name: string) {
  const ok = await confirmDialog("删除分支", `确定删除分支「${name}」吗？`);
  if (!ok) return;
  try {
    await repo.deleteBranch(name, false);
  } catch {
    const force = await confirmDialog(
      "强制删除分支",
      `分支「${name}」尚未合并，删除后其独有的提交将无法找回（除非知道 commit hash）。确定强制删除吗？`,
    );
    if (force) {
      try {
        await repo.deleteBranch(name, true);
      } catch (e2) {
        toast(String(e2), "error");
      }
    }
  }
}

/* ----- branch right-click menu, like IDEA's branch context menu ----- */
const branchMenu = ref<{ x: number; y: number; branch: BranchInfo } | null>(null);
const branchMenuEl = ref<HTMLElement | null>(null);

async function openBranchMenu(e: MouseEvent, b: BranchInfo) {
  branchMenu.value = { x: e.clientX, y: e.clientY, branch: b };
  await positionMenu(branchMenuEl, branchMenu);
}
function closeBranchMenu() {
  branchMenu.value = null;
}
async function onBranchMenuCheckout() {
  const b = branchMenu.value?.branch;
  closeBranchMenu();
  if (!b || b.isCurrent) return;
  await repo.checkoutBranch(b.name, b.isRemote);
}
async function onBranchMenuNewFrom() {
  const b = branchMenu.value?.branch;
  closeBranchMenu();
  if (!b) return;
  const name = await promptDialog("从此新建分支", `基于「${b.name}」创建新分支`);
  if (!name) return;
  await repo.createBranch(name, b.name, true);
}
async function onBranchMenuMerge() {
  const b = branchMenu.value?.branch;
  closeBranchMenu();
  if (!b || b.isCurrent) return;
  await onMergeBranch(b.name);
}
async function onBranchMenuRename() {
  const b = branchMenu.value?.branch;
  closeBranchMenu();
  if (!b) return;
  const newName = await promptDialog("重命名分支", `将「${b.name}」重命名为`, b.name);
  if (!newName || newName === b.name) return;
  await repo.renameBranch(b.name, newName);
}
async function onBranchMenuDelete() {
  const b = branchMenu.value?.branch;
  closeBranchMenu();
  if (!b || b.isCurrent) return;
  await onDeleteBranch(b.name);
}

/* ----- IDEA-style commit graph + branch/tag labels ----- */
const LANE_W = 14;
const LANE_COLORS = ["#6c8ef5", "#e0865a", "#5cb85c", "#c9705a", "#9b7bd8", "#4fb8b0", "#d9a441", "#e06c9f"];
function laneColor(i: number): string {
  return LANE_COLORS[i % LANE_COLORS.length];
}
const graphRows = computed<GraphRow[]>(() => buildCommitGraph(repo.commits));
const graphByHash = computed(() => new Map(graphRows.value.map((r) => [r.hash, r])));
const graphCols = computed(() => (graphRows.value.length ? graphWidth(graphRows.value) : 1));
function isRemoteRef(name: string): boolean {
  return name.includes("/");
}

/* ----- right-hand files panel (IDEA-style left/right split): clicking a
 * commit row highlights it and loads its changed files here, instead of
 * expanding an inline list under the row ----- */
const activeCommitInfo = computed(() => repo.commits.find((c) => c.hash === repo.logActiveCommit) ?? null);
const activeCommitFiles = computed(() => (repo.logActiveCommit ? repo.commitFiles[repo.logActiveCommit] : undefined));

// keeps a teleported context menu on-screen near the click, same clamping
// approach as ProjectPanel/CommitPanel's file context menus
async function positionMenu<T extends { x: number; y: number }>(elRef: Ref<HTMLElement | null>, stateRef: Ref<T | null>) {
  await nextTick();
  const el = elRef.value;
  if (!el || !stateRef.value) return;
  const r = el.getBoundingClientRect();
  const pad = 8;
  const x = Math.max(pad, Math.min(stateRef.value.x, window.innerWidth - r.width - pad));
  const y = Math.max(pad, Math.min(stateRef.value.y, window.innerHeight - r.height - pad));
  stateRef.value = { ...stateRef.value, x, y };
}

// right-click menu: checkout / branch-from-here / cherry-pick / revert /
// reset current branch to here / copy hash-subject.
// cherry-pick/revert disabled for merge commits (>1 parent) — no -m
// mainline picker offered.
const menu = ref<{ x: number; y: number; commit: CommitInfo } | null>(null);
const menuEl = ref<HTMLElement | null>(null);

async function openCommitMenu(e: MouseEvent, c: CommitInfo) {
  menu.value = { x: e.clientX, y: e.clientY, commit: c };
  await positionMenu(menuEl, menu);
}
function closeMenu() {
  menu.value = null;
}

/* ----- log header right-click: choose optional extra columns, like IDEA's
 * log table column picker ----- */
const columnMenu = ref<{ x: number; y: number } | null>(null);
const columnMenuEl = ref<HTMLElement | null>(null);
async function openColumnMenu(e: MouseEvent) {
  columnMenu.value = { x: e.clientX, y: e.clientY };
  await positionMenu(columnMenuEl, columnMenu);
}
function closeColumnMenu() {
  columnMenu.value = null;
}

function onDocPointer(e: MouseEvent) {
  const t = e.target as Node;
  if (!menuEl.value?.contains(t)) closeMenu();
  if (!branchMenuEl.value?.contains(t)) closeBranchMenu();
  if (!columnMenuEl.value?.contains(t)) closeColumnMenu();
}

async function onCherryPick() {
  const c = menu.value?.commit;
  closeMenu();
  if (c) await repo.cherryPickCommit(c.hash);
}

async function onRevertCommit() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c) return;
  const ok = await confirmDialog("回滚提交", `确定要回滚提交「${c.subject}」吗？这会创建一个新的提交来撤销它引入的更改。`);
  if (ok) await repo.revertCommit(c.hash);
}

async function onResetTo() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c || !repo.repo) return;
  const mode = await resetDialog(c.subject, c.hash, repo.repo.root);
  if (!mode) return;
  await repo.resetTo(c.hash, mode);
}

// only the tip of the currently checked-out branch can be "undone" via a
// plain soft reset — anything else needs cherry-pick/reset-to-here instead
function isCurrentHead(c: CommitInfo): boolean {
  return !!repo.repo?.branch && c.refs.includes(repo.repo.branch);
}

// only HEAD can have its message safely reworded via a plain `commit --amend`
// — anything further back needs an interactive rebase, which isn't supported
async function onEditCommitMessage() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c || !repo.repo || !isCurrentHead(c)) return;
  let full = c.subject;
  try {
    full = await api.getCommitMessage(repo.repo.root, c.hash);
  } catch (e) {
    toast(String(e), "error");
    return;
  }
  const next = await promptDialog("编辑提交消息", "", full, { multiline: true });
  if (next == null || next === full) return;
  await repo.createCommit(next, true);
}

async function onUndoCommit() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c || c.parents.length !== 1) return;
  const ok = await confirmDialog(
    "撤销上一次提交",
    `确定要撤销提交「${c.subject}」吗？它的改动会变回已暂存状态，提交本身会从历史中移除。`,
  );
  if (!ok) return;
  await repo.resetTo(c.parents[0], "soft");
}

async function onDropCommit() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c) return;
  const ok = await confirmDialog(
    "丢弃此提交",
    `确定要从历史中彻底删除提交「${c.subject}」吗？此提交之后的所有提交都会被重新应用（rebase），过程中可能出现冲突需要手动解决，此操作不易撤销。`,
  );
  if (!ok) return;
  await repo.dropCommit(c.hash);
}

async function onCheckoutRevision() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c) return;
  await repo.checkoutBranch(c.hash, false); // detached HEAD at this commit
}

async function onCreateBranchFromCommit() {
  const c = menu.value?.commit;
  closeMenu();
  if (!c) return;
  const name = await promptDialog("从此创建分支", `基于提交「${c.shortHash}」创建新分支`);
  if (!name) return;
  await repo.createBranch(name, c.hash, true);
}

async function copyText(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast(`已复制${label}`);
  } catch (e) {
    toast(String(e), "error");
  }
}

onMounted(() => window.addEventListener("click", onDocPointer));
onBeforeUnmount(() => window.removeEventListener("click", onDocPointer));

const BADGE: Record<string, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

const COMMIT_ROW_PX = 26;

function fitPageSize() {
  const h = scroller.value?.clientHeight ?? 600;
  // fill the visible height plus one extra screen so the scrollbar exists
  repo.setCommitPageSize(Math.max(20, Math.ceil((h / COMMIT_ROW_PX) * 2)));
}

function onScroll() {
  const el = scroller.value;
  if (!el || repo.loadingCommits || repo.commitsExhausted) return;
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 200) {
    repo.loadCommits();
  }
}

function name(f: FileStatus): string {
  return f.path.split("/").pop()!;
}

/* ----- draggable Author/Date column widths, like IDEA's log table ----- */
function startResizeCol(e: PointerEvent, key: "logAuthorColWidth" | "logDateColWidth", min: number, max: number) {
  const startX = e.clientX;
  const startW = settings[key];
  const el = e.target as HTMLElement;
  el.setPointerCapture(e.pointerId);
  const move = (ev: PointerEvent) => {
    // dragging left gives the column to its right more room (Subject shrinks)
    settings[key] = Math.min(max, Math.max(min, startW + (startX - ev.clientX)));
  };
  const up = (ev: PointerEvent) => {
    el.releasePointerCapture(ev.pointerId);
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    settings.saveLogColumnWidths();
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
}

onMounted(() => {
  fitPageSize();
  window.addEventListener("resize", fitPageSize);
  if (!repo.commits.length) repo.loadCommits();
  if (repo.ws) repo.loadBranches(repo.ws);
});

onBeforeUnmount(() => window.removeEventListener("resize", fitPageSize));
</script>

<template>
  <aside class="history">
    <div class="history-body">
      <div class="log-branches">
        <div class="branch-toolbar">
          <input v-model="branchFilter" class="branch-filter" placeholder="筛选分支…" />
          <button class="branch-add-btn" title="新建分支" :disabled="creatingBranch" @click="onNewBranchClick">+</button>
        </div>
        <div class="branch-list">
          <div
            class="branch-row"
            :class="{ current: !repo.logBranchFilter }"
            @click="repo.setLogBranchFilter(null)"
          >
            <span class="branch-row-name">全部（HEAD）</span>
          </div>
          <div v-if="filteredBranches.local.length" class="branch-group-label">本地分支</div>
          <div
            v-for="b in filteredBranches.local"
            :key="b.name"
            class="branch-row"
            :class="{ current: b.isCurrent, filtered: repo.logBranchFilter === b.name }"
            @click="onBranchClick(b.name)"
            @dblclick="onBranchDblClick(b.name, false, b.isCurrent)"
            @contextmenu.prevent.stop="openBranchMenu($event, b)"
          >
            <span class="branch-row-name">{{ b.name }}</span>
            <span v-if="b.upstream" class="branch-row-track">
              <template v-if="b.ahead">↑{{ b.ahead }}</template>
              <template v-if="b.behind">↓{{ b.behind }}</template>
            </span>
            <button v-if="!b.isCurrent" class="branch-row-del" title="合并到当前分支" @click.stop="onMergeBranch(b.name)">⇄</button>
            <button v-if="!b.isCurrent" class="branch-row-del" title="删除分支" @click.stop="onDeleteBranch(b.name)">✕</button>
          </div>
          <div v-if="filteredBranches.remote.length" class="branch-group-label">远程分支</div>
          <div
            v-for="b in filteredBranches.remote"
            :key="b.name"
            class="branch-row"
            :class="{ filtered: repo.logBranchFilter === b.name }"
            @click="onBranchClick(b.name)"
            @dblclick="onBranchDblClick(b.name, true, false)"
            @contextmenu.prevent.stop="openBranchMenu($event, b)"
          >
            <span class="branch-row-name muted">{{ b.name }}</span>
          </div>
          <div v-if="!filteredBranches.local.length && !filteredBranches.remote.length" class="branch-empty">没有匹配的分支</div>
        </div>
      </div>
      <div class="log-main">
        <div class="log-search-bar">
          <input v-model="searchInput" type="text" class="log-search-input" placeholder="搜索提交（主题 / 作者 / 哈希）…" />
          <button v-if="searchInput" title="清除搜索" @click="clearSearch">✕</button>
        </div>
        <div v-if="repo.logBranchFilter" class="log-filter-bar">
          <span>筛选：{{ repo.logBranchFilter }}</span>
          <button title="清除筛选" @click="repo.setLogBranchFilter(null)">✕</button>
        </div>
        <div class="log-content">
          <div class="log-table">
            <div class="log-header" @contextmenu.prevent.stop="openColumnMenu">
              <span class="log-header-graph" :style="{ width: graphCols * LANE_W + 'px' }"></span>
              <span class="log-header-subject">主题</span>
              <span class="log-header-author" :style="{ width: settings.logAuthorColWidth + 'px' }">
                <span class="log-col-resizer" @pointerdown="startResizeCol($event, 'logAuthorColWidth', 60, 260)"></span>
                作者
              </span>
              <span class="log-header-date" :style="{ width: settings.logDateColWidth + 'px' }">
                <span class="log-col-resizer" @pointerdown="startResizeCol($event, 'logDateColWidth', 80, 220)"></span>
                日期
              </span>
              <span v-if="settings.logShowHashCol" class="log-header-hash">哈希</span>
              <span v-if="settings.logShowEmailCol" class="log-header-email">邮箱</span>
              <span v-if="settings.logShowChangesCol" class="log-header-changes">改动</span>
            </div>
            <div ref="scroller" class="commits" @scroll.passive="onScroll">
              <div
                v-for="c in repo.commits"
                :key="c.hash"
                class="commit-row"
                :class="{ active: repo.logActiveCommit === c.hash }"
                @click="repo.selectLogCommit(c.hash)"
                @contextmenu.prevent.stop="openCommitMenu($event, c)"
              >
                <svg
                  v-if="graphByHash.get(c.hash)"
                  class="commit-graph"
                  :width="graphCols * LANE_W"
                  :viewBox="`0 0 ${graphCols * LANE_W} ${COMMIT_ROW_PX}`"
                  aria-hidden="true"
                >
                  <template v-for="lane in graphByHash.get(c.hash)!.passingLanes" :key="'p' + lane">
                    <line
                      :x1="lane * LANE_W + LANE_W / 2"
                      y1="0"
                      :x2="lane * LANE_W + LANE_W / 2"
                      :y2="COMMIT_ROW_PX"
                      :stroke="laneColor(lane)"
                      stroke-width="1.6"
                    />
                  </template>
                  <line
                    v-if="graphByHash.get(c.hash)!.hasIncoming"
                    :x1="graphByHash.get(c.hash)!.lane * LANE_W + LANE_W / 2"
                    y1="0"
                    :x2="graphByHash.get(c.hash)!.lane * LANE_W + LANE_W / 2"
                    :y2="COMMIT_ROW_PX / 2"
                    :stroke="laneColor(graphByHash.get(c.hash)!.lane)"
                    stroke-width="1.6"
                  />
                  <line
                    v-for="pLane in graphByHash.get(c.hash)!.parentLanes"
                    :key="'c' + pLane"
                    :x1="graphByHash.get(c.hash)!.lane * LANE_W + LANE_W / 2"
                    :y1="COMMIT_ROW_PX / 2"
                    :x2="pLane * LANE_W + LANE_W / 2"
                    :y2="COMMIT_ROW_PX"
                    :stroke="laneColor(pLane)"
                    stroke-width="1.6"
                  />
                  <circle
                    :cx="graphByHash.get(c.hash)!.lane * LANE_W + LANE_W / 2"
                    :cy="COMMIT_ROW_PX / 2"
                    r="3.2"
                    :fill="laneColor(graphByHash.get(c.hash)!.lane)"
                  />
                </svg>
                <span class="subject" :title="c.subject">
                  <span v-if="c.refs.length" class="ref-pills">
                    <span v-for="r in c.refs" :key="r" class="ref-pill" :class="{ remote: isRemoteRef(r) }">{{ r }}</span>
                  </span>
                  {{ c.subject }}
                </span>
                <span class="author-cell" :style="{ width: settings.logAuthorColWidth + 'px' }" :title="c.author">{{ c.author }}</span>
                <span class="date-cell" :style="{ width: settings.logDateColWidth + 'px' }" :title="c.date">{{ c.date }}</span>
                <span v-if="settings.logShowHashCol" class="hash-cell" :title="c.hash">{{ c.shortHash }}</span>
                <span v-if="settings.logShowEmailCol" class="email-cell" :title="c.email">{{ c.email }}</span>
                <span v-if="settings.logShowChangesCol" class="changes-cell">
                  <span v-if="c.additions" class="add">+{{ c.additions }}</span>
                  <span v-if="c.deletions" class="del">−{{ c.deletions }}</span>
                </span>
              </div>
              <div v-if="repo.loadingCommits" class="list-tail muted"><Spinner :size="18" /></div>
              <div v-else-if="repo.commitsExhausted && repo.commits.length" class="list-tail muted">已到最早提交</div>
              <div v-else-if="!repo.commits.length" class="list-tail muted">暂无提交历史</div>
            </div>
          </div>

          <div class="log-files-panel">
            <div v-if="!activeCommitInfo" class="list-empty">选择一个提交查看改动的文件</div>
            <template v-else>
              <div class="log-files-header">
                <div class="subject" :title="activeCommitInfo.subject">{{ activeCommitInfo.subject }}</div>
                <div class="log-files-meta">
                  <span class="hash">{{ activeCommitInfo.shortHash }}</span>
                  <span>{{ activeCommitInfo.author }}</span>
                  <span>{{ activeCommitInfo.date }}</span>
                </div>
              </div>
              <ul class="commit-files">
                <li v-if="!activeCommitFiles" class="muted loading-files"><Spinner :size="14" /></li>
                <li
                  v-for="f in activeCommitFiles ?? []"
                  :key="f.path"
                  :class="{ active: repo.selectedCommit === activeCommitInfo.hash && repo.selectedCommitPath === f.path }"
                  :title="f.oldPath ? `${f.oldPath} → ${f.path}` : f.path"
                  @click="repo.selectCommitFile(activeCommitInfo.hash, f)"
                >
                  <span class="ficon">
                    <img :src="fileIcon(f.path)" alt="" />
                    <span class="status-dot" :class="f.kind">{{ BADGE[f.kind] }}</span>
                  </span>
                  <span class="path">{{ name(f) }}</span>
                  <span class="stats">
                    <span v-if="f.additions != null" class="add">+{{ f.additions }}</span>
                    <span v-if="f.deletions != null" class="del">−{{ f.deletions }}</span>
                  </span>
                </li>
              </ul>
            </template>
          </div>
        </div>
      </div>
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
        <button @click="onCheckoutRevision">检出此提交</button>
        <button @click="onCreateBranchFromCommit">从此创建分支…</button>
        <button :disabled="!isCurrentHead(menu.commit)" @click="onEditCommitMessage">编辑提交消息…</button>
        <button :disabled="menu.commit.parents.length !== 1 || !isCurrentHead(menu.commit)" @click="onUndoCommit">撤销上一次提交</button>
        <div class="ctx-sep"></div>
        <button :disabled="menu.commit.parents.length > 1" @click="onCherryPick">Cherry-pick</button>
        <button :disabled="menu.commit.parents.length > 1" @click="onRevertCommit">Revert Commit</button>
        <div class="ctx-sep"></div>
        <button @click="onResetTo">重置当前分支到这里…</button>
        <button :disabled="menu.commit.parents.length > 1" @click="onDropCommit">丢弃此提交…</button>
        <div class="ctx-sep"></div>
        <button @click="copyText(menu.commit.hash, '提交哈希')">复制提交哈希</button>
        <button @click="copyText(menu.commit.subject, '提交信息')">复制提交信息</button>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="columnMenu"
        ref="columnMenuEl"
        class="ctx-menu"
        :style="{ left: columnMenu.x + 'px', top: columnMenu.y + 'px' }"
        @click.stop
        @contextmenu.prevent
      >
        <button class="ctx-check-item" @click="settings.toggleLogCol('hash')">
          <span class="ctx-check">{{ settings.logShowHashCol ? "✓" : "" }}</span>
          提交哈希
        </button>
        <button class="ctx-check-item" @click="settings.toggleLogCol('email')">
          <span class="ctx-check">{{ settings.logShowEmailCol ? "✓" : "" }}</span>
          作者邮箱
        </button>
        <button class="ctx-check-item" @click="settings.toggleLogCol('changes')">
          <span class="ctx-check">{{ settings.logShowChangesCol ? "✓" : "" }}</span>
          改动行数
        </button>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="branchMenu"
        ref="branchMenuEl"
        class="ctx-menu"
        :style="{ left: branchMenu.x + 'px', top: branchMenu.y + 'px' }"
        @click.stop
        @contextmenu.prevent
      >
        <button :disabled="branchMenu.branch.isCurrent" @click="onBranchMenuCheckout">检出 '{{ branchMenu.branch.name }}'</button>
        <button @click="onBranchMenuNewFrom">从此新建分支…</button>
        <button :disabled="branchMenu.branch.isCurrent" @click="onBranchMenuMerge">合并到当前分支</button>
        <template v-if="!branchMenu.branch.isRemote">
          <div class="ctx-sep"></div>
          <button @click="onBranchMenuRename">重命名…</button>
          <button :disabled="branchMenu.branch.isCurrent" @click="onBranchMenuDelete">删除…</button>
        </template>
      </div>
    </Teleport>
  </aside>
</template>
