<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  text: string;
  // 带 action 时提示可点击（如「发现新版本」→ 打开 Release 页面）
  action?: (() => void) | null;
  duration?: number;
}>();

const gone = ref(false);

function onClick() {
  gone.value = true;
  props.action?.();
}
</script>

<template>
  <div
    v-if="!gone"
    class="message"
    :class="{ actionable: !!action }"
    :style="{ animationDuration: `${duration ?? 1200}ms` }"
    @click="onClick"
  >
    {{ text }}
  </div>
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

/* 可点击的提示：接住指针、悬停暂停淡出，让用户来得及点 */

.message.actionable {
  pointer-events: auto;
  cursor: pointer;
}

.message.actionable:hover {
  animation-play-state: paused;
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
