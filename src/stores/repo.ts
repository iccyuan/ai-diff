import { acceptHMRUpdate, defineStore } from "pinia";
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

/** everything one open project needs; the window can hold several of these */
export interface Workspace {
  repo: RepoInfo;
  files: FileStatus[];
  allFiles: string[];
  selectedPath: string | null;
  diff: FileDiff | null;
  content: FileContent | null;
  loadingStatus: boolean;
  loadingDiff: boolean;
  historyOpen: boolean;
  commitPageSize: number;
  commits: CommitInfo[];
  commitsExhausted: boolean;
  loadingCommits: boolean;
  expandedCommits: string[];
  commitFiles: Record<string, FileStatus[]>;
  selectedCommit: string | null;
  selectedCommitPath: string | null;
  tabs: ViewTab[];
  activeTabId: string | null;
  pendingRevealLine: number | null;
}

function blankWorkspace(info: RepoInfo): Workspace {
  return {
    repo: info,
    files: [],
    allFiles: [],
    selectedPath: null,
    diff: null,
    content: null,
    loadingStatus: false,
    loadingDiff: false,
    historyOpen: false,
    commitPageSize: 30,
    commits: [],
    commitsExhausted: false,
    loadingCommits: false,
    expandedCommits: [],
    commitFiles: {},
    selectedCommit: null,
    selectedCommitPath: null,
    tabs: [],
    activeTabId: null,
    pendingRevealLine: null,
  };
}

function basename(path: string): string {
  return path.split("/").pop()!;
}

export const useRepoStore = defineStore("repo", {
  state: () => ({
    workspaces: [] as Workspace[],
    active: -1,
    /** sidebar view mode is global: one switch for every project */
    viewMode: "changes" as "changes" | "all",
  }),
  getters: {
    ws(state): Workspace | null {
      return state.workspaces[state.active] ?? null;
    },
    repo(): RepoInfo | null {
      return this.ws?.repo ?? null;
    },
    files(): FileStatus[] {
      return this.ws?.files ?? [];
    },
    allFiles(): string[] {
      return this.ws?.allFiles ?? [];
    },
    mode(state): "changes" | "all" {
      return state.viewMode;
    },
    selectedPath(): string | null {
      return this.ws?.selectedPath ?? null;
    },
    diff(): FileDiff | null {
      return this.ws?.diff ?? null;
    },
    content(): FileContent | null {
      return this.ws?.content ?? null;
    },
    loadingStatus(): boolean {
      return this.ws?.loadingStatus ?? false;
    },
    loadingDiff(): boolean {
      return this.ws?.loadingDiff ?? false;
    },
    historyOpen(): boolean {
      return this.ws?.historyOpen ?? false;
    },
    commitPageSize(): number {
      return this.ws?.commitPageSize ?? 30;
    },
    commits(): CommitInfo[] {
      return this.ws?.commits ?? [];
    },
    commitsExhausted(): boolean {
      return this.ws?.commitsExhausted ?? false;
    },
    loadingCommits(): boolean {
      return this.ws?.loadingCommits ?? false;
    },
    expandedCommits(): string[] {
      return this.ws?.expandedCommits ?? [];
    },
    commitFiles(): Record<string, FileStatus[]> {
      return this.ws?.commitFiles ?? {};
    },
    selectedCommit(): string | null {
      return this.ws?.selectedCommit ?? null;
    },
    selectedCommitPath(): string | null {
      return this.ws?.selectedCommitPath ?? null;
    },
    tabs(): ViewTab[] {
      return this.ws?.tabs ?? [];
    },
    activeTabId(): string | null {
      return this.ws?.activeTabId ?? null;
    },
    pendingRevealLine(): number | null {
      return this.ws?.pendingRevealLine ?? null;
    },
    selected(): FileStatus | null {
      const w = this.ws;
      return w?.files.find((f) => f.path === w.selectedPath) ?? null;
    },
  },
  actions: {
    /** open a repo as a workspace; re-opening an existing one just activates it */
    async openRepo(path: string) {
      try {
        const info = await api.openRepo(path);
        const existing = this.workspaces.findIndex((w) => w.repo.root === info.root);
        if (existing >= 0) {
          this.active = existing;
          this.workspaces[existing].repo = info;
          await this.refreshWs(this.workspaces[existing]);
          return;
        }
        this.workspaces.push(blankWorkspace(info));
        this.active = this.workspaces.length - 1;
        // use the reactive proxy from the array, NOT the raw pushed object —
        // mutating the raw object updates values without triggering reactivity
        const w = this.workspaces[this.active];
        await useSettingsStore().addRecent(info.root);
        await this.refreshWs(w);
        await api.watchRepo(info.root);
        if (!watcherHooked) {
          watcherHooked = true;
          await listen<string>("repo-changed", (e) => {
            const target = this.workspaces.find((x) => x.repo.root === e.payload);
            if (!target || target.loadingStatus || Date.now() < suppressAutoRefreshUntil) return;
            this.refreshWs(target);
          });
        }
      } catch (e) {
        toast(String(e), "error");
      }
    },
    activateWorkspace(i: number) {
      if (i < 0 || i >= this.workspaces.length) return;
      this.active = i;
      if (this.viewMode === "all") this.ensureAllFiles();
    },
    moveWorkspace(from: number, to: number) {
      const n = this.workspaces.length;
      if (from === to || from < 0 || to < 0 || from >= n || to >= n) return;
      const act = this.workspaces[this.active] ?? null;
      const [w] = this.workspaces.splice(from, 1);
      this.workspaces.splice(to, 0, w);
      if (act) this.active = this.workspaces.indexOf(act);
    },
    closeWorkspace(i: number) {
      const w = this.workspaces[i];
      if (!w) return;
      api.unwatchRepo(w.repo.root).catch(() => {});
      this.workspaces.splice(i, 1);
      if (this.active >= this.workspaces.length) this.active = this.workspaces.length - 1;
      else if (this.active > i) this.active--;
    },
    async refresh() {
      if (this.ws) await this.refreshWs(this.ws);
    },
    /**
     * The single consistency rule: every mutation (any revert) must end here,
     * so hunk offsets are always re-derived from the current file state.
     */
    async refreshWs(w: Workspace) {
      w.loadingStatus = true;
      try {
        w.files = await api.getStatus(w.repo.root);
        if (this.viewMode === "all") {
          w.allFiles = await api.listFiles(w.repo.root);
        }
        if (w.historyOpen) {
          // re-fetch the already-loaded depth so new commits surface on top
          const count = Math.max(w.commits.length, w.commitPageSize);
          const list = await api.logCommits(w.repo.root, 0, count);
          w.commits = list;
          w.commitsExhausted = list.length < count;
        }
        const active = w.tabs.find((t) => t.id === w.activeTabId);
        if (active) {
          // worktree tabs auto-degrade to read-only content once the file is clean
          await this.loadForTab(w, active);
        }
      } catch (e) {
        toast(String(e), "error");
      } finally {
        w.loadingStatus = false;
        suppressAutoRefreshUntil = Date.now() + 800;
      }
    },
    async setMode(mode: "changes" | "all") {
      if (this.viewMode === mode) return;
      this.viewMode = mode;
      if (mode === "all") await this.ensureAllFiles();
      // the active working-tree tab flips diff <-> content with the mode
      const w = this.ws;
      const active = w?.tabs.find((t) => t.id === w.activeTabId);
      if (w && active && !active.commit) await this.loadForTab(w, active);
    },
    async ensureAllFiles() {
      const w = this.ws;
      if (!w || w.allFiles.length) return;
      try {
        w.allFiles = await api.listFiles(w.repo.root);
      } catch (e) {
        toast(String(e), "error");
      }
    },
    /** open a file (worktree view) and scroll to a specific line */
    async openAtLine(path: string, line: number) {
      const w = this.ws;
      if (!w) return;
      w.pendingRevealLine = line;
      await this.selectPath(path);
    },
    clearPendingReveal() {
      if (this.ws) this.ws.pendingRevealLine = null;
    },
    async selectFile(f: FileStatus) {
      await this.selectPath(f.path);
    },
    /** open (or focus) a working-tree tab: 更改 shows a diff for changed files, 全部文件 shows content */
    async selectPath(path: string) {
      await this.openTab({
        id: `wt:${path}`,
        title: basename(path),
        path,
        commit: null,
        file: null,
      });
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
    /** ----- viewer tabs ----- */
    async openTab(tab: ViewTab) {
      const w = this.ws;
      if (!w) return;
      if (!w.tabs.some((t) => t.id === tab.id)) w.tabs.push(tab);
      await this.activateTab(tab.id);
    },
    async activateTab(id: string) {
      const w = this.ws;
      const tab = w?.tabs.find((t) => t.id === id);
      if (!w || !tab) return;
      w.activeTabId = id;
      if (tab.commit) {
        w.selectedPath = null;
        w.selectedCommit = tab.commit;
        w.selectedCommitPath = tab.path;
      } else {
        w.selectedCommit = null;
        w.selectedCommitPath = null;
        w.selectedPath = tab.path;
      }
      await this.loadForTab(w, tab);
    },
    async loadForTab(w: Workspace, tab: ViewTab) {
      if (tab.commit && tab.file) {
        w.loadingDiff = true;
        w.content = null;
        try {
          w.diff = await api.getCommitFileDiff(w.repo.root, tab.commit, tab.file);
        } catch (e) {
          w.diff = null;
          toast(String(e), "error");
        } finally {
          w.loadingDiff = false;
        }
        return;
      }
      // 更改 view shows a diff for changed files; 全部文件 always shows content
      const st = w.files.find((f) => f.path === tab.path);
      if (st && this.viewMode === "changes") {
        await this.loadDiff(w, st);
      } else {
        await this.loadContent(w, tab.path);
      }
    },
    moveTab(from: number, to: number) {
      const w = this.ws;
      if (!w) return;
      const n = w.tabs.length;
      if (from === to || from < 0 || to < 0 || from >= n || to >= n) return;
      const [t] = w.tabs.splice(from, 1);
      w.tabs.splice(to, 0, t);
    },
    closeTab(id: string) {
      const w = this.ws;
      if (!w) return;
      const i = w.tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      w.tabs.splice(i, 1);
      if (w.activeTabId !== id) return;
      const next = w.tabs[i] ?? w.tabs[i - 1];
      if (next) {
        this.activateTab(next.id);
      } else {
        this.clearActiveView();
      }
    },
    clearActiveView() {
      const w = this.ws;
      if (!w) return;
      w.activeTabId = null;
      w.selectedPath = null;
      w.selectedCommit = null;
      w.selectedCommitPath = null;
      w.diff = null;
      w.content = null;
    },
    closeAllTabs() {
      const w = this.ws;
      if (!w) return;
      w.tabs = [];
      this.clearActiveView();
    },
    closeOtherTabs(id: string) {
      const w = this.ws;
      if (!w) return;
      const keep = w.tabs.find((t) => t.id === id);
      if (!keep) return;
      w.tabs = [keep];
      if (w.activeTabId !== id) this.activateTab(id);
    },
    closeLeftTabs(id: string) {
      const w = this.ws;
      if (!w) return;
      const i = w.tabs.findIndex((t) => t.id === id);
      if (i <= 0) return;
      const closingActive = w.tabs.slice(0, i).some((t) => t.id === w.activeTabId);
      w.tabs = w.tabs.slice(i);
      if (closingActive) this.activateTab(id);
    },
    closeRightTabs(id: string) {
      const w = this.ws;
      if (!w) return;
      const i = w.tabs.findIndex((t) => t.id === id);
      if (i < 0 || i === w.tabs.length - 1) return;
      const closingActive = w.tabs.slice(i + 1).some((t) => t.id === w.activeTabId);
      w.tabs = w.tabs.slice(0, i + 1);
      if (closingActive) this.activateTab(id);
    },
    async loadDiff(w: Workspace, f: FileStatus) {
      w.loadingDiff = true;
      w.content = null;
      try {
        w.diff = await api.getFileDiff(w.repo.root, f);
      } catch (e) {
        w.diff = null;
        toast(String(e), "error");
      } finally {
        w.loadingDiff = false;
      }
    },
    async loadContent(w: Workspace, path: string) {
      w.loadingDiff = true;
      w.diff = null;
      try {
        w.content = await api.readFile(w.repo.root, path);
      } catch (e) {
        w.content = null;
        toast(String(e), "error");
      } finally {
        w.loadingDiff = false;
      }
    },
    async revertFile(f: FileStatus) {
      const w = this.ws;
      if (!w) return;
      try {
        await api.revertFile(w.repo.root, f);
        toast(f.kind === "untracked" ? `已删除 ${f.path}` : `已还原 ${f.path}`);
      } catch (e) {
        toast(String(e), "error");
      }
      await this.refreshWs(w);
    },
    async revertHunk(hunk: Hunk) {
      const w = this.ws;
      if (!w || !w.diff || !this.selected) return;
      const patch = w.diff.fileHeader + hunk.text;
      try {
        await api.revertHunk(w.repo.root, this.selected.path, patch);
        toast("已还原该修改块");
      } catch (e) {
        toast(String(e), "error");
      }
      await this.refreshWs(w);
    },
    async revertAll() {
      const w = this.ws;
      if (!w) return;
      try {
        await api.revertAll(w.repo.root);
        toast("已还原全部更改");
      } catch (e) {
        toast(String(e), "error");
      }
      await this.refreshWs(w);
    },
    /** ----- history panel ----- */
    async toggleHistory() {
      const w = this.ws;
      if (!w) return;
      w.historyOpen = !w.historyOpen;
      if (w.historyOpen && !w.commits.length) await this.loadCommits();
    },
    setCommitPageSize(n: number) {
      if (this.ws) this.ws.commitPageSize = n;
    },
    async loadCommits(reset = false) {
      const w = this.ws;
      if (!w || w.loadingCommits) return;
      if (reset) {
        w.commits = [];
        w.commitsExhausted = false;
        w.commitFiles = {};
        w.expandedCommits = [];
      }
      if (w.commitsExhausted) return;
      w.loadingCommits = true;
      try {
        const count = w.commitPageSize;
        const batch = await api.logCommits(w.repo.root, w.commits.length, count);
        w.commits.push(...batch);
        if (batch.length < count) w.commitsExhausted = true;
      } catch (e) {
        toast(String(e), "error");
      } finally {
        w.loadingCommits = false;
      }
    },
    async toggleCommit(hash: string) {
      const w = this.ws;
      if (!w) return;
      const i = w.expandedCommits.indexOf(hash);
      if (i >= 0) {
        w.expandedCommits.splice(i, 1);
        return;
      }
      w.expandedCommits.push(hash);
      if (!w.commitFiles[hash]) {
        try {
          w.commitFiles[hash] = await api.commitFiles(w.repo.root, hash);
        } catch (e) {
          toast(String(e), "error");
        }
      }
    },
  },
});

// hot-replace this store's actions on edit instead of keeping the stale instance
if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useRepoStore, import.meta.hot));
}
