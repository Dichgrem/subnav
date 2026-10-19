import { ref } from "vue";

export function useScroll(contentRef) {
  const showBackToTop = ref(false);

  const onContentScroll = () => {
    showBackToTop.value = (contentRef.value?.scrollTop || 0) > 300;
  };

  const scrollToTop = () => {
    contentRef.value?.scrollTo({ top: 0, behavior: "smooth" });
  };

  return { showBackToTop, onContentScroll, scrollToTop };
}
