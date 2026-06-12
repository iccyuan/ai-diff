<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 16 });
</script>

<template>
  <span class="spinner" :style="{ width: size + 'px', height: size + 'px' }" aria-label="加载中"></span>
</template>
