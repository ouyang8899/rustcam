<script setup>
defineProps({
  data: { type: Object, required: true }
})
</script>

<template>
  <section class="border-y border-white/5 bg-ink-900/40">
    <div class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
      <UiSectionHeading :eyebrow="data.eyebrow" :title="data.title" :description="data.description" />
      <div class="mt-12 grid gap-10 lg:grid-cols-[1.15fr_0.85fr] lg:items-center">
        <UiReveal>
          <div class="panel overflow-hidden p-2">
            <img
              src="/img/architecture.webp"
              alt="RustCam 系统架构图"
              width="1920"
              height="2145"
              class="w-full rounded-xl"
              loading="lazy"
              decoding="async"
            >
          </div>
        </UiReveal>
        <div class="space-y-4">
          <UiReveal v-for="(point, i) in data.points" :key="point.title" :delay="i * 90">
            <div class="panel p-5">
              <h3 class="text-sm font-semibold text-white">{{ point.title }}</h3>
              <p class="text-muted mt-2 text-sm leading-relaxed">{{ point.desc }}</p>
            </div>
          </UiReveal>
        </div>
      </div>
    </div>
  </section>
</template>
