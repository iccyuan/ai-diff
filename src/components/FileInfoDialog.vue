<script setup lang="ts">
import { computed } from "vue";
import { fileInfoState, closeFileInfo } from "../lib/fileInfo";
import { fileIcon } from "../lib/fileIcons";

const info = computed(() => fileInfoState.info);

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 2 : 1)} ${units[i]} (${bytes.toLocaleString()} 字节)`;
}

function formatTime(ms: number | null): string {
  if (ms == null) return "—";
  return new Date(ms).toLocaleString();
}

const kindLabel = computed(() => {
  const f = info.value;
  if (!f) return "";
  if (f.isSymlink) return "符号链接";
  if (f.isDir) return "文件夹";
  return "文件";
});
</script>

<template>
  <Teleport to="body">
    <div v-if="fileInfoState.open && info" class="modal-mask" @click.self="closeFileInfo">
      <div class="modal file-info-modal">
        <h3 class="fi-title">
          <img class="fi-icon" :src="fileIcon(info.path)" alt="" />
          <span class="fi-name" :title="info.name">{{ info.name }}</span>
        </h3>
        <dl class="fi-rows">
          <dt>类型</dt>
          <dd>{{ kindLabel }}{{ info.readonly ? " · 只读" : "" }}</dd>
          <dt>相对路径</dt>
          <dd class="fi-path" :title="info.path">{{ info.path }}</dd>
          <dt>完整路径</dt>
          <dd class="fi-path" :title="info.fullPath">{{ info.fullPath }}</dd>
          <template v-if="info.exists">
            <dt v-if="!info.isDir">大小</dt>
            <dd v-if="!info.isDir">{{ formatSize(info.size) }}</dd>
            <template v-if="info.lines != null">
              <dt>行数</dt>
              <dd>{{ info.lines.toLocaleString() }}</dd>
            </template>
            <dt>修改时间</dt>
            <dd>{{ formatTime(info.modified) }}</dd>
            <dt>创建时间</dt>
            <dd>{{ formatTime(info.created) }}</dd>
          </template>
          <template v-else>
            <dt>状态</dt>
            <dd class="fi-missing">文件不存在（可能已删除）</dd>
          </template>
        </dl>
        <div class="modal-actions">
          <button class="btn" @click="closeFileInfo">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.file-info-modal {
  width: 480px;
}
.fi-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.fi-icon {
  width: 18px;
  height: 18px;
  flex: none;
}
.fi-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fi-rows {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 8px 16px;
  margin: 0 0 18px;
  font-size: 13px;
}
.fi-rows dt {
  color: var(--fg-muted);
  white-space: nowrap;
}
.fi-rows dd {
  margin: 0;
  min-width: 0;
  word-break: break-all;
  user-select: text;
}
.fi-path {
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
.fi-missing {
  color: var(--del, #e06c75);
}
</style>
