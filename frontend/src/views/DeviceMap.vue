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
        <el-button @click="fitToDevices" :disabled="!hasDevices">显示全部</el-button>
      </div>
    </div>
    <div class="map-wrapper">
      <div id="device-map" ref="mapContainer"></div>
      <div v-if="selectedDevice" class="device-popup">
        <div class="popup-header">
          <span class="device-name">{{ selectedDevice.name || selectedDevice.device_tag }}</span>
          <el-button text size="small" @click="selectedDevice = null"><el-icon><Close /></el-icon></el-button>
        </div>
        <div class="popup-body">
          <el-descriptions :column="1" size="small">
            <el-descriptions-item label="设备ID">{{ selectedDevice.device_tag }}</el-descriptions-item>
            <el-descriptions-item label="状态">
              <span class="status-badge" :class="selectedDevice.status">{{ getStatusLabel(selectedDevice.status) }}</span>
            </el-descriptions-item>
            <el-descriptions-item label="协议">{{ selectedDevice.protocol }}</el-descriptions-item>
          </el-descriptions>
          <div class="popup-actions">
            <el-button type="primary" size="small" @click="playDevice">播放</el-button>
          </div>
        </div>
      </div>
    </div>

    <el-dialog v-model="showPlayer" title="设备播放" width="640px" destroy-on-close>
      <div class="player-wrapper">
        <VideoPlayer
          v-if="playUrl"
          :liveSrc="playUrl"
          type="wsflv"
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
import maplibregl from 'maplibre-gl'
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
const mapContainer = ref(null)
let map = null
let draw = null
let cluster = null
const featuresGeoJSON = ref({ type: 'FeatureCollection', features: [] })

const searchText = ref('')
const filterStatus = ref('')
const selectedDevice = ref(null)
const showPlayer = ref(false)
const playUrl = ref('')

const hasDevices = computed(() => featuresGeoJSON.value.features.length > 0)

const statusColors = {
  online: '#22c55e',
  offline: '#9ca3af',
  error: '#f97316',
  maintaining: '#3b82f6',
  intrusion: '#ef4444',
}

const statusLabels = {
  online: '在线',
  offline: '离线',
  error: '故障',
  maintaining: '维修',
  intrusion: '入侵报警',
}

function getStatusLabel(status) {
  return statusLabels[status] || status || '未知'
}

function getClusterStyle() {
  return {
    'circle-color': [
      'step',
      ['get', 'point_count'],
      '#3b82f6',
      10, '#f97316',
      50, '#ef4444',
    ],
    'circle-radius': [
      'step',
      ['get', 'point_count'],
      20,
      10, 25,
      50, 30,
    ],
    'circle-stroke-width': 2,
    'circle-stroke-color': '#ffffff',
    'circle-stroke-opacity': 0.8,
  }
}

async function fetchWfsFeatures(bounds) {
  const wfsUrl = settingsStore.mapWfsUrl
  if (!wfsUrl) {
    ElMessage.warning('请先在设置中配置 WFS 服务地址')
    return []
  }

  let bbox = null
  if (bounds) {
    bbox = `${bounds.getWest()},${bounds.getSouth()},${bounds.getEast()},${bounds.getNorth()}`
  }

  let targetUrl = wfsUrl
  if (bbox) {
    targetUrl += (wfsUrl.includes('?') ? '&' : '?') + `bbox=${bbox}`
  }

  const useProxy = import.meta.env.DEV
  const fetchUrl = useProxy ? `/api/v1/proxy/wfs?url=${encodeURIComponent(targetUrl)}` : targetUrl

  try {
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 10000)
    const res = await fetch(fetchUrl, { signal: controller.signal })
    clearTimeout(timeoutId)

    if (!res.ok) {
      console.warn('[DeviceMap] WFS response not ok:', res.status)
      return []
    }

    const text = await res.text()

    if (!text || text.trim() === '') {
      return []
    }

    const trimmed = text.trim()
    let features = []

    if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
      const geojson = JSON.parse(text)
      if (geojson.features) {
        for (const f of geojson.features) {
          const props = f.properties || {}
          let lat = parseFloat(props.latitude || props.lat || props.y)
          let lng = parseFloat(props.longitude || props.lng || props.lon || props.x)

          if (f.geometry && f.geometry.coordinates) {
            lng = f.geometry.coordinates[0]
            lat = f.geometry.coordinates[1]
          }

          if (lat && lng) {
            features.push({
              type: 'Feature',
              geometry: { type: 'Point', coordinates: [lng, lat] },
              properties: props,
            })
          }
        }
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
          const children = featureMember.children
          for (const child of children) {
            properties[child.tagName.split(':').pop()] = child.textContent
          }

          let lat = parseFloat(properties.latitude || properties.lat || properties.y)
          let lng = parseFloat(properties.longitude || properties.lng || properties.lon || properties.x)

          if (!lat || !lng) {
            const pointElement = featureMember.querySelector('Point') || featureMember.querySelector('[gml\\:Point]')
            if (pointElement) {
              const coords = pointElement.textContent.trim().split(/[\s,]+/)
              if (coords.length >= 2) {
                lng = parseFloat(coords[0])
                lat = parseFloat(coords[1])
              }
            }
          }

          if (lat && lng) {
            features.push({
              type: 'Feature',
              geometry: { type: 'Point', coordinates: [lng, lat] },
              properties: properties,
            })
          }
        }
      }
    }

    return features
  } catch (e) {
    if (e.name === 'AbortError') {
      console.warn('[DeviceMap] WFS request timeout')
    } else {
      console.error('[DeviceMap] Failed to fetch WFS:', e)
    }
    return []
  }
}

function filterFeatures(features) {
  return features.filter(f => {
    const props = f.properties
    const name = (props.name || props.device_tag || '').toLowerCase()
    const status = (props.status || '').toLowerCase()
    const search = searchText.value.toLowerCase()

    if (search && !name.includes(search)) return false
    if (filterStatus.value && status !== filterStatus.value) return false
    return true
  })
}

function initCluster(features) {
  cluster = new Supercluster({
    radius: 50,
    maxZoom: 18,
    map: (props) => ({
      status: props.status || 'offline',
    }),
    reduce: (accumulated, props) => {
    },
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

  featuresGeoJSON.value = {
    type: 'FeatureCollection',
    features: clusters,
  }

  const source = map.getSource('devices')
  if (source) {
    source.setData(featuresGeoJSON.value)
  }
}

async function loadDevices() {
  if (!map) return

  const bounds = map.getBounds()
  const features = await fetchWfsFeatures(bounds)
  const filtered = filterFeatures(features)

  if (filtered.length === 0 && features.length > 0) {
    ElMessage.warning('当前范围内没有带坐标的设备，请在设备管理中添加经纬度')
  } else if (features.length === 0) {
    ElMessage.warning('未获取到设备数据，请检查WFS服务配置')
  }

  initCluster(filtered)
  updateMapView()
}

async function playDevice() {
  if (!selectedDevice.value?.device_tag) return

  try {
    const res = await request.get(`/devices/${selectedDevice.value.device_tag}/play-links`)
    const data = res.data?.data || res.data
    if (data?.web_flv) {
      playUrl.value = data.web_flv
    } else if (data?.flv) {
      playUrl.value = data.flc
    } else {
      ElMessage.warning('无法获取播放地址')
      return
    }
    showPlayer.value = true
  } catch (e) {
    ElMessage.error('获取播放地址失败: ' + (e.message || e))
  }
}

function fitToDevices() {
  if (!map || !cluster) return

  const features = cluster.getClusters([-180, -85, 180, 85], 0)
  if (features.length === 0) return

  let minLng = Infinity, minLat = Infinity, maxLng = -Infinity, maxLat = -Infinity

  for (const f of features) {
    if (f.geometry.type === 'Point') {
      const [lng, lat] = f.geometry.coordinates
      minLng = Math.min(minLng, lng)
      minLat = Math.min(minLat, lat)
      maxLng = Math.max(maxLng, lng)
      maxLat = Math.max(maxLat, lat)
    } else if (f.properties.cluster) {
      const clusterId = f.properties.cluster_id
      const leaves = cluster.getLeaves(clusterId, Infinity)
      for (const leaf of leaves) {
        const [lng, lat] = leaf.geometry.coordinates
        minLng = Math.min(minLng, lng)
        minLat = Math.min(minLat, lat)
        maxLng = Math.max(maxLng, lng)
        maxLat = Math.max(maxLat, lat)
      }
    }
  }

  if (minLng !== Infinity) {
    map.fitBounds([[minLng, minLat], [maxLng, maxLat]], { padding: [50, 50] })
  }
}

let loadTimer = null
function debouncedLoad() {
  if (loadTimer) clearTimeout(loadTimer)
  loadTimer = setTimeout(() => loadDevices(), 300)
}

watch([searchText, filterStatus], () => {
  if (map && cluster) {
    updateMapView()
  }
})

onMounted(() => {
  if (!mapContainer.value) return

  map = new maplibregl.Map({
    container: 'device-map',
    center: [121.47, 31.23],
    zoom: 10,
    zoomControl: true,
  })

  map.on('load', () => {
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

    map.addSource('devices', {
      type: 'geojson',
      data: featuresGeoJSON.value,
    })

    map.addLayer({
      id: 'device-clusters',
      type: 'circle',
      source: 'devices',
      filter: ['has', 'point_count'],
      paint: getClusterStyle(),
    })

    map.addLayer({
      id: 'device-points',
      type: 'circle',
      source: 'devices',
      filter: ['!', ['has', 'point_count']],
      paint: {
        'circle-color': [
          'match',
          ['get', 'status'],
          'online', '#22c55e',
          'error', '#f97316',
          'maintaining', '#3b82f6',
          'intrusion', '#ef4444',
          '#9ca3af',
        ],
        'circle-radius': [
          'match',
          ['get', 'status'],
          'intrusion', 12,
          8,
        ],
        'circle-opacity': 0.8,
        'circle-stroke-width': 2,
        'circle-stroke-color': [
          'match',
          ['get', 'status'],
          'online', '#22c55e',
          'error', '#f97316',
          'maintaining', '#3b82f6',
          'intrusion', '#ef4444',
          '#9ca3af',
        ],
        'circle-stroke-opacity': 1,
      },
    })

    loadDevices()
  })

  map.on('moveend', () => {
    debouncedLoad()
  })

  map.on('zoomend', () => {
    debouncedLoad()
  })

  map.on('click', 'device-clusters', (e) => {
    const feature = e.features[0]
    const clusterId = feature.properties.cluster_id
    const zoom = cluster.getClusterExpansionZoom(clusterId)
    map.easeTo({
      center: feature.geometry.coordinates,
      zoom: zoom,
    })
  })

  map.on('click', 'device-points', (e) => {
    const feature = e.features[0]
    const props = feature.properties || {}
    selectedDevice.value = props
  })

  map.on('mouseenter', 'device-clusters', () => {
    map.getCanvas().style.cursor = 'pointer'
  })

  map.on('mouseleave', 'device-clusters', () => {
    map.getCanvas().style.cursor = ''
  })

  map.on('mouseenter', 'device-points', () => {
    map.getCanvas().style.cursor = 'pointer'
  })

  map.on('mouseleave', 'device-points', () => {
    map.getCanvas().style.cursor = ''
  })

  const tmsUrl = settingsStore.mapTmsUrl

  if (tmsUrl && tmsUrl.includes('{x}') && tmsUrl.includes('{y}') && tmsUrl.includes('{z}')) {
    const tileUrls = tmsUrl.split('\n').map(u => u.trim()).filter(u => u && u.length > 0)
    map.addSource('raster-tiles', {
      type: 'raster',
      tiles: tileUrls,
      tileSize: 256,
    })
    map.addLayer({
      id: 'raster-tiles-layer',
      type: 'raster',
      source: 'raster-tiles',
      minzoom: 0,
      maxzoom: 18,
    })
  } else if (tmsUrl) {
    console.warn('[DeviceMap] Invalid TMS URL (missing {x} {y} {z}), using OpenStreetMap instead')
    map.addSource('osm-tiles', {
      type: 'raster',
      tiles: ['https://a.tile.openstreetmap.org/{z}/{x}/{y}.png'],
      tileSize: 256,
      attribution: 'OpenStreetMap',
    })
    map.addLayer({
      id: 'osm-tiles-layer',
      type: 'raster',
      source: 'osm-tiles',
      minzoom: 0,
      maxzoom: 18,
    })
  } else {
    map.addSource('osm-tiles', {
      type: 'raster',
      tiles: ['https://a.tile.openstreetmap.org/{z}/{x}/{y}.png'],
      tileSize: 256,
      attribution: 'OpenStreetMap',
    })
    map.addLayer({
      id: 'osm-tiles-layer',
      type: 'raster',
      source: 'osm-tiles',
      minzoom: 0,
      maxzoom: 18,
    })
  }
})

onUnmounted(() => {
  if (map) {
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
  background: var(--bg-color);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.page-title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
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

.device-popup {
  position: absolute;
  top: 16px;
  right: 16px;
  width: 280px;
  background: var(--bg-color);
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  z-index: 1000;
  overflow: hidden;
}

.popup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: var(--el-color-primary);
  color: white;
}

.device-name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.popup-body {
  padding: 16px;
}

.popup-actions {
  margin-top: 12px;
  display: flex;
  justify-content: flex-end;
}

.status-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
}

.status-badge.online { background: #22c55e20; color: #22c55e; }
.status-badge.offline { background: #9ca3af20; color: #9ca3af; }
.status-badge.error { background: #f9731620; color: #f97316; }
.status-badge.maintaining { background: #3b82f620; color: #3b82f6; }
.status-badge.intrusion { background: #ef444420; color: #ef4444; animation: pulse 1s infinite; }

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.6; }
}

.player-wrapper {
  width: 100%;
  aspect-ratio: 16 / 9;
  background: #000;
}

.player-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #1a1a1a;
  color: #999;
}

:deep(.maplibregl-ctrl-group) {
  background: var(--bg-color);
  border-radius: 4px;
}

:deep(.maplibregl-ctrl-group button) {
  width: 32px;
  height: 32px;
}
</style>
