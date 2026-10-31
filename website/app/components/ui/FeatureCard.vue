<script setup>
defineProps({
  icon: { type: String, default: 'zap' },
  title: { type: String, required: true },
  description: { type: String, default: '' },
  items: { type: Array, default: () => [] }
})
</script>

<template>
  <div class="group panel h-full p-6 transition-all duration-300 hover:-translate-y-0.5 hover:border-accent/40 hover:shadow-[0_0_32px_-8px_rgba(34,211,238,0.45)]">
    <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-accent/30 bg-accent/10 text-accent">
      <UiIcon :name="icon" :size="22" />
    </div>
    <h3 class="mt-5 text-lg font-semibold text-white">{{ title }}</h3>
    <p v-if="description" class="text-muted mt-2.5 text-sm leading-relaxed">{{ description }}</p>
    <ul v-if="items.length" class="mt-4 space-y-2">
      <li v-for="item in items" :key="item" class="flex gap-2 text-sm text-slate-300">
        <span class="text-accent mt-0.5">▸</span>
        <span>{{ item }}</span>
      </li>
    </ul>
  </div>
</template>
