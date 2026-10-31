<script setup>
const { locale, locales, t } = useI18n()
const switchLocalePath = useSwitchLocalePath()
</script>

<template>
  <div class="flex items-center gap-1 rounded-full border border-white/10 bg-white/5 p-0.5">
    <NuxtLink
      v-for="item in locales"
      :key="item.code"
      :to="switchLocalePath(item.code)"
      class="rounded-full px-2.5 py-1 text-xs font-medium transition-colors"
      :class="item.code === locale ? 'bg-accent/15 text-accent' : 'text-muted hover:text-white'"
      :aria-label="`${t('common.switchTo')} ${item.name}`"
      :aria-current="item.code === locale ? 'true' : undefined"
    >
      {{ item.code === 'zh' ? '中' : 'EN' }}
    </NuxtLink>
  </div>
</template>
