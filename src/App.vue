<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import AppToolbar from "./components/AppToolbar.vue";
import FileList from "./components/FileList.vue";
import DiffView from "./components/DiffView.vue";
import ViewTabs from "./components/ViewTabs.vue";
import SummaryView from "./components/SummaryView.vue";
import HistoryPanel from "./components/HistoryPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import FileInfoDialog from "./components/FileInfoDialog.vue";
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

// Eclipse shortcuts: Ctrl+Shift+R open resource, Ctrl+H file search.
// IDEA-style double-tap Shift also opens the file search.
let lastShiftAt = 0;
function onGlobalKey(e: KeyboardEvent) {
  if (!repo.repo) return;
  // double-tap Shift (no other modifier, no key in between) → file search
  if (e.key === "Shift" && !e.repeat && !e.ctrlKey && !e.altKey && !e.metaKey) {
    const now = Date.now();
    if (now - lastShiftAt < 400) {
      lastShiftAt = 0;
      e.preventDefault();
      openQuickOpen();
    } else {
      lastShiftAt = now;
    }
    return;
  }
  // any other key cancels a pending double-Shift
  if (e.key !== "Shift") lastShiftAt = 0;

  if (!e.ctrlKey) return;
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

// suppress webview/browser default actions that don't belong in a desktop app
function onContextMenu(e: MouseEvent) {
  // keep the native menu in real text fields (right-click paste); block elsewhere
  const t = e.target as HTMLElement | null;
  if (t?.closest("input, textarea, [contenteditable='true']")) return;
  e.preventDefault();
}
function blockBrowserDefaults(e: KeyboardEvent) {
  if (!e.ctrlKey && !e.metaKey) return;
  // print / save page / view-source / downloads — meaningless here
  if (!e.shiftKey && !e.altKey && ["KeyP", "KeyS", "KeyU", "KeyJ"].includes(e.code)) {
    e.preventDefault();
  }
}

// size the window to a portion of the screen so it isn't tiny on large /
// high-resolution displays (the fixed config size looked small there)
async function fitWindowToScreen() {
  try {
    const sw = window.screen.availWidth;
    const sh = window.screen.availHeight;
    if (!sw || !sh) return;
    const w = Math.max(900, Math.min(Math.round(sw * 0.8), 2200));
    const h = Math.max(600, Math.min(Math.round(sh * 0.85), 1450));
    const win = getCurrentWindow();
    await win.setSize(new LogicalSize(w, h));
    await win.center();
  } catch {
    /* fall back to the config size */
  }
}

onMounted(async () => {
  // the window starts hidden (tauri.conf visible:false) and is revealed only
  // after Vue mounted with the persisted theme applied — no startup flash
  await fitWindowToScreen();
  getCurrentWindow().show().catch(() => {});
  window.addEventListener("keydown", onGlobalKey, true);
  window.addEventListener("keydown", blockBrowserDefaults, true);
  window.addEventListener("contextmenu", onContextMenu);
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
    <FileInfoDialog />
    <ToastHost />
  </div>
</template>
