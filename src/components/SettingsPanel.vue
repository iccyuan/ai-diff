<script setup lang="ts">
import { THEMES } from "../monaco/setup";
import { useSettingsStore } from "../stores/settings";

defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();
const settings = useSettingsStore();
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
