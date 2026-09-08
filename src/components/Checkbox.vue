<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue?: boolean;
    disabled?: boolean;
  }>(),
  {
    modelValue: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
}>();

function toggle() {
  if (props.disabled) {
    return;
  }
  emit("update:modelValue", !props.modelValue);
}
</script>

<template>
  <button
    type="button"
    class="checkbox"
    :class="{ checked: modelValue, disabled }"
    :disabled="disabled"
    role="checkbox"
    :aria-checked="modelValue"
    @click="toggle"
  >
    <span class="box">
      <svg
        class="check-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="3"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <polyline points="20 6 9 17 4 12" />
      </svg>
    </span>
    <slot />
  </button>
</template>

<style scoped>
.checkbox {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  padding: 2px;
  font-family: var(--font);
  font-size: 0.86rem;
  color: var(--fg);
  cursor: pointer;
  background: transparent;
  border: none;
}

.checkbox.disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.box {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  background: var(--surface-strong);
  border: 1.5px solid var(--border);
  border-radius: 5px;
  transition:
    border-color 150ms,
    background 150ms;
}

.checkbox:not(.disabled):hover .box {
  border-color: var(--accent);
}

.checkbox.checked .box {
  background: var(--accent);
  border-color: var(--accent);
}

.check-icon {
  width: 11px;
  height: 11px;
  color: #fff;
  opacity: 0;
  transform: scale(0.4);
  transition:
    opacity 150ms ease-out,
    transform 150ms ease-out;
}

.checkbox.checked .check-icon {
  opacity: 1;
  transform: scale(1);
}
</style>
