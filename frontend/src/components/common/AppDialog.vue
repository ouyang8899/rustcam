<template>
  <el-dialog
    :model-value="modelValue"
    :title="title"
    :width="width"
    :close-on-click-modal="closeOnClickModal"
    :close-on-press-escape="closeOnPressEscape"
    :draggable="draggable"
    destroy-on-close
    append-to-body
    @update:model-value="onUpdate"
    @open="$emit('open')"
    @opened="$emit('opened')"
    @close="$emit('close')"
    @closed="$emit('closed')"
  >
    <slot />
    <template v-if="$slots.footer" #footer><slot name="footer" /></template>
  </el-dialog>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  title: { type: String, default: '' },
  size: { type: String, default: 'md' },
  closeOnClickModal: { type: Boolean, default: true },
  closeOnPressEscape: { type: Boolean, default: true },
  draggable: { type: Boolean, default: false },
})

const emit = defineEmits(['update:modelValue', 'open', 'opened', 'close', 'closed'])

const WIDTHS = { sm: '420px', md: '560px', lg: '800px', xl: '1000px' }
const width = computed(() => WIDTHS[props.size] || WIDTHS.md)

function onUpdate(v) {
  emit('update:modelValue', v)
}
</script>
