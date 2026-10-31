<script setup>
import homeContent from '~~/content/home'
import { pickLocale } from '~/utils/locale'

const { locale } = useI18n()
const data = computed(() => pickLocale(homeContent, locale.value))

useSeoMeta({
  title: () => data.value.meta.title,
  description: () => data.value.meta.description,
  ogTitle: () => data.value.meta.title,
  ogDescription: () => data.value.meta.description,
  ogType: 'website'
})
</script>

<template>
  <div>
    <HomeHeroSection :data="data.hero" />
    <HomeProtocolStrip :data="data.protocols" />
    <HomeFeatureGrid :data="data.features" />
    <HomeArchitectureSection :data="data.architecture" />
    <HomeScreenshotGallery :data="data.gallery" />
    <HomeScenarioGrid :data="data.scenarios" />
    <HomeQuickStartSection :data="data.quickStart" />
    <HomeTechStackSection :data="data.techStack" />
    <HomeCtaSection :data="data.cta" />
  </div>
</template>
