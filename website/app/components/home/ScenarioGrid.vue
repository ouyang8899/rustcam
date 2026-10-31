<script setup>
defineProps({
  data: { type: Object, required: true }
})
</script>

<template>
  <section class="border-y border-white/5 bg-ink-900/40">
    <div class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
      <UiSectionHeading :eyebrow="data.eyebrow" :title="data.title" :description="data.description" />
      <div class="mt-12 grid gap-5 md:grid-cols-2">
        <UiReveal v-for="(item, i) in data.items" :key="item.title" :delay="i * 80">
          <div class="panel flex h-full gap-4 p-6">
            <div class="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-accent/30 bg-accent/10 text-accent">
              <UiIcon :name="item.icon" :size="22" />
            </div>
            <div>
              <h3 class="text-lg font-semibold text-white">{{ item.title }}</h3>
              <p class="text-muted mt-2 text-sm leading-relaxed">{{ item.desc }}</p>
            </div>
          </div>
        </UiReveal>
      </div>
    </div>
  </section>
</template>
