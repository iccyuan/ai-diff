<script setup lang="ts">
import { useRepoStore } from "../stores/repo";
import { confirmDialog } from "../lib/confirm";
import type { FileStatus } from "../lib/api";

const repo = useRepoStore();

const BADGE: Record<string, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

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
  if (!repo.files.length) return;
  const idx = repo.files.findIndex((f) => f.path === repo.selectedPath);
  const next = idx < 0 ? 0 : Math.min(Math.max(idx + delta, 0), repo.files.length - 1);
  repo.selectFile(repo.files[next]);
}
</script>

<template>
  <aside class="file-list" tabindex="0" @keydown.up.prevent="move(-1)" @keydown.down.prevent="move(1)">
    <div class="list-header">
      <span>更改的文件</span>
      <span class="count">{{ repo.files.length }}</span>
      <span v-if="repo.loadingStatus" class="muted">刷新中…</span>
    </div>
    <div v-if="repo.repo && !repo.loadingStatus && !repo.files.length" class="list-empty">
      工作区干净，没有未提交的更改
    </div>
    <ul>
      <li
        v-for="f in repo.files"
        :key="f.path"
        :class="{ active: f.path === repo.selectedPath }"
        @click="repo.selectFile(f)"
      >
        <span class="badge" :class="f.kind">{{ BADGE[f.kind] }}</span>
        <span class="path" :title="f.oldPath ? `${f.oldPath} → ${f.path}` : f.path">
          <template v-if="f.oldPath">{{ f.oldPath }} → </template>{{ f.path }}
        </span>
        <button class="row-revert" :title="f.kind === 'untracked' ? '删除此文件' : '还原此文件'" @click.stop="revert(f)">
          ⟲
        </button>
      </li>
    </ul>
  </aside>
</template>
