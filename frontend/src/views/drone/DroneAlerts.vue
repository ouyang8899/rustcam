<!-- @ts-check -->
<template>
  <PageView title="告警与审计" subtitle="围栏越界、上下线、指令与控制权操作留痕">
    <template #toolbar>
      <el-button :icon="Refresh" @click="refresh">刷新</el-button>
    </template>

    <FilterBar>
      <template #left>
        <el-select v-model="deviceId" clearable placeholder="全部无人机" style="width: 180px" @change="refresh">
          <el-option v-for="d in store.drones" :key="d.drone_tag" :label="d.name || d.drone_tag" :value="d.drone_tag" />
        </el-select>
      </template>
    </FilterBar>

    <DataCard>
      <el-tabs v-model="tab">
        <el-tab-pane label="告警" name="alerts">
          <el-table :data="pagedAlerts" border stripe size="small">
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
          <EmptyState v-if="store.alerts.length === 0" title="暂无告警" />
        </el-tab-pane>
      <el-tab-pane label="审计" name="audits">
        <el-table :data="pagedAudits" border stripe size="small">
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
        <EmptyState v-if="store.audits.length === 0" title="暂无审计记录" />
      </el-tab-pane>
      </el-tabs>
    </DataCard>

    <template #footer>
      <PaginationBar
        v-model:page-size="pageSize"
        :total="activeTotal"
        :page="activePage"
        show-sizes
        @update:page="onActivePageChange"
      />
    </template>
  </PageView>
</template>

<script setup>
import { computed, onMounted, ref } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import PageView from '../../components/common/PageView.vue'
import DataCard from '../../components/common/DataCard.vue'
import FilterBar from '../../components/common/FilterBar.vue'
import PaginationBar from '../../components/common/PaginationBar.vue'
import EmptyState from '../../components/common/EmptyState.vue'

const store = useDroneStore()
const tab = ref('alerts')
const deviceId = ref('')

const pageSize = ref(20)
const ALERT_WINDOW = 500
const AUDIT_WINDOW = 1000
const alertPage = ref(1)
const auditPage = ref(1)

const pagedAlerts = computed(() => {
  const start = (alertPage.value - 1) * pageSize.value
  return store.alerts.slice(start, start + pageSize.value)
})

const pagedAudits = computed(() => {
  const start = (auditPage.value - 1) * pageSize.value
  return store.audits.slice(start, start + pageSize.value)
})

const isAlertsTab = computed(() => tab.value === 'alerts')
const activeTotal = computed(() => (isAlertsTab.value ? store.alerts.length : store.audits.length))
const activePage = computed(() => (isAlertsTab.value ? alertPage.value : auditPage.value))

function onActivePageChange(page) {
  if (isAlertsTab.value) alertPage.value = page
  else auditPage.value = page
}

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
</style>
