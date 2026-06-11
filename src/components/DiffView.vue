<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { languageForPath, monaco } from "../monaco/setup";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import type { Hunk } from "../lib/api";

const repo = useRepoStore();
const settings = useSettingsStore();
const container = ref<HTMLElement | null>(null);

let editor: monaco.editor.IStandaloneDiffEditor | null = null;
let models: monaco.editor.ITextModel[] = [];
let widgets: monaco.editor.IContentWidget[] = [];
let decorations: monaco.editor.IEditorDecorationsCollection | null = null;

const overlayText = computed(() => {
  if (!repo.repo) return "点击「打开项目」选择一个 git 仓库开始 review";
  if (!repo.selected)
    return repo.files.length ? "从左侧选择一个文件查看更改" : "工作区干净，没有未提交的更改";
  if (repo.loadingDiff && !repo.diff) return "加载 diff…";
  if (repo.diff?.isBinary) return "二进制文件已更改，无法显示文本 diff（仍可在左侧整体还原）";
  if (repo.diff?.tooLarge) return "文件超过 5MB，不显示 diff（仍可在左侧整体还原）";
  return "";
});

function clearView() {
  const mod = editor?.getModifiedEditor();
  if (mod) for (const w of widgets) mod.removeContentWidget(w);
  widgets = [];
  decorations?.clear();
  decorations = null;
  editor?.setModel(null);
  for (const m of models) m.dispose();
  models = [];
}

function render() {
  if (!editor) return;
  clearView();
  const d = repo.diff;
  const f = repo.selected;
  if (!d || !f || d.isBinary || d.tooLarge) return;

  const lang = languageForPath(f.path);
  const original = monaco.editor.createModel(d.original ?? "", lang);
  const modified = monaco.editor.createModel(d.modified ?? "", lang);
  models = [original, modified];
  editor.setModel({ original, modified });

  // hunk-level revert only makes sense for content edits; added/deleted/untracked
  // files are reverted whole from the file list
  if ((f.kind === "modified" || f.kind === "renamed") && d.hunks.length > 0) {
    const mod = editor.getModifiedEditor();
    const decos: monaco.editor.IModelDeltaDecoration[] = [];
    for (const h of d.hunks) {
      const w = makeHunkWidget(h);
      widgets.push(w);
      mod.addContentWidget(w);
      if (h.newLines > 0) {
        decos.push({
          range: new monaco.Range(h.newStart, 1, h.newStart + h.newLines - 1, 1),
          options: { linesDecorationsClassName: "hunk-range-marker" },
        });
      }
    }
    decorations = mod.createDecorationsCollection(decos);
  }
}

function makeHunkWidget(h: Hunk): monaco.editor.IContentWidget {
  const node = document.createElement("div");
  node.className = "hunk-revert";
  const btn = document.createElement("button");
  btn.textContent = "⟲ 还原此块";
  btn.title = "把这个修改块还原为 HEAD 版本";
  btn.onclick = async () => {
    const ok = await confirmDialog(
      "还原修改块",
      `确定还原第 ${h.index + 1} 个修改块（第 ${Math.max(h.newStart, 1)} 行附近）吗？此操作不可撤销。`,
    );
    if (ok) await repo.revertHunk(h);
  };
  node.appendChild(btn);
  return {
    getId: () => `hunk-revert-${h.index}`,
    getDomNode: () => node,
    getPosition: () => ({
      // deletion-only hunks have newLines = 0 and newStart pointing before the cut
      position: { lineNumber: Math.max(h.newStart, 1), column: 1 },
      // ABOVE keeps the button off the first changed line; falls back to EXACT at line 1
      preference: [
        monaco.editor.ContentWidgetPositionPreference.ABOVE,
        monaco.editor.ContentWidgetPositionPreference.EXACT,
      ],
    }),
  };
}

onMounted(() => {
  editor = monaco.editor.createDiffEditor(container.value!, {
    readOnly: true,
    originalEditable: false,
    automaticLayout: true,
    renderSideBySide: settings.renderSideBySide,
    hideUnchangedRegions: { enabled: true },
    diffAlgorithm: "advanced",
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
  });
  render();
});

watch(() => repo.diff, render);
watch(
  () => settings.renderSideBySide,
  (v) => editor?.updateOptions({ renderSideBySide: v }),
);

onBeforeUnmount(() => {
  clearView();
  editor?.dispose();
  editor = null;
});
</script>

<template>
  <section class="diff-pane">
    <div ref="container" class="editor"></div>
    <div v-if="overlayText" class="overlay">{{ overlayText }}</div>
  </section>
</template>
