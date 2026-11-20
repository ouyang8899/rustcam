<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">{{ title }}</h1>
        <span v-if="subtitle" class="header-sub">{{ subtitle }}</span>
      </div>
      <div v-if="$slots.toolbar" class="page-toolbar">
        <slot name="toolbar" />
      </div>
    </div>
    <div class="page-body" :class="{ 'page-body--workspace': layout === 'workspace' }">
      <slot />
    </div>
    <div v-if="$slots.footer" class="page-footer">
      <slot name="footer" />
    </div>
  </div>
</template>

<script setup>
defineProps({
  title: { type: String, default: '' },
  subtitle: { type: String, default: '' },
  layout: { type: String, default: 'page' },
})
</script>
