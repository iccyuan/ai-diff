<script setup lang="ts">
import { ref, watchEffect } from "vue";
import { authorColor, avatarUrl } from "../lib/avatar";

const props = defineProps<{ author: string; email: string }>();

const url = ref<string | null>(null);
const failed = ref(false);

watchEffect(async () => {
  failed.value = false;
  url.value = await avatarUrl(props.email);
});
</script>

<template>
  <span
    class="avatar"
    :style="{ background: url && !failed ? 'var(--bg-active)' : authorColor(author) }"
    :title="`${author} <${email}>`"
  >
    <img v-if="url && !failed" :src="url" alt="" loading="lazy" @error="failed = true" />
    <template v-else>{{ author.slice(0, 1).toUpperCase() }}</template>
  </span>
</template>
