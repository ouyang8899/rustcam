<template>
  <div class="device-form-content">
    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="设备名称 *" required>
          <el-input v-model="localForm.name" />
        </el-form-item>
      </el-col>
    </el-row>

    <el-form-item label="媒体服务器">
      <el-select v-model="localForm.media_server_tag" clearable style="width: 100%">
        <el-option value="">自动选择</el-option>
        <el-option v-for="s in mediaServerOptions" :key="s.server_tag" :label="`${s.name} (${s.server_type})`" :value="s.server_tag" />
      </el-select>
    </el-form-item>

    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="所属地区">
          <el-cascader
            v-model="gbRegionCascader"
            :options="regionTree"
            :props="{ value: 'gb28181_code', label: 'name', children: 'children' }"
            placeholder="选择行政区划（可选）"
            clearable
            style="width: 100%"
          />
        </el-form-item>
      </el-col>
      <el-col :span="12">
        <el-form-item label="自定义分组">
          <el-select v-model="localForm.group_id" placeholder="选择分组（可选）" clearable style="width: 100%">
            <template v-for="group in groupTree" :key="group.id">
              <el-option :label="group.name" :value="group.id" />
              <template v-for="child in (group.children || [])" :key="child.id">
                <el-option :label="'  ├─ ' + child.name" :value="child.id" />
                <template v-for="grandchild in (child.children || [])" :key="grandchild.id">
                  <el-option :label="'    └─ ' + grandchild.name" :value="grandchild.id" />
                </template>
              </template>
            </template>
          </el-select>
        </el-form-item>
      </el-col>
    </el-row>

    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="应用名称 (App)">
          <el-input v-model="localForm.app" placeholder="live" />
        </el-form-item>
      </el-col>
    </el-row>

    <el-divider>ONVIF 特定配置</el-divider>

    <div style="margin-bottom: 12px">
      <el-button type="primary" plain @click="onBroadcastDiscover" :loading="broadcastDiscovering">
        广播发现设备
      </el-button>
    </div>

    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="用户名">
          <el-input v-model="localForm.device_username" placeholder="admin" />
        </el-form-item>
      </el-col>
      <el-col :span="12">
        <el-form-item label="密码">
          <el-input v-model="localForm.device_password" type="password" show-password />
        </el-form-item>
      </el-col>
    </el-row>

    <el-divider content-position="left">通道配置</el-divider>
    
    <div v-if="localForm.pull_urls && localForm.pull_urls.length > 0">
      <el-tag v-for="(url, index) in localForm.pull_urls" :key="index" type="success" style="margin-right: 8px; margin-bottom: 4px">
        {{ url.url }}
      </el-tag>
    </div>
    <div v-else style="color: #909399; font-size: 12px">
      点击「发现设备」按钮获取通道信息
    </div>

    <el-divider>媒体参数</el-divider>
    <el-form-item label="启用流配置">
      <el-switch v-model="localForm.enable_stream_config" size="small" />
    </el-form-item>
    
    <div v-if="localForm.enable_stream_config" style="background: #f5f7fa; padding: 16px; border-radius: 4px; margin-bottom: 16px">
      <p style="margin: 0 0 12px 0; color: #909399; font-size: 12px;">ONVIF 设备会自动获取媒体参数，如有兼容性问题可手动覆盖：</p>
      <el-row :gutter="12">
        <el-col :span="8">
          <el-form-item label="视频编码">
            <el-select v-model="localForm.stream_config.video_codec" placeholder="自动" style="width: 100%">
              <el-option label="自动" value="" />
              <el-option label="H.264" value="H264" />
              <el-option label="H.265" value="H265" />
              <el-option label="MJPEG" value="JPEG" />
            </el-select>
          </el-form-item>
        </el-col>
        <el-col :span="8">
          <el-form-item label="音频编码">
            <el-select v-model="localForm.stream_config.audio_codec" placeholder="自动" style="width: 100%">
              <el-option label="自动" value="" />
              <el-option label="AAC" value="AAC" />
              <el-option label="G.711" value="PCMA" />
              <el-option label="禁用" value="NONE" />
            </el-select>
          </el-form-item>
        </el-col>
        <el-col :span="8">
          <el-form-item label="流模式">
            <el-select v-model="localForm.stream_config.stream_mode" placeholder="自动" style="width: 100%">
              <el-option label="自动" value="" />
              <el-option label="recvonly (仅接收)" value="recvonly" />
              <el-option label="sendrecv (双向)" value="sendrecv" />
            </el-select>
          </el-form-item>
        </el-col>
      </el-row>
    </div>

    <el-divider>录像配置</el-divider>
    <el-form-item label="启用自动录像">
      <el-switch v-model="localForm.enable_recording" size="small" />
    </el-form-item>
    
    <div v-if="localForm.enable_recording" style="background: #f5f7fa; padding: 16px; border-radius: 4px; margin-bottom: 16px">
      <el-row :gutter="12">
        <el-col :span="8">
          <el-form-item label="录像格式">
            <el-select v-model="localForm.recording.format" placeholder="选择格式" style="width: 100%">
              <el-option label="MP4" value="Mp4" />
              <el-option label="HLS" value="Hls" />
              <el-option label="FLV" value="Flv" />
              <el-option label="TS" value="Ts" />
            </el-select>
          </el-form-item>
        </el-col>
        <el-col :span="8">
          <el-form-item label="最大时长(秒)">
            <el-input-number v-model="localForm.recording.max_duration_secs" :min="0" :max="86400" style="width: 100%" placeholder="0表示不限制" />
          </el-form-item>
        </el-col>
        <el-col :span="8">
          <el-form-item label="最大文件大小(MB)">
            <el-input-number v-model="localForm.recording.max_file_size_mb" :min="0" :max="4096" style="width: 100%" placeholder="0表示不限制" />
          </el-form-item>
        </el-col>
      </el-row>
    </div>

    <el-divider>播放认证</el-divider>
    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="播放用户名">
          <el-input v-model="localForm.playback_username" />
        </el-form-item>
      </el-col>
      <el-col :span="12">
        <el-form-item label="播放密码">
          <el-input v-model="localForm.playback_password" type="password" show-password />
        </el-form-item>
      </el-col>
    </el-row>

    <el-form-item>
      <el-checkbox v-model="localForm.is_public">公开设备（无需登录可观看）</el-checkbox>
    </el-form-item>

    <el-dialog v-model="showDiscoverDialog" title="发现(ONVIF)设备" width="780px" draggable destroy-on-close>
      <el-skeleton animated :rows="4" :loading="broadcastDiscovering">
        <el-table :data="discoveredDevices" empty-text="未发现设备，请确保设备已启动且网络可达" highlight-current-row @row-click="(row) => selectedDevice = row">
          <el-table-column label="名称" show-overflow-tooltip>
            <template #default="{ row }">{{ row.name || row.manufacturer || row.model || 'Unknown Device' }}</template>
          </el-table-column>
          <el-table-column label="地址" show-overflow-tooltip>
            <template #default="{ row }">{{ row.address || row.x_addr }}</template>
          </el-table-column>
          <el-table-column label="厂商/型号" width="120">
            <template #default="{ row }">{{ [row.manufacturer, row.model].filter(Boolean).join(' ') || '-' }}</template>
          </el-table-column>
        </el-table>
      </el-skeleton>

      <template v-if="selectedDevice">
        <el-divider content-position="left">设备认证</el-divider>
        <el-row :gutter="12">
          <el-col :span="12">
            <el-form-item label="用户名">
              <el-input v-model="authUsername" placeholder="admin" />
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="密码">
              <el-input v-model="authPassword" type="password" show-password placeholder="password" />
            </el-form-item>
          </el-col>
        </el-row>
        <el-button type="primary" :loading="fetchingProfiles" @click="onFetchProfiles" style="margin-bottom: 12px">
          获取设备能力（RTSP/PTZ/音频）
        </el-button>

        <template v-if="onvifStreams.length > 0">
          <el-divider content-position="left">视频通道选择（{{ onvifSelectedStreams.length }}/{{ onvifStreams.length }}）</el-divider>
          <el-table :data="onvifStreams" empty-text="无" max-height="200" @selection-change="(rows) => onvifSelectedStreams = rows">
            <el-table-column type="selection" width="50" />
            <el-table-column label="名称" prop="name" show-overflow-tooltip />
            <el-table-column label="分辨率" width="120">
              <template #default="{ row }">{{ row.width }}x{{ row.height }}</template>
            </el-table-column>
            <el-table-column label="编码" prop="encoding" width="80" />
            <el-table-column label="RTSP URL" show-overflow-tooltip>
              <template #default="{ row }">
                <code style="font-size: 11px">{{ row.rtsp_url }}</code>
              </template>
            </el-table-column>
          </el-table>
          <div style="margin-top: 8px; color: #909399; font-size: 12px">
            <el-checkbox v-model="selectAllStreams">全选</el-checkbox>
            <span style="margin-left: 16px">将为您创建 {{ onvifSelectedStreams.length }} 个设备通道</span>
          </div>
        </template>
        <template v-else-if="hasFetchedProfiles && onvifStreams.length === 0">
          <el-divider content-position="left">RTSP 连接（设备不支持自动发现，请手动输入）</el-divider>
          <el-form-item label="RTSP URL">
            <el-input v-model="manualRtspUrl" placeholder="rtsp://192.168.1.100:554/live" style="margin-bottom: 8px" />
          </el-form-item>
        </template>
        <template v-if="capabilities.ptz">
          <el-divider content-position="left">PTZ 支持</el-divider>
          <el-tag type="warning" style="margin-right: 8px">支持云台控制</el-tag>
        </template>
        <template v-if="capabilities.audio">
          <el-divider content-position="left">音频支持</el-divider>
          <el-tag type="info" style="margin-right: 8px">支持音频</el-tag>
        </template>
      </template>
      <template #footer>
        <el-button @click="closeDiscoverDialog">取消</el-button>
        <el-button type="primary" :disabled="!selectedDevice || (onvifSelectedStreams.length === 0 && !manualRtspUrl.trim())" :loading="submitting" @click="onConfirmDevice">
          确认添加 {{ onvifSelectedStreams.length > 0 ? `(${onvifSelectedStreams.length}个通道)` : (manualRtspUrl.trim() ? '(1个通道)' : '') }}
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { reactive, ref, computed, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { request } from '../../utils/request'

const props = defineProps({
  modelValue: Object,
  mediaServerOptions: Array,
  regionTree: Array,
  groupTree: Array,
  deviceTag: String,
})

const localForm = reactive({ 
  name: '',
  media_server_tag: '',
  group_id: null,
  app: 'live',
  host: '',
  device_username: '',
  device_password: '',
  pull_urls: [],
  enable_stream_config: false,
  stream_config: {
    video_codec: '',
    audio_codec: '',
    stream_mode: '',
  },
  enable_recording: false,
  recording: {
    format: 'Mp4',
    max_duration_secs: 0,
    max_file_size_mb: 0,
  },
  playback_username: '',
  playback_password: '',
  is_public: false,
})

const gbRegionCascader = ref([])
const showDiscoverDialog = ref(false)
const discoveredDevices = ref([])
const selectedDevice = ref(null)
const authUsername = ref('')
const authPassword = ref('')
const onvifStreams = ref([])
const onvifSelectedStreams = ref([])
const manualRtspUrl = ref('')
const capabilities = ref({ ptz: false, audio: false })
const fetchingProfiles = ref(false)
const hasFetchedProfiles = ref(false)
const submitting = ref(false)
const broadcastDiscovering = ref(false)

const selectAllStreams = computed({
  get: () => onvifStreams.value.length > 0 && onvifSelectedStreams.value.length === onvifStreams.value.length,
  set: (val) => { onvifSelectedStreams.value = val ? [...onvifStreams.value] : [] }
})

watch(() => props.deviceTag, (tag) => {
  if (tag) loadData(tag)
}, { immediate: true })

function closeDiscoverDialog() {
  showDiscoverDialog.value = false
  selectedDevice.value = null
  onvifStreams.value = []
  onvifSelectedStreams.value = []
  capabilities.value = { ptz: false, audio: false }
  authUsername.value = ''
  authPassword.value = ''
  manualRtspUrl.value = ''
  hasFetchedProfiles.value = false
}

async function onBroadcastDiscover() {
  broadcastDiscovering.value = true
  discoveredDevices.value = []
  selectedDevice.value = null
  onvifStreams.value = []
  onvifSelectedStreams.value = []
  capabilities.value = { ptz: false, audio: false }
  showDiscoverDialog.value = true
  try {
    const res = await request.post('/onvif/discover', {})
    discoveredDevices.value = res.data?.data || []
    if (discoveredDevices.value.length === 0) {
      ElMessage.warning('未发现 ONVIF 设备，请确保设备已启动且网络可达')
    }
  } catch (e) {
    ElMessage.error('广播发现失败: ' + (e?.message || e))
  } finally {
    broadcastDiscovering.value = false
  }
}

async function onFetchProfiles() {
  const dev = selectedDevice.value
  if (!dev) return
  const address = dev.address || dev.x_addr || ''
  if (!address) { ElMessage.warning('设备地址为空'); return }
  fetchingProfiles.value = true
  onvifStreams.value = []
  onvifSelectedStreams.value = []
  capabilities.value = { ptz: false, audio: false }
  hasFetchedProfiles.value = true
  try {
    const capsRes = await request.post('/onvif/capabilities', {
      x_addr: address,
      username: authUsername.value || '',
      password: authPassword.value || '',
    })
    const capsData = capsRes.data?.data
    const profiles = capsData?.profiles || []
    if (profiles.length === 0) {
      ElMessage.warning('未发现媒体通道，请检查地址和认证信息')
      return
    }
    const streamRes = await request.post('/onvif/stream-uris', {
      x_addr: address,
      media_x_addr: capsData?.capabilities?.media?.x_addr || null,
      username: authUsername.value || '',
      password: authPassword.value || '',
      profiles: profiles.map(p => p.token),
    })
    const streams = streamRes.data?.data?.streams || []
    onvifStreams.value = profiles.map((p, i) => {
      const stream = streams.find(s => s.token === p.token) || streams[i] || {}
      return {
        name: p.name || `通道${i + 1}`,
        width: p.width || stream.width || 0,
        height: p.height || stream.height || 0,
        encoding: p.encoding || stream.encoding || '',
        rtsp_url: stream.rtsp_url || '',
      }
    }).filter(s => s.rtsp_url)
    if (onvifStreams.value.length > 0) {
      selectAllStreams.value = true
      ElMessage.success(`已获取 ${onvifStreams.value.length} 个视频通道`)
    } else {
      ElMessage.warning('未发现 RTSP 链接，请检查用户名和密码')
    }
    capabilities.value = {
      ptz: !!(capsData?.capabilities?.ptz),
      audio: capsData?.capabilities?.has_audio || false,
    }
  } catch (e) {
    ElMessage.error('获取能力失败: ' + (e?.message || e))
  } finally {
    fetchingProfiles.value = false
  }
}

async function onConfirmDevice() {
  const dev = selectedDevice.value
  if (!dev) return
  if (onvifSelectedStreams.value.length === 0 && !manualRtspUrl.value.trim()) return
  submitting.value = true
  try {
    const address = dev.address || dev.x_addr || ''
    const manufacturer = dev.manufacturer || ''
    const model = dev.model || ''
    const parentName = localForm.name?.trim() || dev.name || `${manufacturer} ${model}`.trim() || 'ONVIF 设备'
    
    localForm.name = parentName
    localForm.host = address
    localForm.protocol = 'Onvif'
    localForm.device_username = authUsername.value || ''
    localForm.device_password = authPassword.value || ''
    
    if (onvifSelectedStreams.value.length > 0) {
      localForm.pull_urls = onvifSelectedStreams.value.map((s, i) => ({ protocol: 'rtsp', url: s.rtsp_url, priority: i + 1 }))
      ElMessage.success(`设备已选择 ${onvifSelectedStreams.value.length} 个通道`)
    } else {
      localForm.pull_urls = [{ protocol: 'rtsp', url: manualRtspUrl.value.trim(), priority: 1 }]
      ElMessage.success(`设备已添加（手动RTSP URL）`)
    }
    
    closeDiscoverDialog()
  } catch (e) {
    ElMessage.error('确认失败: ' + (e?.message || e))
  } finally {
    submitting.value = false
  }
}

function validate() {
  if (!localForm.name?.trim()) {
    return { valid: false, message: '请填写设备名称' }
  }
  return { valid: true }
}

function getData() {
  return { ...localForm }
}

async function loadData(deviceTag) {
  if (!deviceTag) return
  try {
    const res = await request.get(`/devices/${deviceTag}`)
    const fullDevice = res.data?.data || res.data || {}
    const ext = fullDevice.extended || {}
    const devStreamConfig = ext.stream_config || {}

    localForm.name = fullDevice.name || ''
    localForm.host = fullDevice.host || ext.x_addr || ''
    localForm.device_username = fullDevice.device_username || ''
    localForm.device_password = fullDevice.device_password || ''
    localForm.media_server_tag = fullDevice.media_server_tag || ''
    localForm.group_id = fullDevice.group_id ?? null
    localForm.playback_username = fullDevice.playback_username || ''
    localForm.playback_password = fullDevice.playback_password || ''
    localForm.is_public = fullDevice.is_public || false
    localForm.app = fullDevice.app || 'live'
    localForm.pull_urls = fullDevice.pull_urls || []
    localForm.enable_stream_config = !!devStreamConfig && Object.keys(devStreamConfig).length > 0
    localForm.stream_config = {
      video_codec: devStreamConfig.video_codec || '',
      audio_codec: devStreamConfig.audio_codec || '',
      stream_mode: devStreamConfig.stream_mode || '',
    }
    const devRecording = ext.recording || {}
    localForm.enable_recording = !!devRecording && devRecording.enabled === true
    localForm.recording = {
      format: devRecording.format || 'Mp4',
      max_duration_secs: devRecording.max_duration_secs || 0,
      max_file_size_mb: devRecording.max_file_size_mb || 0,
    }
  } catch (e) {
    ElMessage.error('加载设备数据失败: ' + (e?.message || e))
  }
}

defineExpose({ getData, validate, loadData })
</script>