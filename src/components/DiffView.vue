<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { openUrl } from "@tauri-apps/plugin-opener";
import { languageForPath, monaco } from "../monaco/setup";
import { useRepoStore } from "../stores/repo";
import { useSettingsStore } from "../stores/settings";
import { confirmDialog } from "../lib/confirm";
import { openFindDialog, openChooser } from "../lib/palette";
import { api, type Hunk, type ImageDiff } from "../lib/api";
import { isVectorSource, vectorDataUrl } from "../lib/vector";
import { toast } from "../lib/toast";
import Spinner from "./Spinner.vue";

const repo = useRepoStore();
const settings = useSettingsStore();
const container = ref<HTMLElement | null>(null);
const plainContainer = ref<HTMLElement | null>(null);

// ----- image before/after preview -----
// raster formats arrive as binary and are decoded by the backend. vector
// formats (svg + Android vector drawables) are text, converted in the browser,
// and get a preview/source toggle like markdown. .9.png nine-patches already
// match ".png" and preview as plain PNG.
const IMAGE_EXTS = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico", ".avif"];
const imageData = ref<ImageDiff | null>(null); // raster, fetched from backend
const vecPreviewOn = ref(true);
const isRasterImage = computed(() => {
  const p = repo.selectedPath?.toLowerCase() ?? ""; // working-tree files only; history image diffs fall back to notice
  if (!IMAGE_EXTS.some((e) => p.endsWith(e))) return false;
  return (!!repo.diff && repo.diff.isBinary) || (!!repo.content && repo.content.isBinary);
});
// vectors preview like markdown: only when viewing a single file's content
// (全部文件 / content mode), with a 预览/源码 toggle. The 更改 (diff) view just
// shows the textual XML diff — no preview, no toggle.
const isVectorImage = computed(() => contentMode.value && isVectorSource(repo.selectedPath ?? "", repo.content?.content));
const vectorCurrent = computed(() => vectorDataUrl(repo.selectedPath ?? "", repo.content?.content ?? null));
const isImageView = computed(() => isRasterImage.value || (isVectorImage.value && vecPreviewOn.value));
const previewData = computed<ImageDiff | null>(() => (isRasterImage.value ? imageData.value : null));

// ----- next/prev change navigation (IDEA F7 / Shift+F7) -----
const navList = ref<number[]>([]);
const navIdx = ref(-1);
// Prefer git's own hunk line numbers (always available for working-tree diffs,
// no dependency on Monaco's async diff timing); fall back to Monaco's computed
// changes for history diffs which carry no hunks.
function computeNav(): boolean {
  const hunks = repo.diff?.hunks ?? [];
  if (hunks.length) {
    navList.value = hunks.map((h) => Math.max(h.newStart, 1));
    navIdx.value = -1;
    return true;
  }
  const changes = editor?.getLineChanges() ?? [];
  if (changes.length) {
    navList.value = changes.map((c) => c.modifiedStartLineNumber || c.modifiedEndLineNumber || 1);
    navIdx.value = -1;
    return true;
  }
  navList.value = [];
  return false;
}
function refreshNav() {
  computeNav();
}
function scheduleNavRefresh() {
  navList.value = [];
  if (computeNav()) return;
  let tries = 0;
  const tick = () => {
    if (computeNav() || ++tries >= 20) return;
    setTimeout(tick, 60);
  };
  tick();
}
function goToDiff(dir: number) {
  if (!editor || !navList.value.length) return;
  navIdx.value = (navIdx.value + dir + navList.value.length) % navList.value.length;
  const line = navList.value[navIdx.value];
  const mod = editor.getModifiedEditor();
  mod.revealLineInCenter(line);
  mod.setPosition({ lineNumber: line, column: 1 });
  mod.focus();
}

let editor: monaco.editor.IStandaloneDiffEditor | null = null;
let plainEditor: monaco.editor.IStandaloneCodeEditor | null = null;
let models: monaco.editor.ITextModel[] = [];
let decorations: monaco.editor.IEditorDecorationsCollection | null = null;
// glyph-margin line -> hunk, for the gutter revert icons
const glyphHunks = new Map<number, Hunk>();
// view identity of what's currently in the editor; same key = update in place
let renderedKey: string | null = null;

/** update a model without rebuilding the editor; keeps scroll/cursor stable */
function softSetValue(ed: monaco.editor.ICodeEditor | null, model: monaco.editor.ITextModel, value: string) {
  if (model.getValue() === value) return;
  const vs = ed?.saveViewState() ?? null;
  model.setValue(value);
  if (vs) ed?.restoreViewState(vs);
}

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

/** IDEA-style jump: single hit goes straight to the location, several hits
 *  open a small inline chooser at (x, y) — no full-screen dialog. */
async function jumpToSymbol(word: string, x: number, y: number) {
  if (!repo.repo) return;
  let hits;
  try {
    hits = await api.searchText(repo.repo.root, word, true, 200);
  } catch (e) {
    toast(String(e), "error");
    return;
  }
  if (!hits.length) {
    toast(`未找到符号 "${word}"`);
    return;
  }
  if (hits.length === 1) {
    await repo.openAtLine(hits[0].path, hits[0].line);
    return;
  }
  openChooser(x, y, word, hits);
}

/** screen coordinates of the editor cursor, for anchoring the chooser */
function cursorScreenPos(ed: monaco.editor.IStandaloneCodeEditor): { x: number; y: number } {
  const pos = ed.getPosition();
  const dom = ed.getDomNode();
  if (!pos || !dom) return { x: 200, y: 200 };
  const vp = ed.getScrolledVisiblePosition(pos);
  const rect = dom.getBoundingClientRect();
  return { x: rect.left + (vp?.left ?? 0), y: rect.top + (vp?.top ?? 0) + (vp?.height ?? 18) + 4 };
}

/** Eclipse keys: F3 jump to symbol, Ctrl+Shift+G references (bottom panel),
 *  Ctrl+L go-to-line. Ctrl+hover underlines the symbol as a link (IDEA look). */
function addEclipseActions(ed: monaco.editor.IStandaloneCodeEditor) {
  const wordAtCursor = () => {
    const pos = ed.getPosition();
    return pos ? ed.getModel()?.getWordAtPosition(pos)?.word : undefined;
  };
  ed.addAction({
    id: "open-declaration",
    label: "跳转到符号",
    keybindings: [monaco.KeyCode.F3],
    contextMenuGroupId: "navigation",
    contextMenuOrder: 1,
    run: () => {
      const word = wordAtCursor();
      if (!word) return;
      const { x, y } = cursorScreenPos(ed);
      jumpToSymbol(word, x, y);
    },
  });
  ed.addAction({
    id: "find-references",
    label: "查找引用",
    keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyG],
    contextMenuGroupId: "navigation",
    contextMenuOrder: 2,
    run: () => {
      const word = wordAtCursor();
      if (word) openFindDialog(word, true);
    },
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
    if (word) jumpToSymbol(word, e.event.posx, e.event.posy + 16);
  });
}

function revealPendingLine(ed: monaco.editor.ICodeEditor | null) {
  const line = repo.pendingRevealLine;
  if (!line || !ed) return;
  repo.clearPendingReveal();
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
  if (isImageView.value) return "";
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
  renderedKey = null;
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
      cursorBlinking: "solid", // we blink the caret ourselves in CSS (see .cursor rule)
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

function rebuildHunkDecorations() {
  if (!editor) return;
  const d = repo.diff;
  const f = repo.selected;
  const mod = editor.getModifiedEditor();
  glyphHunks.clear();
  decorations?.clear();
  decorations = null;
  // hunk-level revert only applies to working-tree content edits (f is null
  // for history diffs); added/deleted/untracked revert whole from the list
  if (!d || !f || !(f.kind === "modified" || f.kind === "renamed") || !d.hunks.length) return;
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

function render() {
  if (!editor) return;
  const key = (repo.activeTabId ?? "none") + (contentMode.value ? ":content" : ":diff");

  if (contentMode.value) {
    const c = repo.content;
    // same tab refreshed (e.g. the file is being written to): update in place,
    // no model rebuild, no flicker, scroll preserved
    if (renderedKey === key && plainEditor?.getModel() && c?.content != null) {
      softSetValue(plainEditor, plainEditor.getModel()!, c.content);
      revealPendingLine(plainEditor);
      return;
    }
    clearView();
    renderContent();
    renderedKey = key;
    return;
  }

  const d = repo.diff;
  const diffPath = repo.selectedCommitPath ?? repo.selected?.path;
  if (!d || !diffPath || d.isBinary || d.tooLarge) {
    clearView();
    return;
  }

  if (renderedKey === key && models.length === 2) {
    softSetValue(editor.getOriginalEditor(), models[0], d.original ?? "");
    softSetValue(editor.getModifiedEditor(), models[1], d.modified ?? "");
    rebuildHunkDecorations();
    revealPendingLine(editor.getModifiedEditor());
    scheduleNavRefresh();
    return;
  }

  clearView();
  const lang = languageForPath(diffPath);
  const original = monaco.editor.createModel(d.original ?? "", lang);
  const modified = monaco.editor.createModel(d.modified ?? "", lang);
  models = [original, modified];
  editor.setModel({ original, modified });
  renderedKey = key;
  revealPendingLine(editor.getModifiedEditor());
  rebuildHunkDecorations();
  scheduleNavRefresh();
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

// stage/unstage aren't destructive (no confirm needed) — acts on whichever
// hunk the diff-nav prev/next arrows are currently parked on
const canStageHunks = computed(() => {
  const f = repo.selected;
  return repo.mode === "changes" && !!f && (f.kind === "modified" || f.kind === "renamed");
});

async function onStageCurrentHunk() {
  const d = repo.diff;
  if (!d || !d.hunks.length) return;
  const h = d.hunks[navIdx.value >= 0 ? navIdx.value : 0];
  if (!h) return;
  if (repo.activeTabStaged) await repo.unstageHunk(h);
  else await repo.stageHunk(h);
}

onMounted(() => {
  window.addEventListener("keyup", onGlobalKeyUp);
  editor = monaco.editor.createDiffEditor(container.value!, {
    readOnly: true,
    originalEditable: false,
    cursorBlinking: "solid", // we blink the caret ourselves in CSS (see .cursor rule)
    automaticLayout: true,
    renderSideBySide: settings.renderSideBySide,
    hideUnchangedRegions: { enabled: true },
    diffAlgorithm: "advanced",
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    renderOverviewRuler: false,
    scrollbar: {
      vertical: "visible",
      horizontal: "visible",
      verticalScrollbarSize: 10,
      horizontalScrollbarSize: 10,
      useShadows: false,
    },
    // no wrap on either side — rely on the horizontal scrollbar (kept
    // visible below) to reveal long lines, matching IDEA's diff viewer;
    // wrapping used to be enabled here but only reliably applied to one
    // side, producing a left/right mismatch — off is simpler to keep in sync
    wordWrap: "off",
    fontFamily: settings.editorFontFamily,
    fontSize: settings.editorFontSize,
  });
  // gutter revert icons live in the modified editor's glyph margin
  const mod = editor.getModifiedEditor();
  const orig = editor.getOriginalEditor();
  // the diff editor's shared constructor options don't always propagate
  // identically to both sub-editors (seen as the modified/right pane
  // silently missing its horizontal scrollbar) — set them explicitly on
  // both sides so original/modified never drift out of sync
  const sideOptions = {
    wordWrap: "off" as const,
    scrollbar: {
      vertical: "visible" as const,
      horizontal: "visible" as const,
      verticalScrollbarSize: 10,
      horizontalScrollbarSize: 10,
      useShadows: false,
    },
  };
  orig.updateOptions(sideOptions);
  mod.updateOptions({ ...sideOptions, glyphMargin: true });
  addEclipseActions(mod as monaco.editor.IStandaloneCodeEditor);
  addEclipseActions(editor.getOriginalEditor() as monaco.editor.IStandaloneCodeEditor);
  mod.onMouseDown((e) => {
    if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
    const line = e.target.position?.lineNumber;
    if (line) onRevertGlyphClick(line);
  });
  mod.addAction({
    id: "next-diff",
    label: "下一处改动",
    keybindings: [monaco.KeyCode.F7],
    run: () => goToDiff(1),
  });
  mod.addAction({
    id: "prev-diff",
    label: "上一处改动",
    keybindings: [monaco.KeyMod.Shift | monaco.KeyCode.F7],
    run: () => goToDiff(-1),
  });
  editor.onDidUpdateDiff(refreshNav);
  render();
});

watch(() => [repo.diff, repo.content] as const, render);
watch(
  () => [repo.activeTabId, isRasterImage.value] as const,
  async () => {
    imageData.value = null;
    if (!isRasterImage.value || !repo.repo || !repo.selectedPath) return;
    const f = repo.selected;
    // changed image → real kind; unchanged (all-files) → "added" forces single image
    const kind = f?.kind ?? "added";
    try {
      imageData.value = await api.getImageDiff(repo.repo.root, repo.selectedPath, f?.oldPath ?? null, kind);
    } catch (e) {
      toast(String(e), "error");
    }
  },
  { immediate: true },
);
// the editors are display:none while the image preview is up, so Monaco can't
// measure them; force a relayout when we switch back to the text view
watch(isImageView, (now, was) => {
  if (was && !now) nextTick(() => (contentMode.value ? plainEditor : editor)?.layout());
});
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
    <div v-if="!contentMode && !isImageView && settings.renderSideBySide" class="diff-side-labels">
      <span class="diff-side-label">修改前</span>
      <span class="diff-side-label">修改后</span>
    </div>
    <div
      v-show="!contentMode && !isImageView"
      ref="container"
      class="editor"
      :class="{ 'has-side-labels': settings.renderSideBySide }"
    ></div>
    <div v-show="contentMode && !showMdPreview && !isImageView" ref="plainContainer" class="editor"></div>
    <div v-if="showMdPreview" class="md-preview" @click="onPreviewClick" v-html="renderedMd"></div>

    <div v-if="isImageView && isVectorImage" class="image-diff">
      <div v-if="vectorCurrent" class="img-box"><img :src="vectorCurrent" alt="预览" /></div>
      <div v-else class="img-empty">无法预览此矢量文件（格式不受支持或解析失败）</div>
    </div>
    <div v-else-if="isImageView" class="image-diff">
      <div v-if="previewData?.original" class="img-pane">
        <div class="img-label">原始</div>
        <div class="img-box"><img :src="previewData.original" alt="原始" /></div>
      </div>
      <div v-if="previewData?.modified" class="img-pane">
        <div class="img-label">{{ previewData.original ? "当前" : "内容" }}</div>
        <div class="img-box"><img :src="previewData.modified" alt="当前" /></div>
      </div>
      <div v-if="previewData && !previewData.original && !previewData.modified" class="img-pane">
        <div class="img-label muted">无法预览（超过 5MB 或读取失败）</div>
      </div>
    </div>

    <div v-if="!contentMode && !isImageView && navList.length" class="diff-nav">
      <button title="上一处改动 (Shift+F7)" @click="goToDiff(-1)">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M4 10 8 6l4 4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <span class="diff-nav-count">{{ navIdx >= 0 ? navIdx + 1 : "·" }}<i>/</i>{{ navList.length }}</span>
      <button title="下一处改动 (F7)" @click="goToDiff(1)">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M4 6 8 10l4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <button
        v-if="canStageHunks"
        class="hunk-stage-btn"
        :title="repo.activeTabStaged ? '取消暂存当前修改块' : '暂存当前修改块'"
        @click="onStageCurrentHunk"
      >
        {{ repo.activeTabStaged ? "−" : "+" }}
      </button>
    </div>
    <div v-if="isMarkdown" class="md-toggle">
      <button :class="{ active: mdPreviewOn }" @click="mdPreviewOn = true">预览</button>
      <button :class="{ active: !mdPreviewOn }" @click="mdPreviewOn = false">源码</button>
    </div>
    <div v-if="isVectorImage" class="md-toggle">
      <button :class="{ active: vecPreviewOn }" @click="vecPreviewOn = true">预览</button>
      <button :class="{ active: !vecPreviewOn }" @click="vecPreviewOn = false">源码</button>
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
