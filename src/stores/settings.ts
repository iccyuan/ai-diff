import { acceptHMRUpdate, defineStore } from "pinia";
import { LazyStore } from "@tauri-apps/plugin-store";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyTheme, DEFAULT_EDITOR_FONT_ID, EDITOR_FONTS, fontFamilyFor, THEMES } from "../monaco/setup";

const persist = new LazyStore("settings.json");

/** flip the frosted-glass styling (CSS) and the native acrylic material on/off */
function applyGlass(on: boolean) {
  document.documentElement.dataset.glass = on ? "on" : "off";
  getCurrentWindow()
    .setEffects({ effects: on ? ["acrylic"] : [] } as never)
    .catch(() => {});
}

/** base glass opacity (percent 25–75) → the --glass-op CSS knob */
function applyGlassOpacity(percent: number) {
  document.documentElement.style.setProperty("--glass-op", String(percent / 100));
}

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    monacoTheme: "vs-dark",
    renderSideBySide: true,
    recentRepos: [] as string[],
    editorFont: DEFAULT_EDITOR_FONT_ID,
    editorFontSize: 13,
    sidebarWidth: 300,
    glassEffect: true,
    glassOpacity: 54,
    /** 全部文件 view: also list .gitignore'd / hidden files and folders */
    showHidden: false,
  }),
  getters: {
    editorFontFamily(state): string {
      return fontFamilyFor(state.editorFont);
    },
  },
  actions: {
    async load() {
      try {
        const theme = await persist.get<string>("monacoTheme");
        const side = await persist.get<boolean>("renderSideBySide");
        const recent = await persist.get<string[]>("recentRepos");
        const font = await persist.get<string>("editorFont");
        const size = await persist.get<number>("editorFontSize");
        const width = await persist.get<number>("sidebarWidth");
        const glass = await persist.get<boolean>("glassEffect");
        const glassOp = await persist.get<number>("glassOpacity");
        const hidden = await persist.get<boolean>("showHidden");
        if (theme && THEMES.some((t) => t.id === theme)) this.monacoTheme = theme;
        if (typeof side === "boolean") this.renderSideBySide = side;
        if (Array.isArray(recent)) this.recentRepos = recent;
        if (font && EDITOR_FONTS.some((f) => f.id === font)) this.editorFont = font;
        if (typeof size === "number" && size >= 10 && size <= 24) this.editorFontSize = size;
        if (typeof width === "number" && width >= 200 && width <= 600) this.sidebarWidth = width;
        if (typeof glass === "boolean") this.glassEffect = glass;
        if (typeof glassOp === "number" && glassOp >= 25 && glassOp <= 75) this.glassOpacity = glassOp;
        if (typeof hidden === "boolean") this.showHidden = hidden;
      } catch {
        // first launch / unreadable store: keep defaults
      }
      applyTheme(this.monacoTheme);
      applyGlass(this.glassEffect);
      applyGlassOpacity(this.glassOpacity);
    },
    async setGlassEffect(v: boolean) {
      this.glassEffect = v;
      applyGlass(v);
      await persist.set("glassEffect", v);
      await persist.save();
    },
    async setGlassOpacity(percent: number) {
      this.glassOpacity = Math.min(75, Math.max(25, Math.round(percent) || 54));
      applyGlassOpacity(this.glassOpacity);
      await persist.set("glassOpacity", this.glassOpacity);
      await persist.save();
    },
    async setTheme(id: string) {
      this.monacoTheme = id;
      applyTheme(id);
      await persist.set("monacoTheme", id);
      await persist.save();
    },
    async setSideBySide(v: boolean) {
      this.renderSideBySide = v;
      await persist.set("renderSideBySide", v);
      await persist.save();
    },
    async addRecent(path: string) {
      this.recentRepos = [path, ...this.recentRepos.filter((p) => p !== path)].slice(0, 10);
      await persist.set("recentRepos", [...this.recentRepos]);
      await persist.save();
    },
    async setEditorFont(id: string) {
      if (!EDITOR_FONTS.some((f) => f.id === id)) return;
      this.editorFont = id;
      await persist.set("editorFont", id);
      await persist.save();
    },
    async setEditorFontSize(size: number) {
      this.editorFontSize = Math.min(24, Math.max(10, Math.round(size) || 13));
      await persist.set("editorFontSize", this.editorFontSize);
      await persist.save();
    },
    async setShowHidden(v: boolean) {
      this.showHidden = v;
      await persist.set("showHidden", v);
      await persist.save();
    },
    async saveSidebarWidth() {
      await persist.set("sidebarWidth", this.sidebarWidth);
      await persist.save();
    },
  },
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useSettingsStore, import.meta.hot));
}
