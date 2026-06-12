<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { palette, closePalettes } from "../lib/palette";
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

watch(
  () => palette.search,
  async (open) => {
    if (!open) return;
    hits.value = [];
    searched.value = false;
    await nextTick();
    input.value?.focus();
    input.value?.select();
  },
);

watch(
  () => palette.runId,
  () => {
    if (palette.search && palette.query) run();
  },
);

async function run() {
  if (!repo.repo || !palette.query.trim()) return;
  searching.value = true;
  try {
    hits.value = await api.searchText(repo.repo.root, palette.query, palette.wholeWord, MAX);
    searched.value = true;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    searching.value = false;
  }
}

function pick(h: SearchHit) {
  closePalettes();
  repo.openAtLine(h.path, h.line);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Enter") run();
  else if (e.key === "Escape") closePalettes();
}
</script>

<template>
  <Teleport to="body">
    <div v-if="palette.search" class="palette-mask" @click.self="closePalettes()">
      <div class="palette search-palette">
        <div class="search-bar">
          <input
            ref="input"
            v-model="palette.query"
            placeholder="搜索字符串 / 符号（Ctrl+H，回车搜索）"
            spellcheck="false"
            @keydown="onKey"
          />
          <label class="word-toggle">
            <input v-model="palette.wholeWord" type="checkbox" />
            全字匹配
          </label>
        </div>
        <div class="palette-status">
          <Spinner v-if="searching" :size="14" />
          <template v-else-if="searched">
            {{ hits.length }} 个结果{{ hits.length >= MAX ? `（仅显示前 ${MAX} 条）` : "" }}
          </template>
        </div>
        <ul class="palette-list search-list">
          <li v-for="(h, i) in hits" :key="i" @click="pick(h)">
            <img class="vicon" :src="fileIcon(h.path)" alt="" />
            <span class="ppath">{{ h.path }}<span class="pline">:{{ h.line }}</span></span>
            <span class="ptext">{{ h.text.trim() }}</span>
          </li>
          <li v-if="searched && !hits.length" class="palette-empty">没有找到匹配</li>
        </ul>
      </div>
    </div>
  </Teleport>
</template>
