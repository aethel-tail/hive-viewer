<script setup lang="ts">
import { reactive, ref, computed, watch, onMounted, onUnmounted } from "vue";
import { useViewerStore, DEFAULT_GENERAL_SETTINGS, DEFAULT_SHORTCUTS } from "@/stores/viewer";
import { t, type Locale, type MessageKey } from "@/i18n";
import Checkbox from "@/components/Checkbox.vue";
import Select from "@/components/Select.vue";

const props = defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const store = useViewerStore();

// 草稿：面板内编辑，点「确定」才提交到 store 并持久化，点「重置」回默认
const draft = reactive({ ...DEFAULT_GENERAL_SETTINGS });

watch(
  () => props.visible,
  (v) => {
    if (v) {
      Object.assign(draft, store.generalSettings);
    }
  },
);

// 选主题立即生效并持久化，无需点「确定」
watch(
  () => draft.theme,
  (th) => {
    store.generalSettings.theme = th;
  },
);

// 选语言同样立即生效（由 App 同步到 i18n 模块，随 generalSettings 持久化）
watch(
  () => draft.language,
  (l) => {
    store.generalSettings.language = l;
  },
);

// 选字体同样立即生效（'' = 默认链，由 App 覆盖/还原 --font）
watch(
  () => draft.fontFamily,
  (f) => {
    store.generalSettings.fontFamily = f;
  },
);

const activeTab = ref("general");

const TABS = ["general", "theme", "shortcuts", "context", "language"] as const;
type TabId = (typeof TABS)[number];
const tabLabel = (id: TabId) => t(`settings.tabs.${id}` as MessageKey);

type EndReachAction = typeof DEFAULT_GENERAL_SETTINGS.endReachAction;
const endReachOptions = computed<{ value: EndReachAction; label: string }[]>(() => [
  { value: "loop", label: t("settings.general.endLoop") },
  { value: "none", label: t("settings.general.endNone") },
  { value: "ask", label: t("settings.general.endAsk") },
]);

// 快捷键列表（预设，可点击重新录制）
const SHORTCUT_ITEMS = [
  "prev",
  "next",
  "rotateCw",
  "rotateCcw",
  "fullscreen",
  "fullscreenEnter",
  "toggleExif",
  "stopSlideshow",
  "slideshowSpeed",
] as const;

type ShortcutId = (typeof SHORTCUT_ITEMS)[number];
const shortcutLabel = (id: ShortcutId) => t(`settings.shortcuts.${id}` as MessageKey);

const recording = ref<ShortcutId | null>(null);
const conflictMsg = ref("");

const KEY_LABELS: Record<string, string> = {
  arrowleft: "←",
  arrowright: "→",
  arrowup: "↑",
  arrowdown: "↓",
  escape: "Esc",
  f11: "F11",
  enter: "Enter",
  tab: "Tab",
  digit: "1~9",
};

function formatKey(spec: string): string {
  const parts = spec.split("+");
  const key = parts.pop() ?? "";
  const mods = parts.map((m) => ({ ctrl: "Ctrl", alt: "Alt", shift: "Shift" })[m] ?? m);
  const keyLabel =
    KEY_LABELS[key] ??
    (key === " " ? t("settings.shortcuts.space") : key.length === 1 ? key.toUpperCase() : key);
  return [...mods, keyLabel].join(" + ");
}

function startRecord(id: ShortcutId) {
  recording.value = id;
  conflictMsg.value = "";
}

// capture 阶段拦截按键：录制时不让全局快捷键/浏览器默认动作生效
function onRecordKeydown(e: KeyboardEvent) {
  if (recording.value === null) {
    return;
  }
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    recording.value = null;
    return;
  }
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) {
    return;
  }
  const mods = e as unknown as Record<string, boolean>;
  const spec = [
    ...["ctrl", "alt", "shift"].filter((m) => mods[m + "Key"]),
    e.key.toLowerCase(),
  ].join("+");
  const clash = (Object.entries(store.shortcuts) as [ShortcutId, string][]).find(
    ([k, v]) => v === spec && k !== recording.value,
  );
  if (clash) {
    conflictMsg.value = t("settings.shortcuts.conflict", { name: shortcutLabel(clash[0]) });
    return;
  }
  store.shortcuts[recording.value] = spec;
  conflictMsg.value = "";
  recording.value = null;
}

onMounted(() => {
  document.addEventListener("keydown", onRecordKeydown, true);
});

onUnmounted(() => {
  document.removeEventListener("keydown", onRecordKeydown, true);
});

// 主题：选中的瞬间即生效（直接提交 store，由 App 应用并持久化）
const THEMES = [
  { id: "system", colors: ["#E6E4DE", "#141416", "#4A4A52", "#4A3025"] },
  { id: "dark", colors: ["#E09A4B", "#141416", "#4A4A52", "#E6E4DE"] },
  { id: "light", colors: ["#C77826", "#FFFBF0", "#D4C5B2", "#4A3025"] },
  { id: "warm", colors: ["#C77826", "#FFF6E2", "#D4C5B2", "#4A3025"] },
  { id: "night", colors: ["#7C9CF0", "#1E2430", "#3A4356", "#E8ECF4"] },
  { id: "forest", colors: ["#3E7C4F", "#F2F8F0", "#B8CFB4", "#2C3E2E"] },
  { id: "ocean", colors: ["#2E7DA6", "#EFF7FB", "#A8C8DA", "#23485C"] },
  { id: "sakura", colors: ["#D96A8B", "#FDF3F6", "#E8C2CE", "#5C3A44"] },
  { id: "graphite", colors: ["#6B7280", "#F4F4F5", "#C9CDD4", "#33363B"] },
] as const;
type ThemeId = (typeof THEMES)[number]["id"];
const themeName = (id: ThemeId) => t(`settings.themes.${id}` as MessageKey);

// 全局字体下拉：首次点击时枚举。优先 queryLocalFonts（需用户手势，WebView2 可能拒权限），
// 失败/为空则回退 canvas 测量法探测常见字体。
const fontList = ref<string[]>([]);
let fontsLoaded = false;

const fontOptions = computed(() => [
  { value: "", label: t("settings.themes.fontDefault") },
  ...fontList.value.map((f) => ({ value: f, label: f })),
]);

const FONT_PROBES = [
  "Source Han Sans SC",
  "Source Han Sans CN",
  "Noto Sans CJK SC",
  "Noto Sans SC",
  "Source Han Serif SC",
  "Noto Serif CJK SC",
  "PingFang SC",
  "PingFang TC",
  "Hiragino Sans GB",
  "Microsoft YaHei",
  "Microsoft YaHei UI",
  "Microsoft JhengHei",
  "SimSun",
  "SimHei",
  "KaiTi",
  "FangSong",
  "DengXian",
  "Segoe UI",
  "Inter",
  "Arial",
  "Consolas",
  "Verdana",
];

function probeFonts(): string[] {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) {
    return [];
  }
  const text = "mmmmmmmmmmlli字体";
  ctx.font = "72px monospace";
  const base = ctx.measureText(text).width;
  return FONT_PROBES.filter((f) => {
    ctx.font = `72px "${f}", monospace`;
    return ctx.measureText(text).width !== base;
  });
}

async function loadFonts() {
  if (fontsLoaded) {
    return;
  }
  fontsLoaded = true;
  try {
    const q = (window as unknown as { queryLocalFonts?: () => Promise<{ family: string }[]> })
      .queryLocalFonts;
    const fonts = q ? await q.call(window) : [];
    const fams = [...new Set(fonts.map((f) => f.family))].sort((a, b) => a.localeCompare(b));
    fontList.value = fams.length ? fams : probeFonts();
  } catch {
    fontList.value = probeFonts();
  }
}

// 语言名固定用各语言原生写法，不随界面语言变化
const LANGUAGES: { id: Locale; name: string }[] = [
  { id: "zh-CN", name: "简体中文" },
  { id: "zh-TW", name: "繁體中文" },
  { id: "en", name: "English" },
  { id: "ja", name: "日本語" },
];

function resetDraft() {
  Object.assign(draft, DEFAULT_GENERAL_SETTINGS);
  Object.assign(store.shortcuts, DEFAULT_SHORTCUTS);
}

function confirm() {
  const shellChanged = draft.shellContextMenu !== store.generalSettings.shellContextMenu;
  Object.assign(store.generalSettings, draft);
  // 总开关变化时才动 sparse 包；子项开关由 shell-ext 读设置文件自行隐藏
  if (shellChanged) {
    void store.setShellContextMenu(draft.shellContextMenu);
  }
  emit("close");
}
</script>

<template>
  <div class="settings-overlay" :class="{ visible }" @click.self="emit('close')">
    <div class="settings-panel">
      <header class="settings-header">
        <span class="settings-title">{{ t("settings.title") }}</span>
        <button class="settings-close" :title="t('settings.close')" @click="emit('close')">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </header>

      <div class="settings-body">
        <nav class="settings-tabs">
          <button
            v-for="tab in TABS"
            :key="tab"
            :class="{ active: activeTab === tab }"
            @click="activeTab = tab"
          >
            {{ tabLabel(tab) }}
          </button>
        </nav>

        <div class="settings-content">
          <!-- 一般 -->
          <section v-show="activeTab === 'general'" class="tab-pane">
            <div class="setting-row">
              <span class="setting-label">{{ t("settings.general.allowMultipleInstances") }}</span>
              <Checkbox v-model="draft.allowMultipleInstances" />
            </div>
            <div class="setting-row">
              <span class="setting-label">{{ t("settings.general.confirmDelete") }}</span>
              <Checkbox v-model="draft.confirmDelete" />
            </div>
            <div class="setting-row">
              <span class="setting-label">{{ t("settings.general.alwaysOnTop") }}</span>
              <Checkbox v-model="draft.alwaysOnTop" />
            </div>
            <div class="setting-row">
              <span class="setting-label">{{ t("settings.general.escToExit") }}</span>
              <Checkbox v-model="draft.escToExit" />
            </div>
            <div class="setting-row">
              <span class="setting-label">{{ t("settings.general.endReachAction") }}</span>
              <Select v-model="draft.endReachAction" :options="endReachOptions" />
            </div>
          </section>

          <!-- 主题 -->
          <section v-show="activeTab === 'theme'" class="tab-pane">
            <!-- 第一行：跟随系统 / 黑 / 白 + 全局字体下拉（占第 4 格） -->
            <div class="theme-grid">
              <button
                v-for="th in THEMES.slice(0, 3)"
                :key="th.id"
                class="theme-card"
                :class="{ active: draft.theme === th.id }"
                @click="draft.theme = th.id"
              >
                <div class="theme-swatches">
                  <span v-for="c in th.colors" :key="c" :style="{ background: c }"></span>
                </div>
                <span class="theme-name">{{ themeName(th.id) }}</span>
              </button>
              <Select
                v-model="draft.fontFamily"
                :options="fontOptions"
                class="font-select"
                @pointerdown="loadFonts"
              />
            </div>
            <div class="theme-divider"></div>
            <!-- 配色方案 -->
            <div class="theme-grid">
              <button
                v-for="th in THEMES.slice(3)"
                :key="th.id"
                class="theme-card"
                :class="{ active: draft.theme === th.id }"
                @click="draft.theme = th.id"
              >
                <div class="theme-swatches">
                  <span v-for="c in th.colors" :key="c" :style="{ background: c }"></span>
                </div>
                <span class="theme-name">{{ themeName(th.id) }}</span>
              </button>
            </div>
          </section>
          <section v-show="activeTab === 'shortcuts'" class="tab-pane">
            <div class="key-list">
              <button
                v-for="s in SHORTCUT_ITEMS"
                :key="s"
                class="key-row shortcut-row"
                :class="{ recording: recording === s }"
                @click="startRecord(s)"
              >
                <span>{{ shortcutLabel(s) }}</span>
                <kbd v-if="recording !== s">{{ formatKey(store.shortcuts[s]) }}</kbd>
                <kbd v-else class="recording-hint">{{ t("settings.shortcuts.recording") }}</kbd>
              </button>
            </div>
            <p v-if="conflictMsg" class="tab-hint conflict">{{ conflictMsg }}</p>
            <p class="tab-hint">{{ t("settings.shortcuts.hint") }}</p>
          </section>

          <!-- 上下文菜单（Windows 11 右键菜单） -->
          <section v-show="activeTab === 'context'" class="tab-pane">
            <div class="setting-row">
              <span class="setting-label">{{ t("settings.context.useInExplorer") }}</span>
              <Checkbox v-model="draft.shellContextMenu" />
            </div>
            <div class="sub-settings">
              <div class="setting-row">
                <span class="setting-label">{{ t("settings.context.browse") }}</span>
                <Checkbox
                  v-model="draft.shellContextMenuOpen"
                  :disabled="!draft.shellContextMenu"
                />
              </div>
              <div class="setting-row">
                <span class="setting-label">{{ t("settings.context.convert") }}</span>
                <Checkbox
                  v-model="draft.shellContextMenuConvert"
                  :disabled="!draft.shellContextMenu"
                />
              </div>
            </div>
            <p class="tab-hint">{{ t("settings.context.hint") }}</p>
          </section>

          <!-- 语言 -->
          <section v-show="activeTab === 'language'" class="tab-pane">
            <div class="key-list">
              <button
                v-for="l in LANGUAGES"
                :key="l.id"
                class="key-row shortcut-row"
                :class="{ current: draft.language === l.id }"
                @click="draft.language = l.id"
              >
                <span>{{ l.name }}</span>
                <kbd v-if="draft.language === l.id">{{ t("settings.language.current") }}</kbd>
              </button>
            </div>
          </section>
        </div>
      </div>

      <footer class="settings-footer">
        <button class="footer-btn reset-btn" @click="resetDraft">{{ t("settings.reset") }}</button>
        <button class="footer-btn confirm-btn" @click="confirm">{{ t("settings.confirm") }}</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.settings-overlay {
  position: fixed;
  inset: 0;
  z-index: 30;
  display: flex;
  visibility: hidden;
  align-items: center;
  justify-content: center;
  background: rgb(60 30 10 / 35%);
  opacity: 0;
  backdrop-filter: blur(3px);
  transition:
    opacity 200ms ease-out,
    visibility 0s linear 200ms;
}

.settings-overlay.visible {
  visibility: visible;
  opacity: 1;
  transition: opacity 200ms ease-out;
}

.settings-panel {
  display: flex;
  flex-direction: column;
  width: 800px;
  max-width: calc(100vw - 48px);
  height: min(480px, calc(100vh - 96px));
  background: linear-gradient(180deg, var(--panel-a) 0%, var(--panel-b) 100%);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-lg);
  box-shadow:
    0 0 0 1px var(--highlight) inset,
    var(--shadow-md);
  backdrop-filter: blur(24px) saturate(1.3);
  transform: scale(0.95);
  transition: transform 200ms ease-out;
}

.settings-overlay.visible .settings-panel {
  transform: scale(1);
}

.settings-header {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px;
  border-bottom: 1px solid var(--border-soft);
}

.settings-title {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--fg);
}

.settings-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  color: var(--fg-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  isolation: isolate;
  transition: color 150ms;
}

/* hover 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.settings-close::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.settings-close:hover::before {
  opacity: 1;
}

.settings-close:hover {
  color: var(--fg);
}

.settings-close svg {
  width: 16px;
  height: 16px;
}

.settings-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.settings-tabs {
  display: flex;
  flex-shrink: 0;
  flex-direction: column;
  gap: 2px;
  width: 132px;
  padding: 12px 8px;
  border-right: 1px solid var(--border-soft);
}

.settings-tabs button {
  padding: 9px 12px;
  font-family: var(--font);
  font-size: 0.85rem;
  color: var(--fg-muted);
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  isolation: isolate;
  transition: color 150ms;
}

.settings-tabs button::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.settings-tabs button:not(.active):hover::before {
  opacity: 1;
}

.settings-tabs button:hover {
  color: var(--fg);
}

.settings-tabs button.active {
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-soft);
}

.settings-content {
  flex: 1;
  padding: 18px 20px;
  overflow-y: auto;
}

.tab-pane {
  display: flex;
  flex-direction: column;
}

.setting-row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  padding: 11px 4px;
  border-bottom: 1px solid var(--border-alpha);
}

.setting-label {
  font-size: 0.86rem;
  color: var(--fg);
}

.sub-settings {
  padding-left: 20px;
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}

.theme-divider {
  height: 1px;
  margin: 14px 0;
  background: var(--border-alpha);
}

.theme-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
  padding: 12px;
  font-family: var(--font);
  cursor: pointer;
  background: var(--surface-soft);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  transition: border-color 150ms;
}

.theme-card:hover {
  border-color: var(--accent);
}

.theme-card.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent) inset;
}

.theme-swatches {
  display: flex;
  gap: 4px;
}

.theme-swatches span {
  width: 16px;
  height: 16px;
  border: 1px solid rgb(0 0 0 / 8%);
  border-radius: 5px;
}

/* 首行第 4 格：全局字体下拉（限宽，长字体名省略号兜底） */

.font-select {
  align-self: center;
  width: 100%;
}

.font-select :deep(.select-trigger) {
  justify-content: space-between;
  width: 100%;
}

.font-select :deep(.select-value) {
  overflow: hidden;
  text-overflow: ellipsis;
}

.theme-name {
  font-size: 0.8rem;
  color: var(--fg);
}

.key-list {
  display: flex;
  flex-direction: column;
}

.key-row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  padding: 11px 4px;
  font-size: 0.85rem;
  color: var(--fg);
  border-bottom: 1px solid var(--border-alpha);
}

.key-row kbd {
  padding: 2px 10px;
  font-family: var(--font);
  font-size: 0.75rem;
  color: var(--fg-muted);
  white-space: nowrap;
  background: var(--surface-strong);
  border: 1px solid var(--border-alpha);
  border-radius: 5px;
}

.key-row.current kbd {
  color: var(--accent);
  border-color: var(--accent);
}

.shortcut-row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 11px 4px;
  font-family: var(--font);
  font-size: 0.85rem;
  color: var(--fg);
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border-alpha);
  isolation: isolate;
}

/* hover 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.shortcut-row::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 100ms;
}

.shortcut-row:not(.recording):hover::before {
  opacity: 1;
}

.shortcut-row.recording {
  background: var(--accent-soft);
}

.shortcut-row.recording .recording-hint {
  color: var(--accent);
  border-color: var(--accent);
}

.tab-hint.conflict {
  color: var(--danger);
}

.tab-hint {
  margin-top: 14px;
  font-size: 0.76rem;
  color: var(--fg-muted);
}

.settings-footer {
  display: flex;
  flex-shrink: 0;
  gap: 10px;
  justify-content: flex-end;
  padding: 12px 16px;
  border-top: 1px solid var(--border-soft);
}

.footer-btn {
  padding: 7px 24px;
  font-family: var(--font);
  font-size: 0.84rem;
  cursor: pointer;
  border-radius: var(--radius-sm);
  isolation: isolate;
}

.footer-btn::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.footer-btn:hover::before {
  opacity: 1;
}

.reset-btn {
  color: var(--fg-muted);
  background: transparent;
  border: 1px solid var(--border-alpha);
}

.reset-btn:hover {
  color: var(--fg);
}

.confirm-btn {
  color: #fff;
  background: var(--accent);
  border: 1px solid transparent;
}

/* 确定键 hover 是 accent → accent-hover 的底色变化，改覆盖层颜色 */

.confirm-btn::before {
  background: var(--accent-hover);
}

@media (width <= 640px) {
  .settings-panel {
    width: 100vw;
    max-width: none;
    max-height: 100vh;
    border: none;
    border-radius: 0;
  }

  .theme-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}
</style>
