<script setup>
import { links } from '~~/content/shared'

const { t } = useI18n()
const localePath = useLocalePath()
const year = new Date().getFullYear()

const columns = computed(() => [
  {
    title: t('footer.product'),
    items: [
      { label: t('nav.features'), to: localePath('/features') },
      { label: t('nav.scenarios'), to: localePath('/scenarios') }
    ]
  },
  {
    title: t('footer.developer'),
    items: [
      { label: t('nav.docs'), to: localePath('/docs') },
      { label: t('nav.about'), to: localePath('/about') }
    ]
  },
  {
    title: t('footer.community'),
    items: [
      { label: 'Gitee', href: links.gitee },
      { label: 'GitHub', href: links.github }
    ]
  },
  {
    title: t('footer.contact'),
    items: [{ label: links.email.replace('mailto:', ''), href: links.email }]
  }
])
</script>

<template>
  <footer class="border-t border-white/5 bg-ink-950">
    <div class="mx-auto grid max-w-6xl gap-10 px-6 py-14 sm:grid-cols-2 lg:grid-cols-5">
      <div class="sm:col-span-2">
        <div class="flex items-center gap-2.5">
          <span class="h-2.5 w-2.5 rounded-full bg-accent" />
          <span class="text-lg font-semibold tracking-tight">Rust<span class="text-accent">Cam</span></span>
        </div>
        <p class="text-muted mt-4 max-w-xs text-sm leading-relaxed">{{ t('footer.tagline') }}</p>
      </div>

      <div v-for="col in columns" :key="col.title">
        <h3 class="text-sm font-semibold text-white">{{ col.title }}</h3>
        <ul class="mt-4 space-y-2.5">
          <li v-for="item in col.items" :key="item.label">
            <NuxtLink v-if="item.to" :to="item.to" class="text-muted text-sm transition-colors hover:text-accent">
              {{ item.label }}
            </NuxtLink>
            <a v-else :href="item.href" target="_blank" rel="noopener noreferrer" class="text-muted text-sm transition-colors hover:text-accent">
              {{ item.label }}
            </a>
          </li>
        </ul>
      </div>
    </div>

    <div class="border-t border-white/5">
      <div class="text-muted mx-auto flex max-w-6xl flex-col gap-2 px-6 py-6 text-xs sm:flex-row sm:items-center sm:justify-between">
        <p>© {{ year }} RustCam · {{ t('footer.rights') }}</p>
        <p>{{ t('footer.builtWith') }}</p>
      </div>
    </div>
  </footer>
</template>
