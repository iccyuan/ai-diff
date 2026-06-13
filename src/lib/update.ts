import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { confirmDialog } from "./confirm";
import { toast } from "./toast";

/** Check GitHub releases for a newer signed build; prompt then install.
 *  No-op in dev / when offline. `manual` surfaces "already latest" + errors. */
export async function checkForUpdate(manual = false) {
  try {
    const update = await check();
    if (!update) {
      if (manual) toast("已是最新版本");
      return;
    }
    const ok = await confirmDialog(
      `发现新版本 ${update.version}`,
      "是否现在下载并安装？安装完成后应用会自动重启。",
    );
    if (!ok) return;
    toast("正在下载更新…");
    await update.downloadAndInstall();
    await relaunch();
  } catch (e) {
    if (manual) toast(`检查更新失败：${e}`, "error");
  }
}
