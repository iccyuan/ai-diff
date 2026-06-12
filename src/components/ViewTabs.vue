<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";

const repo = useRepoStore();

const menu = ref<{ x: number; y: number; tabId: string } | null>(null);

function openMenu(e: MouseEvent, tabId: string) {
  menu.value = { x: e.clientX, y: e.clientY, tabId };
}

function closeMenu() {
  menu.value = null;
}

function menuIndex(): number {
  return menu.value ? repo.tabs.findIndex((t) => t.id === menu.value!.tabId) : -1;
}

function hasLeft(): boolean {
  return menuIndex() > 0;
}

function hasRight(): boolean {
  const i = menuIndex();
  return i >= 0 && i < repo.tabs.length - 1;
}

function hasOthers(): boolean {
  return repo.tabs.length > 1;
}

function run(action: (id: string) => void) {
  if (menu.value) action(menu.value.tabId);
  closeMenu();
}

onMounted(() => {
  window.addEventListener("click", closeMenu);
  window.addEventListener("blur", closeMenu);
});

onBeforeUnmount(() => {
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("blur", closeMenu);
});
</script>

<template>
  <div v-if="repo.tabs.length" class="view-tabs">
    <div
      v-for="t in repo.tabs"
      :key="t.id"
      class="vtab"
      :class="{ active: t.id === repo.activeTabId }"
      :title="(t.commit ? `${t.commit.slice(0, 7)} · ` : '') + t.path"
      @click="repo.activateTab(t.id)"
      @mousedown.middle.prevent="repo.closeTab(t.id)"
      @contextmenu.prevent="openMenu($event, t.id)"
    >
      <img class="vicon" :src="fileIcon(t.path)" alt="" />
      <span class="vtitle">{{ t.title }}</span>
      <span v-if="t.commit" class="vhash">{{ t.commit.slice(0, 7) }}</span>
      <button class="vclose" title="关闭" @click.stop="repo.closeTab(t.id)">✕</button>
    </div>

    <Teleport to="body">
      <div
        v-if="menu"
        class="ctx-menu"
        :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
        @contextmenu.prevent
      >
        <button @click="run((id) => repo.closeTab(id))">关闭</button>
        <button :disabled="!hasOthers()" @click="run((id) => repo.closeOtherTabs(id))">关闭其他</button>
        <button :disabled="!hasLeft()" @click="run((id) => repo.closeLeftTabs(id))">关闭左边的</button>
        <button :disabled="!hasRight()" @click="run((id) => repo.closeRightTabs(id))">关闭右边的</button>
        <div class="ctx-sep"></div>
        <button @click="run(() => repo.closeAllTabs())">全部关闭</button>
      </div>
    </Teleport>
  </div>
</template>
