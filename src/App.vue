<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import AppToolbar from "./components/AppToolbar.vue";
import FileList from "./components/FileList.vue";
import DiffView from "./components/DiffView.vue";
import ViewTabs from "./components/ViewTabs.vue";
import HistoryPanel from "./components/HistoryPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import ToastHost from "./components/ToastHost.vue";
import QuickOpen from "./components/QuickOpen.vue";
import SearchPanel from "./components/SearchPanel.vue";
import { openQuickOpen, openSearch } from "./lib/palette";
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

// global shortcuts, Eclipse + IDEA flavors:
// open resource = Ctrl+Shift+R (Eclipse) / Ctrl+Shift+N (IDEA)
// text search   = Ctrl+H (Eclipse)       / Ctrl+Shift+F (IDEA)
function onGlobalKey(e: KeyboardEvent) {
  if (!repo.repo || !e.ctrlKey) return;
  const quickOpen = e.shiftKey && (e.code === "KeyR" || e.code === "KeyN");
  const search = (e.shiftKey && e.code === "KeyF") || (!e.shiftKey && !e.altKey && e.code === "KeyH");
  if (quickOpen) {
    e.preventDefault();
    e.stopPropagation();
    openQuickOpen();
  } else if (search) {
    e.preventDefault();
    e.stopPropagation();
    openSearch();
  }
}

onMounted(async () => {
  window.addEventListener("keydown", onGlobalKey, true);
  // `AI_DIFF_OPEN_REPO=<path>` (or legacy VITE_OPEN_REPO) opens a repo on launch;
  // resolved by the Rust side so it works regardless of vite env plumbing
  const auto = await invoke<string | null>("auto_open_path");
  if (auto) useRepoStore().openRepo(auto);
});
</script>

<template>
  <div class="app">
    <AppToolbar @open-settings="settingsOpen = true" />
    <div class="body">
      <FileList />
      <div class="resizer" title="拖动调整宽度" @pointerdown="startResize"></div>
      <div class="center">
        <ViewTabs />
        <DiffView />
      </div>
      <HistoryPanel v-if="repo.historyOpen && repo.repo" />
    </div>
    <SettingsPanel :open="settingsOpen" @close="settingsOpen = false" />
    <QuickOpen />
    <SearchPanel />
    <ConfirmDialog />
    <ToastHost />
  </div>
</template>
