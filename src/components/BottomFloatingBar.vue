<script setup lang="ts">
import { computed } from "vue";
import { storeToRefs } from "pinia";
import { useViewerStore } from "@/stores/viewer";
import { useBarVisibility } from "@/composables/useBarVisibility";
import { t } from "@/i18n";

const store = useViewerStore();
const { files, currentIndex, bottomFloatingBarLocked } = storeToRefs(store);
const { visible, rootProps } = useBarVisibility(bottomFloatingBarLocked);

defineProps<{
  displayZoom: number;
}>();

const emit = defineEmits<{
  openFile: [];
  openFolder: [];
  zoomIn: [];
  zoomOut: [];
  rotateClockwise: [];
  rotateCounterClockwise: [];
  deleteFile: [];
}>();

const progressPercent = computed(() => {
  const total = files.value.length;
  if (total <= 1) {
    return 0;
  }
  return (currentIndex.value / (total - 1)) * 100;
});
</script>

<template>
  <div class="bottom-float-bar" v-bind="rootProps" :class="{ hidden: !visible }">
    <button class="bfb-btn open-btn" :title="t('app.openImage')" @click="emit('openFile')">
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
    </button>

    <button class="bfb-btn open-btn" :title="t('app.openFolder')" @click="emit('openFolder')">
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
    </button>

    <div class="bfb-center">
      <div class="progress-row">
        <div class="progress-track">
          <div
            class="progress-fill"
            :style="{ transform: `scaleX(${progressPercent / 100})` }"
          ></div>
        </div>
      </div>

      <div class="bottom-row">
        <div class="zoom-row">
          <button class="bfb-btn" :title="t('toolbar.zoomOut')" @click="emit('zoomOut')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
              <line x1="8" y1="11" x2="14" y2="11" />
            </svg>
          </button>

          <span class="zoom-label">{{ displayZoom }}%</span>

          <button class="bfb-btn" :title="t('toolbar.zoomIn')" @click="emit('zoomIn')">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
              <line x1="11" y1="8" x2="11" y2="14" />
              <line x1="8" y1="11" x2="14" y2="11" />
            </svg>
          </button>
        </div>

        <div class="divider"></div>

        <div class="rotate-row">
          <button
            class="bfb-btn"
            :title="t('toolbar.rotateCcw')"
            @click="emit('rotateCounterClockwise')"
          >
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
              <path d="M3 3v5h5" />
            </svg>
          </button>

          <button class="bfb-btn" :title="t('toolbar.rotateCw')" @click="emit('rotateClockwise')">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 12a9 9 0 1 1-9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
              <path d="M21 3v5h-5" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <button class="bfb-btn delete-btn" :title="t('toolbar.delete')" @click="emit('deleteFile')">
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M3 6h18" />
        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
        <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
        <line x1="10" y1="11" x2="10" y2="17" />
        <line x1="14" y1="11" x2="14" y2="17" />
      </svg>
    </button>

    <button
      class="bfb-btn lock-toggle"
      :class="{ locked: bottomFloatingBarLocked }"
      @click="bottomFloatingBarLocked = !bottomFloatingBarLocked"
      :title="t('toolbar.lock')"
    >
      <svg
        v-if="bottomFloatingBarLocked"
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
    </button>
  </div>
</template>

<style scoped>
.bottom-float-bar {
  position: absolute;
  bottom: 15%;
  left: 50%;
  display: flex;
  gap: 6px;
  align-items: center;
  padding: 5px 8px;
  pointer-events: auto;
  background: linear-gradient(135deg, var(--panel-a) 0%, var(--panel-b) 100%);
  border: 1px solid var(--border-soft);
  border-radius: 999px;
  box-shadow:
    0 0 6px rgb(240 210 150 / 25%),
    0 0 18px rgb(230 200 140 / 15%),
    0 0 36px rgb(220 190 130 / 8%),
    0 4px 24px rgb(60 30 10 / 10%),
    inset 0 1px 0 var(--highlight),
    inset 0 0 8px rgb(240 220 170 / 15%);
  backdrop-filter: blur(12px) saturate(1.3);
  transform: translateX(-50%);
  transition:
    opacity 250ms ease-out,
    transform 250ms ease-out;
}

.bottom-float-bar.hidden {
  opacity: 0;
  transform: translate(-50%, 12px);
  transition-timing-function: ease-in;
}

.bfb-center {
  display: flex;
  flex-direction: column;
  gap: 5px;
  align-items: center;
  min-width: 293px;
  padding: 16px 6px 0;
}

.progress-row {
  display: flex;
  gap: 10px;
  align-items: center;
  width: 100%;
}

.progress-track {
  flex: 1;
  height: 6px;
  overflow: hidden;
  background: var(--border);
  border: 1px solid var(--border-alpha);
  border-radius: 999px;
  box-shadow: inset 0 1px 2px rgb(80 60 30 / 15%);
}

.progress-fill {
  width: 100%;
  height: 100%;
  background: var(--accent);
  border-radius: 999px;
  transform-origin: left center;
  transition: transform 200ms ease-out;
  will-change: transform;
}

.zoom-row {
  display: flex;
  gap: 4px;
  align-items: center;
}

.bottom-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.divider {
  width: 1px;
  height: 20px;
  background: var(--border-alpha);
}

.rotate-row {
  display: flex;
  gap: 4px;
  align-items: center;
}

.bfb-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  color: var(--fg-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: 50%;
  isolation: isolate;
  transition: color 120ms;
}

/* hover / active 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.bfb-btn::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 120ms;
}

.bfb-btn:hover::before {
  opacity: 1;
}

.bfb-btn:active::before {
  background: var(--border);
}

.bfb-btn:hover {
  color: var(--fg);
}

.bfb-btn svg {
  width: 18px;
  height: 18px;
  filter: drop-shadow(0 1px 2px var(--glow));
}

.open-btn {
  width: 38px;
  height: 38px;
}

.zoom-label {
  min-width: 44px;
  font-size: 0.75rem;
  font-variant-numeric: tabular-nums;
  color: var(--fg-muted);
  text-align: center;
  text-shadow: 0 1px 3px var(--glow);
  user-select: none;
}

.delete-btn {
  width: 38px;
  height: 38px;
  color: var(--fg-muted);
}

.delete-btn:hover {
  color: var(--danger);
  background: var(--danger-soft);
}

.lock-toggle {
  width: 38px;
  height: 38px;
  color: var(--fg-muted);
}

.lock-toggle.locked {
  color: var(--fg);
}

.lock-toggle svg {
  width: 18px;
  height: 18px;
  filter: drop-shadow(0 1px 2px var(--glow));
}

.lock-toggle.locked svg {
  color: var(--fg);
}

.lock-toggle:hover {
  color: var(--fg);
  background: var(--muted);
}

.lock-toggle.locked:hover {
  color: var(--fg);
}

@media (width <= 640px) {
  .bottom-float-bar {
    bottom: 12%;
    gap: 8px;
    padding: 8px 10px;
  }

  .bfb-center {
    min-width: 160px;
  }

  .bfb-btn {
    width: 30px;
    height: 30px;
  }

  .open-btn {
    width: 34px;
    height: 34px;
  }

  .delete-btn {
    width: 34px;
    height: 34px;
  }

  .lock-toggle {
    width: 34px;
    height: 34px;
  }
}
</style>
