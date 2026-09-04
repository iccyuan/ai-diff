<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRepoStore, type Workspace } from "../stores/repo";
import { confirmDialog } from "../lib/confirm";
import { toast } from "../lib/toast";

const props = defineProps<{ workspace: Workspace; index: number }>();
const repo = useRepoStore();

// same "teleport the popover to <body>" trick as AppToolbar's recent-repos
// dropdown — nested inside the glassed sidebar it would only see its own
// empty backdrop and render as flat transparency
const open = ref(false);
const triggerEl = ref<HTMLElement | null>(null);
const menuEl = ref<HTMLElement | null>(null);
const pos = ref({ x: 0, y: 0 });
const filter = ref("");
const newBranchName = ref("");
const creating = ref(false);

const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase();
  const all = props.workspace.branches;
  const list = q ? all.filter((b) => b.name.toLowerCase().includes(q)) : all;
  const local = list.filter((b) => !b.isRemote);
  const remote = list.filter((b) => b.isRemote);
  return { local, remote };
});

async function toggle() {
  if (!open.value) {
    if (repo.active !== props.index) repo.activateWorkspace(props.index);
    await repo.loadBranches(props.workspace);
    const r = triggerEl.value!.getBoundingClientRect();
    pos.value = { x: r.left, y: r.bottom + 6 };
  }
  open.value = !open.value;
  filter.value = "";
  newBranchName.value = "";
}

function close() {
  open.value = false;
}

async function onCheckout(name: string, isRemote: boolean) {
  await repo.checkoutBranch(name, isRemote);
  close();
}

async function onCreate() {
  const name = newBranchName.value.trim();
  if (!name || creating.value) return;
  creating.value = true;
  try {
    await repo.createBranch(name, null, true);
    close();
  } finally {
    creating.value = false;
  }
}

async function onMerge(name: string) {
  const ok = await confirmDialog("合并分支", `确定将「${name}」合并到当前分支「${props.workspace.repo.branch}」吗？`);
  if (!ok) return;
  await repo.mergeBranch(name, false);
  close();
}

async function onDelete(name: string) {
  const ok = await confirmDialog("删除分支", `确定删除分支「${name}」吗？`);
  if (!ok) return;
  try {
    await repo.deleteBranch(name, false);
  } catch {
    const force = await confirmDialog(
      "强制删除分支",
      `分支「${name}」尚未合并，删除后其独有的提交将无法找回（除非知道 commit hash）。确定强制删除吗？`,
    );
    if (force) {
      try {
        await repo.deleteBranch(name, true);
      } catch (e2) {
        toast(String(e2), "error");
      }
    }
  }
}

function onDocPointer(e: MouseEvent) {
  const t = e.target as Node;
  const inTrigger = triggerEl.value?.contains(t);
  const inMenu = menuEl.value?.contains(t);
  if (!inTrigger && !inMenu) open.value = false;
}
function onEsc(e: KeyboardEvent) {
  if (e.key === "Escape") open.value = false;
}
onMounted(() => {
  window.addEventListener("click", onDocPointer);
  window.addEventListener("keydown", onEsc);
});
onBeforeUnmount(() => {
  window.removeEventListener("click", onDocPointer);
  window.removeEventListener("keydown", onEsc);
});
</script>

<template>
  <span ref="triggerEl" class="branch" :class="{ open }" @click.stop="toggle">
    ⎇ {{ workspace.repo.branch ?? "?" }}
  </span>

  <Teleport to="body">
    <div v-if="open" ref="menuEl" class="branch-menu" :style="{ left: pos.x + 'px', top: pos.y + 'px' }" @click.stop>
      <input v-model="filter" class="branch-filter" placeholder="筛选分支…" autofocus />
      <div class="branch-list">
        <div v-if="filtered.local.length" class="branch-group-label">本地分支</div>
        <div
          v-for="b in filtered.local"
          :key="b.name"
          class="branch-row"
          :class="{ current: b.isCurrent }"
          @click="!b.isCurrent && onCheckout(b.name, false)"
        >
          <span class="branch-row-name">{{ b.name }}</span>
          <span v-if="b.upstreamGone" class="branch-row-gone" :title="`上游 ${b.upstream} 已在远程删除`">远程已删除</span>
          <span v-else-if="b.upstream" class="branch-row-track">
            <template v-if="b.ahead">↑{{ b.ahead }}</template>
            <template v-if="b.behind">↓{{ b.behind }}</template>
          </span>
          <button v-if="!b.isCurrent" class="branch-row-del" title="合并到当前分支" @click.stop="onMerge(b.name)">⇄</button>
          <button v-if="!b.isCurrent" class="branch-row-del" title="删除分支" @click.stop="onDelete(b.name)">✕</button>
        </div>
        <div v-if="filtered.remote.length" class="branch-group-label">远程分支</div>
        <div
          v-for="b in filtered.remote"
          :key="b.name"
          class="branch-row"
          @click="onCheckout(b.name, true)"
        >
          <span class="branch-row-name muted">{{ b.name }}</span>
        </div>
        <div v-if="!filtered.local.length && !filtered.remote.length" class="branch-empty">没有匹配的分支</div>
      </div>
      <div class="branch-new">
        <input
          v-model="newBranchName"
          class="branch-new-input"
          placeholder="新建分支…"
          @keydown.enter="onCreate"
        />
        <button class="btn primary" :disabled="!newBranchName.trim() || creating" @click="onCreate">新建</button>
      </div>
    </div>
  </Teleport>
</template>
