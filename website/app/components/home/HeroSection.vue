<script setup>
import { links } from '~~/content/shared'

const props = defineProps({
  data: { type: Object, required: true }
})

const localePath = useLocalePath()
const lines = computed(() => props.data.title.split('\n'))
</script>

<template>
  <section class="relative overflow-hidden">
    <div class="mx-auto max-w-6xl px-6 pt-24 pb-20 sm:pt-32">
      <div class="grid items-center gap-14 lg:grid-cols-[1.05fr_0.95fr]">
        <div>
          <p class="text-accent text-xs font-semibold tracking-[0.24em] uppercase">
            {{ data.eyebrow }}
          </p>
          <h1 class="mt-5 text-4xl leading-tight font-semibold tracking-tight text-white sm:text-5xl lg:text-6xl">
            <span v-for="(line, i) in lines" :key="i" class="block">{{ line }}</span>
            <span class="text-gradient">.</span>
          </h1>
          <p class="text-muted mt-6 max-w-xl text-lg leading-relaxed">
            {{ data.subtitle }}
          </p>

          <div class="mt-9 flex flex-wrap items-center gap-3">
            <NuxtLink
              :to="localePath('/docs')"
              class="rounded-xl bg-accent px-5 py-3 text-sm font-medium text-ink-950 transition-colors hover:bg-accent/85"
            >
              {{ data.primaryCta }}
            </NuxtLink>
            <a
              :href="links.gitee"
              target="_blank"
              rel="noopener noreferrer"
              class="rounded-xl border border-white/15 px-5 py-3 text-sm font-medium text-white transition-colors hover:border-accent/50 hover:text-accent"
            >
              {{ data.secondaryCta }}
            </a>
          </div>

          <dl class="mt-14 grid grid-cols-2 gap-x-6 gap-y-8 sm:grid-cols-4">
            <div v-for="stat in data.stats" :key="stat.label">
              <dt class="text-muted text-xs">{{ stat.label }}</dt>
              <dd class="mt-1 text-2xl font-semibold text-white">
                <UiStatCounter :value="stat.value" :suffix="stat.suffix" />
              </dd>
            </div>
          </dl>
        </div>

        <div class="relative">
          <div class="panel overflow-hidden p-2">
            <img
              src="/img/console-overview.webp"
              alt="RustCam 控制台界面"
              width="1920"
              height="1082"
              class="w-full rounded-xl"
              loading="eager"
            >
          </div>
          <div class="absolute -right-6 -bottom-6 hidden rounded-xl border border-accent/25 bg-ink-900/95 px-4 py-3 backdrop-blur lg:block">
            <p class="text-accent font-mono text-xs">Rust + Axum + Tokio</p>
            <p class="text-muted mt-1 text-xs">102 条受保护 API · 14 条公开路由</p>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
