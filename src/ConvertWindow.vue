<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from "vue";
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

// 空路径列表 = 后端「转换调用但没解析出路径」：`--convert` 没有有效图片路径，
// 或 `--convert-list` 的列表文件丢失/不可读。后端为此专门打开本窗口，
// 这里必须显式报错，而不是静默留一个空窗口。
const listError = ref(false);

function onContextMenu(e: MouseEvent) {
  e.preventDefault();
}

function onKeydown(e: KeyboardEvent) {
  // 焦点不在对话框内时（如刚打开窗口）也能 Esc 关闭
  if (e.key === "Escape") {
    closeWindow();
  }
}

async function closeWindow() {
  try {
    await getCurrentWebviewWindow().close();
  } catch (e) {
    console.error("Close failed:", e);
  }
}

// 非空批次：清掉错误状态并正常载入（先报错后重试的场景）
async function acceptBatch(paths: string[]) {
  if (paths.length === 0) {
    // 先清空上一批的队列/文件，标题才会退回纯「转换格式」；
    // 否则复用窗口会在上一批的「- N 张」标题下显示错误面板。
    store.clearConvertBatch();
    listError.value = true;
    return;
  }
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
  <div v-if="listError" class="cw-error">
    <p class="cw-error-text">{{ t("convert.listError") }}</p>
    <button type="button" class="cw-error-close" @click="closeWindow">
      {{ t("settings.close") }}
    </button>
  </div>
  <ConvertDialog
    v-else-if="store.convertQueue.length > 0 || store.currentFile"
    standalone
    @close="closeWindow"
  />
</template>

<style scoped>
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
