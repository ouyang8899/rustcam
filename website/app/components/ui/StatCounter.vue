<script setup>
const props = defineProps({
  value: { type: Number, required: true },
  suffix: { type: String, default: '' }
})

const target = ref(null)
const { inView } = useInView(target)
const current = ref(0)
let raf = 0

watch(inView, (visible) => {
  if (!visible) return
  const duration = 900
  const start = performance.now()
  const step = (now) => {
    const progress = Math.min((now - start) / duration, 1)
    const eased = 1 - Math.pow(1 - progress, 3)
    current.value = Math.round(props.value * eased)
    if (progress < 1) raf = requestAnimationFrame(step)
  }
  raf = requestAnimationFrame(step)
})

onBeforeUnmount(() => cancelAnimationFrame(raf))
</script>

<template>
  <span ref="target" class="tabular-nums">
    {{ current }}{{ suffix }}
  </span>
</template>
