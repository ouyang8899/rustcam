<template>
  <div class="device-form-content">
    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="设备名称 *" required>
          <el-input v-model="localForm.name" @input="onFormChange" />
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
        <el-form-item label="所属地区 *">
          <el-cascader
            v-model="gbRegionCascader"
            :options="regionTree"
            :props="{ value: 'gb28181_code', label: 'name', children: 'children' }"
            placeholder="选择行政区划"
            clearable
            style="width: 100%"
            @change="onGbRegionChange"
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
          <el-input v-model="localForm.app" placeholder="rtp" disabled />
        </el-form-item>
      </el-col>
    </el-row>

    <el-divider>GB28181 特定配置</el-divider>

    <el-tabs v-model="gb28181Tab">
      <el-tab-pane label="行业类型 *" name="industry">
        <div class="tag-selector">
          <el-tag
            v-for="opt in industryOptions"
            :key="opt.value"
            :type="localForm.gb_industry_code === opt.value ? 'primary' : 'info'"
            class="tag-item selectable"
            @click="localForm.gb_industry_code = opt.value"
          >
            {{ opt.label }}
          </el-tag>
        </div>
      </el-tab-pane>
      <el-tab-pane label="设备类型 *" name="device_type">
        <div class="tag-selector">
          <template v-for="opt in deviceTypeOptions" :key="opt.value || opt.label">
            <div v-if="opt.type === 'header'" class="tag-category-header">{{ opt.label }}</div>
            <el-tag
              v-else
              :type="localForm.gb_device_type === opt.value ? 'primary' : 'info'"
              class="tag-item selectable"
              @click="localForm.gb_device_type = opt.value"
            >
              {{ opt.label }}
            </el-tag>
          </template>
        </div>
      </el-tab-pane>
      <el-tab-pane label="网络类型 *" name="network">
        <div class="tag-selector">
          <el-tag
            v-for="opt in networkOptions"
            :key="opt.value"
            :type="localForm.gb_network_code === opt.value ? 'primary' : 'info'"
            class="tag-item selectable"
            @click="localForm.gb_network_code = opt.value"
          >
            {{ opt.label }}
          </el-tag>
        </div>
      </el-tab-pane>
    </el-tabs>

    <div class="gb28181-selected-row">
      <el-tag 
        class="selection-tag" 
        :type="localForm.gb_industry_code ? 'success' : 'info'"
        @click="gb28181Tab = 'industry'"
      >
        {{ getIndustryCodeLabel(localForm.gb_industry_code) || '未选行业' }}
      </el-tag>
      <el-tag 
        class="selection-tag" 
        :type="localForm.gb_device_type ? 'success' : 'info'"
        @click="gb28181Tab = 'device_type'"
      >
        {{ getDeviceTypeLabel(localForm.gb_device_type) || '未选设备' }}
      </el-tag>
      <el-tag 
        class="selection-tag" 
        :type="localForm.gb_network_code ? 'success' : 'info'"
        @click="gb28181Tab = 'network'"
      >
        {{ getNetworkCodeLabel(localForm.gb_network_code) || '未选网络' }}
      </el-tag>
    </div>

    <div class="sip-id-row">
      <el-form-item label="行政区划" class="sip-field">
        <el-input :model-value="sipIdPrefix" disabled placeholder="6位" />
      </el-form-item>
      <span class="sip-plus">+</span>
      <el-form-item label="基层" class="sip-field">
        <el-input v-model="sipGbCode" placeholder="2位" maxlength="2" @input="sipGbCode = sipGbCode.replace(/\D/g, '')" />
      </el-form-item>
      <span class="sip-plus">+</span>
      <el-form-item label="行业" class="sip-field">
        <el-input :model-value="localForm.gb_industry_code" disabled placeholder="2位" />
      </el-form-item>
      <span class="sip-plus">+</span>
      <el-form-item label="设备" class="sip-field">
        <el-input :model-value="localForm.gb_device_type" disabled placeholder="3位" />
      </el-form-item>
      <span class="sip-plus">+</span>
      <el-form-item label="网络" class="sip-field">
        <el-input :model-value="localForm.gb_network_code" disabled placeholder="1位" />
      </el-form-item>
    </div>

    <div class="sip-id-row sip-id-row-bottom">
      <el-form-item label="后缀" class="sip-field">
        <el-input v-model="localForm.sip_id_suffix" placeholder="6位" maxlength="6" />
      </el-form-item>
      <span class="sip-equals">=</span>
      <el-form-item label="SIP ID" class="sip-field sip-preview">
        <el-input :model-value="sipIdPreview" disabled placeholder="20位">
          <template #append>
            <span style="min-width: 40px">{{ sipIdPreview.length }}/20</span>
          </template>
        </el-input>
      </el-form-item>
    </div>

    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="SIP 服务器">
          <el-input v-model="localForm.host" placeholder="192.168.1.100" />
        </el-form-item>
      </el-col>
      <el-col :span="12">
        <el-form-item label="SIP 端口">
          <el-input-number v-model="localForm.port" :min="1" :max="65535" style="width: 100%" />
        </el-form-item>
      </el-col>
    </el-row>

    <el-row :gutter="12">
      <el-col :span="12">
        <el-form-item label="SIP 密码 *" required>
          <el-input v-model="localForm.device_password" type="password" show-password placeholder="设备SIP认证密码" />
        </el-form-item>
      </el-col>
      <el-col :span="12">
        <el-form-item label="传输协议">
          <el-select v-model="localForm.gb_transport" placeholder="选择协议" style="width: 100%">
            <el-option label="UDP (推荐)" value="UDP" />
            <el-option label="TCP" value="TCP" />
          </el-select>
        </el-form-item>
      </el-col>
    </el-row>

    <el-divider>SIP INVITE 流配置</el-divider>
    <el-form-item label="启用流配置">
      <el-switch v-model="localForm.enable_stream_config" size="small" />
    </el-form-item>
    
    <template v-if="localForm.enable_stream_config">
      <div style="background: #f5f7fa; padding: 16px; border-radius: 4px; margin-bottom: 16px">
        <el-row :gutter="12">
          <el-col :span="6">
            <el-form-item label="视频编码">
              <el-select v-model="localForm.stream_config.video_codec" placeholder="选择编码" style="width: 100%">
                <el-option label="PS (MPEG-PS, 海康大华)" value="PS" />
                <el-option label="H.264 (通用)" value="H264" />
                <el-option label="H.265 (新设备)" value="H265" />
                <el-option label="JPEG (MJPEG)" value="JPEG" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="6">
            <el-form-item label="音频编码">
              <el-select v-model="localForm.stream_config.audio_codec" placeholder="选择编码" style="width: 100%">
                <el-option label="PCMA (G.711 A律)" value="PCMA" />
                <el-option label="PCMU (G.711 U律)" value="PCMU" />
                <el-option label="AAC" value="AAC" />
                <el-option label="禁用" value="NONE" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="6">
            <el-form-item label="Profile/Level">
              <el-input v-model="localForm.stream_config.profile_level_id" placeholder="4D001F" />
            </el-form-item>
          </el-col>
          <el-col :span="6">
            <el-form-item label="流模式">
              <el-select v-model="localForm.stream_config.stream_mode" placeholder="选择模式" style="width: 100%">
                <el-option label="recvonly (仅接收)" value="recvonly" />
                <el-option label="sendonly (仅发送)" value="sendonly" />
                <el-option label="sendrecv (双向)" value="sendrecv" />
              </el-select>
            </el-form-item>
          </el-col>
        </el-row>
        <el-row :gutter="12">
          <el-col :span="6">
            <el-form-item label="视频PT">
              <el-input-number v-model="localForm.stream_config.video_payload_type" :min="0" :max="127" style="width: 100%" placeholder="96" />
            </el-form-item>
          </el-col>
          <el-col :span="6">
            <el-form-item label="音频PT">
              <el-input-number v-model="localForm.stream_config.audio_payload_type" :min="0" :max="127" style="width: 100%" placeholder="8" />
            </el-form-item>
          </el-col>
          <el-col :span="6">
            <el-form-item label="SPS/PPS">
              <el-input v-model="localForm.stream_config.sprop_parameter_sets" placeholder="Base64编码, 可留空" />
            </el-form-item>
          </el-col>
          <el-col :span="6">
            <el-form-item label="封装模式">
              <el-select v-model="localForm.stream_config.packaging_mode" placeholder="选择模式" style="width: 100%">
                <el-option label="HIS (96字节头)" value="HIS" />
                <el-option label="NALU (标准)" value="NALU" />
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
</template>

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
  industryOptions: Array,
  deviceTypeOptions: Array,
  networkOptions: Array,
  deviceTag: String,
})

const localForm = reactive({ 
  name: '',
  media_server_tag: '',
  group_id: null,
  app: 'rtp',
  gb_device_type: '132',
  gb_industry_code: '00',
  gb_network_code: '0',
  gb_transport: 'UDP',
  sip_id_suffix: '',
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
  },
  enable_recording: false,
  recording: {
    format: 'Mp4',
    max_duration_secs: 0,
    max_file_size_mb: 0,
  },
  host: '',
  port: 5060,
  device_password: '',
  playback_username: '',
  playback_password: '',
  is_public: false,
})

const gb28181Tab = ref('industry')
const sipGbCode = ref('')
const gbRegionCascader = ref([])

watch(() => props.deviceTag, (tag) => {
  if (tag) loadData(tag)
}, { immediate: true })

const sipIdPrefix = computed(() => {
  if (gbRegionCascader.value && gbRegionCascader.value.length > 0) {
    const lastCode = gbRegionCascader.value[gbRegionCascader.value.length - 1]
    return lastCode?.toString() || ''
  }
  return ''
})

const sipIdPreview = computed(() => {
  return `${sipIdPrefix.value}${sipGbCode.value || '00'}${localForm.gb_industry_code || '00'}${localForm.gb_device_type || '000'}${localForm.gb_network_code || '0'}${localForm.sip_id_suffix || '000000'}`
})

function onGbRegionChange(val) {
  // nothing to emit
}

function getIndustryCodeLabel(code) {
  const opt = props.industryOptions?.find(o => o.value === code)
  return opt ? opt.label : ''
}

function getDeviceTypeLabel(code) {
  const opt = props.deviceTypeOptions?.find(o => o.value === code)
  return opt ? opt.label : ''
}

function getNetworkCodeLabel(code) {
  const opt = props.networkOptions?.find(o => o.value === code)
  return opt ? opt.label : ''
}

function validate() {
  if (!localForm.name?.trim()) {
    return { valid: false, message: '请填写设备名称' }
  }
  if (!sipIdPrefix.value) {
    return { valid: false, message: '请选择所属地区' }
  }
  if (!localForm.device_password?.trim()) {
    return { valid: false, message: '请填写 SIP 密码' }
  }
  if (sipIdPreview.value.length !== 20) {
    return { valid: false, message: `SIP ID 必须为20位，当前${sipIdPreview.value.length}位` }
  }
  return { valid: true }
}

function getData() {
  const deviceTag = [
    sipIdPrefix.value || '000000',
    sipGbCode.value || '00',
    localForm.gb_industry_code || '00',
    localForm.gb_device_type || '000',
    localForm.gb_network_code || '0',
    localForm.sip_id_suffix || '000000',
  ].join('')
  
  return {
    ...localForm,
    protocol: 'Gb28181',
    device_tag: deviceTag,
    region_code: sipIdPrefix.value || null,
    extended: {
      gb_transport: localForm.gb_transport,
      stream_config: localForm.enable_stream_config ? localForm.stream_config : null,
    },
  }
}

async function loadData(deviceTag) {
  if (!deviceTag) return
  try {
    const res = await request.get(`/devices/${deviceTag}`)
    const fullDevice = res.data?.data || res.data || {}
    const ext = fullDevice.extended || {}
    const devStreamConfig = ext.stream_config || {}
    
    localForm.name = fullDevice.name || ''
    localForm.host = fullDevice.host || ''
    localForm.port = fullDevice.port || 5060
    localForm.device_username = fullDevice.device_username || ''
    localForm.device_password = fullDevice.device_password || ''
    localForm.media_server_tag = fullDevice.media_server_tag || ''
    localForm.group_id = fullDevice.group_id ?? null
    localForm.playback_username = fullDevice.playback_username || ''
    localForm.playback_password = fullDevice.playback_password || ''
    localForm.is_public = fullDevice.is_public || false
    localForm.app = fullDevice.app || 'rtp'
    localForm.gb_transport = ext.gb_transport || 'UDP'
    localForm.enable_stream_config = !!devStreamConfig && Object.keys(devStreamConfig).length > 0
    localForm.stream_config = {
      video_codec: devStreamConfig.video_codec || 'PS',
      audio_codec: devStreamConfig.audio_codec || 'PCMA',
      video_payload_type: devStreamConfig.video_payload_type || 96,
      audio_payload_type: devStreamConfig.audio_payload_type || 8,
      profile_level_id: devStreamConfig.profile_level_id || '4D001F',
      packaging_mode: devStreamConfig.packaging_mode || 'HIS',
      sprop_parameter_sets: devStreamConfig.sprop_parameter_sets || '',
      stream_mode: devStreamConfig.stream_mode || 'recvonly',
    }
    const devRecording = ext.recording || {}
    localForm.enable_recording = !!devRecording && devRecording.enabled === true
    localForm.recording = {
      format: devRecording.format || 'Mp4',
      max_duration_secs: devRecording.max_duration_secs || 0,
      max_file_size_mb: devRecording.max_file_size_mb || 0,
    }
    
    const tag = deviceTag || ''
    if (tag.length === 20) {
      const regionCode = fullDevice.region_code || tag.substring(0, 6)
      gbRegionCascader.value = findRegionPath(regionCode, props.regionTree) || (regionCode ? [regionCode] : [])
      sipGbCode.value = tag.substring(6, 8)
      localForm.gb_industry_code = tag.substring(8, 10)
      localForm.gb_device_type = tag.substring(10, 13)
      localForm.gb_network_code = tag.substring(13, 14)
      localForm.sip_id_suffix = tag.substring(14, 20)
    }
  } catch (e) {
    ElMessage.error('加载设备数据失败: ' + (e?.message || e))
  }
}

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

defineExpose({ getData, validate, loadData })
</script>>

<style scoped>
.tag-selector {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.tag-item {
  cursor: pointer;
}
.tag-item.selectable:hover {
  opacity: 0.8;
}
.tag-category-header {
  width: 100%;
  font-weight: bold;
  margin-top: 8px;
  margin-bottom: 4px;
  color: #606266;
}
.gb28181-selected-row {
  display: flex;
  gap: 8px;
  margin: 12px 0;
}
.selection-tag {
  cursor: pointer;
}
.sip-id-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 12px 0;
}
.sip-id-row-bottom {
  margin-top: 0;
}
.sip-field {
  margin-bottom: 0;
}
.sip-plus, .sip-equals {
  font-weight: bold;
  color: #409eff;
}
.sip-preview {
  flex: 1;
}
</style>
