<script setup lang="ts">
import { computed } from "vue";
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";
import type { ChangeKind, FileStatus } from "../lib/api";

const repo = useRepoStore();

const KIND_LABEL: Record<ChangeKind, string> = {
  modified: "修改",
  added: "新增",
  deleted: "删除",
  renamed: "重命名",
  untracked: "未跟踪",
  conflicted: "冲突",
};
const KIND_ORDER: ChangeKind[] = ["conflicted", "modified", "added", "deleted", "renamed", "untracked"];

const totals = computed(() => {
  let add = 0;
  let del = 0;
  for (const f of repo.files) {
    add += f.additions ?? 0;
    del += f.deletions ?? 0;
  }
  return { add, del, files: repo.files.length };
});

const byKind = computed(() => {
  const m = new Map<ChangeKind, number>();
  for (const f of repo.files) m.set(f.kind, (m.get(f.kind) ?? 0) + 1);
  return KIND_ORDER.filter((k) => m.has(k)).map((k) => ({ kind: k, count: m.get(k)! }));
});

function ext(path: string): string {
  const name = path.split("/").pop()!;
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "(无扩展名)";
}

const byLang = computed(() => {
  const m = new Map<string, { files: number; churn: number }>();
  for (const f of repo.files) {
    const e = ext(f.path);
    const cur = m.get(e) ?? { files: 0, churn: 0 };
    cur.files += 1;
    cur.churn += (f.additions ?? 0) + (f.deletions ?? 0);
    m.set(e, cur);
  }
  const max = Math.max(1, ...[...m.values()].map((v) => v.churn));
  return [...m.entries()]
    .map(([lang, v]) => ({ lang, ...v, pct: Math.round((v.churn / max) * 100) }))
    .sort((a, b) => b.churn - a.churn);
});

const topFiles = computed(() =>
  [...repo.files]
    .map((f) => ({ f, churn: (f.additions ?? 0) + (f.deletions ?? 0) }))
    .sort((a, b) => b.churn - a.churn)
    .slice(0, 10),
);

function open(f: FileStatus) {
  repo.selectFile(f);
}
</script>

<template>
  <div class="summary">
    <template v-if="repo.files.length">
      <div class="sum-hero">
        <div class="sum-title">本次改动总览</div>
        <div class="sum-big">
          <span class="num">{{ totals.files }}</span> 个文件
          <span class="stats">
            <span class="add">+{{ totals.add }}</span>
            <span class="del">−{{ totals.del }}</span>
          </span>
        </div>
        <div class="sum-kinds">
          <span v-for="k in byKind" :key="k.kind" class="kind-chip">
            <i class="status-dot" :class="k.kind">{{ KIND_LABEL[k.kind][0] }}</i>
            {{ KIND_LABEL[k.kind] }} {{ k.count }}
          </span>
        </div>
      </div>

      <div class="sum-cols">
        <section class="sum-card">
          <h4>按文件类型</h4>
          <div v-for="l in byLang" :key="l.lang" class="lang-row">
            <span class="lang-name">{{ l.lang }}</span>
            <span class="lang-count">{{ l.files }}</span>
            <span class="lang-bar"><i :style="{ width: l.pct + '%' }"></i></span>
            <span class="lang-churn">{{ l.churn }}</span>
          </div>
        </section>

        <section class="sum-card">
          <h4>改动最多的文件</h4>
          <div v-for="t in topFiles" :key="t.f.path" class="top-row" @click="open(t.f)">
            <span class="ficon"><img :src="fileIcon(t.f.path)" alt="" /></span>
            <span class="top-path" :title="t.f.path">{{ t.f.path }}</span>
            <span class="stats">
              <span v-if="t.f.additions != null" class="add">+{{ t.f.additions }}</span>
              <span v-if="t.f.deletions != null" class="del">−{{ t.f.deletions }}</span>
            </span>
          </div>
        </section>
      </div>
    </template>

    <div v-else class="sum-empty">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M20 6 9 17l-5-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <span>工作区干净，没有未提交的更改</span>
    </div>
  </div>
</template>
