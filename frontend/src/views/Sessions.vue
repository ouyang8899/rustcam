<template>
  <PageView title="会话管理">
    <template #toolbar>
      <el-button :icon="Refresh" :loading="loading" @click="fetchSessions">刷新</el-button>
    </template>

    <FilterBar>
      <template #left>
        <el-input
          v-model="searchQuery"
          placeholder="搜索..."
          clearable
          style="max-width: 200px"
          @change="onFilterChange"
          @clear="onFilterChange"
        >
          <template #prefix>
            <el-icon><Search /></el-icon>
          </template>
        </el-input>
        <el-select v-model="stateFilter" clearable placeholder="全部状态" style="width: 120px" @change="onFilterChange">
          <el-option value="Active" label="活跃" />
          <el-option value="Initializing" label="初始化" />
          <el-option value="Inactive" label="不活跃" />
        </el-select>
      </template>
    </FilterBar>

    <DataCard>
        <el-skeleton animated :loading="loading && sessions.length === 0" :rows="5">
          <el-table :data="filteredSessions" border stripe style="width: 100%">
              <el-table-column label="ID" prop="id" width="80" align="center" />
              <el-table-column label="类型" prop="session_type" width="100" align="center">
                <template #default="{ row }">
                  <el-tag size="small">{{ sessionTypeLabel(row.session_type) }}</el-tag>
                </template>
              </el-table-column>
              <el-table-column label="设备标识" prop="device_tag" width="140" align="center">
                <template #default="{ row }">
                  <code class="stream-key">{{ row.device_tag || '-' }}</code>
                </template>
              </el-table-column>
              <el-table-column label="流标识" min-width="180">
                <template #default="{ row }">
                  <code class="stream-key">{{ row.device_tag }}_{{ row.channel_tag }}</code>
                </template>
              </el-table-column>
              <el-table-column label="用户ID" prop="user_id" width="100" align="center" />
              <el-table-column label="客户端IP" width="130" align="center">
                <template #default="{ row }">
                  <code class="stream-key">{{ row.client_ip }}</code>
                </template>
              </el-table-column>
              <el-table-column label="协议" prop="protocol" width="100" align="center">
                <template #default="{ row }">{{ row.protocol || '-' }}</template>
              </el-table-column>
              <el-table-column label="状态" width="100" align="center">
                <template #default="{ row }">
                  <el-tag :type="sessionStateType(row.state)" size="small">{{ sessionStateLabel(row.state) }}</el-tag>
                </template>
              </el-table-column>
              <el-table-column label="媒体服务器" width="160" align="center">
                <template #default="{ row }">
                  <span v-if="row.media_server_name">
                    <el-tag size="small" :type="row.media_server_online ? 'success' : 'danger'" style="margin-right: 4px">
                      {{ row.media_server_online ? '在线' : '离线' }}
                    </el-tag>
                    {{ row.media_server_name }}
                  </span>
                  <span v-else-if="row.media_server_tag" style="color: #E6A23C">
                    {{ row.media_server_tag }}
                  </span>
                  <span v-else>-</span>
                </template>
              </el-table-column>
              <el-table-column label="创建时间" width="180" align="center">
                <template #default="{ row }">{{ formatTime(row.created_at) }}</template>
              </el-table-column>
              <el-table-column label="最后活动" width="180" align="center">
                <template #default="{ row }">{{ formatTime(row.last_activity) }}</template>
              </el-table-column>
            </el-table>
        </el-skeleton>
      </DataCard>

      <template #footer>
        <PaginationBar
          v-model:page-size="pageSize"
          :total="total"
          :page="currentPage"
          show-sizes
          @update:page="onPageChange"
        />
      </template>
  </PageView>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { Search, Refresh } from '@element-plus/icons-vue'
import { getSessions } from '../api/sessions'
import DataCard from '../components/common/DataCard.vue'
import PageView from '../components/common/PageView.vue'
import FilterBar from '../components/common/FilterBar.vue'
import PaginationBar from '../components/common/PaginationBar.vue'

const sessions = ref([])
const loading = ref(false)
const searchQuery = ref('')
const stateFilter = ref('')
const currentPage = ref(1)
const pageSize = ref(20)
const total = ref(0)

const filteredSessions = computed(() => {
  return sessions.value
})

let refreshTimer = null

onMounted(async () => {
  await fetchSessions()
  refreshTimer = setInterval(fetchSessions, 15000)
})

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer)
})

async function fetchSessions() {
  loading.value = true
  try {
    const offset = (currentPage.value - 1) * pageSize.value
    const data = await getSessions({
      limit: pageSize.value,
      offset,
      search: searchQuery.value || undefined,
      state: stateFilter.value || undefined,
    })
    sessions.value = data?.items || []
    total.value = data?.total || 0
  } catch (e) {
    console.error('fetchSessions error:', e)
  } finally {
    loading.value = false
  }
}

function onFilterChange() {
  currentPage.value = 1
  fetchSessions()
}

async function onPageChange(page) {
  currentPage.value = page
  await fetchSessions()
}

function sessionTypeLabel(type) {
  const map = { Play: '播放', Talk: '对讲', Push: '推流' }
  return map[type] || type || '-'
}

function sessionStateLabel(state) {
  const map = { Active: '活跃', Initializing: '初始化', Inactive: '不活跃' }
  return map[state] || state || '-'
}

function sessionStateType(state) {
  const map = { Active: 'success', Initializing: 'warning', Inactive: 'info' }
  return map[state] || ''
}

function formatTime(ts) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (isNaN(d)) return ts
  return d.toLocaleString('zh-CN', { hour12: false })
}
</script>

<style scoped>
.stream-key {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--text-secondary);
}
</style>
