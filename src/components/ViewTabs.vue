<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";

const repo = useRepoStore();
const strip = ref<HTMLElement | null>(null);
const canLeft = ref(false);
const canRight = ref(false);

const menu = ref<{ x: number; y: number; tabId: string } | null>(null);
let suppressMenu = false; // set when a right-drag pan happened, so it doesn't pop the menu
function openMenu(e: MouseEvent, tabId: string) {
  if (suppressMenu) {
    suppressMenu = false;
    return;
  }
  menu.value = { x: e.clientX, y: e.clientY, tabId };
}
function closeMenu() {
  menu.value = null;
}
function menuIndex(): number {
  return menu.value ? repo.tabs.findIndex((t) => t.id === menu.value!.tabId) : -1;
}
function hasLeft() {
  return menuIndex() > 0;
}
function hasRight() {
  const i = menuIndex();
  return i >= 0 && i < repo.tabs.length - 1;
}
function hasOthers() {
  return repo.tabs.length > 1;
}
function run(action: (id: string) => void) {
  if (menu.value) action(menu.value.tabId);
  closeMenu();
}

// ----- Notepad++ style overflow arrows -----
function updateArrows() {
  const el = strip.value;
  if (!el) {
    canLeft.value = canRight.value = false;
    return;
  }
  canLeft.value = el.scrollLeft > 1;
  canRight.value = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
}
function scrollBy(dir: number) {
  strip.value?.scrollBy({ left: dir * strip.value.clientWidth * 0.7, behavior: "smooth" });
}
function onWheel(e: WheelEvent) {
  if (!strip.value) return;
  // vertical wheel scrolls the tab strip horizontally
  if (e.deltaY && !e.shiftKey) {
    strip.value.scrollLeft += e.deltaY;
    e.preventDefault();
  }
}

let ro: ResizeObserver | null = null;
onMounted(() => {
  window.addEventListener("click", closeMenu);
  window.addEventListener("blur", closeMenu);
  if (strip.value) {
    ro = new ResizeObserver(updateArrows);
    ro.observe(strip.value);
  }
  updateArrows();
});
onBeforeUnmount(() => {
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("blur", closeMenu);
  window.removeEventListener("pointermove", onTabPointerMove);
  window.removeEventListener("pointermove", onPanMove);
  ro?.disconnect();
});

// scroll the active tab into view & refresh arrows whenever tabs change
watch(
  () => [repo.activeTabId, repo.tabs.length] as const,
  async () => {
    await nextTick();
    strip.value?.querySelector(".vtab.active")?.scrollIntoView({ inline: "nearest", block: "nearest" });
    updateArrows();
  },
);

// ----- drag to reorder (pointer events; HTML5 DnD is taken by Tauri) -----
const drag = ref<{ index: number; id: string; startX: number; moved: boolean } | null>(null);

function onTabPointerMove(e: PointerEvent) {
  const s = drag.value;
  if (!s) return;
  if (!s.moved && Math.abs(e.clientX - s.startX) < 6) return;
  s.moved = true;
  const tabs = [...(strip.value?.querySelectorAll<HTMLElement>(".vtab") ?? [])];
  const over = tabs.findIndex((el) => {
    const r = el.getBoundingClientRect();
    return e.clientX >= r.left && e.clientX <= r.right;
  });
  if (over >= 0 && over !== s.index) {
    repo.moveTab(s.index, over);
    s.index = over;
  }
}
function onTabPointerUp() {
  window.removeEventListener("pointermove", onTabPointerMove);
  const s = drag.value;
  drag.value = null;
  if (s && !s.moved) repo.activateTab(s.id); // no drag = plain click
}
function onTabPointerDown(i: number, id: string, e: PointerEvent) {
  if (e.button !== 0 || (e.target as HTMLElement).closest(".vclose")) return;
  e.preventDefault(); // stop the browser starting a text selection on the title
  drag.value = { index: i, id, startX: e.clientX, moved: false };
  window.addEventListener("pointermove", onTabPointerMove);
  window.addEventListener("pointerup", onTabPointerUp, { once: true });
}

// ----- right-button drag pans (scrolls) the whole tab strip -----
let pan: { startX: number; startScroll: number; moved: boolean } | null = null;
function onPanMove(e: PointerEvent) {
  if (!pan || !strip.value) return;
  if (!pan.moved && Math.abs(e.clientX - pan.startX) < 4) return;
  pan.moved = true;
  strip.value.scrollLeft = pan.startScroll - (e.clientX - pan.startX);
}
function onPanUp() {
  window.removeEventListener("pointermove", onPanMove);
  if (pan?.moved) suppressMenu = true; // a pan happened → swallow the context menu
  pan = null;
  strip.value?.classList.remove("panning");
}
function onStripPointerDown(e: PointerEvent) {
  if (e.button !== 2 || !strip.value) return;
  pan = { startX: e.clientX, startScroll: strip.value.scrollLeft, moved: false };
  strip.value.classList.add("panning");
  window.addEventListener("pointermove", onPanMove);
  window.addEventListener("pointerup", onPanUp, { once: true });
}
</script>

<template>
  <div v-if="repo.tabs.length" class="tabs-bar">
    <button v-if="canLeft" class="tab-arrow" title="向左滚动" @click="scrollBy(-1)">‹</button>
    <div
      ref="strip"
      class="view-tabs"
      @scroll.passive="updateArrows"
      @wheel="onWheel"
      @pointerdown="onStripPointerDown"
    >
      <TransitionGroup tag="div" name="tab" class="tab-track">
        <div
          v-for="(t, i) in repo.tabs"
          :key="t.id"
          class="vtab"
          :class="{ active: t.id === repo.activeTabId, dragging: drag?.moved && drag.id === t.id }"
          :title="(t.commit ? `${t.commit.slice(0, 7)} · ` : '') + t.path"
          @pointerdown="onTabPointerDown(i, t.id, $event)"
          @mousedown.middle.prevent="repo.closeTab(t.id)"
          @contextmenu.prevent="openMenu($event, t.id)"
        >
          <img class="vicon" :src="fileIcon(t.path)" alt="" />
          <span class="vtitle">{{ t.title }}</span>
          <span v-if="t.commit" class="vhash">{{ t.commit.slice(0, 7) }}</span>
          <button class="vclose" title="关闭" @click.stop="repo.closeTab(t.id)">✕</button>
        </div>
      </TransitionGroup>
    </div>
    <button v-if="canRight" class="tab-arrow" title="向右滚动" @click="scrollBy(1)">›</button>

    <Teleport to="body">
      <div v-if="menu" class="ctx-menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }" @contextmenu.prevent>
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
