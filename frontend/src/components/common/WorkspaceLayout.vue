<template>
  <div class="workspace-layout" :style="{ '--workspace-aside-width': asideWidth }">
    <aside class="workspace-layout__aside">
      <slot name="aside" />
    </aside>
    <div class="workspace-layout__main">
      <div class="workspace-layout__content">
        <slot />
      </div>
      <div v-if="$slots.footer" class="workspace-layout__footer">
        <slot name="footer" />
      </div>
    </div>
  </div>
</template>

<script setup>
defineProps({
  asideWidth: { type: String, default: '280px' },
})
</script>

<style scoped>
.workspace-layout {
  display: flex;
  gap: var(--space-4);
  flex: 1 1 auto;
  min-height: 0;
}

.workspace-layout__aside {
  width: var(--workspace-aside-width, 280px);
  flex-shrink: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  overflow: hidden;
}

.workspace-layout__main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.workspace-layout__content {
  flex: 1;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.workspace-layout__content > * {
  flex-shrink: 0;
}

.workspace-layout__content :deep(.workspace-layout__fill) {
  flex: 1 1 auto;
  min-height: 0;
}

.workspace-layout__footer {
  flex-shrink: 0;
}
</style>
