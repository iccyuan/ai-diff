<script setup lang="ts">
import { onMounted, ref } from "vue";
import AppToolbar from "./components/AppToolbar.vue";
import FileList from "./components/FileList.vue";
import DiffView from "./components/DiffView.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import ToastHost from "./components/ToastHost.vue";
import { useRepoStore } from "./stores/repo";

const settingsOpen = ref(false);

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
      <DiffView />
    </div>
    <SettingsPanel :open="settingsOpen" @close="settingsOpen = false" />
    <ConfirmDialog />
    <ToastHost />
  </div>
</template>
