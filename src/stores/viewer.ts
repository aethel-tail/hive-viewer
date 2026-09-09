import { ref, reactive, computed, watch } from "vue";
import { defineStore } from "pinia";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { LazyStore } from "@tauri-apps/plugin-store";
import { t, type Locale } from "@/i18n";

export interface ImageFile {
  name: string;
  path: string;
}

export interface FolderContent {
  dir_path: string;
  files: ImageFile[];
}

type ZoomMode = "fit" | "width" | "custom" | "dual-ltr" | "dual-rtl";
type Size = { w: number; h: number };
type SlideshowOrder = "loop" | "random";
type SlideshowEffect = "none" | "flip" | "fade" | "slide";
type EndReachAction = "loop" | "none" | "ask";
type Theme =
  | "system"
  | "dark"
  | "light"
  | "warm"
  | "night"
  | "forest"
  | "ocean"
  | "sakura"
  | "graphite";

// 设置面板「一般」页的默认值（重置时也以此为基准）
export const DEFAULT_GENERAL_SETTINGS = {
  allowMultipleInstances: false, // 允许多个实例
  confirmDelete: true, // 删除文件时确认
  alwaysOnTop: false, // 总在最前
  escToExit: true, // 按 Esc 退出
  shellContextMenu: true, // 右键菜单：在资源管理器中使用（仅安装版，需已注册 sparse 包）
  shellContextMenuOpen: true, // 右键菜单：打开图片
  shellContextMenuConvert: true, // 右键菜单：格式转换
  endReachAction: "loop" as EndReachAction, // 到达收尾图片时执行
  autoCheckUpdates: false, // 自动获取更新：每次启动后台静默检查 GitHub Release（默认关闭）
  theme: "system" as Theme, // 主题：跟随系统（默认）/ 黑 / 白 / 配色方案
  fontFamily: "", // 全局字体：'' = 默认链（思源 → 苹果 → 雅黑兜底，见 global.css --font-default）
  language: "zh-CN" as Locale, // 界面语言：简中 / 繁中 / EN / 日
};

// 快捷键预设（小写 spec：修饰键 + 主键，如 'ctrl+r'、'arrowleft'；'digit' 表示 1~9 数字键）
export const DEFAULT_SHORTCUTS = {
  prev: "arrowleft", // 上一张图片
  next: "arrowright", // 下一张图片
  rotateCw: "ctrl+r", // 顺时针旋转
  rotateCcw: "ctrl+l", // 逆时针旋转
  fullscreen: "f11", // 全屏 / 退出全屏
  fullscreenEnter: "alt+enter", // 全屏（Alt+Enter）
  toggleExif: "tab", // 打开 / 关闭 EXIF 面板
  stopSlideshow: "ctrl+0", // 停止幻灯片播放
  slideshowSpeed: "ctrl+digit", // 幻灯片播放 1~9 秒
};

// 只读模式：独立转换窗口只读设置，不回写 settings.json，
// 避免与主窗口的保存互相覆盖（两个 webview 各有一份模块状态，互不影响）。
let settingsReadOnly = false;
export function setSettingsReadOnly() {
  settingsReadOnly = true;
}

export const useViewerStore = defineStore("viewer", () => {
  const files = ref<ImageFile[]>([]);
  const currentIndex = ref(0);
  const dataUri = ref("");
  const dataUri2 = ref(""); // 双页第二图；单页/封面/横图独占时为空
  const zoomMode = ref<ZoomMode>("fit");
  const customZoom = ref(100);

  // 当前显示组：双页时为 [head] 或 [head, head+1]（逻辑顺序，方向由组件决定）；单页为 [currentIndex]
  const groupIndices = ref<number[]>([]);

  // 缓存：图片 dataURI 与原始尺寸（按路径）。仅前端内存，不持久化。
  // 超过上限按最近最少使用淘汰，防止超大目录无限占用内存。
  const CACHE_LIMIT = 500;
  const uriCache = reactive<Record<string, string>>({});
  const sizeCache = reactive<Record<string, Size>>({});
  const cacheOrder: string[] = [];

  function touchCache(path: string) {
    const i = cacheOrder.indexOf(path);
    if (i !== -1) {
      cacheOrder.splice(i, 1);
    }
    cacheOrder.push(path);
    while (cacheOrder.length > CACHE_LIMIT) {
      const old = cacheOrder.shift()!;
      delete uriCache[old];
      delete sizeCache[old];
    }
  }

  // 异步竞态守卫：连续翻页时只让最新一次 load 提交状态
  let loadSeq = 0;

  // 幻灯片计时与随机顺序状态
  let timer: number | null = null;
  let shuffledBag: number[] = [];

  const store = new LazyStore("settings.json");

  const topToolbarLocked = ref(true);
  const bottomFloatingBarLocked = ref(true);
  const exifPanelVisible = ref(false);
  const topToolbarLockedEdited = ref(false);
  const bottomFloatingBarLockedEdited = ref(false);
  const exifPanelVisibleEdited = ref(false);

  // 幻灯片设置（active 不持久化）
  const slideshowActive = ref(false);
  const slideshowInterval = ref(3);
  const slideshowOrder = ref<SlideshowOrder>("loop");
  const slideshowEffect = ref<SlideshowEffect>("none");
  const slideshowIntervalEdited = ref(false);
  const slideshowOrderEdited = ref(false);
  const slideshowEffectEdited = ref(false);

  // 一般设置（设置面板「一般」页；行为均已接线：见 App.vue 与下方翻页/删除逻辑）
  const generalSettings = reactive({ ...DEFAULT_GENERAL_SETTINGS });
  const generalSettingsEdited = ref(false);

  // 快捷键配置
  const shortcuts = reactive({ ...DEFAULT_SHORTCUTS });
  const shortcutsEdited = ref(false);

  // 轻提示（如缩放比例）；带 action 时提示可点击（用于更新提示 → 打开 Release 页面）
  const toast = reactive<{
    text: string;
    seq: number;
    action: (() => void) | null;
    duration: number;
  }>({ text: "", seq: 0, action: null, duration: 1200 });
  function showToast(text: string, opts?: { action?: () => void; duration?: number }) {
    toast.text = text;
    toast.action = opts?.action ?? null;
    toast.duration = opts?.duration ?? 1200;
    toast.seq++;
  }

  // 原生模态询问（标题为应用名）；kind: info/warning/error
  async function askNative(
    message: string,
    okLabel: string,
    cancelLabel: string,
    kind = "info",
  ): Promise<boolean> {
    try {
      return await invoke<boolean>("ask_confirm", { message, okLabel, cancelLabel, kind });
    } catch (e) {
      console.error("ask_confirm failed:", e);
      return false;
    }
  }

  // 注册/注销 Windows 11 右键菜单 sparse 包（仅安装版有效）；失败只提示，不阻断设置
  async function setShellContextMenu(enabled: boolean) {
    try {
      await invoke("set_shell_context_menu", { enabled });
    } catch (e) {
      console.error("set_shell_context_menu failed:", e);
      showToast(t("settings.context.error", { msg: String(e) }));
    }
  }

  // 手动翻页到达首/尾时的策略（endReachAction）：
  // loop → 允许回绕；none → toast 并停在原地；ask → 原生询问。
  // 返回 true 表示允许回绕（调用方执行跳转），false 表示不翻页。
  async function boundaryDecision(dir: "prev" | "next"): Promise<boolean> {
    const act = generalSettings.endReachAction;
    if (act === "loop") {
      return true;
    }
    if (act === "none") {
      showToast(dir === "next" ? t("message.atEnd") : t("message.atStart"));
      return false;
    }
    return askNative(
      dir === "next" ? t("message.loopEnd") : t("message.loopStart"),
      t("common.yes"),
      t("common.no"),
    );
  }

  const currentFile = computed(() => files.value[currentIndex.value]);
  const isDual = computed(() => zoomMode.value === "dual-ltr" || zoomMode.value === "dual-rtl");
  const isRtl = computed(() => zoomMode.value === "dual-rtl");

  function isLandscape(s: Size | undefined): boolean {
    return !!s && s.w >= s.h;
  }

  // 读取图片尺寸：优先让后端只读文件头（含 EXIF 方向校正），
  // 避免为了量尺寸把整图解码一遍；后端失败时回退到 <img> 探测。
  async function measure(uri: string, path: string): Promise<Size> {
    const cached = sizeCache[path];
    if (cached) {
      return cached;
    }
    try {
      const [w, h] = await invoke<[number, number]>("image_dims", { path });
      const s = { w, h };
      sizeCache[path] = s;
      return s;
    } catch (e) {
      console.error("image_dims failed, falling back to <img>:", e);
      return measureViaImg(uri, path);
    }
  }

  function measureViaImg(uri: string, path: string): Promise<Size> {
    return new Promise<Size>((resolve) => {
      const img = new Image();
      img.onload = () => {
        const s = { w: img.naturalWidth, h: img.naturalHeight };
        sizeCache[path] = s;
        resolve(s);
      };
      img.onerror = () => {
        const s = { w: 0, h: 0 };
        sizeCache[path] = s;
        resolve(s);
      };
      img.src = uri;
    });
  }

  // 取可加载 URL（asset 协议，WebView2 直接流式读文件，免 base64 大字符串 IPC），并确保 sizeCache 有该图尺寸。
  async function fetchAndMeasure(path: string): Promise<{ uri: string; size: Size }> {
    let uri = uriCache[path];
    if (!uri) {
      uri = convertFileSrc(path);
      uriCache[path] = uri;
    }
    const size = await measure(uri, path);
    touchCache(path);
    return { uri, size };
  }

  // 单页加载（fit/width/custom 用）
  async function loadImage(path: string) {
    const seq = ++loadSeq;
    try {
      const { uri } = await fetchAndMeasure(path);
      if (seq !== loadSeq) {
        return;
      }
      dataUri.value = uri;
      dataUri2.value = "";
      groupIndices.value = [currentIndex.value];

      // look-ahead 1 张：预热下一张（不 await，失败静默）
      const nextIdx = currentIndex.value + 1;
      if (nextIdx < files.value.length) {
        warm(files.value[nextIdx].path);
      }
    } catch (e) {
      console.error("Load failed:", e);
    }
  }

  // 双页组加载：从 head 起按 D1=b 规则决定本组 1 或 2 张。
  // 规则：封面(0)单放；横图(w>=h)独占；竖图+下一竖图成双；竖图+横图/无下一张 → 当前单放。
  async function loadGroup(head: number) {
    const seq = ++loadSeq;
    const len = files.value.length;
    if (len === 0) {
      return;
    }
    head = Math.max(0, Math.min(head, len - 1));

    try {
      const a = files.value[head];
      const aRes = await fetchAndMeasure(a.path);
      if (seq !== loadSeq) {
        return;
      }

      let group: number[] = [head];
      let uri2 = "";

      if (!isLandscape(aRes.size) && head + 1 < len) {
        const b = files.value[head + 1];
        const bRes = await fetchAndMeasure(b.path);
        if (seq !== loadSeq) {
          return;
        }
        if (!isLandscape(bRes.size)) {
          group = [head, head + 1];
          uri2 = bRes.uri;
        }
        // 若 b 为横图：当前 a 单放，b 留给下一组独占
      }

      currentIndex.value = head;
      groupIndices.value = group;
      dataUri.value = aRes.uri;
      dataUri2.value = uri2;

      // look-ahead 1 组：预热下一组（不 await，失败静默）
      const nextHead = head + group.length;
      if (nextHead < len) {
        warm(files.value[nextHead].path);
        if (nextHead + 1 < len) {
          warm(files.value[nextHead + 1].path);
        }
      }
    } catch (e) {
      console.error("Load group failed:", e);
    }
  }

  function warm(path: string) {
    fetchAndMeasure(path).catch(() => {});
  }

  async function openFile() {
    try {
      const result = await invoke<FolderContent>("open_file");
      files.value = result.files;
      currentIndex.value = 0;
      groupIndices.value = [];
      clearCaches();
      zoomMode.value = "fit";
      if (result.files.length > 0) {
        await loadImage(result.files[0].path);
      }
    } catch (e) {
      console.error("Open file failed:", e);
    }
  }

  async function openImageByPath(path: string) {
    try {
      const result = await invoke<FolderContent>("open_path", { path });
      files.value = result.files;
      currentIndex.value = result.files.findIndex((f) => f.path === path);
      if (currentIndex.value === -1) {
        currentIndex.value = 0;
      }
      groupIndices.value = [];
      clearCaches();
      zoomMode.value = "fit";
      if (result.files.length > 0) {
        await loadImage(result.files[currentIndex.value].path);
      }
    } catch (e) {
      console.error("Open path failed:", e);
    }
  }

  async function openFolder() {
    try {
      const result = await invoke<FolderContent>("open_folder");
      files.value = result.files;
      currentIndex.value = 0;
      groupIndices.value = [];
      clearCaches();
      zoomMode.value = "fit";
      if (result.files.length > 0) {
        await loadImage(result.files[0].path);
      } else {
        // 空文件夹：清掉上一张图回到占位页，并提示
        dataUri.value = "";
        dataUri2.value = "";
        showToast(t("message.noImages"));
      }
    } catch (e) {
      console.error("Open folder failed:", e);
    }
  }

  function clearCaches() {
    for (const k of Object.keys(uriCache)) {
      delete uriCache[k];
    }
    for (const k of Object.keys(sizeCache)) {
      delete sizeCache[k];
    }
    cacheOrder.length = 0;
  }

  // 单页翻页：越界时按 endReachAction 决定回绕/停留/询问；返回 null = 不翻页
  async function boundaryIndex(dir: "prev" | "next"): Promise<number | null> {
    const len = files.value.length;
    if (len === 0) {
      return null;
    }
    const cur = currentIndex.value;
    if (dir === "next" && cur < len - 1) {
      return cur + 1;
    }
    if (dir === "prev" && cur > 0) {
      return cur - 1;
    }
    return (await boundaryDecision(dir)) ? (dir === "next" ? 0 : len - 1) : null;
  }

  async function goPrev() {
    if (files.value.length === 0) {
      return;
    }
    if (isDual.value) {
      if (await goPrevDual()) {
        resetSlideshowTimer();
      }
    } else {
      const nxt = await boundaryIndex("prev");
      if (nxt === null) {
        return;
      }
      currentIndex.value = nxt;
      await loadImage(files.value[nxt].path);
      resetSlideshowTimer();
    }
  }

  async function goNext() {
    if (files.value.length === 0) {
      return;
    }
    if (isDual.value) {
      if (await goNextDual()) {
        resetSlideshowTimer();
      }
    } else {
      const nxt = await boundaryIndex("next");
      if (nxt === null) {
        return;
      }
      currentIndex.value = nxt;
      await loadImage(files.value[nxt].path);
      resetSlideshowTimer();
    }
  }

  // 双页按「组」前进：下一组首 = 当前组首 + 当前组张数；到尾按 endReachAction 处理。
  // 返回是否发生了翻页（false = 策略拦截，停在原地）。
  async function goNextDual(): Promise<boolean> {
    const len = files.value.length;
    const head = currentIndex.value;
    const step = Math.max(1, groupIndices.value.length || 1);
    if (head + step >= len) {
      if (!(await boundaryDecision("next"))) {
        return false;
      }
      await loadGroup(0);
      return true;
    }
    await loadGroup(head + step);
    return true;
  }

  // 双页按「组」后退：从 0 起贪心推算“包含 head-1 的组”的组首。
  // 仅依赖已缓存尺寸；缺失尺寸的目标图会现测（goPrev 路径上多为已访问页，命中率高）。
  // 返回是否发生了翻页（false = 到最前被 endReachAction 拦截）。
  async function goPrevDual(): Promise<boolean> {
    const len = files.value.length;
    const head = currentIndex.value;
    if (head === 0) {
      // 到最前：按 endReachAction 决定是否回到最末组；从 0 推算至末尾取末组组首
      if (!(await boundaryDecision("prev"))) {
        return false;
      }
      let h = 0;
      let lastHead = 0;
      while (h < len) {
        lastHead = h;
        const s =
          sizeCache[files.value[h].path] ?? (await fetchAndMeasure(files.value[h].path)).size;
        if (isLandscape(s) || h + 1 >= len) {
          h += 1;
        } else {
          const s2 =
            sizeCache[files.value[h + 1].path] ??
            (await fetchAndMeasure(files.value[h + 1].path)).size;
          h += isLandscape(s2) ? 1 : 2;
        }
      }
      await loadGroup(lastHead);
      return true;
    }
    // 一般后退：从 0 推算，找到「组首 < head」的最大组首
    let h = 0;
    let prevHead = 0;
    while (h < head) {
      prevHead = h;
      const s = sizeCache[files.value[h].path] ?? (await fetchAndMeasure(files.value[h].path)).size;
      if (isLandscape(s) || h + 1 >= len) {
        h += 1;
      } else {
        const s2 =
          sizeCache[files.value[h + 1].path] ??
          (await fetchAndMeasure(files.value[h + 1].path)).size;
        h += isLandscape(s2) ? 1 : 2;
      }
    }
    await loadGroup(prevHead);
    return true;
  }

  function setZoomMode(mode: ZoomMode) {
    const wasDual = isDual.value;
    zoomMode.value = mode;
    const nowDual = mode === "dual-ltr" || mode === "dual-rtl";
    if (files.value.length === 0) {
      return;
    }
    if (nowDual && !wasDual) {
      // 进入双页：以当前页为组首成组
      loadGroup(currentIndex.value);
    } else if (!nowDual && wasDual) {
      // 退出双页：回到单页
      dataUri2.value = "";
      groupIndices.value = [currentIndex.value];
      loadImage(files.value[currentIndex.value].path);
    }
    // ltr<->rtl 互切无需重载，组件按 isRtl 重排
  }

  function setOriginalSize() {
    customZoom.value = 100;
    setZoomMode("custom");
  }

  // 幻灯片控制
  function clearSlideshowTimer() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function reshuffleBag(excludeIndex: number) {
    const len = files.value.length;
    if (len <= 1) {
      shuffledBag = [];
      return;
    }
    const indices: number[] = [];
    for (let i = 0; i < len; i++) {
      if (i !== excludeIndex) {
        indices.push(i);
      }
    }
    // Fisher-Yates shuffle
    for (let i = indices.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [indices[i], indices[j]] = [indices[j], indices[i]];
    }
    shuffledBag = indices;
  }

  async function slideshowTick() {
    if (files.value.length <= 1) {
      return;
    }
    if (slideshowOrder.value === "loop") {
      await stepAuto();
      return;
    }
    // random
    if (shuffledBag.length === 0) {
      reshuffleBag(currentIndex.value);
    }
    let nextIdx = shuffledBag.shift();
    if (nextIdx === undefined) {
      return;
    }
    if (nextIdx === currentIndex.value) {
      // 刚重建的袋可能只剩自己，尝试再取一次
      nextIdx = shuffledBag.shift();
      if (nextIdx === undefined) {
        return;
      }
    }
    await jumpToIndex(nextIdx);
    // random 顺序的 look-ahead：预热袋首（loadImage/loadGroup 的顺序预热对 random 无效）
    const peek = shuffledBag[0];
    if (peek !== undefined) {
      warm(files.value[peek].path);
      if (isDual.value && peek + 1 < files.value.length) {
        warm(files.value[peek + 1].path);
      }
    }
  }

  async function jumpToIndex(index: number) {
    if (index < 0 || index >= files.value.length) {
      return;
    }
    if (isDual.value) {
      await loadGroup(index);
    } else {
      currentIndex.value = index;
      await loadImage(files.value[index].path);
    }
  }

  // 幻灯片自动步进（loop）：按组前进并回绕；与手动翻页不同，不受 endReachAction 限制。
  async function stepAuto() {
    const len = files.value.length;
    if (len === 0) {
      return;
    }
    if (isDual.value) {
      const step = Math.max(1, groupIndices.value.length || 1);
      const next = currentIndex.value + step >= len ? 0 : currentIndex.value + step;
      await loadGroup(next);
    } else {
      const nxt = (currentIndex.value + 1) % len;
      currentIndex.value = nxt;
      await loadImage(files.value[nxt].path);
    }
  }

  // 自排下一拍（而非 setInterval）：等上一张加载完再计时，
  // 避免大图加载超过间隔时叠加解码
  function scheduleNextTick() {
    clearSlideshowTimer();
    timer = window.setTimeout(async () => {
      timer = null;
      try {
        await slideshowTick();
      } finally {
        if (slideshowActive.value) {
          scheduleNextTick();
        }
      }
    }, slideshowInterval.value * 1000);
  }

  function startSlideshow() {
    if (files.value.length <= 1) {
      return;
    }
    slideshowActive.value = true;
    scheduleNextTick();
  }

  function stopSlideshow() {
    slideshowActive.value = false;
    clearSlideshowTimer();
    shuffledBag = [];
  }

  function resetSlideshowTimer() {
    if (!slideshowActive.value) {
      return;
    }
    startSlideshow();
  }
  function setSlideshowInterval(n: number) {
    slideshowInterval.value = n;
  }
  function setSlideshowOrder(order: SlideshowOrder) {
    slideshowOrder.value = order;
  }
  function setSlideshowEffect(effect: SlideshowEffect) {
    slideshowEffect.value = effect;
  }

  // 删除当前页（双页模式 = 整组）：确认（按 confirmDelete 设置）→ 回收站 → 更新列表并加载邻图。
  // 删除后 files 变化会自动停止放映（见 watch(files)）。deleteBusy 防连点弹多个确认框。
  let deleteBusy = false;
  async function deleteCurrent() {
    if (deleteBusy || files.value.length === 0) {
      return;
    }
    deleteBusy = true;
    try {
      const idxs = isDual.value ? [...groupIndices.value] : [currentIndex.value];
      const names: string[] = [];
      for (const i of idxs) {
        const f = files.value[i];
        if (f) {
          names.push(f.name);
        }
      }
      if (names.length === 0) {
        return;
      }
      if (generalSettings.confirmDelete) {
        const ok = await askNative(
          t("delete.confirm", { name: names.join("、") }),
          t("delete.ok"),
          t("common.cancel"),
          "warning",
        );
        if (!ok) {
          return;
        }
      }
      const paths = idxs.map((i) => files.value[i].path);
      try {
        await invoke("delete_files", { paths });
      } catch (e) {
        console.error("Delete failed:", e);
        showToast(String(e));
        return;
      }
      const head = Math.min(...idxs);
      files.value = files.value.filter((_, i) => !idxs.includes(i));
      clearCaches();
      showToast(t("delete.done"));
      if (files.value.length === 0) {
        // 目录删空 → 回到占位页
        currentIndex.value = 0;
        groupIndices.value = [];
        dataUri.value = "";
        dataUri2.value = "";
        return;
      }
      if (isDual.value) {
        await loadGroup(Math.min(head, files.value.length - 1));
      } else {
        const nxt = Math.min(head, files.value.length - 1);
        currentIndex.value = nxt;
        await loadImage(files.value[nxt].path);
      }
    } finally {
      deleteBusy = false;
    }
  }

  async function loadSettings() {
    try {
      // 一次 IPC 取全部，避免 9 次串行 store.get
      const entries = new Map<string, unknown>(await store.entries());
      const top = entries.get("topToolbarLocked");
      const bottom = entries.get("bottomFloatingBarLocked");
      const exifVisible = entries.get("exifPanelVisible");
      const ssInterval = entries.get("slideshowInterval");
      const ssOrder = entries.get("slideshowOrder");
      const ssEffect = entries.get("slideshowEffect");
      if (typeof top === "boolean" && !topToolbarLockedEdited.value) {
        topToolbarLocked.value = top;
      }
      if (typeof bottom === "boolean" && !bottomFloatingBarLockedEdited.value) {
        bottomFloatingBarLocked.value = bottom;
      }
      if (typeof exifVisible === "boolean" && !exifPanelVisibleEdited.value) {
        exifPanelVisible.value = exifVisible;
      }
      if (typeof ssInterval === "number" && !slideshowIntervalEdited.value) {
        slideshowInterval.value = ssInterval;
      }
      if ((ssOrder === "loop" || ssOrder === "random") && !slideshowOrderEdited.value) {
        slideshowOrder.value = ssOrder;
      }
      if (
        (ssEffect === "none" ||
          ssEffect === "flip" ||
          ssEffect === "fade" ||
          ssEffect === "slide") &&
        !slideshowEffectEdited.value
      ) {
        slideshowEffect.value = ssEffect;
      }
      const gs = entries.get("generalSettings");
      if (gs && typeof gs === "object" && !generalSettingsEdited.value) {
        Object.assign(generalSettings, gs);
      }
      const sc = entries.get("shortcuts");
      if (sc && typeof sc === "object" && !shortcutsEdited.value) {
        Object.assign(shortcuts, sc);
      }
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  // 只写变化的那一项（plugin-store 没有批量 set），再统一落盘
  async function saveSettings(keys: Record<string, unknown>) {
    if (settingsReadOnly) {
      return;
    }
    try {
      for (const [key, value] of Object.entries(keys)) {
        await store.set(key, value);
      }
      await store.save();
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  }

  watch(topToolbarLocked, () => {
    topToolbarLockedEdited.value = true;
    saveSettings({ topToolbarLocked: topToolbarLocked.value });
  });

  watch(bottomFloatingBarLocked, () => {
    bottomFloatingBarLockedEdited.value = true;
    saveSettings({ bottomFloatingBarLocked: bottomFloatingBarLocked.value });
  });

  watch(exifPanelVisible, () => {
    exifPanelVisibleEdited.value = true;
    saveSettings({ exifPanelVisible: exifPanelVisible.value });
  });

  watch(slideshowInterval, () => {
    slideshowIntervalEdited.value = true;
    saveSettings({ slideshowInterval: slideshowInterval.value });
    resetSlideshowTimer();
  });

  watch(slideshowOrder, () => {
    slideshowOrderEdited.value = true;
    saveSettings({ slideshowOrder: slideshowOrder.value });
  });

  watch(slideshowEffect, () => {
    slideshowEffectEdited.value = true;
    saveSettings({ slideshowEffect: slideshowEffect.value });
  });

  watch(
    generalSettings,
    () => {
      generalSettingsEdited.value = true;
      saveSettings({ generalSettings: { ...generalSettings } });
    },
    { deep: true },
  );

  watch(
    shortcuts,
    () => {
      shortcutsEdited.value = true;
      saveSettings({ shortcuts: { ...shortcuts } });
    },
    { deep: true },
  );

  watch(files, () => {
    stopSlideshow();
  });

  loadSettings();

  return {
    files,
    currentIndex,
    dataUri,
    dataUri2,
    zoomMode,
    customZoom,
    groupIndices,
    sizeCache,
    topToolbarLocked,
    bottomFloatingBarLocked,
    exifPanelVisible,
    slideshowActive,
    slideshowInterval,
    slideshowOrder,
    slideshowEffect,
    generalSettings,
    shortcuts,
    toast,
    showToast,
    currentFile,
    isDual,
    isRtl,
    loadImage,
    loadGroup,
    openFile,
    openFolder,
    openImageByPath,
    goPrev,
    goNext,
    setZoomMode,
    setOriginalSize,
    startSlideshow,
    stopSlideshow,
    setSlideshowInterval,
    setSlideshowOrder,
    setSlideshowEffect,
    setShellContextMenu,
    resetSlideshowTimer,
    deleteCurrent,
  };
});
