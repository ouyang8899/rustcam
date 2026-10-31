<script setup>
const props = defineProps({
  delay: { type: Number, default: 0 },
  as: { type: String, default: 'div' }
})

const target = ref(null)
const { inView } = useInView(target)
</script>

<template>
  <component
    :is="props.as"
    ref="target"
    class="transition-[opacity,transform] duration-700 ease-out"
    :class="inView ? 'translate-y-0 opacity-100' : 'translate-y-4 opacity-0'"
    :style="{ transitionDelay: `${props.delay}ms` }"
  >
    <slot />
  </component>
</template>
