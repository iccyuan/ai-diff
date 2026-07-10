<script setup lang="ts">
import { EDITOR_FONTS, THEMES } from "../monaco/setup";
import { useSettingsStore } from "../stores/settings";
import { useRepoStore } from "../stores/repo";
import { checkForUpdate, updateState } from "../lib/update";

defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();
const settings = useSettingsStore();
const repo = useRepoStore();
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="modal-mask" @click.self="emit('close')">
      <div class="modal settings">
        <h3>设置</h3>

        <label class="field">
          <span>代码主题</span>
          <select :value="settings.monacoTheme" @change="settings.setTheme(($event.target as HTMLSelectElement).value)">
            <optgroup label="浅色">
              <option v-for="t in THEMES.filter((t) => t.kind === 'light')" :key="t.id" :value="t.id">
                {{ t.label }}
              </option>
            </optgroup>
            <optgroup label="深色">
              <option v-for="t in THEMES.filter((t) => t.kind === 'dark')" :key="t.id" :value="t.id">
                {{ t.label }}
              </option>
            </optgroup>
          </select>
        </label>

        <label class="field">
          <span>代码字体（随软件内置，无需安装）</span>
          <select :value="settings.editorFont" @change="settings.setEditorFont(($event.target as HTMLSelectElement).value)">
            <option v-for="f in EDITOR_FONTS" :key="f.id" :value="f.id" :style="{ fontFamily: f.family }">
              {{ f.label }}
            </option>
          </select>
        </label>

        <label class="field">
          <span>代码字号：{{ settings.editorFontSize }}px</span>
          <input
            type="range"
            min="10"
            max="24"
            step="1"
            :value="settings.editorFontSize"
            @input="settings.setEditorFontSize(Number(($event.target as HTMLInputElement).value))"
          />
        </label>

        <label class="field">
          <span>代码行高：{{ settings.editorLineHeight.toFixed(2) }} 倍（VS Code 约 1.35）</span>
          <input
            type="range"
            min="1.2"
            max="1.8"
            step="0.05"
            :value="settings.editorLineHeight"
            @input="settings.setEditorLineHeight(Number(($event.target as HTMLInputElement).value))"
          />
        </label>

        <label class="field row">
          <input
            type="checkbox"
            :checked="settings.renderSideBySide"
            @change="settings.setSideBySide(($event.target as HTMLInputElement).checked)"
          />
          <span>并排（side-by-side）显示 diff</span>
        </label>

        <label class="field">
          <span>Pull 默认策略</span>
          <select
            :value="settings.pullStrategy"
            @change="settings.setPullStrategy(($event.target as HTMLSelectElement).value as 'merge' | 'rebase')"
          >
            <option value="merge">Merge（默认，创建合并提交）</option>
            <option value="rebase">Rebase（变基，历史保持线性）</option>
          </select>
        </label>

        <label class="field row">
          <input
            type="checkbox"
            :checked="settings.showHidden"
            @change="repo.toggleHidden()"
          />
          <span>显示隐藏的文件和文件夹</span>
        </label>

        <label class="field row">
          <input
            type="checkbox"
            :checked="settings.glassEffect"
            @change="settings.setGlassEffect(($event.target as HTMLInputElement).checked)"
          />
          <span>毛玻璃效果</span>
        </label>

        <label class="field">
          <span>毛玻璃透明度：{{ 100 - settings.glassOpacity }}%</span>
          <input
            type="range"
            min="25"
            max="75"
            step="1"
            :disabled="!settings.glassEffect"
            :value="100 - settings.glassOpacity"
            @input="settings.setGlassOpacity(100 - Number(($event.target as HTMLInputElement).value))"
          />
        </label>

        <div class="modal-actions">
          <button class="btn" :disabled="updateState.busy" @click="checkForUpdate(true)">
            {{ updateState.busy ? "检查中…" : "检查更新" }}
          </button>
          <button class="btn primary" @click="emit('close')">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
