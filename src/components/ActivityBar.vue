<script setup lang="ts">
import { watch } from "vue";
import { useSettingsStore } from "../stores/settings";
import { useRepoStore } from "../stores/repo";

const settings = useSettingsStore();
const repo = useRepoStore();

type PanelId = "project" | "commit";
const PANELS: { id: PanelId; label: string }[] = [
  { id: "project", label: "项目" },
  { id: "commit", label: "提交" },
];

// clicking the already-active panel's icon collapses it instead of
// re-switching to it — matches IDEA's tool window stripe behavior
function select(id: PanelId) {
  if (settings.activeLeftPanel === id && !settings.sidebarCollapsed) {
    settings.toggleSidebarCollapsed();
  } else {
    settings.setActiveLeftPanel(id);
  }
}

// the file-tree mode (更改/全部文件) follows the active panel; the Git panel
// doesn't render a file tree, so it leaves whatever mode was last active
watch(
  () => settings.activeLeftPanel,
  (p) => {
    if (p === "project") repo.setMode("all");
    else if (p === "commit") repo.setMode("changes");
  },
  { immediate: true },
);
</script>

<template>
  <nav class="activity-bar">
    <button
      v-for="p in PANELS"
      :key="p.id"
      :class="{ active: settings.activeLeftPanel === p.id && !settings.sidebarCollapsed }"
      :title="p.label"
      @click="select(p.id)"
    >
      {{ p.label }}
    </button>
    <div class="activity-bar-spacer"></div>
    <button
      :class="{ active: settings.gitPanelOpen }"
      title="Git（日志 / 分支 / 控制台）"
      @click="settings.toggleGitPanel()"
    >
      Git
    </button>
  </nav>
</template>
