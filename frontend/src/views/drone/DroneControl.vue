<!-- @ts-check -->
<template>
  <div class="drone-control">
    <div class="page-head">
      <h2>控制权管理</h2>
      <el-select
        v-model="selectedId"
        placeholder="选择无人机"
        style="width: 220px"
        @change="onSelect"
      >
        <el-option
          v-for="d in store.drones"
          :key="d.drone_tag"
          :label="d.name || d.drone_tag"
          :value="d.drone_tag"
        />
      </el-select>
      <span class="lease-label">租约时长</span>
      <el-select v-model="leaseSeconds" style="width: 120px">
        <el-option
          v-for="opt in leaseOptions"
          :key="opt.value"
          :label="opt.label"
          :value="opt.value"
        />
      </el-select>
      <el-button :loading="loading" @click="refresh">
        <el-icon><Refresh /></el-icon>
      </el-button>
    </div>

    <el-table :data="rows" border stripe>
      <el-table-column label="设备" prop="drone_tag" min-width="140" />
      <el-table-column label="类型" prop="control_type" width="90" />
      <el-table-column label="持有人" prop="holder" min-width="120" />
      <el-table-column label="优先级" prop="priority" width="90" />
      <el-table-column label="剩余租约(s)" prop="remain" width="110" />
      <el-table-column label="操作" width="220">
        <template #default="{ row }">
          <el-button
            size="small"
            type="primary"
            :disabled="!row.online || row.held"
            @click="acquire(row)"
          >
            申请
          </el-button>
          <el-button
            size="small"
            type="warning"
            :disabled="!row.held || row.granted"
            @click="preempt(row)"
          >
            抢占
          </el-button>
          <el-button
            size="small"
            :disabled="!row.granted"
            @click="release(row)"
          >
            释放
          </el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-empty v-if="rows.length === 0" description="暂无设备" />
  </div>
</template>

<script setup>
// @ts-check
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Refresh } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import { preemptControl } from '../../api/drone'
import { DEFAULT_LEASE_SECONDS, LEASE_OPTIONS } from '../../utils/droneLease'

const store = useDroneStore()
const selectedId = ref('')
const loading = ref(false)
const now = ref(Date.now())
/** 申请/抢占时使用的租约时长（秒）。 */
const leaseSeconds = ref(DEFAULT_LEASE_SECONDS)
const leaseOptions = LEASE_OPTIONS

const rows = computed(() => {
  return store.drones.map((d) => {
    const control = store.getControl(d.drone_tag, 'flight')
    const remain = control?.held && control.expire_at_ms
      ? Math.max(0, Math.round((control.expire_at_ms - now.value) / 1000))
      : 0
    return {
      drone_tag: d.drone_tag,
      online: !!d.online,
      control_type: 'flight',
      holder: control?.holder || '—',
      priority: control?.priority ?? 0,
      remain,
      held: !!control?.held,
      granted: !!control?.granted,
    }
  })
})

let timer = null
onMounted(() => {
  timer = setInterval(() => {
    now.value = Date.now()
  }, 1000)
  refresh()
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})

async function refresh() {
  loading.value = true
  try {
    await store.fetchDrones()
    for (const d of store.drones) {
      await store.fetchControl(d.drone_tag, 'flight')
    }
  } finally {
    loading.value = false
  }
}

async function onSelect(id) {
  await store.fetchControl(id, 'flight')
}

async function acquire(row) {
  const res = await store.acquireControl(row.drone_tag, 'flight', 10, leaseSeconds.value)
  if (!res?.granted) {
    ElMessage.warning(res?.reason || '控制权被占用')
  }
}

async function preempt(row) {
  const res = await preemptControl(row.drone_tag, 'flight', 20, leaseSeconds.value)
  if (!res?.granted) {
    ElMessage.warning(res?.reason || '抢占失败（需要更高的优先级）')
  } else {
    ElMessage.success(`已抢占（原持有人 ${res.preempted || '未知'}）`)
  }
  await store.fetchControl(row.drone_tag, 'flight')
}

async function release(row) {
  await store.releaseControl(row.drone_tag, 'flight')
}
</script>

<style scoped>
.drone-control {
  padding: 16px;
}
.page-head {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}
.lease-label {
  font-size: 12px;
  color: var(--text-muted);
}
</style>
