/**
 * 元素首次进入视口时返回 true，仅触发一次。
 * @param {import('vue').Ref<HTMLElement | null>} target
 * @param {{ threshold?: number, rootMargin?: string }} [options]
 */
export function useInView(target, options = {}) {
  const inView = ref(false)
  let observer = null

  onMounted(() => {
    if (!target.value) return
    if (typeof IntersectionObserver === 'undefined') {
      inView.value = true
      return
    }
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            inView.value = true
            observer?.disconnect()
          }
        }
      },
      { threshold: options.threshold ?? 0.2, rootMargin: options.rootMargin ?? '0px' }
    )
    observer.observe(target.value)
  })

  onBeforeUnmount(() => observer?.disconnect())

  return { inView }
}
