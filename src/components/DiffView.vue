<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { openUrl } from "@tauri-apps/plugin-opener";
import { languageForPath, monaco } from "../monaco/setup";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { openSearch } from "../lib/palette";
import Spinner from "./Spinner.vue";
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

// markdown-family files get a rendered preview with a source toggle
const MD_EXTS = [".md", ".markdown", ".mdx"];
const mdPreviewOn = ref(true);
const isMarkdown = computed(() => {
  const p = repo.selectedPath?.toLowerCase() ?? "";
  return contentMode.value && MD_EXTS.some((e) => p.endsWith(e));
});
const showMdPreview = computed(() => isMarkdown.value && mdPreviewOn.value);
const renderedMd = computed(() => {
  if (!showMdPreview.value || repo.content?.content == null) return "";
  return DOMPurify.sanitize(marked.parse(repo.content.content, { async: false }));
});

// active ctrl-hover link clearers; a single global keyup empties them all
const ctrlLinkClearers = new Set<() => void>();

function onGlobalKeyUp(e: KeyboardEvent) {
  if (e.key === "Control") for (const clear of ctrlLinkClearers) clear();
}

/** Eclipse/IDEA navigation: F3 / Ctrl+B jump to symbol, Ctrl+Shift+G / Alt+F7
 *  find references (repo-wide whole-word search), Ctrl+L go-to-line.
 *  Holding Ctrl turns the hovered symbol into an IDEA-style underlined link. */
function addEclipseActions(ed: monaco.editor.IStandaloneCodeEditor) {
  const symbolSearch = () => {
    const pos = ed.getPosition();
    const word = pos && ed.getModel()?.getWordAtPosition(pos)?.word;
    if (word) openSearch(word, true);
  };
  ed.addAction({
    id: "open-declaration",
    label: "跳转到符号（全仓库）",
    keybindings: [monaco.KeyCode.F3, monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyB],
    contextMenuGroupId: "navigation",
    contextMenuOrder: 1,
    run: symbolSearch,
  });
  ed.addAction({
    id: "find-references",
    label: "查找引用（全仓库）",
    keybindings: [
      monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyG,
      monaco.KeyMod.Alt | monaco.KeyCode.F7,
    ],
    contextMenuGroupId: "navigation",
    contextMenuOrder: 2,
    run: symbolSearch,
  });
  ed.addAction({
    id: "goto-line-alias",
    label: "转到行",
    keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyL],
    run: () => {
      ed.focus();
      ed.getAction("editor.action.gotoLine")?.run();
    },
  });

  // IDEA-style ctrl+hover: underline the symbol under the cursor as a link
  const linkDeco = ed.createDecorationsCollection([]);
  let linkKey = "";
  const clearLink = () => {
    if (!linkKey) return;
    linkKey = "";
    linkDeco.set([]);
  };
  ctrlLinkClearers.add(clearLink);
  ed.onMouseMove((e) => {
    if (!e.event.ctrlKey || e.target.type !== monaco.editor.MouseTargetType.CONTENT_TEXT) {
      clearLink();
      return;
    }
    const pos = e.target.position;
    const word = pos && ed.getModel()?.getWordAtPosition(pos);
    if (!pos || !word) {
      clearLink();
      return;
    }
    const key = `${pos.lineNumber}:${word.startColumn}:${word.word}`;
    if (key === linkKey) return;
    linkKey = key;
    linkDeco.set([
      {
        range: new monaco.Range(pos.lineNumber, word.startColumn, pos.lineNumber, word.endColumn),
        options: { inlineClassName: "symbol-link" },
      },
    ]);
  });
  ed.onMouseLeave(clearLink);
  ed.onMouseDown((e) => {
    if (!e.event.ctrlKey || e.target.type !== monaco.editor.MouseTargetType.CONTENT_TEXT) return;
    const pos = e.target.position;
    const word = pos && ed.getModel()?.getWordAtPosition(pos)?.word;
    clearLink();
    if (word) openSearch(word, true);
  });
}

function revealPendingLine(ed: monaco.editor.ICodeEditor | null) {
  const line = repo.pendingRevealLine;
  if (!line || !ed) return;
  repo.pendingRevealLine = null;
  ed.revealLineInCenter(line);
  ed.setPosition({ lineNumber: line, column: 1 });
  ed.focus();
}

function onPreviewClick(e: MouseEvent) {
  // external links leave the webview and open in the system browser
  const a = (e.target as HTMLElement).closest("a");
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute("href") ?? "";
  if (/^https?:\/\//i.test(href)) openUrl(href);
}

const overlayText = computed(() => {
  if (!repo.repo) return "点击「打开项目」选择一个 git 仓库开始 review";
  if (!repo.selectedPath && !repo.selectedCommitPath)
    return repo.mode === "all" || repo.files.length
      ? "从左侧选择一个文件查看"
      : "工作区干净，没有未提交的更改";
  if (repo.loadingDiff && !repo.diff && !repo.content) return "加载中…";
  if (repo.diff?.isBinary) return "二进制文件已更改，无法显示文本 diff（仍可在左侧整体还原）";
  if (repo.diff?.tooLarge) return "文件超过 5MB，不显示 diff（仍可在左侧整体还原）";
  if (repo.content?.isBinary) return "二进制文件，无法预览";
  if (repo.content?.tooLarge) return "文件超过 5MB，不显示内容";
  if (!repo.loadingDiff && !repo.diff && !repo.content) return "文件不存在或无法读取";
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
    addEclipseActions(plainEditor);
  }
  if (!plainEditor) return;
  const model = monaco.editor.createModel(c.content, languageForPath(repo.selectedPath));
  models = [model];
  plainEditor.setModel(model);
  if (repo.pendingRevealLine) mdPreviewOn.value = false;
  revealPendingLine(plainEditor);
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
  const diffPath = repo.selectedCommitPath ?? f?.path;
  if (!d || !diffPath || d.isBinary || d.tooLarge) return;

  const lang = languageForPath(diffPath);
  const original = monaco.editor.createModel(d.original ?? "", lang);
  const modified = monaco.editor.createModel(d.modified ?? "", lang);
  models = [original, modified];
  editor.setModel({ original, modified });
  revealPendingLine(editor.getModifiedEditor());

  // hunk-level revert only applies to working-tree content edits (f is null
  // for history diffs); added/deleted/untracked revert whole from the list
  if (f && (f.kind === "modified" || f.kind === "renamed") && d.hunks.length > 0) {
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
  window.addEventListener("keyup", onGlobalKeyUp);
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
  addEclipseActions(mod as monaco.editor.IStandaloneCodeEditor);
  addEclipseActions(editor.getOriginalEditor() as monaco.editor.IStandaloneCodeEditor);
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
  window.removeEventListener("keyup", onGlobalKeyUp);
  ctrlLinkClearers.clear();
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
    <div v-show="contentMode && !showMdPreview" ref="plainContainer" class="editor"></div>
    <div v-if="showMdPreview" class="md-preview" @click="onPreviewClick" v-html="renderedMd"></div>
    <div v-if="isMarkdown" class="md-toggle">
      <button :class="{ active: mdPreviewOn }" @click="mdPreviewOn = true">预览</button>
      <button :class="{ active: !mdPreviewOn }" @click="mdPreviewOn = false">源码</button>
    </div>
    <div v-if="overlayText" class="overlay">
      <Spinner v-if="repo.loadingDiff" :size="28" />
      <svg v-else class="ghost" viewBox="0 0 24 24" aria-hidden="true">
        <rect x="2.5" y="3" width="8" height="18" rx="1.5" />
        <rect x="13.5" y="3" width="8" height="18" rx="1.5" opacity="0.45" />
        <rect x="4.5" y="6" width="4" height="1.6" rx="0.8" fill="var(--bg)" />
        <rect x="4.5" y="9.2" width="4" height="1.6" rx="0.8" fill="var(--bg)" />
        <rect x="15.5" y="6" width="4" height="1.6" rx="0.8" fill="var(--bg)" />
      </svg>
      <span>{{ overlayText }}</span>
    </div>
  </section>
</template>
