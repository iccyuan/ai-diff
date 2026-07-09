<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRepoStore } from "../stores/repo";
import { api, type RepoStats } from "../lib/api";

const repo = useRepoStore();
const stats = ref<RepoStats | null>(null);
const loading = ref(false);
const loadedForRoot = ref<string | null>(null);

async function load() {
  const root = repo.repo?.root;
  if (!root || loadedForRoot.value === root) return;
  loading.value = true;
  try {
    stats.value = await api.repoStats(root);
    loadedForRoot.value = root;
  } catch {
    stats.value = null;
  } finally {
    loading.value = false;
  }
}

watch(() => repo.repo?.root, load, { immediate: true });

const segments = computed(() => {
  const s = stats.value;
  if (!s || !s.totalLines) return [];
  return s.languages.map((l) => ({ ...l, pct: (l.lines / s.totalLines) * 100 }));
});
</script>

<template>
  <section class="sum-card">
    <h4>项目语言构成</h4>
    <div v-if="loading" class="repo-summary-loading">统计中…</div>
    <template v-else-if="stats && stats.languages.length">
      <div class="repo-summary-totals">
        <span><b>{{ stats.totalLines.toLocaleString() }}</b> 行代码</span>
        <span><b>{{ stats.totalFiles.toLocaleString() }}</b> 个文件</span>
      </div>
      <div class="repo-summary-bar">
        <span
          v-for="l in segments"
          :key="l.language"
          class="repo-summary-bar-seg"
          :style="{ width: l.pct + '%', background: l.color }"
          :title="`${l.language} ${l.pct.toFixed(1)}%`"
        ></span>
      </div>
      <div class="repo-summary-legend">
        <div v-for="l in segments" :key="l.language" class="repo-summary-legend-item">
          <span class="repo-summary-dot" :style="{ background: l.color }"></span>
          <span class="repo-summary-lang-name">{{ l.language }}</span>
          <span class="repo-summary-pct">{{ l.pct.toFixed(1) }}%</span>
          <span class="repo-summary-lines">{{ l.lines.toLocaleString() }} 行</span>
        </div>
      </div>
    </template>
    <div v-else class="repo-summary-loading">未识别到已跟踪的源代码文件</div>
  </section>
</template>
