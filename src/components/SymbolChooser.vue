<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { palette, closeChooser } from "../lib/palette";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";
import type { SearchHit } from "../lib/api";

const repo = useRepoStore();
const box = ref<HTMLElement | null>(null);
const active = ref(0);

watch(
  () => palette.chooser.open,
  async (open) => {
    if (!open) return;
    active.value = 0;
    await nextTick();
    box.value?.focus();
  },
);

const style = computed(() => {
  const w = 480;
  const h = Math.min(palette.chooser.hits.length * 30 + 38, 320);
  const x = Math.min(palette.chooser.x, window.innerWidth - w - 12);
  const y = palette.chooser.y + h > window.innerHeight - 12 ? palette.chooser.y - h - 8 : palette.chooser.y;
  return { left: Math.max(8, x) + "px", top: Math.max(8, y) + "px", width: w + "px" };
});

function pick(h: SearchHit) {
  closeChooser();
  repo.openAtLine(h.path, h.line);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    active.value = Math.min(active.value + 1, palette.chooser.hits.length - 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    active.value = Math.max(active.value - 1, 0);
  } else if (e.key === "Enter") {
    const h = palette.chooser.hits[active.value];
    if (h) pick(h);
  } else if (e.key === "Escape") {
    closeChooser();
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="palette.chooser.open" class="chooser-backdrop" @mousedown.self="closeChooser()">
      <div ref="box" class="symbol-chooser" :style="style" tabindex="0" @keydown="onKey">
        <div class="chooser-title">「{{ palette.chooser.word }}」的 {{ palette.chooser.hits.length }} 个位置</div>
        <ul>
          <li
            v-for="(h, i) in palette.chooser.hits"
            :key="i"
            :class="{ active: i === active }"
            @click="pick(h)"
            @mousemove="active = i"
          >
            <img class="vicon" :src="fileIcon(h.path)" alt="" />
            <span class="ppath">{{ h.path }}<span class="pline">:{{ h.line }}</span></span>
            <span class="ptext">{{ h.text.trim() }}</span>
          </li>
        </ul>
      </div>
    </div>
  </Teleport>
</template>
