import { defineStore } from "pinia";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type FileContent,
  type FileDiff,
  type FileStatus,
  type Hunk,
  type RepoInfo,
} from "../lib/api";
import { toast } from "../lib/toast";
import { useSettingsStore } from "./settings";

let watcherHooked = false;
// our own refreshes touch .git/index and would echo back as watcher events
let suppressAutoRefreshUntil = 0;

export const useRepoStore = defineStore("repo", {
  state: () => ({
    repo: null as RepoInfo | null,
    files: [] as FileStatus[],
    selectedPath: null as string | null,
    diff: null as FileDiff | null,
    loadingStatus: false,
    loadingDiff: false,
    mode: "changes" as "changes" | "all",
    allFiles: [] as string[],
    content: null as FileContent | null,
  }),
  getters: {
    selected(state): FileStatus | null {
      return state.files.find((f) => f.path === state.selectedPath) ?? null;
    },
  },
  actions: {
    async openRepo(path: string) {
      try {
        const info = await api.openRepo(path);
        this.repo = info;
        this.selectedPath = null;
        this.diff = null;
        this.content = null;
        this.allFiles = [];
        await useSettingsStore().addRecent(info.root);
        await this.refresh();
        await api.watchRepo(info.root);
        if (!watcherHooked) {
          watcherHooked = true;
          await listen<string>("repo-changed", (e) => {
            if (!this.repo || e.payload !== this.repo.root) return;
            if (this.loadingStatus || Date.now() < suppressAutoRefreshUntil) return;
            this.refresh();
          });
        }
      } catch (e) {
        toast(String(e), "error");
      }
    },
    /**
     * The single consistency rule: every mutation (any revert) must end here,
     * so hunk offsets are always re-derived from the current file state.
     */
    async refresh() {
      if (!this.repo) return;
      this.loadingStatus = true;
      try {
        this.files = await api.getStatus(this.repo.root);
        if (this.mode === "all") {
          this.allFiles = await api.listFiles(this.repo.root);
        }
        const sel = this.files.find((f) => f.path === this.selectedPath);
        if (sel) {
          await this.loadDiff(sel);
        } else if (this.mode === "all" && this.selectedPath && this.allFiles.includes(this.selectedPath)) {
          await this.loadContent(this.selectedPath);
        } else {
          this.selectedPath = null;
          this.diff = null;
          this.content = null;
        }
      } catch (e) {
        toast(String(e), "error");
      } finally {
        this.loadingStatus = false;
        suppressAutoRefreshUntil = Date.now() + 800;
      }
    },
    async setMode(mode: "changes" | "all") {
      if (this.mode === mode) return;
      this.mode = mode;
      if (mode === "all" && this.repo && !this.allFiles.length) {
        try {
          this.allFiles = await api.listFiles(this.repo.root);
        } catch (e) {
          toast(String(e), "error");
        }
      }
    },
    async selectFile(f: FileStatus) {
      if (this.selectedPath === f.path && this.diff) return;
      this.selectedPath = f.path;
      await this.loadDiff(f);
    },
    /** all-files view: changed files open as diff, the rest as read-only content */
    async selectPath(path: string) {
      if (this.selectedPath === path) return;
      this.selectedPath = path;
      const st = this.files.find((f) => f.path === path);
      if (st) {
        await this.loadDiff(st);
      } else {
        await this.loadContent(path);
      }
    },
    async loadDiff(f: FileStatus) {
      if (!this.repo) return;
      this.loadingDiff = true;
      this.content = null;
      try {
        this.diff = await api.getFileDiff(this.repo.root, f);
      } catch (e) {
        this.diff = null;
        toast(String(e), "error");
      } finally {
        this.loadingDiff = false;
      }
    },
    async loadContent(path: string) {
      if (!this.repo) return;
      this.loadingDiff = true;
      this.diff = null;
      try {
        this.content = await api.readFile(this.repo.root, path);
      } catch (e) {
        this.content = null;
        toast(String(e), "error");
      } finally {
        this.loadingDiff = false;
      }
    },
    async revertFile(f: FileStatus) {
      if (!this.repo) return;
      try {
        await api.revertFile(this.repo.root, f);
        toast(f.kind === "untracked" ? `已删除 ${f.path}` : `已还原 ${f.path}`);
      } catch (e) {
        toast(String(e), "error");
      }
      await this.refresh();
    },
    async revertHunk(hunk: Hunk) {
      if (!this.repo || !this.diff || !this.selected) return;
      const patch = this.diff.fileHeader + hunk.text;
      try {
        await api.revertHunk(this.repo.root, this.selected.path, patch);
        toast("已还原该修改块");
      } catch (e) {
        toast(String(e), "error");
      }
      await this.refresh();
    },
    async revertAll() {
      if (!this.repo) return;
      try {
        await api.revertAll(this.repo.root);
        toast("已还原全部更改");
      } catch (e) {
        toast(String(e), "error");
      }
      await this.refresh();
    },
  },
});
