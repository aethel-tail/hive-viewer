<script setup lang="ts" generic="T extends string">
import { ref, computed, onMounted, onUnmounted } from "vue";

const props = defineProps<{
  modelValue: T;
  options: { value: T; label: string }[];
  disabled?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: T];
}>();

const open = ref(false);
const rootEl = ref<HTMLElement | null>(null);

const selected = computed(() => props.options.find((o) => o.value === props.modelValue));

function onDocumentDown(e: PointerEvent) {
  if (rootEl.value && !rootEl.value.contains(e.target as Node)) {
    open.value = false;
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    open.value = false;
  }
}

onMounted(() => {
  document.addEventListener("pointerdown", onDocumentDown);
  document.addEventListener("keydown", onKeydown);
});

onUnmounted(() => {
  document.removeEventListener("pointerdown", onDocumentDown);
  document.removeEventListener("keydown", onKeydown);
});

function pick(o: { value: T; label: string }) {
  emit("update:modelValue", o.value);
  open.value = false;
}
</script>

<template>
  <div ref="rootEl" class="select" :class="{ open, disabled }">
    <button type="button" class="select-trigger" :disabled="disabled" @click="open = !open">
      <span class="select-value">{{ selected?.label ?? modelValue }}</span>
      <svg
        class="select-arrow"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>

    <div class="select-menu">
      <button
        v-for="o in options"
        :key="o.value"
        type="button"
        class="select-option"
        :class="{ active: o.value === modelValue }"
        @click="pick(o)"
      >
        {{ o.label }}
        <span class="check">&#10003;</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.select {
  position: relative;
}

.select-trigger {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  padding: 6px 12px;
  font-family: var(--font);
  font-size: 0.82rem;
  color: var(--fg);
  cursor: pointer;
  background: var(--surface-strong);
  border: 1px solid var(--border-alpha);
  border-radius: var(--radius-sm);
  isolation: isolate;
  transition: border-color 150ms;
}

/* hover 底色用覆盖层 opacity 过渡（只走合成器），不动 background */

.select-trigger::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 150ms;
}

.select-trigger:hover::before {
  opacity: 1;
}

.select.disabled .select-trigger {
  cursor: not-allowed;
  opacity: 0.45;
}

.select-value {
  white-space: nowrap;
}

.select-arrow {
  width: 12px;
  height: 12px;
  color: var(--fg-muted);
  transform: rotate(0deg);
  transition: transform 150ms ease-out;
}

.select.open .select-arrow {
  transform: rotate(180deg);
}

.select-menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 10;
  min-width: 100%;
  max-height: 260px;
  padding: 4px;
  overflow-y: auto;
  pointer-events: none;
  background: var(--surface-solid);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
  transform-origin: top right;
  transition:
    opacity 150ms ease-out,
    transform 150ms ease-out;
}

.select.open .select-menu {
  pointer-events: auto;
  opacity: 1;
  transform: translateY(0) scale(1);
}

.select-option {
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

.select-option::before {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  content: "";
  background: var(--muted);
  opacity: 0;
  transition: opacity 100ms;
}

.select-option:not(.active):hover::before {
  opacity: 1;
}

.select-option.active {
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-soft);
}

.select-option .check {
  margin-left: auto;
  opacity: 0;
}

.select-option.active .check {
  opacity: 1;
}
</style>
