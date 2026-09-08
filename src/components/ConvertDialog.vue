<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from "vue";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useViewerStore } from "@/stores/viewer";
import { t, type MessageKey } from "@/i18n";
import Select from "@/components/Select.vue";
import Checkbox from "@/components/Checkbox.vue";

type Rotation = "exif" | "ccw90" | "cw90" | "rot180";
type ResizeMode = "none" | "contain" | "fit-width" | "pad" | "crop" | "stretch";
type Format = "avif" | "webp" | "jpg" | "png" | "bmp";
type OutMode = "original" | "pictures" | "custom";

interface PreviewResult {
  path: string;
  width: number;
  height: number;
}

const emit = defineEmits<{ close: [] }>();
// standalone：作为独立转换窗口时铺满整个窗口（而非主窗口内的浮层）
const props = defineProps<{ standalone?: boolean }>();
const store = useViewerStore();

const ROTATIONS: { value: Rotation; labelKey: MessageKey }[] = [
  { value: "exif", labelKey: "convert.rotExif" },
  { value: "ccw90", labelKey: "convert.rotCcw" },
  { value: "cw90", labelKey: "convert.rotCw" },
  { value: "rot180", labelKey: "convert.rot180" },
];

const rotation = ref<Rotation>("exif");
const resizeMode = ref<ResizeMode>("none");
const width = ref<number | null>(null);
const height = ref<number | null>(null);
const padColor = ref("#ffffff");
const format = ref<Format>("avif");
const lossless = ref(false);
const quality = ref<string | number>(80);
const outMode = ref<OutMode>("original");
const customDir = ref("");
const prefix = ref("hive_");

const busy = ref(false);
const errorMsg = ref("");
// standalone 下最近一次转换成功的输出路径（留在窗口里展示，由用户手动关闭）
const savedPath = ref("");

const file = computed(() => store.currentFile);
const stem = computed(() => (file.value ? file.value.name.replace(/\.[^.]+$/, "") : ""));
const filenamePreview = computed(() => `${prefix.value}${stem.value}.${format.value}`);

const resizeOptions = computed<{ value: ResizeMode; label: string }[]>(() => [
  { value: "none", label: t("convert.resizeNone") },
  { value: "contain", label: t("convert.resizeContain") },
  { value: "fit-width", label: t("convert.resizeFitWidth") },
  { value: "pad", label: t("convert.resizePad") },
  { value: "crop", label: t("convert.resizeCrop") },
  { value: "stretch", label: t("convert.resizeStretch") },
]);
const formatOptions = computed<{ value: Format; label: string }[]>(() =>
  (["avif", "webp", "jpg", "png", "bmp"] as Format[]).map((v) => ({
    value: v,
    label: v.toUpperCase(),
  })),
);
const qualityPresets = [100, 95, 90, 85, 80, 70, 60, 50];

const needWidth = computed(() => resizeMode.value !== "none");
const needHeight = computed(() => ["contain", "pad", "crop", "stretch"].includes(resizeMode.value));

// 进入需要尺寸的模式时用原图尺寸预填
watch(resizeMode, (m) => {
  if (m === "none" || !file.value) {
    return;
  }
  const s = store.sizeCache[file.value.path];
  if (s && s.w > 0) {
    if (!width.value) {
      width.value = s.w;
    }
    if (needHeight.value && !height.value) {
      height.value = s.h;
    }
  }
});

// 无损：仅 webp 可选；png/bmp 恒无损；jpg/avif 不支持（ravif 无真无损）
const losslessMeta = computed(() => {
  if (format.value === "webp") {
    return { editable: true, value: lossless.value };
  }
  if (format.value === "png" || format.value === "bmp") {
    return { editable: false, value: true };
  }
  return { editable: false, value: false };
});
const qualityEnabled = computed(
  () => format.value !== "png" && format.value !== "bmp" && !losslessMeta.value.value,
);

function payload() {
  const q = Math.min(100, Math.max(1, Number(quality.value) || 80));
  return {
    rotation: rotation.value,
    resizeMode: resizeMode.value,
    width: width.value && width.value > 0 ? Math.round(width.value) : undefined,
    height: height.value && height.value > 0 ? Math.round(height.value) : undefined,
    padColor: padColor.value,
    format: format.value,
    lossless: losslessMeta.value.value,
    quality: q,
    prefix: prefix.value,
  };
}

// ---- 预览：仅几何（旋转/裁剪）影响画面，格式/品质不触发重算 ----
// 初始用 store.dataUri（webview 已解码的当前图）做 0 延迟占位，后端代理就绪后无缝替换
const previewSrc = ref(store.dataUri || "");
const previewDims = ref("");
const previewLoading = ref(false);
let timer: number | null = null;
let seq = 0;

async function refreshPreview() {
  if (!file.value) {
    return;
  }
  const my = ++seq;
  previewLoading.value = true;
  try {
    const r = await invoke<PreviewResult>("preview_convert", {
      path: file.value.path,
      options: payload(),
    });
    if (my !== seq) {
      return;
    }
    previewSrc.value = convertFileSrc(r.path);
    previewDims.value = `${r.width} × ${r.height}`;
  } catch (e) {
    if (my !== seq) {
      return;
    }
    previewSrc.value = "";
    previewDims.value = "";
    errorMsg.value = String(e);
  } finally {
    if (my === seq) {
      previewLoading.value = false;
    }
  }
}

watch(
  [rotation, resizeMode, width, height, padColor],
  () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = window.setTimeout(refreshPreview, 120);
  },
  { immediate: true },
);

onUnmounted(() => {
  seq++;
  if (timer !== null) {
    clearTimeout(timer);
  }
});

async function browse() {
  try {
    const dir = await invoke<string | null>("pick_folder");
    if (dir) {
      customDir.value = dir;
    }
  } catch (e) {
    errorMsg.value = String(e);
  }
}

async function doConvert() {
  if (!file.value || busy.value) {
    return;
  }
  errorMsg.value = "";
  savedPath.value = "";
  if (
    needWidth.value &&
    (!width.value || width.value < 1 || (needHeight.value && (!height.value || height.value < 1)))
  ) {
    errorMsg.value = t("convert.invalidSize");
    return;
  }
  busy.value = true;
  try {
    let dir = customDir.value;
    if (outMode.value === "original") {
      dir = file.value.path.replace(/[\\/][^\\/]*$/, "");
    } else if (outMode.value === "pictures") {
      dir = await invoke<string>("pictures_dir");
    } else if (!dir) {
      errorMsg.value = t("convert.needDir");
      return;
    }
    const saved = await invoke<string>("convert_image", {
      path: file.value.path,
      options: payload(),
      outputDir: dir,
    });
    store.showToast(t("convert.saved"));
    if (props.standalone) {
      // 独立转换窗口：留在原地展示成功结果，不自动关闭
      savedPath.value = saved;
    } else {
      emit("close");
      // 刷新文件列表并跳到新图
      store.openImageByPath(saved);
    }
  } catch (e) {
    errorMsg.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div
    class="convert-overlay"
    :class="{ standalone: props.standalone }"
    @click.self="emit('close')"
  >
    <div class="convert-dialog" @keydown.esc.stop="emit('close')">
      <header class="cd-header">
        <span class="cd-title">{{ t("convert.title") }}</span>
        <button
          type="button"
          class="cd-close"
          :aria-label="t('settings.close')"
          @click="emit('close')"
        >
          ×
        </button>
      </header>

      <div class="cd-body">
        <div class="cd-settings">
          <!-- 1. 旋转 -->
          <section class="cd-module">
            <h3>{{ t("convert.rotation") }}</h3>
            <label v-for="r in ROTATIONS" :key="r.value" class="cd-radio">
              <input type="radio" v-model="rotation" :value="r.value" />
              <span>{{ t(r.labelKey) }}</span>
            </label>
          </section>

          <!-- 2. 裁剪 -->
          <section class="cd-module">
            <h3>{{ t("convert.resize") }}</h3>
            <Select v-model="resizeMode" :options="resizeOptions" />
            <div v-if="needWidth" class="cd-dims">
              <label>
                <span>{{ t("convert.width") }}</span>
                <input class="cd-input" type="number" min="1" v-model.number="width" />
              </label>
              <label v-if="needHeight">
                <span>{{ t("convert.height") }}</span>
                <input class="cd-input" type="number" min="1" v-model.number="height" />
              </label>
            </div>
            <div v-if="resizeMode === 'pad'" class="cd-dims">
              <label>
                <span>{{ t("convert.padColor") }}</span>
                <input class="cd-color" type="color" v-model="padColor" />
              </label>
            </div>
          </section>

          <!-- 3. 格式设定 -->
          <section class="cd-module">
            <h3>{{ t("convert.format") }}</h3>
            <Select v-model="format" :options="formatOptions" />
            <div class="cd-row">
              <Checkbox v-model="lossless" :disabled="!losslessMeta.editable">{{
                t("convert.lossless")
              }}</Checkbox>
            </div>
            <div class="cd-row">
              <span class="cd-label">{{ t("convert.quality") }}</span>
              <input
                class="cd-input cd-quality"
                type="text"
                inputmode="numeric"
                list="convert-quality-presets"
                v-model="quality"
                :disabled="!qualityEnabled"
              />
              <datalist id="convert-quality-presets">
                <option v-for="q in qualityPresets" :key="q" :value="q" />
              </datalist>
            </div>
          </section>

          <!-- 4. 输出文件夹 -->
          <section class="cd-module">
            <h3>{{ t("convert.output") }}</h3>
            <label class="cd-radio">
              <input type="radio" v-model="outMode" value="original" />
              <span>{{ t("convert.outOriginal") }}</span>
            </label>
            <label class="cd-radio">
              <input type="radio" v-model="outMode" value="pictures" />
              <span>{{ t("convert.outPictures") }}</span>
            </label>
            <label class="cd-radio">
              <input type="radio" v-model="outMode" value="custom" />
              <span>{{ t("convert.outCustom") }}</span>
              <button
                type="button"
                class="cd-browse"
                :disabled="outMode !== 'custom'"
                @click.prevent.stop="browse"
              >
                {{ t("convert.browse") }}
              </button>
            </label>
            <div v-if="outMode === 'custom' && customDir" class="cd-dir" :title="customDir">
              {{ customDir }}
            </div>
          </section>

          <!-- 文件名前缀 + 预览 -->
          <section class="cd-module">
            <h3>{{ t("convert.prefix") }}</h3>
            <input class="cd-input" type="text" v-model="prefix" />
            <div class="cd-filename">
              <span class="cd-note">{{ t("convert.filenamePreview") }}</span>
              <span class="cd-filename-value" :title="filenamePreview">{{ filenamePreview }}</span>
            </div>
          </section>

          <div v-if="errorMsg" class="cd-error">{{ errorMsg }}</div>

          <div v-if="savedPath" class="cd-success">
            <span class="cd-success-label">✓ {{ t("convert.saved") }}</span>
            <span class="cd-success-path" :title="savedPath">{{ savedPath }}</span>
          </div>

          <button type="button" class="cd-start" :disabled="busy" @click="doConvert">
            {{ busy ? t("convert.converting") : t("convert.start") }}
          </button>
        </div>

        <div class="cd-preview">
          <div class="cd-preview-box">
            <img v-if="previewSrc" :src="previewSrc" alt="" />
            <span v-if="previewLoading" class="cd-preview-hint">{{ t("convert.loading") }}</span>
          </div>
          <div class="cd-preview-meta">
            <span v-if="previewDims">{{ t("convert.outputSize") }}: {{ previewDims }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.convert-overlay {
  position: fixed;
  inset: 0;
  z-index: 30;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(0 0 0 / 55%);
  animation: cd-fade 150ms ease-out;
}

.convert-overlay.standalone {
  background: transparent;
  animation: none;
}

.convert-overlay.standalone .convert-dialog {
  width: 100%;
  height: 100%;
  border: none;
  border-radius: 0;
  box-shadow: none;
  animation: none;
}

.convert-dialog {
  display: flex;
  flex-direction: column;
  width: min(1280px, 96vw);
  height: min(860px, 94vh);
  overflow: hidden;
  background: var(--surface-solid);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  animation: cd-pop 180ms ease-out;
}

@keyframes cd-fade {
  from {
    opacity: 0;
  }
}

@keyframes cd-pop {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.98);
  }
}

.cd-header {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-alpha);
}

.cd-title {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--fg);
}

.cd-close {
  padding: 4px 8px;
  font-size: 1.2rem;
  line-height: 1;
  color: var(--fg-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  transition: opacity 150ms;
}

.cd-close:hover {
  opacity: 0.6;
}

.cd-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.cd-settings {
  display: flex;
  flex-shrink: 0;
  flex-direction: column;
  gap: 16px;
  width: 360px;
  padding: 14px 16px;
  overflow-y: auto;
  border-right: 1px solid var(--border-alpha);
}

.cd-module h3 {
  margin: 0 0 8px;
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--fg-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.cd-radio {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 3px 0;
  font-size: 0.85rem;
  color: var(--fg);
  cursor: pointer;
}

.cd-radio input {
  margin: 0;
  accent-color: var(--accent);
}

.cd-dims {
  display: flex;
  gap: 10px;
  margin-top: 8px;
}

.cd-dims label {
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 0.82rem;
  color: var(--fg-muted);
}

.cd-dims .cd-input {
  width: 84px;
}

.cd-input {
  box-sizing: border-box;
  width: 100%;
  padding: 6px 10px;
  font-family: var(--font);
  font-size: 0.82rem;
  color: var(--fg);
  background: var(--surface-strong);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-sm);
}

.cd-input:disabled {
  opacity: 0.45;
}

.cd-color {
  width: 44px;
  height: 28px;
  padding: 0;
  cursor: pointer;
  background: var(--surface-strong);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-sm);
}

.cd-row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 8px;
}

.cd-label {
  font-size: 0.82rem;
  color: var(--fg-muted);
}

.cd-quality {
  flex-shrink: 0;
  width: 90px;
}

.cd-note {
  font-size: 0.75rem;
  color: var(--fg-muted);
}

.cd-browse {
  padding: 3px 10px;
  margin-left: auto;
  font-family: var(--font);
  font-size: 0.78rem;
  color: var(--fg);
  cursor: pointer;
  background: var(--surface-strong);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-sm);
  transition: opacity 150ms;
}

.cd-browse:hover:not(:disabled) {
  opacity: 0.7;
}

.cd-browse:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.cd-dir {
  margin-top: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 0.75rem;
  color: var(--fg-muted);
  white-space: nowrap;
}

.cd-filename {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-top: 8px;
}

.cd-filename-value {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 0.82rem;
  color: var(--accent);
  white-space: nowrap;
}

.cd-error {
  padding: 8px 10px;
  font-size: 0.8rem;
  color: var(--accent);
  word-break: break-all;
  background: var(--accent-soft);
  border-radius: var(--radius-sm);
}

.cd-success {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  font-size: 0.8rem;
  color: var(--accent);
  background: var(--accent-soft);
  border-radius: var(--radius-sm);
}

.cd-success-label {
  font-weight: 600;
}

.cd-success-path {
  color: var(--fg-muted);
  word-break: break-all;
}

.cd-start {
  padding: 9px 0;
  margin-top: auto;
  font-family: var(--font);
  font-size: 0.88rem;
  font-weight: 600;
  color: #fff;
  cursor: pointer;
  background: var(--accent);
  border: none;
  border-radius: var(--radius-md);
  transition: opacity 150ms;
}

.cd-start:hover:not(:disabled) {
  opacity: 0.85;
}

.cd-start:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.cd-preview {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  padding: 14px 16px;
}

.cd-preview-box {
  position: relative;
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  min-height: 0;
  overflow: hidden;
  background: var(--bg);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-md);
}

.cd-preview-box img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}

.cd-preview-hint {
  position: absolute;
  bottom: 10px;
  left: 50%;
  padding: 3px 12px;
  font-size: 0.78rem;
  color: var(--fg-muted);
  background: var(--surface-solid);
  border: 1px solid var(--border-alpha);
  border-radius: 999px;
  transform: translateX(-50%);
}

.cd-preview-meta {
  min-height: 1.2em;
  font-size: 0.8rem;
  color: var(--fg-muted);
}

/* Select 组件撑满设置栏宽度 */

.cd-module :deep(.select) {
  width: 100%;
}

.cd-module :deep(.select-trigger) {
  box-sizing: border-box;
  justify-content: space-between;
  width: 100%;
}
</style>
