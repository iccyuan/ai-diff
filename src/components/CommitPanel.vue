<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRepoStore } from "../stores/repo";
import { api } from "../lib/api";

const repo = useRepoStore();

const message = ref("");
const amend = ref(false);
const committing = ref(false);

const stagedCount = computed(() => repo.files.filter((f) => f.staged).length);
const unstagedCount = computed(() => repo.files.filter((f) => !f.staged && f.kind !== "conflicted").length);
const hasConflicts = computed(() => repo.files.some((f) => f.kind === "conflicted"));

const canCommit = computed(
  () => !committing.value && !hasConflicts.value && message.value.trim().length > 0 && (stagedCount.value > 0 || amend.value),
);

// toggling amend on prefills the box with HEAD's subject (only if empty, so
// it never clobbers something the user already typed)
watch(amend, async (on) => {
  if (!on || message.value.trim() || !repo.repo?.hasHead) return;
  try {
    const [head] = await api.logCommits(repo.repo.root, 0, 1);
    if (head) message.value = head.subject;
  } catch {
    // best-effort prefill; leave the box empty on failure
  }
});

async function doCommit() {
  if (!canCommit.value) return;
  committing.value = true;
  try {
    const ok = await repo.createCommit(message.value.trim(), amend.value);
    if (ok) {
      message.value = "";
      amend.value = false;
    }
  } finally {
    committing.value = false;
  }
}

function stageAllUnstaged() {
  repo.stageAll();
}
</script>

<template>
  <div v-if="repo.repo && repo.mode === 'changes'" class="commit-panel">
    <div class="commit-summary">
      <span v-if="stagedCount">已暂存 {{ stagedCount }} 个文件</span>
      <span v-else-if="unstagedCount">{{ unstagedCount }} 个文件未暂存</span>
      <span v-else>没有更改</span>
      <button v-if="unstagedCount && !stagedCount" class="btn-link" @click="stageAllUnstaged">全部暂存</button>
    </div>
    <textarea
      v-model="message"
      class="commit-message"
      rows="3"
      placeholder="提交信息…"
      @keydown.ctrl.enter="doCommit"
      @keydown.meta.enter="doCommit"
    ></textarea>
    <div class="commit-actions">
      <label class="amend-check">
        <input v-model="amend" type="checkbox" />
        修补上一次提交
      </label>
      <button class="btn primary" :disabled="!canCommit" @click="doCommit">
        {{ amend ? "修补提交" : "提交" }}
      </button>
    </div>
  </div>
</template>
