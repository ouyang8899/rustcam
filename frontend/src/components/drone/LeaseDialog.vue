<!-- @ts-check -->
<template>
  <el-dialog
    :model-value="modelValue"
    :title="title"
    width="420px"
    destroy-on-close
    append-to-body
    @update:model-value="$emit('update:modelValue', $event)"
  >
    <div class="lease-target">
      <span class="lease-drone">{{ droneName || droneTag || '未选择无人机' }}</span>
      <el-tag size="small" effect="plain">
        {{ controlType === 'gimbal' ? '云台控制' : '飞行控制' }}
      </el-tag>
    </div>

    <el-form label-width="84px" class="lease-form">
      <el-form-item label="租约时长">
        <el-select v-model="selected" style="width: 100%">
          <el-option
            v-for="opt in leaseOptions"
            :key="opt.value"
            :label="opt.label"
            :value="opt.value"
          />
          <el-option label="自定义" value="custom" />
        </el-select>
      </el-form-item>
      <el-form-item v-if="isCustom" label="自定义">
        <el-input-number v-model="customMinutes" :min="1" :max="60" :step="1" controls-position="right" />
        <span class="unit">分钟</span>
      </el-form-item>
      <el-form-item label="生效方式">
        <span class="instant">立即生效</span>
      </el-form-item>
    </el-form>

    <div class="lease-hint">
      控制权从现在开始计时，{{ durationLabel }}后自动释放；到期前可续约或释放。
      <span v-if="expireText">约 {{ expireText }} 到期。</span>
    </div>

    <template #footer>
      <el-button @click="$emit('update:modelValue', false)">取消</el-button>
      <el-button type="primary" @click="confirm">{{ confirmText }}</el-button>
    </template>
  </el-dialog>
</template>

<script setup>
// @ts-check
import { computed, ref, watch } from 'vue'
import { LEASE_OPTIONS, DEFAULT_LEASE_SECONDS } from '../../utils/droneLease'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  droneTag: { type: String, default: '' },
  droneName: { type: String, default: '' },
  controlType: { type: String, default: 'flight' },
  title: { type: String, default: '申请控制权' },
  confirmText: { type: String, default: '确认申请' },
  defaultSeconds: { type: Number, default: DEFAULT_LEASE_SECONDS },
})

const emit = defineEmits(['update:modelValue', 'confirm'])

const leaseOptions = LEASE_OPTIONS
const selected = ref(DEFAULT_LEASE_SECONDS)
const customMinutes = ref(5)

const isCustom = computed(() => selected.value === 'custom')
const leaseSeconds = computed(() =>
  isCustom.value
    ? Math.max(60, Math.round(Number(customMinutes.value || 0) * 60))
    : Number(selected.value) || DEFAULT_LEASE_SECONDS
)
const durationLabel = computed(() => {
  const mins = leaseSeconds.value / 60
  return mins >= 60
    ? `${(mins / 60).toFixed(mins % 60 === 0 ? 0 : 1)} 小时`
    : `${mins} 分钟`
})
const expireText = computed(() => {
  const d = new Date(Date.now() + leaseSeconds.value * 1000)
  return d.toLocaleTimeString('zh-CN', { hour12: false })
})

watch(
  () => props.modelValue,
  (open) => {
    if (!open) return
    const preset = LEASE_OPTIONS.find((o) => o.value === props.defaultSeconds)
    if (preset) {
      selected.value = preset.value
    } else {
      selected.value = 'custom'
      customMinutes.value = Math.max(1, Math.round(props.defaultSeconds / 60))
    }
  }
)

function confirm() {
  emit('confirm', leaseSeconds.value)
  emit('update:modelValue', false)
}
</script>

<style scoped>
.lease-target {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 12px;
  margin-bottom: 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
}
.lease-drone {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.lease-form :deep(.el-form-item:last-child) {
  margin-bottom: 4px;
}
.unit {
  margin-left: 8px;
  color: var(--text-muted);
  font-size: 12px;
}
.instant {
  font-family: var(--font-mono);
  font-size: 13px;
}
.lease-hint {
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-muted);
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--bg-base);
}
</style>
