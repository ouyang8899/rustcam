<script setup>
import docsContent from '~~/content/docs'
import { pickLocale } from '~/utils/locale'

const { locale } = useI18n()
const data = computed(() => pickLocale(docsContent, locale.value))

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
      :title="$t('nav.docs')"
      :description="data.meta.description"
      align="left"
    />

    <section class="mt-14">
      <h2 class="text-2xl font-semibold text-white">Environment</h2>
      <div class="mt-5 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <div v-for="req in data.requirements" :key="req.name" class="panel p-5">
          <p class="font-mono text-sm text-white">{{ req.name }}</p>
          <p class="text-accent mt-1 text-sm">{{ req.version }}</p>
        </div>
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.structure.title }}</h2>
      <div class="panel mt-5 overflow-hidden">
        <div
          v-for="row in data.structure.rows"
          :key="row.path"
          class="flex flex-col gap-1 border-b border-white/5 px-5 py-3.5 last:border-0 sm:flex-row sm:gap-6"
        >
          <code class="font-mono text-sm text-accent sm:w-40 sm:shrink-0">{{ row.path }}</code>
          <span class="text-muted text-sm">{{ row.desc }}</span>
        </div>
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.startup.title }}</h2>
      <div class="mt-5 grid gap-5 md:grid-cols-2">
        <div v-for="item in data.startup.items" :key="item.title">
          <h3 class="mb-3 font-semibold text-white">{{ item.title }}</h3>
          <UiCodeBlock :code="item.code" :lang="item.lang" />
        </div>
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.config.title }}</h2>
      <p class="text-muted mt-3 max-w-3xl text-sm leading-relaxed">{{ data.config.desc }}</p>
      <div class="mt-5">
        <UiCodeBlock :code="data.config.code" lang="toml" />
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.api.title }}</h2>
      <p class="text-muted mt-3 max-w-3xl text-sm leading-relaxed">{{ data.api.desc }}</p>

      <div class="panel mt-6 p-5">
        <h3 class="text-sm font-semibold text-white">{{ data.api.authTitle }}</h3>
        <p class="text-muted mt-2 text-sm">{{ data.api.authDesc }}</p>
        <div class="mt-4">
          <UiCodeBlock code="Authorization: Bearer &lt;access_token&gt;" lang="http" />
        </div>
      </div>

      <h3 class="mt-10 text-lg font-semibold text-white">{{ data.api.groupsTitle }}</h3>
      <div class="panel mt-4 overflow-x-auto">
        <table class="w-full min-w-[720px] text-left text-sm">
          <thead class="text-muted border-b border-white/10 text-xs uppercase">
            <tr>
              <th class="px-5 py-3 font-medium">Group</th>
              <th class="px-5 py-3 font-medium">Count</th>
              <th class="px-5 py-3 font-medium">Endpoints</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="group in data.api.groups" :key="group.group" class="border-b border-white/5 last:border-0">
              <td class="px-5 py-3.5 text-white">{{ group.group }}</td>
              <td class="text-accent px-5 py-3.5 tabular-nums">{{ group.count }}</td>
              <td class="text-muted px-5 py-3.5 font-mono text-xs">{{ group.endpoints }}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <h3 class="mt-10 text-lg font-semibold text-white">{{ data.api.publicTitle }}</h3>
      <div class="panel mt-4 overflow-x-auto">
        <table class="w-full min-w-[720px] text-left text-sm">
          <tbody>
            <tr v-for="row in data.api.publicRows" :key="row.path" class="border-b border-white/5 last:border-0">
              <td class="px-5 py-3.5 font-mono text-xs text-accent">{{ row.path }}</td>
              <td class="text-muted px-5 py-3.5">{{ row.desc }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <section class="mt-16">
      <h2 class="text-2xl font-semibold text-white">{{ data.faq.title }}</h2>
      <div class="mt-5 space-y-3">
        <details v-for="item in data.faq.items" :key="item.q" class="panel p-5">
          <summary class="cursor-pointer list-none font-semibold text-white marker:hidden">
            {{ item.q }}
          </summary>
          <p class="text-muted mt-3 text-sm leading-relaxed">{{ item.a }}</p>
        </details>
      </div>
    </section>
  </div>
</template>
