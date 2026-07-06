<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";
import { confirmDialog } from "../lib/confirm";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import Avatar from "./Avatar.vue";
import Spinner from "./Spinner.vue";
import type { CommitInfo, FileStatus, ResetMode } from "../lib/api";

const repo = useRepoStore();
const scroller = ref<HTMLElement | null>(null);

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

const COMMIT_ROW_PX = 68;

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

/** GitHub-style diffstat: five blocks split between added / deleted / neutral */
function statBlocks(c: CommitInfo): ("a" | "d" | "n")[] {
  const total = c.additions + c.deletions;
  if (!total) return ["n", "n", "n", "n", "n"];
  let a = Math.round((c.additions / total) * 5);
  let d = Math.round((c.deletions / total) * 5);
  if (c.additions > 0) a = Math.max(a, 1);
  if (c.deletions > 0) d = Math.max(d, 1);
  while (a + d > 5) {
    if (a >= d) a--;
    else d--;
  }
  return [
    ...Array<"a">(a).fill("a"),
    ...Array<"d">(d).fill("d"),
    ...Array<"n">(5 - a - d).fill("n"),
  ];
}

onMounted(() => {
  fitPageSize();
  window.addEventListener("resize", fitPageSize);
  if (!repo.commits.length) repo.loadCommits();
});

onBeforeUnmount(() => window.removeEventListener("resize", fitPageSize));
</script>

<template>
  <aside class="history">
    <div class="history-header">
      <span>提交历史</span>
      <button class="btn icon close" title="关闭历史面板" @click="repo.toggleHistory()">✕</button>
    </div>
    <div ref="scroller" class="commits" @scroll.passive="onScroll">
      <div v-for="c in repo.commits" :key="c.hash" class="commit">
        <div class="commit-row" @click="repo.toggleCommit(c.hash)" @contextmenu.prevent.stop="openCommitMenu($event, c)">
          <svg
            class="chevron"
            :class="{ open: repo.expandedCommits.includes(c.hash) }"
            viewBox="0 0 16 16"
            aria-hidden="true"
          >
            <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
          </svg>
          <Avatar :author="c.author" :email="c.email" />
          <div class="commit-main">
            <div class="subject" :title="c.subject">{{ c.subject }}</div>
            <div class="meta">
              <span class="hash">{{ c.shortHash }}</span>
              <span class="author">{{ c.author }}</span>
              <span>{{ c.date }}</span>
              <span class="stats" :title="`+${c.additions} −${c.deletions}`">
                <span class="add">+{{ c.additions }}</span>
                <span class="del">−{{ c.deletions }}</span>
                <span class="stat-blocks">
                  <i v-for="(b, i) in statBlocks(c)" :key="i" :class="b"></i>
                </span>
              </span>
            </div>
          </div>
        </div>
        <ul v-if="repo.expandedCommits.includes(c.hash)" class="commit-files">
          <li v-if="!repo.commitFiles[c.hash]" class="muted loading-files"><Spinner :size="14" /></li>
          <li
            v-for="f in repo.commitFiles[c.hash] ?? []"
            :key="f.path"
            :class="{ active: repo.selectedCommit === c.hash && repo.selectedCommitPath === f.path }"
            :title="f.oldPath ? `${f.oldPath} → ${f.path}` : f.path"
            @click="repo.selectCommitFile(c.hash, f)"
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
      </div>
      <div v-if="repo.loadingCommits" class="list-tail muted"><Spinner :size="18" /></div>
      <div v-else-if="repo.commitsExhausted && repo.commits.length" class="list-tail muted">已到最早提交</div>
      <div v-else-if="!repo.commits.length" class="list-tail muted">暂无提交历史</div>
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
