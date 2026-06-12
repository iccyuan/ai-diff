<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";
import type { FileStatus } from "../lib/api";

const repo = useRepoStore();
const scroller = ref<HTMLElement | null>(null);

const BADGE: Record<string, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

const COMMIT_ROW_PX = 52;

const AVATAR_COLORS = ["#0969da", "#2da44e", "#8250df", "#cf222e", "#d29922", "#0e7490", "#bf3989"];

function authorColor(name: string): string {
  let sum = 0;
  for (const ch of name) sum += ch.codePointAt(0) ?? 0;
  return AVATAR_COLORS[sum % AVATAR_COLORS.length];
}

function fitPageSize() {
  const h = scroller.value?.clientHeight ?? 600;
  // fill the visible height plus one extra screen so the scrollbar exists
  repo.commitPageSize = Math.max(20, Math.ceil((h / COMMIT_ROW_PX) * 2));
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
        <div class="commit-row" @click="repo.toggleCommit(c.hash)">
          <svg
            class="chevron"
            :class="{ open: repo.expandedCommits.includes(c.hash) }"
            viewBox="0 0 16 16"
            aria-hidden="true"
          >
            <path d="M5.7 13.7 5 13l4.6-4.6L5 3.7l.7-.7 5.3 5.3z" />
          </svg>
          <span class="avatar" :style="{ background: authorColor(c.author) }" :title="c.author">{{
            c.author.slice(0, 1).toUpperCase()
          }}</span>
          <div class="commit-main">
            <div class="subject" :title="c.subject">{{ c.subject }}</div>
            <div class="meta">
              <span class="hash">{{ c.shortHash }}</span>
              <span class="author">{{ c.author }}</span>
              <span>{{ c.date }}</span>
              <span class="stats">
                <span class="add">+{{ c.additions }}</span>
                <span class="del">−{{ c.deletions }}</span>
              </span>
            </div>
          </div>
        </div>
        <ul v-if="repo.expandedCommits.includes(c.hash)" class="commit-files">
          <li v-if="!repo.commitFiles[c.hash]" class="muted loading-files">加载文件…</li>
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
      <div v-if="repo.loadingCommits" class="list-tail muted">加载中…</div>
      <div v-else-if="repo.commitsExhausted && repo.commits.length" class="list-tail muted">已到最早提交</div>
      <div v-else-if="!repo.commits.length" class="list-tail muted">暂无提交历史</div>
    </div>
  </aside>
</template>
