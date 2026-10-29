<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">电子地图</h1>
      <div class="page-toolbar">
        <el-input v-model="searchText" placeholder="搜索设备名称" style="width: 200px; margin-right: 12px" clearable />
        <el-select v-model="filterStatus" placeholder="筛选状态" style="width: 120px; margin-right: 12px" clearable>
          <el-option label="在线" value="online" />
          <el-option label="离线" value="offline" />
          <el-option label="故障" value="error" />
          <el-option label="维修" value="maintaining" />
          <el-option label="报警" value="intrusion" />
        </el-select>
        <el-button @click="fitToPoints" :disabled="!hasPoints">显示全部</el-button>
      </div>
    </div>
    <div class="map-wrapper">
      <div id="device-map" ref="mapContainer"></div>

      <div class="map-legend">
        <div class="legend-title">摄像头类型</div>
        <div v-for="item in typeLegend" :key="item.type" class="legend-item">
          <span class="legend-icon" :style="{ color: item.color }" v-html="cameraIcon(item.type)"></span>
          <span class="legend-label">{{ item.label }}</span>
        </div>
      </div>

      <div v-if="selectedCamera" class="device-popup">
        <div class="popup-header">
          <span class="device-name">{{ selectedCamera.name || selectedCamera.channel_tag }}</span>
          <el-button text size="small" @click="selectedCamera = null"><el-icon><Close /></el-icon></el-button>
        </div>
        <div class="popup-body">
          <el-descriptions :column="1" size="small">
            <el-descriptions-item label="设备ID">{{ selectedCamera.device_tag }}</el-descriptions-item>
            <el-descriptions-item label="通道ID">{{ selectedCamera.channel_tag }}</el-descriptions-item>
            <el-descriptions-item label="类型">
              <span class="status-badge" :style="{ background: selectedCamera.color }">{{ selectedCamera.device_type }}</span>
            </el-descriptions-item>
            <el-descriptions-item label="状态">
              <span class="status-badge" :class="selectedCamera.status">{{ getStatusLabel(selectedCamera.status) }}</span>
            </el-descriptions-item>
          </el-descriptions>
          <div class="popup-actions">
            <el-button type="primary" size="small" @click="playCamera">播放</el-button>
          </div>
        </div>
      </div>
    </div>

    <el-dialog v-model="showPlayer" :title="playerTitle" width="640px" destroy-on-close>
      <div class="player-wrapper">
        <VideoPlayer
          v-if="playUrl"
          :liveSrc="playUrl"
          type="auto"
          :isLive="true"
          :autoplay="true"
        />
        <div v-else class="player-placeholder">正在获取播放地址...</div>
      </div>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { useRoute } from 'vue-router'
import { Map as MapLibreMap, Marker } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { TerraDraw, TerraDrawPolygonMode, TerraDrawLineStringMode, TerraDrawPointMode, TerraDrawSelectMode } from 'terra-draw'
import { TerraDrawMapLibreGLAdapter } from 'terra-draw-maplibre-gl-adapter'
import Supercluster from 'supercluster'
import { ElMessage } from 'element-plus'
import { Close } from '@element-plus/icons-vue'
import { useSettingsStore } from '../stores/settingsStore'
import { request } from '../utils/request'
import VideoPlayer from '../components/VideoPlayer.vue'

const settingsStore = useSettingsStore()
const route = useRoute()
const mapContainer = ref(null)
let map = null
let draw = null
let cluster = null

const searchText = ref('')
const filterStatus = ref('')
const selectedCamera = ref(null)
const showPlayer = ref(false)
const playUrl = ref('')

// 上海默认坐标：设备没有经纬度时使用
const SHANGHAI_LNG = 121.4737
const SHANGHAI_LAT = 31.2304
const SHANGHAI_ZOOM = 11

// 摄像头类型 -> 颜色
const TYPE_COLORS = {
  ipc: '#3b82f6',
  camera: '#8b5cf6',
  nvr: '#f59e0b',
  dvr: '#ef4444',
  encoder: '#10b981',
  videoserver: '#ec4899',
  platform: '#06b6d4',
  other: '#9ca3af',
}

const TYPE_LABELS = {
  ipc: '网络摄像机 IPC',
  camera: '摄像机 Camera',
  nvr: '录像机 NVR',
  dvr: '数字录像机 DVR',
  encoder: '编码器 Encoder',
  videoserver: '视频服务器',
  platform: '平台设备',
  other: '其他',
}

const STATUS_COLORS = {
  online: '#22c55e',
  offline: '#9ca3af',
  error: '#f97316',
  maintaining: '#3b82f6',
  intrusion: '#ef4444',
}

const STATUS_LABELS = {
  online: '在线',
  offline: '离线',
  error: '故障',
  maintaining: '维修',
  intrusion: '入侵报警',
}

// 所有点位（摄像头 + 无通道的设备）
const points = ref([])
const clusterMarkers = new Map()
const clusterInfo = new Map()
const cameraMarkers = new Map()
const cameraProps = new Map()
let locatedKey = ''

// 标准摄像头图标（按类型）
const CAMERA_ICONS = {
  // 枪机 / 网络摄像机
  ipc: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <rect x="2.5" y="8" width="9.5" height="7.5" rx="1.6"/>
    <path d="M12 10.2 L20.2 7.2 L21.6 11.2 L13.4 13.6 Z"/>
    <path d="M7.2 15.5 V18.6 M4 18.6 H11"/>
  </svg>`,
  // 球机
  camera: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <path d="M4.2 12.6 A7.8 7.8 0 0 1 19.8 12.6 Z"/>
    <path d="M3.4 12.6 H20.6 M4.6 12.6 V15.2 H19.4 V12.6"/>
    <circle cx="12" cy="9.4" r="1.5"/>
  </svg>`,
  // 录像机 / NVR / DVR
  nvr: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <rect x="2.5" y="6" width="19" height="12" rx="2"/>
    <path d="M6 10.6 H15.5 M6 13.6 H11"/>
    <circle cx="17.6" cy="14" r="1"/>
  </svg>`,
  dvr: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <rect x="2.5" y="6" width="19" height="12" rx="2"/>
    <path d="M6 10.6 H15.5 M6 13.6 H11"/>
    <circle cx="17.6" cy="14" r="1"/>
  </svg>`,
  // 编码器
  encoder: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <rect x="2.5" y="7" width="13" height="10" rx="2"/>
    <path d="M6 10.8 H11.5 M6 13.4 H9.5"/>
    <path d="M16.2 12 H21.5 M19.2 9.4 L21.8 12 L19.2 14.6"/>
  </svg>`,
  // 视频服务器
  videoserver: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <rect x="3.5" y="3.5" width="17" height="6.5" rx="1.6"/>
    <rect x="3.5" y="14" width="17" height="6.5" rx="1.6"/>
    <circle cx="7.4" cy="6.8" r="1"/>
    <circle cx="7.4" cy="17.3" r="1"/>
    <path d="M11 6.8 H17 M11 17.3 H17"/>
  </svg>`,
  // 平台
  platform: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <circle cx="12" cy="12" r="8.6"/>
    <path d="M3.4 12 H20.6"/>
    <ellipse cx="12" cy="12" rx="4" ry="8.6"/>
  </svg>`,
  // 其他
  other: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <rect x="2.5" y="8" width="10" height="7.5" rx="1.6"/>
    <path d="M12.5 10.2 L20.5 7.6 L21.5 11.6 L13.5 13.6 Z"/>
  </svg>`,
}

function cameraIcon(deviceType) {
  return CAMERA_ICONS[(deviceType || 'other').toLowerCase()] || CAMERA_ICONS.other
}

const hasPoints = computed(() => points.value.length > 0)

const playerTitle = computed(() => {
  const c = selectedCamera.value
  if (!c) return '实时视频'
  const label = c.name || c.channel_tag || c.device_tag || ''
  return label ? `实时视频 - ${label}` : '实时视频'
})

const typeLegend = computed(() => {
  const used = new Set(points.value.map(p => (p.device_type || 'Other').toLowerCase()))
  const entries = Object.keys(TYPE_COLORS).filter(t => used.has(t))
  const list = entries.length ? entries : ['other']
  return list.map(t => ({ type: t, label: TYPE_LABELS[t] || t, color: TYPE_COLORS[t] }))
})

function getStatusLabel(status) {
  return STATUS_LABELS[status] || status || '未知'
}

function typeColor(deviceType) {
  return TYPE_COLORS[(deviceType || 'other').toLowerCase()] || TYPE_COLORS.other
}

function statusColor(status) {
  return STATUS_COLORS[(status || 'offline').toLowerCase()] || STATUS_COLORS.offline
}

// WFS: 通道点位图层 myw:channels（开发环境走后端代理，正式部署由 nginx 代理）
async function fetchWfsChannels() {
  const wfsUrl = settingsStore.mapWfsUrl
  if (!wfsUrl) {
    ElMessage.warning('请先在设置中配置 WFS 服务地址')
    return new Map()
  }

  let targetUrl = wfsUrl
  const useProxy = import.meta.env.DEV
  const fetchUrl = useProxy ? `/api/v1/proxy/wfs?url=${encodeURIComponent(targetUrl)}` : targetUrl

  try {
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 10000)
    const res = await fetch(fetchUrl, { signal: controller.signal })
    clearTimeout(timeoutId)

    if (!res.ok) {
      console.warn('[DeviceMap] WFS response not ok:', res.status)
      return new Map()
    }

    const text = await res.text()
    if (!text || text.trim() === '') return new Map()

    const trimmed = text.trim()
    const channels = new Map()

    if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
      const geojson = JSON.parse(text)
      for (const f of geojson.features || []) {
        const props = f.properties || {}
        const deviceTag = (props.device_tag || '').toLowerCase()
        const channelTag = (props.channel_tag || '').toLowerCase()
        if (!deviceTag || !channelTag) continue
        const tag = `${deviceTag}|${channelTag}`

        let lng = null
        let lat = null
        if (f.geometry && f.geometry.coordinates) {
          lng = f.geometry.coordinates[0]
          lat = f.geometry.coordinates[1]
        } else {
          lng = parseFloat(props.longitude || props.lng || props.lon)
          lat = parseFloat(props.latitude || props.lat)
        }
        if (!Number.isFinite(lng) || !Number.isFinite(lat)) continue

        channels.set(tag, {
          device_tag: deviceTag,
          channel_tag: channelTag,
          name: props.name || channelTag,
          device_type: props.device_type || 'Other',
          status: (props.status || 'offline').toLowerCase(),
          lng,
          lat,
        })
      }
    } else {
      const parser = new DOMParser()
      const xml = parser.parseFromString(text, 'text/xml')
      const featureCollections = xml.getElementsByTagName('FeatureCollection')
      if (featureCollections.length > 0) {
        const memberElements = featureCollections[0].getElementsByTagName('member')
        for (const member of memberElements) {
          const featureMember = member.firstElementChild
          if (!featureMember) continue

          const properties = {}
          for (const child of featureMember.children) {
            properties[child.tagName.split(':').pop()] = child.textContent
          }
          const deviceTag = (properties.device_tag || '').toLowerCase()
          const channelTag = (properties.channel_tag || '').toLowerCase()
          if (!deviceTag || !channelTag) continue
          const tag = `${deviceTag}|${channelTag}`

          let lng = parseFloat(properties.longitude || properties.lng || properties.lon)
          let lat = parseFloat(properties.latitude || properties.lat)
          if (!Number.isFinite(lng) || !Number.isFinite(lat)) {
            const pointElement = featureMember.querySelector('Point') || featureMember.querySelector('[gml\\:Point]')
            if (pointElement) {
              const coords = pointElement.textContent.trim().split(/[\s,]+/)
              if (coords.length >= 2) {
                lng = parseFloat(coords[0])
                lat = parseFloat(coords[1])
              }
            }
          }
          if (!Number.isFinite(lng) || !Number.isFinite(lat)) continue

          channels.set(tag, {
            device_tag: deviceTag,
            channel_tag: channelTag,
            name: properties.name || channelTag,
            device_type: properties.device_type || 'Other',
            status: (properties.status || 'offline').toLowerCase(),
            lng,
            lat,
          })
        }
      }
    }

    return channels
  } catch (e) {
    if (e.name === 'AbortError') {
      console.warn('[DeviceMap] WFS request timeout')
    } else {
      console.error('[DeviceMap] Failed to fetch WFS:', e)
    }
    return new Map()
  }
}

// 通道列表：提供摄像头类型与状态
async function fetchChannels() {
  try {
    const res = await request.get('/channels', { params: { limit: 500 } })
    const data = res.data?.data || res.data
    return data?.items || []
  } catch (e) {
    console.error('[DeviceMap] Failed to fetch channels:', e)
    return []
  }
}

function matchesFilter(point) {
  const search = searchText.value.trim().toLowerCase()
  if (search) {
    const haystack = `${point.name || ''} ${point.device_tag || ''} ${point.channel_tag || ''}`.toLowerCase()
    if (!haystack.includes(search)) return false
  }
  if (filterStatus.value && (point.status || 'offline') !== filterStatus.value) return false
  return true
}

function buildPoints(wfsChannels, channels) {
  const result = []

  for (const channel of channels) {
    const deviceTag = (channel.device_tag || '').toLowerCase()
    const channelTag = (channel.channel_tag || '').toLowerCase()
    const key = `${deviceTag}|${channelTag}`
    const wfs = wfsChannels.get(key)

    // 坐标优先级：API 返回的通道坐标 → WFS feature 坐标 → 不上图。
    // 早期版本围绕设备点位做黄金角偏移，是为解决"一个设备多通道挤在同一点"；
    // 现在每个通道有独立坐标，偏移只会让点位失真。
    let lng = null
    let lat = null
    if (channel.latitude != null && channel.longitude != null) {
      lng = Number(channel.longitude)
      lat = Number(channel.latitude)
    } else if (wfs) {
      lng = wfs.lng
      lat = wfs.lat
    }
    if (!Number.isFinite(lng) || !Number.isFinite(lat)) continue

    result.push({
      kind: 'camera',
      device_tag: channel.device_tag,
      channel_tag: channel.channel_tag,
      name: channel.name || channel.channel_tag,
      device_type: channel.device_type || (wfs ? wfs.device_type : 'Other'),
      status: (channel.status || (wfs ? wfs.status : 'offline')).toLowerCase(),
      protocol: wfs ? wfs.protocol || '' : '',
      lng,
      lat,
    })
  }

  return result
}

function toFeatureCollection(list) {
  return {
    type: 'FeatureCollection',
    features: list.map(p => ({
      type: 'Feature',
      geometry: { type: 'Point', coordinates: [p.lng, p.lat] },
      properties: {
        kind: p.kind,
        device_tag: p.device_tag,
        channel_tag: p.channel_tag,
        name: p.name,
        device_type: p.device_type,
        status: p.status,
        color: typeColor(p.device_type),
        status_color: statusColor(p.status),
      },
    })),
  }
}

function initCluster() {
  const features = points.value
    .filter(matchesFilter)
    .map(p => ({
      type: 'Feature',
      geometry: { type: 'Point', coordinates: [p.lng, p.lat] },
      properties: {
        kind: p.kind,
        device_tag: p.device_tag,
        channel_tag: p.channel_tag,
        name: p.name,
        device_type: p.device_type,
        status: p.status,
        color: typeColor(p.device_type),
        status_color: statusColor(p.status),
      },
    }))

  cluster = new Supercluster({
    radius: 60,
    maxZoom: 18,
  })
  cluster.load(features)
}

function updateMapView() {
  if (!map || !cluster) return

  const bounds = map.getBounds()
  const zoom = Math.floor(map.getZoom())

  const clusters = cluster.getClusters(
    [bounds.getWest(), bounds.getSouth(), bounds.getEast(), bounds.getNorth()],
    zoom
  )

  renderClusterMarkers(clusters)
}

// 稠密摄像头按 zoom 合并，标识为数量；单个摄像头用类型图标绘制
function renderClusterMarkers(clusters) {
  if (!map) return

  const seen = new Set()

  for (const feature of clusters) {
    const props = feature.properties || {}
    const [lng, lat] = feature.geometry.coordinates

    if (props.cluster) {
      const id = props.cluster_id
      const count = props.point_count
      seen.add('c' + id)

      let marker = clusterMarkers.get(id)
      if (!marker) {
        const el = document.createElement('div')
        el.className = 'map-cluster'
        el.textContent = count
        el.addEventListener('click', (e) => {
          e.stopPropagation()
          const info = clusterInfo.get(id)
          if (info) zoomToCluster(id, info.lng, info.lat)
        })
        marker = new Marker({ element: el }).setLngLat([lng, lat]).addTo(map)
        clusterMarkers.set(id, marker)
      } else {
        marker.setLngLat([lng, lat])
        marker.getElement().textContent = count
      }
      clusterInfo.set(id, { lng, lat, count })
      continue
    }

    const key = `${props.device_tag}|${props.channel_tag}`
    seen.add(key)
    cameraProps.set(key, props)

    let marker = cameraMarkers.get(key)
    if (!marker) {
      const el = document.createElement('div')
      el.className = 'map-camera'
      el.addEventListener('click', (e) => {
        e.stopPropagation()
        const current = cameraProps.get(key)
        if (current) onCameraMarkerClick(current, lng, lat)
      })
      marker = new Marker({ element: el, anchor: 'center' }).setLngLat([lng, lat]).addTo(map)
      cameraMarkers.set(key, marker)
      updateCameraMarkerElement(marker.getElement(), props)
    } else {
      marker.setLngLat([lng, lat])
      updateCameraMarkerElement(marker.getElement(), props)
    }
  }

  for (const [id, marker] of clusterMarkers) {
    if (!seen.has('c' + id)) {
      marker.remove()
      clusterMarkers.delete(id)
      clusterInfo.delete(id)
    }
  }
  for (const [key, marker] of cameraMarkers) {
    if (!seen.has(key)) {
      marker.remove()
      cameraMarkers.delete(key)
      cameraProps.delete(key)
    }
  }
}

function updateCameraMarkerElement(el, props) {
  const status = (props.status || 'offline').toLowerCase()
  const type = (props.device_type || 'Other').toLowerCase()

  if (el.dataset.status !== status || el.dataset.type !== type) {
    el.dataset.status = status
    el.dataset.type = type
    el.innerHTML = `
      <span class="map-camera-disc" style="background:${statusColor(status)}"></span>
      <span class="map-camera-icon">${cameraIcon(props.device_type)}</span>
    `
  }

  el.title = `${props.name || props.device_tag || ''} (${props.device_type || 'Other'})`
  el.classList.toggle('located', locatedKey === `${props.device_tag}|${props.channel_tag}`)
}

function onCameraMarkerClick(props, lng, lat) {
  selectedCamera.value = {
    kind: props.kind,
    device_tag: props.device_tag,
    channel_tag: props.channel_tag,
    name: props.name,
    device_type: props.device_type,
    status: props.status,
    color: typeColor(props.device_type),
    lng,
    lat,
  }
  locatedKey = `${props.device_tag}|${props.channel_tag}`
  updateMapView()
  playCamera()
}

function zoomToCluster(clusterId, lng, lat) {
  if (!cluster) return
  const zoom = cluster.getClusterExpansionZoom(clusterId)
  map.easeTo({ center: [lng, lat], zoom })
}

function clearClusterMarkers() {
  for (const marker of clusterMarkers.values()) {
    marker.remove()
  }
  clusterMarkers.clear()
  for (const marker of cameraMarkers.values()) {
    marker.remove()
  }
  cameraMarkers.clear()
}

async function loadPoints() {
  if (!map) return

  const [wfsChannels, channels] = await Promise.all([fetchWfsChannels(), fetchChannels()])
  points.value = buildPoints(wfsChannels, channels)

  initCluster()
  updateMapView()

  if (points.value.length === 0) {
    ElMessage.warning('未获取到设备数据，请检查 WFS 服务与通道配置')
  }
}

function fitToPoints() {
  if (!map || points.value.length === 0) return

  let minLng = Infinity, minLat = Infinity, maxLng = -Infinity, maxLat = -Infinity
  for (const p of points.value) {
    minLng = Math.min(minLng, p.lng)
    minLat = Math.min(minLat, p.lat)
    maxLng = Math.max(maxLng, p.lng)
    maxLat = Math.max(maxLat, p.lat)
  }

  if (minLng !== Infinity) {
    map.fitBounds([[minLng, minLat], [maxLng, maxLat]], { padding: [80, 80], maxZoom: 16 })
  }
}

// 精确定位：跳到摄像头点并高亮
function locateCamera(deviceTag, channelTag) {
  if (!map || !deviceTag) return

  const target = points.value.find(p =>
    (p.device_tag || '').toLowerCase() === deviceTag.toLowerCase() &&
    (!channelTag || (p.channel_tag || '').toLowerCase() === channelTag.toLowerCase())
  )

  if (!target) {
    ElMessage.warning('地图上未找到该设备的通道')
    return
  }

  map.flyTo({ center: [target.lng, target.lat], zoom: 17, duration: 1200 })
  selectedCamera.value = { ...target, color: typeColor(target.device_type) }
  locatedKey = `${target.device_tag}|${target.channel_tag}`
  updateMapView()
}

async function playCamera() {
  const camera = selectedCamera.value
  if (!camera?.device_tag) return

  const channelTag = camera.channel_tag || camera.device_tag

  try {
    playUrl.value = ''
    const res = await request.get(
      `/channels/${encodeURIComponent(camera.device_tag)}/${encodeURIComponent(channelTag)}/play-links`
    )
    const data = res.data?.data || res.data
    const url = data?.flv || data?.web_flv || data?.hls || data?.webrtc
    if (!url) {
      ElMessage.warning('无法获取播放地址')
      return
    }
    playUrl.value = url
    showPlayer.value = true
  } catch (e) {
    ElMessage.error('获取播放地址失败: ' + (e.message || e))
  }
}

const DEFAULT_TMS_URL = 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'

function clampZoom(value, fallback) {
  const n = Number(value)
  if (!Number.isFinite(n)) return fallback
  return Math.min(24, Math.max(0, Math.round(n)))
}

function addBasemap() {
  const configured = settingsStore.browserTmsUrl
  const tileUrls = (configured || '')
    .split('\n')
    .map(u => u.trim())
    .filter(u => u && u.includes('{x}') && u.includes('{z}'))

  const tiles = tileUrls.length > 0 ? tileUrls : [DEFAULT_TMS_URL]

  const minZoom = clampZoom(settingsStore.browserMinZoom, 0)
  const maxZoom = clampZoom(settingsStore.browserMaxZoom, 19)

  // MapLibre 通过 scheme:'tms' 处理 TMS 的 y 轴翻转；
  // 约定：配置里写 {-y} 就按 TMS 处理，其余按标准 XYZ 直接请求。
  const hasNegY = tiles.some(u => u.includes('{-y}'))
  const normalized = tiles.map(u => (hasNegY ? u.replace(/\{-y\}/g, '{y}') : u))

  if (map.getLayer('raster-tiles-layer')) {
    map.removeLayer('raster-tiles-layer')
  }
  if (map.getSource('raster-tiles')) {
    map.removeSource('raster-tiles')
  }

  map.addSource('raster-tiles', {
    type: 'raster',
    tiles: normalized,
    tileSize: 256,
    minzoom: minZoom,
    maxzoom: maxZoom,
    scheme: hasNegY ? 'tms' : 'xyz',
  })
  map.addLayer({
    id: 'raster-tiles-layer',
    type: 'raster',
    source: 'raster-tiles',
    minzoom: minZoom,
    maxzoom: maxZoom,
  })

  map.setMinZoom(minZoom)
  map.setMaxZoom(maxZoom)
  if (map.getZoom() < minZoom) map.setZoom(minZoom)
  if (map.getZoom() > maxZoom) map.setZoom(maxZoom)
}

let loadTimer = null
function debouncedLoad() {
  if (loadTimer) clearTimeout(loadTimer)
  loadTimer = setTimeout(() => loadPoints(), 300)
}

watch([searchText, filterStatus], () => {
  if (map && cluster) {
    initCluster()
    updateMapView()
  }
})

watch(() => `${route.query.device_tag || ''}|${route.query.channel_tag || ''}`, () => {
  locateCamera(route.query.device_tag, route.query.channel_tag)
})

onMounted(async () => {
  if (!mapContainer.value) return

  await settingsStore.loadMapSettings()

  map = new MapLibreMap({
    container: 'device-map',
    center: [SHANGHAI_LNG, SHANGHAI_LAT],
    zoom: SHANGHAI_ZOOM,
    zoomControl: true,
    // 内联空样式：避免 MapLibre 去请求默认在线样式，导致离线时 load 不触发
    style: {
      version: 8,
      sources: {},
      layers: [
        {
          id: 'background',
          type: 'background',
          paint: { 'background-color': '#1a2a3a' },
        },
      ],
    },
  })

  map.on('load', () => {
    if (import.meta.env.DEV) window.__deviceMap = map
    map.on('error', (e) => {
      console.error('[DeviceMap] map error:', e && e.error && e.error.message ? e.error.message : e)
    })

    const adapter = new TerraDrawMapLibreGLAdapter({
      map,
    })

    draw = new TerraDraw({
      adapter,
      modes: [
        new TerraDrawPointMode(),
        new TerraDrawLineStringMode(),
        new TerraDrawPolygonMode(),
        new TerraDrawSelectMode(),
      ],
    })

    draw.start()

    addBasemap()

    loadPoints().then(() => {
      if (route.query.device_tag) {
        locateCamera(route.query.device_tag, route.query.channel_tag)
      }
    })
  })

  map.on('moveend', () => {
    debouncedLoad()
  })

  map.on('zoomend', () => {
    debouncedLoad()
  })

})

onUnmounted(() => {
  if (map) {
    clearClusterMarkers()
    map.remove()
    map = null
  }
  if (loadTimer) {
    clearTimeout(loadTimer)
  }
})
</script>

<style scoped>
.page-container {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
}

.page-title {
  margin: 0;
  font-size: 18px;
}

.page-toolbar {
  display: flex;
  align-items: center;
}

.map-wrapper {
  flex: 1;
  position: relative;
  overflow: hidden;
}

#device-map {
  width: 100%;
  height: 100%;
}

.map-legend {
  position: absolute;
  left: 12px;
  bottom: 24px;
  z-index: 5;
  background: rgba(17, 24, 39, 0.82);
  color: #e5e7eb;
  border-radius: 6px;
  padding: 10px 12px;
  font-size: 12px;
  backdrop-filter: blur(4px);
  pointer-events: none;
}

.legend-title {
  font-weight: 600;
  margin-bottom: 6px;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 6px;
  line-height: 18px;
}

.legend-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  display: inline-block;
}

.legend-icon {
  display: inline-flex;
  width: 16px;
  height: 16px;
}

.legend-icon :deep(svg) {
  width: 16px;
  height: 16px;
}

.device-popup {
  position: absolute;
  right: 16px;
  top: 16px;
  width: 280px;
  z-index: 10;
  background: #fff;
  border-radius: 8px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  overflow: hidden;
}

.popup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: #f5f7fa;
  border-bottom: 1px solid #e5e7eb;
}

.device-name {
  font-weight: 600;
  font-size: 14px;
}

.popup-body {
  padding: 10px 12px;
}

.popup-actions {
  margin-top: 10px;
  display: flex;
  justify-content: flex-end;
}

.status-badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 10px;
  font-size: 12px;
  color: #fff;
  background: #9ca3af;
}

.status-badge.online { background: #22c55e; }
.status-badge.offline { background: #9ca3af; }
.status-badge.error { background: #f97316; }
.status-badge.maintaining { background: #3b82f6; }
.status-badge.intrusion { background: #ef4444; }

.player-wrapper {
  width: 100%;
  aspect-ratio: 16 / 9;
  background: #000;
}

.player-placeholder {
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}
</style>

<style>
/* 聚合标识：显示摄像头数量 */
.map-cluster {
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: rgba(59, 130, 246, 0.92);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 2px solid #fff;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
  cursor: pointer;
  user-select: none;
}

/* 摄像头点位：类型图标 + 状态底色 */
.map-camera {
  position: relative;
  width: 28px;
  height: 28px;
  cursor: pointer;
}

.map-camera-disc {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.95);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
}

.map-camera-icon {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
}

.map-camera-icon svg {
  width: 17px;
  height: 17px;
}

.map-camera.located::after {
  content: '';
  position: absolute;
  inset: -8px;
  border-radius: 50%;
  border: 3px solid #facc15;
  animation: camera-pulse 1.4s ease-out infinite;
}

@keyframes camera-pulse {
  0% { transform: scale(0.85); opacity: 1; }
  100% { transform: scale(1.35); opacity: 0; }
}
</style>
