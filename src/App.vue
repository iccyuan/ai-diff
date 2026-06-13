<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import AppToolbar from "./components/AppToolbar.vue";
import FileList from "./components/FileList.vue";
import DiffView from "./components/DiffView.vue";
import ViewTabs from "./components/ViewTabs.vue";
import SummaryView from "./components/SummaryView.vue";
import HistoryPanel from "./components/HistoryPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import ToastHost from "./components/ToastHost.vue";
import QuickOpen from "./components/QuickOpen.vue";
import SearchPanel from "./components/SearchPanel.vue";
import SymbolChooser from "./components/SymbolChooser.vue";
import { openQuickOpen, openFindDialog } from "./lib/palette";
import { checkForUpdate } from "./lib/update";
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

// Eclipse shortcuts: Ctrl+Shift+R open resource, Ctrl+H file search
function onGlobalKey(e: KeyboardEvent) {
  if (!repo.repo || !e.ctrlKey) return;
  if (e.shiftKey && e.code === "KeyR") {
    e.preventDefault();
    e.stopPropagation();
    openQuickOpen();
  } else if (!e.shiftKey && !e.altKey && e.code === "KeyH") {
    e.preventDefault();
    e.stopPropagation();
    openFindDialog();
  }
}

onMounted(async () => {
  // the window starts hidden (tauri.conf visible:false) and is revealed only
  // after Vue mounted with the persisted theme applied — no startup flash
  getCurrentWindow().show().catch(() => {});
  window.addEventListener("keydown", onGlobalKey, true);
  // drag a folder (or any file inside a repo) onto the window to open it
  await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === "drop" && e.payload.paths.length) {
      repo.openRepo(e.payload.paths[0]);
    }
  });
  // `AI_DIFF_OPEN_REPO=<path>` (or legacy VITE_OPEN_REPO) opens a repo on launch;
  // resolved by the Rust side so it works regardless of vite env plumbing
  const auto = await invoke<string | null>("auto_open_path");
  if (auto) useRepoStore().openRepo(auto);
  // silent update check shortly after launch (main window, prod builds only)
  setTimeout(() => checkForUpdate(false), 3000);
});
</script>

<template>
  <div class="app">
    <AppToolbar @open-settings="settingsOpen = true" />
    <div class="body">
      <FileList />
      <div class="resizer" title="拖动调整宽度" @pointerdown="startResize"></div>
      <div class="center-col">
        <ViewTabs />
        <SummaryView v-if="repo.ws && !repo.activeTabId" />
        <DiffView v-show="!(repo.ws && !repo.activeTabId)" />
      </div>
      <HistoryPanel v-if="repo.historyOpen && repo.repo" />
    </div>
    <SettingsPanel :open="settingsOpen" @close="settingsOpen = false" />
    <QuickOpen />
    <SearchPanel />
    <SymbolChooser />
    <ConfirmDialog />
    <ToastHost />
  </div>
</template>
