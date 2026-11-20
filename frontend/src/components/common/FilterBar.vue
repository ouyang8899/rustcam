<template>
  <div class="filter-bar">
    <div class="filter-bar__left">
      <slot name="left" />
    </div>
    <div class="filter-bar__right">
      <slot name="right" />
    </div>
  </div>
</template>

<script setup>
</script>

<style scoped>
.filter-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.filter-bar__left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
  flex: 1 1 auto;
  min-width: 0;
}

.filter-bar__left > * {
  flex: 0 0 auto;
}

.filter-bar__right {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}
</style>
