<script setup lang="ts">
import { ref, watch } from "vue";
import { useSettingsStore } from "../stores/settings";
import { useRepoStore } from "../stores/repo";
import { api, type ConsoleEntry } from "../lib/api";
import { toast } from "../lib/toast";
import HistoryPanel from "./HistoryPanel.vue";

const settings = useSettingsStore();
const repo = useRepoStore();
const subTab = ref<"log" | "console">("log");

function startResize(e: PointerEvent) {
  const startY = e.clientY;
  const startH = settings.gitPanelHeight;
  const el = e.target as HTMLElement;
  el.setPointerCapture(e.pointerId);
  const move = (ev: PointerEvent) => {
    // dragging the top edge up increases height, matching the sidebar's
    // left-edge-drag-to-widen convention (inverse sign since this edge is on top)
    settings.gitPanelHeight = Math.min(800, Math.max(140, startH - (ev.clientY - startY)));
  };
  const up = (ev: PointerEvent) => {
    el.releasePointerCapture(ev.pointerId);
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    settings.saveGitPanelHeight();
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
}

/* ----- 控制台 sub-tab: recent git invocations, no push/WS needed ----- */
const consoleEntries = ref<ConsoleEntry[]>([]);
async function loadConsole() {
  const root = repo.repo?.root;
  if (!root) return;
  try {
    consoleEntries.value = await api.getConsoleLog(root);
  } catch (e) {
    toast(String(e), "error");
  }
}
function fmtTime(ms: number): string {
  return new Date(ms).toLocaleTimeString();
}
/** splits "diff --cached HEAD --name-status" into the subcommand and its
 * arguments, so the console can highlight the part that actually matters */
function splitArgs(args: string): { sub: string; rest: string } {
  const i = args.indexOf(" ");
  return i < 0 ? { sub: args, rest: "" } : { sub: args.slice(0, i), rest: args.slice(i + 1) };
}

watch(subTab, (t) => {
  if (t === "console") loadConsole();
});
// keep an already-open console tab live: refreshWs bumps refreshSeq each time
// it completes, an explicit "the workspace's data just changed" signal
watch(
  () => repo.refreshSeq,
  () => {
    if (subTab.value === "console") loadConsole();
  },
);
watch(
  () => repo.repo?.root,
  () => {
    if (subTab.value === "console") loadConsole();
  },
);
</script>

<template>
  <div v-if="repo.repo" class="git-tool-window" :style="{ height: settings.gitPanelHeight + 'px' }">
    <div class="git-tool-resizer" title="拖动调整高度" @pointerdown="startResize"></div>
    <div class="git-tool-tabs">
      <button :class="{ active: subTab === 'log' }" @click="subTab = 'log'">日志</button>
      <button :class="{ active: subTab === 'console' }" @click="subTab = 'console'">控制台</button>
      <div class="git-tool-tabs-spacer"></div>
      <button class="btn icon close" title="关闭 Git 面板" @click="settings.setGitPanelOpen(false)">✕</button>
    </div>
    <div class="git-tool-body">
      <HistoryPanel v-show="subTab === 'log'" />

      <div v-show="subTab === 'console'" class="console-panel" :style="{ fontFamily: settings.editorFontFamily }">
        <div v-if="!consoleEntries.length" class="list-empty">还没有执行过 git 命令</div>
        <div v-for="(e, i) in consoleEntries" :key="i" class="console-entry" :class="{ error: !e.ok }">
          <span class="console-time">{{ fmtTime(e.atMs) }}</span>
          <span class="console-cmd">git</span>
          <span class="console-sub">{{ splitArgs(e.args).sub }}</span>
          <span class="console-flags">{{ splitArgs(e.args).rest }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
