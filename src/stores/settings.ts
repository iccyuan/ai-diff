import { defineStore } from "pinia";
import { LazyStore } from "@tauri-apps/plugin-store";
import { applyTheme, DEFAULT_EDITOR_FONT_ID, EDITOR_FONTS, fontFamilyFor, THEMES } from "../monaco/setup";

const persist = new LazyStore("settings.json");

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    monacoTheme: "vs-dark",
    renderSideBySide: true,
    recentRepos: [] as string[],
    editorFont: DEFAULT_EDITOR_FONT_ID,
    editorFontSize: 13,
    sidebarWidth: 300,
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
        if (theme && THEMES.some((t) => t.id === theme)) this.monacoTheme = theme;
        if (typeof side === "boolean") this.renderSideBySide = side;
        if (Array.isArray(recent)) this.recentRepos = recent;
        if (font && EDITOR_FONTS.some((f) => f.id === font)) this.editorFont = font;
        if (typeof size === "number" && size >= 10 && size <= 24) this.editorFontSize = size;
        if (typeof width === "number" && width >= 200 && width <= 600) this.sidebarWidth = width;
      } catch {
        // first launch / unreadable store: keep defaults
      }
      applyTheme(this.monacoTheme);
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
    async saveSidebarWidth() {
      await persist.set("sidebarWidth", this.sidebarWidth);
      await persist.save();
    },
  },
});
