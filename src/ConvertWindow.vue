<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useViewerStore } from "@/stores/viewer";
import { t, locale } from "@/i18n";
import { useGeneralSettingsEffects } from "@/composables/useGeneralSettingsEffects";
import ConvertDialog from "@/components/ConvertDialog.vue";
import Message from "@/components/Message.vue";

const store = useViewerStore();
useGeneralSettingsEffects();

let unlistenConvert: UnlistenFn | null = null;
let unlistenClose: UnlistenFn | null = null;
// 程序主动关闭时置位，避免 onCloseRequested 再次弹确认
let closing = false;

// 空路径列表 = 后端「转换调用但没解析出路径」：`--convert` 没有有效图片路径，
// 或 `--convert-list` 的列表文件丢失/不可读。后端为此专门打开本窗口，
// 这里必须显式报错，而不是静默留一个空窗口。
const listError = ref(false);
// 转换进行中收到空 payload：不卸载对话框，只用非破坏性状态条提示（见 acceptBatch）
const listWarning = ref(false);

// 状态条文案：优先用被取代旧批的常驻提示（进度 → 最终汇总+失败明细），
// 否则用简单的 pendingBatch 文本兜底。旧批结束且无提示时隐藏（结果就在下面的面板里）。
const warningText = computed(() => {
  if (store.convertNotice) {
    return store.convertNotice.text;
  }
  // 旧批还没上报第一笔进度时的兜底；不编造 0/N，避免与真实进度矛盾
  return t("convert.converting");
});
const warningVisible = computed(
  () => listWarning.value && (store.convertBusy || store.convertNotice !== null),
);

function onContextMenu(e: MouseEvent) {
  e.preventDefault();
}

function onKeydown(e: KeyboardEvent) {
  // 焦点不在对话框内时（如刚打开窗口）也能 Esc 关闭
  if (e.key === "Escape") {
    closeWindow();
  }
}

// 窗口关闭路径（Esc / 系统关闭）：busy 时先弹一次确认。
// 独立转换窗口关闭会销毁 webview、中止剩余转换，所以用 standalone 措辞。
async function closeWindow() {
  if (!(await store.confirmConvertClose("convert.closeWhileBusyStandalone"))) {
    return;
  }
  await closeNow();
}

// ConvertDialog 的关闭按钮/遮罩/Esc 已自行确认过，这里不再二次弹窗
async function closeNow() {
  closing = true;
  try {
    await getCurrentWebviewWindow().close();
  } catch (e) {
    console.error("Close failed:", e);
  }
}

// 非空批次：清掉错误/警告状态并正常载入（先报错后重试的场景）
async function acceptBatch(paths: string[]) {
  if (paths.length === 0) {
    if (store.convertBusy) {
      // 旧批仍在转换：保持队列与 ConvertDialog 挂载（清队列会把旧批结果写进已卸载的组件），
      // 只置非破坏性状态条；空闲时才回到原来的全屏错误面板。
      listWarning.value = true;
      return;
    }
    // 先清空上一批的队列/文件，标题才会退回纯「转换格式」；
    // 否则复用窗口会在上一批的「- N 张」标题下显示错误面板。
    store.clearConvertBatch();
    listWarning.value = false;
    listError.value = true;
    return;
  }
  listWarning.value = false;
  listError.value = false;
  await store.openConvertBatch(paths);
}

// 窗口标题：转换格式 - 文件名（多选时为张数，未取到路径时只显示标题）
watch(
  () => [store.convertQueue.length, store.currentFile, locale.value] as const,
  async () => {
    try {
      const count = store.convertQueue.length;
      const name = count > 1 ? t("convert.batchTitle", { n: count }) : store.currentFile?.name;
      await getCurrentWebviewWindow().setTitle(
        name ? `${t("convert.title")} - ${name}` : t("convert.title"),
      );
    } catch (e) {
      console.error("Set title failed:", e);
    }
  },
  { immediate: true },
);

onMounted(async () => {
  window.addEventListener("contextmenu", onContextMenu);
  window.addEventListener("keydown", onKeydown);

  // 原生标题栏 X / Alt+F4 也走确认：同步 preventDefault，再异步确认后主动关闭
  unlistenClose = await getCurrentWebviewWindow().onCloseRequested((event) => {
    if (closing) {
      return;
    }
    event.preventDefault();
    void (async () => {
      if (await store.confirmConvertClose("convert.closeWhileBusyStandalone")) {
        await closeNow();
      }
    })();
  });

  unlistenConvert = await listen<string[]>("convert-file", (event) => {
    if (event.payload) {
      acceptBatch(event.payload);
    }
  });

  // 后端在创建/通知窗口前已把路径写入 pending；挂载时取一次，
  // 兜住「事件早于监听」的竞态。空数组同样是错误信号。
  const pending = await invoke<string[] | null>("take_pending_convert");
  if (pending) {
    await acceptBatch(pending);
  }
});

onUnmounted(() => {
  window.removeEventListener("contextmenu", onContextMenu);
  window.removeEventListener("keydown", onKeydown);
  unlistenConvert?.();
  unlistenClose?.();
});
</script>

<template>
  <Message
    v-if="store.toast.seq > 0"
    :key="store.toast.seq"
    :text="store.toast.text"
    :action="store.toast.action"
    :duration="store.toast.duration"
  />
  <div v-if="warningVisible" class="cw-warning">
    <span class="cw-warning-text">{{ warningText }}</span>
  </div>
  <div v-if="listError" class="cw-error">
    <p class="cw-error-text">{{ t("convert.listError") }}</p>
    <button type="button" class="cw-error-close" @click="closeWindow">
      {{ t("settings.close") }}
    </button>
  </div>
  <ConvertDialog
    v-else-if="store.convertQueue.length > 0 || store.currentFile"
    standalone
    :class="{ 'with-warning': warningVisible }"
    @close="closeNow"
  />
</template>

<style scoped>
.cw-warning {
  position: fixed;
  top: 0;
  right: 0;
  left: 0;
  z-index: 40;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  height: 44px;
  padding: 0 14px;
  overflow: hidden;
  color: var(--accent);
  background: var(--accent-soft);
  border-bottom: 1px solid var(--border-alpha);
}

.cw-warning-text {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 0.82rem;
  line-height: 1.4;
  white-space: nowrap;
}

/* 状态条占据顶部 44px，把固定定位的对话框向下让开 */

.convert-overlay.with-warning {
  top: 44px;
}

.cw-error {
  display: flex;
  flex-direction: column;
  gap: 18px;
  align-items: center;
  justify-content: center;
  height: 100vh;
  padding: 24px;
  text-align: center;
}

.cw-error-text {
  max-width: 32em;
  margin: 0;
  font-size: 0.9rem;
  line-height: 1.6;
  color: var(--fg);
}

.cw-error-close {
  padding: 8px 28px;
  font-family: var(--font);
  font-size: 0.86rem;
  font-weight: 600;
  color: #fff;
  cursor: pointer;
  background: var(--accent);
  border: none;
  border-radius: var(--radius-md);
  transition: opacity 150ms;
}

.cw-error-close:hover {
  opacity: 0.85;
}
</style>
