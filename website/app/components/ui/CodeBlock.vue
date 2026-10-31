<script setup>
const props = defineProps({
  code: { type: String, required: true },
  lang: { type: String, default: 'bash' }
})

const { t } = useI18n()
const state = ref('idle')
let timer = null

async function copy() {
  try {
    await navigator.clipboard.writeText(props.code)
    state.value = 'done'
  } catch {
    state.value = 'error'
  }
  clearTimeout(timer)
  timer = setTimeout(() => { state.value = 'idle' }, 1800)
}

onBeforeUnmount(() => clearTimeout(timer))
</script>

<template>
  <div class="overflow-hidden rounded-xl border border-white/10 bg-ink-900/80">
    <div class="flex items-center justify-between border-b border-white/10 px-4 py-2">
      <span class="font-mono text-xs text-muted">{{ lang }}</span>
      <button
        type="button"
        class="rounded-md px-2 py-1 text-xs text-muted transition-colors hover:bg-white/5 hover:text-white"
        @click="copy"
      >
        {{ state === 'done' ? t('common.copied') : state === 'error' ? t('common.copyFailed') : t('common.copy') }}
      </button>
    </div>
    <pre class="overflow-x-auto p-4 font-mono text-sm leading-relaxed text-slate-200"><code>{{ code }}</code></pre>
  </div>
</template>
