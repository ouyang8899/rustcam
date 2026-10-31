<script setup>
defineProps({
  data: { type: Object, required: true }
})
</script>

<template>
  <section class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
    <UiSectionHeading :eyebrow="data.eyebrow" :title="data.title" :description="data.description" />
    <div class="mt-12 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
      <UiReveal v-for="(item, i) in data.items" :key="item.title" :delay="(i % 3) * 80">
        <UiFeatureCard
          :icon="item.icon"
          :title="item.title"
          :description="item.description"
          :items="item.items"
        />
      </UiReveal>
    </div>
  </section>
</template>
