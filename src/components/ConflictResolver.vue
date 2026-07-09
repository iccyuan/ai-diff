<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRepoStore } from "../stores/repo";
import { api, type ConflictSides } from "../lib/api";
import { confirmDialog } from "../lib/confirm";
import { toast } from "../lib/toast";
import { parseConflictMarkers, applyResolution } from "../lib/conflictParser";
import { fileIcon } from "../lib/fileIcons";

const repo = useRepoStore();

const conflicted = computed(() => repo.files.filter((f) => f.kind === "conflicted"));
const selectedPath = ref<string | null>(null);

watch(
  conflicted,
  (list) => {
    if (!selectedPath.value || !list.some((f) => f.path === selectedPath.value)) {
      selectedPath.value = list[0]?.path ?? null;
    }
  },
  { immediate: true },
);

const sides = ref<ConflictSides | null>(null);
const buffer = ref("");
const loading = ref(false);

// a text (both-modified-style) conflict has real content on both sides;
// add/add, delete/modify, both-deleted and binary all route to the simple
// "keep mine / keep theirs" UI instead
const isTextConflict = computed(
  () => !!sides.value && !sides.value.isBinary && !sides.value.tooLarge && sides.value.ours != null && sides.value.theirs != null,
);

async function loadSelected() {
  const path = selectedPath.value;
  const root = repo.repo?.root;
  if (!path || !root) {
    sides.value = null;
    return;
  }
  loading.value = true;
  try {
    sides.value = await api.getConflictSides(root, path);
    if (sides.value && !sides.value.isBinary && !sides.value.tooLarge && sides.value.ours != null && sides.value.theirs != null) {
      const content = await api.readFile(root, path);
      buffer.value = content.content ?? "";
    } else {
      buffer.value = "";
    }
  } catch (e) {
    toast(String(e), "error");
  } finally {
    loading.value = false;
  }
}
watch(selectedPath, loadSelected, { immediate: true });

const regions = computed(() => parseConflictMarkers(buffer.value));

function takeRegion(idx: number, side: "ours" | "theirs" | "both") {
  buffer.value = applyResolution(buffer.value, idx, side);
}

function selectFile(path: string) {
  selectedPath.value = path;
}

async function markResolved() {
  const path = selectedPath.value;
  const root = repo.repo?.root;
  if (!path || !root) return;
  try {
    await api.resolveConflict(root, path, buffer.value);
    toast(`已标记 ${path} 为已解决`);
    if (repo.ws) await repo.refreshWs(repo.ws);
  } catch (e) {
    toast(String(e), "error");
  }
}

async function keepMine() {
  const path = selectedPath.value;
  const root = repo.repo?.root;
  if (!path || !root || !sides.value) return;
  try {
    if (sides.value.isBinary) await api.resolveConflictBinary(root, path, "ours");
    else if (sides.value.ours == null) await api.resolveConflictDelete(root, path);
    else await api.resolveConflict(root, path, sides.value.ours);
    if (repo.ws) await repo.refreshWs(repo.ws);
  } catch (e) {
    toast(String(e), "error");
  }
}

async function keepTheirs() {
  const path = selectedPath.value;
  const root = repo.repo?.root;
  if (!path || !root || !sides.value) return;
  try {
    if (sides.value.isBinary) await api.resolveConflictBinary(root, path, "theirs");
    else if (sides.value.theirs == null) await api.resolveConflictDelete(root, path);
    else await api.resolveConflict(root, path, sides.value.theirs);
    if (repo.ws) await repo.refreshWs(repo.ws);
  } catch (e) {
    toast(String(e), "error");
  }
}

const opLabel = computed(() => {
  switch (repo.repo?.operation) {
    case "merge":
      return "合并";
    case "cherryPick":
      return "Cherry-pick";
    case "revert":
      return "回滚提交";
    case "rebase":
      return "Rebase";
    default:
      return "";
  }
});

// which side "我的"/"对方" actually refer to — varies by operation, unlike
// the diff view's always-the-same 修改前/修改后
const mineLabel = computed(() => `我的（当前分支${repo.repo?.branch ? ` ${repo.repo.branch}` : ""}）`);
const theirsLabel = computed(() => {
  switch (repo.repo?.operation) {
    case "merge":
      return "对方（被合并进来的分支）";
    case "cherryPick":
      return "对方（被拣选的提交）";
    case "revert":
      return "对方（回滚产生的改动）";
    case "rebase":
      return "对方（变基重放的提交）";
    default:
      return "对方";
  }
});

async function onContinue() {
  if (conflicted.value.length) {
    toast("还有未解决的冲突", "error");
    return;
  }
  await repo.continueOperation();
}

async function onAbort() {
  const ok = await confirmDialog(`中止${opLabel.value}`, `确定要中止当前的${opLabel.value}吗？已经做的冲突解决工作会丢失，且不可撤销。`);
  if (ok) await repo.abortOperation();
}
</script>

<template>
  <div class="conflict-resolver">
    <div class="conflict-files">
      <div class="conflict-files-head">{{ opLabel }}冲突（{{ conflicted.length }}）</div>
      <div
        v-for="f in conflicted"
        :key="f.path"
        class="conflict-file-row"
        :class="{ active: f.path === selectedPath }"
        @click="selectFile(f.path)"
      >
        <span class="ficon"><img :src="fileIcon(f.path)" alt="" /></span>
        <span class="path" :title="f.path">{{ f.path }}</span>
      </div>
    </div>
    <div class="conflict-main">
      <template v-if="selectedPath && sides">
        <div class="conflict-legend">
          <span class="conflict-legend-mine">{{ mineLabel }}</span>
          <span class="conflict-legend-theirs">{{ theirsLabel }}</span>
        </div>
        <div v-if="sides.tooLarge" class="conflict-empty">文件过大，无法在此预览/解决，请在外部工具中处理后刷新。</div>
        <div v-else-if="!isTextConflict" class="conflict-binary">
          <p v-if="sides.isBinary">这是一个二进制文件冲突，无法在编辑器中合并。</p>
          <p v-else-if="sides.ours == null">对方修改了此文件，你的分支删除了它。</p>
          <p v-else-if="sides.theirs == null">你修改了此文件，对方删除了它。</p>
          <p v-else>双方都新增了同名文件（内容不同）。</p>
          <div class="conflict-binary-actions">
            <button class="btn" @click="keepMine">保留我的版本{{ sides.ours == null ? "（即删除）" : "" }}</button>
            <button class="btn" @click="keepTheirs">保留对方版本{{ sides.theirs == null ? "（即删除）" : "" }}</button>
          </div>
        </div>
        <template v-else>
          <div v-if="regions.length" class="conflict-regions">
            <div v-for="(_, i) in regions" :key="i" class="conflict-region-bar">
              <span>冲突块 {{ i + 1 }}</span>
              <button @click="takeRegion(i, 'ours')">采用我的</button>
              <button @click="takeRegion(i, 'theirs')">采用对方</button>
              <button @click="takeRegion(i, 'both')">两者都要</button>
            </div>
          </div>
          <textarea class="conflict-editor" v-model="buffer" spellcheck="false"></textarea>
        </template>
      </template>
      <div v-else class="conflict-empty">选择左侧的文件开始解决冲突</div>
    </div>
    <div class="conflict-footer">
      <button
        v-if="selectedPath && isTextConflict"
        class="btn"
        :disabled="!!regions.length"
        :title="regions.length ? '还有未处理的冲突块' : ''"
        @click="markResolved"
      >
        标记为已解决
      </button>
      <div class="conflict-footer-spacer"></div>
      <button class="btn danger" @click="onAbort">中止{{ opLabel }}</button>
      <button class="btn primary" :disabled="!!conflicted.length" @click="onContinue">继续{{ opLabel }}</button>
    </div>
  </div>
</template>
