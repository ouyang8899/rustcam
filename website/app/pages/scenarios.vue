<script setup>
import scenariosContent from '~~/content/scenarios'
import { pickLocale } from '~/utils/locale'

const { locale } = useI18n()
const data = computed(() => pickLocale(scenariosContent, locale.value))

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
      :title="$t('nav.scenarios')"
      :description="data.meta.description"
      align="left"
    />

    <div class="mt-14 space-y-6">
      <UiReveal v-for="(item, i) in data.items" :key="item.id" :delay="i * 60">
        <article class="panel overflow-hidden">
          <div class="grid gap-8 p-7 lg:grid-cols-[1fr_1.1fr]">
            <div>
              <div class="flex items-center gap-3">
                <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-accent/30 bg-accent/10 text-accent">
                  <UiIcon :name="item.icon" :size="22" />
                </div>
                <h2 class="text-xl font-semibold text-white">{{ item.title }}</h2>
              </div>
              <p class="text-muted mt-4 text-sm leading-relaxed">{{ item.pain }}</p>
              <p class="mt-4 flex gap-2 text-sm leading-relaxed text-slate-300">
                <span class="text-accent-amber shrink-0">▸</span>
                <span>{{ item.outcome }}</span>
              </p>
            </div>
            <div>
              <h3 class="text-muted text-xs font-semibold tracking-[0.18em] uppercase">Capability mapping</h3>
              <ul class="mt-4 space-y-2.5">
                <li v-for="entry in item.mapping" :key="entry" class="text-muted flex gap-2 text-sm">
                  <span class="text-accent">·</span>
                  <span>{{ entry }}</span>
                </li>
              </ul>
            </div>
          </div>
        </article>
      </UiReveal>
    </div>
  </div>
</template>
