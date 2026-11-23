<template>
  <PageView title="设备管理" layout="workspace">
    <template #toolbar>
      <el-button type="primary" :icon="Plus" @click="openAddModal()">添加设备</el-button>
    </template>

    <FilterBar>
      <template #left>
        <el-input v-model="searchQuery" placeholder="搜索设备..." clearable style="max-width: 200px" @change="onFilterChange">
          <template #prefix><el-icon><Search /></el-icon></template>
        </el-input>
        <el-select v-model="protocolFilter" clearable placeholder="全部协议" style="width: 110px" @change="onFilterChange">
          <el-option value="" label="全部协议" />
          <el-option value="Rtsp" label="RTSP" />
          <el-option value="Gb28181" label="GB28181" />
          <el-option value="Onvif" label="ONVIF" />
          <el-option value="Rtmp" label="RTMP" />
        </el-select>
        <el-select v-model="statusFilter" clearable placeholder="全部状态" style="width: 120px" @change="onFilterChange">
          <el-option value="" label="全部状态" />
          <el-option value="Online" label="在线" />
          <el-option value="Offline" label="离线" />
          <el-option value="Maintaining" label="维护中" />
          <el-option value="Error" label="错误" />
        </el-select>
        <el-select v-model="deviceTypeFilter" clearable placeholder="全部类型" style="width: 100px" @change="onFilterChange">
          <el-option value="" label="全部类型" />
          <el-option value="NVR" label="NVR" />
          <el-option value="IPC" label="IPC" />
          <el-option value="DVR" label="DVR" />
          <el-option value="Camera" label="Camera" />
          <el-option value="Platform" label="Platform" />
          <el-option value="Other" label="Other" />
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
                  <el-radio-group v-model="groupMode" size="small" style="float:right;">
                    <el-radio-button value="region">地区</el-radio-button>
                    <el-radio-button value="custom">自定义</el-radio-button>
                  </el-radio-group>
                </div>
              </div>
              <div class="data-card__body" v-if="groupMode === 'region'">
                <GroupTree
                  mode="region"
                  :show-devices="false"
                  searchable
                  refreshable
                  fill
                  @node-click="onRegionNodeClick"
                />
              </div>
              <div class="data-card__body" v-else>
                <el-button size="small" :icon="Plus" style="margin-bottom: 8px" @click="addRootGroup">新建分组</el-button>
                <GroupTree
                  :show-devices="false"
                  searchable
                  refreshable
                  fill
                  @node-click="onGroupNodeClick"
                  @node-contextmenu="onGroupContextmenu"
                />

                <teleport to="body">
                  <div
                    v-if="contextmenuVisible"
                    class="group-contextmenu"
                    :style="{ left: contextmenuX + 'px', top: contextmenuY + 'px' }"
                    @click.stop
                  >
                    <div class="ctx-item" @click="handleCtxAddChild"><el-icon><FolderAdd /></el-icon> 新建子分组</div>
                    <div class="ctx-item" @click="handleCtxRename"><el-icon><Edit /></el-icon> 重命名</div>
                    <div class="ctx-item ctx-danger" @click="handleCtxDelete"><el-icon><Delete /></el-icon> 删除</div>
                  </div>
                </teleport>
              </div>
            </div>
          </div>
        </template>

        <DataCard>
          <el-skeleton animated :rows="5" :loading="tableLoading">
              <el-table :data="filteredDevices" row-key="id" border stripe empty-text="暂无设备">
                  <el-table-column label="状态" width="70" align="center">
                    <template #default="{ row }">
                      <StatusDot :status="getDeviceStatusClass(row.status)" />
                    </template>
                  </el-table-column>
                  <el-table-column label="名称" min-width="180">
                    <template #default="{ row }">
                      <el-link type="primary" @click="$router.push(`/devices/${row.device_tag}`)">{{ row.name }}</el-link>
                    </template>
                  </el-table-column>
                  <el-table-column label="协议" width="100" align="center">
                    <template #default="{ row }">
                      <el-tag size="small" class="protocol-tag" :class="`protocol-${row.protocol?.toLowerCase()}`">{{ row.protocol }}</el-tag>
                    </template>
                  </el-table-column>
                  <el-table-column label="类型" min-width="100" align="center">
                    <template #default="{ row }">
                      <el-tag v-if="row.is_channel" size="small" type="warning">通道</el-tag>
                      <el-tag v-else-if="row.protocol && row.protocol.toLowerCase().includes('gb') && row.device_tag && row.device_tag.length >= 13" size="small" type="primary">
                        {{ getDeviceTypeLabel(row.device_tag.substring(10, 13)) || row.device_type || 'Other' }}
                      </el-tag>
                      <el-tag v-else size="small" type="info">{{ row.device_type || 'Other' }}</el-tag>
                    </template>
                  </el-table-column>
                  <el-table-column label="地址" min-width="140">
                    <template #default="{ row }">
                      <code class="stream-key" style="font-size: 11px">{{ row.host ? (row.port ? `${row.host}:${row.port}` : row.host) : '-' }}</code>
                    </template>
                  </el-table-column>
                  <el-table-column label="流源" width="110">
                    <template #default="{ row }">
                      <span v-if="row.pull_urls?.length">拉 {{ row.pull_urls.length }}</span>
                      <span v-if="row.push_urls?.length">推 {{ row.push_urls.length }}</span>
                      <span v-if="!row.pull_urls?.length && !row.push_urls?.length">-</span>
                    </template>
                  </el-table-column>
                  <el-table-column label="观看" prop="view_count" width="80" align="center" />
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
                  <el-table-column label="操作" width="220" align="center">
                    <template #default="{ row }">
                      <el-button v-if="row.has_stream" size="small" type="primary" plain @click="router.push('/streams')">流</el-button>
                      <el-button size="small" @click="router.push(`/channels?device_tag=${row.device_tag}`)">通道</el-button>
                      <el-button size="small" @click="router.push(`/devices/${row.device_tag}`)">详情</el-button>
                      <el-button size="small" @click="openEditModal(row)">编辑</el-button>
                      <el-button size="small" type="danger" plain @click="deleteDevice(row)">删除</el-button>
                    </template>
                  </el-table-column>
              </el-table>
          </el-skeleton>
        </DataCard>


        <template #footer>
          <div v-if="!tableLoading" class="workspace-footer">
            <PaginationBar
              v-model:page-size="deviceStore.limit"
              :total="deviceStore.total"
              :page="currentPage"
              show-sizes
              @update:page="onPageChange"
            />
          </div>
        </template>
    </WorkspaceLayout>

    <AppDialog v-model="showModal" :title="isEdit ? '编辑设备' : '添加设备'" size="lg" draggable>
      <template v-if="!isEdit">
        <el-tabs v-model="activeDeviceTab" tab-position="top">
          <el-tab-pane label="GB28181" name="gb28181">
            <Gb28181DeviceForm
              ref="gbFormRef"
              :media-server-options="mediaServerStore.servers"
              :region-tree="regionStore.regionTree"
              :industry-options="gbIndustryCodeOptions"
              :device-type-options="gbDeviceTypeOptions"
              :network-options="gbNetworkCodeOptions"
              />
            </el-tab-pane>
            <el-tab-pane label="RTSP" name="rtsp">
            <RtspDeviceForm
              ref="rtspFormRef"
              v-model="form"
              :media-server-options="mediaServerStore.servers"
              :region-tree="regionStore.regionTree"
              :push-address="rtspPushAddress"
              :push-port="rtspPushPort"
            />
          </el-tab-pane>
          <el-tab-pane label="ONVIF" name="onvif">
            <OnvifDeviceForm
              ref="onvifFormRef"
              v-model="form"
              :media-server-options="mediaServerStore.servers"
              :region-tree="regionStore.regionTree"
            />
          </el-tab-pane>
          <el-tab-pane label="RTMP" name="rtmp">
            <RtmpDeviceForm
              ref="rtmpFormRef"
              v-model="form"
              :media-server-options="mediaServerStore.servers"
              :region-tree="regionStore.regionTree"
              :push-address="rtmpPushAddress"
              :push-port="rtmpPushPortActual"
            />
          </el-tab-pane>
        </el-tabs>
      </template>
      <template v-else>
        <Gb28181DeviceForm v-if="form.protocol === 'Gb28181'" :key="editingId" ref="gbFormRef"
          :media-server-options="mediaServerStore.servers"
          :region-tree="regionStore.regionTree"
          :industry-options="gbIndustryCodeOptions"
          :device-type-options="gbDeviceTypeOptions"
          :network-options="gbNetworkCodeOptions"
          :device-tag="form.device_tag"
        />
        <RtspDeviceForm v-else-if="form.protocol === 'Rtsp'" :key="editingId" ref="rtspFormRef"
          :media-server-options="mediaServerStore.servers"
          :region-tree="regionStore.regionTree"
          :push-address="rtspPushAddress"
          :push-port="rtspPushPort"
          :device-tag="form.device_tag"
        />
        <OnvifDeviceForm v-else-if="form.protocol === 'Onvif'" :key="editingId" ref="onvifFormRef"
          :media-server-options="mediaServerStore.servers"
          :region-tree="regionStore.regionTree"
          :device-tag="form.device_tag"
        />
        <RtmpDeviceForm v-else-if="form.protocol === 'Rtmp'" :key="editingId" ref="rtmpFormRef"
          :media-server-options="mediaServerStore.servers"
          :region-tree="regionStore.regionTree"
          :push-address="rtmpPushAddress"
          :push-port="rtmpPushPortActual"
          :device-tag="form.device_tag"
        />
      </template>
      <template #footer>
        <el-button @click="closeModal">取消</el-button>
        <el-button type="primary" @click="submitDevice">{{ isEdit ? '保存修改' : '确认添加' }}</el-button>
      </template>
    </AppDialog>

    <AppDialog v-model="showPlayLinksModal" :title="`播放链接: ${selectedDevice?.name}`" size="md" draggable>
      <el-descriptions :column="1" border v-if="availablePlayLinks.length > 0">
        <el-descriptions-item v-for="link in availablePlayLinks" :key="link.key" :label="link.label">
          <div style="display: flex; gap: 8px; align-items: center">
            <el-text truncated style="flex: 1; font-family: monospace; font-size: 12px">{{ link.url }}</el-text>
            <el-button size="small" @click="copyLink(link.url)">复制</el-button>
          </div>
        </el-descriptions-item>
      </el-descriptions>
      <EmptyState v-else title="暂无播放链接" />
    </AppDialog>
  </PageView>
</template>

<script setup>
import { ref, computed, onMounted, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { ElMessageBox } from 'element-plus'
import { Search, Refresh, Plus, Delete, Edit, FolderAdd } from '@element-plus/icons-vue'
import { useDeviceStore } from '../stores/deviceStore'
import { useMediaServerStore } from '../stores/mediaServerStore'
import { useRegionStore } from '../stores/regionStore'
import { useGroupStore } from '../stores/groupStore'
import { useStreamStore } from '../stores/streamStore'
import { useGb28181RefStore } from '../stores/gb28181RefStore'
import { request } from '../utils/request'
import { useToast } from '../composables/useToast'
import StatusDot from '../components/common/StatusDot.vue'
import DataCard from '../components/common/DataCard.vue'
import GroupTree from '../components/common/GroupTree.vue'
import PageView from '../components/common/PageView.vue'
import WorkspaceLayout from '../components/common/WorkspaceLayout.vue'
import FilterBar from '../components/common/FilterBar.vue'
import PaginationBar from '../components/common/PaginationBar.vue'
import EmptyState from '../components/common/EmptyState.vue'
import AppDialog from '../components/common/AppDialog.vue'
import RtspDeviceForm from './device-forms/RtspDeviceForm.vue'
import Gb28181DeviceForm from './device-forms/Gb28181DeviceForm.vue'
import OnvifDeviceForm from './device-forms/OnvifDeviceForm.vue'
import RtmpDeviceForm from './device-forms/RtmpDeviceForm.vue'

const deviceStore = useDeviceStore()
const mediaServerStore = useMediaServerStore()
const regionStore = useRegionStore()
const groupStore = useGroupStore()
const streamStore = useStreamStore()
const gb28181RefStore = useGb28181RefStore()
const toast = useToast()
const router = useRouter()
const route = useRoute()



function protocolColor(protocol) {
  const map = {
    'GB28181': 'var(--protocol-gb28181)',
    'RTSP': 'var(--protocol-rtsp)',
    'ONVIF': 'var(--protocol-onvif)',
    'RTMP': 'var(--protocol-rtmp)',
  }
  return map[protocol] || 'var(--text-muted)'
}

function getDeviceStatusClass(status) {
  if (!status) return 'offline'
  const s = status.toLowerCase()
  if (s === 'online') return 'online'
  if (s === 'maintaining') return 'maintaining'
  if (s === 'error') return 'error'
  return 'offline'
}

const tableLoading = computed(() => deviceStore.loading)

const showModal = ref(false)
const searchQuery = ref('')
const protocolFilter = ref('')
const statusFilter = ref('')
const deviceTypeFilter = ref('')
const isEdit = ref(false)
const editingId = ref(null)
const showPlayLinksModal = ref(false)
const selectedDevice = ref(null)
const playLinks = ref(null)
const gbRegionCascader = ref([])
const sipGbCode = ref('00')
const sipIdSuffix = ref('')
const gbFormRef = ref(null)
const rtspFormRef = ref(null)
const onvifFormRef = ref(null)
const rtmpFormRef = ref(null)

watch(sipGbCode, (val) => {
  const num = parseInt(val, 10)
  if (isNaN(num) || num < 0) {
    sipGbCode.value = '00'
  } else if (val.length > 2) {
    sipGbCode.value = val.slice(0, 2)
  } else {
    sipGbCode.value = val.padStart(2, '0')
  }
})

const selectedRegion = ref(null)
const selectedUnassignedRegion = ref(false)
const groupMode = ref('region')
const selectedGroup = ref(null)
const contextmenuVisible = ref(false)
const contextmenuX = ref(0)
const contextmenuY = ref(0)
const contextmenuNode = ref(null)

const currentPage = ref(Math.floor(deviceStore.offset / deviceStore.limit) + 1)

let copiedTimer = null

const sipIdPrefix = computed(() => {
  if (!gbRegionCascader.value || gbRegionCascader.value.length === 0) return ''
  const code = gbRegionCascader.value[gbRegionCascader.value.length - 1] || ''
  return code
})

function findRegionPath(code, tree, path = []) {
  for (const node of tree) {
    const currentPath = [...path, node.gb28181_code]
    if (node.gb28181_code === code) {
      return currentPath
    }
    if (node.children && node.children.length > 0) {
      const found = findRegionPath(code, node.children, currentPath)
      if (found) return found
    }
  }
  return null
}

const sipIdTotalLength = 20
const sipIdSuffixMaxLength = 6  // suffix is always 6 characters

const sipIdPreview = computed(() => {
  if (!sipIdPrefix.value) return ''
  const suffix = sipIdSuffix.value || ''
  const paddedSuffix = suffix.padEnd(6, '_')
  const gbCode = sipGbCode.value || '00'
  return sipIdPrefix.value + gbCode + sipIdIndustryCode.value + (form.value.gb_device_type || '') + sipIdNetworkCode.value + paddedSuffix
})

function onGbRegionChange(val) {
}

const rtspPushServer = computed(() => {
  if (!form.value.media_server_tag) return null
  return mediaServerStore.servers.find(s => s.server_tag === form.value.media_server_tag) || null
})

const rtspPushAddress = computed(() => {
  const server = rtspPushServer.value
  if (!server) return ''
  return server.url || ''
})

const rtspPushPort = computed(() => {
  const server = rtspPushServer.value
  if (!server) return 554
  return server.protocol_ports?.rtsp || 554
})

const rtmpPushServer = computed(() => {
  if (!form.value.media_server_tag) return null
  return mediaServerStore.servers.find(s => s.server_tag === form.value.media_server_tag) || null
})

const rtmpPushAddress = computed(() => {
  const server = rtmpPushServer.value
  if (!server) return ''
  return server.url || ''
})

const rtmpPushPortActual = computed(() => {
  const server = rtmpPushServer.value
  if (!server) return 1935
  return server.protocol_ports?.rtmp || 1935
})


const availablePlayLinks = computed(() => {
  if (!playLinks.value) return []
  return [
    { key: 'rtsp_signaling', label: 'RTSP (信令)', url: playLinks.value.rtsp_signaling },
    { key: 'rtsp_media', label: 'RTSP (直连)', url: playLinks.value.rtsp_media },
    { key: 'flv', label: 'HTTP-FLV', url: playLinks.value.flv },
    { key: 'web_flv', label: 'Web-FLV', url: playLinks.value.web_flv },
    { key: 'hls', label: 'HLS', url: playLinks.value.hls },
    { key: 'webrtc', label: 'WebRTC', url: playLinks.value.webrtc },
  ].filter(l => l.url)
})

const filteredDevices = computed(() => {
  return deviceStore.devices
})

const defaultForm = () => ({
  name: '', protocol: 'Gb28181', host: '', port: 554, device_username: '', device_password: '',
  media_server_tag: '', pull_urls: [], push_urls: [], playback_username: '', playback_password: '',
  device_tag: '', group_id: null, is_public: false,
  rtsp_mode: 'pull', app: 'live', rtsp_full_url: '',
  rtmp_mode: 'push',
  gb_device_type: '132',
  gb_industry_code: '00',
  gb_network_code: '0',
  sip_id_suffix: '',
  extended: {},
  enable_stream_config: false,
  stream_config: {
    video_codec: 'PS',
    audio_codec: 'PCMA',
    video_payload_type: 96,
    audio_payload_type: 8,
    profile_level_id: '4D001F',
    packaging_mode: 'HIS',
    sprop_parameter_sets: '',
    stream_mode: 'recvonly',
    media_transport: '',
  },
})

const gb28181Tab = ref('industry')
const gbNetworkCodeOptions = computed(() => gb28181RefStore.networkCodeOptions)
const gbIndustryCodeOptions = computed(() => gb28181RefStore.industryCodeOptions)
const gbDeviceTypeOptions = computed(() => gb28181RefStore.deviceTypeOptions)

const getNetworkCodeLabel = (code) => {
  const opt = gbNetworkCodeOptions.value.find(o => o.value === code)
  return opt ? opt.label : ''
}
const getDeviceTypeLabel = (code) => {
  const opt = gbDeviceTypeOptions.value.find(o => o.value === code)
  return opt ? opt.label : ''
}
const getIndustryCodeLabel = (code) => {
  const opt = gbIndustryCodeOptions.value.find(o => o.value === code)
  return opt ? opt.label : ''
}

const sipIdIndustryCode = computed(() => form.value.gb_industry_code || '')
const sipIdNetworkCode = computed(() => form.value.gb_network_code || '')

const activeProtocolTab = ref('Gb28181')
const activeDeviceTab = computed({
  get: () => {
    switch (form.value.protocol) {
      case 'Gb28181': return 'gb28181'
      case 'Rtsp': return 'rtsp'
      case 'Onvif': return 'onvif'
      case 'Rtmp': return 'rtmp'
      default: return 'gb28181'
    }
  },
  set: (val) => {
    const protoMap = { gb28181: 'Gb28181', rtsp: 'Rtsp', onvif: 'Onvif', rtmp: 'Rtmp' }
    form.value.protocol = protoMap[val] || 'Gb28181'
  }
})

function onProtocolTabChange(tab) {
  const protoMap = { Gb28181: 'gb28181', Rtsp: 'rtsp', Onvif: 'onvif', Rtmp: 'rtmp' }
  activeDeviceTab.value = protoMap[tab] || 'gb28181'
  form.value.protocol = tab
}

const protocolLabel = computed(() => {
  const map = { Gb28181: 'GB28181', Rtsp: 'RTSP', Onvif: 'ONVIF', Rtmp: 'RTMP' }
  return map[form.value.protocol] || form.value.protocol
})

const form = ref(defaultForm())

watch(() => form.value.protocol, (newProto) => {
  if (newProto !== 'Rtsp') {
    form.value.rtsp_mode = 'pull'
    form.value.rtsp_full_url = ''
  }
  if (newProto !== 'Rtmp') {
    form.value.rtmp_mode = 'push'
  }
  if (newProto === 'Gb28181') {
    form.value.app = 'rtp'
  } else if (newProto !== 'Rtsp') {
    form.value.app = 'live'
  }
})



watch(() => form.value.rtsp_mode, (newMode) => {
  if (form.value.protocol !== 'Rtsp') return
  if (newMode === 'push') {
    form.value.host = ''
    form.value.port = 554
    form.value.device_username = ''
    form.value.device_password = ''
    form.value.rtsp_full_url = ''
  } else {
    form.value.app = 'live'
    form.value.rtsp_full_url = ''
    form.value.host = ''
    form.value.port = 554
    form.value.device_username = ''
    form.value.device_password = ''
  }
})

onMounted(async () => {
  await Promise.all([
    fetchDevices(),
    mediaServerStore.fetchServers(),
    fetchRegionTree(),
    fetchGroupTree(),
    gb28181RefStore.fetchRefData(),
  ])
  document.addEventListener('click', closeContextmenu)

  const editId = route.query.edit
  if (editId) {
    const device = deviceStore.devices.find(d => String(d.device_tag) === String(editId))
    if (device) openEditModal(device)
  }
})

async function fetchDevices() {
  const regionCode = groupMode.value === 'region' ? selectedRegion.value : null
  const groupId = groupMode.value === 'custom' ? selectedGroup.value : null
  const unassignedRegion = groupMode.value === 'region' ? selectedUnassignedRegion.value : null
  const search = searchQuery.value || null
  const protocol = protocolFilter.value || null
  const status = statusFilter.value || null
  const deviceType = deviceTypeFilter.value || null
  await deviceStore.fetchDevices(deviceStore.limit, deviceStore.offset, regionCode, search, protocol, status, deviceType, groupId, unassignedRegion)
}

async function fetchRegionTree() {
  await regionStore.fetchRegionTree()
}

async function fetchGroupTree() {
  await groupStore.fetchGroupTree()
}

watch(groupMode, () => {
  selectedRegion.value = null
  selectedUnassignedRegion.value = false
  selectedGroup.value = null
  currentPage.value = 1
  deviceStore.offset = 0
  fetchDevices()
})

function onRegionNodeClick(data) {
  if (data.type === 'region') {
    selectedRegion.value = data.region_code ?? null
    selectedUnassignedRegion.value = false
  } else if (data.type === 'ungrouped') {
    selectedRegion.value = null
    selectedUnassignedRegion.value = true
  } else if (data.type === 'all') {
    selectedRegion.value = null
    selectedUnassignedRegion.value = false
    selectedGroup.value = null
  } else {
    return
  }
  currentPage.value = 1
  deviceStore.offset = 0
  fetchDevices()
}
function onFilterChange() {
  currentPage.value = 1
  deviceStore.offset = 0
  fetchDevices()
}

function onPageChange(page) {
  currentPage.value = page
  deviceStore.offset = (page - 1) * deviceStore.limit
  fetchDevices()
}
function onGroupNodeClick(data) {
  let groupId = null
  if (data.type === 'group') {
    groupId = data.id
  } else if (data.type === 'ungrouped') {
    groupId = 0
  } else if (data.type === 'all') {
    groupId = null
  } else {
    return
  }
  selectedGroup.value = groupId
  selectedRegion.value = null
  selectedUnassignedRegion.value = false
  currentPage.value = 1
  deviceStore.offset = 0
  fetchDevices()
}

function onGroupContextmenu(data, node, component, event) {
  if (data.type !== 'group') return
  contextmenuNode.value = data
  contextmenuX.value = event.clientX
  contextmenuY.value = event.clientY
  contextmenuVisible.value = true
}

function closeContextmenu() {
  contextmenuVisible.value = false
  contextmenuNode.value = null
}

function handleCtxAddChild() {
  if (contextmenuNode.value) addChildGroup(contextmenuNode.value)
  closeContextmenu()
}

function handleCtxRename() {
  if (contextmenuNode.value) editGroup(contextmenuNode.value)
  closeContextmenu()
}

function handleCtxDelete() {
  if (contextmenuNode.value) deleteGroup(contextmenuNode.value)
  closeContextmenu()
}

async function addRootGroup() {
  try {
    const raw = await ElMessageBox.prompt('请输入分组名称:', '新建分组', { confirmButtonText: '确定', cancelButtonText: '取消' })
    const name = typeof raw === 'object' ? raw.value : raw
    if (!name || !String(name).trim()) return
    await groupStore.createGroup({ name: String(name).trim() })
    toast.success('分组已创建')
    await fetchGroupTree()
  } catch (e) {
    if (e !== 'cancel') toast.error('创建分组失败: ' + (e?.message || e))
  }
}

async function addChildGroup(parentNode) {
  try {
    const raw = await ElMessageBox.prompt('请输入子分组名称:', '添加子分组', { confirmButtonText: '确定', cancelButtonText: '取消' })
    const name = typeof raw === 'object' ? raw.value : raw
    if (!name || !String(name).trim()) return
    await groupStore.createGroup({ name: String(name).trim(), parent_id: parentNode.id })
    toast.success('子分组已创建')
    await fetchGroupTree()
  } catch (e) {
    if (e !== 'cancel') toast.error('创建子分组失败: ' + (e?.message || e))
  }
}

async function editGroup(node) {
  try {
    const raw = await ElMessageBox.prompt('请输入分组名称:', '重命名', {
      confirmButtonText: '确定', cancelButtonText: '取消', inputValue: node.name,
    })
    const name = typeof raw === 'object' ? raw.value : raw
    if (!name || !String(name).trim()) return
    await groupStore.updateGroup(node.id, { name: String(name).trim() })
    toast.success('分组已重命名')
    await fetchGroupTree()
  } catch (e) {
    if (e !== 'cancel') toast.error('重命名失败: ' + (e?.message || e))
  }
}

async function deleteGroup(node) {
  try {
    const childNames = node.children?.map(c => c.name).join('、') || ''
    const msg = childNames
      ? `确定删除分组"${node.name}"吗？删除后以下子分组也会被删除：${childNames}`
      : `确定删除分组"${node.name}"吗？`
    await ElMessageBox.confirm(msg, '确认删除', { type: 'warning', confirmButtonText: '确定', cancelButtonText: '取消' })
    await groupStore.deleteGroup(node.id)
    if (selectedGroup.value === node.id) selectedGroup.value = null
    toast.success('分组已删除')
    await fetchGroupTree()
  } catch (e) {
    if (e !== 'cancel') toast.error('删除失败: ' + (e?.message || e))
  }
}

function openAddModal() {
  isEdit.value = false; editingId.value = null
  const newForm = defaultForm()
  newForm.gb_industry_code = newForm.gb_industry_code || '00'
  newForm.gb_device_type = newForm.gb_device_type || '132'
  newForm.gb_network_code = newForm.gb_network_code || '0'
  form.value = newForm
  gbRegionCascader.value = []
  sipGbCode.value = '00'
  sipIdSuffix.value = ''
  showModal.value = true
}

async function openEditModal(device) {
  isEdit.value = true; editingId.value = device.id
  try {
    const fullDevice = await deviceStore.fetchDevice(device.device_tag)
    form.value = {
      protocol: fullDevice.protocol || 'Gb28181',
      device_tag: fullDevice.device_tag || '',
    }
    showModal.value = true
  } catch (e) {
    toast.error('获取设备详情失败: ' + (e?.message || e))
  }
}

function closeModal() { showModal.value = false }

function addPullUrl() { form.value.pull_urls.push({ protocol: 'rtsp', url: '', priority: form.value.pull_urls.length + 1 }) }
function removePullUrl(index) { form.value.pull_urls.splice(index, 1) }
function addPushUrl() { form.value.push_urls.push({ protocol: 'rtmp', url: '', priority: form.value.push_urls.length + 1 }) }
function removePushUrl(index) { form.value.push_urls.splice(index, 1) }

const currentFormRef = computed(() => {
  if (isEdit.value) {
    switch (form.value.protocol) {
      case 'Gb28181': return gbFormRef.value
      case 'Rtsp': return rtspFormRef.value
      case 'Onvif': return onvifFormRef.value
      case 'Rtmp': return rtmpFormRef.value
    }
  } else {
    switch (activeDeviceTab.value) {
      case 'gb28181': return gbFormRef.value
      case 'rtsp': return rtspFormRef.value
      case 'onvif': return onvifFormRef.value
      case 'rtmp': return rtmpFormRef.value
    }
  }
  return null
})

async function submitDevice() {
  const formRef = currentFormRef.value
  if (!formRef) { toast.error('无法获取表单'); return }
  
  const validation = formRef.validate()
  if (!validation.valid) {
    toast.error(validation.message)
    return
  }
  
  const data = formRef.getData()

  const finalDeviceTag = data.protocol === 'Gb28181' ? data.device_tag : null

  const payload = {
    name: data.name, protocol: data.protocol,
    host: data.host || null, port: data.port || null,
    device_username: data.device_username || null, device_password: data.device_password || null,
    media_server_tag: data.media_server_tag || null,
    pull_urls: data.pull_urls?.filter(u => u.url).length > 0 ? data.pull_urls : undefined,
    push_urls: data.push_urls?.filter(u => u.url).length > 0 ? data.push_urls : undefined,
    playback_username: data.playback_username || null, playback_password: data.playback_password || null,
    device_tag: finalDeviceTag, region_code: data.region_code || null, group_id: data.group_id || null, is_public: data.is_public,
    app: data.app || null,
    extended: {
      ...(data.extended || {}),
      rtsp_mode: data.protocol === 'Rtsp' ? (data.rtsp_mode || null) : null,
      rtsp_full_url: data.protocol === 'Rtsp' && data.rtsp_mode === 'pull' ? (data.rtsp_full_url || null) : null,
      rtmp_mode: data.protocol === 'Rtmp' ? (data.rtmp_mode || null) : null,
    },
  }

  if (data.protocol === 'Gb28181' && data.enable_stream_config) {
    payload.extended.stream_config = {
      video_codec: data.stream_config.video_codec || null,
      audio_codec: data.stream_config.audio_codec || null,
      video_payload_type: data.stream_config.video_payload_type,
      audio_payload_type: data.stream_config.audio_payload_type,
      profile_level_id: data.stream_config.profile_level_id || null,
      packaging_mode: data.stream_config.packaging_mode || null,
      sprop_parameter_sets: data.stream_config.sprop_parameter_sets || null,
      stream_mode: data.stream_config.stream_mode || null,
      media_transport: data.stream_config.media_transport || null,
    }
  }

  if (data.enable_recording) {
    payload.extended.recording = {
      enabled: true,
      format: data.recording?.format || 'Mp4',
      max_duration_secs: data.recording?.max_duration_secs || 0,
      max_file_size_mb: data.recording?.max_file_size_mb || 0,
    }
  } else {
    payload.extended.recording = { enabled: false }
  }

  if (isEdit.value) {
    await deviceStore.updateDevice(editingId.value, payload, data.device_tag)
    await fetchDevices(); closeModal()
    return
  }
  if (data.protocol === 'Onvif' && data.pull_urls?.filter(u => u.url).length > 1) {
    const streams = data.pull_urls.map((url, i) => ({
      token: data.extended?.profile_token?.[i] || `stream_${i}`,
      name: `${data.name} - 通道${i + 1}`,
      rtsp_url: url.url,
    }))
    const res = await request.post('/onvif/devices', {
      urn: null,
      x_addr: data.extended?.x_addr || data.host,
      name: data.name,
      manufacturer: null,
      model: null,
      username: data.device_username,
      password: data.device_password,
      capabilities: {
        media: data.extended?.capabilities?.media ?? null,
        ptz: data.extended?.capabilities?.ptz ?? null,
        events: data.extended?.capabilities?.events ?? null,
        imaging: data.extended?.capabilities?.imaging ?? null,
      },
      types: [],
      scopes: [],
      streams,
    })
    toast.success(`成功创建 ${streams.length} 个 ONVIF 通道`)
    await fetchDevices(); closeModal()
    return
  }
  await deviceStore.createDevice(payload)
  await fetchDevices(); closeModal()
}

async function openPlayLinks(device) {
  selectedDevice.value = device
  playLinks.value = await deviceStore.getPlayLinks(device.device_tag)
  showPlayLinksModal.value = true
}

async function deleteDevice(device) {
  try {
    await ElMessageBox.confirm(`确定删除设备 "${device.name}" 吗？`, '确认删除', { type: 'warning' })
    await deviceStore.deleteDevice(device.device_tag)
    await fetchDevices()
  } catch {}
}

async function copyLink(text) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const el = document.createElement('textarea')
    el.value = text; document.body.appendChild(el); el.select(); document.execCommand('copy'); document.body.removeChild(el)
  }
  toast.success('已复制到剪贴板')
}
</script>

<style scoped>
.group-header { display: flex; align-items: center; justify-content: space-between;width: 100%;}

.group-node-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding-right: 4px;
}
.group-node-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.group-node-actions {
  display: none;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}
.group-node-row:hover .group-node-actions {
  display: flex;
}
.group-node-actions .el-button {
  padding: 2px;
  font-size: 12px;
}

.group-contextmenu {
  position: fixed;
  z-index: 9999;
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius-base);
  box-shadow: var(--shadow-lg);
  padding: var(--space-2);
  min-width: 140px;
}
.ctx-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
  font-size: var(--text-sm);
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  transition: background 0.15s;
  font-family: var(--font-sans);
}
.ctx-item:hover {
  background: var(--color-accent-dim);
  color: var(--color-accent);
}
.ctx-danger { color: var(--color-danger); }
.ctx-danger:hover {
  background: rgba(239, 68, 68, 0.1);
  color: var(--color-danger);
}
.protocol-tag {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: var(--weight-semibold);
  border: none;
  color: #fff;
}
.protocol-tag.protocol-gb28181 { background-color: var(--protocol-gb28181); }
.protocol-tag.protocol-rtsp    { background-color: var(--protocol-rtsp); }
.protocol-tag.protocol-onvif   { background-color: var(--protocol-onvif); }
.protocol-tag.protocol-rtmp    { background-color: var(--protocol-rtmp); }

.tag-selector {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px 0;
  min-height: 80px;
  max-height: 160px;
  overflow-y: auto;
}

.gb28181-selected-row {
  display: flex;
  gap: 12px;
  margin-top: 12px;
}
.selection-tag {
  cursor: pointer;
}
.tag-item {
  cursor: pointer;
  transition: all 0.2s;
}
.tag-item.selectable {
  user-select: none;
}
.tag-item:hover {
  transform: translateY(-1px);
}
.tag-item.clear-btn {
  border-style: dashed;
}
.tag-selected-info {
  font-size: 12px;
  color: var(--text-secondary);
  padding: 4px 8px;
  background: var(--bg-surface);
  border-radius: 4px;
  margin-top: 8px;
}
.tag-category-header {
  width: 100%;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 8px 0 4px 0;
  padding-left: 4px;
  border-left: 3px solid var(--color-accent);
}
.tag-category-header:first-child {
  margin-top: 0;
}

.gb28181-selected-summary {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg-surface);
  border-radius: 6px;
  margin-bottom: 12px;
  font-size: 13px;
}
.gb28181-selected-summary .summary-item {
  color: var(--text-secondary);
}
.gb28181-selected-summary .summary-item strong {
  color: var(--text-primary);
  font-weight: 600;
}
.gb28181-selected-summary .summary-divider {
  color: var(--border);
}

.sip-id-row {
  display: flex;
  align-items: center;
  gap: 4px;
  margin: 16px 0;
  flex-wrap: nowrap;
  overflow-x: auto;
}
.sip-id-row-bottom {
  margin-top: 8px;
}
.sip-id-row .el-form-item {
  margin-bottom: 0;
  flex-shrink: 0;
}
.sip-field {
  width: 90px;
}
.sip-field .el-input__inner {
  font-family: var(--font-mono);
  font-size: 12px;
  text-align: center;
  padding: 0 4px;
}
.sip-field.sip-preview {
  width: 280px;
}
.sip-field.sip-preview .el-input__inner {
  font-size: 14px;
  letter-spacing: 1px;
}
.sip-plus, .sip-equals {
  color: var(--text-muted);
  font-weight: bold;
  flex-shrink: 0;
  align-self: flex-end;
  padding-bottom: 6px;
}
</style>
