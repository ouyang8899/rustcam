<script setup>
import { links } from '~~/content/shared'

const { t } = useI18n()
const localePath = useLocalePath()
const open = ref(false)
const route = useRoute()

watch(() => route.fullPath, () => { open.value = false })

const navItems = computed(() => [
  { to: localePath('/'), label: t('nav.home') },
  { to: localePath('/features'), label: t('nav.features') },
  { to: localePath('/scenarios'), label: t('nav.scenarios') },
  { to: localePath('/docs'), label: t('nav.docs') },
  { to: localePath('/about'), label: t('nav.about') }
])
</script>

<template>
  <header class="sticky top-0 z-50 border-b border-white/5 bg-ink-950/80 backdrop-blur-xl">
    <a
      href="#main"
      class="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-3 focus:z-50 focus:rounded focus:bg-accent focus:px-3 focus:py-2 focus:text-sm focus:text-ink-950"
    >
      {{ t('nav.skipToContent') }}
    </a>

    <div class="mx-auto flex h-16 max-w-6xl items-center justify-between gap-4 px-6">
      <NuxtLink :to="localePath('/')" class="flex items-center gap-2.5">
        <span class="relative flex h-2.5 w-2.5">
          <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent opacity-60" />
          <span class="relative inline-flex h-2.5 w-2.5 rounded-full bg-accent" />
        </span>
        <span class="text-lg font-semibold tracking-tight">Rust<span class="text-accent">Cam</span></span>
      </NuxtLink>

      <nav class="hidden items-center gap-1 md:flex" aria-label="主导航">
        <NuxtLink
          v-for="item in navItems"
          :key="item.to"
          :to="item.to"
          class="rounded-lg px-3 py-2 text-sm text-muted transition-colors hover:bg-white/5 hover:text-white"
          active-class="text-accent! bg-accent/10"
        >
          {{ item.label }}
        </NuxtLink>
      </nav>

      <div class="hidden items-center gap-3 md:flex">
        <SiteLocaleSwitch />
        <a
          :href="links.gitee"
          target="_blank"
          rel="noopener noreferrer"
          class="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent/85"
        >
          {{ t('nav.cta') }}
        </a>
      </div>

      <button
        type="button"
        class="inline-flex h-10 w-10 items-center justify-center rounded-lg border border-white/10 text-muted md:hidden"
        :aria-expanded="open"
        aria-controls="mobile-nav"
        :aria-label="open ? t('nav.close') : t('nav.menu')"
        @click="open = !open"
      >
        <span class="sr-only">{{ open ? t('nav.close') : t('nav.menu') }}</span>
        <svg viewBox="0 0 24 24" class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
          <path v-if="!open" d="M4 7h16M4 12h16M4 17h16" />
          <path v-else d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </div>

    <div v-show="open" id="mobile-nav" class="border-t border-white/5 bg-ink-950/95 px-6 py-4 md:hidden">
      <nav class="flex flex-col gap-1" aria-label="移动端导航">
        <NuxtLink
          v-for="item in navItems"
          :key="item.to"
          :to="item.to"
          class="rounded-lg px-3 py-2.5 text-sm text-muted transition-colors hover:bg-white/5 hover:text-white"
          active-class="text-accent! bg-accent/10"
        >
          {{ item.label }}
        </NuxtLink>
      </nav>
      <div class="mt-4 flex items-center justify-between gap-3">
        <SiteLocaleSwitch />
        <a :href="links.gitee" target="_blank" rel="noopener noreferrer" class="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-ink-950">
          {{ t('nav.cta') }}
        </a>
      </div>
    </div>
  </header>
</template>
