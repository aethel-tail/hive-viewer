<script setup lang="ts">
import { computed } from "vue";
import { t, te, type MessageKey } from "@/i18n";

const props = defineProps<{
  data: Record<string, string>;
  visible: boolean;
}>();

// 后端返回的 key 即 EXIF tag 名；这里按期望顺序排列
const ORDERED_KEYS = [
  "Make",
  "Model",
  "LensMake",
  "LensModel",
  "LensSpecification",
  "Software",
  "DateTimeOriginal",
  "DateTimeDigitized",
  "DateTime",
  "GPSTimeStamp",
  "GPSDateStamp",
  "ImageWidth",
  "ImageLength",
  "PixelXDimension",
  "PixelYDimension",
  "Orientation",
  "ISOSpeedRatings",
  "PhotographicSensitivity",
  "ExposureTime",
  "ShutterSpeedValue",
  "FNumber",
  "ApertureValue",
  "MaxApertureValue",
  "ExposureProgram",
  "ExposureBiasValue",
  "MeteringMode",
  "Flash",
  "BrightnessValue",
  "SubjectDistance",
  "FocalLength",
  "FocalLengthIn35mmFilm",
  "LightSource",
  "ColorSpace",
  "XResolution",
  "YResolution",
  "ResolutionUnit",
  "Artist",
  "Copyright",
  "UserComment",
  "GPSLatitude",
  "GPSLongitude",
  "GPSAltitude",
  "MakerNote",
];

// tag 名 → 本地化标签；无翻译时显示 tag 原名
function label(key: string): string {
  const k = `exif.${key}`;
  return te(k) ? t(k as MessageKey) : key;
}

const orderedEntries = computed(() => {
  const ordered: { key: string; value: string }[] = [];
  const seen = new Set<string>();

  for (const key of ORDERED_KEYS) {
    if (props.data[key] !== undefined) {
      ordered.push({ key, value: props.data[key] });
      seen.add(key);
    }
  }

  for (const [key, value] of Object.entries(props.data)) {
    if (!seen.has(key)) {
      ordered.push({ key, value });
    }
  }

  return ordered;
});

function copyMakerNote(value: string) {
  navigator.clipboard.writeText(value).catch((err) => {
    console.error("Copy failed:", err);
  });
}
</script>

<template>
  <div class="exif-panel" :class="{ visible }">
    <div class="exif-panel-body">
      <div v-for="{ key, value } in orderedEntries" :key="key" class="exif-row">
        <div class="exif-label">{{ label(key) }}</div>
        <div class="exif-value-wrapper">
          <div class="exif-value" :class="{ mono: key === 'MakerNote' }">{{ value }}</div>
          <button
            v-if="key === 'MakerNote'"
            class="exif-copy-btn"
            :title="t('exif.copy')"
            @click="copyMakerNote(value)"
          >
            {{ t("exif.copy") }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.exif-panel {
  position: fixed;
  top: 56px;
  bottom: 0;
  left: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  width: 320px;
  pointer-events: none;
  background: transparent;
  opacity: 0;
  transform: translateX(-100%);
  transition:
    transform 250ms ease-out,
    opacity 250ms ease-out;
}

.exif-panel.visible {
  pointer-events: auto;
  opacity: 1;
  transform: translateX(0);
  transition-timing-function: ease-out;
}

.exif-panel-body {
  flex: 1;
  padding: 12px 16px;
  overflow-y: auto;
  /* 隐藏滚动条（内容仍可用滚轮滚动） */
  scrollbar-width: none;
}

.exif-panel-body::-webkit-scrollbar {
  display: none;
}

.exif-row {
  margin-bottom: 12px;
}

.exif-label {
  margin-bottom: 4px;
  font-size: 0.72rem;
  font-weight: 600;
  color: var(--fg-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  text-shadow: 0 1px 3px var(--glow);
}

.exif-value-wrapper {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.exif-value {
  flex: 1;
  font-size: 0.84rem;
  line-height: 1.4;
  color: var(--fg);
  overflow-wrap: anywhere;
  text-shadow: 0 1px 2px var(--glow);
}

.exif-value.mono {
  display: -webkit-box;
  max-height: 96px;
  overflow: hidden;
  text-overflow: ellipsis;
  -webkit-line-clamp: 4;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 0.72rem;
  -webkit-box-orient: vertical;
}

.exif-copy-btn {
  flex-shrink: 0;
  padding: 3px 8px;
  font-size: 0.72rem;
  color: var(--fg-muted);
  cursor: pointer;
  background: var(--surface-soft);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-sm);
  isolation: isolate;
  transition: color 150ms;
}

/* hover 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.exif-copy-btn::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.exif-copy-btn:hover::before {
  opacity: 1;
}

.exif-copy-btn:hover {
  color: var(--fg);
}

@media (width <= 640px) {
  .exif-panel {
    top: 48px;
    width: 280px;
  }
}
</style>
