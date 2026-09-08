<script setup lang="ts">
defineProps<{
  text: string;
}>();
</script>

<template>
  <div class="message">{{ text }}</div>
</template>

<style scoped>
.message {
  position: fixed;
  top: 64px;
  left: 50%;
  z-index: 40;
  padding: 8px 18px;
  font-family: var(--font);
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--fg);
  white-space: nowrap;
  pointer-events: none;
  user-select: none;
  background: var(--surface-solid);
  border: 1px solid var(--border);
  border-radius: 999px;
  box-shadow: var(--shadow-md);
  animation: msg-fade 1.2s ease-out forwards;
}

@keyframes msg-fade {
  0% {
    opacity: 0;
    transform: translate(-50%, 8px);
  }

  12% {
    opacity: 1;
    transform: translate(-50%, 0);
  }

  80% {
    opacity: 1;
    transform: translate(-50%, 0);
  }

  100% {
    opacity: 0;
    transform: translate(-50%, -8px);
  }
}
</style>
