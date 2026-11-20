<template>
  <PageView :title="deviceTagFilter ? `设备通道: ${deviceTagFilter}` : '通道管理'" layout="workspace">
    <template #toolbar>
      <el-button v-if="deviceTagFilter" text @click="clearDeviceFilter">
        <el-icon><ArrowLeft /></el-icon> 返回设备列表
      </el-button>
      <el-button type="primary" @click="openCreateDialog">新建通道</el-button>
      <el-button :icon="Refresh" @click="fetchChannels">刷新</el-button>
    </template>

    <FilterBar>
      <template #left>
        <el-input v-model="searchQuery" placeholder="搜索通道..." clearable style="max-width: 200px" @change="onFilterChange" @clear="onFilterChange">
          <template #prefix><el-icon><Search /></el-icon></template>
        </el-input>
        <el-select v-model="statusFilter" clearable placeholder="全部状态" style="width: 110px" @change="onFilterChange">
          <el-option value="Online" label="在线" />
          <el-option value="Offline" label="离线" />
          <el-option value="Maintaining" label="维护中" />
          <el-option value="Error" label="错误" />
        </el-select>
      </template>
    </FilterBar>

    <WorkspaceLayout>
        <template #aside>
          <div class="tree-panel">
            <div class="data-card">
              <div class="data-card__header">
                <div class="group-header">
                  <span>分组</span>
                  <el-radio-group v-model="treeMode" size="small">
                    <el-radio-button value="region">地区</el-radio-button>
                    <el-radio-button value="custom">自定义</el-radio-button>
                  </el-radio-group>
                </div>
              </div>
              <div class="data-card__body">
                <GroupTree
                  :mode="treeMode === 'region' ? 'region' : 'group'"
                  :show-devices="false"
                  searchable
                  refreshable
                  fill
                  @node-click="onTreeNodeClick"
                />
              </div>
            </div>
          </div>
        </template>

        <DataCard>
          <el-skeleton animated :rows="5" :loading="channelStore.loading">
            <el-table :data="channelStore.channels" row-key="id" border stripe empty-text="暂无通道">
            <el-table-column label="状态" width="70" align="center">
              <template #default="{ row }">
                <StatusDot :status="getChannelStatusClass(row.status)" />
              </template>
            </el-table-column>
            <el-table-column label="通道名称" min-width="180">
              <template #default="{ row }">
                <el-link type="primary" @click="$router.push(`/channels/${row.device_tag}/${row.channel_tag}`)">{{ row.name }}</el-link>
              </template>
            </el-table-column>
            <el-table-column label="设备ID" min-width="180">
              <template #default="{ row }">
                <code class="stream-key" style="font-size: 11px">{{ row.device_tag }}</code>
              </template>
            </el-table-column>
            <el-table-column label="通道ID" min-width="180">
              <template #default="{ row }">
                <code class="stream-key" style="font-size: 11px">{{ row.channel_tag }}</code>
              </template>
            </el-table-column>
            <el-table-column label="类型" width="100" align="center">
              <template #default="{ row }">
                <el-tag size="small" type="primary">{{ row.device_type || 'IPC' }}</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="地址" min-width="140">
              <template #default="{ row }">
                <span v-if="row.ip_address">{{ row.port ? `${row.ip_address}:${row.port}` : row.ip_address }}</span>
                <span v-else>-</span>
              </template>
            </el-table-column>
            <el-table-column label="坐标" min-width="180">
              <template #default="{ row }">
                <span v-if="row.latitude != null && row.longitude != null">
                  {{ Number(row.latitude).toFixed(6) }}, {{ Number(row.longitude).toFixed(6) }}
                </span>
                <span v-else>-</span>
              </template>
            </el-table-column>
            <el-table-column label="厂商/型号" min-width="140">
              <template #default="{ row }">
                <span>{{ [row.manufacturer, row.model].filter(Boolean).join(' ') || '-' }}</span>
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
            <el-table-column label="操作" width="320" align="center">
              <template #default="{ row }">
                <el-button size="small" type="primary" @click="$router.push(`/channels/${row.device_tag}/${row.channel_tag}`)">详情</el-button>
                <el-button size="small" @click="openEditDialog(row)">编辑</el-button>
                <el-button size="small" @click="locateOnMap(row)">定位</el-button>
                <el-button size="small" @click="startChannelStream(row)">播放</el-button>
              </template>
            </el-table-column>
          </el-table>
          </el-skeleton>
        </DataCard>

        <template #footer>
          <div v-if="!channelStore.loading" class="workspace-footer">
            <PaginationBar
              v-model:page-size="channelStore.limit"
              :total="channelStore.total"
              :page="currentPage"
              show-sizes
              @update:page="onPageChange"
            />
          </div>
        </template>
    </WorkspaceLayout>

      <ChannelEditDialog
        v-model="editVisible"
        :mode="editMode"
        :device-tag="editingChannel?.device_tag || ''"
        :channel-tag="editingChannel?.channel_tag || ''"
        :channel="editingChannel"
        @saved="onChannelSaved"
      />
  </PageView>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Search, Refresh, ArrowLeft } from '@element-plus/icons-vue'
import { useChannelStore } from '../stores/channelStore'
import { useStreamStore } from '../stores/streamStore'
import StatusDot from '../components/common/StatusDot.vue'
import GroupTree from '../components/common/GroupTree.vue'
import DataCard from '../components/common/DataCard.vue'
import PageView from '../components/common/PageView.vue'
import WorkspaceLayout from '../components/common/WorkspaceLayout.vue'
import FilterBar from '../components/common/FilterBar.vue'
import PaginationBar from '../components/common/PaginationBar.vue'
import ChannelEditDialog from '../components/channel/ChannelEditDialog.vue'
import { useToast } from '../composables/useToast'

const route = useRoute()
const router = useRouter()
const channelStore = useChannelStore()
const streamStore = useStreamStore()
const toast = useToast()

const searchQuery = ref('')
const statusFilter = ref('')
const currentPage = ref(1)
const selectedGroup = ref(null)
const treeMode = ref('custom')
const selectedRegion = ref(null)
const selectedUnassignedRegion = ref(false)

const editVisible = ref(false)
const editMode = ref('edit')
const editingChannel = ref(null)

const deviceTagFilter = computed(() => route.query.device_tag || '')

function getChannelStatusClass(status) {
  if (!status) return 'offline'
  const s = status.toLowerCase()
  if (s === 'online') return 'online'
  if (s === 'maintaining') return 'maintaining'
  if (s === 'error') return 'error'
  return 'offline'
}

function onTreeNodeClick(data) {
  if (data.type === 'all') {
    selectedGroup.value = null
    selectedRegion.value = null
    selectedUnassignedRegion.value = false
  } else if (treeMode.value === 'region') {
    if (data.type === 'region') {
      selectedRegion.value = data.region_code ?? null
      selectedUnassignedRegion.value = false
    } else if (data.type === 'ungrouped') {
      selectedRegion.value = null
      selectedUnassignedRegion.value = true
    } else {
      return
    }
    selectedGroup.value = null
  } else {
    let groupId = null
    if (data.type === 'group') {
      groupId = data.id
    } else if (data.type === 'ungrouped') {
      groupId = 0
    } else {
      return
    }
    selectedGroup.value = groupId
    selectedRegion.value = null
    selectedUnassignedRegion.value = false
  }
  currentPage.value = 1
  channelStore.offset = 0
  fetchChannels()
}

async function fetchChannels() {
  await channelStore.fetchChannels({
    limit: channelStore.limit,
    offset: (currentPage.value - 1) * channelStore.limit,
    device_tag: deviceTagFilter.value || undefined,
    search: searchQuery.value || undefined,
    status: statusFilter.value || undefined,
    group_id: selectedGroup.value ?? undefined,
    region_code: selectedRegion.value ?? undefined,
    unassigned_region: selectedUnassignedRegion.value || undefined,
  })
}

function onFilterChange() {
  currentPage.value = 1
  channelStore.offset = 0
  fetchChannels()
}

function onPageChange(page) {
  channelStore.offset = (page - 1) * channelStore.limit
  fetchChannels()
}

function clearDeviceFilter() {
  router.push('/channels')
}

function locateOnMap(channel) {
  router.push({
    path: '/device-map',
    query: {
      device_tag: channel.device_tag,
      channel_tag: channel.channel_tag,
    },
  })
}

function openEditDialog(channel) {
  editingChannel.value = channel
  editMode.value = 'edit'
  editVisible.value = true
}

function openCreateDialog() {
  editingChannel.value = deviceTagFilter.value
    ? { device_tag: deviceTagFilter.value }
    : null
  editMode.value = 'create'
  editVisible.value = true
}

async function onChannelSaved() {
  await fetchChannels()
}

async function startChannelStream(channel) {
  try {
    await streamStore.startStream(channel.device_tag, channel.channel_tag)
    toast.success('已开始播放')
    router.push('/streams')
  } catch (e) {
    toast.error('启动播放失败: ' + (e?.message || e))
  }
}

onMounted(() => {
  fetchChannels()
})

watch(() => route.query.device_tag, () => {
  currentPage.value = 1
  channelStore.offset = 0
  fetchChannels()
})
</script>

<style scoped>
.group-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
</style>
