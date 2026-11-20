<template>
  <div class="data-card">
    <div class="data-card__header" v-if="title || $slots.header">
      <slot name="header">
        <span class="data-card__title">{{ title }}</span>
      </slot>
      <div class="data-card__toolbar" v-if="$slots.toolbar">
        <slot name="toolbar" />
      </div>
    </div>
    <div class="data-card__body">
      <slot />
    </div>
  </div>
</template>

<script setup>
defineProps({
  title: { type: String, default: '' },
})
</script>

