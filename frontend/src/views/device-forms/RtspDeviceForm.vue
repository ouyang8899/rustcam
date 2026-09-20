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

    <el-divider>RTSP 特定配置</el-divider>

    <el-form-item label="传输模式">
      <el-radio-group v-model="localForm.rtsp_mode">
        <el-radio value="pull">拉取（从设备拉流）</el-radio>
        <el-radio value="push">推送（向媒体服务器推流）</el-radio>
      </el-radio-group>
    </el-form-item>

    <div v-if="localForm.rtsp_mode === 'push'">
      <el-form-item v-if="pushAddressPreview" label="推送地址预览">
        <el-input :model-value="pushAddressPreview" disabled placeholder="请先在基础设置中选择媒体服务器" />
      </el-form-item>
      <el-form-item v-if="!localForm.media_server_tag" label="&nbsp;">
        <el-alert type="info" :closable="false" show-icon>
          请在「基础设置」中选择媒体服务器，推送地址将自动生成
        </el-alert>
      </el-form-item>
    </div>

    <div v-if="localForm.rtsp_mode === 'pull'">
      <el-row :gutter="12">
        <el-col :span="24">
          <el-form-item label="RTSP 全链接 *" required>
            <el-input v-model="localForm.rtsp_full_url" placeholder="rtsp://192.168.1.100:554/live/stream1" />
          </el-form-item>
        </el-col>
      </el-row>
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
    </div>

    <el-divider>媒体参数</el-divider>
    <el-form-item label="启用流配置">
      <el-switch v-model="localForm.enable_stream_config" size="small" />
    </el-form-item>
    
    <div v-if="localForm.enable_stream_config" style="background: #f5f7fa; padding: 16px; border-radius: 4px; margin-bottom: 16px">
      <p style="margin: 0 0 12px 0; color: #909399; font-size: 12px;">RTSP 设备会自动获取媒体参数，如有兼容性问题可手动覆盖：</p>
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
  rtsp_mode: 'pull',
  rtsp_full_url: '',
  device_username: '',
  device_password: '',
  enable_stream_config: false,
  stream_config: {
    video_codec: '',
    audio_codec: '',
    stream_mode: '',
  },
  playback_username: '',
  playback_password: '',
  is_public: false,
})

const gbRegionCascader = ref([])

watch(() => props.deviceTag, (tag) => {
  if (tag) loadData(tag)
}, { immediate: true })

const pushAddressPreview = computed(() => {
  return ''
})

function validate() {
  if (!localForm.name?.trim()) {
    return { valid: false, message: '请填写设备名称' }
  }
  if (localForm.rtsp_mode === 'pull' && !localForm.rtsp_full_url?.trim()) {
    return { valid: false, message: '请填写 RTSP URL' }
  }
  return { valid: true }
}

function getData() {
  return { ...localForm, protocol: 'Rtsp' }
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
    localForm.port = fullDevice.port || 554
    localForm.device_username = fullDevice.device_username || ''
    localForm.device_password = fullDevice.device_password || ''
    localForm.media_server_tag = fullDevice.media_server_tag || ''
    localForm.group_id = fullDevice.group_id ?? null
    localForm.playback_username = fullDevice.playback_username || ''
    localForm.playback_password = fullDevice.playback_password || ''
    localForm.is_public = fullDevice.is_public || false
    localForm.app = fullDevice.app || 'live'
    localForm.rtsp_mode = ext.rtsp_mode || 'pull'
    localForm.rtsp_full_url = ext.rtsp_full_url || ''
    localForm.enable_stream_config = !!devStreamConfig && Object.keys(devStreamConfig).length > 0
    localForm.stream_config = {
      video_codec: devStreamConfig.video_codec || '',
      audio_codec: devStreamConfig.audio_codec || '',
      stream_mode: devStreamConfig.stream_mode || '',
    }
  } catch (e) {
    ElMessage.error('加载设备数据失败: ' + (e?.message || e))
  }
}

defineExpose({ getData, validate, loadData })
</script>
