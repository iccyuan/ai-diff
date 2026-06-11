import { defineStore } from "pinia";
import { LazyStore } from "@tauri-apps/plugin-store";
import { applyTheme, THEMES } from "../monaco/setup";

const persist = new LazyStore("settings.json");

export const DEFAULT_EDITOR_FONT =
  '"Cascadia Code", "JetBrains Mono", "Fira Code", "Sarasa Mono SC", Consolas, "Courier New", monospace';

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    monacoTheme: "vs-dark",
    renderSideBySide: true,
    recentRepos: [] as string[],
    editorFontFamily: DEFAULT_EDITOR_FONT,
    editorFontSize: 13,
    sidebarWidth: 300,
  }),
  actions: {
    async load() {
      try {
        const theme = await persist.get<string>("monacoTheme");
        const side = await persist.get<boolean>("renderSideBySide");
        const recent = await persist.get<string[]>("recentRepos");
        const font = await persist.get<string>("editorFontFamily");
        const size = await persist.get<number>("editorFontSize");
        const width = await persist.get<number>("sidebarWidth");
        if (theme && THEMES.some((t) => t.id === theme)) this.monacoTheme = theme;
        if (typeof side === "boolean") this.renderSideBySide = side;
        if (Array.isArray(recent)) this.recentRepos = recent;
        if (font && font.trim()) this.editorFontFamily = font;
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
    async setEditorFont(family: string) {
      this.editorFontFamily = family.trim() || DEFAULT_EDITOR_FONT;
      await persist.set("editorFontFamily", this.editorFontFamily);
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
