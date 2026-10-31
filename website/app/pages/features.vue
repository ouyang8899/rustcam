<script setup>
import featuresContent from '~~/content/features'
import { pickLocale } from '~/utils/locale'

const { locale } = useI18n()
const data = computed(() => pickLocale(featuresContent, locale.value))

const DONE_LABELS = ['完成', 'Done']

function isDone(status) {
  return DONE_LABELS.includes(status)
}

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
      :title="$t('nav.features')"
      :description="data.meta.description"
      align="left"
    />

    <div class="mt-14 grid gap-5 md:grid-cols-2">
      <UiReveal v-for="(domain, i) in data.domains" :key="domain.id" :delay="(i % 2) * 70">
        <div class="panel h-full p-6">
          <div class="flex items-center gap-3">
            <div class="flex h-11 w-11 items-center justify-center rounded-xl border border-accent/30 bg-accent/10 text-accent">
              <UiIcon :name="domain.icon" :size="22" />
            </div>
            <div>
              <h2 class="font-semibold text-white">{{ domain.title }}</h2>
              <p class="text-muted text-xs">{{ domain.summary }}</p>
            </div>
          </div>
          <ul class="mt-5 space-y-2">
            <li v-for="item in domain.items" :key="item" class="text-muted flex gap-2 text-sm">
              <span class="text-accent">▸</span>
              <span>{{ item }}</span>
            </li>
          </ul>
        </div>
      </UiReveal>
    </div>

    <div class="mt-20">
      <h2 class="text-2xl font-semibold text-white">{{ data.protocols.title }}</h2>
      <div class="panel mt-6 overflow-x-auto">
        <table class="w-full min-w-[640px] text-left text-sm">
          <thead class="text-muted border-b border-white/10 text-xs uppercase">
            <tr>
              <th v-for="col in data.protocols.columns" :key="col" class="px-5 py-3 font-medium">{{ col }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in data.protocols.rows" :key="row.name" class="border-b border-white/5 last:border-0">
              <td class="px-5 py-3.5 font-mono text-white">{{ row.name }}</td>
              <td class="px-5 py-3.5">
                <span
                  class="rounded-full px-2.5 py-1 text-xs"
                  :class="isDone(row.status) ? 'bg-accent/12 text-accent' : 'bg-accent-amber/12 text-accent-amber'"
                >{{ row.status }}</span>
              </td>
              <td class="text-muted px-5 py-3.5">{{ row.support }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="mt-20">
      <h2 class="text-2xl font-semibold text-white">{{ data.progress.title }}</h2>
      <div class="panel mt-6 overflow-x-auto">
        <table class="w-full min-w-[640px] text-left text-sm">
          <thead class="text-muted border-b border-white/10 text-xs uppercase">
            <tr>
              <th v-for="col in data.progress.columns" :key="col" class="px-5 py-3 font-medium">{{ col }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in data.progress.rows" :key="row.module" class="border-b border-white/5 last:border-0">
              <td class="px-5 py-3.5 text-white">{{ row.module }}</td>
              <td class="px-5 py-3.5">
                <span
                  class="rounded-full px-2.5 py-1 text-xs"
                  :class="isDone(row.status) ? 'bg-accent/12 text-accent' : 'bg-accent-amber/12 text-accent-amber'"
                >{{ row.status }}</span>
              </td>
              <td class="text-muted px-5 py-3.5">{{ row.note }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
