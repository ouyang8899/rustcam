<script setup>
const props = defineProps({
  items: { type: Array, required: true },
  modelValue: { type: Number, default: null }
})

const emit = defineEmits(['update:modelValue'])

const index = computed(() => props.modelValue)
const current = computed(() => (index.value === null ? null : props.items[index.value]))

function close() { emit('update:modelValue', null) }
function prev() {
  if (index.value === null) return
  emit('update:modelValue', (index.value - 1 + props.items.length) % props.items.length)
}
function next() {
  if (index.value === null) return
  emit('update:modelValue', (index.value + 1) % props.items.length)
}

function onKey(event) {
  if (index.value === null) return
  if (event.key === 'Escape') close()
  if (event.key === 'ArrowLeft') prev()
  if (event.key === 'ArrowRight') next()
}

watch(index, (value) => {
  if (typeof document === 'undefined') return
  document.body.style.overflow = value === null ? '' : 'hidden'
})

onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  document.body.style.overflow = ''
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="current"
      class="fixed inset-0 z-[100] flex flex-col items-center justify-center bg-ink-950/92 p-6 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      :aria-label="current.caption"
      @click.self="close"
    >
      <img
        :src="current.src"
        :alt="current.alt"
        :width="current.width"
        :height="current.height"
        class="max-h-[80vh] w-auto rounded-xl border border-white/10 object-contain"
      >
      <p class="text-muted mt-4 text-sm">{{ current.caption }}</p>

      <div class="mt-4 flex items-center gap-3">
        <button type="button" class="rounded-lg border border-white/10 px-3 py-1.5 text-sm text-muted hover:text-white" @click="prev">←</button>
        <span class="text-muted text-xs tabular-nums">{{ index + 1 }} / {{ items.length }}</span>
        <button type="button" class="rounded-lg border border-white/10 px-3 py-1.5 text-sm text-muted hover:text-white" @click="next">→</button>
        <button type="button" class="rounded-lg border border-white/10 px-3 py-1.5 text-sm text-muted hover:text-white" @click="close">✕</button>
      </div>
    </div>
  </Teleport>
</template>
