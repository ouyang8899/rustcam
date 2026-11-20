<template>
  <AppDialog
    :model-value="modelValue"
    :title="dialogTitle"
    size="lg"
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
          <el-form-item label="地址">
            <el-input v-model="form.address" placeholder="安装位置描述" />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="通道标识">
            <el-input
              v-model="form.channel_tag"
              :disabled="mode !== 'create'"
              placeholder="如 ch-1"
            />
          </el-form-item>
        </el-col>
        <el-col :span="12">
          <el-form-item label="设备标识">
            <el-input
              v-model="form.device_tag"
              :disabled="mode !== 'create' || !!deviceTag"
              placeholder="平台设备 tag"
            />
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
  </AppDialog>
</template>

<script setup>
import { ref, reactive, computed, nextTick, watch } from 'vue'
import { Map as MapLibreMap, Marker } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import AppDialog from '../common/AppDialog.vue'
import { useToast } from '../../composables/useToast'

const toast = useToast()
import { useSettingsStore } from '../../stores/settingsStore'
import { buildRasterSource, selectLayers, viewZoomRange } from '../../utils/mapTiles'
import { updateChannel, createChannel, getChannel } from '../../api/channels'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  mode: { type: String, default: 'edit' },
  deviceTag: { type: String, default: '' },
  channelTag: { type: String, default: '' },
  channel: { type: Object, default: null },
})

const emit = defineEmits(['update:modelValue', 'saved'])

const settingsStore = useSettingsStore()
const saving = ref(false)
const pickerVisible = ref(false)
const pickerContainer = ref(null)

const form = reactive({
  name: '',
  address: '',
  latitude: null,
  longitude: null,
  channel_tag: '',
  device_tag: '',
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
  form.name = channel?.name ?? ''
  form.address = channel?.address ?? ''
  form.latitude = channel?.latitude ?? null
  form.longitude = channel?.longitude ?? null
  form.channel_tag = channel?.channel_tag ?? props.channelTag
  form.device_tag = channel?.device_tag ?? props.deviceTag
}

async function onOpen() {
  resetForm(props.channel)
  if (props.mode === 'edit' && props.channelTag) {
    try {
      const channel = await getChannel(props.deviceTag, props.channelTag)
      resetForm({
        ...props.channel,
        ...channel,
        name: channel?.name || props.channel?.name || '',
      })
    } catch (e) {
      toast.error('加载通道信息失败: ' + (e?.message || e))
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

async function initPickerMap() {
  await nextTick()
  if (!pickerContainer.value || map) return

  await settingsStore.loadMapSettings()

  const layers = selectLayers(settingsStore.browserLayers, '2d')
  let source = null
  let chosen = null
  for (const layer of layers) {
    const candidate = buildRasterSource(layer)
    if (candidate) {
      source = candidate
      chosen = layer
      break
    }
    console.warn('[ChannelEditDialog] 底图图层在该端不可用，已跳过:', layer.name || layer.url, layer.srs)
    toast.warning(
      `底图图层「${layer.name || layer.url}」的投影 ${layer.srs} 不被浏览器 2D 支持（仅 EPSG:3857），已跳过`,
    )
  }

  const { minZoom, maxZoom } = viewZoomRange(chosen)
  if (!source) {
    source = {
      type: 'raster',
      tiles: [DEFAULT_TMS_URL],
      tileSize: 256,
      minzoom: minZoom,
      maxzoom: maxZoom,
      scheme: 'xyz',
    }
  }

  const center =
    form.longitude !== null && form.latitude !== null
      ? [form.longitude, form.latitude]
      : [121.4737, 31.2304]

  map = new MapLibreMap({
    container: pickerContainer.value,
    center,
    zoom: 11,
    minZoom,
    maxZoom,
    zoomControl: true,
    style: {
      version: 8,
      sources: {},
      layers: [
        { id: 'background', type: 'background', paint: { 'background-color': '#1a2a3a' } },
      ],
    },
  })

  map.on('load', () => {
    map.addSource('raster-tiles', source)
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

watch(pickerVisible, async (visible) => {
  if (visible) {
    await initPickerMap()
  } else {
    destroyPickerMap()
  }
})

async function submit() {
  const deviceTag = form.device_tag.trim()
  const channelTag = form.channel_tag.trim()
  if (props.mode === 'create' && (!deviceTag || !channelTag)) {
    toast.error('新建通道需要指定设备标识与通道标识')
    return
  }

  saving.value = true
  try {
    const payload = {
      name: form.name || null,
      address: form.address || null,
      latitude: form.latitude,
      longitude: form.longitude,
    }

    let channel
    if (props.mode === 'create') {
      channel = await createChannel({
        device_tag: deviceTag,
        channel_tag: channelTag,
        name: form.name || channelTag,
        address: payload.address,
        latitude: payload.latitude,
        longitude: payload.longitude,
      })
    } else {
      channel = await updateChannel(props.deviceTag, props.channelTag, payload)
    }

    toast.success('保存成功')
    emit('saved', channel)
    emit('update:modelValue', false)
  } catch (e) {
    toast.error('保存失败: ' + (e?.message || e))
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
  border-radius: 0;
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
