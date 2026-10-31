<script setup>
defineProps({
  data: { type: Object, required: true }
})
</script>

<template>
  <section class="border-y border-white/5 bg-ink-900/40">
    <div class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
      <UiSectionHeading :eyebrow="data.eyebrow" :title="data.title" :description="data.description" />
      <div class="mt-12 grid gap-5 md:grid-cols-3">
        <UiReveal v-for="(column, i) in data.columns" :key="column.title" :delay="i * 90">
          <div class="panel h-full p-6">
            <h3 class="font-mono text-sm font-semibold text-accent">{{ column.title }}</h3>
            <ul class="mt-4 space-y-2.5">
              <li v-for="item in column.items" :key="item" class="text-muted flex gap-2 text-sm">
                <span class="text-accent">·</span>
                <span>{{ item }}</span>
              </li>
            </ul>
          </div>
        </UiReveal>
      </div>
    </div>
  </section>
</template>
