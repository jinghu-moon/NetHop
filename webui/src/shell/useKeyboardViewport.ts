import { onBeforeUnmount, onMounted, ref } from "vue";

export function keyboardVisible(layoutHeight: number, visualHeight: number): boolean {
  return layoutHeight > 0 && visualHeight > 0 && layoutHeight - visualHeight >= 160;
}

export function useKeyboardViewport() {
  const visible = ref(false);
  const viewportHeight = ref(window.visualViewport?.height ?? window.innerHeight);
  const update = (): void => {
    const visual = window.visualViewport?.height ?? window.innerHeight;
    viewportHeight.value = visual;
    visible.value = keyboardVisible(window.innerHeight, visual);
  };
  onMounted(() => {
    update();
    window.addEventListener("resize", update);
    window.visualViewport?.addEventListener("resize", update);
  });
  onBeforeUnmount(() => {
    window.removeEventListener("resize", update);
    window.visualViewport?.removeEventListener("resize", update);
  });
  return { visible, viewportHeight };
}
