<script setup lang="ts">
import { THEMES } from "../monaco/setup";
import { DEFAULT_EDITOR_FONT, useSettingsStore } from "../stores/settings";

defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();
const settings = useSettingsStore();

const FONT_SUGGESTIONS = [
  "Cascadia Code",
  "JetBrains Mono",
  "Fira Code",
  "Source Code Pro",
  "Consolas",
  "Sarasa Mono SC",
  "等距更纱黑体 SC",
  "Maple Mono",
  "MesloLGS NF",
  "Courier New",
];
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
          <span>代码字体（可输入本机任意字体，多个用逗号分隔）</span>
          <input
            type="text"
            list="font-suggestions"
            :value="settings.editorFontFamily"
            spellcheck="false"
            @change="settings.setEditorFont(($event.target as HTMLInputElement).value)"
          />
          <datalist id="font-suggestions">
            <option v-for="f in FONT_SUGGESTIONS" :key="f" :value="f" />
          </datalist>
          <button class="link-btn" @click="settings.setEditorFont(DEFAULT_EDITOR_FONT)">恢复默认字体</button>
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

        <label class="field row">
          <input
            type="checkbox"
            :checked="settings.renderSideBySide"
            @change="settings.setSideBySide(($event.target as HTMLInputElement).checked)"
          />
          <span>并排（side-by-side）显示 diff</span>
        </label>

        <div class="modal-actions">
          <button class="btn primary" @click="emit('close')">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
