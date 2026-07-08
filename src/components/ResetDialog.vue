<script setup lang="ts">
import { ref, watch } from "vue";
import { resetDialogState, settleResetDialog } from "../lib/resetDialog";
import { api } from "../lib/api";
import type { ResetMode } from "../lib/api";

const aheadCount = ref<number | null>(null);

const OPTIONS: { mode: ResetMode; label: string; desc: string }[] = [
  {
    mode: "soft",
    label: "Soft",
    desc: "分支指针移动到这里，索引和工作区都保持不变——之前的提交内容会变为「已暂存」，可以重新整理后再提交。",
  },
  {
    mode: "mixed",
    label: "Mixed（默认）",
    desc: "分支指针移动到这里，工作区文件保持不变，但索引会重置——之前的提交内容会变为「未暂存」的改动。",
  },
  {
    mode: "hard",
    label: "Hard",
    desc: "分支指针移动到这里，索引和工作区都会被重置。工作区、暂存区中所有未提交的改动都会被永久丢弃，此操作不可撤销。",
  },
];

watch(
  () => resetDialogState.open,
  async (open) => {
    if (!open) return;
    aheadCount.value = null;
    try {
      aheadCount.value = await api.countCommitsBetween(resetDialogState.root, resetDialogState.hash, "HEAD");
    } catch {
      aheadCount.value = null;
    }
  },
);
</script>

<template>
  <Teleport to="body">
    <div v-if="resetDialogState.open" class="modal-mask" @click.self="settleResetDialog(false)">
      <div class="modal reset-modal">
        <h3>重置当前分支到这里</h3>
        <p class="reset-target">
          将重置到「{{ resetDialogState.subject }}」
          <span class="hash">{{ resetDialogState.hash.slice(0, 7) }}</span>
        </p>
        <div class="reset-options">
          <label
            v-for="o in OPTIONS"
            :key="o.mode"
            class="reset-option"
            :class="{ active: resetDialogState.mode === o.mode, danger: o.mode === 'hard' }"
          >
            <input v-model="resetDialogState.mode" type="radio" name="reset-mode" :value="o.mode" />
            <span class="reset-option-body">
              <span class="reset-option-label">{{ o.label }}</span>
              <span class="reset-option-desc">
                {{ o.desc }}
                <template v-if="o.mode === 'hard' && aheadCount">
                  当前分支上此提交之后的 {{ aheadCount }} 个提交也会一并丢失（除非记得 commit hash）。
                </template>
              </span>
            </span>
          </label>
        </div>
        <div class="modal-actions">
          <button class="btn" @click="settleResetDialog(false)">取消</button>
          <button class="btn primary" :class="{ danger: resetDialogState.mode === 'hard' }" @click="settleResetDialog(true)">重置</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
