<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";

const repo = useRepoStore();
const settings = useSettingsStore();
const emit = defineEmits<{ (e: "open-settings"): void }>();

// custom recent-repos dropdown (a native <select> popup can't be glassed).
// the menu is teleported to <body> so its frosted-glass backdrop-filter blurs
// the app behind it — nested inside the (also glassed) toolbar it would only
// see the toolbar's empty backdrop and render as flat transparency.
const recentOpen = ref(false);
const recentWrap = ref<HTMLElement | null>(null);
const menuEl = ref<HTMLElement | null>(null);
const menuPos = ref({ x: 0, y: 0 });
function toggleRecent() {
  if (!recentOpen.value && recentWrap.value) {
    const r = recentWrap.value.getBoundingClientRect();
    menuPos.value = { x: r.left, y: r.bottom + 6 };
  }
  recentOpen.value = !recentOpen.value;
}
async function pickRecent(path: string) {
  recentOpen.value = false;
  await repo.openRepo(path);
}
// split a path (either separator) into its folder name and parent dir
function projName(p: string): string {
  return p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;
}
function projDir(p: string): string {
  const trimmed = p.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return i > 0 ? trimmed.slice(0, i) : "";
}
function onDocPointer(e: MouseEvent) {
  const t = e.target as Node;
  // the menu is teleported out of recentWrap, so check it separately
  const inWrap = recentWrap.value?.contains(t);
  const inMenu = menuEl.value?.contains(t);
  if (!inWrap && !inMenu) recentOpen.value = false;
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
</script>

<template>
  <header class="toolbar">
    <button class="btn primary" @click="pickFolder">打开项目</button>
    <div v-if="settings.recentRepos.length" ref="recentWrap" class="recent-wrap">
      <button class="btn recent-trigger" :class="{ 'toggle-on': recentOpen }" @click="toggleRecent">
        <span class="recent-trigger-icon">🕘</span>
        <span>最近打开</span>
        <svg class="recent-caret" :class="{ open: recentOpen }" viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
          <path d="M3 4.5 L6 7.5 L9 4.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <Teleport to="body">
        <div
          v-if="recentOpen"
          ref="menuEl"
          class="recent-menu"
          :style="{ left: menuPos.x + 'px', top: menuPos.y + 'px' }"
        >
          <div class="recent-head">最近打开</div>
          <button v-for="p in settings.recentRepos" :key="p" class="recent-item" :title="p" @click="pickRecent(p)">
            <span class="recent-icon">📁</span>
            <span class="recent-text">
              <span class="recent-name">{{ projName(p) }}</span>
              <span v-if="projDir(p)" class="recent-dir">{{ projDir(p) }}</span>
            </span>
          </button>
        </div>
      </Teleport>
    </div>


    <div class="spacer"></div>

    <button class="btn" :disabled="!repo.repo" title="改动总览" @click="repo.clearActiveView()">摘要</button>
    <button class="btn icon" :disabled="!repo.repo" title="重新读取更改列表（已自动监听，手动刷新作兜底）" @click="repo.refresh()">
      ⟳
    </button>
    <button class="btn icon" title="设置" @click="emit('open-settings')">⚙</button>
  </header>
</template>
