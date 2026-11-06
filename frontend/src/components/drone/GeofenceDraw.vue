<!-- @ts-check -->
<template>
  <div class="fence-draw">
    <div class="fence-draw__bar">
      <span class="fence-draw__hint">{{ hint }}</span>
      <el-button size="small" type="primary" :disabled="!geometry" @click="save">
        保存围栏
      </el-button>
      <el-button size="small" @click="clear">清除</el-button>
    </div>
    <div ref="mapEl" class="fence-draw__map" />
  </div>
</template>

<script setup>
// @ts-check
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Map as MapLibreMap, NavigationControl } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { TerraDraw, TerraDrawPolygonMode } from 'terra-draw'
import { TerraDrawMapLibreGLAdapter } from 'terra-draw-maplibre-gl-adapter'
import { useSettingsStore } from '../../stores/settingsStore'

const DEFAULT_TMS_URL = 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'
const DEFAULT_VIEW = { lat: 31.2304, lon: 121.4737, zoom: 10 }
const HINT_IDLE = '在地图上点击绘制多边形，双击闭合'

const props = defineProps({
  /** @type {{ lat: number, lon: number, zoom?: number }} */
  center: { type: Object, default: () => ({ lat: 31.2304, lon: 121.4737, zoom: 10 }) },
  /** @type {Array<any>} 只读回显的围栏（当前编辑项由 initialGeometry 载入） */
  fences: { type: Array, default: () => [] },
  /** @type {any} 载入到 TerraDraw 中可编辑的 GeoJSON */
  initialGeometry: { type: Object, default: null },
})

const emit = defineEmits(['save'])

const settingsStore = useSettingsStore()
const mapEl = ref(null)
const hint = ref(HINT_IDLE)
/** @type {import('vue').Ref<any>} */
const geometry = ref(null)
/** @type {any} */
let map = null
/** @type {any} */
let draw = null

function newId() {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `fence-${Date.now()}-${Math.round(Math.random() * 1e6)}`
}

/**
 * @param {unknown} value
 * @returns {any}
 */
function parseGeometry(value) {
  if (!value) return null
  try {
    const geo = typeof value === 'string' ? JSON.parse(value) : value
    if (!geo) return null
    if (geo.type === 'Feature') return geo.geometry || null
    if (geo.type === 'Polygon' || geo.type === 'MultiPolygon') return geo
    return null
  } catch (e) {
    return null
  }
}

/**
 * 圆形围栏后端只存圆心/半径，2D 回显时离散成多边形。
 * @param {number} lon
 * @param {number} lat
 * @param {number} radiusM
 * @param {number} steps
 */
function circleRing(lon, lat, radiusM, steps = 64) {
  const R = 6378137
  const d = radiusM / R
  const lat1 = (lat * Math.PI) / 180
  const lon1 = (lon * Math.PI) / 180
  const ring = []
  for (let i = 0; i <= steps; i++) {
    const brg = (i / steps) * 2 * Math.PI
    const lat2 = Math.asin(
      Math.sin(lat1) * Math.cos(d) + Math.cos(lat1) * Math.sin(d) * Math.cos(brg)
    )
    const lon2 =
      lon1 +
      Math.atan2(
        Math.sin(brg) * Math.sin(d) * Math.cos(lat1),
        Math.cos(d) - Math.sin(lat1) * Math.sin(lat2)
      )
    ring.push([(lon2 * 180) / Math.PI, (lat2 * 180) / Math.PI])
  }
  return ring
}

function fenceCollection() {
  const features = []
  for (const f of props.fences || []) {
    let geo = parseGeometry(f.geojson)
    const cLon = Number(f.center_lon)
    const cLat = Number(f.center_lat)
    const radius = Number(f.radius_m)
    if (
      !geo &&
      f.shape === 'circle' &&
      Number.isFinite(cLon) &&
      Number.isFinite(cLat) &&
      radius > 0
    ) {
      geo = { type: 'Polygon', coordinates: [circleRing(cLon, cLat, radius)] }
    }
    if (!geo) continue
    features.push({
      type: 'Feature',
      id: f.id,
      properties: { name: f.name, type: f.type },
      geometry: geo,
    })
  }
  return { type: 'FeatureCollection', features }
}

function addBasemap() {
  if (!map || map.getSource('basemap')) return
  const configured = settingsStore.browserTmsUrl || ''
  const configuredTiles = configured
    .split('\n')
    .map((u) => u.trim())
    .filter((u) => u.includes('{x}') && u.includes('{z}'))
  const tiles = configuredTiles.length > 0 ? configuredTiles : [DEFAULT_TMS_URL]
  const hasNegY = tiles.some((u) => u.includes('{-y}'))
  const normalized = tiles.map((u) => (hasNegY ? u.replace(/\{-y\}/g, '{y}') : u))

  map.addSource('basemap', {
    type: 'raster',
    tiles: normalized,
    tileSize: 256,
    maxzoom: 19,
    scheme: hasNegY ? 'tms' : 'xyz',
  })
  map.addLayer({ id: 'basemap-layer', type: 'raster', source: 'basemap' })
}

function addFenceLayers() {
  if (!map || map.getSource('drone-fences')) return
  map.addSource('drone-fences', { type: 'geojson', data: fenceCollection() })
  map.addLayer({
    id: 'drone-fences-fill',
    type: 'fill',
    source: 'drone-fences',
    paint: {
      'fill-color': ['case', ['==', ['get', 'type'], 'allowed'], '#22c55e', '#ef4444'],
      'fill-opacity': 0.18,
    },
  })
  map.addLayer({
    id: 'drone-fences-line',
    type: 'line',
    source: 'drone-fences',
    paint: {
      'line-color': ['case', ['==', ['get', 'type'], 'allowed'], '#22c55e', '#ef4444'],
      'line-width': 2,
    },
  })
}

function syncFences() {
  const source = map && map.getSource('drone-fences')
  if (source) source.setData(fenceCollection())
}

function syncGeometry() {
  if (!draw) return
  const polygons = draw
    .getSnapshot()
    .filter((feature) => feature.geometry && feature.geometry.type === 'Polygon')
  geometry.value = polygons.length > 0 ? polygons[polygons.length - 1].geometry : null
  hint.value = geometry.value ? '已绘制，点击「保存围栏」提交' : HINT_IDLE
}

function syncInitial() {
  if (!draw) return
  draw.clear()
  const geo = parseGeometry(props.initialGeometry)
  if (geo && geo.type === 'Polygon') {
    draw.addFeatures([
      {
        type: 'Feature',
        id: newId(),
        properties: { mode: 'polygon' },
        geometry: geo,
      },
    ])
  }
  syncGeometry()
}

function save() {
  syncGeometry()
  if (geometry.value) emit('save', geometry.value)
}

function clear() {
  if (draw) draw.clear()
  geometry.value = null
  hint.value = HINT_IDLE
}

watch(() => props.fences, syncFences, { deep: true })
watch(() => props.initialGeometry, syncInitial, { deep: true })
watch(
  () => props.center,
  (center) => {
    if (!map || !center) return
    map.flyTo({
      center: [center.lon, center.lat],
      zoom: center.zoom ?? map.getZoom(),
      duration: 600,
    })
  },
  { deep: true }
)

onMounted(async () => {
  if (!mapEl.value) return

  // 复用系统地图配置；没有配置时回退到 ArcGIS 影像。
  await settingsStore.loadMapSettings()

  map = new MapLibreMap({
    container: mapEl.value,
    center: [props.center.lon, props.center.lat],
    zoom: props.center.zoom ?? DEFAULT_VIEW.zoom,
    style: {
      version: 8,
      sources: {},
      layers: [
        {
          id: 'background',
          type: 'background',
          paint: { 'background-color': '#0f172a' },
        },
      ],
    },
  })
  map.addControl(new NavigationControl({ showCompass: false }), 'top-right')

  map.on('load', () => {
    addBasemap()
    addFenceLayers()

    // 规则 §10.5：TerraDraw + TerraDrawPolygonMode + MapLibre 适配器。
    draw = new TerraDraw({
      adapter: new TerraDrawMapLibreGLAdapter({ map }),
      modes: [new TerraDrawPolygonMode()],
    })
    draw.start()
    draw.on('finish', (id) => {
      // 只保留刚完成的多边形，避免多次绘制后取到旧图形。
      for (const feature of draw.getSnapshot()) {
        if (feature.id !== id) draw.removeFeatures([feature.id])
      }
      syncGeometry()
    })
    draw.on('change', syncGeometry)
    syncInitial()
  })
})

onBeforeUnmount(() => {
  if (draw) draw.stop()
  if (map) map.remove()
  draw = null
  map = null
})
</script>

<style scoped>
.fence-draw {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.fence-draw__bar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-surface);
}
.fence-draw__hint {
  flex: 1;
  color: var(--text-muted);
  font-size: 12px;
}
.fence-draw__map {
  flex: 1;
  min-height: 320px;
}
</style>
