<script setup>
import { links } from '~~/content/shared'

const props = defineProps({
  data: { type: Object, required: true }
})

const localePath = useLocalePath()
</script>

<template>
  <section class="mx-auto max-w-6xl px-6 py-20 sm:py-24">
    <UiReveal>
      <div class="panel relative overflow-hidden p-10 text-center sm:p-14">
        <div class="pointer-events-none absolute inset-0 bg-[radial-gradient(50%_80%_at_50%_0%,rgba(34,211,238,0.16),transparent_70%)]" aria-hidden="true" />
        <div class="relative">
          <h2 class="text-3xl font-semibold tracking-tight text-white sm:text-4xl">{{ data.title }}</h2>
          <p class="text-muted mx-auto mt-4 max-w-2xl text-base leading-relaxed">{{ data.description }}</p>
          <div class="mt-8 flex flex-wrap justify-center gap-3">
            <NuxtLink
              :to="localePath('/docs')"
              class="rounded-xl bg-accent px-5 py-3 text-sm font-medium text-ink-950 transition-colors hover:bg-accent/85"
            >
              {{ data.primary }}
            </NuxtLink>
            <a
              :href="links.gitee"
              target="_blank"
              rel="noopener noreferrer"
              class="rounded-xl border border-white/15 px-5 py-3 text-sm font-medium text-white transition-colors hover:border-accent/50 hover:text-accent"
            >
              {{ data.secondary }}
            </a>
          </div>
        </div>
      </div>
    </UiReveal>
  </section>
</template>
