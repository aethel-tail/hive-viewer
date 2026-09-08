<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useViewerStore } from "@/stores/viewer";
import { useBarVisibility } from "@/composables/useBarVisibility";
import { t } from "@/i18n";

const SLIDESHOW_INTERVALS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 20, 30, 60, 90];

const store = useViewerStore();
const { topToolbarLocked, slideshowActive, slideshowInterval, slideshowOrder, slideshowEffect } =
  storeToRefs(store);
const { visible, rootProps } = useBarVisibility(topToolbarLocked);

defineProps<{
  isFullscreen: boolean;
}>();

const emit = defineEmits<{
  toggleFullscreen: [];
  viewExif: [];
  openSettings: [];
  openConvert: [];
}>();
</script>

<template>
  <div class="top-toolbar" v-bind="rootProps" :class="{ hidden: !visible }">
    <!-- Left button group -->
    <div class="toolbar-group left-group">
      <button class="tb-btn exif-btn" :title="t('toolbar.viewExif')" @click="emit('viewExif')">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="3" y="5" width="18" height="14" rx="2" ry="2" />
          <circle cx="12" cy="10" r="2" />
          <line x1="8" y1="15" x2="16" y2="15" />
        </svg>
      </button>
      <button class="tb-btn exif-btn" :title="t('toolbar.settings')" @click="emit('openSettings')">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="12" cy="12" r="3" />
          <path
            d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
          />
        </svg>
      </button>
    </div>

    <span class="spacer"></span>

    <!-- Right button group: existing functionality -->
    <div class="toolbar-group right-group">
      <!-- Convert Format -->
      <button class="tb-btn" :title="t('convert.menu')" @click="emit('openConvert')">
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="17 1 21 5 17 9" />
          <path d="M3 11V9a4 4 0 0 1 4-4h14" />
          <polyline points="7 23 3 19 7 15" />
          <path d="M21 13v2a4 4 0 0 1-4 4H3" />
        </svg>
        {{ t("convert.menu") }}
      </button>

      <!-- Zoom Mode Dropdown -->
      <div class="dropdown">
        <button class="tb-btn">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
            <line x1="11" y1="8" x2="11" y2="14" />
            <line x1="8" y1="11" x2="14" y2="11" />
          </svg>
          {{ t("zoom.mode") }}
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            style="width: 12px; height: 12px"
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </button>
        <div class="dropdown-menu">
          <button :class="{ active: store.zoomMode === 'fit' }" @click="store.setZoomMode('fit')">
            {{ t("zoom.fit") }}<span class="check">&#10003;</span>
          </button>
          <button
            :class="{ active: store.zoomMode === 'width' }"
            @click="store.setZoomMode('width')"
          >
            {{ t("zoom.fitWidth") }}<span class="check">&#10003;</span>
          </button>
          <button
            :class="{ active: store.zoomMode === 'custom' && store.customZoom === 100 }"
            @click="store.setOriginalSize()"
          >
            {{ t("zoom.original") }}<span class="check">&#10003;</span>
          </button>
          <div class="dropdown-divider"></div>
          <button
            :class="{ active: store.zoomMode === 'dual-ltr' }"
            @click="store.setZoomMode('dual-ltr')"
          >
            {{ t("zoom.dualLtr") }}<span class="check">&#10003;</span>
          </button>
          <button
            :class="{ active: store.zoomMode === 'dual-rtl' }"
            @click="store.setZoomMode('dual-rtl')"
          >
            {{ t("zoom.dualRtl") }}<span class="check">&#10003;</span>
          </button>
        </div>
      </div>

      <!-- Slideshow Dropdown -->
      <div class="dropdown">
        <button class="tb-btn">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
            <polygon points="9 8 17 12 9 16 9 8" />
          </svg>
          {{ t("slideshow.title") }}
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            style="width: 12px; height: 12px"
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </button>
        <div class="dropdown-menu slideshow-menu">
          <div class="dropdown-section">
            <button :class="{ active: !slideshowActive }" @click="store.stopSlideshow()">
              {{ t("slideshow.stop") }}
              <span class="check">&#10003;</span>
            </button>
          </div>

          <div class="dropdown-section">
            <div class="section-label">{{ t("slideshow.interval") }}</div>
            <div class="interval-grid">
              <button
                v-for="n in SLIDESHOW_INTERVALS"
                :key="n"
                :class="{ active: slideshowActive && slideshowInterval === n }"
                @click="
                  store.startSlideshow();
                  store.setSlideshowInterval(n);
                "
              >
                {{ t("slideshow.seconds", { n }) }}
              </button>
            </div>
          </div>

          <div class="dropdown-section">
            <button
              :class="{ active: slideshowOrder === 'loop' }"
              @click="store.setSlideshowOrder('loop')"
            >
              {{ t("slideshow.loop") }}
              <span class="check">&#10003;</span>
            </button>
            <button
              :class="{ active: slideshowOrder === 'random' }"
              @click="store.setSlideshowOrder('random')"
            >
              {{ t("slideshow.random") }}
              <span class="check">&#10003;</span>
            </button>
          </div>

          <div class="dropdown-section">
            <div class="section-label">{{ t("slideshow.effect") }}</div>
            <button
              :class="{ active: slideshowEffect === 'none' }"
              @click="store.setSlideshowEffect('none')"
            >
              {{ t("slideshow.effectNone") }}
              <span class="check">&#10003;</span>
            </button>
            <button
              :class="{ active: slideshowEffect === 'flip' }"
              @click="store.setSlideshowEffect('flip')"
            >
              {{ t("slideshow.effectFlip") }}
              <span class="check">&#10003;</span>
            </button>
            <button
              :class="{ active: slideshowEffect === 'fade' }"
              @click="store.setSlideshowEffect('fade')"
            >
              {{ t("slideshow.effectFade") }}
              <span class="check">&#10003;</span>
            </button>
            <button
              :class="{ active: slideshowEffect === 'slide' }"
              @click="store.setSlideshowEffect('slide')"
            >
              {{ t("slideshow.effectSlide") }}
              <span class="check">&#10003;</span>
            </button>
          </div>
        </div>
      </div>

      <button
        class="tb-btn lock-toggle"
        :class="{ locked: topToolbarLocked }"
        @click="topToolbarLocked = !topToolbarLocked"
        :title="t('toolbar.lock')"
      >
        <svg
          v-if="topToolbarLocked"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="5" y="11" width="14" height="10" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        <svg
          v-else
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <rect x="5" y="11" width="14" height="10" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 9.9-1" />
        </svg>
        <span>{{ t("toolbar.lockLabel") }}</span>
      </button>

      <button
        class="tb-btn fullscreen-toggle"
        :title="isFullscreen ? t('toolbar.exitFullscreen') : t('toolbar.fullscreen')"
        @click="emit('toggleFullscreen')"
      >
        <svg
          v-if="!isFullscreen"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M8 3H5a2 2 0 0 0-2 2v3" />
          <path d="M16 3h3a2 2 0 0 1 2 2v3" />
          <path d="M8 21H5a2 2 0 0 1-2-2v-3" />
          <path d="M16 21h3a2 2 0 0 0 2-2v-3" />
        </svg>
        <svg
          v-else
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M4 8V5a2 2 0 0 1 2-2h3" />
          <path d="M20 8V5a2 2 0 0 0-2-2h-3" />
          <path d="M4 16v3a2 2 0 0 0 2 2h3" />
          <path d="M20 16v3a2 2 0 0 1-2 2h-3" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.top-toolbar {
  position: relative;
  display: flex;
  flex-shrink: 0;
  gap: 10px;
  align-items: center;
  padding: 10px 16px;
  background: linear-gradient(180deg, var(--panel-a) 0%, var(--panel-b) 100%);
  border-bottom: 1px solid var(--border-soft);
  box-shadow:
    0 1px 0 var(--highlight) inset,
    0 2px 8px rgb(220 190 140 / 15%),
    0 4px 16px rgb(60 30 10 / 6%);
  backdrop-filter: blur(12px) saturate(1.3);
  transition:
    opacity 250ms ease-out,
    transform 250ms ease-out;
}

.top-toolbar::after {
  position: absolute;
  right: 0;
  bottom: -3px;
  left: 0;
  height: 3px;
  content: "";
  background: linear-gradient(180deg, rgb(240 220 170 / 25%) 0%, transparent 100%);
}

.top-toolbar .spacer {
  flex: 1;
}

.tb-btn {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 6px 14px;
  font-family: var(--font);
  font-size: 0.82rem;
  color: var(--fg);
  white-space: nowrap;
  cursor: pointer;
  background: var(--surface-soft);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-sm);
  isolation: isolate;
  backdrop-filter: blur(8px);
}

/* hover 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.tb-btn::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.tb-btn:hover::before {
  opacity: 1;
}

.tb-btn svg {
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  color: var(--fg-muted);
  filter: drop-shadow(0 1px 2px var(--glow));
}

.dropdown {
  position: relative;
}

.dropdown::after {
  position: absolute;
  top: 100%;
  left: 0;
  width: 100%;
  height: 6px;
  content: "";
}

.dropdown-menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 10;
  visibility: hidden;
  min-width: 200px;
  padding: 4px;
  pointer-events: none;
  background: var(--surface-solid);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  /* 仅 transform/opacity 动画；visibility 延时切换让关闭后不可交互 */
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
  transform-origin: top left;
  transition:
    opacity 140ms ease-out,
    transform 140ms ease-out,
    visibility 0s linear 140ms;
}

.dropdown:hover .dropdown-menu {
  visibility: visible;
  pointer-events: auto;
  opacity: 1;
  transform: translateY(0) scale(1);
  transition:
    opacity 140ms ease-out,
    transform 140ms ease-out;
}

.dropdown-divider {
  height: 1px;
  margin: 4px 8px;
  background: var(--border-alpha);
}

.dropdown-menu button {
  display: flex;
  gap: 8px;
  align-items: center;
  width: 100%;
  padding: 8px 12px;
  font-family: var(--font);
  font-size: 0.82rem;
  color: var(--fg);
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  isolation: isolate;
}

.dropdown-menu button::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 100ms;
}

.dropdown-menu button:not(.active):hover::before {
  opacity: 1;
}

.dropdown-menu button.active {
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-soft);
}

.dropdown-menu .check {
  margin-left: auto;
  opacity: 0;
}

.dropdown-menu button.active .check {
  opacity: 1;
}

.slideshow-menu {
  min-width: 200px;
  padding: 6px;
}

.dropdown-section {
  padding: 4px 0;
  border-bottom: 1px solid var(--border-alpha);
}

.dropdown-section:first-child {
  padding-top: 0;
}

.dropdown-section:last-child {
  padding-bottom: 0;
  border-bottom: none;
}

.dropdown-section .section-label {
  padding: 6px 10px 4px;
  font-size: 0.72rem;
  font-weight: 600;
  color: var(--fg-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  user-select: none;
}

.dropdown-section .interval-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 4px;
  padding: 0 4px 4px;
}

.dropdown-section .interval-grid button {
  justify-content: center;
  padding: 6px 0;
  font-size: 0.75rem;
  text-align: center;
}

@media (width <= 640px) {
  .top-toolbar {
    gap: 6px;
    padding: 8px 10px;
  }

  .tb-btn {
    padding: 5px 10px;
    font-size: 0.78rem;
  }
}

.top-toolbar.hidden {
  opacity: 0;
  transform: translateY(-12px);
  transition-timing-function: ease-in;
}

.lock-toggle {
  color: var(--fg-muted);
}

.lock-toggle.locked {
  color: var(--fg);
}

.lock-toggle svg {
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  color: var(--fg-muted);
  filter: drop-shadow(0 1px 2px var(--glow));
}

.lock-toggle.locked svg {
  color: var(--fg);
}

.lock-toggle:hover {
  background: var(--muted);
}

.fullscreen-toggle {
  color: var(--fg-muted);
}

.fullscreen-toggle svg {
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  color: var(--fg-muted);
  filter: drop-shadow(0 1px 2px var(--glow));
}

.fullscreen-toggle:hover {
  color: var(--fg);
  background: var(--muted);
}

.toolbar-group {
  display: flex;
  gap: 10px;
  align-items: center;
}

.exif-btn {
  padding: 6px 10px;
  color: var(--fg-muted);
}

.exif-btn:hover {
  color: var(--fg);
}

.exif-btn svg {
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  color: inherit;
  filter: drop-shadow(0 1px 2px var(--glow));
}

@media (width <= 640px) {
  .toolbar-group {
    gap: 6px;
  }

  .exif-btn {
    padding: 5px 8px;
  }
}
</style>
