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

    <el-divider>RTMP 特定配置</el-divider>

    <el-form-item label="传输模式">
      <el-radio-group v-model="localForm.rtmp_mode">
        <el-radio value="push">推送（向媒体服务器推流）</el-radio>
      </el-radio-group>
    </el-form-item>

    <el-form-item label="推送地址预览">
      <el-input :model-value="pushAddressPreview" disabled placeholder="请先在基础设置中选择媒体服务器" />
    </el-form-item>
    <el-form-item v-if="!localForm.media_server_tag" label="&nbsp;">
      <el-alert type="info" :closable="false" show-icon>
        请在「基础设置」中选择媒体服务器，推送地址将自动生成
      </el-alert>
    </el-form-item>

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
  </div>
</template>

<script setup>
import { reactive, computed, watch, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { request } from '../../utils/request'

const props = defineProps({
  modelValue: Object,
  mediaServerOptions: Array,
  regionTree: Array,
  groupTree: Array,
  pushAddress: String,
  pushPort: Number,
  deviceTag: String,
})

const localForm = reactive({
  name: '',
  media_server_tag: '',
  group_id: null,
  app: 'live',
  rtmp_mode: 'push',
  stream_key: '',
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

watch(() => props.deviceTag, (tag) => {
  if (tag) loadData(tag)
}, { immediate: true })

function generateStreamKey(name) {
  if (!name?.trim()) return ''
  return name.trim()
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fa5]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '') || Math.random().toString(36).slice(2, 10)
}

const autoStreamKey = computed(() => generateStreamKey(localForm.name))

const pushAddressPreview = computed(() => {
  if (!props.pushAddress) return ''
  const key = autoStreamKey.value || '{stream_key}'
  return `${props.pushAddress.replace(/\/$/, '')}:${props.pushPort}/${localForm.app}/${key}`
})

function validate() {
  if (!localForm.name?.trim()) {
    return { valid: false, message: '请填写设备名称' }
  }
  return { valid: true }
}

function getData() {
  return { ...localForm, stream_key: autoStreamKey.value, protocol: 'Rtmp' }
}

async function loadData(deviceTag) {
  if (!deviceTag) return
  try {
    const res = await request.get(`/devices/${deviceTag}`)
    const fullDevice = res.data?.data || res.data || {}
    const ext = fullDevice.extended || {}
    
    localForm.name = fullDevice.name || ''
    localForm.host = fullDevice.host || ''
    localForm.media_server_tag = fullDevice.media_server_tag || ''
    localForm.group_id = fullDevice.group_id ?? null
    localForm.playback_username = fullDevice.playback_username || ''
    localForm.playback_password = fullDevice.playback_password || ''
    localForm.is_public = fullDevice.is_public || false
    localForm.app = fullDevice.app || 'live'
    localForm.rtmp_mode = ext.rtmp_mode || 'push'
    localForm.stream_key = ext.stream_key || ''
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