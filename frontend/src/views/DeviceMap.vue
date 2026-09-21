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
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import 'leaflet.markercluster'
import 'leaflet.markercluster/dist/MarkerCluster.css'
import 'leaflet.markercluster/dist/MarkerCluster.Default.css'
import { ElMessage } from 'element-plus'
import { Close } from '@element-plus/icons-vue'
import { useSettingsStore } from '../stores/settingsStore'
import { request } from '../utils/request'
import VideoPlayer from '../components/VideoPlayer.vue'

const settingsStore = useSettingsStore()
const mapContainer = ref(null)
let map = null
let markersLayer = null
const markersMap = ref(new Map())

const searchText = ref('')
const filterStatus = ref('')
const selectedDevice = ref(null)
const showPlayer = ref(false)
const playUrl = ref('')

const hasDevices = computed(() => markersMap.value.size > 0)

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

function getFeatureStyle(feature) {
  const status = feature.properties.status || 'offline'
  const color = statusColors[status] || statusColors.offline
  return {
    color: color,
    fillColor: color,
    fillOpacity: 0.6,
    weight: 2,
    radius: 8,
  }
}

async function fetchWfsFeatures(bounds) {
  const wfsUrl = settingsStore.mapWfsUrl
  if (!wfsUrl) {
    ElMessage.warning('请先在设置中配置 WFS 服务地址')
    return []
  }

  const bbox = bounds ? `${bounds.getWest()},${bounds.getSouth()},${bounds.getEast()},${bounds.getNorth()}` : null

  let url = wfsUrl
  if (bbox) {
    url += (wfsUrl.includes('?') ? '&' : '?') + `bbox=${bbox}`
  }

  try {
    const res = await fetch(url)
    const text = await res.text()
    const parser = new DOMParser()
    const xml = parser.parseFromString(text, 'text/xml')

    const features = []
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

    return features
  } catch (e) {
    console.error('[DeviceMap] Failed to fetch WFS:', e)
    ElMessage.error('获取设备数据失败: ' + e.message)
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

async function loadDevices(bounds) {
  if (!map) return

  const features = await fetchWfsFeatures(bounds)
  const filtered = filterFeatures(features)

  markersMap.value.clear()
  if (markersLayer) {
    map.removeLayer(markersLayer)
  }

  markersLayer = L.markerClusterGroup({
    maxClusterRadius: 50,
    spiderfyOnMaxZoom: true,
    showCoverageOnHover: false,
    zoomToBoundsOnClick: true,
  })

  for (const feature of filtered) {
    const [lng, lat] = feature.geometry.coordinates
    const props = feature.properties
    const status = props.status || 'offline'

    const marker = L.circleMarker([lat, lng], {
      ...getFeatureStyle({ properties: { status } }),
      radius: status === 'intrusion' ? 12 : 8,
    })

    marker.bindTooltip(props.name || props.device_tag || '未知设备', {
      direction: 'top',
      offset: [0, -10],
    })

    marker.on('click', () => {
      selectedDevice.value = props
    })

    markersLayer.addLayer(marker)
    markersMap.value.set(props.device_tag, { marker, feature })
  }

  markersLayer.addTo(map)
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
  if (!map || markersMap.value.size === 0) return

  const group = L.featureGroup(Array.from(markersMap.value.values()).map(v => v.marker))
  map.fitBounds(group.getBounds(), { padding: [50, 50] })
}

let loadTimer = null
function debouncedLoad(bounds) {
  if (loadTimer) clearTimeout(loadTimer)
  loadTimer = setTimeout(() => loadDevices(bounds), 300)
}

watch([searchText, filterStatus], () => {
  if (map) {
    loadDevices(map.getBounds())
  }
})

onMounted(() => {
  if (!mapContainer.value) return

  map = L.map('device-map', {
    center: [35, 105],
    zoom: 5,
    zoomControl: true,
  })

  const tmsUrl = settingsStore.mapTmsUrl
  if (tmsUrl) {
    L.tileLayer(tmsUrl, {
      maxZoom: 18,
      attribution: 'GeoServer',
    }).addTo(map)
  } else {
    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      maxZoom: 18,
      attribution: 'OpenStreetMap',
    }).addTo(map)
  }

  loadDevices()

  map.on('moveend', () => {
    debouncedLoad(map.getBounds())
  })

  map.on('zoomend', () => {
    debouncedLoad(map.getBounds())
  })
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

:deep(.custom-marker) {
  background: transparent;
  border: none;
}

:deep(.alarm-marker) {
  animation: alarm-pulse 1s infinite;
}

@keyframes alarm-pulse {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(1.2); }
}
</style>
