<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { palette } from "../lib/palette";
import { useRepoStore } from "../stores/repo";
import { api, type SearchHit } from "../lib/api";
import { fileIcon } from "../lib/fileIcons";
import { toast } from "../lib/toast";
import Spinner from "./Spinner.vue";

const MAX = 500;
const repo = useRepoStore();
const input = ref<HTMLInputElement | null>(null);
const hits = ref<SearchHit[]>([]);
const searching = ref(false);
const searched = ref(false);
const activePath = ref("");
const activeLine = ref(0);

watch(
  () => palette.bottom.open,
  async (open) => {
    if (!open) return;
    await nextTick();
    input.value?.focus();
    input.value?.select();
  },
);

watch(
  () => palette.bottom.runId,
  () => {
    if (palette.bottom.open && palette.bottom.query) run();
  },
);

async function run() {
  if (!repo.repo || !palette.bottom.query.trim()) return;
  searching.value = true;
  try {
    hits.value = await api.searchText(repo.repo.root, palette.bottom.query, palette.bottom.wholeWord, MAX);
    searched.value = true;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    searching.value = false;
  }
}

/** IDEA Find tool window behavior: panel stays open, clicks navigate */
function pick(h: SearchHit) {
  activePath.value = h.path;
  activeLine.value = h.line;
  repo.openAtLine(h.path, h.line);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Enter") run();
  else if (e.key === "Escape") palette.bottom.open = false;
}
</script>

<template>
  <section v-if="palette.bottom.open" class="bottom-search">
    <div class="bs-bar">
      <input
        ref="input"
        v-model="palette.bottom.query"
        placeholder="在仓库中搜索（回车执行，Esc 关闭）"
        spellcheck="false"
        @keydown="onKey"
      />
      <label class="word-toggle">
        <input v-model="palette.bottom.wholeWord" type="checkbox" />
        全字匹配
      </label>
      <span class="bs-status">
        <Spinner v-if="searching" :size="14" />
        <template v-else-if="searched">
          {{ hits.length }} 个结果{{ hits.length >= MAX ? `（前 ${MAX} 条）` : "" }}
        </template>
      </span>
      <button class="bs-close" title="关闭（Esc）" @click="palette.bottom.open = false">✕</button>
    </div>
    <ul class="bs-list">
      <li
        v-for="(h, i) in hits"
        :key="i"
        :class="{ active: h.path === activePath && h.line === activeLine }"
        @click="pick(h)"
      >
        <img class="vicon" :src="fileIcon(h.path)" alt="" />
        <span class="ppath">{{ h.path }}<span class="pline">:{{ h.line }}</span></span>
        <span class="ptext">{{ h.text.trim() }}</span>
      </li>
      <li v-if="searched && !hits.length" class="bs-empty">没有找到匹配</li>
    </ul>
  </section>
</template>
