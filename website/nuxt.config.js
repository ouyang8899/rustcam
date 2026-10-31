import { defineNuxtConfig } from 'nuxt/config'
import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  compatibilityDate: '2026-01-01',
  ssr: true,
  devtools: { enabled: false },
  modules: ['@nuxtjs/i18n', '@nuxtjs/sitemap', '@nuxtjs/robots'],
  css: ['~/assets/css/main.css'],
  vite: { plugins: [tailwindcss()] },
  app: {
    baseUrl: process.env.NUXT_APP_BASE_URL || '/',
    head: {
      htmlAttrs: { lang: 'zh-CN' },
      link: [{ rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
      meta: [
        { name: 'theme-color', content: '#05080E' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1' }
      ]
    }
  },
  i18n: {
    defaultLocale: 'zh',
    strategy: 'prefix_except_default',
    restructureDir: 'i18n',
    langDir: 'locales',
    detectBrowserLanguage: false,
    bundle: { optimizeTranslationDirective: false },
    locales: [
      { code: 'zh', language: 'zh-CN', name: '中文', file: 'zh.js' },
      { code: 'en', language: 'en-US', name: 'English', file: 'en.js' }
    ]
  },
  site: {
    url: process.env.NUXT_APP_SITE_URL || 'https://www.rustcam.com',
    name: 'RustCam'
  },
  robots: {
    groups: [{ userAgent: '*', allow: '/' }]
  },
  nitro: {
    prerender: {
      crawlLinks: true,
      failOnError: false,
      routes: [
        '/', '/features', '/scenarios', '/docs', '/about',
        '/en', '/en/features', '/en/scenarios', '/en/docs', '/en/about'
      ]
    }
  }
})
