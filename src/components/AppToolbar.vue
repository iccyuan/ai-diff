<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
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

function newWindow() {
  invoke("new_window");
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
    <button class="btn icon" title="新窗口（同时查看多个项目）" @click="newWindow">⧉</button>
    <select v-if="settings.recentRepos.length" v-model="recentSel" class="recent" @change="openRecent">
      <option value="" disabled>最近打开…</option>
      <option v-for="p in settings.recentRepos" :key="p" :value="p">{{ p }}</option>
    </select>

    <div v-if="repo.workspaces.length" class="ws-tabs">
      <div
        v-for="(w, i) in repo.workspaces"
        :key="w.repo.root"
        class="ws-tab"
        :class="{ active: i === repo.active }"
        :title="w.repo.root"
        @click="repo.activateWorkspace(i)"
        @mousedown.middle.prevent="repo.closeWorkspace(i)"
      >
        <span class="repo-name">{{ repoName(w.repo.root) }}</span>
        <span v-if="i === repo.active && w.repo.branch" class="branch">⎇ {{ w.repo.branch }}</span>
        <span v-else-if="i === repo.active && !w.repo.hasHead" class="branch warn">空仓库</span>
        <button class="vclose" title="关闭项目" @click.stop="repo.closeWorkspace(i)">✕</button>
      </div>
    </div>

    <div class="spacer"></div>

    <button class="btn icon" :disabled="!repo.repo" title="重新读取更改列表（已自动监听，手动刷新作兜底）" @click="repo.refresh()">
      ⟳
    </button>
    <button
      class="btn danger"
      :disabled="!repo.repo || !repo.repo.hasHead || !repo.files.length"
      title="丢弃所有未提交更改"
      @click="revertAll"
    >
      还原全部
    </button>
    <button
      class="btn"
      :class="{ 'toggle-on': repo.historyOpen }"
      :disabled="!repo.repo"
      title="显示/隐藏提交历史"
      @click="repo.toggleHistory()"
    >
      历史
    </button>
    <button class="btn icon" title="设置" @click="emit('open-settings')">⚙</button>
  </header>
</template>
