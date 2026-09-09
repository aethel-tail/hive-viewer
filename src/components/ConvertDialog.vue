<script setup lang="ts">
import { ref, computed, watch, toRefs, onUnmounted } from "vue";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import {
  useViewerStore,
  type ConvertFormat,
  type ConvertResizeMode,
  type ConvertRotation,
} from "@/stores/viewer";
import { t, type MessageKey } from "@/i18n";
import Select from "@/components/Select.vue";
import Checkbox from "@/components/Checkbox.vue";

interface PreviewResult {
  path: string;
  width: number;
  height: number;
}

const emit = defineEmits<{ close: [] }>();
// standalone：作为独立转换窗口时铺满整个窗口（而非主窗口内的浮层）
const props = defineProps<{ standalone?: boolean }>();
const store = useViewerStore();

const ROTATIONS: { value: ConvertRotation; labelKey: MessageKey }[] = [
  { value: "exif", labelKey: "convert.rotExif" },
  { value: "ccw90", labelKey: "convert.rotCcw" },
  { value: "cw90", labelKey: "convert.rotCw" },
  { value: "rot180", labelKey: "convert.rot180" },
];

// 转换设置持久化在 store.convertSettings（独立的 convert-settings.json，两窗口共享）
const cs = store.convertSettings;
const {
  rotation,
  resizeMode,
  width,
  height,
  padColor,
  format,
  lossless,
  outMode,
  customDir,
  prefix,
} = toRefs(cs);

// 品质在 store 里是 number，输入框用字符串代理：非法输入不落盘，payload 再兜底 clamp
const quality = computed({
  get: () => String(cs.quality),
  set: (v: string) => {
    const n = Math.round(Number(v));
    if (Number.isFinite(n) && n >= 1 && n <= 100) {
      cs.quality = n;
    }
  },
});

const errorMsg = ref("");
// standalone 下的结果展示：单张 = 输出路径；批量 = 全部成功时的摘要
const savedPath = ref("");
const batchSummary = ref("");
const progress = ref({ done: 0, total: 0 });

// 本批是否仍由面板承载进度/结果：被新批次取代后（store.convertNotice 出现）改走常驻状态条
const panelRunning = computed(() => store.convertBusy && store.convertNotice === null);

// 开始按钮文案：busy 时显示进度（未取到进度则只显示“转换中”），空闲时显示“开始转换”
const startLabel = computed(() => {
  if (!store.convertBusy) {
    return t("convert.start");
  }
  if (panelRunning.value && isBatch.value && progress.value.total > 0) {
    return t("convert.convertingProgress", {
      done: progress.value.done,
      total: progress.value.total,
    });
  }
  return t("convert.converting");
});

// 批量队列优先；否则主窗口/独立窗口里的单张当前图。预览只画第一个目标。
const targets = computed(() =>
  store.convertQueue.length > 0 ? store.convertQueue : store.currentFile ? [store.currentFile] : [],
);
const isBatch = computed(() => targets.value.length > 1);
const file = computed(() => targets.value[0]);
const stem = computed(() => (file.value ? file.value.name.replace(/\.[^.]+$/, "") : ""));
const filenamePreview = computed(() => `${prefix.value}${stem.value}.${format.value}`);

const resizeOptions = computed<{ value: ConvertResizeMode; label: string }[]>(() => [
  { value: "none", label: t("convert.resizeNone") },
  { value: "contain", label: t("convert.resizeContain") },
  { value: "fit-width", label: t("convert.resizeFitWidth") },
  { value: "pad", label: t("convert.resizePad") },
  { value: "crop", label: t("convert.resizeCrop") },
  { value: "stretch", label: t("convert.resizeStretch") },
]);
const formatOptions = computed<{ value: ConvertFormat; label: string }[]>(() =>
  (["avif", "webp", "jpg", "png", "bmp"] as ConvertFormat[]).map((v) => ({
    value: v,
    label: v.toUpperCase(),
  })),
);
const qualityPresets = [100, 95, 90, 85, 80, 70, 60, 50];

const needWidth = computed(() => resizeMode.value !== "none");
const needHeight = computed(() => ["contain", "pad", "crop", "stretch"].includes(resizeMode.value));

// 进入需要尺寸的模式时用第一个目标的原图尺寸预填（批量时取队首）；
// 尺寸可能晚于对话框挂载到达（loadImage 是异步的），所以也监听 sizeCache 的那一项。
watch(
  [
    resizeMode,
    () => file.value?.path,
    () => (file.value ? store.sizeCache[file.value.path] : undefined),
  ],
  () => {
    const m = resizeMode.value;
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
  },
  { immediate: true },
);

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
  // 也监听首个目标的路径：复用转换窗口收到新一批路径时，预览要跟着换图
  [rotation, resizeMode, width, height, padColor, () => file.value?.path],
  () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = window.setTimeout(refreshPreview, 120);
  },
  { immediate: true },
);

// 换批/换图时的清理：busy 时不能静默返回，也不能清面板（旧批还在往这里写）——
// 递增 store.convertRunId 作废旧批的 UI 写入，改用常驻状态条承载旧批进度/结果。
// 放在预览 watch 之后：两者同时被换图触发，这里同步清空，refreshPreview 的 120ms 防抖
// 之后才写 errorMsg，所以新目标自己的预览错误不会被吃掉。
// 监听 convertQueue 的数组身份：openConvertBatch 每批都赋一个新数组，所以首路径和张数
// 都相同的新批次（{a,b} → {a,c}）也能命中；首个目标路径覆盖主窗口换图的单张场景。
watch([() => store.convertQueue, () => file.value?.path], () => {
  if (store.convertBusy) {
    store.convertRunId++;
    // 同时作废正在跑的预览（120ms 防抖内可能返回旧的错误，不能落到新批次的面板里）
    seq++;
    store.convertNotice = {
      text: t("convert.pendingBatch", {
        done: progress.value.done,
        total: progress.value.total,
      }),
      tone: "info",
    };
    return;
  }
  savedPath.value = "";
  batchSummary.value = "";
  errorMsg.value = "";
});

onUnmounted(() => {
  seq++;
  // 对话框卸载（主窗口关闭/切图）时作废旧批的 UI 写入：旧批继续在后台跑完，
  // 进度/结果改走 store.convertNotice；convertBusy 由 doConvert 的 finally 清除。
  store.convertRunId++;
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

// 关闭确认：转换进行中弹一次原生确认。
// 主窗口对话框关闭后旧批继续在后台跑完；独立转换窗口关闭会销毁 webview、中止剩余转换，
// 因此两者用不同措辞（standalone 用“关闭窗口会中止剩余转换”）。
async function requestClose() {
  const key = props.standalone ? "convert.closeWhileBusyStandalone" : "convert.closeWhileBusy";
  if (await store.confirmConvertClose(key)) {
    emit("close");
  }
}

async function doConvert() {
  if (!file.value || store.convertBusy) {
    return;
  }
  // 批次序号：所有 await 之后的 UI 写入都先核对它；被新批次取代的旧批只写常驻状态条
  const runId = ++store.convertRunId;
  store.convertNotice = null;
  errorMsg.value = "";
  savedPath.value = "";
  batchSummary.value = "";
  if (
    needWidth.value &&
    (!width.value || width.value < 1 || (needHeight.value && (!height.value || height.value < 1)))
  ) {
    errorMsg.value = t("convert.invalidSize");
    return;
  }
  // 快照队列与总数：转换期间换批不影响正在跑的这一批的进度/计数
  const list = targets.value.slice();
  const runTotal = list.length;
  if (runTotal === 0) {
    return;
  }
  // 快照：批量转换期间改设置不应影响正在跑的这一批（尤其 outMode 决定输出目录）
  const opts = payload();
  const outMode = cs.outMode;
  const customDirPath = customDir.value;

  // 第一个 await 前同步置位（store 级）：主窗口对话框卸载再打开也不会误判空闲、起第二个并发批次
  store.convertBusy = true;
  progress.value = { done: 0, total: runTotal };
  const failures: { name: string; msg: string }[] = [];
  let ok = 0;
  let done = 0;
  let lastSaved = "";
  try {
    // 输出目录只解析一次：original 每张各自的目录，pictures/custom 共享
    let sharedDir = "";
    if (outMode === "pictures") {
      sharedDir = await invoke<string>("pictures_dir");
    } else if (outMode === "custom") {
      if (!customDirPath) {
        errorMsg.value = t("convert.needDir");
        return;
      }
      sharedDir = customDirPath;
    }

    // 顺序转换，单张失败不中断整批
    for (const item of list) {
      try {
        const dir = outMode === "original" ? item.path.replace(/[\\/][^\\/]*$/, "") : sharedDir;
        const saved = await invoke<string>("convert_image", {
          path: item.path,
          options: opts,
          outputDir: dir,
        });
        ok += 1;
        lastSaved = saved;
      } catch (e) {
        failures.push({ name: item.name, msg: String(e) });
      } finally {
        done += 1;
        if (runId === store.convertRunId) {
          progress.value = { done, total: runTotal };
        } else {
          // 已被新批次取代：进度改走常驻状态条，不再写面板
          store.convertNotice = {
            text: t("convert.pendingBatch", { done, total: runTotal }),
            tone: "info",
          };
        }
      }
    }

    // 被取代的旧批跑完：汇总（含最多 5 条失败明细）留在状态条上，
    // 不写 savedPath / batchSummary / errorMsg，也不弹 toast / 关对话框。
    if (runId !== store.convertRunId) {
      const summary = t("convert.batchSummary", { ok, fail: failures.length });
      // 失败明细最多列前 5 条，其余折成一行，避免批量失败时状态条无限拉长
      const MAX_FAILURES = 5;
      const lines = failures
        .slice(0, MAX_FAILURES)
        .map((f) => t("convert.batchFailedItem", { name: f.name, msg: f.msg }));
      if (failures.length > MAX_FAILURES) {
        lines.push(t("convert.batchMoreFailures", { n: failures.length - MAX_FAILURES }));
      }
      store.convertNotice = {
        text: failures.length > 0 ? [summary, ...lines].join("\n") : summary,
        tone: failures.length > 0 ? "error" : "info",
      };
      return;
    }

    if (list.length === 1) {
      // 单张：保持原有行为
      if (failures.length > 0) {
        errorMsg.value = failures[0].msg;
        return;
      }
      store.showToast(t("convert.saved"));
      if (props.standalone) {
        // 独立转换窗口：留在原地展示成功结果，不自动关闭
        savedPath.value = lastSaved;
      } else {
        emit("close");
        // 刷新文件列表并跳到新图
        store.openImageByPath(lastSaved);
      }
      return;
    }

    // 批量：成功才进 success 框；有失败时汇总+失败明细走 error 区，避免 ✓ 与失败并存
    const summary = t("convert.batchSummary", { ok, fail: failures.length });
    if (failures.length === 0) {
      store.showToast(t("convert.batchSaved", { ok }));
      if (props.standalone) {
        batchSummary.value = summary;
      }
    } else {
      store.showToast(summary);
      // 失败明细最多列前 5 条，其余折成一行，避免批量失败时面板无限拉长
      const MAX_FAILURES = 5;
      const lines = failures
        .slice(0, MAX_FAILURES)
        .map((f) => t("convert.batchFailedItem", { name: f.name, msg: f.msg }));
      if (failures.length > MAX_FAILURES) {
        lines.push(t("convert.batchMoreFailures", { n: failures.length - MAX_FAILURES }));
      }
      errorMsg.value = [summary, ...lines].join("\n");
    }
  } catch (e) {
    if (runId === store.convertRunId) {
      errorMsg.value = String(e);
    } else {
      store.convertNotice = { text: String(e), tone: "error" };
    }
  } finally {
    store.convertBusy = false;
  }
}
</script>

<template>
  <div class="convert-overlay" :class="{ standalone: props.standalone }" @click.self="requestClose">
    <div class="convert-dialog" @keydown.esc.stop="requestClose">
      <header v-if="!props.standalone" class="cd-header">
        <span class="cd-title">{{ t("convert.title") }}</span>
        <button
          type="button"
          class="cd-close"
          :aria-label="t('settings.close')"
          @click="requestClose"
        >
          ×
        </button>
      </header>

      <!-- 常驻状态条：被新批次取代的旧批进度/结果（不随 toast 消失，可手动关闭） -->
      <div
        v-if="store.convertNotice"
        class="cd-notice"
        :class="`cd-notice-${store.convertNotice.tone}`"
      >
        <span class="cd-notice-text">{{ store.convertNotice.text }}</span>
        <button
          type="button"
          class="cd-notice-close"
          :aria-label="t('settings.close')"
          @click="store.convertNotice = null"
        >
          ×
        </button>
      </div>

      <div class="cd-body">
        <div class="cd-settings">
          <div v-if="isBatch" class="cd-batch">
            <span class="cd-batch-count">{{ t("convert.batchCount", { n: targets.length }) }}</span>
            <span class="cd-note">{{ t("convert.batchNote") }}</span>
          </div>

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

          <div v-if="panelRunning && isBatch" class="cd-progress">
            {{ t("convert.convertingProgress", { done: progress.done, total: progress.total }) }}
          </div>

          <div v-if="errorMsg" class="cd-error">{{ errorMsg }}</div>

          <div v-if="savedPath" class="cd-success">
            <span class="cd-success-label">✓ {{ t("convert.saved") }}</span>
            <span class="cd-success-path" :title="savedPath">{{ savedPath }}</span>
          </div>
          <div v-else-if="batchSummary" class="cd-success">
            <span class="cd-success-label">✓ {{ batchSummary }}</span>
          </div>

          <button type="button" class="cd-start" :disabled="store.convertBusy" @click="doConvert">
            {{ startLabel }}
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

.cd-notice {
  display: flex;
  flex-shrink: 0;
  gap: 10px;
  align-items: flex-start;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-alpha);
}

.cd-notice-info {
  color: var(--accent);
  background: var(--accent-soft);
}

.cd-notice-error {
  color: var(--danger);
  background: var(--danger-soft);
}

.cd-notice-text {
  flex: 1;
  min-width: 0;
  font-size: 0.8rem;
  line-height: 1.5;
  word-break: break-all;
  white-space: pre-line;
}

.cd-notice-close {
  flex-shrink: 0;
  padding: 0 4px;
  font-size: 1rem;
  line-height: 1;
  color: inherit;
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  transition: opacity 150ms;
}

.cd-notice-close:hover {
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
  white-space: pre-line;
  background: var(--accent-soft);
  border-radius: var(--radius-sm);
}

.cd-batch {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.cd-batch-count {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--fg);
}

.cd-progress {
  font-size: 0.8rem;
  color: var(--accent);
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
