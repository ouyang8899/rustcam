<!-- @ts-check -->
<template>
  <div class="fence-draw">
    <div ref="mapEl" class="fence-draw__map" />
  </div>
</template>

<script setup>
// @ts-check
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Map as MapLibreMap, NavigationControl, setWorkerUrl } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { TerraDraw, TerraDrawCircleMode, TerraDrawPolygonMode } from 'terra-draw'
import { TerraDrawMapLibreGLAdapter } from 'terra-draw-maplibre-gl-adapter'
import { useSettingsStore } from '../../stores/settingsStore'
import { FENCE_COLORS, fenceColors, fenceRule } from '../../utils/fenceStyle'

// 同 Map2D.vue：经 Vite 预打包后 import.meta.url 推出的 Worker 地址失效，
// GeoJSON 图层（围栏、TerraDraw 图形）会因 Worker 加载失败而完全不渲染。
setWorkerUrl(`${import.meta.env.BASE_URL}maplibre/maplibre-gl-worker.mjs`)

const DEFAULT_TMS_URL = 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'
const DEFAULT_VIEW = { lat: 31.2304, lon: 121.4737, zoom: 10 }

const props = defineProps({
  /** @type {{ lat: number, lon: number, zoom?: number }} */
  center: { type: Object, default: () => ({ lat: 31.2304, lon: 121.4737, zoom: 10 }) },
  /** @type {Array<any>} 只读回显的围栏（当前编辑项由 initialGeometry 载入） */
  fences: { type: Array, default: () => [] },
  /** @type {any} 载入到 TerraDraw 中可编辑的 GeoJSON */
  initialGeometry: { type: Object, default: null },
  /** @type {'polygon' | 'circle'} */
  shape: { type: String, default: 'polygon' },
  /** @type {'no_fly' | 'allowed'} 当前编辑围栏的规则，决定绘制中的图形颜色 */
  rule: { type: String, default: 'no_fly' },
  /** @type {{ lon: number, lat: number, radius_m: number } | null} 圆形围栏回显圆心/半径 */
  initialCircle: { type: Object, default: null },
})

const emit = defineEmits(['change'])

const settingsStore = useSettingsStore()
const mapEl = ref(null)
/** @type {import('vue').Ref<any>} */
const geometry = ref(null)
/** 最近一次向上同步的几何，避免父组件回写触发重复清空/重绘。 */
let lastEmitted = ''
/** syncInitial 内部清空/载入时抑制中间态上报。 */
let syncingInitial = false
/** 拖拽顶点会高频触发 change，合并后再同步给父组件。 */
let emitTimer = null
/** @type {any} */
let map = null
/** @type {any} */
let draw = null
/** 当前规则，TerraDraw 样式函数在渲染时读取 */
let currentRule = fenceRule(props.rule)

/** 正在绘制/编辑的围栏按规则着色：禁飞区红、允许区绿 */
function drawStyles() {
  return {
    fillColor: () => fenceColors(currentRule).fill,
    outlineColor: () => fenceColors(currentRule).outline,
    fillOpacity: 0.3,
    outlineOpacity: 1,
    outlineWidth: 3,
  }
}

/** 统一坐标精度，保证与父组件回写的几何可以直接比较。 */
function canonicalGeometry(geo) {
  if (!geo || geo.type !== 'Polygon') return null
  return {
    type: 'Polygon',
    coordinates: geo.coordinates.map((ring) =>
      ring.map(([lon, lat]) => [Number(Number(lon).toFixed(6)), Number(Number(lat).toFixed(6))])
    ),
  }
}

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
      properties: { name: f.name, type: fenceRule(f.type) },
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
      'fill-color': [
        'case',
        ['==', ['get', 'type'], 'allowed'],
        FENCE_COLORS.allowed.fill,
        FENCE_COLORS.no_fly.fill,
      ],
      'fill-opacity': 0.18,
    },
  })
  map.addLayer({
    id: 'drone-fences-line',
    type: 'line',
    source: 'drone-fences',
    paint: {
      'line-color': [
        'case',
        ['==', ['get', 'type'], 'allowed'],
        FENCE_COLORS.allowed.outline,
        FENCE_COLORS.no_fly.outline,
      ],
      'line-width': 2,
    },
  })}

function syncFences() {
  const source = map && map.getSource('drone-fences')
  if (source) source.setData(fenceCollection())
}

function emitGeometry() {
  const json = geometry.value ? JSON.stringify(geometry.value) : ''
  if (json === lastEmitted) return
  lastEmitted = json
  emit('change', geometry.value)
}

function flushEmitTimer() {
  if (emitTimer) {
    clearTimeout(emitTimer)
    emitTimer = null
  }
}

/**
 * @param {boolean} [immediate] 完成/载入等关键节点立即上报，其余变更合并上报。
 * @param {boolean} [report] false 表示这是父组件下发的同步结果，只记录基线、不回传，
 *   否则“父写子→子回传→父重算→props 变化→父写子”会形成死循环。
 */
function syncGeometry(immediate = false, report = true) {
  if (!draw) return
  const polygons = draw
    .getSnapshot()
    .filter(
      (feature) =>
        feature.geometry &&
        feature.geometry.type === 'Polygon' &&
        !feature.properties?.currentlyDrawing
    )
  const geo = polygons.length > 0 ? polygons[polygons.length - 1].geometry : null
  geometry.value = canonicalGeometry(geo)
  if (syncingInitial) return
  if (!report) {
    flushEmitTimer()
    lastEmitted = geometry.value ? JSON.stringify(geometry.value) : ''
    return
  }
  if (immediate) {
    flushEmitTimer()
    emitGeometry()
    return
  }
  flushEmitTimer()
  emitTimer = setTimeout(() => {
    emitTimer = null
    emitGeometry()
  }, 120)
}

/**
 * 清空画布（含尚未完成的草稿），供父组件“清空/新建/切换/载入”复用。
 * @param {boolean} [silent] 父组件随后会写入新几何时用 silent，避免中间态把表单清空。
 */
function clear(silent = false) {
  if (!draw) return
  flushEmitTimer()
  const hadGeometry = lastEmitted !== ''
  syncingInitial = true
  try {
    draw.clear()
  } finally {
    syncingInitial = false
  }
  geometry.value = null
  lastEmitted = ''
  if (!silent && hadGeometry) emit('change', null)
}

defineExpose({ clear })

function applyShape() {
  if (!draw) return
  const mode = props.shape === 'circle' ? 'circle' : 'polygon'
  if (draw.getMode() !== mode) draw.setMode(mode)
}

/** 右键结束绘制：转发 TerraDraw 自身的 Enter 完成逻辑，与键盘结束保持一致。 */
function finishByContextMenu(e) {
  if (e && typeof e.preventDefault === 'function') e.preventDefault()
  if (!map) return
  map.getCanvas().dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter' }))
}

function syncInitial() {
  if (!draw) return
  let geo = parseGeometry(props.initialGeometry)
  const c = props.initialCircle
  const cLon = Number(c?.lon)
  const cLat = Number(c?.lat)
  const cRadius = Number(c?.radius_m)
  if (
    !geo &&
    props.shape === 'circle' &&
    Number.isFinite(cLon) &&
    Number.isFinite(cLat) &&
    cRadius > 0
  ) {
    geo = { type: 'Polygon', coordinates: [circleRing(cLon, cLat, cRadius)] }
  }
  geo = canonicalGeometry(geo)
  const json = geo ? JSON.stringify(geo) : ''
  // 父组件回写的就是刚同步上去的几何：保持现状，避免清空重绘造成闪烁。
  if (json === lastEmitted) return
  syncingInitial = true
  try {
    draw.clear()
    if (geo) {
      draw.addFeatures([
        {
          type: 'Feature',
          id: newId(),
          properties: { mode: props.shape === 'circle' ? 'circle' : 'polygon' },
          geometry: geo,
        },
      ])
    }
  } catch (e) {
    console.error('[fence] 同步二维围栏失败', e)
  } finally {
    syncingInitial = false
  }
  syncGeometry(true, false)
}

watch(() => props.fences, syncFences, { deep: true })
watch(
  () => props.rule,
  (rule) => {
    currentRule = fenceRule(rule)
    // 重新下发样式，触发 TerraDraw 以新规则重绘当前图形
    if (!draw) return
    try {
      draw.updateModeOptions('polygon', { styles: drawStyles() })
      draw.updateModeOptions('circle', { styles: drawStyles() })
    } catch (e) {
      console.error('[fence] 更新围栏配色失败', e)
    }
  }
)
watch(() => props.initialGeometry, syncInitial, { deep: true })
watch(() => props.initialCircle, syncInitial, { deep: true })
watch(() => props.shape, () => {
  applyShape()
  syncInitial()
})
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

    // 规则 §10.5：TerraDraw + MapLibre 适配器；多边形与圆形都支持二维绘制。
    draw = new TerraDraw({
      adapter: new TerraDrawMapLibreGLAdapter({ map }),
      modes: [
        new TerraDrawPolygonMode({ styles: drawStyles() }),
        // 圆形右键交由 contextmenu 统一结束，避免 pointerup 又开一个圆心
        new TerraDrawCircleMode({
          pointerEvents: {
            rightClick: false,
            contextMenu: false,
            leftClick: true,
            onDragStart: true,
            onDrag: true,
            onDragEnd: true,
          },
          styles: drawStyles(),
        }),
      ],
    })
    draw.start()
    applyShape()
    map.on('contextmenu', finishByContextMenu)
    draw.on('finish', (id) => {
      // 只保留刚完成的图形，避免多次绘制后取到旧图形。
      for (const feature of draw.getSnapshot()) {
        if (feature.id !== id) draw.removeFeatures([feature.id])
      }
      syncGeometry(true)
    })
    draw.on('change', () => syncGeometry())
    syncInitial()
  })
})

onBeforeUnmount(() => {
  flushEmitTimer()
  if (map) map.off('contextmenu', finishByContextMenu)
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
.fence-draw__map {
  flex: 1;
  min-height: 320px;
}
.fence-draw__map :deep(.maplibregl-canvas) {
  cursor: crosshair !important;
}
</style>
