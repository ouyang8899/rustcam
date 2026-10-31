<script setup>
defineProps({
  data: { type: Object, required: true }
})
</script>

<template>
  <section class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
    <UiSectionHeading :eyebrow="data.eyebrow" :title="data.title" :description="data.description" />

    <div class="mt-12 grid gap-6 lg:grid-cols-3">
      <UiReveal v-for="(step, i) in data.steps" :key="step.title" :delay="i * 90">
        <div class="h-full">
          <div class="mb-3 flex items-center gap-3">
            <span class="text-accent flex h-7 w-7 items-center justify-center rounded-full border border-accent/40 font-mono text-xs">
              {{ i + 1 }}
            </span>
            <h3 class="font-semibold text-white">{{ step.title }}</h3>
          </div>
          <UiCodeBlock :code="step.code" :lang="step.lang" />
        </div>
      </UiReveal>
    </div>

    <UiReveal :delay="120">
      <div class="mt-10 grid gap-6 lg:grid-cols-2">
        <div>
          <h3 class="mb-3 font-semibold text-white">{{ data.frontendTitle }}</h3>
          <UiCodeBlock :code="data.frontendCode" lang="bash" />
        </div>
        <div class="panel p-6">
          <p class="text-muted text-sm leading-relaxed">{{ data.note }}</p>
        </div>
      </div>
    </UiReveal>
  </section>
</template>
