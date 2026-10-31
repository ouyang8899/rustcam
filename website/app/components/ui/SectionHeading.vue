<script setup>
defineProps({
  eyebrow: { type: String, default: '' },
  title: { type: String, required: true },
  description: { type: String, default: '' },
  align: { type: String, default: 'center' }
})
</script>

<template>
  <div :class="align === 'center' ? 'mx-auto max-w-2xl text-center' : 'max-w-2xl'">
    <p v-if="eyebrow" class="text-accent text-xs font-semibold tracking-[0.2em] uppercase">
      {{ eyebrow }}
    </p>
    <h2 class="mt-3 text-3xl font-semibold tracking-tight text-white sm:text-4xl">
      {{ title }}
    </h2>
    <p v-if="description" class="text-muted mt-4 text-base leading-relaxed">
      {{ description }}
    </p>
  </div>
</template>
