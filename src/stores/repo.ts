import { defineStore } from "pinia";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type CommitInfo,
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

export interface ViewTab {
  id: string;
  title: string;
  path: string;
  /** commit hash for history tabs, null for working-tree / file-content tabs */
  commit: string | null;
  /** status snapshot needed by get_commit_file_diff */
  file: FileStatus | null;
}

function basename(path: string): string {
  return path.split("/").pop()!;
}

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
    // commit history panel
    historyOpen: true,
    commitPageSize: 30,
    commits: [] as CommitInfo[],
    commitsExhausted: false,
    loadingCommits: false,
    expandedCommits: [] as string[],
    commitFiles: {} as Record<string, FileStatus[]>,
    // when set, the diff pane shows a historical commit's file (read-only)
    selectedCommit: null as string | null,
    selectedCommitPath: null as string | null,
    // open viewer tabs
    tabs: [] as ViewTab[],
    activeTabId: null as string | null,
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
        this.commits = [];
        this.commitsExhausted = false;
        this.expandedCommits = [];
        this.commitFiles = {};
        this.selectedCommit = null;
        this.selectedCommitPath = null;
        this.tabs = [];
        this.activeTabId = null;
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
        if (this.historyOpen) {
          // re-fetch the already-loaded depth so new commits surface on top
          const count = Math.max(this.commits.length, this.commitPageSize);
          const list = await api.logCommits(this.repo.root, 0, count);
          this.commits = list;
          this.commitsExhausted = list.length < count;
        }
        const active = this.tabs.find((t) => t.id === this.activeTabId);
        if (active) {
          // worktree tabs auto-degrade to read-only content once the file is clean
          await this.loadForTab(active);
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
      await this.selectPath(f.path);
    },
    /** open (or focus) a working-tree tab: changed files show a diff, clean files read-only content */
    async selectPath(path: string) {
      await this.openTab({
        id: `wt:${path}`,
        title: basename(path),
        path,
        commit: null,
        file: null,
      });
    },
    /** ----- viewer tabs ----- */
    async openTab(tab: ViewTab) {
      if (!this.tabs.some((t) => t.id === tab.id)) this.tabs.push(tab);
      await this.activateTab(tab.id);
    },
    async activateTab(id: string) {
      const tab = this.tabs.find((t) => t.id === id);
      if (!tab) return;
      this.activeTabId = id;
      if (tab.commit) {
        this.selectedPath = null;
        this.selectedCommit = tab.commit;
        this.selectedCommitPath = tab.path;
      } else {
        this.selectedCommit = null;
        this.selectedCommitPath = null;
        this.selectedPath = tab.path;
      }
      await this.loadForTab(tab);
    },
    async loadForTab(tab: ViewTab) {
      if (!this.repo) return;
      if (tab.commit && tab.file) {
        this.loadingDiff = true;
        this.content = null;
        try {
          this.diff = await api.getCommitFileDiff(this.repo.root, tab.commit, tab.file);
        } catch (e) {
          this.diff = null;
          toast(String(e), "error");
        } finally {
          this.loadingDiff = false;
        }
        return;
      }
      const st = this.files.find((f) => f.path === tab.path);
      if (st) {
        await this.loadDiff(st);
      } else {
        await this.loadContent(tab.path);
      }
    },
    closeTab(id: string) {
      const i = this.tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      this.tabs.splice(i, 1);
      if (this.activeTabId !== id) return;
      const next = this.tabs[i] ?? this.tabs[i - 1];
      if (next) {
        this.activateTab(next.id);
      } else {
        this.clearActiveView();
      }
    },
    clearActiveView() {
      this.activeTabId = null;
      this.selectedPath = null;
      this.selectedCommit = null;
      this.selectedCommitPath = null;
      this.diff = null;
      this.content = null;
    },
    closeAllTabs() {
      this.tabs = [];
      this.clearActiveView();
    },
    closeOtherTabs(id: string) {
      const keep = this.tabs.find((t) => t.id === id);
      if (!keep) return;
      this.tabs = [keep];
      if (this.activeTabId !== id) this.activateTab(id);
    },
    closeLeftTabs(id: string) {
      const i = this.tabs.findIndex((t) => t.id === id);
      if (i <= 0) return;
      const closingActive = this.tabs.slice(0, i).some((t) => t.id === this.activeTabId);
      this.tabs = this.tabs.slice(i);
      if (closingActive) this.activateTab(id);
    },
    closeRightTabs(id: string) {
      const i = this.tabs.findIndex((t) => t.id === id);
      if (i < 0 || i === this.tabs.length - 1) return;
      const closingActive = this.tabs.slice(i + 1).some((t) => t.id === this.activeTabId);
      this.tabs = this.tabs.slice(0, i + 1);
      if (closingActive) this.activateTab(id);
    },
    /** history panel actions */
    async toggleHistory() {
      this.historyOpen = !this.historyOpen;
      if (this.historyOpen && !this.commits.length) await this.loadCommits();
    },
    async loadCommits(reset = false) {
      if (!this.repo || this.loadingCommits) return;
      if (reset) {
        this.commits = [];
        this.commitsExhausted = false;
        this.commitFiles = {};
        this.expandedCommits = [];
      }
      if (this.commitsExhausted) return;
      this.loadingCommits = true;
      try {
        // page size adapts to the panel height; caller passes via pageSize param-free heuristic
        const count = this.commitPageSize;
        const batch = await api.logCommits(this.repo.root, this.commits.length, count);
        this.commits.push(...batch);
        if (batch.length < count) this.commitsExhausted = true;
      } catch (e) {
        toast(String(e), "error");
      } finally {
        this.loadingCommits = false;
      }
    },
    async toggleCommit(hash: string) {
      const i = this.expandedCommits.indexOf(hash);
      if (i >= 0) {
        this.expandedCommits.splice(i, 1);
        return;
      }
      this.expandedCommits.push(hash);
      if (!this.commitFiles[hash] && this.repo) {
        try {
          this.commitFiles[hash] = await api.commitFiles(this.repo.root, hash);
        } catch (e) {
          toast(String(e), "error");
        }
      }
    },
    async selectCommitFile(hash: string, f: FileStatus) {
      await this.openTab({
        id: `${hash.slice(0, 12)}:${f.path}`,
        title: basename(f.path),
        path: f.path,
        commit: hash,
        file: f,
      });
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
