<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">登录会话</h1>
      <div class="page-toolbar">
        <el-button :icon="Refresh" :loading="loading" @click="fetchSessions">刷新</el-button>
      </div>
    </div>
    <div class="page-body">
      <DataCard>
        <el-alert
          v-if="!isSuper"
          type="info"
          :closable="false"
          show-icon
          style="margin-bottom: 12px"
          title="这里只显示你当前账号的登录会话；超级用户可以看到并管理全部账号的会话。"
        />
        <el-skeleton animated :loading="loading && sessions.length === 0" :rows="5">
          <el-table :data="sessions" border stripe style="width: 100%">
            <el-table-column v-if="isSuper" label="用户" min-width="120" align="center">
              <template #default="{ row }">
                <span>{{ row.username || row.user_id }}</span>
              </template>
            </el-table-column>
            <el-table-column label="客户端" width="110" align="center">
              <template #default="{ row }">
                <el-tag size="small" :type="row.client_type === 'desktop' ? 'success' : 'info'">
                  {{ clientTypeLabel(row.client_type) }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column label="设备名" min-width="160">
              <template #default="{ row }">{{ row.client_name || '-' }}</template>
            </el-table-column>
            <el-table-column label="来源 IP" width="140" align="center">
              <template #default="{ row }">
                <code class="stream-key">{{ row.client_ip || '-' }}</code>
              </template>
            </el-table-column>
            <el-table-column label="登录时间" width="180" align="center">
              <template #default="{ row }">{{ formatTime(row.created_at) }}</template>
            </el-table-column>
            <el-table-column label="最近活跃" width="180" align="center">
              <template #default="{ row }">{{ formatTime(row.last_seen_at) }}</template>
            </el-table-column>
            <el-table-column label="状态" width="100" align="center">
              <template #default="{ row }">
                <el-tag v-if="row.current" size="small" type="success">当前</el-tag>
                <el-tag v-else size="small" type="info">其他</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="操作" width="120" align="center">
              <template #default="{ row }">
                <el-button
                  type="danger"
                  link
                  size="small"
                  @click="confirmRevoke(row)"
                >
                  {{ row.current ? '下线当前' : '剔出' }}
                </el-button>
              </template>
            </el-table-column>
          </el-table>
        </el-skeleton>
      </DataCard>

      <div class="pagination-wrapper" v-if="total > pageSize">
        <el-pagination
          v-model:current-page="currentPage"
          :page-size="pageSize"
          :total="total"
          layout="prev, pager, next"
          @current-change="onPageChange"
        />
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { ElMessageBox } from 'element-plus'
import { getMySessions, getAllSessions, revokeSession } from '../api/auth'
import { useAuthStore } from '../stores/authStore'
import { useToast } from '../composables/useToast'
import DataCard from '../components/common/DataCard.vue'

const auth = useAuthStore()
const toast = useToast()

const sessions = ref([])
const loading = ref(false)
const total = ref(0)
const currentPage = ref(1)
const pageSize = 20

/** 超级用户看全部会话；普通用户只看自己的（服务端同样做了限制）。 */
const isSuper = computed(() => !!auth.currentUser?.is_super)

onMounted(fetchSessions)

async function fetchSessions() {
  loading.value = true
  try {
    const params = { limit: pageSize, offset: (currentPage.value - 1) * pageSize }
    const data = isSuper.value ? await getAllSessions(params) : await getMySessions(params)
    sessions.value = data.items || []
    total.value = data.total || 0
  } catch (e) {
    toast.error('获取登录会话失败: ' + (e.message || e))
  } finally {
    loading.value = false
  }
}

function onPageChange() {
  fetchSessions()
}

function clientTypeLabel(type) {
  if (type === 'desktop') return '桌面端'
  if (type === 'web') return '网页端'
  return type || '-'
}

function formatTime(value) {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString('zh-CN', { hour12: false })
}

/**
 * 剔出会话。
 *
 * 剔出当前会话会立刻退出登录；服务端吊销后目标端的 WebSocket 会收到 4401 并跳回登录页。
 */
async function confirmRevoke(row) {
  const tip = row.current
    ? '这是你当前的会话，剔出后会立即退出登录，确定继续吗？'
    : `确定要剔出会话 ${row.client_name || row.client_ip || row.id} 吗？`
  try {
    await ElMessageBox.confirm(tip, '剔出会话', {
      confirmButtonText: '确定',
      cancelButtonText: '取消',
      type: 'warning',
    })
  } catch {
    return
  }

  try {
    await revokeSession(row.id)
    toast.success('会话已吊销')
    if (row.current) {
      auth.logout()
      window.location.href = '/login'
      return
    }
    fetchSessions()
  } catch (e) {
    toast.error('吊销会话失败: ' + (e.message || e))
  }
}
</script>
