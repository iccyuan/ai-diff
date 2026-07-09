<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { promptState, settlePrompt } from "../lib/prompt";

const inputEl = ref<HTMLInputElement | null>(null);
const textareaEl = ref<HTMLTextAreaElement | null>(null);

watch(
  () => promptState.open,
  async (open) => {
    if (!open) return;
    await nextTick();
    const el = promptState.multiline ? textareaEl.value : inputEl.value;
    el?.focus();
    el?.select();
  },
);
</script>

<template>
  <Teleport to="body">
    <div v-if="promptState.open" class="modal-mask" @click.self="settlePrompt(false)">
      <div class="modal">
        <h3>{{ promptState.title }}</h3>
        <p v-if="promptState.message">{{ promptState.message }}</p>
        <textarea
          v-if="promptState.multiline"
          ref="textareaEl"
          v-model="promptState.value"
          class="prompt-input prompt-textarea"
          rows="8"
          spellcheck="false"
          @keydown.ctrl.enter="settlePrompt(true)"
          @keydown.meta.enter="settlePrompt(true)"
          @keydown.esc="settlePrompt(false)"
        ></textarea>
        <input
          v-else
          ref="inputEl"
          v-model="promptState.value"
          type="text"
          class="prompt-input"
          @keydown.enter="settlePrompt(true)"
          @keydown.esc="settlePrompt(false)"
        />
        <div class="modal-actions">
          <button class="btn" @click="settlePrompt(false)">取消</button>
          <button class="btn primary" @click="settlePrompt(true)">确定</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
