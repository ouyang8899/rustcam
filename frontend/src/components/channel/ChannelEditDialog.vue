<template>
  <el-dialog
    :model-value="modelValue"
    :title="dialogTitle"
    width="720px"
    destroy-on-close
    @update:model-value="$emit('update:modelValue', $event)"
    @open="onOpen"
  >
    <el-form :model="form" label-width="90px" label-suffix="：">
      <el-row :gutter="16">
        <el-col :span="12">
          <el-form-item label="通道名称">
            <el-input v-model="form.name" placeholder="通道显示名称" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="通道标识">
            <el-input :model-value="channelTag" disabled />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="设备标识">
            <el-input :model-value="deviceTag" disabled />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="IP 地址">
            <el-input v-model="form.ip_address" placeholder="例如 192.168.1.64" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="端口">
            <el-input-number
              v-model="form.port"
              :min="0"
              :max="65535"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="地址">
            <el-input v-model="form.address" placeholder="安装位置描述" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="行政区划">
            <el-input v-model="form.civil_code" placeholder="GB28181 行政区划编码" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="厂商">
            <el-input v-model="form.manufacturer" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="型号">
            <el-input v-model="form.model" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="经度">
            <el-input-number
              v-model="form.longitude"
              :precision="6"
              :step="0.000001"
              :min="-180"
              :max="180"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="纬度">
            <el-input-number
              v-model="form.latitude"
              :precision="6"
              :step="0.000001"
              :min="-90"
              :max="90"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
        </el-col>
      </el-row>

      <el-form-item label="地图选点">
        <el-button size="small" @click="openPicker">在地图上选择位置</el-button>
        <el-button size="small" @click="clearPosition">清空坐标</el-button>
        <span class="position-hint">{{ positionHint }}</span>
      </el-form-item>
    </el-form>

    <template #footer>
      <el-button @click="$emit('update:modelValue', false)">取消</el-button>
      <el-button type="primary" :loading="saving" @click="submit">保存</el-button>
    </template>

    <el-drawer v-model="pickerVisible" title="地图选点" size="520px" append-to-body>
      <div class="picker-wrap">
        <div ref="pickerContainer" class="picker-map"></div>
        <div class="picker-tip">点击地图设置坐标，关闭抽屉后数值仍可手动微调。</div>
      </div>
    </el-drawer>
  </el-dialog>
</template>

<script setup>
import { ref, reactive, computed, nextTick, watch } from 'vue'
import { Map as MapLibreMap, Marker } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { ElMessage } from 'element-plus'
import { useSettingsStore } from '../../stores/settingsStore'
import { updateChannel, createChannel, getChannel } from '../../api/channels'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  mode: { type: String, default: 'edit' },
  deviceTag: { type: String, default: '' },
  channelTag: { type: String, default: '' },
})

const emit = defineEmits(['update:modelValue', 'saved'])

const settingsStore = useSettingsStore()
const saving = ref(false)
const pickerVisible = ref(false)
const pickerContainer = ref(null)

const form = reactive({
  name: '',
  civil_code: '',
  address: '',
  ip_address: '',
  port: 0,
  manufacturer: '',
  model: '',
  latitude: null,
  longitude: null,
})

const dialogTitle = computed(() => (props.mode === 'create' ? '新建通道' : '编辑通道'))

const positionHint = computed(() => {
  if (form.latitude === null || form.longitude === null) {
    return '未设置坐标的通道不会显示在地图上'
  }
  return `${Number(form.latitude).toFixed(6)}, ${Number(form.longitude).toFixed(6)}`
})

let map = null
let marker = null

function resetForm(channel) {
  form.name = channel?.name || ''
  form.civil_code = channel?.civil_code || ''
  form.address = channel?.address || ''
  form.ip_address = channel?.ip_address || ''
  form.port = channel?.port ?? 0
  form.manufacturer = channel?.manufacturer || ''
  form.model = channel?.model || ''
  form.latitude = channel?.latitude ?? null
  form.longitude = channel?.longitude ?? null
}

async function onOpen() {
  resetForm()
  if (props.mode === 'edit' && props.channelTag) {
    try {
      const channel = await getChannel(props.deviceTag, props.channelTag)
      resetForm(channel)
    } catch (e) {
      ElMessage.error('加载通道信息失败: ' + (e?.message || e))
    }
  }
}

function clearPosition() {
  form.latitude = null
  form.longitude = null
  if (marker && map) {
    marker.remove()
    marker = null
  }
}

function applyPosition(lng, lat) {
  form.longitude = Number(lng.toFixed(6))
  form.latitude = Number(lat.toFixed(6))
}

const DEFAULT_TMS_URL =
  'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'

function clampZoom(value, fallback) {
  const n = Number(value)
  if (!Number.isFinite(n)) return fallback
  return Math.min(24, Math.max(0, Math.round(n)))
}

async function initPickerMap() {
  await nextTick()
  if (!pickerContainer.value || map) return

  await settingsStore.loadMapSettings()

  const configured = (settingsStore.browserTmsUrl || '')
    .split('\n')
    .map((u) => u.trim())
    .filter((u) => u && u.includes('{x}') && u.includes('{z}'))
  const tiles = configured.length > 0 ? configured : [DEFAULT_TMS_URL]

  // 与 DeviceMap.vue 保持一致：配置里写 {-y} 就按 TMS 处理，其余按标准 XYZ
  const hasNegY = tiles.some((u) => u.includes('{-y}'))
  const normalized = tiles.map((u) => (hasNegY ? u.replace(/\{-y\}/g, '{y}') : u))

  const center =
    form.longitude !== null && form.latitude !== null
      ? [form.longitude, form.latitude]
      : [121.4737, 31.2304]

  map = new MapLibreMap({
    container: pickerContainer.value,
    center,
    zoom: 11,
    zoomControl: true,
    // 内联空样式：避免 MapLibre 去请求默认在线样式
    style: {
      version: 8,
      sources: {},
      layers: [
        { id: 'background', type: 'background', paint: { 'background-color': '#1a2a3a' } },
      ],
    },
  })

  map.on('load', () => {
    map.addSource('raster-tiles', {
      type: 'raster',
      tiles: normalized,
      tileSize: 256,
      minzoom: clampZoom(settingsStore.browserMinZoom, 0),
      maxzoom: clampZoom(settingsStore.browserMaxZoom, 19),
      scheme: hasNegY ? 'tms' : 'xyz',
    })
    map.addLayer({
      id: 'raster-tiles-layer',
      type: 'raster',
      source: 'raster-tiles',
    })

    if (form.longitude !== null && form.latitude !== null) {
      marker = new Marker().setLngLat([form.longitude, form.latitude]).addTo(map)
    }
  })

  map.on('error', (e) => {
    console.error('[ChannelEditDialog] map error:', e && e.error && e.error.message ? e.error.message : e)
  })

  map.on('click', (e) => {
    applyPosition(e.lngLat.lng, e.lngLat.lat)
    if (marker) {
      marker.setLngLat([form.longitude, form.latitude])
    } else if (map) {
      marker = new Marker().setLngLat([form.longitude, form.latitude]).addTo(map)
    }
  })
}

function openPicker() {
  pickerVisible.value = true
}

function destroyPickerMap() {
  if (map) {
    map.remove()
    map = null
    marker = null
  }
}

// 直接 watch ref 本身，而不是 watch(() => pickerVisible.value)，
// 后者拿到的 getter 返回值不是响应式源，watcher 永远不会触发。
watch(pickerVisible, async (visible) => {
  if (visible) {
    await initPickerMap()
  } else {
    destroyPickerMap()
  }
})

async function submit() {
  if (props.mode === 'create' && (!props.deviceTag || !props.channelTag)) {
    ElMessage.error('新建通道需要指定设备标识与通道标识')
    return
  }

  saving.value = true
  try {
    const payload = {
      name: form.name || null,
      civil_code: form.civil_code || null,
      address: form.address || null,
      ip_address: form.ip_address || null,
      port: form.port ?? 0,
      manufacturer: form.manufacturer || null,
      model: form.model || null,
      latitude: form.latitude,
      longitude: form.longitude,
    }

    let channel
    if (props.mode === 'create') {
      channel = await createChannel({
        device_tag: props.deviceTag,
        channel_tag: props.channelTag,
        name: form.name || props.channelTag,
        civil_code: payload.civil_code,
        address: payload.address,
        ip_address: payload.ip_address,
        port: payload.port,
        manufacturer: payload.manufacturer,
        model: payload.model,
        latitude: payload.latitude,
        longitude: payload.longitude,
      })
    } else {
      channel = await updateChannel(props.deviceTag, props.channelTag, payload)
    }

    ElMessage.success('保存成功')
    emit('saved', channel)
    emit('update:modelValue', false)
  } catch (e) {
    ElMessage.error('保存失败: ' + (e?.message || e))
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.picker-wrap {
  display: flex;
  flex-direction: column;
}
.picker-map {
  height: calc(100vh - 180px);
  min-height: 360px;
  border-radius: 4px;
  overflow: hidden;
}
.picker-tip {
  margin-top: 8px;
  font-size: 12px;
  color: #909399;
}
.position-hint {
  margin-left: 12px;
  font-size: 12px;
  color: #909399;
}
</style>
