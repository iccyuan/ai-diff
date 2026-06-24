import { reactive } from "vue";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { confirmDialog } from "./confirm";
import { toast, updateToast, dismissToast } from "./toast";

/** Shared state so the "检查更新" button can show progress / disable itself. */
export const updateState = reactive<{ busy: boolean }>({ busy: false });

function formatMB(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Check GitHub releases for a newer signed build; prompt then install.
 *  No-op in dev / when offline. `manual` surfaces progress, "already latest"
 *  and errors via toasts; the silent launch check stays quiet. */
export async function checkForUpdate(manual = false) {
  if (updateState.busy) return;
  updateState.busy = true;
  // a single sticky toast we keep updating through the flow (manual only)
  let progress: number | null = manual ? toast("正在检查更新…", "progress") : null;
  const setProgress = (text: string, kind?: "ok" | "error" | "progress") => {
    if (progress != null) updateToast(progress, text, kind);
    else if (manual && kind === "error") toast(text, "error");
  };
  try {
    const update = await check();
    if (!update) {
      setProgress("已是最新版本", "ok");
      return;
    }
    // hide the sticky toast while the confirm dialog is up
    if (progress != null) {
      dismissToast(progress);
      progress = null;
    }
    const ok = await confirmDialog(
      `发现新版本 ${update.version}`,
      "是否现在下载并安装？安装完成后应用会自动重启。",
    );
    if (!ok) return;

    progress = toast("正在下载更新…", "progress");
    let downloaded = 0;
    let total = 0;
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          total = event.data.contentLength ?? 0;
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          setProgress(
            total
              ? `正在下载更新… ${Math.floor((downloaded / total) * 100)}% (${formatMB(downloaded)} / ${formatMB(total)})`
              : `正在下载更新… ${formatMB(downloaded)}`,
          );
          break;
        case "Finished":
          setProgress("下载完成，正在安装…");
          break;
      }
    });
    setProgress("安装完成，正在重启…");
    await relaunch();
  } catch (e) {
    setProgress(`检查更新失败：${e}`, "error");
  } finally {
    updateState.busy = false;
  }
}
