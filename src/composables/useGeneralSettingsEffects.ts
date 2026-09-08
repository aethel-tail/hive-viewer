import { watch } from "vue";
import { useViewerStore } from "@/stores/viewer";
import { setLocale } from "@/i18n";

/// 一般设置里「立即生效」的三项：主题 / 语言 / 字体。
/// 主窗口与独立转换窗口各自调用一次（两个 webview 是独立 JS 上下文）。
export function useGeneralSettingsEffects() {
  const store = useViewerStore();

  // 应用主题：data-theme 驱动 global.css 的 token 切换；用 View Transition 做淡入淡出过渡
  let themeFirstApply = true;
  watch(
    () => store.generalSettings.theme,
    (t) => {
      if (themeFirstApply) {
        themeFirstApply = false;
        document.documentElement.dataset.theme = t;
        return;
      }
      const doc = document as Document & { startViewTransition?: (cb: () => void) => void };
      if (doc.startViewTransition) {
        doc.startViewTransition(() => {
          document.documentElement.dataset.theme = t;
        });
      } else {
        document.documentElement.dataset.theme = t;
      }
    },
    { immediate: true },
  );

  // 语言：generalSettings.language 是唯一持久化来源，同步到 i18n 模块的响应式 locale
  watch(
    () => store.generalSettings.language,
    (l) => setLocale(l),
    { immediate: true },
  );

  // 全局字体：非空时把选中字体插到默认链前面覆盖 --font；空值移除内联覆盖，回到 global.css 默认链
  watch(
    () => store.generalSettings.fontFamily,
    (f) => {
      document.documentElement.style.setProperty("--font", f ? `"${f}", var(--font-default)` : "");
    },
    { immediate: true },
  );
}
