<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { cloneDialog } from "../lib/cloneDialog";

const repo = useRepoStore();
const settings = useSettingsStore();
const emit = defineEmits<{ (e: "open-settings"): void }>();

// IDEA-style merged title bar: native window decorations are off
// (tauri.conf.json), so this header doubles as the draggable title bar and
// draws its own minimize/maximize/close buttons.
const win = getCurrentWindow();
const isMaximized = ref(false);
async function refreshMaximized() {
  isMaximized.value = await win.isMaximized();
}
let unlistenResize: (() => void) | undefined;
onMounted(async () => {
  await refreshMaximized();
  unlistenResize = await win.onResized(() => refreshMaximized());
});
onBeforeUnmount(() => unlistenResize?.());

function minimizeWin() {
  win.minimize();
}
function toggleMaximizeWin() {
  win.toggleMaximize();
}
function closeWin() {
  win.close();
}

// two dropdown menus (文件 / 视图) — 首选项 is a separate top-level button
// with no dropdown of its own (a single action doesn't need one)
type MenuId = "file" | "view";
const openMenuId = ref<MenuId | null>(null);
const fileBtn = ref<HTMLElement | null>(null);
const viewBtn = ref<HTMLElement | null>(null);
const menuEl = ref<HTMLElement | null>(null);
const menuPos = ref({ x: 0, y: 0 });

function toggleMenu(id: MenuId, triggerEl: HTMLElement | null) {
  recentSubmenuOpen.value = false;
  if (openMenuId.value === id) {
    openMenuId.value = null;
    return;
  }
  if (triggerEl) {
    const r = triggerEl.getBoundingClientRect();
    menuPos.value = { x: r.left, y: r.bottom + 4 };
  }
  openMenuId.value = id;
}
function closeMenu() {
  openMenuId.value = null;
  recentSubmenuOpen.value = false;
}

// "最近打开" opens as its own flyout to the side, instead of inlining the
// (potentially long) recent-repos list into the 文件 dropdown itself
const recentBtn = ref<HTMLElement | null>(null);
const recentMenuEl = ref<HTMLElement | null>(null);
const recentSubmenuOpen = ref(false);
const recentSubmenuPos = ref({ x: 0, y: 0 });

function openRecentSubmenu() {
  if (recentSubmenuOpen.value) return;
  if (recentBtn.value) {
    const r = recentBtn.value.getBoundingClientRect();
    recentSubmenuPos.value = { x: r.right + 4, y: r.top };
  }
  recentSubmenuOpen.value = true;
}
function toggleRecentSubmenu() {
  if (recentSubmenuOpen.value) {
    recentSubmenuOpen.value = false;
    return;
  }
  openRecentSubmenu();
}

function onDocPointer(e: MouseEvent) {
  const t = e.target as Node;
  const inTriggers = fileBtn.value?.contains(t) || viewBtn.value?.contains(t);
  const inMenu = menuEl.value?.contains(t);
  const inRecentMenu = recentMenuEl.value?.contains(t);
  if (!inTriggers && !inMenu && !inRecentMenu) closeMenu();
}
function onEsc(e: KeyboardEvent) {
  if (e.key === "Escape") closeMenu();
}
onMounted(() => {
  window.addEventListener("click", onDocPointer);
  window.addEventListener("keydown", onEsc);
});
onBeforeUnmount(() => {
  window.removeEventListener("click", onDocPointer);
  window.removeEventListener("keydown", onEsc);
});

// split a path (either separator) into its folder name and parent dir
function projName(p: string): string {
  return p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;
}
function projDir(p: string): string {
  const trimmed = p.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return i > 0 ? trimmed.slice(0, i) : "";
}

async function pickFolder() {
  closeMenu();
  const dir = await open({ directory: true, title: "选择 git 仓库目录" });
  if (typeof dir === "string") await repo.openRepo(dir);
}
async function pickRecent(path: string) {
  closeMenu();
  await repo.openRepo(path);
}
async function onClone() {
  closeMenu();
  await cloneDialog();
}
function onSummary() {
  closeMenu();
  repo.clearActiveView();
}
function onRefresh() {
  closeMenu();
  repo.refresh();
}
function onOpenSettings() {
  closeMenu();
  emit("open-settings");
}
</script>

<template>
  <header class="toolbar" data-tauri-drag-region>
    <nav class="menu-bar">
      <button ref="fileBtn" class="menu-trigger" :class="{ active: openMenuId === 'file' }" @click="toggleMenu('file', fileBtn)">
        文件
      </button>
      <button ref="viewBtn" class="menu-trigger" :class="{ active: openMenuId === 'view' }" @click="toggleMenu('view', viewBtn)">
        视图
      </button>
      <button class="menu-trigger" @click="onOpenSettings">首选项</button>
    </nav>

    <div class="spacer" data-tauri-drag-region></div>

    <div class="win-controls">
      <button class="win-btn" title="最小化" @click="minimizeWin">
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" stroke="currentColor" stroke-width="1" /></svg>
      </button>
      <button class="win-btn" :title="isMaximized ? '还原' : '最大化'" @click="toggleMaximizeWin">
        <svg v-if="!isMaximized" viewBox="0 0 10 10" aria-hidden="true">
          <rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
        <svg v-else viewBox="0 0 10 10" aria-hidden="true">
          <rect x="0.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1" />
          <path d="M2.5 2.5V0.5h7v7h-2" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
      <button class="win-btn win-close" title="关闭" @click="closeWin">
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
    </div>

    <Teleport to="body">
      <div v-if="openMenuId === 'file'" ref="menuEl" class="recent-menu" :style="{ left: menuPos.x + 'px', top: menuPos.y + 'px' }">
        <!-- hovering a sibling item closes the flyout, like a native menu -->
        <button class="recent-item" @click="pickFolder" @mouseenter="recentSubmenuOpen = false">
          <span class="recent-icon">📂</span>
          <span class="recent-text"><span class="recent-name">打开项目…</span></span>
        </button>
        <button class="recent-item" @click="onClone" @mouseenter="recentSubmenuOpen = false">
          <span class="recent-icon">📥</span>
          <span class="recent-text"><span class="recent-name">克隆仓库…</span></span>
        </button>
        <div class="ctx-sep"></div>
        <button
          ref="recentBtn"
          class="recent-item"
          :class="{ active: recentSubmenuOpen }"
          @mouseenter="openRecentSubmenu"
          @click.stop="toggleRecentSubmenu"
        >
          <span class="recent-icon">🕘</span>
          <span class="recent-text"><span class="recent-name">最近打开</span></span>
          <span class="submenu-caret">›</span>
        </button>
      </div>

      <div
        v-if="recentSubmenuOpen"
        ref="recentMenuEl"
        class="recent-menu"
        :style="{ left: recentSubmenuPos.x + 'px', top: recentSubmenuPos.y + 'px' }"
      >
        <div class="recent-head">最近打开</div>
        <button v-for="p in settings.recentRepos" :key="p" class="recent-item" :title="p" @click="pickRecent(p)">
          <span class="recent-icon">📁</span>
          <span class="recent-text">
            <span class="recent-name">{{ projName(p) }}</span>
            <span v-if="projDir(p)" class="recent-dir">{{ projDir(p) }}</span>
          </span>
        </button>
        <div v-if="!settings.recentRepos.length" class="recent-empty">暂无最近打开的项目</div>
      </div>

      <div v-if="openMenuId === 'view'" ref="menuEl" class="recent-menu" :style="{ left: menuPos.x + 'px', top: menuPos.y + 'px' }">
        <button class="recent-item" :disabled="!repo.repo" @click="onSummary">
          <span class="recent-icon">📊</span>
          <span class="recent-text"><span class="recent-name">摘要</span></span>
        </button>
        <button class="recent-item" :disabled="!repo.repo" @click="onRefresh">
          <span class="recent-icon">🔄</span>
          <span class="recent-text"><span class="recent-name">刷新</span></span>
        </button>
      </div>
    </Teleport>
  </header>
</template>
