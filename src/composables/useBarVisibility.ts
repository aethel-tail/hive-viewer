import { ref, computed, onUnmounted } from "vue";
import type { Ref, ComputedRef } from "vue";

const HIDE_DELAY_MS = 256;

export interface BarVisibility {
  visible: ComputedRef<boolean>;
  rootProps: {
    onMouseenter: () => void;
    onMouseleave: () => void;
  };
}

export function useBarVisibility(locked: Ref<boolean>): BarVisibility {
  const hovered = ref(false);
  let hideTimer: ReturnType<typeof setTimeout> | null = null;

  const visible = computed(() => locked.value || hovered.value);

  function show() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    hovered.value = true;
  }

  function scheduleHide() {
    if (hideTimer) {
      clearTimeout(hideTimer);
    }
    hideTimer = setTimeout(() => {
      hovered.value = false;
    }, HIDE_DELAY_MS);
  }

  onUnmounted(() => {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
  });

  return {
    visible,
    rootProps: {
      onMouseenter: show,
      onMouseleave: scheduleHide,
    },
  };
}
