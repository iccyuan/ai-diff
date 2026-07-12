<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRepoStore } from "../stores/repo";
import { confirmDialog } from "../lib/confirm";
import { toast } from "../lib/toast";
import { fileIcon } from "../lib/fileIcons";
import { STATUS_BADGE } from "../lib/fileTree";
import Spinner from "./Spinner.vue";
import { api, type FileStatus, type StashInfo } from "../lib/api";

/* IDEA-style Shelf tab: stash entries on the left, the selected entry's
 * files on the right; clicking a file opens its diff in the main view via
 * the ordinary commit-diff machinery — a stash IS a commit. */
const repo = useRepoStore();

const stashes = ref<StashInfo[]>([]);
const activeHash = ref<string | null>(null);
const filesByHash = ref<Record<string, FileStatus[]>>({});
const loadingFiles = ref(false);

const activeStash = computed(() => stashes.value.find((s) => s.hash === activeHash.value) ?? null);
const activeFiles = computed(() => (activeHash.value ? filesByHash.value[activeHash.value] : undefined));

async function load() {
  const root = repo.repo?.root;
  if (!root) {
    stashes.value = [];
    activeHash.value = null;
    return;
  }
  try {
    stashes.value = await api.listStashes(root);
  } catch {
    stashes.value = [];
  }
  if (activeHash.value && !stashes.value.some((s) => s.hash === activeHash.value)) activeHash.value = null;
}

async function select(s: StashInfo) {
  activeHash.value = s.hash;
  const root = repo.repo?.root;
  if (!root || filesByHash.value[s.hash]) return;
  loadingFiles.value = true;
  try {
    // tracked changes: stash commit vs its first parent; untracked files
    // live in the third-parent commit (parentless → shows as all-added)
    const tracked = await api.commitFiles(root, s.hash);
    const untracked = s.untrackedHash ? await api.commitFiles(root, s.untrackedHash) : [];
    filesByHash.value[s.hash] = [
      ...tracked,
      ...untracked.map((f) => ({ ...f, kind: "untracked" as const })),
    ];
  } catch (e) {
    toast(String(e), "error");
  } finally {
    loadingFiles.value = false;
  }
}

function openFile(s: StashInfo, f: FileStatus) {
  const hash = f.kind === "untracked" && s.untrackedHash ? s.untrackedHash : s.hash;
  repo.selectCommitFile(hash, f);
}

// unshelve triggers a full workspace refresh — without a busy state the
// panel looks frozen between click and completion
const busy = ref(false);

async function unshelve(s: StashInfo) {
  if (busy.value) return;
  busy.value = true;
  try {
    await repo.unshelve(s.index);
    await load();
  } finally {
    busy.value = false;
  }
}

async function drop(s: StashInfo) {
  if (busy.value) return;
  const ok = await confirmDialog("删除搁置", `确定删除搁置「${s.message}」吗？其中的更改将无法找回。`);
  if (!ok) return;
  busy.value = true;
  try {
    await repo.dropShelf(s.index);
    await load();
  } finally {
    busy.value = false;
  }
}

function name(f: FileStatus): string {
  return f.path.split("/").pop()!;
}

onMounted(load);
watch(() => repo.repo?.root, load);
// shelving from the commit panel bumps refreshSeq — keep the list live
watch(
  () => repo.refreshSeq,
  () => load(),
);
</script>

<template>
  <div class="shelf-panel">
    <div class="shelf-entries">
      <div v-if="!stashes.length" class="list-empty">没有搁置的更改（在提交面板右键文件 → 搁置）</div>
      <div
        v-for="s in stashes"
        :key="s.hash"
        class="shelf-entry"
        :class="{ active: s.hash === activeHash }"
        @click="select(s)"
      >
        <span class="shelf-entry-name" :title="s.message">{{ s.message }}</span>
        <span class="shelf-entry-date">{{ s.date }}</span>
        <Spinner v-if="busy" :size="12" />
        <button class="btn-link" :disabled="busy" title="恢复到工作区，并从搁置架移除" @click.stop="unshelve(s)">恢复</button>
        <button class="btn-link danger" :disabled="busy" title="删除该搁置（更改将丢失）" @click.stop="drop(s)">✕</button>
      </div>
    </div>
    <div class="shelf-files">
      <div v-if="!activeStash" class="list-empty">选择一个搁置查看其中的文件</div>
      <template v-else>
        <ul class="commit-files">
          <li v-if="loadingFiles && !activeFiles" class="muted loading-files"><Spinner :size="14" /></li>
          <li
            v-for="f in activeFiles ?? []"
            :key="f.path"
            :title="f.oldPath ? `${f.oldPath} → ${f.path}` : f.path"
            @click="openFile(activeStash, f)"
          >
            <span class="ficon">
              <img :src="fileIcon(f.path)" alt="" />
              <span class="status-dot" :class="f.kind">{{ STATUS_BADGE[f.kind] }}</span>
            </span>
            <span class="path">{{ name(f) }}</span>
            <span class="stats">
              <span v-if="f.additions != null" class="add">+{{ f.additions }}</span>
              <span v-if="f.deletions != null" class="del">−{{ f.deletions }}</span>
            </span>
          </li>
        </ul>
      </template>
    </div>
  </div>
</template>
