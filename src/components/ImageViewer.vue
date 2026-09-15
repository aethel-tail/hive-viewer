<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted, onUnmounted } from "vue";
import { useViewerStore } from "@/stores/viewer";
import { hqDownscale } from "@/hqDownscale";
import { t } from "@/i18n";

const ZOOM_STEP = 25; // 固定档位步长（%）
const ZOOM_MIN = 25;
const ZOOM_MAX = 3200;
const WHEEL_COOLDOWN_MS = 300;
const HQ_DEBOUNCE_MS = 150; // 高画质位图重生成防抖（缩放/窗口停稳后执行）

type Size = { w: number; h: number };

// 高画质降采样位图：显示 canvas 的 backing 尺寸（设备像素）
type HqBitmap = { w: number; h: number };

// 双层交叉缓冲：front 在显示，back 在加载；back 全部 img load 完成后才翻转，
// 翻转前旧图始终完整可见 → 任何加载耗时不黑屏。翻转后清空 back（释放解码内存）。
interface Layer {
  uri: string;
  uri2: string; // 双页第二图；单页/独占时为空
  group: number[]; // 提交时的 groupIndices 快照（back 层布局独立于 store 当前组）
  size: Size | null; // img A 的原始尺寸（单页缩放用）
  need: number; // 需等待的 img 数
  got: number;
  panX: number; // 屏幕坐标平移（拖动查看放大后的图片）
  panY: number;
  hq: [HqBitmap | null, HqBitmap | null]; // 高画质位图（就绪后顶替 <img> 显示）
}

function emptyLayer(): Layer {
  return {
    uri: "",
    uri2: "",
    group: [],
    size: null,
    need: 0,
    got: 0,
    panX: 0,
    panY: 0,
    hq: [null, null],
  };
}

const store = useViewerStore();

const layers = reactive<[Layer, Layer]>([emptyLayer(), emptyLayer()]);
const activeIdx = ref(0);
const effectLayer = ref(-1); // 正在播切换效果的层（新 front），-1 = 无

const containerSize = ref<Size | null>(null);
const viewerEl = ref<HTMLElement | null>(null);
const lastWheelTime = ref(0);
const rotation = ref(0);

const activeLayer = computed(() => layers[activeIdx.value]);

// store 提交新组 → 装入 back 层，等该层 img 全部 load 后翻转
watch(
  () => [store.dataUri, store.dataUri2, store.groupIndices] as const,
  ([uri, uri2, group]) => {
    if (!uri) {
      return;
    }
    const idx = 1 - activeIdx.value;
    releaseHq(idx);
    const L = layers[idx];
    L.uri = uri;
    L.uri2 = uri2;
    L.group = [...group];
    // 尺寸在 loadImage/loadGroup 提交前已由 fetchAndMeasure 测量并缓存，
    // 直接预填 → back 层首次渲染即为适配比例，切换图片无缩放跳变
    const headPath = group.length ? store.files[group[0]]?.path : undefined;
    L.size = (headPath ? store.sizeCache[headPath] : undefined) ?? null;
    L.need = uri2 ? 2 : 1;
    L.got = 0;
    // 每层独立平移：新图从居中开始，旧图翻到 back 后保留原视角（随即被清空）
    L.panX = 0;
    L.panY = 0;
  },
);

// store 清空（目录删空等）→ 释放两层，回到占位页
watch(
  () => store.dataUri,
  (uri) => {
    if (!uri) {
      for (let i = 0; i < layers.length; i++) {
        releaseHq(i);
        Object.assign(layers[i], emptyLayer());
      }
      activeIdx.value = 0;
    }
  },
);

function onLayerImgDone(idx: number, which: 0 | 1, e: Event) {
  const L = layers[idx];
  const img = e.target as HTMLImageElement;
  const expect = which === 0 ? L.uri : L.uri2;
  if (!expect || img.src !== expect) {
    return;
  } // 过期事件（快速翻页时被替换的 src）
  if (e.type === "load") {
    hqSources[idx][which] = img;
    if (which === 0) {
      L.size = { w: img.naturalWidth, h: img.naturalHeight };
    }
  } else {
    hqSources[idx][which] = null;
  }
  L.got++;
  if (L.got >= L.need) {
    scheduleHq(idx, true); // 高画质位图后台生成、就绪后热替换（不阻塞翻页）
    if (idx !== activeIdx.value) {
      flipTo(idx);
    }
  }
}

function flipTo(idx: number) {
  stopPan(); // 拖拽中翻页：结束旧图拖拽，新图从居中开始
  activeIdx.value = idx;
  if (store.slideshowActive && store.slideshowEffect !== "none") {
    effectLayer.value = idx; // 动画结束后再清 back（见 onEffectEnd）
  } else {
    clearBack();
  }
}

function clearBack() {
  const idx = 1 - activeIdx.value;
  releaseHq(idx);
  Object.assign(layers[idx], emptyLayer());
}

function onEffectEnd() {
  effectLayer.value = -1;
  clearBack();
}

// ---- 高画质降采样位图（适配窗口/缩小场景）----
// <img> 始终是解码源，也是任何失败时的回退显示；图片被明显缩小时，
// 后台按“显示分辨率”生成一张位图（分步预滤波 + Lanczos3 收尾，见 @/hqDownscale），
// 就绪后用它顶替 <img>。生成不阻塞翻页（先翻转、后热替换）。

const hqCanvasEls = new Map<string, HTMLCanvasElement>();
const hqSources: (HTMLImageElement | null)[][] = [
  [null, null],
  [null, null],
];
const hqTimers: (number | null)[] = [null, null];
const hqTokens = [0, 0];

function hqRef(i: number, which: 0 | 1, el: unknown) {
  const key = `${i}:${which}`;
  if (el instanceof HTMLCanvasElement) {
    hqCanvasEls.set(key, el);
  } else {
    hqCanvasEls.delete(key);
  }
}

// 各页的目标 backing 尺寸（显示设备像素 = 显示尺寸 × devicePixelRatio）
function hqTargets(L: Layer): { which: 0 | 1; w: number; h: number }[] {
  if (!L.size || !containerSize.value) {
    return [];
  }
  const dpr = window.devicePixelRatio || 1;
  if (L.uri2) {
    const d = dualLayoutFor(L);
    if (!d) {
      return [];
    }
    return [
      { which: 0, w: Math.round(d.w0 * dpr), h: Math.round(d.H * dpr) },
      { which: 1, w: Math.round(d.w1 * dpr), h: Math.round(d.H * dpr) },
    ];
  }
  const s = layerSingleScale(L);
  return [{ which: 0, w: Math.round(L.size.w * s * dpr), h: Math.round(L.size.h * s * dpr) }];
}

function scheduleHq(i: number, immediate = false) {
  const L = layers[i];
  if (!L.uri || !L.size) {
    return;
  }
  const timer = hqTimers[i];
  if (timer !== null) {
    clearTimeout(timer);
  }
  hqTimers[i] = window.setTimeout(
    () => {
      hqTimers[i] = null;
      void runHq(i);
    },
    immediate ? 0 : HQ_DEBOUNCE_MS,
  );
}

async function runHq(i: number) {
  const L = layers[i];
  if (!L.uri || !L.size || !containerSize.value) {
    return;
  }
  const uri = L.uri;
  const token = ++hqTokens[i];
  for (const t of hqTargets(L)) {
    const img = hqSources[i][t.which];
    if (!img) {
      continue;
    }
    // ponytail: 只按扩展名跳过 GIF；动画 WebP 无法廉价探测，命中该路径会停在首帧
    const src = store.files[L.group[t.which]]?.path ?? "";
    if (src.toLowerCase().endsWith(".gif")) {
      continue; // 动图交给 <img> 播放，静态位图会定格
    }
    let result: HTMLCanvasElement | null = null;
    try {
      result = await hqDownscale(img, t.w, t.h);
    } catch {
      result = null;
    }
    if (token !== hqTokens[i] || L.uri !== uri) {
      return; // 已过期（换图或重新调度）
    }
    if (!result) {
      // 不适用（放大/几乎不缩/超预算）或失败：退回 <img>
      releaseHqPage(i, t.which);
      continue;
    }
    const el = hqCanvasEls.get(`${i}:${t.which}`);
    const ctx = el?.getContext("2d");
    if (!el || !ctx) {
      continue;
    }
    el.width = result.width;
    el.height = result.height;
    ctx.drawImage(result, 0, 0);
    result.width = 1; // 已拷入显示 canvas：立即释放临时位图
    result.height = 1;
    L.hq[t.which] = { w: el.width, h: el.height };
  }
}

// 释放单页位图（缩回 backing 内存释放）
function releaseHqPage(i: number, which: 0 | 1) {
  layers[i].hq[which] = null;
  const el = hqCanvasEls.get(`${i}:${which}`);
  if (el) {
    el.width = 1;
    el.height = 1;
  }
}

// 释放整层（换图 / 清空时调用）
function releaseHq(i: number) {
  hqTokens[i]++;
  hqSources[i][0] = null;
  hqSources[i][1] = null;
  releaseHqPage(i, 0);
  releaseHqPage(i, 1);
}

// 缩放模式/倍率/窗口尺寸变化 → 停稳后按新尺度重生成（旧位图继续显示到替换完成）
watch(
  () => [containerSize.value, store.zoomMode, store.customZoom] as const,
  () => {
    scheduleHq(0);
    scheduleHq(1);
  },
);

// 单页 canvas 的 CSS 尺寸/变换，与 <img> 完全一致（backing 是“显示设备像素”）
function hqSingleStyle(L: Layer) {
  if (!L.size) {
    return {};
  }
  return {
    width: `${L.size.w}px`,
    height: `${L.size.h}px`,
    transform: `scale(${layerSingleScale(L)})`,
  };
}

const effectClasses = computed(() => ({
  "effect-fade": store.slideshowEffect === "fade",
  "effect-flip": store.slideshowEffect === "flip",
  "effect-slide": store.slideshowEffect === "slide",
}));

// 双页（两张竖图）布局：把两图等比缩到同高 H，再整体 fit 进容器。
// 这里用「静态 width/height」排版而非 transform: scale——因为两图缩放比不同，
// 用 transform 不改变布局盒会导致 flex 行错位/裁剪。width/height 只在翻页时离散赋值，
// 不做 transition，不违反 CLAUDE.md「不得动画 width/height」的约束。
// 每层用自己的 group 快照计算，back 层布局不影响 front 显示。
function dualLayoutFor(L: Layer) {
  if (L.group.length !== 2 || !containerSize.value) {
    return null;
  }
  const { w: cw, h: ch } = containerSize.value;
  if (cw === 0 || ch === 0) {
    return null;
  }
  const f0 = store.files[L.group[0]];
  const f1 = store.files[L.group[1]];
  if (!f0 || !f1) {
    return null;
  }
  const s0 = store.sizeCache[f0.path];
  const s1 = store.sizeCache[f1.path];
  if (!s0 || !s1 || s0.h === 0 || s1.h === 0) {
    return null;
  }
  const a0 = s0.w / s0.h;
  const a1 = s1.w / s1.h;
  // 与单页 fit 同规则：只缩不放。上限取两页较小原生高度，页面比窗口小就按原生尺寸并排。
  const H = Math.min(cw / (a0 + a1), ch, s0.h, s1.h);
  return { H, w0: H * a0, w1: H * a1, h0: s0.h, h1: s1.h };
}

function dualStyleFor(L: Layer, which: 0 | 1) {
  const d = dualLayoutFor(L);
  if (!d) {
    return {};
  }
  return which === 0
    ? { width: d.w0 + "px", height: d.H + "px" }
    : { width: d.w1 + "px", height: d.H + "px" };
}

// 单页缩放（fit/width/custom；双页下的单张组按 fit）
function layerSingleScale(L: Layer): number {
  if (!L.size || !containerSize.value) {
    return 1;
  }
  const { w: iw, h: ih } = L.size;
  const { w: cw, h: ch } = containerSize.value;
  if (iw === 0 || ih === 0 || cw === 0 || ch === 0) {
    return 1;
  }
  if (store.zoomMode === "width") {
    return cw / iw;
  }
  if (store.zoomMode === "custom") {
    return store.customZoom / 100;
  }
  // fit（含双页模式下的封面/横图独占等单张组）：只缩不放。
  // 图片大于窗口时等比缩到长宽都装得下；原生尺寸装得下时保持 100% 不放大。
  return Math.min(1, cw / iw, ch / ih);
}

// ---- 拖动平移 ----
// 平移作用在 .stage 的屏幕坐标上（transform: translate(...) rotate(...)），
// 拖动方向始终跟随鼠标、与显示旋转无关；边界按旋转后的可视包围盒计算：
// 放大后可以拖到任意边缘，缩到小于窗口时自动回中。
const isPanning = ref(false);
let panPointerId: number | null = null;
let lastPointerX = 0;
let lastPointerY = 0;

// 图片当前的实际可视尺寸（含缩放；双页为整组的排版尺寸）
function layerVisualSize(L: Layer): Size | null {
  if (L.uri2) {
    const d = dualLayoutFor(L);
    return d ? { w: d.w0 + d.w1, h: d.H } : null;
  }
  if (!L.size) {
    return null;
  }
  const s = layerSingleScale(L);
  return { w: L.size.w * s, h: L.size.h * s };
}

// 允许的平移范围：图片边缘最多贴到窗口边缘，不允许拖出黑边（图片比窗口小时锁死居中）
function panBounds(L: Layer): { x: number; y: number } {
  const vs = layerVisualSize(L);
  const cs = containerSize.value;
  if (!vs || !cs) {
    return { x: 0, y: 0 };
  }
  const oddTurn = Math.abs(Math.round(rotation.value / 90)) % 2 === 1;
  const bw = oddTurn ? vs.h : vs.w;
  const bh = oddTurn ? vs.w : vs.h;
  return {
    x: Math.max(0, (bw - cs.w) / 2),
    y: Math.max(0, (bh - cs.h) / 2),
  };
}

function clampPan(L: Layer) {
  const b = panBounds(L);
  L.panX = Math.min(b.x, Math.max(-b.x, L.panX));
  L.panY = Math.min(b.y, Math.max(-b.y, L.panY));
}

const canPan = computed(() => {
  const b = panBounds(activeLayer.value);
  return b.x > 0.5 || b.y > 0.5;
});

// 缩放/旋转/容器尺寸/换图变化后，旧平移量可能超出新边界 → 立即回夹
watch(
  () =>
    [
      containerSize.value,
      rotation.value,
      store.zoomMode,
      store.customZoom,
      layers[0].size,
      layers[1].size,
      layers[0].group,
      layers[1].group,
    ] as const,
  () => {
    for (const L of layers) {
      clampPan(L);
    }
  },
);

function stageTransform(L: Layer) {
  return { transform: `translate(${L.panX}px, ${L.panY}px) rotate(${rotation.value}deg)` };
}

function onPointerDown(e: PointerEvent) {
  if (!e.isPrimary || e.button !== 0 || !canPan.value) {
    return;
  }
  e.preventDefault();
  panPointerId = e.pointerId;
  lastPointerX = e.clientX;
  lastPointerY = e.clientY;
  isPanning.value = true;
  viewerEl.value?.setPointerCapture(e.pointerId);
}

function onPointerMove(e: PointerEvent) {
  if (!isPanning.value || e.pointerId !== panPointerId) {
    return;
  }
  const L = activeLayer.value;
  L.panX += e.clientX - lastPointerX;
  L.panY += e.clientY - lastPointerY;
  lastPointerX = e.clientX;
  lastPointerY = e.clientY;
  clampPan(L);
}

function stopPan() {
  const id = panPointerId;
  panPointerId = null;
  isPanning.value = false;
  if (id !== null && viewerEl.value?.hasPointerCapture(id)) {
    viewerEl.value.releasePointerCapture(id);
  }
}

function endPan(e: PointerEvent) {
  if (e.pointerId !== panPointerId) {
    return;
  }
  stopPan();
}

// 旋转时把平移向量一起旋转，保持当前观察的图片区域仍在视野中央
function rotatePan(deg: 90 | -90) {
  const L = activeLayer.value;
  if (!L.uri) {
    return;
  }
  const { panX, panY } = L;
  if (deg === 90) {
    L.panX = -panY;
    L.panY = panX;
  } else {
    L.panX = panY;
    L.panY = -panX;
  }
  clampPan(L);
}

// 当前有效缩放（displayZoom 与缩放按钮用）：双页对取整体缩放，单页取自身缩放
const effectiveScale = computed(() => {
  const L = activeLayer.value;
  const d = L.uri2 ? dualLayoutFor(L) : null;
  if (d) {
    return d.H / Math.max(d.h0, d.h1);
  }
  return layerSingleScale(L);
});

const displayZoom = computed(() => Math.round(effectiveScale.value * 100));

const emit = defineEmits<{
  "update:displayZoom": [value: number];
}>();

watch(displayZoom, (value) => emit("update:displayZoom", value), { immediate: true });

function updateContainerSize() {
  if (viewerEl.value) {
    const r = viewerEl.value.getBoundingClientRect();
    containerSize.value = { w: r.width, h: r.height };
  }
}

let resizeObs: ResizeObserver | null = null;

onMounted(() => {
  updateContainerSize();
  resizeObs = new ResizeObserver(updateContainerSize);
  if (viewerEl.value) {
    resizeObs.observe(viewerEl.value);
  }
  window.addEventListener("blur", stopPan);
});

onUnmounted(() => {
  resizeObs?.disconnect();
  window.removeEventListener("blur", stopPan);
  for (const t of hqTimers) {
    if (t !== null) {
      clearTimeout(t);
    }
  }
});

function zoomIn() {
  if (!activeLayer.value.uri) {
    return;
  }
  const cur = Math.round(effectiveScale.value * 100);
  const next = Math.min(ZOOM_MAX, Math.floor(cur / ZOOM_STEP) * ZOOM_STEP + ZOOM_STEP);
  if (next === cur) {
    return;
  }
  store.customZoom = next;
  store.zoomMode = "custom";
  store.showToast(next + "%");
}

function zoomOut() {
  if (!activeLayer.value.uri) {
    return;
  }
  const cur = Math.round(effectiveScale.value * 100);
  const next = Math.max(ZOOM_MIN, Math.ceil(cur / ZOOM_STEP) * ZOOM_STEP - ZOOM_STEP);
  if (next >= cur) {
    return;
  }
  store.customZoom = next;
  store.zoomMode = "custom";
  store.showToast(next + "%");
}

function rotateClockwise() {
  rotation.value += 90;
  rotatePan(90);
}

function rotateCounterClockwise() {
  rotation.value -= 90;
  rotatePan(-90);
}

function onWheel(e: WheelEvent) {
  // Ctrl + 滚轮：缩放（不受翻页冷却影响）
  if (e.ctrlKey) {
    if (e.deltaY === 0) {
      return;
    }
    if (e.deltaY < 0) {
      zoomIn();
    } else {
      zoomOut();
    }
    return;
  }
  if (store.files.length === 0) {
    return;
  }
  if (e.deltaY === 0) {
    return;
  }

  const now = Date.now();
  if (now - lastWheelTime.value < WHEEL_COOLDOWN_MS) {
    return;
  }
  lastWheelTime.value = now;

  if (e.deltaY < 0) {
    store.goPrev();
  } else {
    store.goNext();
  }
}

defineExpose({
  zoomIn,
  zoomOut,
  rotateClockwise,
  rotateCounterClockwise,
});
</script>

<template>
  <div
    ref="viewerEl"
    class="viewer-layer"
    :class="{ 'can-pan': canPan, panning: isPanning }"
    :style="{ '--rot': rotation + 'deg' }"
    @wheel.prevent="onWheel"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="endPan"
    @pointercancel="endPan"
    @lostpointercapture="endPan"
  >
    <!-- 双层交叉缓冲：front 覆盖 back；back 加载完成即翻转为 front -->
    <div
      v-for="(L, i) in layers"
      :key="i"
      v-show="L.uri"
      class="stage"
      :class="[
        L.uri2 ? 'dual' : 'single',
        { rtl: store.isRtl, front: i === activeIdx, back: i !== activeIdx },
        i === effectLayer ? effectClasses : {},
      ]"
      :style="stageTransform(L)"
      @animationend="i === effectLayer && onEffectEnd()"
    >
      <!-- 双页：两张竖图并排；RTL 时第一张在右 -->
      <template v-if="L.uri2">
        <img
          class="page page-a"
          crossorigin="anonymous"
          :class="{ 'hq-source': L.hq[0] }"
          :src="L.uri"
          :style="dualStyleFor(L, 0)"
          @load="onLayerImgDone(i, 0, $event)"
          @error="onLayerImgDone(i, 0, $event)"
        />
        <canvas
          v-show="L.hq[0]"
          :ref="(el) => hqRef(i, 0, el)"
          class="page hq-cv"
          :style="dualStyleFor(L, 0)"
        />
        <img
          class="page page-b"
          crossorigin="anonymous"
          :class="{ 'hq-source': L.hq[1] }"
          :src="L.uri2"
          :style="dualStyleFor(L, 1)"
          @load="onLayerImgDone(i, 1, $event)"
          @error="onLayerImgDone(i, 1, $event)"
        />
        <canvas
          v-show="L.hq[1]"
          :ref="(el) => hqRef(i, 1, el)"
          class="page hq-cv"
          :style="dualStyleFor(L, 1)"
        />
      </template>
      <!-- 单页：含 fit/width/custom，以及双页下的封面/横图独占/尾页 -->
      <template v-else>
        <img
          crossorigin="anonymous"
          :class="{ 'no-anim': i !== activeIdx, 'hq-source': L.hq[0] }"
          :src="L.uri"
          :style="{ transform: `scale(${layerSingleScale(L)})` }"
          @load="onLayerImgDone(i, 0, $event)"
          @error="onLayerImgDone(i, 0, $event)"
        />
        <canvas
          v-show="L.hq[0]"
          :ref="(el) => hqRef(i, 0, el)"
          class="hq-cv"
          :class="{ 'no-anim': i !== activeIdx }"
          :style="hqSingleStyle(L)"
        />
      </template>
    </div>
    <div v-if="!store.dataUri" class="placeholder">
      <div class="placeholder-actions">
        <button class="placeholder-btn" @click="store.openFile">
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
          {{ t("app.openImage") }}
        </button>
        <button class="placeholder-btn secondary" @click="store.openFolder">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
          {{ t("app.openFolder") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.viewer-layer {
  position: fixed;
  inset: 0;
  z-index: 0;
  overflow: hidden;
  touch-action: none;
  background: var(--bg);
  perspective: 1200px;
}

/* 可拖动时给出抓手光标；拖动中禁用 stage 的 transform 过渡，保证跟手 */

.viewer-layer.can-pan {
  cursor: grab;
}

.viewer-layer.panning {
  cursor: grabbing;
}

.viewer-layer.panning .stage {
  transition: none;
}

.viewer-layer img,
.viewer-layer canvas {
  max-width: none;
  max-height: none;
  user-select: none;
  transform-origin: center center;
  transition: transform 300ms ease-out;
  -webkit-user-drag: none;
}

/* 高画质位图就绪后 <img> 退出布局（继续作为解码源保留） */

.hq-source {
  position: absolute;
  visibility: hidden;
  pointer-events: none;
}

/* will-change 只留在 .stage（它已是常驻合成层，且保证 back 层翻转前已光栅化）；
   img 再单独提升一层是冗余的，缩放过渡开始时浏览器会自动提升 */

/* back 层：新图先以 scale(1) 渲染，load 后才知道适配比例，
   若保留过渡会看到“缩放动画”→ 未显示的层禁用过渡，翻转时已是最终比例 */

.viewer-layer img.no-anim,
.viewer-layer canvas.no-anim {
  transition: none;
}

.stage {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  /* transform 由 stageTransform(L) 内联提供：translate(pan) rotate(rot) */
  transform-origin: center center;
  transition: transform 300ms ease-out;
  will-change: transform;
}

/* front 覆盖 back：back 始终完整渲染但被遮住，翻转瞬间无重解码 */

.stage.front {
  z-index: 2;
}

.stage.back {
  z-index: 1;
}

.stage.dual {
  flex-direction: row;
  gap: 0;
}

.stage.dual.rtl {
  flex-direction: row-reverse;
}

.stage.dual .page {
  display: block;
  flex: 0 0 auto;
  /* width/height 由内联样式按 dualLayoutFor 静态赋值（非动画） */
}

/* 幻灯片切换效果：仅 transform / opacity，GPU 友好；
   作用于新 front 层，旧图垫在底下 → fade 即真正的交叉淡入淡出 */

.effect-fade {
  animation: fx-fade 300ms ease-out;
}

.effect-flip {
  backface-visibility: hidden;
  animation: fx-flip 400ms ease-out;
}

.effect-slide {
  animation: fx-slide 300ms ease-out;
}

@keyframes fx-fade {
  from {
    opacity: 0;
  }

  to {
    opacity: 1;
  }
}

@keyframes fx-flip {
  from {
    transform: rotate(var(--rot, 0deg)) rotateY(90deg);
  }

  to {
    transform: rotate(var(--rot, 0deg)) rotateY(0deg);
  }
}

@keyframes fx-slide {
  from {
    opacity: 0;
    transform: rotate(var(--rot, 0deg)) translateX(30px);
  }

  to {
    opacity: 1;
    transform: rotate(var(--rot, 0deg)) translateX(0);
  }
}

.placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.placeholder-btn {
  display: inline-flex;
  gap: 10px;
  align-items: center;
  padding: 14px 32px;
  font-family: var(--font);
  font-size: 1rem;
  font-weight: 600;
  color: var(--fg);
  cursor: pointer;
  background: var(--surface-soft);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  isolation: isolate;
  backdrop-filter: blur(12px);
}

/* hover 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.placeholder-btn::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.placeholder-btn:hover::before {
  opacity: 1;
}

.placeholder-actions {
  display: flex;
  gap: 12px;
}

.placeholder-btn.secondary {
  color: var(--fg-muted);
  background: transparent;
}

.placeholder-btn.secondary:hover {
  color: var(--fg);
}

.placeholder-btn svg {
  width: 20px;
  height: 20px;
  color: var(--accent);
}
</style>
