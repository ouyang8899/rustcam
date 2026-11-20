<template>
  <span class="status-dot" :class="cls" :title="title" />
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  status: { type: String, default: 'offline' },
  title: { type: String, default: '' },
})

const cls = computed(() => {
  const s = (props.status || '').toLowerCase()
  if (s === 'online' || s === 'recording') return 'online'
  if (s === 'warning') return 'warning'
  if (s === 'error') return 'error'
  if (s === 'maintaining') return 'maintaining'
  return 'offline'
})
</script>

