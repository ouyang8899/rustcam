<script setup>
const props = defineProps({
  data: { type: Object, required: true }
})

const active = ref(null)

const groups = computed(() => [
  { title: props.data.web.title, items: props.data.web.items },
  { title: props.data.desktop.title, items: props.data.desktop.items }
])

const flat = computed(() => groups.value.flatMap((group) => group.items))

function open(item) {
  active.value = flat.value.findIndex((entry) => entry.src === item.src)
}
</script>

<template>
  <section class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
    <UiSectionHeading :eyebrow="data.eyebrow" :title="data.title" :description="data.description" />

    <div v-for="group in groups" :key="group.title" class="mt-12">
      <h3 class="text-sm font-semibold tracking-wide text-white">{{ group.title }}</h3>
      <div class="mt-5 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <UiReveal v-for="(item, i) in group.items" :key="item.src" :delay="(i % 3) * 70">
          <button
            type="button"
            class="group block w-full overflow-hidden rounded-xl border border-white/10 bg-ink-900/60 text-left transition-colors hover:border-accent/45"
            @click="open(item)"
          >
            <img
              :src="item.src"
              :alt="item.alt"
              :width="item.width"
              :height="item.height"
              class="aspect-video w-full object-cover object-top transition-transform duration-500 group-hover:scale-[1.03]"
              loading="lazy"
              decoding="async"
            >
            <span class="text-muted block px-4 py-3 text-sm group-hover:text-accent">{{ item.caption }}</span>
          </button>
        </UiReveal>
      </div>
    </div>

    <UiLightBox :items="flat" v-model="active" />
  </section>
</template>
