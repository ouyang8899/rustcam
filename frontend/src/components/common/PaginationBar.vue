<template>
  <div class="pagination-bar">
    <el-pagination
      :current-page="page"
      :page-size="pageSize"
      :page-sizes="pageSizes"
      :total="total"
      :layout="resolvedLayout"
      background
      @current-change="onPageChange"
      @size-change="onSizeChange"
    />
  </div>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  total: { type: Number, default: 0 },
  page: { type: Number, default: 1 },
  pageSize: { type: Number, default: 20 },
  pageSizes: { type: Array, default: () => [20, 50, 100, 200] },
  showSizes: { type: Boolean, default: false },
  layout: { type: String, default: '' },
})

const emit = defineEmits(['update:page', 'update:pageSize', 'change'])

const resolvedLayout = computed(
  () => props.layout || (props.showSizes ? 'total, sizes, prev, pager, next, jumper' : 'total, prev, pager, next, jumper'),
)

function onPageChange(p) {
  emit('update:page', p)
  emit('change', { page: p, pageSize: props.pageSize })
}

function onSizeChange(size) {
  emit('update:pageSize', size)
  emit('update:page', 1)
  emit('change', { page: 1, pageSize: size })
}
</script>

<style scoped>
.pagination-bar {
  margin-top: var(--space-4);
  display: flex;
  justify-content: flex-end;
}
</style>
