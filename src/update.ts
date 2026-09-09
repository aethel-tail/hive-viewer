import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { useViewerStore } from "@/stores/viewer";
import { t } from "@/i18n";

// GitHub 最新 Release（api 用于比对版本，页面固定由后端 open_release_page 打开）
const RELEASE_API = "https://api.github.com/repos/aethel-tail/hive-viewer/releases/latest";

// 版本号解析：去掉 v 前缀，核心段按数字比对（1.10 > 1.9），预发布后缀单独保留
function parseVersion(v: string): { nums: number[]; pre: string } {
  const [core, ...rest] = v.trim().replace(/^v/i, "").split("-");
  return { nums: core.split(".").map((n) => parseInt(n, 10) || 0), pre: rest.join("-") };
}

// 返回值：a > b → 1，a < b → -1，相等 → 0；预发布（1.1.0-beta）小于同号正式版。
export function compareVersions(a: string, b: string): number {
  const va = parseVersion(a);
  const vb = parseVersion(b);
  const len = Math.max(va.nums.length, vb.nums.length);
  for (let i = 0; i < len; i++) {
    const d = (va.nums[i] ?? 0) - (vb.nums[i] ?? 0);
    if (d !== 0) {
      return d > 0 ? 1 : -1;
    }
  }
  if (va.pre === vb.pre) {
    return 0;
  }
  if (!va.pre) {
    return 1;
  }
  if (!vb.pre) {
    return -1;
  }
  return va.pre > vb.pre ? 1 : -1;
}

// 后台静默检查更新（设置里「自动获取更新」开启时，每次启动调用一次）。
// 全程不阻塞界面：网络/限流/解析失败都只记日志，绝不弹错；只有确实有更新时才轻提示。
export async function checkForUpdates(): Promise<void> {
  const store = useViewerStore();
  try {
    const [current, res] = await Promise.all([
      getVersion(),
      fetch(RELEASE_API, {
        headers: { Accept: "application/vnd.github+json" },
        signal: AbortSignal.timeout(10000),
      }),
    ]);
    if (!res.ok) {
      return;
    }
    const data = (await res.json()) as { tag_name?: unknown };
    const tag = typeof data.tag_name === "string" ? data.tag_name : "";
    if (!tag || compareVersions(tag, current) <= 0) {
      return;
    }
    // 轻提示：停留 8 秒、可点击，点了用默认浏览器打开最新 Release 页面
    store.showToast(t("update.available", { version: tag.replace(/^v/i, "") }), {
      duration: 8000,
      action: () => {
        invoke("open_release_page").catch((e) => console.error("Open release page failed:", e));
      },
    });
  } catch (e) {
    console.debug("Update check skipped:", e);
  }
}
