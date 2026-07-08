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
      class="git-logo-btn"
      :class="{ active: settings.gitPanelOpen }"
      title="Git（日志 / 分支 / 控制台）"
      @click="settings.toggleGitPanel()"
    >
      <!-- official Git logo mark (git-scm.com brand icon, #F05032) -->
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          fill="#F05032"
          d="M23.546 10.93L13.067.452c-.604-.603-1.582-.603-2.188 0L8.708 2.627l2.76 2.76c.645-.215 1.379-.07 1.889.441.516.515.658 1.258.438 1.9l2.658 2.66c.645-.223 1.387-.078 1.9.435.721.72.721 1.884 0 2.604-.719.719-1.881.719-2.6 0-.539-.541-.674-1.337-.404-1.996L12.86 8.955v6.525c.176.086.342.203.488.348.713.721.713 1.883 0 2.6-.719.721-1.889.721-2.609 0-.719-.719-.719-1.879 0-2.598.182-.18.387-.316.605-.406V8.835c-.217-.091-.424-.222-.6-.401-.545-.545-.676-1.342-.396-2.009L7.636 3.7.45 10.881c-.6.605-.6 1.584 0 2.189l10.48 10.477c.604.604 1.582.604 2.186 0l10.43-10.43c.605-.603.605-1.582 0-2.187"
        />
      </svg>
    </button>
  </nav>
</template>
