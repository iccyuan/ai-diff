import { defineStore } from "pinia";
import { LazyStore } from "@tauri-apps/plugin-store";
import { applyTheme, THEMES } from "../monaco/setup";

const persist = new LazyStore("settings.json");

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    monacoTheme: "vs-dark",
    renderSideBySide: true,
    recentRepos: [] as string[],
  }),
  actions: {
    async load() {
      try {
        const theme = await persist.get<string>("monacoTheme");
        const side = await persist.get<boolean>("renderSideBySide");
        const recent = await persist.get<string[]>("recentRepos");
        if (theme && THEMES.some((t) => t.id === theme)) this.monacoTheme = theme;
        if (typeof side === "boolean") this.renderSideBySide = side;
        if (Array.isArray(recent)) this.recentRepos = recent;
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
  },
});
