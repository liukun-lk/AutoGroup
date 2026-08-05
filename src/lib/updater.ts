import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { ask, message } from "@tauri-apps/plugin-dialog";

/**
 * Check GitHub Releases for a newer version. If found, ask the user,
 * then download, install and relaunch.
 *
 * Silently no-ops in dev mode or when the update endpoint is unreachable,
 * so it is safe to call unconditionally on startup.
 */
export async function checkForUpdates(): Promise<void> {
  let update;
  try {
    update = await check();
  } catch (e) {
    console.warn("update check failed:", e);
    return;
  }
  if (!update) return;

  const confirmed = await ask(
    `发现新版本 v${update.version}（当前 v${update.currentVersion}）。\n是否立即下载并安装？`,
    { title: "版本更新", kind: "info", okLabel: "更新", cancelLabel: "稍后" }
  );
  if (!confirmed) return;

  try {
    await update.downloadAndInstall();
  } catch (e) {
    console.error("update download failed:", e);
    await message("更新下载失败，请稍后重试。", {
      title: "版本更新",
      kind: "error",
    });
    return;
  }

  await message("更新已安装，应用即将重启。", { title: "版本更新" });
  await relaunch();
}
