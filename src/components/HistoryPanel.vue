<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";
import { confirmDialog } from "../lib/confirm";
import { promptDialog } from "../lib/prompt";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { buildCommitGraph, graphWidth, type GraphRow } from "../lib/commitGraph";
import Spinner from "./Spinner.vue";
import type { CommitInfo, FileStatus, ResetMode } from "../lib/api";

const repo = useRepoStore();
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

// right-click menu: cherry-pick / revert / reset current branch to here.
// disabled for merge commits (>1 parent) — no -m mainline picker offered.
const menu = ref<{ x: number; y: number; commit: CommitInfo } | null>(null);
const menuEl = ref<HTMLElement | null>(null);

function openCommitMenu(e: MouseEvent, c: CommitInfo) {
  menu.value = { x: e.clientX, y: e.clientY, commit: c };
}
function closeMenu() {
  menu.value = null;
}
function onDocPointer(e: MouseEvent) {
  if (!menuEl.value?.contains(e.target as Node)) closeMenu();
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

async function onResetTo(mode: ResetMode) {
  const c = menu.value?.commit;
  closeMenu();
  if (!c || !repo.repo) return;
  if (mode === "hard") {
    let count: number | null = null;
    try {
      count = await api.countCommitsBetween(repo.repo.root, c.hash, "HEAD");
    } catch (e) {
      toast(String(e), "error");
      return;
    }
    const ok = await confirmDialog(
      "重置分支（硬重置）",
      `确定要将当前分支重置到「${c.subject}」吗？这将丢弃之后 ${count} 个提交，且工作区、暂存区中所有未提交的改动都会被清除。此操作不可撤销。`,
    );
    if (!ok) return;
  } else {
    const detail =
      mode === "soft"
        ? "分支指针会移动到这里，但索引和工作区保持不变——之后的改动会变为「已暂存」。"
        : "分支指针会移动到这里，工作区文件保持不变，但索引会重置——之后的改动会变为「未暂存」。";
    const ok = await confirmDialog(
      `重置分支（${mode === "soft" ? "soft" : "mixed"}）`,
      `确定要将当前分支重置到「${c.subject}」吗？${detail}`,
    );
    if (!ok) return;
  }
  await repo.resetTo(c.hash, mode);
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
          >
            <span class="branch-row-name muted">{{ b.name }}</span>
          </div>
          <div v-if="!filteredBranches.local.length && !filteredBranches.remote.length" class="branch-empty">没有匹配的分支</div>
        </div>
      </div>
      <div class="log-main">
        <div v-if="repo.logBranchFilter" class="log-filter-bar">
          <span>筛选：{{ repo.logBranchFilter }}</span>
          <button title="清除筛选" @click="repo.setLogBranchFilter(null)">✕</button>
        </div>
        <div class="log-content">
          <div class="log-table">
            <div class="log-header">
              <span class="log-header-graph"></span>
              <span class="log-header-subject">Subject</span>
              <span class="log-header-author">Author</span>
              <span class="log-header-date">Date</span>
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
                <span class="author-cell" :title="c.author">{{ c.author }}</span>
                <span class="date-cell" :title="c.date">{{ c.date }}</span>
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
        <button :disabled="menu.commit.parents.length > 1" @click="onCherryPick">Cherry-pick</button>
        <button :disabled="menu.commit.parents.length > 1" @click="onRevertCommit">Revert Commit</button>
        <button @click="onResetTo('soft')">重置当前分支到这里（soft）</button>
        <button @click="onResetTo('mixed')">重置当前分支到这里（mixed）</button>
        <button @click="onResetTo('hard')">重置当前分支到这里（hard）</button>
      </div>
    </Teleport>
  </aside>
</template>
