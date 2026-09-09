<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useViewerStore } from "@/stores/viewer";
import { t, locale } from "@/i18n";
import { useGeneralSettingsEffects } from "@/composables/useGeneralSettingsEffects";
import { checkForUpdates } from "@/update";
import ImageViewer from "@/components/ImageViewer.vue";
import TopToolbar from "@/components/TopToolbar.vue";
import BottomFloatingBar from "@/components/BottomFloatingBar.vue";
import ExifPanel from "@/components/ExifPanel.vue";
import SettingsPanel from "@/components/SettingsPanel.vue";
import ConvertDialog from "@/components/ConvertDialog.vue";
import Message from "@/components/Message.vue";

const store = useViewerStore();
useGeneralSettingsEffects();
const imageViewerRef = ref<InstanceType<typeof ImageViewer> | null>(null);
const displayZoom = ref(100);
const isFullscreen = ref(false);

// 总在最前：设置变化立即生效，启动时应用持久化值
watch(
  () => store.generalSettings.alwaysOnTop,
  async (v) => {
    try {
      const win = getCurrentWebviewWindow();
      await win.setAlwaysOnTop(v);
    } catch (e) {
      console.error("Set always-on-top failed:", e);
    }
  },
  { immediate: true },
);

const exifData = ref<Record<string, string>>({});
const exifError = ref("");
const settingsVisible = ref(false);
const convertVisible = ref(false);

// 自动获取更新（默认关闭）：设置加载完成或用户在设置里打开时，本会话只跑一次。
// 后台静默执行，失败无感；有新版本才弹可点击的轻提示。
let updateChecked = false;
watch(
  () => store.generalSettings.autoCheckUpdates,
  (on) => {
    if (!on || updateChecked) {
      return;
    }
    updateChecked = true;
    // 延后一点，让首屏加载先跑完（启动扫描目录时不要争网络/主线程）
    setTimeout(() => void checkForUpdates(), 2500);
  },
  { immediate: true },
);

async function loadExif() {
  if (!store.currentFile) {
    return;
  }
  try {
    exifError.value = "";
    const result = await invoke<Record<string, string>>("read_exif", {
      path: store.currentFile.path,
    });
    exifData.value = result;
  } catch (e) {
    exifError.value = String(e);
    exifData.value = { [t("exif.error")]: String(e) };
  }
}

async function onViewExif() {
  if (!store.exifPanelVisible) {
    await loadExif();
  }
  store.exifPanelVisible = !store.exifPanelVisible;
}

async function toggleFullscreen() {
  try {
    const win = getCurrentWebviewWindow();
    const next = !isFullscreen.value;
    await win.setFullscreen(next);
    isFullscreen.value = next;
  } catch (e) {
    console.error("Toggle fullscreen failed:", e);
  }
}

async function exitFullscreen() {
  if (!isFullscreen.value) {
    return;
  }
  try {
    const win = getCurrentWebviewWindow();
    await win.setFullscreen(false);
    isFullscreen.value = false;
  } catch (e) {
    console.error("Exit fullscreen failed:", e);
  }
}

// escToExit：关窗即退出（最后一个窗口关闭时应用默认退出）
async function exitApp() {
  closingApp = true;
  try {
    await getCurrentWebviewWindow().close();
  } catch (e) {
    console.error("Close failed:", e);
  }
}

// 全局水波纹：事件委托，按下时在 <button> 内插入 .ripple 子元素。
// 动画仅 transform/opacity（合成器），元素随 animationend 自清理；
// 尊重 prefers-reduced-motion。
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

// 屏蔽 WebView 自带右键菜单（无自定义菜单时右键不应弹出浏览器菜单）
function onContextMenu(e: MouseEvent) {
  e.preventDefault();
}

function spawnRipple(e: PointerEvent) {
  if (reduceMotion.matches || !e.isPrimary) {
    return;
  }
  const btn = (e.target as HTMLElement | null)?.closest<HTMLButtonElement>("button");
  if (!btn || btn.disabled) {
    return;
  }
  const rect = btn.getBoundingClientRect();
  const size = Math.max(rect.width, rect.height) * 2;
  const ripple = document.createElement("span");
  ripple.className = "ripple";
  ripple.style.setProperty("--ripple-size", `${size}px`);
  ripple.style.setProperty("--ripple-x", `${e.clientX - rect.left}px`);
  ripple.style.setProperty("--ripple-y", `${e.clientY - rect.top}px`);
  ripple.addEventListener("animationend", () => ripple.remove());
  btn.appendChild(ripple);
}

let unlistenOpenFile: UnlistenFn | null = null;
let unlistenDragDrop: UnlistenFn | null = null;
let unlistenClose: UnlistenFn | null = null;
// 程序主动关窗时置位，避免 onCloseRequested 再次弹确认
let closingApp = false;

// 拖入图片：Tauri 默认拦截 HTML5 拖放（dragDropEnabled），改用窗口 drag-drop 事件取文件路径
const dragging = ref(false);
const IMAGE_EXTS = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];

function isImagePath(p: string): boolean {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTS.includes(ext);
}

watch(
  // locale 入依赖：未打开文件时切语言，默认标题也要跟着变
  () => [store.currentFile, store.isDual, store.groupIndices, locale.value] as const,
  async () => {
    const win = getCurrentWebviewWindow();
    const f = store.currentFile;
    let title = t("app.name");
    if (f) {
      if (store.isDual && store.groupIndices.length === 2) {
        const a = store.files[store.groupIndices[0]];
        const b = store.files[store.groupIndices[1]];
        title = a && b ? `${a.name}  +  ${b.name}` : f.name;
      } else {
        title = f.name;
      }
    }
    await win.setTitle(title);
  },
);

watch(
  () => store.currentFile,
  async () => {
    if (store.exifPanelVisible) {
      await loadExif();
    }
  },
);

// 匹配快捷键 spec（小写，如 'ctrl+r'、'arrowleft'；'digit' 匹配 1~9 数字键）：修饰键必须精确一致
function matchKey(e: KeyboardEvent, spec: string): boolean {
  const parts = spec.toLowerCase().split("+");
  const key = parts.pop() ?? "";
  const expected = parts.filter((m) => ["ctrl", "alt", "shift"].includes(m));
  const mods = e as unknown as Record<string, boolean>;
  const actual = ["ctrl", "alt", "shift"].filter((m) => mods[m + "Key"]);
  if (actual.length !== expected.length) {
    return false;
  }
  const keyOk = key === "digit" ? /^[1-9]$/.test(e.key) : e.key.toLowerCase() === key;
  return expected.every((m) => mods[m + "Key"]) && keyOk;
}

function onKeydown(e: KeyboardEvent) {
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
    return;
  }

  const sc = store.shortcuts;

  if (matchKey(e, sc.fullscreen) || matchKey(e, sc.fullscreenEnter)) {
    e.preventDefault();
    toggleFullscreen();
    return;
  }

  if (e.key === "Escape") {
    // ConvertDialog 内 input 的 Esc 由组件自身 @keydown.esc.stop 处理；此处兜底焦点在外的情况
    if (convertVisible.value) {
      e.preventDefault();
      // 转换进行中先弹一次确认（关闭后旧批继续跑）；确认通过才关对话框
      void store.confirmConvertClose().then((ok) => {
        if (ok) {
          convertVisible.value = false;
        }
      });
      return;
    }
    if (settingsVisible.value) {
      e.preventDefault();
      settingsVisible.value = false;
      return;
    }
    if (store.slideshowActive) {
      e.preventDefault();
      store.stopSlideshow();
      return;
    }
    if (isFullscreen.value) {
      e.preventDefault();
      exitFullscreen();
      return;
    }
    // EXIF 面板开着时先关面板（避免一按 Esc 就退出）
    if (store.exifPanelVisible) {
      e.preventDefault();
      store.exifPanelVisible = false;
      return;
    }
    // 没有更上层的处理对象时，按 escToExit 设置决定是否退出应用
    if (store.generalSettings.escToExit) {
      e.preventDefault();
      exitApp();
    }
    return;
  }

  if (e.key === "Delete") {
    // 对话框/设置面板打开时忽略；焦点在按钮/下拉上时也忽略（防 Tab 键盘用户误删）
    if (convertVisible.value || settingsVisible.value) {
      return;
    }
    if (e.target instanceof HTMLButtonElement || e.target instanceof HTMLSelectElement) {
      return;
    }
    e.preventDefault();
    store.deleteCurrent();
    return;
  }

  if (matchKey(e, sc.slideshowSpeed)) {
    e.preventDefault();
    const n = parseInt(e.key, 10);
    if (n >= 1 && n <= 9) {
      store.setSlideshowInterval(n);
      store.startSlideshow();
    }
    return;
  }

  if (matchKey(e, sc.stopSlideshow)) {
    e.preventDefault();
    store.stopSlideshow();
    return;
  }

  if (matchKey(e, sc.rotateCcw)) {
    e.preventDefault();
    imageViewerRef.value?.rotateCounterClockwise();
    return;
  }

  if (matchKey(e, sc.rotateCw)) {
    e.preventDefault();
    imageViewerRef.value?.rotateClockwise();
    return;
  }

  if (matchKey(e, sc.toggleExif)) {
    e.preventDefault();
    onViewExif();
    return;
  }

  if (matchKey(e, sc.prev)) {
    e.preventDefault();
    store.goPrev();
    return;
  }
  if (matchKey(e, sc.next)) {
    e.preventDefault();
    store.goNext();
  }
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown);
  // 捕获阶段：先于任何 stopPropagation 的按钮 handler 插入涟漪
  window.addEventListener("pointerdown", spawnRipple, true);
  window.addEventListener("contextmenu", onContextMenu);

  unlistenOpenFile = await listen<string>("open-file", (event) => {
    if (event.payload) {
      store.openImageByPath(event.payload);
    }
  });

  try {
    unlistenDragDrop = await getCurrentWebviewWindow().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") {
        dragging.value = true;
      } else if (p.type === "leave") {
        dragging.value = false;
      } else {
        dragging.value = false;
        const file = p.paths.find(isImagePath);
        if (file) {
          store.openImageByPath(file);
        }
      }
    });
  } catch (e) {
    console.error("Register drag-drop listener failed:", e);
  }

  const initialFile = await invoke<string | null>("get_initial_file");
  if (initialFile) {
    await store.openImageByPath(initialFile);
    if (store.exifPanelVisible) {
      await loadExif();
    }
  }

  try {
    const win = getCurrentWebviewWindow();
    isFullscreen.value = await win.isFullscreen();
  } catch {}

  // 原生标题栏 X / Alt+F4：转换进行中时确认（关窗即退出，会中止剩余转换）
  unlistenClose = await getCurrentWebviewWindow().onCloseRequested((event) => {
    if (closingApp || !store.convertBusy) {
      return;
    }
    event.preventDefault();
    void (async () => {
      if (await store.confirmConvertClose("convert.closeWhileBusyStandalone")) {
        await exitApp();
      }
    })();
  });
});

onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("pointerdown", spawnRipple, true);
  window.removeEventListener("contextmenu", onContextMenu);
  unlistenOpenFile?.();
  unlistenDragDrop?.();
  unlistenClose?.();
});
</script>

<template>
  <ImageViewer ref="imageViewerRef" @update:displayZoom="displayZoom = $event" />

  <div class="ui-layer">
    <Transition name="drop-hint">
      <div v-if="dragging" class="drop-hint">
        <div class="drop-hint-card">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <circle cx="8.5" cy="8.5" r="1.5" />
            <path d="m21 15-5-5L5 21" />
          </svg>
          {{ t("drop.hint") }}
        </div>
      </div>
    </Transition>
    <Message
      v-if="store.toast.seq > 0"
      :key="store.toast.seq"
      :text="store.toast.text"
      :action="store.toast.action"
      :duration="store.toast.duration"
    />
    <ExifPanel :data="exifData" :visible="store.exifPanelVisible" />

    <SettingsPanel :visible="settingsVisible" @close="settingsVisible = false" />

    <ConvertDialog v-if="convertVisible && store.currentFile" @close="convertVisible = false" />

    <TopToolbar
      :isFullscreen="isFullscreen"
      @toggleFullscreen="toggleFullscreen"
      @viewExif="onViewExif"
      @openSettings="settingsVisible = true"
      @openConvert="convertVisible = true"
    />

    <div style="flex: 1; pointer-events: none"></div>

    <div style="position: relative; flex: 1; pointer-events: none" v-if="store.dataUri">
      <BottomFloatingBar
        :displayZoom="displayZoom"
        @openFile="store.openFile"
        @openFolder="store.openFolder"
        @zoomIn="imageViewerRef?.zoomIn()"
        @zoomOut="imageViewerRef?.zoomOut()"
        @rotateClockwise="imageViewerRef?.rotateClockwise()"
        @rotateCounterClockwise="imageViewerRef?.rotateCounterClockwise()"
        @deleteFile="store.deleteCurrent"
      />
    </div>
  </div>
</template>

<style scoped>
.ui-layer {
  position: fixed;
  inset: 0;
  z-index: 1;
  display: flex;
  flex-direction: column;
  pointer-events: none;
}

.ui-layer > * {
  pointer-events: auto;
}

/* 拖入提示：静态底色 + opacity 过渡（GPU），不拦截拖放事件 */

.drop-hint {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
  background: rgb(20 12 4 / 28%);
}

.drop-hint-card {
  display: flex;
  gap: 12px;
  align-items: center;
  padding: 20px 36px;
  font-family: var(--font);
  font-size: 1rem;
  font-weight: 600;
  color: var(--fg);
  background: var(--surface-solid);
  border: 2px dashed var(--accent);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}

.drop-hint-card svg {
  width: 22px;
  height: 22px;
  color: var(--accent);
}

.drop-hint-enter-active,
.drop-hint-leave-active {
  transition: opacity 150ms ease-out;
}

.drop-hint-enter-from,
.drop-hint-leave-to {
  opacity: 0;
}
</style>
