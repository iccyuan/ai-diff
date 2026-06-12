<script setup lang="ts">
import { useRepoStore } from "../stores/repo";
import { fileIcon } from "../lib/fileIcons";

const repo = useRepoStore();
</script>

<template>
  <div v-if="repo.tabs.length" class="view-tabs">
    <div
      v-for="t in repo.tabs"
      :key="t.id"
      class="vtab"
      :class="{ active: t.id === repo.activeTabId }"
      :title="(t.commit ? `${t.commit.slice(0, 7)} · ` : '') + t.path"
      @click="repo.activateTab(t.id)"
      @mousedown.middle.prevent="repo.closeTab(t.id)"
    >
      <img class="vicon" :src="fileIcon(t.path)" alt="" />
      <span class="vtitle">{{ t.title }}</span>
      <span v-if="t.commit" class="vhash">{{ t.commit.slice(0, 7) }}</span>
      <button class="vclose" title="关闭" @click.stop="repo.closeTab(t.id)">✕</button>
    </div>
  </div>
</template>
