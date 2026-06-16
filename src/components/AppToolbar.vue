<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";

const repo = useRepoStore();
const settings = useSettingsStore();
const emit = defineEmits<{ (e: "open-settings"): void }>();

// custom recent-repos dropdown (a native <select> popup can't be glassed)
const recentOpen = ref(false);
const recentWrap = ref<HTMLElement | null>(null);
function toggleRecent() {
  recentOpen.value = !recentOpen.value;
}
async function pickRecent(path: string) {
  recentOpen.value = false;
  await repo.openRepo(path);
}
function onDocPointer(e: MouseEvent) {
  if (recentWrap.value && !recentWrap.value.contains(e.target as Node)) recentOpen.value = false;
}
function onEsc(e: KeyboardEvent) {
  if (e.key === "Escape") recentOpen.value = false;
}
onMounted(() => {
  window.addEventListener("click", onDocPointer);
  window.addEventListener("keydown", onEsc);
});
onBeforeUnmount(() => {
  window.removeEventListener("click", onDocPointer);
  window.removeEventListener("keydown", onEsc);
});

async function pickFolder() {
  const dir = await open({ directory: true, title: "选择 git 仓库目录" });
  if (typeof dir === "string") await repo.openRepo(dir);
}

async function revertAll() {
  const ok = await confirmDialog(
    "还原全部更改",
    "将丢弃所有未提交的更改，并删除全部未跟踪文件（.gitignore 中的文件不受影响）。此操作不可撤销，确定继续吗？",
  );
  if (ok) await repo.revertAll();
}

</script>

<template>
  <header class="toolbar">
    <button class="btn primary" @click="pickFolder">打开项目</button>
    <div v-if="settings.recentRepos.length" ref="recentWrap" class="recent-wrap">
      <button class="btn" @click="toggleRecent">最近打开 ▾</button>
      <div v-if="recentOpen" class="recent-menu">
        <button v-for="p in settings.recentRepos" :key="p" class="recent-item" :title="p" @click="pickRecent(p)">
          {{ p }}
        </button>
      </div>
    </div>


    <div class="spacer"></div>

    <button class="btn" :disabled="!repo.repo" title="改动总览" @click="repo.clearActiveView()">摘要</button>
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
