<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";

const repo = useRepoStore();
const strip = ref<HTMLElement | null>(null);
const canLeft = ref(false);
const canRight = ref(false);

const menu = ref<{ x: number; y: number; tabId: string } | null>(null);
const menuEl = ref<HTMLElement | null>(null);
let suppressMenu = false; // set when a right-drag pan happened, so it doesn't pop the menu
async function openMenu(e: MouseEvent, tabId: string) {
  if (suppressMenu) {
    suppressMenu = false;
    return;
  }
  menu.value = { x: e.clientX, y: e.clientY, tabId };
  // keep the menu inside the viewport (the rightmost tab would clip it otherwise)
  await nextTick();
  const el = menuEl.value;
  if (!el || !menu.value) return;
  const r = el.getBoundingClientRect();
  const pad = 8;
  const x = Math.max(pad, Math.min(menu.value.x, window.innerWidth - r.width - pad));
  const y = Math.max(pad, Math.min(menu.value.y, window.innerHeight - r.height - pad));
  menu.value = { ...menu.value, x, y };
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
  stopAuto();
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
let lastX = 0;
let autoTimer: number | null = null;

// move the dragged tab to whatever tab sits under x.
// hit-test against layout positions (offsetLeft), NOT getBoundingClientRect:
// during the FLIP slide the rects are mid-transform, which made the drop
// slot oscillate and the tabs flicker. offsetLeft ignores the transform.
function reorderAt(x: number) {
  const s = drag.value;
  const track = strip.value?.querySelector<HTMLElement>(".tab-track");
  if (!s || !track) return;
  const px = x - track.getBoundingClientRect().left;
  const tabs = [...track.querySelectorAll<HTMLElement>(".vtab")];
  const over = tabs.findIndex((el) => px >= el.offsetLeft && px <= el.offsetLeft + el.offsetWidth);
  if (over >= 0 && over !== s.index) {
    repo.moveTab(s.index, over);
    s.index = over;
  }
}

// while dragging near a strip edge, keep scrolling so off-screen slots are reachable
function autoTick() {
  const el = strip.value;
  if (!el || !drag.value) return;
  const r = el.getBoundingClientRect();
  const EDGE = 52;
  const STEP = 16;
  let d = 0;
  if (lastX < r.left + EDGE) d = -STEP;
  else if (lastX > r.right - EDGE) d = STEP;
  if (!d) return;
  const before = el.scrollLeft;
  el.scrollLeft = Math.max(0, Math.min(el.scrollWidth - el.clientWidth, el.scrollLeft + d));
  if (el.scrollLeft !== before) reorderAt(lastX);
}

function onTabPointerMove(e: PointerEvent) {
  const s = drag.value;
  if (!s) return;
  lastX = e.clientX;
  if (!s.moved && Math.abs(e.clientX - s.startX) < 6) return;
  s.moved = true;
  reorderAt(e.clientX);
  if (autoTimer == null) autoTimer = window.setInterval(autoTick, 16);
}
function stopAuto() {
  if (autoTimer != null) {
    clearInterval(autoTimer);
    autoTimer = null;
  }
}
function onTabPointerUp() {
  window.removeEventListener("pointermove", onTabPointerMove);
  stopAuto();
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
      <div v-if="menu" ref="menuEl" class="ctx-menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }" @contextmenu.prevent>
        <button @click="run((id) => repo.closeTab(id))">关闭</button>
        <button :disabled="!hasOthers()" @click="run((id) => repo.closeOtherTabs(id))">关闭其他</button>
        <button :disabled="!hasLeft()" @click="run((id) => repo.closeLeftTabs(id))">关闭左边</button>
        <button :disabled="!hasRight()" @click="run((id) => repo.closeRightTabs(id))">关闭右边</button>
        <div class="ctx-sep"></div>
        <button @click="run(() => repo.closeAllTabs())">全部关闭</button>
      </div>
    </Teleport>
  </div>
</template>
