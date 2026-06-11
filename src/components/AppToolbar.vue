<script setup lang="ts">
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";

const repo = useRepoStore();
const settings = useSettingsStore();
const emit = defineEmits<{ (e: "open-settings"): void }>();
const recentSel = ref("");

async function pickFolder() {
  const dir = await open({ directory: true, title: "选择 git 仓库目录" });
  if (typeof dir === "string") await repo.openRepo(dir);
}

async function openRecent() {
  if (!recentSel.value) return;
  const path = recentSel.value;
  recentSel.value = "";
  await repo.openRepo(path);
}

async function revertAll() {
  const ok = await confirmDialog(
    "还原全部更改",
    "将丢弃所有未提交的更改，并删除全部未跟踪文件（.gitignore 中的文件不受影响）。此操作不可撤销，确定继续吗？",
  );
  if (ok) await repo.revertAll();
}

function repoName(root: string): string {
  return root.split("/").pop() ?? root;
}
</script>

<template>
  <header class="toolbar">
    <button class="btn primary" @click="pickFolder">打开项目</button>
    <select v-if="settings.recentRepos.length" v-model="recentSel" class="recent" @change="openRecent">
      <option value="" disabled>最近打开…</option>
      <option v-for="p in settings.recentRepos" :key="p" :value="p">{{ p }}</option>
    </select>

    <div v-if="repo.repo" class="repo-info">
      <span class="repo-name">{{ repoName(repo.repo.root) }}</span>
      <span v-if="repo.repo.branch" class="branch">⎇ {{ repo.repo.branch }}</span>
      <span v-else-if="!repo.repo.hasHead" class="branch warn">空仓库（无提交）</span>
    </div>

    <div class="spacer"></div>

    <button class="btn" :disabled="!repo.repo" title="重新读取更改列表" @click="repo.refresh()">⟳ 刷新</button>
    <button
      class="btn danger"
      :disabled="!repo.repo || !repo.repo.hasHead || !repo.files.length"
      title="丢弃所有未提交更改"
      @click="revertAll"
    >
      还原全部
    </button>
    <button class="btn" title="设置" @click="emit('open-settings')">⚙</button>
  </header>
</template>
