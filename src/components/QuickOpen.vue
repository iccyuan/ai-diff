<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { palette } from "../lib/palette";

function closePalettes() {
  palette.quickOpen = false;
}
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";

const repo = useRepoStore();
const input = ref<HTMLInputElement | null>(null);
const query = ref("");
const active = ref(0);

watch(
  () => palette.quickOpen,
  async (open) => {
    if (!open) return;
    query.value = "";
    active.value = 0;
    await repo.ensureAllFiles();
    await nextTick();
    input.value?.focus();
  },
);

interface Scored {
  path: string;
  score: number;
}

/** simple subsequence fuzzy match; basename hits rank far higher */
function score(path: string, q: string): number {
  const name = path.split("/").pop()!.toLowerCase();
  const lower = path.toLowerCase();
  if (name.startsWith(q)) return 1000 - path.length;
  if (name.includes(q)) return 800 - path.length;
  if (lower.includes(q)) return 500 - path.length;
  let i = 0;
  for (const ch of lower) {
    if (ch === q[i]) i++;
    if (i === q.length) return 100 - path.length;
  }
  return -1;
}

const results = computed<string[]>(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return repo.allFiles.slice(0, 50);
  return repo.allFiles
    .map((path): Scored => ({ path, score: score(path, q) }))
    .filter((s) => s.score >= 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, 50)
    .map((s) => s.path);
});

watch(results, () => (active.value = 0));

function pick(path: string) {
  closePalettes();
  repo.selectPath(path);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    active.value = Math.min(active.value + 1, results.value.length - 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    active.value = Math.max(active.value - 1, 0);
  } else if (e.key === "Enter") {
    const p = results.value[active.value];
    if (p) pick(p);
  } else if (e.key === "Escape") {
    closePalettes();
  }
}

function dir(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}
</script>

<template>
  <Teleport to="body">
    <div v-if="palette.quickOpen" class="palette-mask" @click.self="closePalettes()">
      <div class="palette">
        <input
          ref="input"
          v-model="query"
          placeholder="输入文件名（Ctrl+Shift+R）"
          spellcheck="false"
          @keydown="onKey"
        />
        <ul class="palette-list">
          <li
            v-for="(p, i) in results"
            :key="p"
            :class="{ active: i === active }"
            @click="pick(p)"
            @mousemove="active = i"
          >
            <img class="vicon" :src="fileIcon(p)" alt="" />
            <span class="pname">{{ p.split("/").pop() }}</span>
            <span class="pdir">{{ dir(p) }}</span>
          </li>
          <li v-if="!results.length" class="palette-empty">没有匹配的文件</li>
        </ul>
      </div>
    </div>
  </Teleport>
</template>
