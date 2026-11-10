<!-- @ts-check -->
<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">告警与审计</h1>
        <span class="header-sub">围栏越界、上下线、指令与控制权操作留痕</span>
      </div>
      <div class="page-toolbar">
        <el-select v-model="deviceId" clearable placeholder="全部无人机" style="width: 180px" @change="refresh">
          <el-option v-for="d in store.drones" :key="d.drone_tag" :label="d.name || d.drone_tag" :value="d.drone_tag" />
        </el-select>
        <el-button :icon="Refresh" @click="refresh">刷新</el-button>
      </div>
    </div>

    <div class="page-body">
      <el-tabs v-model="tab">
        <el-tab-pane label="告警" name="alerts">
          <el-table :data="pagedAlerts" stripe>
            <el-table-column prop="ts_ms" label="时间" width="180">
              <template #default="{ row }">{{ formatTime(row.ts_ms) }}</template>
            </el-table-column>
            <el-table-column prop="drone_tag" label="无人机" width="150" />
            <el-table-column label="规则" width="160">
              <template #default="{ row }">
                <el-tag size="small" :type="row.rule === 'geofence_breach' ? 'danger' : 'info'">{{ row.rule }}</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="级别" width="90">
              <template #default="{ row }">
                <el-tag size="small" :type="row.level === 'high' ? 'danger' : row.level === 'warning' ? 'warning' : 'info'">
                  {{ row.level }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column prop="message" label="内容" min-width="320" />
          </el-table>
          <el-empty v-if="store.alerts.length === 0" description="暂无告警" />
          <div v-if="store.alerts.length > pageSize" class="pagination-wrapper">
            <el-pagination
              v-model:current-page="alertPage"
              :page-size="pageSize"
              :total="store.alerts.length"
              layout="prev, pager, next"
            />
          </div>
        </el-tab-pane>
        <el-tab-pane label="审计" name="audits">
          <el-table :data="pagedAudits" stripe>
            <el-table-column prop="ts_ms" label="时间" width="180">
              <template #default="{ row }">{{ formatTime(row.ts_ms) }}</template>
            </el-table-column>
            <el-table-column prop="drone_tag" label="无人机" width="140" />
            <el-table-column prop="category" label="类别" width="100" />
            <el-table-column prop="action" label="动作" width="120" />
            <el-table-column prop="operator" label="操作者" width="120" />
            <el-table-column label="结果" width="110">
              <template #default="{ row }">
                <el-tag size="small" :type="['ok', 'accepted', 'sent'].includes(row.result) ? 'success' : 'warning'">
                  {{ row.result }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column prop="detail" label="详情" min-width="260" />
          </el-table>
          <el-empty v-if="store.audits.length === 0" description="暂无审计记录" />
          <div v-if="store.audits.length > pageSize" class="pagination-wrapper">
            <el-pagination
              v-model:current-page="auditPage"
              :page-size="pageSize"
              :total="store.audits.length"
              layout="prev, pager, next"
            />
          </div>
        </el-tab-pane>
      </el-tabs>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, ref } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'

const store = useDroneStore()
const tab = ref('alerts')
const deviceId = ref('')

/**
 * 告警/审计各取最近的一段再分页。
 *
 * 服务端只保留内存窗口（告警 2000 条、审计 5000 条），
 * 因此这里拉满一页窗口后在表格上翻页，而不是逐页回服务端。
 */
const pageSize = 20
const ALERT_WINDOW = 500
const AUDIT_WINDOW = 1000
const alertPage = ref(1)
const auditPage = ref(1)

const pagedAlerts = computed(() => {
  const start = (alertPage.value - 1) * pageSize
  return store.alerts.slice(start, start + pageSize)
})

const pagedAudits = computed(() => {
  const start = (auditPage.value - 1) * pageSize
  return store.audits.slice(start, start + pageSize)
})

function formatTime(ms) {
  if (!ms) return '—'
  return new Date(ms).toLocaleString('zh-CN', { hour12: false })
}

async function refresh() {
  alertPage.value = 1
  auditPage.value = 1
  await Promise.all([
    store.fetchAlerts(deviceId.value, ALERT_WINDOW),
    store.fetchAudits(deviceId.value, AUDIT_WINDOW),
  ])
}

onMounted(async () => {
  await store.fetchDrones()
  await refresh()
})
</script>

<style scoped>
.header-left {
  display: flex;
  align-items: baseline;
  gap: 12px;
}
.header-sub {
  font-size: 12px;
  color: var(--text-muted);
}
.pagination-wrapper {
  display: flex;
  justify-content: center;
  margin-top: var(--space-4);
}
</style>
