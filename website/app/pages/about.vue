<script setup>
import aboutContent from '~~/content/about'
import { pickLocale } from '~/utils/locale'

const { locale } = useI18n()
const data = computed(() => pickLocale(aboutContent, locale.value))

const STATUS_CLASS = {
  done: 'bg-accent/12 text-accent',
  progress: 'bg-accent-amber/12 text-accent-amber',
  next: 'bg-white/5 text-muted'
}

const STATUS_LABEL = {
  zh: { done: '完成', progress: '进行中', next: '规划中' },
  en: { done: 'Done', progress: 'In progress', next: 'Next' }
}

const statusLabel = (status) => STATUS_LABEL[locale.value]?.[status] ?? status

useSeoMeta({
  title: () => data.value.meta.title,
  description: () => data.value.meta.description,
  ogTitle: () => data.value.meta.title,
  ogDescription: () => data.value.meta.description
})
</script>

<template>
  <div class="mx-auto max-w-6xl px-6 py-20">
    <UiSectionHeading
      eyebrow="RustCam"
      :title="$t('nav.about')"
      :description="data.intro.desc"
      align="left"
    />

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.principles.title }}</h2>
      <div class="mt-6 grid gap-5 md:grid-cols-2">
        <UiReveal v-for="(item, i) in data.principles.items" :key="item.title" :delay="(i % 2) * 70">
          <div class="panel h-full p-6">
            <h3 class="font-semibold text-white">{{ item.title }}</h3>
            <p class="text-muted mt-2.5 text-sm leading-relaxed">{{ item.desc }}</p>
          </div>
        </UiReveal>
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.roadmap.title }}</h2>
      <div class="mt-6 space-y-3">
        <UiReveal v-for="(item, i) in data.roadmap.items" :key="item.title" :delay="i * 60">
          <div class="panel flex flex-col gap-3 p-5 sm:flex-row sm:items-start sm:gap-6">
            <span class="shrink-0 rounded-full px-3 py-1 text-xs" :class="STATUS_CLASS[item.status]">
              {{ statusLabel(item.status) }}
            </span>
            <div>
              <h3 class="font-semibold text-white">{{ item.title }}</h3>
              <p class="text-muted mt-1.5 text-sm leading-relaxed">{{ item.desc }}</p>
            </div>
          </div>
        </UiReveal>
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.contribute.title }}</h2>
      <p class="text-muted mt-3 max-w-3xl text-sm leading-relaxed">{{ data.contribute.desc }}</p>
      <div class="mt-5">
        <UiCodeBlock :code="data.contribute.code" lang="bash" />
      </div>
      <p class="text-muted mt-4 text-sm">{{ data.contribute.note }}</p>
    </section>

    <section class="mt-16">
      <div class="panel p-6">
        <h2 class="text-lg font-semibold text-white">{{ data.license.title }}</h2>
        <p class="text-muted mt-2.5 text-sm leading-relaxed">{{ data.license.desc }}</p>
      </div>
    </section>
  </div>
</template>
