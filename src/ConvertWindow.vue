<script setup lang="ts">
import { watch, onMounted, onUnmounted } from "vue";
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

// 窗口标题：转换格式 - 文件名（未取到路径时只显示标题）
watch(
  () => [store.currentFile, locale.value] as const,
  async () => {
    try {
      const name = store.currentFile?.name;
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

  unlistenConvert = await listen<string>("convert-file", (event) => {
    if (event.payload) {
      store.openImageByPath(event.payload);
    }
  });

  // 后端在创建/通知窗口前已把路径写入 pending；挂载时取一次，
  // 兜住「事件早于监听」的竞态。
  const pending = await invoke<string | null>("take_pending_convert");
  if (pending) {
    await store.openImageByPath(pending);
  }
});

onUnmounted(() => {
  window.removeEventListener("contextmenu", onContextMenu);
  window.removeEventListener("keydown", onKeydown);
  unlistenConvert?.();
});
</script>

<template>
  <Message v-if="store.toast.seq > 0" :key="store.toast.seq" :text="store.toast.text" />
  <ConvertDialog v-if="store.currentFile" standalone @close="closeWindow" />
</template>
