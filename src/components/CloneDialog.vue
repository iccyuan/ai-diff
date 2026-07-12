<script setup lang="ts">
import { ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { cloneDialogState, closeCloneDialog } from "../lib/cloneDialog";
import { useRepoStore } from "../stores/repo";
import { api } from "../lib/api";
import { toast } from "../lib/toast";

const repo = useRepoStore();

const url = ref("");
const parentDir = ref("");
const folderName = ref("");
const branch = ref("");
const depth = ref("");
const busy = ref(false);

watch(
  () => cloneDialogState.open,
  (isOpen) => {
    if (!isOpen) return;
    url.value = "";
    parentDir.value = "";
    folderName.value = "";
    branch.value = "";
    depth.value = "";
    busy.value = false;
  },
);

function guessFolderName(u: string): string {
  const trimmed = u.trim().replace(/[/\\]+$/, "");
  const last = trimmed.split(/[/\\]/).pop() ?? "";
  return last.replace(/\.git$/i, "");
}

function fillFolderNameIfEmpty() {
  if (!folderName.value.trim()) folderName.value = guessFolderName(url.value);
}

function joinPath(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.replace(/[/\\]+$/, "") + sep + name;
}

async function pickParentDir() {
  const dir = await open({ directory: true, title: "选择克隆到的父目录" });
  if (typeof dir === "string") parentDir.value = dir;
}

async function submit() {
  if (!url.value.trim() || !parentDir.value || !folderName.value.trim()) {
    toast("请填写仓库地址并选择目标目录", "error");
    return;
  }
  let depthNum: number | null = null;
  if (depth.value.trim()) {
    depthNum = Number(depth.value.trim());
    if (!Number.isInteger(depthNum) || depthNum <= 0) {
      toast("克隆深度必须是正整数", "error");
      return;
    }
  }
  const dest = joinPath(parentDir.value, folderName.value.trim());
  busy.value = true;
  try {
    await api.cloneRepo(url.value.trim(), dest, branch.value.trim() || null, depthNum);
    toast("克隆完成");
    closeCloneDialog();
    await repo.openRepo(dest);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="cloneDialogState.open" class="modal-mask" @click.self="!busy && closeCloneDialog()">
      <div class="modal clone-modal">
        <h3>克隆仓库</h3>

        <label class="field">
          <span>仓库地址（URL）</span>
          <input v-model="url" type="text" placeholder="https://github.com/user/repo.git" @blur="fillFolderNameIfEmpty" />
        </label>

        <label class="field row">
          <span class="clone-label">目标目录</span>
          <input v-model="parentDir" type="text" placeholder="选择一个父目录…" readonly />
          <button class="btn" :disabled="busy" @click="pickParentDir">浏览</button>
        </label>

        <label class="field">
          <span>文件夹名称</span>
          <input v-model="folderName" type="text" placeholder="repo" />
        </label>

        <label class="field">
          <span>只同步分支（可选，留空则克隆所有分支）</span>
          <input v-model="branch" type="text" placeholder="例如 main" />
        </label>

        <label class="field">
          <span>克隆深度（可选，留空则完整历史）</span>
          <input v-model="depth" type="number" min="1" placeholder="例如 1（浅克隆）" />
        </label>

        <div class="modal-actions">
          <button class="btn" :disabled="busy" @click="closeCloneDialog">取消</button>
          <button class="btn primary" :disabled="busy" @click="submit">{{ busy ? "克隆中…" : "克隆" }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
