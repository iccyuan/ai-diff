<script setup lang="ts">
import { onMounted, ref } from "vue";
import AppToolbar from "./components/AppToolbar.vue";
import FileList from "./components/FileList.vue";
import DiffView from "./components/DiffView.vue";
import HistoryPanel from "./components/HistoryPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import ToastHost from "./components/ToastHost.vue";
import { useRepoStore } from "./stores/repo";
import { useSettingsStore } from "./stores/settings";

const settingsOpen = ref(false);
const settings = useSettingsStore();
const repo = useRepoStore();

function startResize(e: PointerEvent) {
  const startX = e.clientX;
  const startW = settings.sidebarWidth;
  const el = e.target as HTMLElement;
  el.setPointerCapture(e.pointerId);
  const move = (ev: PointerEvent) => {
    settings.sidebarWidth = Math.min(600, Math.max(200, startW + ev.clientX - startX));
  };
  const up = (ev: PointerEvent) => {
    el.releasePointerCapture(ev.pointerId);
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    settings.saveSidebarWidth();
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
}

onMounted(() => {
  // dev convenience: `VITE_OPEN_REPO=<path> npm run tauri dev` opens a repo on launch
  const auto = import.meta.env.VITE_OPEN_REPO;
  if (import.meta.env.DEV && typeof auto === "string" && auto) {
    useRepoStore().openRepo(auto);
  }
});
</script>

<template>
  <div class="app">
    <AppToolbar @open-settings="settingsOpen = true" />
    <div class="body">
      <FileList />
      <div class="resizer" title="拖动调整宽度" @pointerdown="startResize"></div>
      <DiffView />
      <HistoryPanel v-if="repo.historyOpen && repo.repo" />
    </div>
    <SettingsPanel :open="settingsOpen" @close="settingsOpen = false" />
    <ConfirmDialog />
    <ToastHost />
  </div>
</template>
