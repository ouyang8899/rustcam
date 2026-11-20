<template>
  <PageView :title="device?.name || '设备详情'">
    <template #toolbar>
      <el-button text :icon="ArrowLeft" @click="$router.push('/devices')">返回</el-button>
      <el-tag v-if="device?.protocol" size="small">{{ device.protocol }}</el-tag>
      <el-tag v-if="isGb28181" size="small" type="success">GB28181</el-tag>
      <el-tag v-if="isOnvif" size="small" type="info">ONVIF</el-tag>
      <el-button type="primary" :icon="Edit" @click="$router.push({ path: '/devices', query: { edit: route.params.deviceTag } })">编辑设备</el-button>
      <el-button type="warning" :icon="Close" @click="toggleMaintenance">维护</el-button>
    </template>

    <el-skeleton animated :loading="loading" :rows="6">
      <template #default>
        <div v-if="device">
          <el-row :gutter="12">
            <el-col :span="16">
              <DataCard title="设备信息">
                <el-descriptions :column="2" border size="small">
                  <el-descriptions-item label="状态">
                    <span class="status-dot" :class="getStatusClass(device.status)" style="margin-right: 6px" />{{ device.status }}
                  </el-descriptions-item>
                  <el-descriptions-item label="设备类型">{{ device.device_type || 'Other' }}</el-descriptions-item>
                  <el-descriptions-item label="主机地址">{{ device.host ? (device.port ? `${device.host}:${device.port}` : device.host) : '-' }}</el-descriptions-item>
                  <el-descriptions-item label="媒体服务器">{{ device.media_server_name || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="应用名称">{{ device.app || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="维护模式">
                    <el-switch
                      v-model="maintenanceMode"
                      active-text="开启"
                      inactive-text="关闭"
                      :loading="maintenanceLoading"
                      @change="toggleMaintenance"
                    />
                  </el-descriptions-item>
                  <el-descriptions-item label="创建时间">{{ formatDate(device.created_at) }}</el-descriptions-item>
                  <el-descriptions-item label="最后在线">{{ device.last_seen ? formatDate(device.last_seen) : '从未' }}</el-descriptions-item>
                </el-descriptions>
              </DataCard>

              <DataCard v-if="isGb28181" title="GB28181 配置" style="margin-top: 12px">
                <el-descriptions :column="2" border size="small">
                  <el-descriptions-item label="SIP ID">{{ device.extended?.gb_id || device.device_tag || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="所属地区">{{ device.region_code || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="视频编码">{{ device.extended?.stream_config?.video_codec || 'PS' }}</el-descriptions-item>
                  <el-descriptions-item label="音频编码">{{ device.extended?.stream_config?.audio_codec || 'PCMA' }}</el-descriptions-item>
                </el-descriptions>
              </DataCard>

              <DataCard v-if="isOnvif" title="ONVIF 配置" style="margin-top: 12px">
                <el-descriptions :column="2" border size="small">
                  <el-descriptions-item label="设备地址">{{ device.extended?.x_addr || device.host || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="厂商">{{ device.extended?.manufacturer || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="型号">{{ device.extended?.model || '-' }}</el-descriptions-item>
                  <el-descriptions-item label="能力">
                    <span v-if="device.extended?.capabilities?.media" style="color: var(--el-color-success)">媒体</span>
                    <span v-if="device.extended?.capabilities?.ptz" style="color: var(--el-color-success)"> PTZ</span>
                    <span v-if="device.extended?.capabilities?.events" style="color: var(--el-color-success)"> 事件</span>
                  </el-descriptions-item>
                </el-descriptions>
              </DataCard>
            </el-col>

            <el-col :span="8">
              <DataCard title="通道列表">
                <el-table :data="channels" border stripe size="small" style="width: 100%" v-if="channels.length">
                  <el-table-column prop="channel_tag" label="通道ID" />
                  <el-table-column prop="name" label="名称" />
                  <el-table-column prop="status" label="状态">
                    <template #default="{ row }">
                      <el-tag size="small" :type="row.status === 'ON' ? 'success' : 'info'">{{ row.status }}</el-tag>
                    </template>
                  </el-table-column>
                  <el-table-column label="操作" width="80">
                    <template #default="{ row }">
                      <el-button size="small" @click="$router.push(`/channels/${device.device_tag}/${row.channel_tag}`)">详情</el-button>
                    </template>
                  </el-table-column>
                </el-table>
                <EmptyState v-else title="暂无通道" />
              </DataCard>
            </el-col>
          </el-row>
        </div>
      </template>
    </el-skeleton>
  </PageView>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { ArrowLeft, Edit, Close } from '@element-plus/icons-vue'
import { useDeviceStore } from '../stores/deviceStore'
import * as channelsApi from '../api/channels'
import { useToast } from '../composables/useToast'
import PageView from '../components/common/PageView.vue'
import DataCard from '../components/common/DataCard.vue'
import EmptyState from '../components/common/EmptyState.vue'

const route = useRoute()
const deviceStore = useDeviceStore()
const toast = useToast()

const device = ref(null)
const channels = ref([])
const loading = ref(false)
const maintenanceMode = ref(false)
const maintenanceLoading = ref(false)

const proto = computed(() => (device.value?.protocol || '').toLowerCase())
const isOnvif = computed(() => proto.value === 'onvif')
const isGb28181 = computed(() => proto.value === 'gb28181')

onMounted(async () => {
  await fetchDevice()
  await fetchChannels()
})

async function fetchDevice() {
  loading.value = true
  try {
    device.value = await deviceStore.fetchDevice(route.params.deviceTag)
    maintenanceMode.value = device.value.status === 'Maintaining'
  } finally {
    loading.value = false
  }
}

async function fetchChannels() {
  if (!isGb28181.value && !isOnvif.value) return
  try {
    const data = await channelsApi.getChannels({ device_tag: route.params.deviceTag })
    channels.value = data?.items ?? data ?? []
  } catch {
    channels.value = []
  }
}

async function toggleMaintenance(val) {
  if (!device.value) return
  const newStatus = maintenanceMode.value ? 'Maintaining' : 'Online'
  maintenanceLoading.value = true
  try {
    await deviceStore.updateDevice(device.value.id, { status: newStatus }, device.value.device_tag)
    device.value.status = newStatus
    toast.success(maintenanceMode.value ? '已开启维护模式' : '已关闭维护模式')
  } catch (e) {
    maintenanceMode.value = !maintenanceMode.value
    toast.error('设置维护模式失败: ' + (e.message || e))
  } finally {
    maintenanceLoading.value = false
  }
}

function formatDate(dateStr) {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleString('zh-CN', { hour12: false })
}

function getStatusClass(status) {
  if (!status) return 'offline'
  const s = status.toLowerCase()
  if (s === 'online') return 'online'
  if (s === 'maintaining') return 'maintaining'
  if (s === 'error') return 'error'
  return 'offline'
}
</script>
