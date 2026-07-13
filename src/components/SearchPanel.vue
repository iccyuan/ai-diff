<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { palette } from "../lib/palette";
import { useRepoStore } from "../stores/repo";
import { api, type SearchHit } from "../lib/api";
import { fileIcon } from "../lib/fileIcons";
import { confirmDialog } from "../lib/confirm";
import { toast } from "../lib/toast";
import Spinner from "./Spinner.vue";

const MAX = 500;
const CONTEXT = 6;
const repo = useRepoStore();
const input = ref<HTMLInputElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const hits = ref<SearchHit[]>([]);
const active = ref(0);
const searching = ref(false);
const searched = ref(false);
const previewLines = ref<{ no: number; text: string }[]>([]);

const fileCache = new Map<string, string[]>();
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let runSeq = 0;

watch(
  () => palette.find.open,
  async (open) => {
    if (!open) return;
    fileCache.clear();
    hits.value = [];
    previewLines.value = [];
    searched.value = false;
    await nextTick();
    input.value?.focus();
    input.value?.select();
  },
);

watch(
  () => palette.find.runId,
  () => {
    if (palette.find.open && palette.find.query) run();
  },
);

// IDEA-style live search while typing
watch(
  () => [palette.find.query, palette.find.wholeWord],
  () => {
    if (!palette.find.open) return;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(run, 250);
  },
);

async function run() {
  if (!repo.repo) return;
  const q = palette.find.query;
  if (!q.trim()) {
    hits.value = [];
    previewLines.value = [];
    searched.value = false;
    return;
  }
  const seq = ++runSeq;
  searching.value = true;
  try {
    const result = await api.searchText(repo.repo.root, q, palette.find.wholeWord, MAX);
    if (seq !== runSeq) return; // a newer search superseded this one
    hits.value = result;
    active.value = 0;
    searched.value = true;
    loadPreview();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    if (seq === runSeq) searching.value = false;
  }
}

async function loadPreview() {
  const h = hits.value[active.value];
  if (!h || !repo.repo) {
    previewLines.value = [];
    return;
  }
  let lines = fileCache.get(h.path);
  if (!lines) {
    try {
      const c = await api.readFile(repo.repo.root, h.path);
      lines = (c.content ?? "").split("\n");
    } catch {
      lines = [];
    }
    fileCache.set(h.path, lines);
  }
  const start = Math.max(1, h.line - CONTEXT);
  const end = Math.min(lines.length, h.line + CONTEXT);
  previewLines.value = lines.slice(start - 1, end).map((text, i) => ({ no: start + i, text }));
}

watch(active, () => {
  loadPreview();
  // keep the active row in view
  nextTick(() => listEl.value?.querySelector("li.active")?.scrollIntoView({ block: "nearest" }));
});

function pick(h: SearchHit) {
  palette.find.open = false;
  repo.openAtLine(h.path, h.line);
}

/* ----- IDEA-style Replace in Files ----- */
const replaceOpen = ref(false);
const replaceText = ref("");
const replacing = ref(false);
const fileCount = computed(() => new Set(hits.value.map((h) => h.path)).size);

async function doReplace(scopePath: string | null) {
  if (!repo.repo || replacing.value) return;
  const q = palette.find.query;
  if (!q.trim() || !hits.value.length) return;
  const scopeHits = scopePath ? hits.value.filter((h) => h.path === scopePath).length : hits.value.length;
  const where = scopePath ? `文件「${scopePath}」中约 ${scopeHits} 处` : `${fileCount.value} 个文件中约 ${hits.value.length} 处`;
  const ok = await confirmDialog(
    "替换",
    `确定将「${q}」替换为「${replaceText.value}」吗？将替换${where}匹配。文件的更改会出现在提交面板，可随时还原。`,
  );
  if (!ok) return;
  replacing.value = true;
  try {
    const res = await api.replaceInFiles(
      repo.repo.root,
      q,
      replaceText.value,
      palette.find.wholeWord,
      scopePath ? [scopePath] : null,
    );
    toast(`已在 ${res.files} 个文件中替换 ${res.replacements} 处`);
    fileCache.clear();
    await run(); // refresh the result list against the new content
    await repo.refresh(); // changed files surface in the commit panel
  } catch (e) {
    toast(String(e), "error");
  } finally {
    replacing.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    active.value = Math.min(active.value + 1, hits.value.length - 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    active.value = Math.max(active.value - 1, 0);
  } else if (e.key === "Enter") {
    const h = hits.value[active.value];
    if (h) pick(h);
  } else if (e.key === "Escape") {
    palette.find.open = false;
  }
}

/** split a result line around the match so the template can <mark> it */
function highlight(text: string): { pre: string; match: string; post: string } {
  const t = text.trim();
  const i = t.toLowerCase().indexOf(palette.find.query.toLowerCase());
  if (i < 0) return { pre: t, match: "", post: "" };
  return { pre: t.slice(0, i), match: t.slice(i, i + palette.find.query.length), post: t.slice(i + palette.find.query.length) };
}

const activeHit = () => hits.value[active.value];
</script>

<template>
  <Teleport to="body">
    <div v-if="palette.find.open" class="palette-mask" @mousedown.self="palette.find.open = false">
      <div class="find-dialog">
        <div class="find-bar">
          <input
            ref="input"
            v-model="palette.find.query"
            placeholder="在仓库中查找（↑↓ 选择，回车打开，Esc 关闭）"
            spellcheck="false"
            @keydown="onKey"
          />
          <label class="word-toggle">
            <input v-model="palette.find.wholeWord" type="checkbox" />
            全字匹配
          </label>
          <button class="replace-toggle" :class="{ active: replaceOpen }" @click="replaceOpen = !replaceOpen">替换</button>
          <span class="find-status">
            <Spinner v-if="searching" :size="14" />
            <template v-else-if="searched">{{ hits.length }}{{ hits.length >= MAX ? "+" : "" }} 个结果</template>
          </span>
        </div>
        <div v-if="replaceOpen" class="find-bar replace-bar">
          <input v-model="replaceText" placeholder="替换为" spellcheck="false" @keydown.esc="palette.find.open = false" />
          <button
            class="btn"
            :disabled="replacing || !hits.length || !activeHit()"
            :title="activeHit() ? `只替换 ${activeHit()!.path} 中的匹配` : ''"
            @click="doReplace(activeHit()?.path ?? null)"
          >
            替换当前文件
          </button>
          <button class="btn primary" :disabled="replacing || !hits.length" @click="doReplace(null)">
            {{ replacing ? "替换中…" : "全部替换" }}
          </button>
        </div>
        <ul ref="listEl" class="find-list">
          <li
            v-for="(h, i) in hits"
            :key="i"
            :class="{ active: i === active }"
            @click="active = i"
            @dblclick="pick(h)"
          >
            <img class="vicon" :src="fileIcon(h.path)" alt="" />
            <span class="ppath">{{ h.path }}<span class="pline">:{{ h.line }}</span></span>
            <span class="ptext">
              <template v-if="highlight(h.text).match">
                {{ highlight(h.text).pre }}<mark>{{ highlight(h.text).match }}</mark>{{ highlight(h.text).post }}
              </template>
              <template v-else>{{ h.text.trim() }}</template>
            </span>
          </li>
          <li v-if="searched && !hits.length" class="find-empty">没有找到匹配</li>
        </ul>
        <div v-if="previewLines.length && activeHit()" class="find-preview">
          <div class="fp-title">{{ activeHit()!.path }}</div>
          <pre><code><span
            v-for="l in previewLines"
            :key="l.no"
            class="fp-line"
            :class="{ hit: l.no === activeHit()!.line }"
          ><span class="fp-no">{{ l.no }}</span>{{ l.text }}
</span></code></pre>
        </div>
      </div>
    </div>
  </Teleport>
</template>
