<script setup>
defineProps({
  data: { type: Object, required: true }
})
</script>

<template>
  <section class="border-y border-white/5 bg-ink-900/40">
    <div class="mx-auto max-w-6xl px-6 py-14">
      <UiSectionHeading :title="data.title" :description="data.description" />
      <div class="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <UiReveal v-for="(item, i) in data.items" :key="item.name" :delay="i * 60">
          <div class="panel h-full p-5">
            <div class="flex items-center gap-2.5">
              <span class="text-accent h-1.5 w-1.5 rounded-full" />
              <h3 class="font-mono text-sm font-semibold text-white">{{ item.name }}</h3>
            </div>
            <p class="text-muted mt-2.5 text-sm leading-relaxed">{{ item.desc }}</p>
          </div>
        </UiReveal>
      </div>
    </div>
  </section>
</template>
