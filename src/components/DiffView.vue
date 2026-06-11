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
const plainContainer = ref<HTMLElement | null>(null);

let editor: monaco.editor.IStandaloneDiffEditor | null = null;
let plainEditor: monaco.editor.IStandaloneCodeEditor | null = null;
let models: monaco.editor.ITextModel[] = [];
let decorations: monaco.editor.IEditorDecorationsCollection | null = null;
// glyph-margin line -> hunk, for the gutter revert icons
const glyphHunks = new Map<number, Hunk>();

const contentMode = computed(() => !!repo.content && !repo.diff);

const overlayText = computed(() => {
  if (!repo.repo) return "点击「打开项目」选择一个 git 仓库开始 review";
  if (!repo.selectedPath)
    return repo.mode === "all" || repo.files.length
      ? "从左侧选择一个文件查看"
      : "工作区干净，没有未提交的更改";
  if (repo.loadingDiff && !repo.diff && !repo.content) return "加载中…";
  if (repo.diff?.isBinary) return "二进制文件已更改，无法显示文本 diff（仍可在左侧整体还原）";
  if (repo.diff?.tooLarge) return "文件超过 5MB，不显示 diff（仍可在左侧整体还原）";
  if (repo.content?.isBinary) return "二进制文件，无法预览";
  if (repo.content?.tooLarge) return "文件超过 5MB，不显示内容";
  return "";
});

function clearView() {
  glyphHunks.clear();
  decorations?.clear();
  decorations = null;
  editor?.setModel(null);
  plainEditor?.setModel(null);
  for (const m of models) m.dispose();
  models = [];
}

function renderContent() {
  const c = repo.content;
  if (!c || c.content == null || !repo.selectedPath) return;
  if (!plainEditor && plainContainer.value) {
    plainEditor = monaco.editor.create(plainContainer.value, {
      readOnly: true,
      automaticLayout: true,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10, useShadows: false },
      fontFamily: settings.editorFontFamily,
      fontSize: settings.editorFontSize,
    });
  }
  if (!plainEditor) return;
  const model = monaco.editor.createModel(c.content, languageForPath(repo.selectedPath));
  models = [model];
  plainEditor.setModel(model);
}

function render() {
  if (!editor) return;
  clearView();
  if (contentMode.value) {
    renderContent();
    return;
  }
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
      // deletion-only hunks have newLines = 0 and newStart pointing before the cut
      const line = Math.max(h.newStart, 1);
      glyphHunks.set(line, h);
      decos.push({
        range: new monaco.Range(line, 1, line, 1),
        options: {
          glyphMarginClassName: "hunk-revert-glyph",
          glyphMarginHoverMessage: { value: "还原此修改块（恢复为 HEAD 版本）" },
        },
      });
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

async function onRevertGlyphClick(line: number) {
  const h = glyphHunks.get(line);
  if (!h) return;
  const ok = await confirmDialog(
    "还原修改块",
    `确定还原第 ${h.index + 1} 个修改块（第 ${Math.max(h.newStart, 1)} 行附近）吗？此操作不可撤销。`,
  );
  if (ok) await repo.revertHunk(h);
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
    renderOverviewRuler: false,
    scrollbar: {
      verticalScrollbarSize: 10,
      horizontalScrollbarSize: 10,
      useShadows: false,
    },
    fontFamily: settings.editorFontFamily,
    fontSize: settings.editorFontSize,
  });
  // gutter revert icons live in the modified editor's glyph margin
  const mod = editor.getModifiedEditor();
  mod.updateOptions({ glyphMargin: true });
  mod.onMouseDown((e) => {
    if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
    const line = e.target.position?.lineNumber;
    if (line) onRevertGlyphClick(line);
  });
  render();
});

watch(() => [repo.diff, repo.content] as const, render);
watch(
  () => settings.renderSideBySide,
  (v) => editor?.updateOptions({ renderSideBySide: v }),
);
watch(
  () => [settings.editorFontFamily, settings.editorFontSize] as const,
  ([family, size]) => {
    editor?.updateOptions({ fontFamily: family, fontSize: size });
    plainEditor?.updateOptions({ fontFamily: family, fontSize: size });
    // glyph widths must be re-measured once the newly selected webfont is in
    document.fonts?.ready.then(() => monaco.editor.remeasureFonts());
  },
);

onBeforeUnmount(() => {
  clearView();
  editor?.dispose();
  editor = null;
  plainEditor?.dispose();
  plainEditor = null;
});
</script>

<template>
  <section class="diff-pane">
    <div v-show="!contentMode" ref="container" class="editor"></div>
    <div v-show="contentMode" ref="plainContainer" class="editor"></div>
    <div v-if="overlayText" class="overlay">{{ overlayText }}</div>
  </section>
</template>
